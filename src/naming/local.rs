//! Native Rust OPSIN adapter, isolated in the bounded same-executable worker.
use super::{Provenance, Record, canonical_smiles, worker};
use std::{
    path::Path,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
const OUTPUT_LIMIT: usize = worker::OUTPUT_LIMIT;
const DEADLINE: Duration = Duration::from_secs(15);
const POLL: Duration = Duration::from_millis(25);
const LIMITS: reshiki_process::Limits = reshiki_process::Limits {
    address_bytes: 8 * 1024 * 1024 * 1024,
    memory_bytes: 384 * 1024 * 1024,
    cpu_seconds: 20,
    file_bytes: 8 * 1024 * 1024,
};
static SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
#[derive(Debug, Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);
impl Cancel {
    pub fn stop(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub(super) fn stopped(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
    pub(super) async fn cancelled(&self) {
        while !self.stopped() {
            tokio::time::sleep(POLL).await;
        }
    }
}
struct StopOnDrop(Cancel);
impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.stop();
    }
}

/// One deterministic local parse. Dropping the future cancels the supervised
/// worker, which kills and waits for its child before releasing its directory.
pub async fn resolve_name(name: &str, cancel: Cancel) -> Result<Record, String> {
    resolve_with(name, cancel, &SLOTS, run).await
}

async fn resolve_with(
    name: &str,
    cancel: Cancel,
    slots: &'static tokio::sync::Semaphore,
    run: impl FnOnce(&str, &Cancel) -> Result<Record, String> + Send + 'static,
) -> Result<Record, String> {
    if name.len() > 2_048 || name.chars().any(char::is_control) {
        return Err(
            "Enter a chemical name of at most 2048 bytes without control characters".into(),
        );
    }
    let name = name.trim();
    if name.is_empty() {
        return Err(
            "Enter a chemical name of at most 2048 bytes without control characters".into(),
        );
    }
    let stop = StopOnDrop(cancel.clone());
    let permit = tokio::select! {
        result = slots.acquire() => result.map_err(|_| "Local parser is unavailable")?,
        () = cancel.cancelled() => return Err("Local name parsing was cancelled".into()),
    };
    let name = name.to_owned();
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        run(&name, &cancel)
    })
    .await
    .map_err(|e| format!("Local parser worker failed: {e}"))?;
    drop(stop);
    result
}

