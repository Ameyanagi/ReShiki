//! A cancellation-safe request becomes a persistent owner only after its ACK.
use super::{CommandRequest, JSON_LIMIT, Packet, Representation};
use serde::Deserialize;
use std::{process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, Command},
};

struct PendingChild {
    child: Child,
    armed: bool,
}
impl Drop for PendingChild {
    fn drop(&mut self) {
        if self.armed {
            let _ = self.child.start_kill();
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Response {
    Success(Packet),
    Failure { error: String },
}

pub(super) async fn invoke(
    operation: &str,
    representations: &[Representation],
) -> Result<Packet, String> {
    let input = serde_json::to_vec(&CommandRequest {
        operation,
        representations,
    })
    .map_err(|e| e.to_string())?;
    if input.len() > JSON_LIMIT {
        return Err("Clipboard request is too large".into());
    }
    let executable =
        std::env::current_exe().map_err(|e| format!("Could not locate the application: {e}"))?;
    let child = Command::new(executable)
        .arg("--clipboard-worker")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Could not start Linux clipboard worker: {e}"))?;
    // Tokio's kill_on_drop cannot be disarmed. This guard retains the same
    // cancellation behavior until a completed write can intentionally detach.
    let mut pending = PendingChild { child, armed: true };
    let mut stdin = pending
        .child
        .stdin
        .take()
        .ok_or("Clipboard input is unavailable")?;
    let stdout = pending
        .child
        .stdout
        .take()
        .ok_or("Clipboard output is unavailable")?;
    let request = async {
        stdin.write_all(&input).await.map_err(|e| e.to_string())?;
        stdin.shutdown().await.map_err(|e| e.to_string())?;
        drop(stdin);
        let mut output = Vec::new();
        BufReader::new(stdout.take(JSON_LIMIT as u64 + 1))
            .read_until(b'\n', &mut output)
            .await
            .map_err(|e| e.to_string())?;
        if output.len() > JSON_LIMIT || output.last() != Some(&b'\n') {
            return Err("Invalid or oversized Linux clipboard response".into());
        }
        let packet = match serde_json::from_slice::<Response>(&output)
            .map_err(|_| "Invalid Linux clipboard response")?
        {
            Response::Success(packet) => packet,
            Response::Failure { error } => return Err(error.chars().take(1000).collect()),
        };
        if operation == "write" {
            if !packet.representations.is_empty() {
                return Err("Invalid clipboard write acknowledgement".into());
            }
            // No await between disarming and returning: cancellation before the
            // ACK kills the worker; after it, the clipboard remains available.
            pending.armed = false;
        } else if !pending
            .child
            .wait()
            .await
            .map_err(|e| e.to_string())?
            .success()
        {
            return Err("Linux clipboard read failed".into());
        }
        Ok(packet)
    };
    // Reads may make steady INCR progress beyond the write acknowledgement
    // timeout. Allow the worker's total transfer budget plus startup/response time.
    let timeout = if operation == "write" {
        Duration::from_secs(10)
    } else {
        reshiki_linux::TRANSFER_TOTAL_TIMEOUT + Duration::from_secs(5)
    };
    tokio::time::timeout(timeout, request)
        .await
        .map_err(|_| "Clipboard operation timed out".to_owned())?
}
