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
        assert!(message.contains("retry") || message.contains("test") || message.contains("then"));
    }
}
