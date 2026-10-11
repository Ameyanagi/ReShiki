//! Setup failures are classified at their source, never by matching provider text.
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionError {
    MissingInstallation,
    InvalidExecutable,
    StartFailed,
    HandshakeFailed,
    AccountFailed,
    ModelsFailed,
    NoModels,
    Cancelled,
}

impl std::fmt::Display for ConnectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::MissingInstallation => "Codex could not be found. Install the Codex CLI, restart ReShiki, then test the connection.",
            Self::InvalidExecutable => "RESHIKI_CODEX must point to an existing absolute Codex executable. Correct the path in the environment that launches ReShiki, then restart and retry.",
            Self::StartFailed => "Codex was found but could not start. Check `codex --version` in a terminal, update or reinstall Codex, then retry.",
            Self::HandshakeFailed => "Codex started but the connection did not complete. Check `codex --version`, update Codex if needed, then retry. You can keep drawing.",
            Self::AccountFailed => "Codex could not read the saved sign-in. Check `codex login status` in a terminal, sign in again if needed, then retry.",
            Self::ModelsFailed => "Codex could not load its model catalog. Check your internet connection and Codex account access, then retry.",
            Self::NoModels => "Codex returned no selectable models. Check your account access, update Codex if needed, then retry.",
            Self::Cancelled => "Connection check cancelled. You can keep drawing and retry when ready.",
        })
    }
}

impl std::error::Error for ConnectionError {}

impl From<ConnectionError> for String {
    fn from(error: ConnectionError) -> Self {
        error.to_string()
    }
}

pub(super) fn find_executable(
    override_path: Option<&Path>,
    candidates: impl IntoIterator<Item = PathBuf>,
) -> Result<PathBuf, ConnectionError> {
    if let Some(path) = override_path {
        return if path.is_absolute() && path.is_file() {
            Ok(path.to_path_buf())
        } else {
            Err(ConnectionError::InvalidExecutable)
        };
    }
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or(ConnectionError::MissingInstallation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executable_discovery_distinguishes_missing_and_invalid_override_on_all_platforms() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            find_executable(None, vec![]),
            Err(ConnectionError::MissingInstallation)
        );
        for name in ["codex", "codex.exe"] {
            let path = dir.path().join(name);
            std::fs::write(&path, b"fixture").unwrap();
            assert_eq!(find_executable(None, [path.clone()]), Ok(path.clone()));
            assert_eq!(find_executable(Some(&path), []), Ok(path));
        }
        for invalid in [
            PathBuf::from("relative/codex"),
            dir.path().join("absent"),
            dir.path().to_path_buf(),
        ] {
            assert_eq!(
                find_executable(Some(&invalid), []),
                Err(ConnectionError::InvalidExecutable)
            );
        }
    }

    #[test]
    fn setup_failures_have_specific_safe_actions() {
        for failure in [
            ConnectionError::MissingInstallation,
            ConnectionError::InvalidExecutable,
            ConnectionError::StartFailed,
            ConnectionError::HandshakeFailed,
            ConnectionError::AccountFailed,
            ConnectionError::ModelsFailed,
            ConnectionError::NoModels,
            ConnectionError::Cancelled,
        ] {
            let message = failure.to_string();
            assert!(message.contains("Codex") || message.contains("Connection"));
            assert!(
                message.contains("retry") || message.contains("test") || message.contains("then")
            );
        }
    }
}
