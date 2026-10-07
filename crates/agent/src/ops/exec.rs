//! The bounded executor: request IDs, admission, permits, cooperative
//! cancellation, deadlines and an uncancellable effect phase.
//!
//! [`Executor::run`] admits a call, waits for one of
//! [`Budgets::concurrency`] permits and runs the operation on its own task.
//! The admission, the permit and the progress finisher move into that task,
//! which is never aborted. They are released only once the operation has
//! ended, including every [`Context::blocking`] and [`Context::effect`] job
//! it awaits, whether it returned, panicked or lost its caller. Blocking work
//! keeps running when its future is dropped, so a permit released earlier
//! would let detached work exceed the concurrency budget.
//!
//! Cancellation and deadlines are cooperative: an operation observes them at
//! [`Context::checkpoint`], which [`Context::blocking`] calls before and after
//! its job. [`Context::effect`] is the last such point. Once its job starts,
//! the operation is committed and later checkpoints always pass, so a
//! deadline that passes after the commit never turns the result into a
//! timeout. A cancelled request still ends in [`ErrorKind::Cancelled`] (no
//! response is sent), and anything the effect stored stays replayable.
//!
//! Only tokio's `sync` and `rt` features are used: no `select!` and no tokio
//! timers. The transport bounds [`Executor::drained`] with its own clock.
use super::{
    budget::Budgets,
    error::{ErrorKind, OpError},
    progress::{Monotonic, Sink},
    wire::{Principal, RequestId},
};
#[cfg(test)]
use std::sync::Barrier;
use std::{
    collections::HashMap,
    future::{Future, poll_fn},
    pin::pin,
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    task::Poll,
    time::Instant,
};
use tokio::sync::{Notify, Semaphore};

type Key = (Principal, RequestId);

fn busy(message: &str) -> OpError {
    OpError::new(ErrorKind::Busy, message)
}

fn cancelled() -> OpError {
    OpError::new(ErrorKind::Cancelled, "request cancelled")
}

fn internal() -> OpError {
    OpError::new(ErrorKind::Failed, "internal error")
}

/// Set once, by [`Executor::cancel`] or [`Executor::drained`].
#[derive(Clone, Default)]
struct CancelToken(Arc<TokenState>);

#[derive(Default)]
struct TokenState {
    flag: AtomicBool,
    notify: Notify,
}

impl CancelToken {
    fn cancel(&self) {
        self.0.flag.store(true, Ordering::SeqCst);
        self.0.notify.notify_waiters();
    }

    fn is_cancelled(&self) -> bool {
        self.0.flag.load(Ordering::SeqCst)
    }

    /// Resolves once the token is cancelled.
    async fn cancelled(&self) {
        loop {
            // Register before checking the flag, so a cancel between the
            // check and the await still wakes this waiter.
            let mut notified = pin!(self.0.notify.notified());
            notified.as_mut().enable();
            if self.is_cancelled() {
                return;
            }
            notified.await;
        }
    }
}

/// The admitted calls.
#[derive(Default)]
struct Registry {
    /// Set by [`Executor::drained`]; no call is admitted afterwards.
    closed: bool,
    /// Admitted calls still waiting for a permit.
    waiting: usize,
    /// Admitted calls, waiting or running.
    live: usize,
    tokens: HashMap<Key, CancelToken>,
}

struct Shared {
    registry: Mutex<Registry>,
    permits: Arc<Semaphore>,
    /// Notified when `live` drops to zero.
    idle: Notify,
    budgets: Budgets,
    #[cfg(test)]
    hooks: Mutex<Hooks>,
}

impl Shared {
    /// Recovers from poisoning, so admission cleanup always runs.
    fn registry(&self) -> MutexGuard<'_, Registry> {
        self.registry.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// One admitted call. Dropping it removes the call from the registry,
/// whether the call finished, was refused a permit or lost its caller.
struct Admission {
    shared: Arc<Shared>,
    key: Key,
    /// Still counted in `waiting`.
    queued: bool,
}

impl Drop for Admission {
    fn drop(&mut self) {
        let mut registry = self.shared.registry();
        registry.tokens.remove(&self.key);
        if self.queued {
            registry.waiting = registry.waiting.saturating_sub(1);
        }
        registry.live = registry.live.saturating_sub(1);
        if registry.live == 0 {
            self.shared.idle.notify_waiters();
        }
    }
}

/// Finishes the call's progress when the operation ends.
struct FinishGuard(Option<Arc<Monotonic>>);

impl Drop for FinishGuard {
    fn drop(&mut self) {
        if let Some(progress) = &self.0 {
            progress.finish();
        }
    }
}

/// Runs operations within [`Budgets::concurrency`] permits and a queue of
/// [`Budgets::queue`] waiting calls, each under a [`Budgets::op_deadline`]
/// measured from admission.
#[derive(Clone)]
pub struct Executor {
    shared: Arc<Shared>,
}

impl Executor {
    pub fn new(budgets: Budgets) -> Self {
        let permits = budgets.concurrency.min(Semaphore::MAX_PERMITS);
        Self {
            shared: Arc::new(Shared {
                registry: Mutex::default(),
                permits: Arc::new(Semaphore::new(permits)),
                idle: Notify::new(),
                budgets,
                #[cfg(test)]
                hooks: Mutex::default(),
            }),
        }
    }

