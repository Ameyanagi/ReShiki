use super::*;
use std::{cell::Cell, future::Future, task::Poll};
#[tokio::test]
async fn response_limit_rejects_extra_byte_and_accepts_exact_boundary() {
    assert_eq!(
        read_limited(b"abcd".as_slice(), 4, "test").await.unwrap(),
        b"abcd"
    );
    assert!(matches!(
        read_limited(b"abcde".as_slice(), 4, "test").await,
        Err(Error::Limit("test"))
    ));
}
#[cfg(unix)]
#[tokio::test]
async fn deadline_and_dropping_future_kill_worker() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("worker");
    let pid_file = dir.path().join("pid");
    let quoted = pid_file.to_string_lossy().replace('\'', "'\\''");
    std::fs::write(
        &script,
        format!("#!/bin/sh\nprintf '%s' \"$$\" > '{quoted}'\nexec /bin/sleep 30\n"),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    // Signal the same deadline branch only after the child acknowledges
    // startup. OS executable validation can legitimately exceed a short
    // wall-clock deadline on the first run of a newly created script.
    let (trigger, deadline) = tokio::sync::oneshot::channel();
    let deadline_script = script.clone();
    let task = tokio::spawn(async move {
        exchange_with_deadline(deadline_script, b"x", async {
            let _ = deadline.await;
        })
        .await
    });
    let pid = ready_pid(&pid_file).await;
    trigger.send(()).unwrap();
    assert!(matches!(task.await.unwrap(), Err(Error::Timeout)));
    assert_process_stopped(&pid).await;
    std::fs::remove_file(&pid_file).unwrap();
    let task = tokio::spawn(async move {
        exchange(script, b"x", Instant::now() + Duration::from_secs(30)).await
    });
    let pid = ready_pid(&pid_file).await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert_process_stopped(&pid).await;
}
#[cfg(unix)]
async fn ready_pid(path: &std::path::Path) -> String {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(pid) = std::fs::read_to_string(path)
                && pid.trim().parse::<u32>().is_ok()
            {
                return pid;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("Geometry stub did not acknowledge startup")
}
#[cfg(unix)]
async fn assert_process_stopped(pid: &str) {
    for _ in 0..100 {
        let status = Command::new("/bin/ps")
            .args(["-p", pid, "-o", "pid="])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .unwrap();
        if !status.success() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("Cancelled worker {pid} remained alive");
}

#[tokio::test]
async fn expired_queued_request_never_reacquires_or_dispatches_worker() {
    let pool = tokio::sync::Semaphore::new(1);
    let occupied = pool.try_acquire().unwrap();
    let attempts = Cell::new(0);
    let dispatched = Cell::new(false);
    let dir = tempfile::tempdir().unwrap();
    let missing_worker = dir.path().join("must-not-be-spawned");
    let deadline = Instant::now() + Duration::from_secs(1);
    let request = async {
        let _reservation = reserve_until(deadline, || {
            attempts.set(attempts.get() + 1);
            pool.try_acquire().ok()
        })
        .await?;
        dispatched.set(true);
        exchange(missing_worker, b"x", deadline).await
    };
    tokio::pin!(request);
    // Establish an actual queued request, then stop polling it. Capacity is
    // released only after its absolute deadline, before that waiter resumes.
    assert!(std::future::poll_fn(|cx| Poll::Ready(request.as_mut().poll(cx).is_pending())).await);
    assert_eq!(attempts.get(), 1);
    assert_eq!(pool.available_permits(), 0);
    tokio::time::sleep_until(deadline).await;
    drop(occupied);
    assert!(matches!(request.await, Err(Error::Timeout)));
    assert_eq!(
        attempts.get(),
        1,
        "expired waiter must not reacquire capacity"
    );
    assert!(
        !dispatched.get(),
        "expired admission must not reach worker dispatch"
    );
    assert_eq!(pool.available_permits(), 1);

    // A live request can still reserve the entire isolated pool and release it.
    let reservation = reserve_until(Instant::now() + Duration::from_secs(1), || {
        pool.try_acquire().ok()
    })
    .await
    .unwrap();
    assert_eq!(pool.available_permits(), 0);
    drop(reservation);
    assert_eq!(pool.available_permits(), 1);
}

#[tokio::test]
async fn expiry_during_successful_acquisition_releases_the_reservation() {
    let pool = tokio::sync::Semaphore::new(1);
    let acquired = Cell::new(false);
    let deadline = Instant::now() + Duration::from_millis(50);
    let result = reserve_until(deadline, || {
        let reservation = pool.try_acquire().ok()?;
        acquired.set(true);
        // Simulate descheduling inside acquisition without changing the clock,
        // using a global pool, or relying on a guessed wake-up order.
        while Instant::now() < deadline {
            std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
        }
        Some(reservation)
    })
    .await;
    assert!(
        acquired.get(),
        "fixture must acquire before simulating expiry"
    );
    assert!(matches!(result, Err(Error::Timeout)));
    assert_eq!(
        pool.available_permits(),
        1,
        "late acquisition must be dropped"
    );
}

#[tokio::test]
async fn ready_deadline_wins_before_worker_spawn() {
    let dir = tempfile::tempdir().unwrap();
    let missing_worker = dir.path().join("missing-worker");
    // Spawning this path would immediately produce Io, so Timeout distinguishes
    // deadline rejection from a polled worker startup without launching a child.
    assert!(matches!(
        exchange(missing_worker.clone(), b"x", Instant::now()).await,
        Err(Error::Timeout)
    ));
    assert!(matches!(
        exchange_with_deadline(missing_worker.clone(), b"x", std::future::ready(())).await,
        Err(Error::Timeout)
    ));
    assert!(matches!(
        exchange(
            missing_worker,
            b"x",
            Instant::now() + Duration::from_secs(1)
        )
        .await,
        Err(Error::Io(_))
    ));
}
