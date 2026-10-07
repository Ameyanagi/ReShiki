use super::*;
use crate::ops::progress::Progress;
use std::{sync::atomic::AtomicUsize, time::Duration};
use tokio::task::JoinHandle;

/// A generous bound for every wait, so a bug fails instead of hanging.
const BOUND: Duration = Duration::from_secs(60);

async fn bounded<F: Future>(future: F) -> F::Output {
    tokio::time::timeout(BOUND, future)
        .await
        .expect("timed out")
}

/// Yields until `condition` holds.
async fn until(mut condition: impl FnMut() -> bool) {
    bounded(async {
        while !condition() {
            tokio::task::yield_now().await;
        }
    })
    .await;
}

/// Waits on a std barrier without blocking a runtime worker.
async fn wait(barrier: &Arc<Barrier>) {
    let barrier = barrier.clone();
    bounded(tokio::task::spawn_blocking(move || {
        barrier.wait();
    }))
    .await
    .unwrap();
}

fn executor(concurrency: usize, queue: usize) -> Executor {
    Executor::new(Budgets {
        concurrency,
        queue,
        ..Budgets::default()
    })
}

fn who() -> Principal {
    Principal::local()
}

fn id(n: i64) -> RequestId {
    RequestId::Int(n)
}

fn live(exec: &Executor) -> usize {
    exec.shared.registry().live
}

fn admitted(exec: &Executor, n: i64) -> bool {
    exec.shared.registry().tokens.contains_key(&(who(), id(n)))
}

fn entries(exec: &Executor) -> usize {
    exec.shared.registry().tokens.len()
}

fn permits(exec: &Executor) -> usize {
    exec.shared.permits.available_permits()
}

/// No registry entry, no waiting or live call, and every permit free.
fn assert_idle(exec: &Executor) {
    let registry = exec.shared.registry();
    assert!(registry.tokens.is_empty());
    assert_eq!((registry.waiting, registry.live), (0, 0));
    drop(registry);
    assert_eq!(permits(exec), exec.shared.budgets.concurrency);
}

/// Panics, as a bug in an operation would.
fn bug() -> Result<(), OpError> {
    panic!("operation bug")
}

/// Runs request `n` for [`who`] on its own task.
fn spawn_op<T, F, Fut>(exec: &Executor, n: i64, f: F) -> JoinHandle<Result<T, OpError>>
where
    F: FnOnce(Context) -> Fut + Send + 'static,
    Fut: Future<Output = Result<T, OpError>> + Send + 'static,
    T: Send + 'static,
{
    let exec = exec.clone();
    tokio::spawn(async move { exec.run(who(), id(n), None, f).await })
}

/// Two barriers a blocking job waits on: `started`, then `release`.
struct Gate {
    started: Arc<Barrier>,
    release: Arc<Barrier>,
}

impl Gate {
    fn new() -> Self {
        Self {
            started: Arc::new(Barrier::new(2)),
            release: Arc::new(Barrier::new(2)),
        }
    }

    fn job(&self) -> impl FnOnce() -> Result<(), OpError> + Send + 'static {
        let (started, release) = (self.started.clone(), self.release.clone());
        move || {
            started.wait();
            release.wait();
            Ok(())
        }
    }
}

/// Runs request `n` as a `ctx.blocking` job gated by `gate`.
fn spawn_gated(exec: &Executor, n: i64, gate: &Gate) -> JoinHandle<Result<(), OpError>> {
    let job = gate.job();
    spawn_op(exec, n, move |ctx| async move { ctx.blocking(job).await })
}

/// Runs request `n` as an operation that only sets `ran`.
fn spawn_flagged(
    exec: &Executor,
    n: i64,
    ran: &Arc<AtomicBool>,
) -> JoinHandle<Result<(), OpError>> {
    let ran = ran.clone();
    spawn_op(exec, n, move |_| async move {
        ran.store(true, Ordering::SeqCst);
        Ok(())
    })
}

