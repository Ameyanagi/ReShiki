//! The optional Microsoft 365 task pane acknowledges the exact native save only
//! after Office reads back the intended embedded object. Local persistence alone
//! must never tell the user that their Office document has been updated.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

static REQUEST_SERIAL: AtomicU64 = AtomicU64::new(0);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Session {
    version: u32,
    session_id: String,
    #[serde(default)]
    closed: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Receipt {
    version: u32,
    session_id: String,
    request_id: String,
    revision: String,
    #[serde(default)]
    accepted: bool,
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 4096 {
        return Err("Invalid Microsoft 365 edit-session metadata".into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|error| error.to_string())?
        .take(4097)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > 4096 {
        return Err("Microsoft 365 edit-session metadata exceeds limit".into());
    }
    Ok(bytes)
}

/// Keep the draft even on timeout. An error leaves the native document dirty
/// and cancels a pending close, allowing another save or an explicit Save As.
pub fn save(path: &Path, bytes: &[u8]) -> Result<(), String> {
    save_with_timeout(path, bytes, Duration::from_secs(20))
}

fn save_with_timeout(path: &Path, bytes: &[u8], timeout: Duration) -> Result<(), String> {
    let directory = path
        .parent()
        .ok_or("Microsoft 365 recovery directory is missing")?;
    let session: Session = serde_json::from_slice(&read_bounded(&directory.join("session.json"))?)
        .map_err(|error| format!("Invalid Microsoft 365 session: {error}"))?;
    if session.version != 1
        || session.session_id.len() != 36
        || !session
            .session_id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
    {
        return Err("Invalid Microsoft 365 edit-session identity".into());
    }
    if session.closed {
        crate::storage::write_atomic(path, bytes)?;
        return Err(format!(
            "This Microsoft 365 edit session has ended. Draft saved at {}. Use Save As to keep a separate drawing, or reopen the drawing from the Office task pane.",
            path.display()
        ));
    }
    let request_id = format!(
        "{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos(),
        REQUEST_SERIAL.fetch_add(1, Ordering::Relaxed)
    );
    let request = Receipt {
        version: 1,
        session_id: session.session_id,
        request_id,
        revision: format!("{:x}", Sha256::digest(bytes)),
        accepted: false,
    };
    crate::storage::write_atomic(path, bytes)?;
    crate::storage::write_atomic(
        &directory.join("request.json"),
        &serde_json::to_vec(&request).map_err(|error| error.to_string())?,
    )?;
    let started = Instant::now();
    while started.elapsed() < timeout {
        if let Ok(bytes) = read_bounded(&directory.join("ack.json"))
            && let Ok(ack) = serde_json::from_slice::<Receipt>(&bytes)
            && ack.version == 1
            && ack.accepted
            && ack.session_id == request.session_id
            && ack.request_id == request.request_id
            && ack.revision == request.revision
        {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(format!(
        "Draft saved at {}. Microsoft 365 has not confirmed the update. Keep the task pane open and check its status; Save As can keep a separate drawing file.",
        path.display()
    ))
}

#[cfg(test)]
mod tests {
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
}
