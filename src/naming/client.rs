//! Shared scheduling and supervision for bounded native naming workers.
use super::{local::Cancel, worker};
use std::{
    ffi::OsStr,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const DEADLINE: Duration = Duration::from_secs(15);
pub(super) const POLL: Duration = Duration::from_millis(25);
const WORKER_LIMITS: reshiki_process::Limits = reshiki_process::Limits {
    address_bytes: 8 * 1024 * 1024 * 1024,
    memory_bytes: 384 * 1024 * 1024,
    cpu_seconds: 20,
    file_bytes: 8 * 1024 * 1024,
};

pub(super) struct CancelOnDrop(pub(super) Cancel);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.stop();
    }
}

pub(super) async fn on_slot<T: Send + 'static>(
    input: &str,
    cancel: &Cancel,
    slots: &'static tokio::sync::Semaphore,
    unavailable: &'static str,
    cancelled: &'static str,
    failed: &'static str,
    run: impl FnOnce(&str, &Cancel) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let permit = tokio::select! {
        result = slots.acquire() => result.map_err(|_| unavailable)?,
        () = cancel.cancelled() => return Err(cancelled.into()),
    };
    let input = input.to_owned();
    let cancel = cancel.clone();
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        run(&input, &cancel)
    })
    .await
    .map_err(|error| format!("{failed}: {error}"))?
}

pub(super) fn run_worker<P: AsRef<OsStr>>(
    prefix: &str,
    request: &[u8],
    cancel: &Cancel,
    executable: impl FnOnce() -> Result<P, String>,
) -> Result<Vec<u8>, String> {
    let directory = tempfile::Builder::new()
        .prefix(prefix)
        .tempdir()
        .map_err(|error| error.to_string())?;
    let mut command = Command::new(executable()?);
    command
        .arg(crate::workers::NAMING_WORKER)
        .current_dir(directory.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    run_child(&mut command, request, cancel)
}

/// Shared local child supervision, independent of parser and naming-rule implementation.
// Preserve the parser wording of supervision errors for both local and reverse.
fn run_child(command: &mut Command, request: &[u8], cancel: &Cancel) -> Result<Vec<u8>, String> {
    if request.len() > worker::WIRE_LIMIT {
        return Err("Local naming request exceeds its supported limit".into());
    }
    let mut child = reshiki_process::spawn(command, WORKER_LIMITS)
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
            let error = "Local worker exited without closing its response pipes; daemonizing launchers are unsupported";
            return Err(error.into());
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

pub(super) fn transient_measurement_error(error: &std::io::Error) -> bool {
    matches!(error.raw_os_error(), Some(2 | 3))
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
            if bytes.len() + count > worker::OUTPUT_LIMIT {
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