    /// Runs `f` as the principal's request `id`.
    ///
    /// - [`ErrorKind::Busy`] when the executor is shutting down, `id` is
    ///   already in flight for `who` (MCP forbids reusing in-flight IDs), or
    ///   [`Budgets::queue`] calls already wait for a permit.
    /// - [`ErrorKind::Cancelled`] when the call is cancelled at any point,
    ///   whatever `f` returned; `f` never runs if the call is cancelled while
    ///   it waits for a permit.
    /// - [`ErrorKind::Failed`] when `f` panics.
    ///
    /// Dropping the returned future never stops `f` once it has started:
    /// `f` keeps its permit until it ends.
    pub async fn run<T, F, Fut>(
        &self,
        who: Principal,
        id: RequestId,
        progress: Option<Sink>,
        f: F,
    ) -> Result<T, OpError>
    where
        F: FnOnce(Context) -> Fut + Send + 'static,
        Fut: Future<Output = Result<T, OpError>> + Send + 'static,
        T: Send + 'static,
    {
        let deadline = Instant::now().checked_add(self.shared.budgets.op_deadline);
        let (token, mut admission) = self.admit((who, id))?;
        let permit = {
            let mut permit = pin!(Arc::clone(&self.shared.permits).acquire_owned());
            let mut cancel = pin!(token.cancelled());
            poll_fn(|cx| {
                // Cancellation wins a tie, so a cancelled call never starts.
                if cancel.as_mut().poll(cx).is_ready() {
                    return Poll::Ready(None);
                }
                permit.as_mut().poll(cx).map(Some)
            })
            .await
        };
        let permit = match permit {
            Some(Ok(permit)) => permit,
            // The semaphore is never closed.
            Some(Err(_)) => return Err(internal()),
            None => return Err(cancelled()),
        };
        {
            let mut registry = self.shared.registry();
            registry.waiting = registry.waiting.saturating_sub(1);
            admission.queued = false;
        }
        let progress = progress.map(|sink| Arc::new(Monotonic::new(sink)));
        let ctx = Context {
            token: token.clone(),
            deadline,
            progress: progress.clone(),
            committed: false,
            #[cfg(test)]
            hooks: self
                .shared
                .hooks
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone(),
        };
        let finish = FinishGuard(progress);
        let task = tokio::spawn(async move {
            // Dropped in reverse order when `f` returns or unwinds: the
            // progress finishes, the permit returns, then the call leaves the
            // registry.
            let _admission = admission;
            let _permit = permit;
            let _finish = finish;
            f(ctx).await
        });
        // Awaited but never aborted. Dropping the handle detaches the task,
        // which keeps the permit until `f` ends.
        let result = task.await.unwrap_or_else(|_| Err(internal()));
        if token.is_cancelled() {
            return Err(cancelled());
        }
        result
    }

    /// Admits `key` under one registry lock.
    fn admit(&self, key: Key) -> Result<(CancelToken, Admission), OpError> {
        let mut registry = self.shared.registry();
        if registry.closed {
            return Err(busy("shutting down"));
        }
        if registry.tokens.contains_key(&key) {
            return Err(busy("request id already in use"));
        }
        if registry.waiting >= self.shared.budgets.queue {
            return Err(busy("server busy; retry later"));
        }
        let token = CancelToken::default();
        registry.tokens.insert(key.clone(), token.clone());
        registry.waiting += 1;
        registry.live += 1;
        let admission = Admission {
            shared: Arc::clone(&self.shared),
            key,
            queued: true,
        };
        Ok((token, admission))
    }

