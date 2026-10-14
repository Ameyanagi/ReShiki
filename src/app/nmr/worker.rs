//! Detached prediction work with process-wide admission and an outer timeout.
use reshiki::nmr::{self as engine, Nucleus, Report};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

static WORK: Semaphore = Semaphore::const_new(1);

/// Must exceed the engine's three-second `Environment::deadline()` work
/// deadline so an engine error normally arrives before this outer timeout.
pub(super) const PREDICTION_TIMEOUT: Duration = Duration::from_secs(6);

pub(super) async fn calculate(
    request: engine::Request,
    nucleus: Nucleus,
) -> Result<Arc<Report>, String> {
    run(&WORK, PREDICTION_TIMEOUT, move || {
        engine::predict(&request, nucleus).map(Arc::new)
    })
    .await
}

async fn run<T: Send + 'static>(
    work: &'static Semaphore,
    timeout: Duration,
    calculate: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let permit = work
        .try_acquire()
        .map_err(|_| "An NMR calculation is already running; try again shortly")?;
    let job = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        calculate()
    });
    tokio::time::timeout(timeout, job)
        .await
        .map_err(|_| "NMR prediction exceeded its time limit")?
        .map_err(|e| format!("NMR calculation failed: {e}"))?
}

#[cfg(test)]
mod tests;
