//! Persistent, synchronous supervision of one bounded xTB worker.
//!
//! A session owns its process until it is closed or dropped. Cancellation and
//! transport failures destroy that session, including its process group/job,
//! before control returns to the caller.

use crate::protocol::{
    self, Closed, Compute, Computed, ErrorKind, Hello, Open, Opened, Outcome, Request, RequestOp,
    Response,
};
use serde::de::DeserializeOwned;
use std::{
    collections::VecDeque,
    io,
    marker::PhantomData,
    path::Path,
    process::{Command, ExitStatus},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const POLL: Duration = Duration::from_millis(5);
const STDERR_BYTES: usize = 4096;

/// Resource budgets for the process and deadlines for individual requests.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Overrides `Open::heap_bytes`, so the caller controls one heap policy.
    pub heap_bytes: u64,
    pub memory_bytes: u64,
    pub cpu_seconds: u64,
    /// Applied on Linux; the supervisor uses resident memory on macOS.
    pub address_bytes: u64,
    pub file_bytes: u64,
    pub hello: Duration,
    pub open: Duration,
    pub compute: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            heap_bytes: 2 * 1024 * 1024 * 1024,
            memory_bytes: 3 * 1024 * 1024 * 1024,
            cpu_seconds: 3600,
            address_bytes: 64 * 1024 * 1024 * 1024,
            file_bytes: 1024 * 1024,
            hello: Duration::from_secs(10),
            open: Duration::from_secs(30),
            compute: Duration::from_secs(600),
        }
    }
}

impl Limits {
    fn process(self) -> reshiki_process::Limits {
        reshiki_process::Limits {
            address_bytes: self.address_bytes,
            memory_bytes: self.memory_bytes,
            cpu_seconds: self.cpu_seconds,
            file_bytes: self.file_bytes,
        }
    }
}

/// A cancellation signal that can be shared with another thread.
#[derive(Debug, Default, Clone)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    fn cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Could not start bounded xTB worker: {0}")]
    Spawn(#[source] io::Error),
    #[error("xTB calculation was cancelled")]
    Cancelled,
    #[error("xTB worker reached its request deadline")]
    Timeout,
    #[error("xTB worker exited with {status}: {stderr}")]
    Exited { status: ExitStatus, stderr: String },
    #[error("xTB resource limit: {0}")]
    Limit(String),
    #[error("xTB input: {0}")]
    Input(String),
    #[error("xTB worker protocol error: {0}")]
    Protocol(String),
    #[error("xTB {kind}: {message}")]
    Engine { kind: ErrorKind, message: String },
    #[error("xTB is not included in this build")]
    Unavailable,
}

pub type Result<T> = std::result::Result<T, Error>;

/// One molecule and its warm-started engine in one supervised child process.
///
/// A session must stay on the thread that started it. On Linux, the worker
/// receives a parent-death signal when that thread exits.
///
/// ```compile_fail
/// fn require_send<T: Send>() {}
/// require_send::<reshiki_xtb_worker::client::Session>();
/// ```
pub struct Session {
    child: Option<reshiki_process::Child>,
    pid: u32,
    limits: Limits,
    next_id: u64,
    max_atoms: usize,
    atoms: Option<usize>,
    stderr: VecDeque<u8>,
    _thread: PhantomData<*const ()>,
}

