//! Experimental MCP transport for ReShiki's agent API.
//!
//! This crate is experimental and its API may change without notice. It
//! owns the bounded line framing that `reshiki --mcp` serves over stdio:
//! [`framing`] reads and writes one JSON-RPC message per line on dedicated
//! threads, with admission, backpressure and cancellation tracking, and
//! [`log`] is the bounded, content-free stderr log. [`server`] serves the
//! MCP protocol over that framing with rmcp, through the crate's own
//! transport.
//!
//! The crate never installs a tracing subscriber and holds no static
//! connection state: every tracker, status and log belongs to one
//! connection, so later transports can serve several connections at once.
#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
        clippy::todo,
        clippy::unimplemented,
        clippy::indexing_slicing,
        // Stdout belongs to the MCP protocol, stderr to the bounded log.
        clippy::print_stdout,
        clippy::print_stderr,
        clippy::dbg_macro
    )
)]

pub mod framing;
pub mod log;
pub mod server;
mod transport;
