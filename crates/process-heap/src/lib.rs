//! Budget every live Rust allocation made after the request has been parsed.
//! Each allocation carries its charge, so releasing pre-budget request buffers
//! cannot subtract from the operation's usage. The helper exits on exhaustion;
//! unwinding or recovering inside an allocator would be unsound.
#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(feature = "allocation-metrics")]
pub mod allocation_metrics;

pub mod policy;

/// Dedicated worker exit status for an exhausted allocation budget.
pub const RESOURCE_EXIT: i32 = 75;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    io::{self, Write},
    sync::atomic::{AtomicUsize, Ordering},
};

pub struct BoundedHeap;
static BUDGET: AtomicUsize = AtomicUsize::new(0);
static USED: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

pub fn begin(bytes: usize) {
    PEAK.store(USED.load(Ordering::SeqCst), Ordering::SeqCst);
    BUDGET.store(bytes, Ordering::SeqCst);
}
/// High-water mark of charged live Rust allocations in the dedicated worker.
/// This excludes pre-budget buffers and allocations outside Rust's allocator.
pub fn peak() -> usize {
    PEAK.load(Ordering::SeqCst)
}
fn layout(original: Layout) -> Option<(Layout, usize)> {
    Layout::new::<usize>()
        .extend(original)
        .ok()
        .map(|(layout, offset)| (layout.pad_to_align(), offset))
}
fn exhausted(budget: usize, used: usize, requested: usize) -> ! {
    // Formatting integers and writing an initialized stderr do not allocate.
    // Keep this marker separate from chemistry diagnostics and machine checked.
    let _ = writeln!(
        io::stderr(),
        "RESHIKI_HEAP_LIMIT {budget} {used} {requested}"
    );
    std::process::exit(RESOURCE_EXIT);
}
// SAFETY: allocations are delegated to System with an extended, aligned layout.
// The header records exactly the bytes charged for that allocation. Deallocation
// recovers the same base and layout from the original layout supplied by Rust.
unsafe impl GlobalAlloc for BoundedHeap {
    unsafe fn alloc(&self, original: Layout) -> *mut u8 {
        let Some((layout, offset)) = layout(original) else {
            return std::ptr::null_mut();
        };
        let budget = BUDGET.load(Ordering::SeqCst);
        let charge = if budget == 0 { 0 } else { layout.size() };
        if charge != 0 {
            match USED.try_update(Ordering::SeqCst, Ordering::SeqCst, |used| {
                used.checked_add(charge).filter(|&value| value <= budget)
            }) {
                Ok(used) => {
                    PEAK.fetch_max(used + charge, Ordering::SeqCst);
                }
                Err(used) => exhausted(budget, used, charge),
            }
        }
        // SAFETY: System receives the valid layout computed above.
        let base = unsafe { System.alloc(layout) };
        if base.is_null() {
            if charge != 0 {
                USED.fetch_sub(charge, Ordering::SeqCst);
            }
            return base;
        }
        // SAFETY: base has room and alignment for the header and original data.
        unsafe {
            base.cast::<usize>().write(charge);
            base.add(offset)
        }
    }
    unsafe fn dealloc(&self, data: *mut u8, original: Layout) {
        let Some((layout, offset)) = layout(original) else {
            return;
        };
        // SAFETY: data was returned by alloc with this same original layout.
        unsafe {
            let base = data.sub(offset);
            let charge = base.cast::<usize>().read();
            System.dealloc(base, layout);
            if charge != 0 {
                USED.fetch_sub(charge, Ordering::SeqCst);
            }
        }
    }

    unsafe fn realloc(&self, data: *mut u8, original: Layout, new_size: usize) -> *mut u8 {
        // The editor never enables a budget. Preserve System's in-place growth
        // there, rather than forcing every Vec growth to allocate/copy/free.
        if BUDGET.load(Ordering::SeqCst) == 0 {
            let Some((old_layout, offset)) = layout(original) else {
                return std::ptr::null_mut();
            };
            let Ok(new) = Layout::from_size_align(new_size, original.align()) else {
                return std::ptr::null_mut();
            };
            let Some((new_layout, _)) = layout(new) else {
                return std::ptr::null_mut();
            };
            // SAFETY: data came from our extended layout. Alignment and header
            // offset do not change, and all pre-budget headers have zero charge.
            let base = unsafe { System.realloc(data.sub(offset), old_layout, new_layout.size()) };
            return if base.is_null() {
                base
            } else {
                // SAFETY: the resized allocation still contains the same header.
                unsafe { base.add(offset) }
            };
        }
        let Ok(new) = Layout::from_size_align(new_size, original.align()) else {
            return std::ptr::null_mut();
        };
        // SAFETY: use the default allocate/copy/free strategy in the worker so
        // the temporary allocation peak remains part of the exact heap budget.
        unsafe {
            let resized = self.alloc(new);
            if !resized.is_null() {
                std::ptr::copy_nonoverlapping(data, resized, original.size().min(new_size));
                self.dealloc(data, original);
            }
            resized
        }
    }
}

#[cfg(test)]
mod tests;
