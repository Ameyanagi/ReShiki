//! Exercise native unwinding through the allocator used by the application.
#![forbid(unsafe_code)]

use reshiki_process_heap::BoundedHeap;
use std::panic::{catch_unwind, panic_any};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[cfg(not(feature = "allocation-metrics"))]
#[global_allocator]
static ALLOCATOR: BoundedHeap = BoundedHeap;
#[cfg(feature = "allocation-metrics")]
#[global_allocator]
static ALLOCATOR: reshiki_process_heap::allocation_metrics::MeasuredAllocator<BoundedHeap> =
    reshiki_process_heap::allocation_metrics::MeasuredAllocator::new(BoundedHeap);

struct Guard(Arc<AtomicUsize>);
impl Drop for Guard {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
struct Payload {
    message: String,
    drops: Arc<AtomicUsize>,
}
impl Drop for Payload {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

fn assert_panic_is_recoverable(use_thread: bool, custom_payload: bool) {
    // Counters belong to this panic, independently of parallel test execution.
    let guard_drops = Arc::new(AtomicUsize::new(0));
    let payload_drops = Arc::new(AtomicUsize::new(0));
    let guard_count = Arc::clone(&guard_drops);
    let payload_count = Arc::clone(&payload_drops);
    let work = move || {
        let _guard = Guard(guard_count);
        if custom_payload {
            panic_any(Payload {
                message: "owned panic payload".into(),
                drops: payload_count,
            });
        }
        panic!("literal panic payload");
    };
    let result = if use_thread {
        std::thread::spawn(work).join()
    } else {
        catch_unwind(work)
    };
    let error = result.expect_err("The intentional panic must be recovered");
    assert_eq!(guard_drops.load(Ordering::SeqCst), 1);
    assert_eq!(payload_drops.load(Ordering::SeqCst), 0);
    if custom_payload {
        assert_eq!(
            error.downcast_ref::<Payload>().unwrap().message,
            "owned panic payload"
        );
    } else {
        assert_eq!(error.downcast_ref::<&str>(), Some(&"literal panic payload"));
    }
    drop(error);
    assert_eq!(
        payload_drops.load(Ordering::SeqCst),
        usize::from(custom_payload)
    );
}

#[test]
fn catch_unwind_preserves_literal_and_owned_payloads_and_runs_destructors() {
    for custom_payload in [false, true] {
        assert_panic_is_recoverable(false, custom_payload);
    }
}

#[test]
fn thread_join_preserves_literal_and_owned_payloads_and_runs_destructors() {
    for custom_payload in [false, true] {
        assert_panic_is_recoverable(true, custom_payload);
    }
}