#[test]
fn executor_and_its_futures_are_send() {
    fn send<T: Send>(_: &T) {}
    fn send_sync<T: Send + Sync>() {}
    send_sync::<Executor>();
    send_sync::<Context>();
    let exec = executor(1, 1);
    let run = exec.run(who(), id(1), None, |mut ctx| async move {
        ctx.blocking(|| Ok(())).await?;
        ctx.effect(|| Ok(())).await
    });
    send(&run);
    send(&exec.drained());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancelled_and_dropped_call_keeps_its_permit_until_its_blocking_job_ends() {
    let exec = executor(2, 8);
    let gate = Gate::new();
    let call = spawn_gated(&exec, 1, &gate);
    wait(&gate.started).await;
    assert_eq!(permits(&exec), 1);
    exec.cancel(&who(), &id(1));
    call.abort();
    assert!(bounded(call).await.unwrap_err().is_cancelled());
    // The caller is gone, but the blocking job still runs.
    tokio::task::yield_now().await;
    assert_eq!(permits(&exec), 1);
    assert_eq!(live(&exec), 1);
    wait(&gate.release).await;
    until(|| live(&exec) == 0).await;
    assert_idle(&exec);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelling_an_unknown_or_completed_request_is_a_no_op() {
    let exec = executor(2, 8);
    exec.cancel(&who(), &id(1));
    let first = exec.run(who(), id(1), None, |_| async { Ok(1) }).await;
    assert_eq!(first, Ok(1));
    exec.cancel(&who(), &id(1));
    let again = exec.run(who(), id(1), None, |ctx| async move {
        ctx.checkpoint()?;
        Ok(2)
    });
    assert_eq!(again.await, Ok(2));

    // Another principal's request and a string ID are different requests.
    let gate = Gate::new();
    let running = spawn_gated(&exec, 1, &gate);
    wait(&gate.started).await;
    exec.cancel(&Principal::new("other"), &id(1));
    exec.cancel(&who(), &RequestId::Str("1".into()));
    wait(&gate.release).await;
    assert_eq!(bounded(running).await.unwrap(), Ok(()));
    assert_idle(&exec);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_full_queue_is_busy_and_leaves_no_registry_entry() {
    let exec = executor(1, 1);
    let gate = Gate::new();
    let running = spawn_gated(&exec, 1, &gate);
    wait(&gate.started).await;
    let ran = Arc::new(AtomicBool::new(false));
    let queued = spawn_flagged(&exec, 2, &ran);
    until(|| exec.queued() == 1).await;

    let refused = exec.run(who(), id(3), None, |_| async { Ok(()) }).await;
    assert_eq!(refused, Err(busy("server busy; retry later")));
    assert_eq!(refused.unwrap_err().kind, ErrorKind::Busy);
    assert!(!admitted(&exec, 3));
    assert_eq!((entries(&exec), live(&exec), exec.queued()), (2, 2, 1));

    wait(&gate.release).await;
    assert_eq!(bounded(running).await.unwrap(), Ok(()));
    assert_eq!(bounded(queued).await.unwrap(), Ok(()));
    assert!(ran.load(Ordering::SeqCst));
    assert_idle(&exec);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_duplicate_in_flight_id_is_busy_and_reusable_after_completion() {
    let exec = executor(2, 8);
    let gate = Gate::new();
    let first = spawn_gated(&exec, 1, &gate);
    wait(&gate.started).await;

    let duplicate = exec.run(who(), id(1), None, |_| async { Ok(()) }).await;
    assert_eq!(duplicate, Err(busy("request id already in use")));
    let other = Principal::new("other");
    let other = exec.run(other, id(1), None, |_| async { Ok(2) }).await;
    assert_eq!(other, Ok(2));
    let string = exec.run(who(), RequestId::Str("1".into()), None, |_| async { Ok(3) });
    assert_eq!(string.await, Ok(3));
    assert_eq!(entries(&exec), 1);

    wait(&gate.release).await;
    assert_eq!(bounded(first).await.unwrap(), Ok(()));
    let reused = exec.run(who(), id(1), None, |_| async { Ok(4) }).await;
    assert_eq!(reused, Ok(4));
    assert_idle(&exec);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_passed_deadline_times_out_at_the_next_checkpoint() {
    let exec = Executor::new(Budgets {
        op_deadline: Duration::from_nanos(1),
        ..Budgets::default()
    });
    let ran = Arc::new(AtomicBool::new(false));
    let blocking = exec.run(who(), id(1), None, {
        let ran = ran.clone();
        move |ctx| async move {
            // The deadline is measured from admission; wait it out.
            while ctx
                .deadline
                .is_some_and(|deadline| Instant::now() < deadline)
            {
                std::hint::spin_loop();
            }
            ctx.blocking(move || {
                ran.store(true, Ordering::SeqCst);
                Ok(())
            })
            .await
        }
    });
    assert_eq!(blocking.await.unwrap_err().kind, ErrorKind::Timeout);

    let effect = exec.run(who(), id(2), None, {
        let ran = ran.clone();
        move |mut ctx| async move {
            ctx.deadline = Some(Instant::now());
            ctx.effect(move || {
                ran.store(true, Ordering::SeqCst);
                Ok(())
            })
            .await
        }
    });
    assert_eq!(effect.await.unwrap_err().kind, ErrorKind::Timeout);
    assert!(!ran.load(Ordering::SeqCst));
    assert_idle(&exec);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_deadline_that_passes_after_the_effect_still_returns_ok() {
    let exec = executor(2, 8);
    let result = exec.run(who(), id(1), None, |mut ctx| async move {
        let value = ctx.effect(|| Ok(7)).await?;
        ctx.deadline = Some(Instant::now());
        ctx.checkpoint()?;
        ctx.blocking(|| Ok(())).await?;
        Ok(value)
    });
    assert_eq!(result.await, Ok(7));
    assert_idle(&exec);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn drained_waits_for_a_running_blocking_job_and_refuses_new_calls() {
    let exec = executor(2, 8);
    let gate = Gate::new();
    let running = spawn_gated(&exec, 1, &gate);
    wait(&gate.started).await;
    let drain = tokio::spawn({
        let exec = exec.clone();
        async move { exec.drained().await }
    });
    until(|| exec.shared.registry().closed).await;

    let refused = exec.run(who(), id(2), None, |_| async { Ok(()) }).await;
    assert_eq!(refused, Err(busy("shutting down")));
    assert!(!drain.is_finished());
    assert_eq!((permits(&exec), live(&exec)), (1, 1));

    wait(&gate.release).await;
    bounded(drain).await.unwrap();
    assert_idle(&exec);
    assert_eq!(bounded(running).await.unwrap(), Err(cancelled()));
    bounded(exec.drained()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancelled_call_is_cancelled_even_when_its_operation_succeeds() {
    let exec = executor(2, 8);
    let gate = Gate::new();
    let job = gate.job();
    let call = spawn_op(&exec, 1, move |_| async move {
        // Never checks for cancellation.
        tokio::task::spawn_blocking(job).await.unwrap()?;
        Ok(5)
    });
    wait(&gate.started).await;
    exec.cancel(&who(), &id(1));
    wait(&gate.release).await;
    assert_eq!(bounded(call).await.unwrap(), Err(cancelled()));
    assert_idle(&exec);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_operation_that_panics_after_its_caller_is_dropped_releases_everything() {
    let exec = executor(2, 8);
    let gate = Gate::new();
    let job = gate.job();
    let call = spawn_op(&exec, 1, move |_| async move {
        tokio::task::spawn_blocking(job).await.unwrap()?;
        bug()
    });
    wait(&gate.started).await;
    call.abort();
    assert!(bounded(call).await.unwrap_err().is_cancelled());
    assert_eq!(live(&exec), 1);
    wait(&gate.release).await;
    bounded(exec.drained()).await;
    assert_idle(&exec);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn panics_become_internal_errors() {
    let exec = executor(2, 8);
    let direct = exec.run(who(), id(1), None, |_| async { bug() }).await;
    assert_eq!(direct, Err(internal()));
    let blocking = exec.run(
        who(),
        id(2),
        None,
        |ctx| async move { ctx.blocking(bug).await },
    );
    assert_eq!(blocking.await, Err(internal()));
    let effect = exec.run(who(), id(3), None, |mut ctx| async move {
        ctx.effect(bug).await
    });
    assert_eq!(effect.await, Err(internal()));
    assert_eq!(internal().kind, ErrorKind::Failed);
    assert_eq!(internal().message, "internal error");
    assert_idle(&exec);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_queued_caller_dropped_before_its_permit_leaves_nothing_behind() {
    let exec = executor(1, 8);
    let gate = Gate::new();
    let running = spawn_gated(&exec, 1, &gate);
    wait(&gate.started).await;
    let ran = Arc::new(AtomicBool::new(false));
    let queued = spawn_flagged(&exec, 2, &ran);
    until(|| exec.queued() == 1).await;
    assert_eq!((entries(&exec), live(&exec)), (2, 2));

    queued.abort();
    assert!(bounded(queued).await.unwrap_err().is_cancelled());
    assert_eq!((exec.queued(), entries(&exec), live(&exec)), (0, 1, 1));
    assert!(!admitted(&exec, 2));

    wait(&gate.release).await;
    assert_eq!(bounded(running).await.unwrap(), Ok(()));
    assert!(!ran.load(Ordering::SeqCst));
    assert_idle(&exec);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelling_a_queued_call_returns_cancelled_without_running_it() {
    let exec = executor(1, 8);
    let gate = Gate::new();
    let running = spawn_gated(&exec, 1, &gate);
    wait(&gate.started).await;
    let ran = Arc::new(AtomicBool::new(false));
    let queued = spawn_flagged(&exec, 2, &ran);
    until(|| exec.queued() == 1).await;

    exec.cancel(&who(), &id(2));
    let cancelled_call = bounded(queued).await.unwrap();
    assert_eq!(cancelled_call, Err(cancelled()));
    assert_eq!(cancelled_call.unwrap_err().kind, ErrorKind::Cancelled);
    assert_eq!((exec.queued(), entries(&exec), live(&exec)), (0, 1, 1));

    wait(&gate.release).await;
    assert_eq!(bounded(running).await.unwrap(), Ok(()));
    assert!(!ran.load(Ordering::SeqCst));
    assert_idle(&exec);
}

/// Counts an operation as ended when its future completes or is dropped.
struct Ended(Arc<AtomicUsize>);

impl Drop for Ended {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn admission_racing_drain_leaves_no_call_running() {
    const CALLERS: usize = 12;
    for _ in 0..100 {
        let exec = executor(2, 8);
        let entered = Arc::new(AtomicUsize::new(0));
        let ended = Arc::new(AtomicUsize::new(0));
        let start = Arc::new(tokio::sync::Barrier::new(CALLERS + 1));
        let callers: Vec<_> = (0..CALLERS)
            .map(|n| {
                let exec = exec.clone();
                let (start, entered, ended) = (start.clone(), entered.clone(), ended.clone());
                tokio::spawn(async move {
                    start.wait().await;
                    let n = i64::try_from(n).unwrap();
                    exec.run(who(), id(n), None, move |ctx| async move {
                        let _ended = Ended(ended);
                        entered.fetch_add(1, Ordering::SeqCst);
                        ctx.blocking(|| Ok(())).await?;
                        ctx.blocking(|| Ok(())).await
                    })
                    .await
                })
            })
            .collect();
        start.wait().await;
        bounded(exec.drained()).await;

        assert_eq!(entered.load(Ordering::SeqCst), ended.load(Ordering::SeqCst));
        assert_idle(&exec);
        let late = exec.run(who(), id(-1), None, |_| async { Ok(()) }).await;
        assert_eq!(late, Err(busy("shutting down")));
        for caller in callers {
            let kind = bounded(caller).await.unwrap().map_err(|error| error.kind);
            assert!(
                matches!(kind, Ok(()) | Err(ErrorKind::Cancelled | ErrorKind::Busy)),
                "{kind:?}"
            );
        }
        assert_eq!(entered.load(Ordering::SeqCst), ended.load(Ordering::SeqCst));
        assert_idle(&exec);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_parked_after_the_effect_keeps_the_effect_and_returns_cancelled() {
    let exec = executor(2, 8);
    let before = Arc::new(Barrier::new(2));
    let after = Arc::new(Barrier::new(2));
    exec.set_hooks(Hooks {
        before_effect: Some(before.clone()),
        after_effect: Some(after.clone()),
    });
    let effects = Arc::new(AtomicUsize::new(0));
    let returned = Arc::new(AtomicBool::new(false));
    let call = spawn_op(&exec, 1, {
        let (effects, returned) = (effects.clone(), returned.clone());
        move |mut ctx| async move {
            let value = ctx
                .effect(move || {
                    effects.fetch_add(1, Ordering::SeqCst);
                    Ok(7)
                })
                .await?;
            // Committed: the cancel below does not fail this checkpoint.
            ctx.checkpoint()?;
            returned.store(true, Ordering::SeqCst);
            Ok(value)
        }
    });
    wait(&before).await;
    until(|| effects.load(Ordering::SeqCst) == 1).await;
    exec.cancel(&who(), &id(1));
    wait(&after).await;

    assert_eq!(bounded(call).await.unwrap(), Err(cancelled()));
    assert_eq!(effects.load(Ordering::SeqCst), 1);
    assert!(returned.load(Ordering::SeqCst));
    assert_idle(&exec);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn progress_is_forwarded_until_the_operation_ends() {
    let exec = executor(2, 8);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink: Sink = {
        let seen = seen.clone();
        Arc::new(move |progress: Progress| seen.lock().unwrap().push(progress.completed))
    };
    let at = |completed| Progress {
        completed,
        total: None,
        message: None,
    };
    let progress = exec.run(who(), id(1), Some(sink), move |ctx| async move {
        let progress = ctx.progress().unwrap();
        progress.report(at(1.));
        Ok(progress)
    });
    let progress = progress.await.unwrap();
    progress.report(at(2.));
    assert_eq!(*seen.lock().unwrap(), [1.]);

    let none = exec.run(who(), id(2), None, |ctx| async move {
        Ok(ctx.progress().is_none())
    });
    assert_eq!(none.await, Ok(true));
}
