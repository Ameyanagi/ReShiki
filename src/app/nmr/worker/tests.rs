use super::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use tokio::sync::oneshot;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn busy_admission_rejects_without_running_a_second_job() {
    static PERMITS: Semaphore = Semaphore::const_new(1);
    let (started_tx, started_rx) = oneshot::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let active = tokio::spawn(run(&PERMITS, PREDICTION_TIMEOUT, move || {
        started_tx.send(()).unwrap();
        release_rx.recv().unwrap();
        Ok(1)
    }));
    started_rx.await.unwrap();

    let ran = Arc::new(AtomicBool::new(false));
    let attempted = Arc::clone(&ran);
    let result = run(&PERMITS, PREDICTION_TIMEOUT, move || {
        attempted.store(true, Ordering::SeqCst);
        Ok(2)
    })
    .await;
    assert_eq!(
        result,
        Err("An NMR calculation is already running; try again shortly".into())
    );
    assert!(!ran.load(Ordering::SeqCst));

    release_tx.send(()).unwrap();
    assert_eq!(active.await.unwrap(), Ok(1));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn timeout_keeps_the_permit_until_the_blocking_job_finishes() {
    static PERMITS: Semaphore = Semaphore::const_new(1);
    let (started_tx, started_rx) = oneshot::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let active = tokio::spawn(run(&PERMITS, Duration::ZERO, move || {
        started_tx.send(()).unwrap();
        release_rx.recv().unwrap();
        Ok(1)
    }));
    started_rx.await.unwrap();
    assert_eq!(
        active.await.unwrap(),
        Err("NMR prediction exceeded its time limit".into())
    );
    assert_eq!(PERMITS.available_permits(), 0);
    assert_eq!(
        run(&PERMITS, PREDICTION_TIMEOUT, || Ok(2)).await,
        Err("An NMR calculation is already running; try again shortly".into())
    );

    release_tx.send(()).unwrap();
    // Acquiring waits for the detached blocking closure to release its permit.
    let permit = PERMITS.acquire().await.unwrap();
    drop(permit);
    assert_eq!(run(&PERMITS, PREDICTION_TIMEOUT, || Ok(3)).await, Ok(3));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inner_error_is_preserved_and_releases_the_permit() {
    static PERMITS: Semaphore = Semaphore::const_new(1);
    let result = run::<()>(&PERMITS, PREDICTION_TIMEOUT, || {
        Err("Test prediction rejected its input".into())
    })
    .await;
    assert_eq!(result, Err("Test prediction rejected its input".into()));
    assert_eq!(run(&PERMITS, PREDICTION_TIMEOUT, || Ok(1)).await, Ok(1));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn panic_maps_the_join_error_and_releases_the_permit() {
    static PERMITS: Semaphore = Semaphore::const_new(1);
    let error = run::<()>(&PERMITS, PREDICTION_TIMEOUT, || {
        panic!("Test prediction panic");
    })
    .await
    .unwrap_err();
    assert!(error.starts_with("NMR calculation failed: "));
    assert!(error.contains("Test prediction panic"));
    assert_eq!(run(&PERMITS, PREDICTION_TIMEOUT, || Ok(1)).await, Ok(1));
}
