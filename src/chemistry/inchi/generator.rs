//! Standard InChI generation through an isolated, versioned native helper.
//! The application never loads the C kernel or calls FFI. One process handles
//! one immutable request; dropping the future kills the child.
mod transport;
use super::input::Input;
use std::{path::Path, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::Command,
};

pub const MAX_REQUEST_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
const MAX_STDERR_BYTES: usize = 64 * 1024;
pub const MAX_TIMEOUT: Duration = Duration::from_secs(120);
pub const DEFAULT_KERNEL_HEAP_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_KERNEL_HEAP_BYTES: usize = 512 * 1024 * 1024;

/// The arena covers the kernel's direct C heap, including allocation metadata.
/// It is not a process RSS limit; stack, system runtime and bridge buffers are separate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub timeout: Duration,
    pub kernel_heap_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    KernelHeap,
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
    #[error("InChI helper could not allocate its {budget} byte {resource:?} arena")]
    ResourceUnavailable { resource: Resource, budget: u64 },
    #[error("InChI generation exceeded its time limit")]
    Timeout,
}

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
/// The executable is launched directly, without arguments or a shell.
pub async fn generate(helper: &Path, input: &Input, timeout: Duration) -> Result<Output, Error> {
    generate_with_limits(
        helper,
        input,
        Limits {
            timeout,
            kernel_heap_bytes: DEFAULT_KERNEL_HEAP_BYTES,
        },
    )
    .await
}

pub async fn generate_with_limits(
    helper: &Path,
    input: &Input,
    limits: Limits,
) -> Result<Output, Error> {
    if limits.timeout.is_zero() || limits.timeout > MAX_TIMEOUT {
        return Err(Error::Input(
            "Timeout must be greater than zero and at most 120 seconds",
        ));
    }
    if limits.kernel_heap_bytes == 0 || limits.kernel_heap_bytes > MAX_KERNEL_HEAP_BYTES {
        return Err(Error::Input(
            "Kernel heap budget must be between 1 byte and 512 MiB",
        ));
    }
    let request = transport::encode(input, limits.kernel_heap_bytes)?;
    let operation = async {
        let mut child = Command::new(helper)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;
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
            stdin.write_all(&request).await?;
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
        if !status.success() {
            return Err(Error::Exit {
                code: status.code(),
                diagnostic: String::from_utf8_lossy(&diagnostic).into_owned(),
            });
        }
        transport::decode(&response)
    };
    tokio::time::timeout(limits.timeout, operation)
        .await
        .map_err(|_| Error::Timeout)?
}
