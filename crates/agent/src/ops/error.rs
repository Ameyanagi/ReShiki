//! Operation errors and their stable wire codes.
use crate::{access::AccessError, transaction::Rejection};
use serde_json::{Value, json};
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
    /// Every operation error kind, in declaration order.
    pub const ALL: &[Self] = &[
        Self::UnknownTool,
        Self::InvalidArguments,
        Self::UnknownDocument,
        Self::UnknownObject,
        Self::Stale,
        Self::Busy,
        Self::Budget,
        Self::Timeout,
        Self::Rejected,
        Self::Unsupported,
        Self::Access,
        Self::Failed,
        Self::Cancelled,
    ];

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

    /// What the code means, for reference documentation. The match has no
    /// wildcard, so a new kind needs a meaning before it compiles.
    pub fn meaning(self) -> &'static str {
        match self {
            Self::UnknownTool => "No tool has this name; MCP reports it as a JSON-RPC error.",
            Self::InvalidArguments => "The arguments break the tool's schema or one of its rules.",
            Self::UnknownDocument => {
                "The document handle is unknown, closed, expired or not the caller's."
            }
            Self::UnknownObject => "An object ID is not in the document.",
            Self::Stale => {
                "The document's revision is not the one the call was based on; nothing changed."
            }
            Self::Busy => {
                "The server is stopping, the request ID is in flight or the queue is full."
            }
            Self::Budget => "The call would exceed a budget.",
            Self::Timeout => "The call passed its deadline before it committed any change.",
            Self::Rejected => "Validation or the document's edit policy refused the change.",
            Self::Unsupported => {
                "The host cannot do this, such as chemical naming in a host without it."
            }
            Self::Access => {
                "The file is outside the granted folders, has an extension the tool does not \
                 accept, or the operating system denied access."
            }
            Self::Failed => {
                "The operation failed, for example on unreadable chemistry, a missing or \
                 existing file, an I/O error or an internal error."
            }
            Self::Cancelled => "The caller cancelled the request; no response is sent.",
        }
    }

    /// Whether the same call can succeed when sent again unchanged: only
    /// [`ErrorKind::Busy`] and [`ErrorKind::Timeout`]. A stale call needs a
    /// new base revision first.
    pub fn retryable(self) -> bool {
        matches!(self, Self::Busy | Self::Timeout)
    }

    /// True only for [`ErrorKind::UnknownTool`].
    pub fn is_protocol_error(self) -> bool {
        matches!(self, Self::UnknownTool)
    }
}

/// An operation error with a bounded, client-facing message.
///
/// On the wire it is `{code, message, retryable}` plus `reason`, `param` and
/// `current_revision` when they are known, so a client never has to read
/// them out of the message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpError {
    pub kind: ErrorKind,
    pub message: String,
    /// The specific reason under the code. File errors carry their
    /// [`AccessError::code`], such as `path_not_granted` or `file_exists`.
    pub reason: Option<&'static str>,
    /// The argument at fault, when one argument is.
    pub param: Option<&'static str>,
    /// The document's revision, for [`ErrorKind::Stale`].
    pub current_revision: Option<u64>,
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
            reason: None,
            param: None,
            current_revision: None,
        }
    }

    /// Names the argument at fault.
    pub fn param(self, param: &'static str) -> Self {
        Self {
            param: Some(param),
            ..self
        }
    }

    /// Reports the revision the document is at now, for a stale call.
    pub fn at_revision(self, current_revision: u64) -> Self {
        Self {
            current_revision: Some(current_revision),
            ..self
        }
    }

    /// The wire form: `{code, message, retryable}`, then `reason`, `param`
    /// and `current_revision` (a decimal string) when set.
    pub fn json(&self) -> Value {
        let mut error = json!({
            "code": self.kind.code(),
            "message": self.message,
            "retryable": self.kind.retryable(),
        });
        if let Some(fields) = error.as_object_mut() {
            if let Some(reason) = self.reason {
                fields.insert("reason".to_owned(), reason.into());
            }
            if let Some(param) = self.param {
                fields.insert("param".to_owned(), param.into());
            }
            if let Some(revision) = self.current_revision {
                fields.insert("current_revision".to_owned(), revision.to_string().into());
            }
        }
        error
    }

    /// A failed operation with a bounded, client-facing message.
    pub fn failed(message: impl AsRef<str>) -> Self {
        Self::new(ErrorKind::Failed, message)
    }

    /// Invalid arguments with a bounded, client-facing message.
    pub fn invalid(message: impl AsRef<str>) -> Self {
        Self::new(ErrorKind::InvalidArguments, message)
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

/// The access code is the error's `reason`. The message is still
/// `"{access code}: {message}"`, so it reads the same in a terminal;
/// `path_not_granted` adds how folders are granted. When that would be cut at
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
        let reason = Some(error.code());
        let param = Some("path");
        let message = |error: &AccessError| format!("{}: {error}{hint}", error.code());
        let full = message(&error);
        let message = if full.chars().count() <= Self::MAX_MESSAGE_CHARS {
            full
        } else {
            message(&shortened(error))
        };
        Self {
            reason,
            param,
            ..Self::new(kind, message)
        }
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

#[cfg(test)]
mod tests;
