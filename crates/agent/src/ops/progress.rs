//! Progress reports from a running operation to the transport.
use std::sync::Arc;

/// Receives progress reports for one call.
pub type Sink = Arc<dyn Fn(Progress) + Send + Sync>;

/// One progress report, as in MCP `notifications/progress`.
#[derive(Debug, Clone, PartialEq)]
pub struct Progress {
    pub completed: f64,
    pub total: Option<f64>,
    pub message: Option<String>,
}
