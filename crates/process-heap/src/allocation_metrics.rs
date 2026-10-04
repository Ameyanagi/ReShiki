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
mod tests {
    use super::*;
    use std::alloc::System;

    struct RefuseResize {
        allocations_fail: bool,
    }
    // SAFETY: successful allocations/deallocations delegate unchanged to System;
    // failed requests return null, and failed realloc never frees the old pointer.
    unsafe impl GlobalAlloc for RefuseResize {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            if self.allocations_fail {
                std::ptr::null_mut()
            } else {
                // SAFETY: the caller supplied a valid layout.
                unsafe { System.alloc(layout) }
            }
        }
        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            if self.allocations_fail {
                std::ptr::null_mut()
            } else {
                // SAFETY: the caller supplied a valid layout.
                unsafe { System.alloc_zeroed(layout) }
            }
        }
        unsafe fn dealloc(&self, data: *mut u8, layout: Layout) {
            // SAFETY: successful allocations came from System with this layout.
            unsafe { System.dealloc(data, layout) };
        }
        unsafe fn realloc(&self, _: *mut u8, _: Layout, _: usize) -> *mut u8 {
            std::ptr::null_mut()
        }
    }

    #[test]
    fn requested_bytes_track_zeroed_resize_reset_and_failures() {
        assert_eq!(reset(), 0);
        let allocator = MeasuredAllocator::new(System);
        let original = Layout::from_size_align(16, 64).unwrap();
        let zeroed_layout = Layout::from_size_align(8, 32).unwrap();
        // SAFETY: this test owns every non-null pointer, passes each allocation's
        // exact layout, and retains the old pointer whenever realloc returns null.
        unsafe {
            let mut data = allocator.alloc(original);
            assert!(!data.is_null());
            assert_eq!(data as usize % original.align(), 0);
            data.write_bytes(0x5a, original.size());
            let zeroed = allocator.alloc_zeroed(zeroed_layout);
            assert!(!zeroed.is_null());
            assert_eq!(std::slice::from_raw_parts(zeroed, 8), &[0; 8]);
            assert_eq!(
                snapshot(),
                Snapshot {
                    allocated_bytes: 24,
                    allocation_count: 2,
                    live_bytes: 24,
                    peak_bytes: 24,
                }
            );

            let grown = allocator.realloc(data, original, 64);
            assert!(!grown.is_null());
            data = grown;
            assert_eq!(data as usize % original.align(), 0);
            assert_eq!(std::slice::from_raw_parts(data, 16), &[0x5a; 16]);
            assert_eq!(
                snapshot(),
                Snapshot {
                    allocated_bytes: 88,
                    allocation_count: 3,
                    live_bytes: 72,
                    peak_bytes: 72,
                }
            );
            let grown_layout = Layout::from_size_align(64, 64).unwrap();
            let shrunk = allocator.realloc(data, grown_layout, 4);
            assert!(!shrunk.is_null());
            data = shrunk;
            assert_eq!(std::slice::from_raw_parts(data, 4), &[0x5a; 4]);
            assert_eq!(
                snapshot(),
                Snapshot {
                    allocated_bytes: 92,
                    allocation_count: 4,
                    live_bytes: 12,
                    peak_bytes: 72,
                }
            );
            assert_eq!(reset(), 12);
            assert_eq!(
                snapshot(),
                Snapshot {
                    allocated_bytes: 0,
                    allocation_count: 0,
                    live_bytes: 12,
                    peak_bytes: 12,
                }
            );
            allocator.dealloc(zeroed, zeroed_layout);
            assert_eq!(snapshot().live_bytes, 4);
            allocator.dealloc(data, Layout::from_size_align(4, 64).unwrap());
            assert_eq!(snapshot().live_bytes, 0);
            assert_eq!(snapshot().peak_bytes, 12);

            let refused = MeasuredAllocator::new(RefuseResize {
                allocations_fail: true,
            });
            let before = snapshot();
            assert!(refused.alloc(original).is_null());
            assert!(refused.alloc_zeroed(original).is_null());
            assert_eq!(snapshot(), before);

            let refused_resize = MeasuredAllocator::new(RefuseResize {
                allocations_fail: false,
            });
            let data = refused_resize.alloc(original);
            assert!(!data.is_null());
            data.write_bytes(0x5a, original.size());
            let before = snapshot();
            assert!(refused_resize.realloc(data, original, 128).is_null());
            assert_eq!(snapshot(), before);
            assert_eq!(std::slice::from_raw_parts(data, 16), &[0x5a; 16]);
            refused_resize.dealloc(data, original);
            assert_eq!(snapshot().live_bytes, 0);
        }
    }
}
