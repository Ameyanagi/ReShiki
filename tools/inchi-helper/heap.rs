//! Budget every live Rust allocation made after the request has been parsed.
//! Each allocation carries its charge, so releasing pre-budget request buffers
//! cannot subtract from the operation's usage. The helper exits on exhaustion;
//! unwinding or recovering inside an allocator would be unsound.
use reshiki::chemistry::inchi::wire::RESOURCE_EXIT;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    io::{self, Write},
    sync::atomic::{AtomicUsize, Ordering},
};

pub struct BoundedHeap;
static BUDGET: AtomicUsize = AtomicUsize::new(0);
static USED: AtomicUsize = AtomicUsize::new(0);

pub fn begin(bytes: usize) {
    BUDGET.store(bytes, Ordering::SeqCst);
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
        if charge != 0
            && let Err(used) = USED.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |used| {
                used.checked_add(charge).filter(|&value| value <= budget)
            })
        {
            exhausted(budget, used, charge);
        }
        // SAFETY: System receives the valid layout computed above.
        let base = unsafe { System.alloc(layout) };
        if base.is_null() {
            USED.fetch_sub(charge, Ordering::SeqCst);
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
            USED.fetch_sub(charge, Ordering::SeqCst);
        }
    }
}
