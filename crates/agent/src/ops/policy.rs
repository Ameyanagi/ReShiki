//! Who may read or edit which document.
use super::error::{ErrorKind, OpError};
use serde::Serialize;

/// The message for an unknown, expired or foreign session handle. It is the
/// same in every case, so a handle's existence never leaks to another
/// principal.
pub const UNKNOWN_DOCUMENT: &str = "Unknown or expired document handle; create a new document";

/// What a document handle names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HandleKind {
    /// A document created through the operation API, owned by one principal.
    Session,
    /// An open app tab.
    Live,
}

/// What an operation needs from a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Access {
    Read,
    Edit,
}

/// How a mutation took effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    /// The document changed.
    Applied,
    /// The change waits for the user's review.
    Proposed,
}

/// Whether `access` to a handle of `kind` is allowed.
///
/// - A session handle that the caller does not own is [`ErrorKind::UnknownDocument`],
///   exactly as if it did not exist.
/// - The owner may read and edit a session document.
/// - Reading a live document is allowed; editing one is [`ErrorKind::Rejected`].
pub fn check(kind: HandleKind, access: Access, owner_matches: bool) -> Result<(), OpError> {
    match (kind, access) {
        (HandleKind::Session, _) if !owner_matches => {
            Err(OpError::new(ErrorKind::UnknownDocument, UNKNOWN_DOCUMENT))
        }
        (HandleKind::Session, _) | (HandleKind::Live, Access::Read) => Ok(()),
        (HandleKind::Live, Access::Edit) => Err(OpError::new(
            ErrorKind::Rejected,
            "live documents are read-only in this version",
        )),
    }
}

#[cfg(test)]
mod tests;
