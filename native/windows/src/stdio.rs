//! Keeps this process's standard handles out of the processes it starts.
//!
//! `std::process::Command` creates every child with `bInheritHandles = TRUE`,
//! so each inheritable handle of this process is duplicated into the child,
//! even when the child's own standard handles are other pipes. An MCP client
//! that launched `reshiki --mcp` gave it inheritable pipes as stdin and
//! stdout; a worker process still running when we exit would then hold the
//! client's stdout open and delay its end of file. Clearing the inherit flag
//! on our own standard handles stops that and leaves our reads and writes
//! unchanged; `Stdio::inherit` duplicates a handle for its child, so it keeps
//! working too.
use std::{fmt, io};
use windows::Win32::{
    Foundation::{
        GetHandleInformation, HANDLE, HANDLE_FLAG_INHERIT, HANDLE_FLAGS, SetHandleInformation,
    },
    Storage::FileSystem::{FILE_TYPE_CHAR, FILE_TYPE_DISK, FILE_TYPE_PIPE, GetFileType},
    System::Console::{
        GetStdHandle, STD_ERROR_HANDLE, STD_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    },
};

#[cfg(test)]
mod tests;

/// One of the three standard handles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StdHandle {
    Input,
    Output,
    Error,
}

impl StdHandle {
    const ALL: [Self; 3] = [Self::Input, Self::Output, Self::Error];

    fn name(self) -> &'static str {
        match self {
            Self::Input => "stdin",
            Self::Output => "stdout",
            Self::Error => "stderr",
        }
    }

    fn id(self) -> STD_HANDLE {
        match self {
            Self::Input => STD_INPUT_HANDLE,
            Self::Output => STD_OUTPUT_HANDLE,
            Self::Error => STD_ERROR_HANDLE,
        }
    }
}

/// What a standard handle refers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Pipe,
    /// A console or another character device.
    Char,
    Disk,
    Unknown,
}

/// The handle operations [`policy`] needs, faked in tests.
trait HandleOps {
    type Raw: Copy;
    /// The standard handle `which`; None when it is null or invalid.
    fn std_handle(&self, which: StdHandle) -> Option<Self::Raw>;
    fn kind(&self, handle: Self::Raw) -> Kind;
    fn clear_inherit(&self, handle: Self::Raw) -> io::Result<()>;
    fn inheritable(&self, handle: Self::Raw) -> io::Result<bool>;
}

/// The standard handles of this process, through Win32.
struct Win32Ops;

impl HandleOps for Win32Ops {
    type Raw = HANDLE;

    fn std_handle(&self, which: StdHandle) -> Option<HANDLE> {
        // SAFETY: GetStdHandle takes no pointers. The handle it returns is
        // borrowed from the process parameters and is never closed here.
        let handle = unsafe { GetStdHandle(which.id()) };
        handle.ok().filter(|handle| !handle.is_invalid())
    }

    fn kind(&self, handle: HANDLE) -> Kind {
        // SAFETY: `handle` is a non-null, valid standard handle of this
        // process; GetFileType only queries its type.
        match unsafe { GetFileType(handle) } {
            FILE_TYPE_PIPE => Kind::Pipe,
            FILE_TYPE_CHAR => Kind::Char,
            FILE_TYPE_DISK => Kind::Disk,
            _ => Kind::Unknown,
        }
    }

    fn clear_inherit(&self, handle: HANDLE) -> io::Result<()> {
        // SAFETY: `handle` is a non-null, valid standard handle of this
        // process; only its inherit flag changes.
        unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT.0, HANDLE_FLAGS(0)) }
            .map_err(io::Error::from)
    }

    fn inheritable(&self, handle: HANDLE) -> io::Result<bool> {
        let mut flags = 0;
        // SAFETY: `handle` is a non-null, valid standard handle of this
        // process, and `flags` is a writable u32 that outlives the call.
        unsafe { GetHandleInformation(handle, &mut flags) }.map_err(io::Error::from)?;
        Ok(flags & HANDLE_FLAG_INHERIT.0 != 0)
    }
}

/// Which standard handles were pipes, now kept out of child processes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Protected {
    pub stdin: bool,
    pub stdout: bool,
    pub stderr: bool,
}

impl Protected {
    fn mark(&mut self, which: StdHandle) {
        match which {
            StdHandle::Input => self.stdin = true,
            StdHandle::Output => self.stdout = true,
            StdHandle::Error => self.stderr = true,
        }
    }
}

/// A standard pipe that child processes could still inherit.
#[derive(Debug)]
pub struct StdioError {
    handle: StdHandle,
    cause: Cause,
}

#[derive(Debug)]
enum Cause {
    /// The flag stayed set, with the error that clearing it returned.
    Inheritable(Option<io::Error>),
    /// The flag could not be read back.
    Unchecked(io::Error),
}

impl fmt::Display for StdioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = self.handle.name();
        match &self.cause {
            Cause::Inheritable(None) => write!(f, "the {name} pipe stays inheritable"),
            Cause::Inheritable(Some(error)) => {
                write!(f, "the {name} pipe stays inheritable: {error}")
            }
            Cause::Unchecked(error) => {
                write!(
                    f,
                    "cannot check whether the {name} pipe is inheritable: {error}"
                )
            }
        }
    }
}

impl std::error::Error for StdioError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.cause {
            Cause::Inheritable(error) => error.as_ref().map(|error| error as _),
            Cause::Unchecked(error) => Some(error),
        }
    }
}

/// Clears the inherit flag of each standard handle. A pipe must be
/// confirmed non-inheritable afterwards; consoles, files and unknown handles
/// cannot hold a client's end of file, so their failures are ignored.
fn policy<O: HandleOps>(ops: &O) -> Result<Protected, StdioError> {
    let mut protected = Protected::default();
    for which in StdHandle::ALL {
        let Some(handle) = ops.std_handle(which) else {
            continue;
        };
        let pipe = ops.kind(handle) == Kind::Pipe;
        let cleared = ops.clear_inherit(handle);
        if !pipe {
            continue;
        }
        let cause = match ops.inheritable(handle) {
            Ok(false) => {
                protected.mark(which);
                continue;
            }
            Ok(true) => Cause::Inheritable(cleared.err()),
            Err(error) => Cause::Unchecked(error),
        };
        return Err(StdioError {
            handle: which,
            cause,
        });
    }
    Ok(protected)
}

/// Keeps this process's standard handles out of every process it starts
/// from now on. Call it before starting any process; it fails if a standard
/// handle that is a pipe stays inheritable.
pub fn disinherit_standard_handles() -> Result<Protected, StdioError> {
    policy(&Win32Ops)
}

/// Whether the standard handle `which` is inheritable; None when it is
/// missing or cannot be queried.
pub fn standard_handle_inheritable(which: StdHandle) -> Option<bool> {
    let handle = Win32Ops.std_handle(which)?;
    Win32Ops.inheritable(handle).ok()
}
