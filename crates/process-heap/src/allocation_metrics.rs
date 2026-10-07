//! Optional development measurements of requested Rust allocation sizes.
//! Counters are process-wide across wrapped allocators. They exclude allocator
//! headers, internal realloc temporaries, native allocations, and RSS. Use an
//! isolated serial workload: reset and snapshots are not multi-counter transactions.
use std::{
    alloc::{GlobalAlloc, Layout},
    sync::atomic::{AtomicUsize, Ordering},
};

static ALLOCATED_BYTES: AtomicUsize = AtomicUsize::new(0);
static ALLOCATION_COUNT: AtomicUsize = AtomicUsize::new(0);
static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);
static PEAK_BYTES: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snapshot {
    /// Sum of requested sizes of successful alloc, alloc_zeroed, and realloc calls.
    pub allocated_bytes: usize,
    pub allocation_count: usize,
    /// Outstanding requested allocation sizes, including allocations before reset.
    pub live_bytes: usize,
    /// Maximum outstanding requested bytes since reset, including its baseline.
    pub peak_bytes: usize,
}

/// Begin a measurement and return its live-byte baseline. Existing allocations
/// remain accounted for when subsequently resized or freed. Reset while the
/// measured workload is quiescent, and snapshot before printing the result.
pub fn reset() -> usize {
    ALLOCATED_BYTES.store(0, Ordering::Relaxed);
    ALLOCATION_COUNT.store(0, Ordering::Relaxed);
    let baseline = LIVE_BYTES.load(Ordering::Relaxed);
    PEAK_BYTES.store(baseline, Ordering::Relaxed);
    baseline
}

pub fn snapshot() -> Snapshot {
    Snapshot {
        allocated_bytes: ALLOCATED_BYTES.load(Ordering::Relaxed),
        allocation_count: ALLOCATION_COUNT.load(Ordering::Relaxed),
        live_bytes: LIVE_BYTES.load(Ordering::Relaxed),
        peak_bytes: PEAK_BYTES.load(Ordering::Relaxed),
    }
}

/// Delegate allocation unchanged while measuring its requested sizes.
/// Constructing a wrapper does not reset the process-wide counters.
pub struct MeasuredAllocator<A: GlobalAlloc> {
    inner: A,
}
impl<A: GlobalAlloc> MeasuredAllocator<A> {
    pub const fn new(inner: A) -> Self {
        Self { inner }
    }
}

fn record_request(size: usize, previous_size: usize) {
    ALLOCATED_BYTES.fetch_add(size, Ordering::Relaxed);
    ALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
    let live = if size >= previous_size {
        let growth = size - previous_size;
        LIVE_BYTES
            .fetch_add(growth, Ordering::Relaxed)
            .wrapping_add(growth)
    } else {
        let shrinkage = previous_size - size;
        LIVE_BYTES
            .fetch_sub(shrinkage, Ordering::Relaxed)
            .wrapping_sub(shrinkage)
    };
    PEAK_BYTES.fetch_max(live, Ordering::Relaxed);
}

// SAFETY: every operation passes the original pointer, layout, and requested size
// directly to the inner allocator. Only successful results affect requested-byte
// counters; a failed realloc retains the original allocation and its live charge.
unsafe impl<A: GlobalAlloc> GlobalAlloc for MeasuredAllocator<A> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplies the valid allocation layout unchanged.
        let data = unsafe { self.inner.alloc(layout) };
        if !data.is_null() {
            record_request(layout.size(), 0);
        }
        data
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: preserve the inner allocator's zeroing and layout contract.
        let data = unsafe { self.inner.alloc_zeroed(layout) };
        if !data.is_null() {
            record_request(layout.size(), 0);
        }
        data
    }

    unsafe fn dealloc(&self, data: *mut u8, layout: Layout) {
        // SAFETY: the caller supplies the inner allocation's original pointer/layout.
        unsafe { self.inner.dealloc(data, layout) };
        LIVE_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, data: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: pointer, old layout, and valid new size are delegated unchanged.
        let resized = unsafe { self.inner.realloc(data, layout, new_size) };
        if !resized.is_null() {
            record_request(new_size, layout.size());
        }
        resized
    }
}

#[cfg(test)]
mod tests;