fn run(name: &str, cancel: &Cancel) -> Result<Record, String> {
    run_with_executable(name, cancel, &worker::executable()?)
}
fn run_with_executable(name: &str, cancel: &Cancel, executable: &Path) -> Result<Record, String> {
    if cancel.stopped() {
        return Err("Local name parsing was cancelled".into());
    }
    let request = serde_json::to_vec(&worker::Request::Parse {
        protocol: worker::PROTOCOL,
        name: name.into(),
    })
    .map_err(|error| error.to_string())?;
    let directory = tempfile::Builder::new()
        .prefix("reshiki-native-name-")
        .tempdir()
        .map_err(|e| e.to_string())?;
    let mut command = Command::new(executable);
    command
        .arg("--naming-worker")
        .current_dir(directory.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = run_child(&mut command, &request, cancel, LIMITS)?;
    parse_reply(&output, name)
}
/// Shared local child supervision, independent of parser and naming-rule implementation.
pub(super) fn run_child(
    command: &mut Command,
    request: &[u8],
    cancel: &Cancel,
    limits: reshiki_process::Limits,
) -> Result<Vec<u8>, String> {
    if request.len() > worker_request_limit() {
        return Err("Local naming request exceeds its supported limit".into());
    }
    let mut child = reshiki_process::spawn(command, limits)
        .map_err(|e| format!("Could not start bounded local naming worker: {e}"))?;
    let started = Instant::now();
    let mut output = Vec::new();
    let mut errors = Vec::new();
    let mut output_closed = false;
    let mut errors_closed = false;
    let mut written = 0;
    let mut write_error = None;
    let mut status = None;
    let mut exited = None;
    let mut missing_samples = 0;
    loop {
        if cancel.stopped() {
            return Err("Local name parsing was cancelled".into());
        }
        if started.elapsed() > DEADLINE {
            return Err("Local name parsing reached its 15-second time limit".into());
        }
        if exited.is_some_and(|time: Instant| time.elapsed() > Duration::from_secs(1)) {
            return Err("Local worker exited without closing its response pipes; daemonizing launchers are unsupported".into());
        }
        if written < request.len() && write_error.is_none() {
            match child.write_input(
                request
                    .get(written..)
                    .ok_or("Invalid input write position")?,
            ) {
                Ok(0) => write_error = Some("Local worker stopped accepting input".to_owned()),
                Ok(count) => written += count,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => (),
                Err(e) => write_error = Some(e.to_string()),
            }
        }
        if written == request.len() || write_error.is_some() {
            child.close_input();
        }
        if !output_closed {
            output_closed = read_available(|bytes| child.read_output(bytes), &mut output)?;
        }
        if !errors_closed {
            errors_closed = read_available(|bytes| child.read_errors(bytes), &mut errors)?;
        }
        if status.is_none() {
            status = child.try_wait().map_err(|e| e.to_string())?;
            if status.is_some() {
                // Clean up the dedicated Unix group/Windows job even if the
                // leader exited. Pipe completion is still polled and bounded.
                child.terminate();
                exited = Some(Instant::now());
            } else {
                match child.check() {
                    Ok(()) => missing_samples = 0,
                    Err(error) => {
                        // Task info can disappear during exec/exit before
                        // the PID is waitable; Linux also omits VmRSS once
                        // the address space is gone. Permit four retries;
                        // measured excess and other failures stop
                        // even if the worker exits successfully immediately.
                        if !transient_measurement_error(&error) {
                            return Err(format!("Local parser resource check failed: {error}"));
                        }
                        if child.try_wait().map_err(|e| e.to_string())?.is_none() {
                            missing_samples += 1;
                            if missing_samples > 4 {
                                return Err(format!("Local parser resource check failed: {error}"));
                            }
                        }
                    }
                }
            }
        }
        if let Some(status) = status
            && output_closed
            && errors_closed
        {
            if !status.success() {
                let message = String::from_utf8_lossy(&errors);
                return Err(format!(
                    "The bounded local naming worker exited with {status}. {}",
                    message.chars().take(1024).collect::<String>()
                ));
            }
            if let Some(error) = write_error {
                return Err(error);
            }
            if written != request.len() {
                return Err("Local worker exited before receiving the complete request".into());
            }
            return Ok(output);
        }
        std::thread::sleep(POLL);
    }
}

fn transient_measurement_error(error: &std::io::Error) -> bool {
    matches!(error.raw_os_error(), Some(2 | 3))
}

const fn worker_request_limit() -> usize {
    32_768
}

/// One bounded nonblocking read per poll. No background thread can retain a
/// pipe after cancellation, and an escaped pipe holder cannot prolong joins.
fn read_available(
    mut read: impl FnMut(&mut [u8]) -> std::io::Result<usize>,
    bytes: &mut Vec<u8>,
) -> Result<bool, String> {
    let mut buffer = [0_u8; 8192];
    match read(&mut buffer) {
        Ok(0) => Ok(true),
        Ok(count) => {
            if bytes.len() + count > OUTPUT_LIMIT {
                return Err("Local parser output exceeded the supported limit".into());
            }
            bytes.extend_from_slice(buffer.get(..count).ok_or("Invalid output byte count")?);
            Ok(false)
        }
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(false),
        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => Ok(false),
        Err(e) => Err(e.to_string()),
    }
}

fn parse_reply(bytes: &[u8], name: &str) -> Result<Record, String> {
    if bytes.len() > OUTPUT_LIMIT {
        return Err("Local parser response exceeds the supported limit".into());
    }
    let response: worker::Response = serde_json::from_slice(bytes)
        .map_err(|_| "Local parser returned an invalid structured response")?;
    let worker::Response::Parse(reply) = response else {
        return Err("Local parser returned a result for a different operation".into());
    };
    if reply.protocol != worker::PROTOCOL
        || reply.backend != worker::BACKEND
        || reply.port_version != worker::PORT_VERSION
        || reply.upstream_version != worker::UPSTREAM_VERSION
        || reply.upstream_commit != worker::UPSTREAM_COMMIT
        || reply.resource_fingerprint != worker::RESOURCE_FINGERPRINT
        || reply.options != worker::OPTIONS
        || reply.name != name
    {
        return Err(
            "Local parser result has inconsistent version, options or input identity".into(),
        );
    }
    if !matches!(reply.status, worker::ParserStatus::Success) || !reply.warnings.is_empty() {
        let details = reply
            .warnings
            .iter()
            .take(16)
            .map(|w| format!("{}: {}", w.kind, w.message))
            .chain(std::iter::once(reply.message))
            .collect::<Vec<_>>()
            .join(" · ");
        return Err(format!(
            "The OPSIN Rust port could not interpret this name unambiguously with complete stereochemistry: {}",
            details.chars().take(2048).collect::<String>()
        ));
    }
    let smiles = reply
        .cxsmiles
        .ok_or("The OPSIN Rust port returned no molecular structure")?;
    if smiles.contains('|') {
        return Err("This name requires relative/racemic stereo, polymer or atom-label semantics unsupported by editable naming previews. No simplified structure was accepted".into());
    }
    if smiles.contains('.') {
        return Err("Local naming supports one connected molecule; disconnected salts and mixtures are unsupported".into());
    }
    let canonical_smiles = canonical_smiles(&smiles)?;
    Ok(Record {
        title: name.into(),
        systematic_name: None,
        smiles,
        canonical_smiles,
        synonyms: vec![],
        warnings: vec![],
        provenance: Provenance::OpsinRust,
    })
}

#[cfg(test)]
mod tests;
