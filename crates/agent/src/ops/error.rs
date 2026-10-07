//! Operation errors and their stable wire codes.
use crate::transaction::Rejection;
use std::fmt;

/// Why an operation failed.
///
/// Only [`ErrorKind::UnknownTool`] is a protocol error (the transport sends a
/// JSON-RPC error). Every other kind is a tool execution error, reported as a
/// result with `isError` set
/// (<https://modelcontextprotocol.io/specification/2026-07-28/server/tools#error-handling>).
/// [`ErrorKind::Cancelled`] is the exception to both: no response is sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    UnknownTool,
    InvalidArguments,
    UnknownDocument,
    UnknownObject,
    Stale,
    Busy,
    Budget,
    Timeout,
    Rejected,
    Unsupported,
    Failed,
    /// The caller cancelled the request; no response is sent.
    Cancelled,
}

impl ErrorKind {
    /// The stable snake_case code clients see.
    pub fn code(self) -> &'static str {
        match self {
            Self::UnknownTool => "unknown_tool",
            Self::InvalidArguments => "invalid_arguments",
            Self::UnknownDocument => "unknown_document",
            Self::UnknownObject => "unknown_object",
            Self::Stale => "stale",
            Self::Busy => "busy",
            Self::Budget => "budget",
            Self::Timeout => "timeout",
            Self::Rejected => "rejected",
            Self::Unsupported => "unsupported",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    /// True only for [`ErrorKind::UnknownTool`].
    pub fn is_protocol_error(self) -> bool {
        matches!(self, Self::UnknownTool)
    }
}

/// An operation error with a bounded, client-facing message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpError {
    pub kind: ErrorKind,
    pub message: String,
}

impl OpError {
    /// Messages are cut to this many characters so they never echo large input.
    pub const MAX_MESSAGE_CHARS: usize = 500;

    /// Keeps at most [`OpError::MAX_MESSAGE_CHARS`] characters of `message`,
    /// never splitting a character.
    pub fn new(kind: ErrorKind, message: impl AsRef<str>) -> Self {
        Self {
            kind,
            message: message
                .as_ref()
                .chars()
                .take(Self::MAX_MESSAGE_CHARS)
                .collect(),
        }
    }
}

impl fmt::Display for OpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for OpError {}

impl From<Rejection> for OpError {
    fn from(rejection: Rejection) -> Self {
        Self::new(ErrorKind::Rejected, rejection.message())
    }
}
