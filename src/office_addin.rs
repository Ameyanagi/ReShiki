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
mod tests;
