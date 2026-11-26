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
    collections::{BTreeMap, VecDeque},
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

    /// Whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled()
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
    #[error("xTB unsupported: {0}")]
    Unsupported(String),
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
    max_hessian_atoms: usize,
    opened: Option<Opened>,
    open: Option<Open>,
    geometry: Option<(Vec<[f64; 3]>, protocol::Units)>,
    gfnff: Option<(protocol::GfnFfSettings, Vec<u32>)>,
    stderr: VecDeque<u8>,
    _thread: PhantomData<*const ()>,
}

impl Session {
    /// Start the worker and complete its cancellable hello handshake.
    /// An already cancelled token prevents the worker from being spawned.
    pub fn start(executable: &Path, limits: Limits, cancel: &Cancel) -> Result<(Self, Hello)> {
        if cancel.cancelled() {
            return Err(Error::Cancelled);
        }
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
            max_hessian_atoms: 0,
            opened: None,
            open: None,
            geometry: None,
            gfnff: None,
            stderr: VecDeque::with_capacity(STDERR_BYTES),
            _thread: PhantomData,
        };
        let hello: Hello = session.request(RequestOp::Hello {}, limits.hello, cancel, false)?;
        if let Err(message) = validate_hello(&hello) {
            return Err(session.protocol_error(message));
        }
        session.max_atoms = hello.max_atoms;
        session.max_hessian_atoms = hello.max_hessian_atoms;
        Ok((session, hello))
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// The session's heap policy replaces `open.heap_bytes` on the wire.
    pub fn open(&mut self, open: &Open, cancel: &Cancel) -> Result<Opened> {
        self.check_cancelled(cancel)?;
        open.admit(None, None, self.max_atoms)
            .map_err(admission_error)?;
        let mut request = open.clone();
        request.heap_bytes = self.limits.heap_bytes;
        let opened: Opened =
            self.request(RequestOp::Open(request), self.limits.open, cancel, false)?;
        if let Err(message) = validate_opened(&opened, open, self.max_atoms) {
            return Err(self.protocol_error(message));
        }
        self.opened = Some(opened.clone());
        self.open = Some(open.clone());
        self.geometry = Some((open.positions.clone(), open.units));
        self.gfnff = None;
        Ok(opened)
    }

    pub fn compute(&mut self, compute: &Compute, cancel: &Cancel) -> Result<Computed> {
        self.check_cancelled(cancel)?;
        let open = self.open.clone();
        let opened = self.opened.clone();
        let frame = if let Some(open) = &open {
            if compute.hessian.is_some() && open.numbers.len() > self.max_hessian_atoms {
                return Err(Error::Limit(format!(
                    "expected at most {} Hessian atoms",
                    self.max_hessian_atoms
                )));
            }
            let mut current = compute.clone();
            if current.positions.is_none()
                && let Some((positions, units)) = &self.geometry
            {
                current.positions = Some(positions.clone());
                current.units = *units;
            }
            Some(
                open.admit(
                    Some(&current),
                    opened.as_ref().and_then(|o| o.basis_functions),
                    self.max_atoms,
                )
                .map_err(admission_error)?,
            )
        } else {
            // Preserve the worker's state error for compute-before-open.
            if let Some(positions) = &compute.positions {
                if positions.is_empty() || positions.iter().flatten().any(|v| !v.is_finite()) {
                    return Err(Error::Input("expected one finite position per atom".into()));
                }
                if positions.len() > self.max_atoms {
                    return Err(Error::Limit(format!(
                        "expected at most {} atoms",
                        self.max_atoms
                    )));
                }
            }
            None
        };
        let computed: Computed = self.request_with(
            RequestOp::Compute(compute.clone()),
            self.limits.compute,
            cancel,
            false,
            |session, computed, stream, started, deadline, cancel| {
                let plans =
                    validate_computed_header(computed, opened.as_ref(), open.as_ref(), compute)
                        .map_err(|message| session.protocol_error(message))?;
                for plan in plans {
                    let values = read_array(&plan, || {
                        session.receive_frame(
                            &[],
                            protocol::FrameKind::Chunk,
                            stream,
                            deadline,
                            started,
                            cancel,
                        )
                    })
                    .inspect_err(|_| {
                        session.terminate();
                    })?;
                    install_array(computed, plan.name, values);
                }
                validate_computed_payload(computed, frame.as_ref())
                    .map_err(|message| session.protocol_error(message))
            },
        )?;
        if let Err(message) = remember_gfnff_topology(&mut self.gfnff, &computed) {
            return Err(self.protocol_error(message));
        }
        // Only successful calculations advance the geometry used for admission.
        if let Some(positions) = &compute.positions {
            self.geometry = Some((positions.clone(), compute.units));
        }
        Ok(computed)
    }

