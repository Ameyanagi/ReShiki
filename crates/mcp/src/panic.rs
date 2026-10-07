//! A content-free panic hook for the MCP server process.
use crate::log::{Level, Log};

/// Replaces the process's panic hook with one that logs only where a panic
/// happened, as `worker panicked at <file>:<line>`, through `log`.
///
/// The payload is never formatted or printed: it may carry client text such
/// as a tool argument or document content. The `panicked at` marker stays so
/// a supervisor can still recognize a panic on stderr.
pub fn install_panic_hook(log: Log) {
    std::panic::set_hook(Box::new(move |info| match info.location() {
        Some(location) => log.panicked(location.file(), location.line()),
        None => log.event(Level::Error, "worker panicked at <unknown>", &[]),
    }));
}
