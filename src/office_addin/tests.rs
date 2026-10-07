use super::*;

fn fixture() -> (tempfile::TempDir, std::path::PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("session.json"),
        br#"{"version":1,"sessionId":"00000000-0000-0000-0000-000000000001"}"#,
    )
    .unwrap();
    let path = directory.path().join("drawing.rsk");
    (directory, path)
}

#[test]
fn unacknowledged_save_keeps_recovery_and_returns_error() {
    let (_directory, path) = fixture();
    let error = save_with_timeout(&path, b"native", Duration::from_millis(10)).unwrap_err();
    assert!(error.contains("has not confirmed"));
    assert_eq!(std::fs::read(&path).unwrap(), b"native");
}

#[test]
fn stale_ack_does_not_accept_repeated_identical_native_bytes() {
    let (directory, path) = fixture();
    assert!(save_with_timeout(&path, b"native", Duration::ZERO).is_err());
    let mut previous: Receipt =
        serde_json::from_slice(&std::fs::read(directory.path().join("request.json")).unwrap())
            .unwrap();
    previous.accepted = true;
    std::fs::write(
        directory.path().join("ack.json"),
        serde_json::to_vec(&previous).unwrap(),
    )
    .unwrap();
    assert!(save_with_timeout(&path, b"native", Duration::from_millis(10)).is_err());
}

#[test]
fn only_matching_session_request_and_revision_accepts_save() {
    let (directory, path) = fixture();
    let receipt_dir = directory.path().to_owned();
    let responder = std::thread::spawn(move || {
        let request_path = receipt_dir.join("request.json");
        let deadline = Instant::now() + Duration::from_secs(2);
        while !request_path.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        let mut request: Receipt =
            serde_json::from_slice(&std::fs::read(request_path).unwrap()).unwrap();
        request.accepted = true;
        crate::storage::write_atomic(
            &receipt_dir.join("ack.json"),
            &serde_json::to_vec(&request).unwrap(),
        )
        .unwrap();
    });
    assert!(save_with_timeout(&path, b"native", Duration::from_secs(2)).is_ok());
    responder.join().unwrap();
}

#[test]
fn wrong_receipts_remain_pending_until_exact_acceptance() {
    let (directory, path) = fixture();
    let (finished_tx, finished_rx) = std::sync::mpsc::channel();
    let saving = std::thread::spawn(move || {
        let result = save_with_timeout(&path, b"native", Duration::from_secs(3));
        finished_tx.send(result).unwrap();
    });
    let request_path = directory.path().join("request.json");
    let deadline = Instant::now() + Duration::from_secs(2);
    while !request_path.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    let original = std::fs::read(request_path).unwrap();
    for wrong_field in ["session", "request", "revision", "accepted"] {
        let mut receipt: Receipt = serde_json::from_slice(&original).unwrap();
        receipt.accepted = true;
        match wrong_field {
            "session" => receipt.session_id = "00000000-0000-0000-0000-000000000002".into(),
            "request" => receipt.request_id.push_str("-wrong"),
            "revision" => receipt.revision = "0".repeat(64),
            _ => receipt.accepted = false,
        }
        crate::storage::write_atomic(
            &directory.path().join("ack.json"),
            &serde_json::to_vec(&receipt).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            finished_rx.recv_timeout(Duration::from_millis(150)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        ));
    }
    let mut receipt: Receipt = serde_json::from_slice(&original).unwrap();
    receipt.accepted = true;
    crate::storage::write_atomic(
        &directory.path().join("ack.json"),
        &serde_json::to_vec(&receipt).unwrap(),
    )
    .unwrap();
    assert!(
        finished_rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .is_ok()
    );
    saving.join().unwrap();
}

#[test]
fn finished_office_session_keeps_late_native_draft_without_an_update_request() {
    let (directory, path) = fixture();
    std::fs::write(
        directory.path().join("session.json"),
        br#"{"version":1,"sessionId":"00000000-0000-0000-0000-000000000001","closed":true}"#,
    )
    .unwrap();
    let error = save_with_timeout(&path, b"late edit", Duration::ZERO).unwrap_err();
    assert!(error.contains("session has ended"));
    assert_eq!(std::fs::read(path).unwrap(), b"late edit");
    assert!(!directory.path().join("request.json").exists());
}
