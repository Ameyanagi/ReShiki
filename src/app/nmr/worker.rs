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

#[cfg(not(target_arch = "wasm32"))]
async fn run<T: Send + 'static>(
    work: &'static Semaphore,
    timeout: Duration,
    calculate: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let permit = work
        .try_acquire()
        .map_err(|_| "An NMR calculation is already running; try again shortly")?;
    let job = reshiki::task::spawn_blocking(move || {
        let _permit = permit;
        calculate()
    });
    tokio::time::timeout(timeout, job)
        .await
        .map_err(|_| "NMR prediction exceeded its time limit")?
        .map_err(|e| format!("NMR calculation failed: {e}"))?
}

/// The browser has no worker thread yet, so prediction runs on the page's
/// thread, bounded by the engine's three-second `Environment::deadline()`.
#[cfg(target_arch = "wasm32")]
async fn run<T>(
    work: &'static Semaphore,
    _timeout: Duration,
    calculate: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let _permit = work
        .try_acquire()
        .map_err(|_| "An NMR calculation is already running; try again shortly")?;
    after_paint().await;
    calculate()
}

/// Waits until the next frame is drawn, so the busy state shows before the
/// page's thread is held.
#[cfg(target_arch = "wasm32")]
async fn after_paint() {
    for frame in [true, false] {
        let next = js_sys::Promise::new(&mut |resolve, _| {
            let scheduled = web_sys::window().and_then(|window| match frame {
                true => window.request_animation_frame(&resolve).ok().map(drop),
                false => window.set_timeout_with_callback(&resolve).ok().map(drop),
            });
            if scheduled.is_none() {
                let _ = resolve.call0(&wasm_bindgen::JsValue::NULL);
            }
        });
        let _ = wasm_bindgen_futures::JsFuture::from(next).await;
    }
}

#[cfg(test)]
mod tests;