impl Session {
    pub fn start(executable: &Path, limits: Limits) -> Result<(Self, Hello)> {
        if !executable.is_absolute() {
            return Err(Error::Spawn(io::Error::new(
                io::ErrorKind::InvalidInput,
                "xTB worker executable must be an absolute path",
            )));
        }
        if limits.heap_bytes == 0 {
            return Err(Error::Limit("Heap budget must be positive".into()));
        }
        let mut command = worker_command(executable);
        let child =
            reshiki_process::spawn_session(&mut command, limits.process()).map_err(Error::Spawn)?;
        let mut session = Self {
            pid: child.id(),
            child: Some(child),
            limits,
            next_id: 1,
            max_atoms: 0,
            atoms: None,
            stderr: VecDeque::with_capacity(STDERR_BYTES),
            _thread: PhantomData,
        };
        let hello: Hello =
            session.request(RequestOp::Hello {}, limits.hello, &Cancel::default(), false)?;
        if let Err(message) = validate_hello(&hello) {
            return Err(session.protocol_error(message));
        }
        session.max_atoms = hello.max_atoms;
        Ok((session, hello))
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// The session's heap policy replaces `open.heap_bytes` on the wire.
    pub fn open(&mut self, open: &Open, cancel: &Cancel) -> Result<Opened> {
        open.validate_geometry_and_threads(self.max_atoms)
            .map_err(|error| Error::Input(error.message))?;
        let mut request = open.clone();
        request.heap_bytes = self.limits.heap_bytes;
        let opened: Opened =
            self.request(RequestOp::Open(request), self.limits.open, cancel, false)?;
        if let Err(message) = validate_opened(&opened, open, self.max_atoms) {
            return Err(self.protocol_error(message));
        }
        self.atoms = Some(opened.atoms);
        Ok(opened)
    }

    pub fn compute(&mut self, compute: &Compute, cancel: &Cancel) -> Result<Computed> {
        if let Some(positions) = &compute.positions {
            protocol::validate_atom_count(positions.len(), self.max_atoms)
                .and_then(|()| {
                    protocol::validate_positions(positions, self.atoms.unwrap_or(positions.len()))
                })
                .map_err(|error| Error::Input(error.message))?;
        }
        let computed: Computed = self.request(
            RequestOp::Compute(compute.clone()),
            self.limits.compute,
            cancel,
            false,
        )?;
        if let Err(message) = validate_computed(&computed, self.atoms, compute.gradient) {
            return Err(self.protocol_error(message));
        }
        Ok(computed)
    }

    /// Close the worker and reap it. Closing uses the hello request deadline.
    pub fn close(mut self) -> Result<()> {
        let _: Closed = self.request(
            RequestOp::Close {},
            self.limits.hello,
            &Cancel::default(),
            true,
        )?;
        Ok(())
    }

    fn request<T: DeserializeOwned>(
        &mut self,
        op: RequestOp,
        deadline: Duration,
        cancel: &Cancel,
        closing: bool,
    ) -> Result<T> {
        let started = Instant::now();
        self.check_request(started, deadline, cancel)?;
        self.preflight()?;
        let id = self.next_id;
        self.next_id = match id.checked_add(1) {
            Some(next) => next,
            None => return Err(self.protocol_error("Request id space is exhausted".into())),
        };
        let frame = protocol::encode_frame(&Request { id, op })
            .map_err(|error| self.protocol_error(error.to_string()))?;
        let (body, ended) = self.exchange(&frame, deadline, started, cancel)?;
        let response: Response<T> = serde_json::from_slice(&body)
            .map_err(|error| self.protocol_error(format!("Invalid response JSON: {error}")))?;
        self.check_request(started, deadline, cancel)?;
        if response.id == 0 || response.id != id {
            return Err(self.protocol_error(format!(
                "Expected response id {id}, received {}",
                response.id
            )));
        }
        match response.result {
            Outcome::Ok { ok } => {
                if closing {
                    self.finish_close(started, deadline, cancel)?;
                } else if ended {
                    return Err(self.exited());
                }
                Ok(ok)
            }
            Outcome::Error { error } => {
                if ended || error.kind == ErrorKind::Protocol {
                    self.terminate();
                }
                Err(match error.kind {
                    ErrorKind::Protocol => Error::Protocol(error.message),
                    ErrorKind::Limit => Error::Limit(error.message),
                    ErrorKind::Unavailable => Error::Unavailable,
                    kind => Error::Engine {
                        kind,
                        message: error.message,
                    },
                })
            }
        }
    }

    /// A persistent worker must not send another frame between requests.
    fn preflight(&mut self) -> Result<()> {
        self.read_stderr()?;
        if self.status()?.is_some() {
            return Err(self.exited());
        }
        if self.no_more_output()? {
            return Err(self.exited());
        }
        self.check_resources()
    }

    fn exchange(
        &mut self,
        request: &[u8],
        deadline: Duration,
        started: Instant,
        cancel: &Cancel,
    ) -> Result<(Vec<u8>, bool)> {
        let mut written = 0;
        let mut output = Vec::new();
        let mut expected = None;
        let mut output_closed = false;
        loop {
            self.check_request(started, deadline, cancel)?;

            let mut progress = false;
            if written < request.len() {
                match self.child_mut()?.write_input(&request[written..]) {
                    Ok(0) => return Err(self.exited()),
                    Ok(count) => {
                        written += count;
                        progress = true;
                    }
                    Err(error) if retryable(&error) => (),
                    Err(_) => return Err(self.exited()),
                }
            }

            let mut buffer = [0_u8; 8192];
            if !output_closed {
                match self.read_stdout(&mut buffer)? {
                    Some(0) => output_closed = true,
                    Some(count) => {
                        append_output(&mut output, &mut expected, &buffer[..count])
                            .map_err(|message| self.protocol_error(message))?;
                        progress = true;
                    }
                    None => (),
                }
            }
            progress |= self.read_stderr()?;
            let status = self.status()?;
            if status.is_some_and(|status| status.code() == Some(75)) {
                return Err(self.exited());
            }
            if status.is_none() {
                self.check_resources()?;
            }

            let complete = expected.is_some_and(|length| output.len() == length);
            if complete && written != request.len() {
                return Err(
                    self.protocol_error("Response arrived before the complete request".into())
                );
            }
            if complete {
                // Check once more even when the first read ended exactly at a
                // frame boundary. A second response is never a future reply.
                if !output_closed {
                    output_closed = self.no_more_output()?;
                }
                return Ok((
                    output.split_off(protocol::HEADER_LEN),
                    output_closed || status.is_some(),
                ));
            } else if output_closed || (status.is_some() && !progress) {
                return Err(self.exited());
            }
            if !progress {
                std::thread::sleep(POLL.min(deadline.saturating_sub(started.elapsed())));
            }
        }
    }

    fn check_request(
        &mut self,
        started: Instant,
        deadline: Duration,
        cancel: &Cancel,
    ) -> Result<()> {
        let error = if cancel.cancelled() {
            Some(Error::Cancelled)
        } else if started.elapsed() >= deadline {
            Some(Error::Timeout)
        } else {
            None
        };
        if let Some(error) = error {
            self.terminate();
            return Err(error);
        }
        Ok(())
    }

    fn finish_close(
        &mut self,
        started: Instant,
        deadline: Duration,
        cancel: &Cancel,
    ) -> Result<()> {
        loop {
            self.check_request(started, deadline, cancel)?;
            self.no_more_output()?;
            self.read_stderr()?;
            if let Some(status) = self.status()? {
                return if status.success() {
                    self.no_more_output()?;
                    self.terminate();
                    Ok(())
                } else {
                    Err(self.exited())
                };
            }
            self.check_resources()?;
            std::thread::sleep(POLL.min(deadline.saturating_sub(started.elapsed())));
        }
    }

    fn child_mut(&mut self) -> Result<&mut reshiki_process::Child> {
        self.child
            .as_mut()
            .ok_or_else(|| Error::Protocol("The xTB session is closed".into()))
    }

    fn status(&mut self) -> Result<Option<ExitStatus>> {
        self.child_mut()?
            .try_wait()
            .map_err(|error| self.protocol_error(format!("Could not observe worker exit: {error}")))
    }

    fn check_resources(&mut self) -> Result<()> {
        let result = self.child_mut()?.check();
        if let Err(error) = result {
            // Exit/exec may temporarily remove the task or its RSS sample.
            // The request deadline bounds these missing samples; an actual
            // measured excess is never treated as transient.
            if !matches!(error.raw_os_error(), Some(2 | 3)) {
                self.terminate();
                return Err(Error::Limit(error.to_string()));
            }
        }
        Ok(())
    }

    fn read_stderr(&mut self) -> Result<bool> {
        let mut buffer = [0_u8; 8192];
        match self.child_mut()?.read_errors(&mut buffer) {
            Ok(count) => {
                remember_stderr(&mut self.stderr, &buffer[..count]);
                Ok(count != 0)
            }
            Err(error) if retryable(&error) => Ok(false),
            Err(error) => {
                Err(self.protocol_error(format!("Could not read worker diagnostics: {error}")))
            }
        }
    }

    fn read_stdout(&mut self, buffer: &mut [u8]) -> Result<Option<usize>> {
        match self.child_mut()?.read_output(buffer) {
            Ok(count) => Ok(Some(count)),
            Err(error) if retryable(&error) => Ok(None),
            Err(error) => Err(self.protocol_error(format!("Could not read response: {error}"))),
        }
    }

    /// Return EOF; any byte outside the expected frame is a protocol error.
    fn no_more_output(&mut self) -> Result<bool> {
        match self.read_stdout(&mut [0; 1])? {
            Some(0) => Ok(true),
            Some(_) => Err(self.protocol_error("Unsolicited or extra worker output".into())),
            None => Ok(false),
        }
    }

    fn protocol_error(&mut self, message: String) -> Error {
        self.terminate();
        Error::Protocol(message)
    }

    fn exited(&mut self) -> Error {
        // Windows uses 75 to terminate a job during our own cleanup. Only a
        // status observed before that cleanup identifies worker heap failure.
        let observed = if cfg!(windows) {
            self.child
                .as_mut()
                .and_then(|child| child.try_wait().ok().flatten())
        } else {
            None
        };
        match self.terminate() {
            Some(status) => classify_exit(status, observed, self.stderr_text()),
            None => Error::Protocol("Could not reap the xTB worker".into()),
        }
    }

    fn stderr_text(&mut self) -> String {
        String::from_utf8_lossy(self.stderr.make_contiguous()).into_owned()
    }

    /// The supervisor terminates the entire group/job and synchronously reaps.
    fn terminate(&mut self) -> Option<ExitStatus> {
        let mut child = self.child.take()?;
        child.terminate();
        let status = child.wait().ok();
        // Reaping closes all trusted writers. Drain the remaining pipe so the
        // diagnostic tail includes the last bytes written before the crash.
        let mut buffer = [0_u8; 8192];
        loop {
            match child.read_errors(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    remember_stderr(&mut self.stderr, &buffer[..count]);
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
        status
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.terminate();
    }
}

fn worker_command(executable: &Path) -> Command {
    let mut command = Command::new(executable);
    command
        .arg("--xtb-worker")
        .env_remove("AXTB_LAPACK")
        .env_remove("AXTB_LAPACK_BLAS");
    command
}

fn classify_exit(reaped: ExitStatus, observed: Option<ExitStatus>, stderr: String) -> Error {
    let natural_status = if cfg!(windows) {
        observed
    } else {
        Some(reaped)
    };
    if natural_status.is_some_and(|status| status.code() == Some(75)) {
        Error::Limit(if stderr.is_empty() {
            "Worker exhausted its heap budget (exit 75)".into()
        } else {
            format!("Worker exhausted its heap budget (exit 75): {stderr}")
        })
    } else {
        Error::Exited {
            status: reaped,
            stderr,
        }
    }
}

fn retryable(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
    )
}

fn remember_stderr(tail: &mut VecDeque<u8>, bytes: &[u8]) {
    for byte in bytes {
        if tail.len() == STDERR_BYTES {
            tail.pop_front();
        }
        tail.push_back(*byte);
    }
}

fn append_output(
    output: &mut Vec<u8>,
    expected: &mut Option<usize>,
    bytes: &[u8],
) -> std::result::Result<(), String> {
    output.extend_from_slice(bytes);
    if expected.is_none() && output.len() >= protocol::HEADER_LEN {
        let mut header = [0; protocol::HEADER_LEN];
        header.copy_from_slice(&output[..protocol::HEADER_LEN]);
        let length = protocol::decode_header(&header).map_err(|error| error.to_string())?;
        *expected = Some(protocol::HEADER_LEN + length);
    }
    if expected.is_some_and(|length| output.len() > length) {
        return Err("More than one response frame or trailing output".into());
    }
    Ok(())
}

fn validate_hello(hello: &Hello) -> std::result::Result<(), String> {
    if hello.protocol != protocol::VERSION {
        return Err("Worker advertised an incompatible protocol".into());
    }
    if hello.max_atoms == 0 || hello.max_atoms > protocol::MAX_ATOMS {
        return Err("Worker advertised an invalid atom limit".into());
    }
    let Some(engine) = &hello.engine else {
        return if hello.methods.is_empty() && hello.operations.is_empty() {
            Ok(())
        } else {
            Err("Unavailable worker advertised engine capabilities".into())
        };
    };
    if [
        &engine.name,
        &engine.version,
        &engine.license,
        &engine.source,
        &engine.rev,
    ]
    .iter()
    .any(|value| value.trim().is_empty())
    {
        return Err("Worker advertised incomplete engine metadata".into());
    }
    if hello.methods != [protocol::Method::Gfn2]
        || hello.operations.len() != 2
        || !hello.operations.contains(&protocol::Operation::Energy)
        || !hello.operations.contains(&protocol::Operation::Gradient)
    {
        return Err("Worker advertised inconsistent capabilities".into());
    }
    Ok(())
}

fn validate_opened(
    opened: &Opened,
    open: &Open,
    max_atoms: usize,
) -> std::result::Result<(), String> {
    if opened.atoms == 0 || opened.atoms > max_atoms || opened.atoms != open.numbers.len() {
        return Err("Worker returned an inconsistent atom count".into());
    }
    if opened.method != open.method || opened.numerics.trim().is_empty() {
        return Err("Worker returned inconsistent method metadata".into());
    }
    Ok(())
}

fn validate_computed(
    computed: &Computed,
    atoms: Option<usize>,
    gradient: bool,
) -> std::result::Result<(), String> {
    let atoms = atoms.ok_or_else(|| "Worker computed before opening a molecule".to_owned())?;
    if !computed.energy.is_finite() {
        return Err("Worker returned a non-finite energy".into());
    }
    if !computed.seconds.is_finite() || computed.seconds < 0.0 {
        return Err("Worker returned an invalid elapsed time".into());
    }
    match (gradient, &computed.gradient) {
        (true, Some(rows))
            if rows.len() == atoms && rows.iter().flatten().all(|x| x.is_finite()) =>
        {
            Ok(())
        }
        (true, _) => {
            Err("Worker returned a missing, non-finite or incorrectly sized gradient".into())
        }
        (false, None) => Ok(()),
        (false, Some(_)) => Err("Worker returned an unrequested gradient".into()),
    }
}

#[cfg(test)]
mod tests;
