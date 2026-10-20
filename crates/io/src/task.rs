//! Blocking chemistry and rendering work off the async executor. The browser
//! has no threads to hand it to, so there it runs inline on the caller's task.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use tokio::task::spawn_blocking;

/// Runs `work` at once; it never fails to join.
#[cfg(target_arch = "wasm32")]
pub(crate) async fn spawn_blocking<F, R>(work: F) -> Result<R, tokio::task::JoinError>
where
    F: FnOnce() -> R,
{
    Ok(work())
}
