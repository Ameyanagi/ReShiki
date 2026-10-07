//! Progress reports from a running operation to the transport.
use crate::progress::Event;
use std::sync::{Arc, Mutex, PoisonError};
use tokio::sync::mpsc::Receiver;

/// Receives progress reports for one call.
pub type Sink = Arc<dyn Fn(Progress) + Send + Sync>;

/// One progress report, as in MCP `notifications/progress`.
#[derive(Debug, Clone, PartialEq)]
pub struct Progress {
    pub completed: f64,
    pub total: Option<f64>,
    pub message: Option<String>,
}

/// Forwards progress to a [`Sink`] only while it keeps increasing.
///
/// MCP progress MUST increase with each notification and stop once the
/// request completes
/// (<https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/progress>).
/// A report is dropped when its `completed` is not finite or not above the
/// last forwarded value, and every report after [`Monotonic::finish`] is
/// dropped.
pub struct Monotonic {
    sink: Sink,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    last: Option<f64>,
    finished: bool,
}

impl Monotonic {
    pub fn new(sink: Sink) -> Self {
        Self {
            sink,
            state: Mutex::default(),
        }
    }

    /// Forwards `progress` if it is the highest so far and the call has not
    /// finished.
    pub fn report(&self, progress: Progress) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if state.finished
            || !progress.completed.is_finite()
            || state.last.is_some_and(|last| progress.completed <= last)
        {
            return;
        }
        state.last = Some(progress.completed);
        // The sink runs under the lock, so reports reach it in order and none
        // reaches it after `finish` returns.
        (self.sink)(progress);
    }

    /// Drops every later report.
    pub fn finish(&self) {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .finished = true;
    }
}

/// Reports each [`Event::Structures`] from `rx` to `sink` until every sender
/// is dropped. Proposals and previews are ignored.
pub async fn forward(mut rx: Receiver<Event>, sink: Arc<Monotonic>) {
    while let Some(event) = rx.recv().await {
        if let Event::Structures { completed, total } = event {
            sink.report(Progress {
                completed: completed as f64,
                total: Some(total as f64),
                message: None,
            });
        }
    }
}

#[cfg(test)]
mod tests;
