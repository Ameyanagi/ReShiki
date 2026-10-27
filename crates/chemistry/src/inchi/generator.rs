//! InChI generation and import through an isolated, versioned Rust helper.
//! Both the chemistry adapter and InChI kernel are Rust. One process handles
//! one immutable request; dropping the future kills the child.
mod transport;
use super::{
    kernel::{Imported, Molecule as Input},
    output,
};
#[cfg(not(target_arch = "wasm32"))]
use std::process::Stdio;
use std::{path::Path, time::Duration};
#[cfg(not(target_arch = "wasm32"))]
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::Command,
};

pub const MAX_INCHI_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_REQUEST_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
#[cfg(not(target_arch = "wasm32"))]
const MAX_STDERR_BYTES: usize = 64 * 1024;
pub const MAX_TIMEOUT: Duration = Duration::from_secs(120);
pub const DEFAULT_HEAP_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_HEAP_BYTES: usize = 512 * 1024 * 1024;
/// Keeps the worker console hidden. Copies: reshiki::workers, reshiki-io's
/// geometry/client.rs and engine/reference.rs, reshiki-windows'
/// metafile_worker.rs (none of them can share one crate).
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Bounds live Rust allocations during the operation, including the adapter
/// and response. This is not an RSS or stack limit. Request parsing is byte bounded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub timeout: Duration,
    pub heap_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    Heap,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Break,
    Skipped,
    Empty,
    Success,
    Warning,
    Error,
    Fatal,
    Unknown,
    Busy,
}
impl Status {
    pub fn code(self) -> i16 {
        match self {
            Self::Break => -100,
            Self::Skipped => -2,
            Self::Empty => -1,
            Self::Success => 0,
            Self::Warning => 1,
            Self::Error => 2,
            Self::Fatal => 3,
            Self::Unknown => 4,
            Self::Busy => 5,
        }
    }
    pub fn is_success(self) -> bool {
        matches!(self, Self::Success | Self::Warning)
    }
    fn from_code(code: i16) -> Result<Self, Error> {
        match code {
            -100 => Ok(Self::Break),
            -2 => Ok(Self::Skipped),
            -1 => Ok(Self::Empty),
            0 => Ok(Self::Success),
            1 => Ok(Self::Warning),
            2 => Ok(Self::Error),
            3 => Ok(Self::Fatal),
            4 => Ok(Self::Unknown),
            5 => Ok(Self::Busy),
            _ => Err(Error::Protocol("Unknown native return status")),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Output {
    pub status: Status,
    pub inchi: String,
    pub message: String,
    pub log: String,
    pub auxiliary: String,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid InChI helper input: {0}")]
    Input(&'static str),
    #[error("InChI helper {0} exceeds its byte limit")]
    Limit(&'static str),
    #[error("InChI helper I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("InChI helper protocol: {0}")]
    Protocol(&'static str),
    #[error("InChI helper reports incompatible version {0}")]
    Version(String),
    #[error("InChI helper rejected the request: {0}")]
    Rejected(String),
    #[error("InChI helper exited with code {code:?}: {diagnostic}")]
    Exit {
        code: Option<i32>,
        diagnostic: String,
    },
    #[error("InChI helper {resource:?} exhausted its {budget} byte budget")]
    ResourceLimit {
        resource: Resource,
        budget: u64,
        used: u64,
        requested: u64,
    },
    #[error("InChI operation exceeded its time limit")]
    Timeout,
}

#[cfg(not(target_arch = "wasm32"))]
async fn read_limited(
    mut stream: impl AsyncRead + Unpin,
    limit: usize,
    name: &'static str,
) -> Result<Vec<u8>, Error> {
    let mut result = Vec::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let count = stream.read(&mut buffer).await?;
        if count == 0 {
            return Ok(result);
        }
        if count > limit.saturating_sub(result.len()) {
            return Err(Error::Limit(name));
        }
        result.try_reserve(count).map_err(|_| Error::Limit(name))?;
        result.extend_from_slice(
            buffer
                .get(..count)
                .ok_or(Error::Protocol("Invalid stream length"))?,
        );
    }
}

/// Return the kernel status even when it produces no identifier. Chemistry
/// warnings/errors are distinct from transport, timeout and process failures.
/// The executable is launched directly in InChI worker mode, without a shell.
pub async fn generate(helper: &Path, input: &Input, timeout: Duration) -> Result<Output, Error> {
    generate_with_limits(
        helper,
        input,
        Limits {
            timeout,
            heap_bytes: DEFAULT_HEAP_BYTES,
        },
    )
    .await
}

pub async fn generate_with_limits(
    helper: &Path,
    input: &Input,
    limits: Limits,
) -> Result<Output, Error> {
    validate_limits(limits)?;
    let request = transport::encode(input, limits.heap_bytes)?;
    transport::decode(&exchange(helper, &request, limits.timeout, limits.heap_bytes).await?)
}

/// Generate an identifier in a library embedding without launching a helper.
/// Input and output retain the helper protocol's byte and graph validation.
/// The caller owns cancellation and process resource limits in this mode.
pub fn generate_in_process(input: &Input) -> Result<Output, Error> {
    let request = transport::encode(input, DEFAULT_HEAP_BYTES)?;
    transport::decode(&exchange_in_process(&request)?)
}

/// Import an InChI in a library embedding, preserving explicit hydrogens.
pub fn read_in_process(inchi: &str) -> Result<Imported, Error> {
    let request = transport::encode_import(
        inchi,
        DEFAULT_HEAP_BYTES,
        output::Options {
            sanitize: true,
            remove_hydrogens: false,
        },
    )?;
    transport::decode_import(&exchange_in_process(&request)?)
}

fn exchange_in_process(request: &[u8]) -> Result<Vec<u8>, Error> {
    // The recursive kernel uses the same explicit stack as the native worker.
    // A scoped thread lets the embedding retain ownership of request bytes.
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::thread::scope(|scope| {
            std::thread::Builder::new()
                .name("reshiki-inchi".into())
                .stack_size(32 * 1024 * 1024)
                .spawn_scoped(scope, || super::worker::respond(request))?
                .join()
                .map_err(|_| Error::Rejected("InChI operation panicked".into()))?
                .map_err(Error::Rejected)
        })
    }
    #[cfg(target_arch = "wasm32")]
    super::worker::respond(request).map_err(Error::Rejected)
}

/// Read a sanitized molecule, preserving explicit graph hydrogens.
/// Chemical statuses and diagnostics are separate from process failures.
/// The dependency does not expose raw native records or warning bitmasks.
pub async fn read(helper: &Path, inchi: &str, timeout: Duration) -> Result<Imported, Error> {
    read_with_limits(
        helper,
        inchi,
        Limits {
            timeout,
            heap_bytes: DEFAULT_HEAP_BYTES,
        },
    )
    .await
}

pub async fn read_with_limits(
    helper: &Path,
    inchi: &str,
    limits: Limits,
) -> Result<Imported, Error> {
    read_with_options(
        helper,
        inchi,
        limits,
        output::Options {
            sanitize: true,
            remove_hydrogens: false,
        },
    )
    .await
}

pub async fn read_with_options(
    helper: &Path,
    inchi: &str,
    limits: Limits,
    options: output::Options,
) -> Result<Imported, Error> {
    validate_limits(limits)?;
    let request = transport::encode_import(inchi, limits.heap_bytes, options)?;
    transport::decode_import(&exchange(helper, &request, limits.timeout, limits.heap_bytes).await?)
}

fn validate_limits(limits: Limits) -> Result<(), Error> {
    if limits.timeout.is_zero() || limits.timeout > MAX_TIMEOUT {
        return Err(Error::Input(
            "Timeout must be greater than zero and at most 120 seconds",
        ));
    }
    if limits.heap_bytes == 0 || limits.heap_bytes > MAX_HEAP_BYTES {
        return Err(Error::Input(
            "Helper heap budget must be between 1 byte and 512 MiB",
        ));
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn helper_command(helper: &Path) -> Command {
    let mut command = Command::new(helper);
    command
        .arg("--inchi-worker")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

/// The browser has no helper processes, so the request runs in-process
/// through the same framing. The timeout and heap budget do not apply there.
#[cfg(target_arch = "wasm32")]
async fn exchange(
    _helper: &Path,
    request: &[u8],
    _timeout: Duration,
    _heap_bytes: usize,
) -> Result<Vec<u8>, Error> {
    super::worker::respond(request).map_err(Error::Rejected)
}

#[cfg(not(target_arch = "wasm32"))]
async fn exchange(
    helper: &Path,
    request: &[u8],
    timeout: Duration,
    heap_bytes: usize,
) -> Result<Vec<u8>, Error> {
    let operation = async {
        let mut child = helper_command(helper).spawn()?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or(Error::Protocol("Missing request pipe"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or(Error::Protocol("Missing response pipe"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or(Error::Protocol("Missing diagnostic pipe"))?;
        let writer = async {
            stdin.write_all(request).await?;
            stdin.shutdown().await?;
            drop(stdin);
            Ok::<_, Error>(())
        };
        let waiter = async { child.wait().await.map_err(Error::Io) };
        let ((), response, diagnostic, status) = tokio::try_join!(
            writer,
            read_limited(stdout, MAX_RESPONSE_BYTES, "response"),
            read_limited(stderr, MAX_STDERR_BYTES, "diagnostic"),
            waiter
        )?;
        if status.code() == Some(super::wire::RESOURCE_EXIT) {
            let marker = std::str::from_utf8(&diagnostic)
                .map_err(|_| Error::Protocol("Invalid heap limit diagnostic"))?;
            let fields = marker
                .strip_prefix("RESHIKI_HEAP_LIMIT ")
                .ok_or(Error::Protocol("Missing heap limit diagnostic"))?
                .split_whitespace()
                .map(str::parse::<u64>)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| Error::Protocol("Invalid heap limit diagnostic"))?;
            let [budget, used, requested] = fields.as_slice() else {
                return Err(Error::Protocol("Invalid heap limit diagnostic"));
            };
            if *budget != heap_bytes as u64
                || *budget > MAX_HEAP_BYTES as u64
                || used > budget
                || *requested == 0
            {
                return Err(Error::Protocol("Invalid heap accounting"));
            }
            return Err(Error::ResourceLimit {
                resource: Resource::Heap,
                budget: *budget,
                used: *used,
                requested: *requested,
            });
        }
        if !status.success() {
            return Err(Error::Exit {
                code: status.code(),
                diagnostic: String::from_utf8_lossy(&diagnostic).into_owned(),
            });
        }
        Ok(response)
    };
    tokio::time::timeout(timeout, operation)
        .await
        .map_err(|_| Error::Timeout)?
}
