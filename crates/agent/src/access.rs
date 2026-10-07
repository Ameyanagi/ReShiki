//! Capability-based filesystem grants: the single filesystem authority for
//! the agent API's file tools.
//!
//! # Model
//!
//! - Roots are opened once, at startup, from their canonical path
//!   ([`Grants::open`]). The display spelling the user gave is kept for
//!   matching and messages; the handle always comes from the canonical path.
//!   On Unix the opened handle's (device, inode) must match the canonical
//!   path after opening; otherwise the grant fails with
//!   [`GrantError::Changed`].
//! - Read and write roots are separate. A write root never implies read.
//! - Every request is a plain absolute path. It is checked lexically first
//!   (no `..`, no URIs, no Windows verbatim, UNC, device or alternate data
//!   stream syntax), then matched to the root with the longest whole-component
//!   prefix, and the remainder is resolved relative to that root's handle.
//!   cap-std rejects `..`, absolute paths and symlinks that leave the root
//!   during that resolution (<https://github.com/bytecodealliance/cap-std>).
//! - Writes go to a temporary file beside the destination. A new file is
//!   published with a no-clobber hard link; a replacement with a rename. The
//!   destination name is never removed, truncated or renamed on a failure
//!   path, and directories are never created.
//!
//! # Residual risks
//!
//! - Hard links: a file inside a root may be a hard link to a file outside
//!   it. Reads follow it, and nothing short of refusing every multiply-linked
//!   file can tell; this is accepted and documented.
//! - Same-user processes: another process running as the same user can swap
//!   directories, including while roots are being acquired at startup. The
//!   post-open checks narrow that window; they cannot close it.
//! - Cloud placeholders (OneDrive, iCloud Drive and similar) may download
//!   their contents when read.
//! - Windows: cap-std offers no handle identity check, so only the
//!   canonical-path recheck runs there.
//! - On a file system without hard links (FAT, exFAT), creating a new file
//!   fails with `no_clobber_unsupported`; replacing still works.
//! - Matching is exact (Unix) or ASCII-case-insensitive (Windows). Other
//!   spellings of a root, such as a different Unicode normalization or case
//!   on a case-insensitive volume, fail closed as `path_not_granted`.
//! - An interrupted publication can leave a `.reshiki-<pid>-<n>.tmp` file
//!   beside a complete destination.
//!
//! MCP roots are never consulted: they are deprecated by SEP-2577 and are
//! informational guidance rather than an access-control mechanism
//! (<https://modelcontextprotocol.io/specification/2026-07-28/client/roots>).
use std::{fmt, io};

mod path;
mod read;
mod root;
mod write;

pub use root::{GRANT_EXIT_CODE, GrantError, GrantSummary, Grants};
pub use write::{WriteMode, WriteReceipt};

/// Requested paths are echoed in messages up to this many characters.
const ECHO_CHARS: usize = 512;

/// Why a file request was refused. Messages never include file contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessError {
    /// The path failed a lexical rule; `rule` names which one.
    PathInvalid {
        path: String,
        rule: &'static str,
    },
    /// No root of the requested kind contains the path.
    PathNotGranted {
        path: String,
        access: &'static str,
        roots: Vec<String>,
    },
    /// Resolution left the granted folder, or the operating system refused it.
    PathEscapesRoot {
        path: String,
    },
    NotARegularFile {
        path: String,
    },
    FileTooLarge {
        path: String,
        limit: usize,
    },
    FileExists {
        path: String,
    },
    FileNotFound {
        path: String,
    },
    ExtensionNotAllowed {
        path: String,
        allowed: Vec<String>,
    },
    OsDenied {
        path: String,
    },
    /// The folder's file system cannot publish a new file without overwriting.
    NoClobberUnsupported {
        path: String,
    },
    Io {
        path: String,
        message: String,
    },
}

impl AccessError {
    /// A stable code for clients and tests.
    pub fn code(&self) -> &'static str {
        match self {
            Self::PathInvalid { .. } => "path_invalid",
            Self::PathNotGranted { .. } => "path_not_granted",
            Self::PathEscapesRoot { .. } => "path_escapes_root",
            Self::NotARegularFile { .. } => "not_a_regular_file",
            Self::FileTooLarge { .. } => "file_too_large",
            Self::FileExists { .. } => "file_exists",
            Self::FileNotFound { .. } => "file_not_found",
            Self::ExtensionNotAllowed { .. } => "extension_not_allowed",
            Self::OsDenied { .. } => "os_denied",
            Self::NoClobberUnsupported { .. } => "no_clobber_unsupported",
            Self::Io { .. } => "io_error",
        }
    }
}

