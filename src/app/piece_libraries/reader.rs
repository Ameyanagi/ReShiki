//! Detached native readers for equipment library folders.
//!
//! A linked share can block indefinitely. Its reader must not hold Tokio's
//! blocking pool open when the editor exits.
use std::{io, panic::AssertUnwindSafe, thread::JoinHandle};
use tokio::sync::oneshot;

const FAILED: &str = "The library could not be read";

pub(super) async fn read<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    read_with_spawn(work, |worker| {
        std::thread::Builder::new()
            .name("reshiki-piece-library-read".into())
            .spawn(worker)
    })
    .await
}

async fn read_with_spawn<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
    spawn: impl FnOnce(Box<dyn FnOnce() + Send>) -> io::Result<JoinHandle<()>>,
) -> Result<T, String> {
    let (send, receive) = oneshot::channel();
    let reader = spawn(Box::new(move || {
        // Keep a worker panic in the ordinary result path, including its reason.
        let result = std::panic::catch_unwind(AssertUnwindSafe(work)).map_err(|panic| {
            let reason = panic
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| panic.downcast_ref::<&str>().copied())
                .unwrap_or("unknown panic");
            format!("{FAILED}: the scan worker panicked: {reason}")
        });
        let _ = send.send(result);
    }))
    .map_err(|e| format!("{FAILED}: {e}"))?;
    // Dropping the handle detaches the read. Dropping the async receiver at
    // shutdown never waits for an unresponsive filesystem operation.
    drop(reader);
    receive
        .await
        .map_err(|_| format!("{FAILED}: the scan worker stopped before returning a result"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
            mpsc,
        },
        thread,
        time::Duration,
    };

    #[tokio::test]
    async fn read_returns_its_value_on_a_named_detached_thread() {
        let caller = thread::current().id();
        assert_eq!(
            read(move || {
                assert_ne!(thread::current().id(), caller);
                assert_eq!(thread::current().name(), Some("reshiki-piece-library-read"));
                Some(42)
            })
            .await,
            Ok(Some(42))
        );
    }

    #[tokio::test]
    async fn panic_reason_and_thread_spawn_failure_are_returned_as_errors() {
        let error = read(|| panic!("Controlled scan panic")).await.unwrap_err();
        assert!(error.starts_with(FAILED));
        assert!(error.contains("Controlled scan panic"), "{error}");

        let ran = Arc::new(AtomicBool::new(false));
        let attempted = ran.clone();
        let error = read_with_spawn(
            move || attempted.store(true, Ordering::SeqCst),
            |_| Err(io::Error::other("Controlled thread spawn failure")),
        )
        .await
        .unwrap_err();
        assert_eq!(error, format!("{FAILED}: Controlled thread spawn failure"));
        assert!(!ran.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn missing_worker_receipt_is_returned_as_an_error() {
        let error = read_with_spawn(
            || (),
            |worker| {
                drop(worker);
                thread::Builder::new().spawn(|| ())
            },
        )
        .await
        .unwrap_err();
        assert_eq!(
            error,
            format!("{FAILED}: the scan worker stopped before returning a result")
        );
    }

    #[test]
    fn shutdown_does_not_wait_for_a_pending_library_read() {
        let (release, blocked) = mpsc::channel();
        let (ready, about_to_stop) = mpsc::channel();
        let (stopped, shutdown) = mpsc::channel();
        let app = thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap();
            let (reading, started) = oneshot::channel();
            runtime.block_on(async {
                tokio::spawn(read(move || {
                    let _ = reading.send(());
                    let _ = blocked.recv();
                }));
                started.await.unwrap();
            });
            ready.send(()).unwrap();
            drop(runtime);
            stopped.send(()).unwrap();
        });
        let pending = about_to_stop.recv_timeout(Duration::from_secs(5)).is_ok();
        let stopped_before_release =
            pending && shutdown.recv_timeout(Duration::from_secs(5)).is_ok();
        // Always release the controlled worker before checking the assertion.
        let _ = release.send(());
        app.join().unwrap();
        assert!(stopped_before_release);
    }
}