    /// Close the worker and reap it. Closing uses the hello request deadline,
    /// and cancellation remains active until the worker exits.
    pub fn close(mut self, cancel: &Cancel) -> Result<()> {
        let _: Closed = self.request(RequestOp::Close {}, self.limits.hello, cancel, true)?;
        Ok(())
    }

    fn request<T: DeserializeOwned>(
        &mut self,
        op: RequestOp,
        deadline: Duration,
        cancel: &Cancel,
        closing: bool,
    ) -> Result<T> {
        self.request_with(op, deadline, cancel, closing, |_, _, _, _, _, _| Ok(()))
    }

    fn request_with<T: DeserializeOwned>(
        &mut self,
        op: RequestOp,
        deadline: Duration,
        cancel: &Cancel,
        closing: bool,
        load: impl FnOnce(
            &mut Self,
            &mut T,
            &mut ResponseInput,
            Instant,
            Duration,
            &Cancel,
        ) -> Result<()>,
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
        let mut stream = ResponseInput::default();
        let body = self.receive_frame(
            &frame,
            protocol::FrameKind::Json,
            &mut stream,
            deadline,
            started,
            cancel,
        )?;
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
            Outcome::Ok { mut ok } => {
                load(self, &mut ok, &mut stream, started, deadline, cancel)?;
                let ended = self.finish_response(&stream)?;
                self.check_request(started, deadline, cancel)?;
                if closing {
                    self.finish_close(started, deadline, cancel)?;
                } else if ended {
                    return Err(self.exited());
                }
                Ok(ok)
            }
            Outcome::Error { error } => {
                let ended = self.finish_response(&stream)?;
                if error.message.len() > 1024 {
                    return Err(self.protocol_error("Worker error message exceeds 1 KiB".into()));
                }
                if ended || error.kind == ErrorKind::Protocol {
                    self.terminate();
                }
                Err(match error.kind {
                    ErrorKind::Protocol => Error::Protocol(error.message),
                    ErrorKind::Limit => Error::Limit(error.message),
                    ErrorKind::Unsupported => Error::Unsupported(error.message),
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

    fn receive_frame(
        &mut self,
        request: &[u8],
        kind: protocol::FrameKind,
        output: &mut ResponseInput,
        deadline: Duration,
        started: Instant,
        cancel: &Cancel,
    ) -> Result<Vec<u8>> {
        let mut written = 0;
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

            let mut frame = output
                .take_frame(kind)
                .map_err(|message| self.protocol_error(message))?;
            if frame.is_none() && !output.closed {
                let mut buffer = [0_u8; 8192];
                match self.read_stdout(&mut buffer)? {
                    Some(0) => output.closed = true,
                    Some(count) => {
                        output.bytes.extend_from_slice(&buffer[..count]);
                        progress = true;
                    }
                    None => (),
                }
                frame = output
                    .take_frame(kind)
                    .map_err(|message| self.protocol_error(message))?;
            }
            progress |= self.read_stderr()?;
            let status = self.status()?;
            if status.is_some_and(|status| status.code() == Some(75)) {
                return Err(self.exited());
            }
            if status.is_none() {
                self.check_resources()?;
            }
            if let Some(frame) = frame {
                if written != request.len() {
                    return Err(
                        self.protocol_error("Response arrived before the complete request".into())
                    );
                }
                return Ok(frame);
            }
            if output.closed || (status.is_some() && !progress) {
                return Err(self.exited());
            }
            if !progress {
                std::thread::sleep(POLL.min(deadline.saturating_sub(started.elapsed())));
            }
        }
    }

    fn finish_response(&mut self, output: &ResponseInput) -> Result<bool> {
        if !output.bytes.is_empty() {
            return Err(self.protocol_error("Unsolicited or extra worker output".into()));
        }
        let closed = output.closed || self.no_more_output()?;
        self.read_stderr()?;
        Ok(closed || self.status()?.is_some())
    }

    fn check_request(
        &mut self,
        started: Instant,
        deadline: Duration,
        cancel: &Cancel,
    ) -> Result<()> {
        self.check_cancelled(cancel)?;
        if started.elapsed() >= deadline {
            self.terminate();
            return Err(Error::Timeout);
        }
        Ok(())
    }

    fn check_cancelled(&mut self, cancel: &Cancel) -> Result<()> {
        if cancel.cancelled() {
            self.terminate();
            return Err(Error::Cancelled);
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

#[derive(Default)]
struct ResponseInput {
    bytes: Vec<u8>,
    closed: bool,
}

impl ResponseInput {
    /// Consume one frame, preserving bytes already read from the next frame.
    fn take_frame(
        &mut self,
        expected: protocol::FrameKind,
    ) -> std::result::Result<Option<Vec<u8>>, String> {
        if self.bytes.len() < protocol::HEADER_LEN {
            return Ok(None);
        }
        let header: &[u8; protocol::HEADER_LEN] =
            self.bytes[..protocol::HEADER_LEN].try_into().unwrap();
        let (kind, length) = protocol::decode_header(header).map_err(|error| error.to_string())?;
        if kind != expected {
            return Err(format!("Expected {expected:?} frame, received {kind:?}"));
        }
        let total = protocol::HEADER_LEN + length;
        if self.bytes.len() < total {
            return Ok(None);
        }
        let trailing = self.bytes.split_off(total);
        let mut frame = std::mem::replace(&mut self.bytes, trailing);
        frame.drain(..protocol::HEADER_LEN);
        Ok(Some(frame))
    }
}

fn admission_error(error: protocol::ResponseError) -> Error {
    match error.kind {
        ErrorKind::Unsupported => Error::Unsupported(error.message),
        ErrorKind::Limit => Error::Limit(error.message),
        _ => Error::Input(error.message),
    }
}

fn validate_hello(hello: &Hello) -> std::result::Result<(), String> {
    if hello.protocol != protocol::VERSION {
        return Err("Worker advertised an incompatible protocol".into());
    }
    if hello.max_atoms == 0
        || hello.max_atoms > protocol::MAX_ATOMS
        || hello.max_hessian_atoms != protocol::MAX_HESSIAN_ATOMS
        || hello.max_hessian_atoms > hello.max_atoms
    {
        return Err("Worker advertised an invalid atom limit".into());
    }
    let Some(engine) = &hello.engine else {
        return if hello.methods.is_empty() {
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
    if engine.name != "axtb"
        || hello.methods
            != [
                protocol::Method::Gfn2,
                protocol::Method::Gfnff,
                protocol::Method::Gxtb,
            ]
    {
        return Err("Worker advertised inconsistent capabilities".into());
    }
    Ok(())
}

fn engine_conventions(
    method: protocol::Method,
    conventions: protocol::Conventions,
) -> &'static str {
    match (method, conventions) {
        (_, protocol::Conventions::Axtb) => "axtb",
        (protocol::Method::Gxtb, protocol::Conventions::Xtb68) => "gxtb",
        (_, protocol::Conventions::Xtb68) => "xtb",
    }
}

fn validate_opened(
    opened: &Opened,
    open: &Open,
    max_atoms: usize,
) -> std::result::Result<(), String> {
    if opened.atoms == 0 || opened.atoms > max_atoms || opened.atoms != open.numbers.len() {
        return Err("Worker returned an inconsistent atom count".into());
    }
    if opened.method != open.method
        || opened.conventions != open.conventions
        || opened.engine_conventions != engine_conventions(open.method, open.conventions)
        || opened.numerics.trim().is_empty()
        || (open.method == protocol::Method::Gxtb && !opened.numerics.starts_with("gxtb-v2.0.1:"))
    {
        return Err("Worker returned inconsistent method metadata".into());
    }
    if open.uhf.is_some_and(|uhf| uhf != opened.uhf)
        || (open.method == protocol::Method::Gfnff && opened.uhf != 0)
    {
        return Err("Worker returned an inconsistent resolved spin".into());
    }
    let scf = open.method != protocol::Method::Gfnff;
    if opened.basis_functions.is_some() != scf || opened.basis_functions == Some(0) {
        return Err("Worker returned an inconsistent basis function count".into());
    }
    Ok(())
}

#[derive(Debug)]
struct ArrayPlan {
    name: protocol::ArrayName,
    values: usize,
    chunks: u32,
}

/// Validate every descriptor before allocating any binary array.
fn validate_array_headers(
    headers: &[protocol::ArrayHeader],
    expected: &[(protocol::ArrayName, [u32; 2])],
) -> std::result::Result<Vec<ArrayPlan>, String> {
    if headers.len() != expected.len() {
        return Err("Worker returned missing or unrequested binary arrays".into());
    }
    headers
        .iter()
        .zip(expected)
        .map(|(header, (name, shape))| {
            if header.name != *name || header.shape != *shape || header.shape.contains(&0) {
                return Err(
                    "Worker returned an unexpected binary array name, order or shape".into(),
                );
            }
            let values = (header.shape[0] as usize)
                .checked_mul(header.shape[1] as usize)
                .ok_or_else(|| "Binary array shape overflows".to_owned())?;
            let max = match header.name {
                protocol::ArrayName::CoefficientsAlpha | protocol::ArrayName::CoefficientsBeta => {
                    reshiki_qcdata::MAX_COEFFICIENTS
                }
                _ => (3 * reshiki_qcdata::MAX_HESSIAN_ATOMS).pow(2),
            };
            if values > max || header.chunks != protocol::chunk_count(values) {
                return Err(
                    "Worker returned an oversized binary array or invalid chunk count".into(),
                );
            }
            Ok(ArrayPlan {
                name: header.name,
                values,
                chunks: header.chunks,
            })
        })
        .collect()
}

fn validate_computed_header(
    computed: &Computed,
    opened: Option<&Opened>,
    open: Option<&Open>,
    request: &Compute,
) -> std::result::Result<Vec<ArrayPlan>, String> {
    let opened = opened.ok_or_else(|| "Worker computed before opening a molecule".to_owned())?;
    let open = open.ok_or_else(|| "Worker computed before opening a molecule".to_owned())?;
    if computed.method != opened.method
        || computed.conventions != opened.conventions
        || computed.engine_conventions != opened.engine_conventions
        || computed.numerics != opened.numerics
    {
        return Err("Worker returned inconsistent method metadata".into());
    }
    if !computed.energy.is_finite() || computed.energy_terms.values().any(|v| !v.is_finite()) {
        return Err("Worker returned a non-finite energy".into());
    }
    if computed
        .homo_lumo_gap_ev
        .is_some_and(|gap| !gap.is_finite() || gap < 0.0)
        || computed.near_degenerate != protocol::is_near_degenerate(computed.homo_lumo_gap_ev)
    {
        return Err("Worker returned an invalid HOMO-LUMO gap or near-degenerate flag".into());
    }
    if !computed.seconds.is_finite() || computed.seconds < 0.0 {
        return Err("Worker returned an invalid elapsed time".into());
    }
    match (
        request.gradient || request.hessian.is_some(),
        &computed.gradient,
    ) {
        (true, Some(rows))
            if rows.len() == opened.atoms && rows.iter().flatten().all(|x| x.is_finite()) => {}
        (true, _) => {
            return Err(
                "Worker returned a missing, non-finite or incorrectly sized gradient".into(),
            );
        }
        (false, None) => (),
        (false, Some(_)) => return Err("Worker returned an unrequested gradient".into()),
    }
    validate_settings(computed, opened, open)?;
    let scf = open.method != protocol::Method::Gfnff;
    let charges = computed
        .charges
        .as_ref()
        .ok_or_else(|| "Worker omitted atomic charges".to_owned())?;
    let scheme = if scf {
        reshiki_qcdata::ChargeScheme::Mulliken
    } else {
        reshiki_qcdata::ChargeScheme::Eeq
    };
    if charges.scheme != scheme
        || charges.values.len() != opened.atoms
        || charges.values.iter().any(|v| !v.is_finite())
    {
        return Err("Worker returned invalid atomic charges".into());
    }
    if computed.dipole_e_bohr.is_none()
        || computed.quadrupole_e_bohr2.is_some() != scf
        || computed
            .dipole_e_bohr
            .iter()
            .flatten()
            .chain(computed.quadrupole_e_bohr2.iter().flatten())
            .any(|v| !v.is_finite())
        || computed.scf.is_some() != scf
        || computed.gfnff.is_some() == scf
        || (!scf && computed.homo_lumo_gap_ev.is_some())
    {
        return Err("Worker returned inconsistent method-specific outputs".into());
    }
    let composite = open.method == protocol::Method::Gxtb && open.solvent.is_some();
    let valid_parts = |parts: &BTreeMap<String, protocol::CompositePart>, hessian| {
        let names = ["gxtb", "gfn2_solvated", "gfn2_gas"];
        parts.len() == names.len()
            && names.iter().all(|name| {
                parts
                    .get(*name)
                    .is_some_and(|part| part.iterations.map_or(hessian, |count| count > 0))
            })
    };
    if computed.composite.is_some() != composite
        || computed.composite.as_ref().is_some_and(|composite| {
            composite.components.is_empty()
                || composite.components.values().any(|v| !v.is_finite())
                || !valid_parts(&composite.parts, false)
                || composite.hessian_parts.is_some() != request.hessian.is_some()
                || composite
                    .hessian_parts
                    .as_ref()
                    .is_some_and(|p| !valid_parts(p, true))
        })
    {
        return Err("Worker returned inconsistent solvent composite outputs".into());
    }
    if let Some(report) = &computed.scf {
        if report.iterations == 0
            || !["previous", "eeq", "sad", "eeq-bc"].contains(&report.start.as_str())
            || !report.electronic_temperature_k.is_finite()
            || report.electronic_temperature_k < 0.0
        {
            return Err("Worker returned an invalid SCF report".into());
        }
        let settings = open.scf.clone();
        match (&report.retry, settings.retries.len()) {
            (None, 0) => (),
            (Some(retry), count) if count > 0 && retry.of == count && retry.rung <= count => (),
            _ => return Err("Worker returned an inconsistent SCF retry report".into()),
        }
    }
    match (request.bond_orders, &computed.bond_orders) {
        (false, None) => (),
        (true, Some(bonds)) if scf => {
            let mut previous = None;
            for bond in bonds {
                let pair = (bond.a, bond.b);
                if bond.a >= bond.b
                    || bond.b as usize >= opened.atoms
                    || !bond.order.is_finite()
                    || bond.order < protocol::BOND_ORDER_MIN
                    || previous.is_some_and(|p| p >= pair)
                {
                    return Err("Worker returned invalid bond order pairs".into());
                }
                previous = Some(pair);
            }
        }
        _ => return Err("Worker returned missing or unrequested bond orders".into()),
    }
    let axes = (opened.atoms * 3) as u32;
    let mut expected = Vec::new();
    if let Some(hessian) = &request.hessian {
        if opened.atoms > reshiki_qcdata::MAX_HESSIAN_ATOMS
            || computed.hessian_method.as_deref() != Some("analytic")
        {
            return Err("Worker returned an invalid Hessian method or atom count".into());
        }
        let vibrations = computed
            .vibrations
            .as_ref()
            .ok_or_else(|| "Worker omitted vibrational analysis".to_owned())?;
        validate_vibrations(vibrations, opened)?;
        if hessian
            .masses_amu
            .as_ref()
            .is_some_and(|masses| *masses != vibrations.masses_amu)
        {
            return Err("Worker returned vibrational masses that differ from the request".into());
        }
        validate_thermo(
            computed
                .thermo
                .as_ref()
                .ok_or_else(|| "Worker omitted thermochemistry".to_owned())?,
            hessian,
        )?;
        expected.push((protocol::ArrayName::Hessian, [axes, axes]));
        let dipole_derivatives = computed
            .arrays
            .iter()
            .any(|a| a.name == protocol::ArrayName::DipoleDerivatives);
        if dipole_derivatives != vibrations.ir_intensities_km_mol.is_some() {
            return Err(
                "Worker returned inconsistent dipole derivatives and IR intensities".into(),
            );
        }
        if dipole_derivatives {
            expected.push((protocol::ArrayName::DipoleDerivatives, [axes, 3]));
        }
        let modes = vibrations.frequencies_cm1.len() as u32;
        if modes > 0 {
            expected.push((protocol::ArrayName::NormalModes, [modes, axes]));
        }
    } else if computed.hessian_method.is_some()
        || computed.vibrations.is_some()
        || computed.thermo.is_some()
    {
        return Err("Worker returned unrequested Hessian outputs".into());
    }
    match (&request.orbitals, &computed.orbitals) {
        (protocol::OrbitalOutput::None, None) => (),
        (protocol::OrbitalOutput::None, Some(_)) | (_, None) => {
            return Err("Worker returned missing or unrequested orbitals".into());
        }
        (output, Some(orbitals)) => {
            let width = opened
                .basis_functions
                .ok_or_else(|| "Orbitals returned without an SCF basis".to_owned())?;
            let rows = output.coefficient_rows(width);
            if orbitals.alpha.rows != rows
                || orbitals.alpha.energies_hartree.len() != width as usize
                || orbitals.alpha.occupations.len() != width as usize
                || !orbitals.alpha.coefficients.is_empty()
                || orbitals.basis.is_some() != !rows.is_empty()
            {
                return Err("Worker returned inconsistent orbital levels, rows or basis".into());
            }
            if let Some(basis) = &orbitals.basis
                && basis.shells.iter().map(|s| s.n_functions()).sum::<usize>() != width as usize
            {
                return Err("Worker returned an inconsistent orbital basis width".into());
            }
            if !rows.is_empty() {
                expected.push((
                    protocol::ArrayName::CoefficientsAlpha,
                    [rows.len() as u32, width],
                ));
            }
            let spin_matches = match (open.method, computed.settings.uhf, &orbitals.beta) {
                (_, 0, reshiki_qcdata::Beta::Restricted) => true,
                (
                    protocol::Method::Gfn2,
                    1..,
                    reshiki_qcdata::Beta::SameOrbitals { occupations },
                ) => occupations.len() == width as usize,
                (protocol::Method::Gxtb, 1.., reshiki_qcdata::Beta::Unrestricted(_)) => true,
                _ => false,
            };
            if !spin_matches {
                return Err("Worker returned an inconsistent orbital spin representation".into());
            }
            if let reshiki_qcdata::Beta::Unrestricted(beta) = &orbitals.beta {
                if beta.rows != rows
                    || beta.energies_hartree.len() != width as usize
                    || beta.occupations.len() != width as usize
                    || !beta.coefficients.is_empty()
                {
                    return Err("Worker returned inconsistent beta orbital levels or rows".into());
                }
                if !rows.is_empty() {
                    expected.push((
                        protocol::ArrayName::CoefficientsBeta,
                        [rows.len() as u32, width],
                    ));
                }
            }
        }
    }
    if !computed.hessian.is_empty()
        || !computed.dipole_derivatives.is_empty()
        || !computed.normal_modes.is_empty()
    {
        return Err("Worker embedded binary arrays in its JSON result".into());
    }
    validate_array_headers(&computed.arrays, &expected)
}

fn validate_settings(
    computed: &Computed,
    opened: &Opened,
    open: &Open,
) -> std::result::Result<(), String> {
    let settings = &computed.settings;
    if settings.charge != open.charge
        || settings.uhf != opened.uhf
        || settings.solvent != open.solvent
        || settings.scf != open.scf.clone()
        || settings.threads != open.threads
    {
        return Err("Worker returned settings that differ from the opened session".into());
    }
    if open.method != protocol::Method::Gfnff {
        return if settings.gfnff.is_none() {
            Ok(())
        } else {
            Err("Worker returned unrequested GFN-FF settings".into())
        };
    }
    let topology = settings
        .gfnff
        .as_ref()
        .ok_or_else(|| "Worker omitted GFN-FF topology settings".to_owned())?;
    let report = computed
        .gfnff
        .as_ref()
        .ok_or_else(|| "Worker omitted GFN-FF fragments".to_owned())?;
    let reference = open
        .gfnff
        .as_ref()
        .and_then(|g| g.reference_positions.as_ref())
        .unwrap_or(&open.positions);
    if topology.reference_positions_bohr.len() != open.numbers.len()
        || report.fragments.len() != open.numbers.len()
        || topology
            .reference_positions_bohr
            .iter()
            .flatten()
            .any(|v| !v.is_finite())
    {
        return Err("Worker returned an invalid GFN-FF reference geometry".into());
    }
    let scale = match open.units {
        protocol::Units::Bohr => 1.0,
        protocol::Units::Angstrom => 1.0 / reshiki_qcdata::ANGSTROM_PER_BOHR,
    };
    if reference
        .iter()
        .flatten()
        .zip(topology.reference_positions_bohr.iter().flatten())
        .any(|(&input, &actual)| {
            let expected = input * scale;
            match open.units {
                protocol::Units::Bohr => actual != expected,
                protocol::Units::Angstrom => {
                    (actual - expected).abs() > 1e-7 * expected.abs().max(1.0)
                }
            }
        })
    {
        return Err("Worker returned a different GFN-FF reference geometry".into());
    }
    let mut fragments = 0_u32;
    for &fragment in &report.fragments {
        if fragment > fragments {
            return Err("Worker returned unordered GFN-FF fragments".into());
        }
        if fragment == fragments {
            fragments += 1;
        }
    }
    if topology.fragment_charges.len() != fragments as usize
        || topology
            .fragment_charges
            .iter()
            .map(|&q| i64::from(q))
            .sum::<i64>()
            != i64::from(open.charge)
    {
        return Err("Worker returned invalid GFN-FF fragment charges".into());
    }
    if let Some(charges) = open
        .gfnff
        .as_ref()
        .and_then(|g| g.fragment_charges.as_ref())
        .filter(|charges| !charges.is_empty())
    {
        if topology.fragment_charges != *charges {
            return Err("Worker changed explicit GFN-FF fragment charges".into());
        }
    } else if topology.fragment_charges.first() != Some(&open.charge)
        || topology
            .fragment_charges
            .iter()
            .skip(1)
            .any(|&charge| charge != 0)
    {
        return Err("Worker returned inconsistent default GFN-FF fragment charges".into());
    }
    Ok(())
}

fn remember_gfnff_topology(
    previous: &mut Option<(protocol::GfnFfSettings, Vec<u32>)>,
    computed: &Computed,
) -> std::result::Result<(), String> {
    let (Some(settings), Some(report)) = (&computed.settings.gfnff, &computed.gfnff) else {
        return Ok(());
    };
    if let Some((previous_settings, fragments)) = previous {
        if settings != &*previous_settings || report.fragments != *fragments {
            return Err("Worker changed the opened GFN-FF topology".into());
        }
    } else {
        *previous = Some((settings.clone(), report.fragments.clone()));
    }
    Ok(())
}

fn validate_vibrations(
    v: &protocol::VibrationsReport,
    opened: &Opened,
) -> std::result::Result<(), String> {
    let axes = opened.atoms * 3;
    let modes = v.frequencies_cm1.len();
    let expected_conventions = if opened.engine_conventions == "gxtb" {
        "xtb-6.7.1"
    } else {
        &opened.engine_conventions
    };
    let expected_modes = match opened.atoms {
        0 | 1 => 0,
        2 => 1,
        _ => axes - if v.linear { 5 } else { 6 },
    };
    if v.conventions != expected_conventions
        || (opened.atoms == 2 && !v.linear)
        || modes != expected_modes
        || v.frequencies_cm1.iter().any(|v| !v.is_finite())
        || v.frequencies_cm1.windows(2).any(|p| p[0] > p[1])
        || v.ir_intensities_km_mol
            .as_ref()
            .is_some_and(|v| v.len() != modes)
        || v.reduced_masses_amu.len() != modes
        || v.force_constants_mdyn_per_angstrom.len() != modes
        || v.masses_amu.len() != opened.atoms
        || v.ir_intensities_km_mol
            .iter()
            .flatten()
            .any(|v| !v.is_finite() || *v < 0.0)
        || v.force_constants_mdyn_per_angstrom
            .iter()
            .any(|v| !v.is_finite())
        || v.reduced_masses_amu
            .iter()
            .chain(&v.masses_amu)
            .any(|v| !v.is_finite() || *v <= 0.0)
        || v.point_group.as_ref().is_some_and(|p| p.trim().is_empty())
    {
        return Err("Worker returned invalid vibrational analysis".into());
    }
    Ok(())
}

fn validate_thermo(
    t: &reshiki_qcdata::Thermo,
    hessian: &protocol::HessianRequest,
) -> std::result::Result<(), String> {
    if [
        t.temperature_k,
        t.pressure_pa,
        t.zero_point_energy_hartree,
        t.enthalpy_correction_hartree,
        t.entropy_hartree_per_k,
        t.gibbs_correction_hartree,
        t.frequency_scale,
        t.rotor_cutoff_cm1,
        t.imaginary_cutoff_cm1,
    ]
    .iter()
    .any(|v| !v.is_finite())
        || t.heat_capacity_hartree_per_k
            .is_some_and(|v| !v.is_finite())
        || t.temperature_k <= 0.0
        || t.pressure_pa <= 0.0
        || t.frequency_scale <= 0.0
        || t.symmetry_number == 0
        || (t.gibbs_correction_hartree
            - (t.enthalpy_correction_hartree - t.temperature_k * t.entropy_hartree_per_k))
            .abs()
            > 1e-5
    {
        return Err("Worker returned invalid thermochemistry".into());
    }
    let requested = &hessian.thermo;
    if [
        (requested.temperature_k, t.temperature_k),
        (requested.pressure_pa, t.pressure_pa),
        (requested.frequency_scale, t.frequency_scale),
        (requested.rotor_cutoff_cm1, t.rotor_cutoff_cm1),
    ]
    .iter()
    .any(|(expected, actual)| expected.is_some_and(|v| v != *actual))
        || requested
            .imaginary_cutoff_cm1
            .is_some_and(|v| v.abs() != t.imaginary_cutoff_cm1.abs())
        || requested
            .symmetry_number
            .is_some_and(|v| v != t.symmetry_number)
    {
        return Err("Worker returned thermochemistry settings that differ from the request".into());
    }
    Ok(())
}

fn install_array(computed: &mut Computed, name: protocol::ArrayName, values: Vec<f64>) {
    match name {
        protocol::ArrayName::Hessian => computed.hessian = values,
        protocol::ArrayName::DipoleDerivatives => computed.dipole_derivatives = values,
        protocol::ArrayName::NormalModes => computed.normal_modes = values,
        protocol::ArrayName::CoefficientsAlpha => {
            computed.orbitals.as_mut().unwrap().alpha.coefficients = values
        }
        protocol::ArrayName::CoefficientsBeta => {
            if let reshiki_qcdata::Beta::Unrestricted(set) =
                &mut computed.orbitals.as_mut().unwrap().beta
            {
                set.coefficients = values;
            }
        }
    }
}

fn append_chunk(
    out: &mut Vec<f64>,
    body: &[u8],
    index: u32,
    values: usize,
) -> std::result::Result<(), String> {
    if (index as usize).checked_mul(protocol::CHUNK_VALUES) != Some(out.len())
        || out.len() >= values
    {
        return Err("Worker returned a chunk beyond the announced array".into());
    }
    let remaining = (values - out.len()).min(protocol::CHUNK_VALUES);
    protocol::decode_chunk(body, index, remaining, out).map_err(|error| error.to_string())
}

fn read_array(plan: &ArrayPlan, mut receive: impl FnMut() -> Result<Vec<u8>>) -> Result<Vec<f64>> {
    let mut values = Vec::with_capacity(plan.values);
    for index in 0..plan.chunks {
        append_chunk(&mut values, &receive()?, index, plan.values).map_err(Error::Protocol)?;
    }
    Ok(values)
}

fn validate_computed_payload(
    computed: &Computed,
    frame: Option<&reshiki_qcdata::Frame>,
) -> std::result::Result<(), String> {
    let mut frame = frame
        .ok_or_else(|| "Worker computed before opening a molecule".to_owned())?
        .clone();
    frame.multiplicity = computed
        .settings
        .uhf
        .checked_add(1)
        .ok_or_else(|| "Worker returned an invalid resolved spin".to_owned())?;
    if let Some(orbitals) = &computed.orbitals {
        orbitals
            .validate(&frame)
            .map_err(|error| format!("Worker returned invalid orbitals: {error}"))?;
    }
    let axes = frame.numbers.len() * 3;
    if let Some(vibrations) = &computed.vibrations {
        if computed.hessian.len() != axes * axes
            || (0..axes).any(|a| {
                (a + 1..axes)
                    .any(|b| computed.hessian[a * axes + b] != computed.hessian[b * axes + a])
            })
        {
            return Err("Worker returned an asymmetric or incorrectly sized Hessian".into());
        }
        if computed.normal_modes.len() != vibrations.frequencies_cm1.len() * axes {
            return Err("Worker returned an incorrect number of normal modes".into());
        }
    }
    if computed.normal_modes.chunks_exact(axes).any(|row| {
        let norm = row.iter().map(|v| v * v).sum::<f64>();
        !norm.is_finite() || (norm - 1.0).abs() > 1e-6
    }) {
        return Err(
            "Worker returned a normal mode that is not a unit Cartesian displacement".into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests;