    /// Cancels the principal's in-flight request `id`. Unknown and completed
    /// requests are ignored
    /// (<https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/cancellation>).
    pub fn cancel(&self, who: &Principal, id: &RequestId) {
        let registry = self.shared.registry();
        if let Some(token) = registry.tokens.get(&(who.clone(), id.clone())) {
            token.cancel();
        }
    }

    /// Refuses new calls, cancels every admitted one and resolves once all of
    /// them have ended. The transport bounds the wait.
    pub async fn drained(&self) {
        {
            let mut registry = self.shared.registry();
            registry.closed = true;
            for token in registry.tokens.values() {
                token.cancel();
            }
        }
        loop {
            // Register before checking, so the last call ending between the
            // check and the await still wakes this waiter.
            let mut idle = pin!(self.shared.idle.notified());
            idle.as_mut().enable();
            let live = self.shared.registry().live;
            if live == 0 {
                break;
            }
            idle.await;
        }
    }

    /// Installs barriers around every later effect job.
    #[cfg(test)]
    pub(crate) fn set_hooks(&self, hooks: Hooks) {
        *self
            .shared
            .hooks
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = hooks;
    }

    /// Admitted calls still waiting for a permit.
    #[cfg(test)]
    pub(crate) fn queued(&self) -> usize {
        self.shared.registry().waiting
    }
}

/// Test barriers around [`Context::effect`] jobs.
#[cfg(test)]
#[derive(Clone, Default)]
pub(crate) struct Hooks {
    /// Waited on by the blocking job just before the effect runs.
    pub(crate) before_effect: Option<Arc<Barrier>>,
    /// Waited on by the blocking job just after the effect returns.
    pub(crate) after_effect: Option<Arc<Barrier>>,
}

#[cfg(test)]
impl Hooks {
    fn around<R>(
        &self,
        f: impl FnOnce() -> Result<R, OpError> + Send + 'static,
    ) -> impl FnOnce() -> Result<R, OpError> + Send + 'static {
        let before = self.before_effect.clone();
        let after = self.after_effect.clone();
        move || {
            if let Some(before) = before {
                before.wait();
            }
            let result = f();
            if let Some(after) = after {
                after.wait();
            }
            result
        }
    }
}

/// What a running operation sees of its call.
pub struct Context {
    token: CancelToken,
    /// `None` when the deadline is too far away to represent.
    deadline: Option<Instant>,
    progress: Option<Arc<Monotonic>>,
    /// Set when the effect starts. Later checkpoints always pass.
    committed: bool,
    #[cfg(test)]
    hooks: Hooks,
}

impl Context {
    /// Ok once committed; otherwise [`ErrorKind::Cancelled`] when the call
    /// was cancelled and [`ErrorKind::Timeout`] when its deadline has passed.
    pub fn checkpoint(&self) -> Result<(), OpError> {
        if self.committed {
            return Ok(());
        }
        if self.token.is_cancelled() {
            return Err(cancelled());
        }
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(OpError::new(
                ErrorKind::Timeout,
                "the operation exceeded its deadline",
            ));
        }
        Ok(())
    }

    /// Runs `f` on the blocking pool between two checkpoints. A panic in `f`
    /// becomes [`ErrorKind::Failed`].
    pub async fn blocking<R: Send + 'static>(
        &self,
        f: impl FnOnce() -> Result<R, OpError> + Send + 'static,
    ) -> Result<R, OpError> {
        self.checkpoint()?;
        let result = tokio::task::spawn_blocking(f)
            .await
            .unwrap_or_else(|_| Err(internal()));
        self.checkpoint()?;
        result
    }

    /// Runs the operation's effect `f` on the blocking pool after a final
    /// checkpoint. The operation is committed from then on: there is no
    /// checkpoint after `f`, and every later checkpoint passes. A panic in
    /// `f` becomes [`ErrorKind::Failed`].
    pub async fn effect<R: Send + 'static>(
        &mut self,
        f: impl FnOnce() -> Result<R, OpError> + Send + 'static,
    ) -> Result<R, OpError> {
        self.checkpoint()?;
        self.committed = true;
        #[cfg(test)]
        let f = self.hooks.around(f);
        tokio::task::spawn_blocking(f)
            .await
            .unwrap_or_else(|_| Err(internal()))
    }

    /// The call's progress, when the client asked for it.
    pub fn progress(&self) -> Option<Arc<Monotonic>> {
        self.progress.clone()
    }
}

#[cfg(test)]
mod tests;