impl fmt::Display for AccessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PathInvalid { path, rule } => {
                write!(f, "invalid path \"{path}\": {}", rule_message(rule))
            }
            Self::PathNotGranted {
                path,
                access,
                roots,
            } => {
                write!(
                    f,
                    "\"{path}\" is not inside a folder granted for {access}; "
                )?;
                if roots.is_empty() {
                    write!(f, "no folders are granted for {access}")
                } else {
                    write!(f, "granted folders: {}", roots.join(", "))
                }
            }
            Self::PathEscapesRoot { path } => write!(
                f,
                "\"{path}\" is outside the granted folder or denied by the operating system"
            ),
            Self::NotARegularFile { path } => write!(f, "\"{path}\" is not a regular file"),
            Self::FileTooLarge { path, limit } => {
                write!(f, "\"{path}\" is larger than the {limit}-byte limit")
            }
            Self::FileExists { path } => write!(f, "\"{path}\" already exists"),
            Self::FileNotFound { path } => write!(f, "\"{path}\" does not exist"),
            Self::ExtensionNotAllowed { path, allowed } => write!(
                f,
                "\"{path}\" does not have an allowed extension ({})",
                allowed.join(", ")
            ),
            Self::OsDenied { path } => {
                write!(f, "the operating system denied access to \"{path}\"")
            }
            Self::NoClobberUnsupported { path } => write!(
                f,
                "\"{path}\" cannot be created without risking an overwrite because \
                 the folder's file system has no hard links; overwrite=true replaces instead"
            ),
            Self::Io { path, message } => write!(f, "I/O error on \"{path}\": {message}"),
        }
    }
}

impl std::error::Error for AccessError {}

fn rule_message(rule: &str) -> &'static str {
    match rule {
        "empty" => "the path is empty",
        "too_long" => "the path is longer than 4096 bytes",
        "nul" => "the path contains a NUL character",
        "file_uri" => "file: URIs are not accepted; pass a plain absolute path",
        "relative" => "the path must be absolute",
        "dotdot" => "'..' components are not allowed",
        "trailing_separator" => "the path must name a file, without a trailing separator",
        "verbatim" => "\\\\?\\ paths are not accepted",
        "unc" => "network (UNC) paths are not accepted",
        "device" => "device paths are not accepted",
        "drive_relative" => "drive-relative paths such as C:file are not accepted",
        "rooted_without_drive" => "the path must start with a drive letter",
        "ads" => "':' is only allowed after the drive letter",
        "reserved_name" => "the path contains a reserved device name",
        "trailing_dot_space" => "path components must not end with '.' or ' '",
        "leading_dot" => "file names must not start with '.'",
        "name_too_long" => "the file name is longer than 255 bytes",
        "control_character" => "the file name contains a control character",
        "reserved_character" => "the file name contains one of <>:\"|?*",
        _ => "the path is not accepted",
    }
}

/// The requested path as echoed in messages: at most [`ECHO_CHARS`] characters.
fn echo(path: &str) -> String {
    match path.char_indices().nth(ECHO_CHARS) {
        Some(_) => {
            let mut short: String = path.chars().take(ECHO_CHARS - 1).collect();
            short.push('…');
            short
        }
        None => path.to_owned(),
    }
}

fn invalid(path: &str, rule: &'static str) -> AccessError {
    AccessError::PathInvalid {
        path: echo(path),
        rule,
    }
}

/// Map an error from resolving a requested path under a root handle.
///
/// cap-std reports its own refusals (`..`, absolute or escaping symlinks) as
/// PermissionDenied without an OS error code; the operating system's own
/// denials carry one.
fn resolution_error(error: io::Error, path: &str) -> AccessError {
    let path = path.to_owned();
    match error.kind() {
        io::ErrorKind::NotFound | io::ErrorKind::NotADirectory => {
            AccessError::FileNotFound { path }
        }
        io::ErrorKind::IsADirectory => AccessError::NotARegularFile { path },
        io::ErrorKind::PermissionDenied if error.raw_os_error().is_none() => {
            AccessError::PathEscapesRoot { path }
        }
        io::ErrorKind::PermissionDenied => AccessError::OsDenied { path },
        _ => AccessError::Io {
            path,
            message: error.to_string(),
        },
    }
}

/// Map an error from I/O on an already opened file.
fn io_error(error: io::Error, path: &str) -> AccessError {
    let path = path.to_owned();
    match error.kind() {
        io::ErrorKind::PermissionDenied => AccessError::OsDenied { path },
        _ => AccessError::Io {
            path,
            message: error.to_string(),
        },
    }
}

#[cfg(test)]
mod tests;
