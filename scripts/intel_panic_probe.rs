//! Standalone diagnostic; no application, Tokio, chemistry or signing code.
#[cfg(allocator_system)]
use std::alloc::System;
use std::alloc::{Layout, alloc, dealloc};
use std::panic::{AssertUnwindSafe, catch_unwind, panic_any};
use std::sync::atomic::{AtomicUsize, Ordering};

#[cfg(allocator_system)]
#[global_allocator]
static ALLOCATOR: System = System;
#[cfg(allocator_bounded)]
#[global_allocator]
static ALLOCATOR: reshiki_process_heap::BoundedHeap = reshiki_process_heap::BoundedHeap;
#[cfg(allocator_measured)]
#[global_allocator]
static ALLOCATOR: reshiki_process_heap::allocation_metrics::MeasuredAllocator<
    reshiki_process_heap::BoundedHeap,
> = reshiki_process_heap::allocation_metrics::MeasuredAllocator::new(
    reshiki_process_heap::BoundedHeap,
);

unsafe extern "C" {
    fn malloc(size: usize) -> *mut std::ffi::c_void;
    fn free(data: *mut std::ffi::c_void);
}

static GUARD_DROPS: AtomicUsize = AtomicUsize::new(0);
static PAYLOAD_DROPS: AtomicUsize = AtomicUsize::new(0);
struct Guard;
impl Drop for Guard {
    fn drop(&mut self) {
        GUARD_DROPS.fetch_add(1, Ordering::SeqCst);
    }
}
#[derive(Debug)]
struct Payload(String);
impl Drop for Payload {
    fn drop(&mut self) {
        PAYLOAD_DROPS.fetch_add(1, Ordering::SeqCst);
    }
}

fn alignment() {
    for size in [8, 16, 32, 80, 96, 128] {
        let layout = Layout::from_size_align(size, 8).unwrap();
        // SAFETY: valid nonzero sizes; each allocator receives its own pointer.
        unsafe {
            let system = malloc(size);
            let user = alloc(layout);
            assert!(!system.is_null() && !user.is_null());
            println!(
                "size={size} requested_align=8 malloc={system:p} malloc_mod16={} user={user:p} user_mod16={}",
                system as usize % 16,
                user as usize % 16,
            );
            dealloc(user, layout);
            free(system);
        }
    }
}

fn panic_body(payload: bool) {
    let _guard = Guard;
    if payload {
        panic_any(Payload("diagnostic payload".into()));
    }
    panic!("diagnostic literal");
}

fn inspect(result: std::thread::Result<()>, payload: bool) {
    let error = result.expect_err("The intentional panic must be caught");
    assert_eq!(GUARD_DROPS.load(Ordering::SeqCst), 1);
    assert_eq!(PAYLOAD_DROPS.load(Ordering::SeqCst), 0);
    if payload {
        assert_eq!(
            error.downcast_ref::<Payload>().unwrap().0,
            "diagnostic payload"
        );
    } else {
        assert_eq!(error.downcast_ref::<&str>(), Some(&"diagnostic literal"));
    }
    println!("caught=true guard_drops=1 payload_verified=true");
    drop(error);
    assert_eq!(PAYLOAD_DROPS.load(Ordering::SeqCst), usize::from(payload));
    println!("payload_drops={} complete=true", usize::from(payload));
}

fn main() {
    let mode = std::env::args().nth(1).expect("A probe mode is required");
    eprintln!("mode={mode} pointer_bits={}", usize::BITS);
    match mode.as_str() {
        "alignment" => alignment(),
        "main-literal" | "main-payload" => {
            let payload = mode.ends_with("payload");
            inspect(
                catch_unwind(AssertUnwindSafe(|| panic_body(payload))),
                payload,
            );
        }
        "thread-literal" | "thread-payload" => {
            let payload = mode.ends_with("payload");
            std::thread::spawn(move || {
                inspect(
                    catch_unwind(AssertUnwindSafe(|| panic_body(payload))),
                    payload,
                );
            })
            .join()
            .expect("The thread must catch its own intentional panic");
        }
        "thread-join-payload" => {
            inspect(std::thread::spawn(|| panic_body(true)).join(), true);
        }
        _ => panic!("Unknown probe mode"),
    }
}
