//! Operation errors and their stable wire codes.
use crate::{access::AccessError, transaction::Rejection};
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
    /// A file request outside the granted folders or extensions, or one the
    /// operating system denied.
    Access,
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
            Self::Access => "access_denied",
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

/// Appended to `path_not_granted`: only the user can grant a folder.
const GRANT_HINT: &str = ". Folders are granted by the user when the server starts, with --allow-read and --allow-write or in agent-access.json in ReShiki's data folder; info lists them.";

/// The message is `"{access code}: {message}"`, so clients and tests can
/// read the specific reason under the operation code; `path_not_granted`
/// adds how folders are granted. When that would be cut at
/// [`OpError::MAX_MESSAGE_CHARS`], the echoed path and lists are
/// [`shortened`] instead, so the reason and the hint stay whole.
///
/// - `path_invalid` is [`ErrorKind::InvalidArguments`];
/// - `extension_not_allowed`, `path_not_granted`, `path_escapes_root` and
///   `os_denied` are [`ErrorKind::Access`];
/// - `file_too_large` is [`ErrorKind::Budget`];
/// - `file_exists`, `file_not_found`, `not_a_regular_file`,
///   `no_clobber_unsupported` and `io_error` are [`ErrorKind::Failed`].
impl From<AccessError> for OpError {
    fn from(error: AccessError) -> Self {
        let kind = match error {
            AccessError::PathInvalid { .. } => ErrorKind::InvalidArguments,
            AccessError::ExtensionNotAllowed { .. }
            | AccessError::PathNotGranted { .. }
            | AccessError::PathEscapesRoot { .. }
            | AccessError::OsDenied { .. } => ErrorKind::Access,
            AccessError::FileTooLarge { .. } => ErrorKind::Budget,
            AccessError::FileExists { .. }
            | AccessError::FileNotFound { .. }
            | AccessError::NotARegularFile { .. }
            | AccessError::NoClobberUnsupported { .. }
            | AccessError::Io { .. } => ErrorKind::Failed,
        };
        let hint = match error {
            AccessError::PathNotGranted { .. } => GRANT_HINT,
            _ => "",
        };
        let message = |error: &AccessError| format!("{}: {error}{hint}", error.code());
        let full = message(&error);
        if full.chars().count() <= Self::MAX_MESSAGE_CHARS {
            return Self::new(kind, full);
        }
        Self::new(kind, message(&shortened(error)))
    }
}

/// A message too long for [`OpError::MAX_MESSAGE_CHARS`] keeps this many
/// characters of each echoed path and list, so the reason and the grant hint
/// always fit.
const SHORT_CHARS: usize = 100;

/// `error` with its path, its lists and its I/O message cut to
/// [`SHORT_CHARS`] each.
fn shortened(mut error: AccessError) -> AccessError {
    match &mut error {
        AccessError::PathNotGranted { path, roots, .. } => {
            *path = shorten(path);
            shorten_list(roots);
        }
        AccessError::ExtensionNotAllowed { path, allowed } => {
            *path = shorten(path);
            shorten_list(allowed);
        }
        AccessError::Io { path, message } => {
            *path = shorten(path);
            *message = shorten(message);
        }
        AccessError::PathInvalid { path, .. }
        | AccessError::PathEscapesRoot { path }
        | AccessError::NotARegularFile { path }
        | AccessError::FileTooLarge { path, .. }
        | AccessError::FileExists { path }
        | AccessError::FileNotFound { path }
        | AccessError::OsDenied { path }
        | AccessError::NoClobberUnsupported { path } => *path = shorten(path),
    }
    error
}

/// `text` as at most [`SHORT_CHARS`] characters: its start and its end around
/// an ellipsis, so a path keeps its root and its file name.
fn shorten(text: &str) -> String {
    let count = text.chars().count();
    if count <= SHORT_CHARS {
        return text.to_owned();
    }
    let head = (SHORT_CHARS - 1) / 2;
    let tail = SHORT_CHARS - 1 - head;
    text.chars()
        .take(head)
        .chain(['…'])
        .chain(text.chars().skip(count.saturating_sub(tail)))
        .collect()
}

/// A list displayed joined by ", ", as one [`shorten`]ed entry when the
/// joined list is longer than [`SHORT_CHARS`].
fn shorten_list(list: &mut Vec<String>) {
    let joined = list.join(", ");
    if joined.chars().count() > SHORT_CHARS {
        *list = vec![shorten(&joined)];
    }
}
