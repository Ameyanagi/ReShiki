//! Safe, naming-specific child limits. All platform FFI is contained here.
#![deny(unsafe_op_in_unsafe_fn)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
use std::{io, process::Command};

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Linux virtual-address bound. macOS rejects the finite AS request on the
    /// validated host and instead uses the separate polled resident limit.
    pub address_bytes: u64,
    /// Resident memory on Unix; job committed memory on Windows.
    pub memory_bytes: u64,
    pub cpu_seconds: u64,
    pub file_bytes: u64,
}

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::Child;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::Child;

/// Spawn a trusted direct executable with limits applied before it executes.
/// Always supplies three pipes; pipe operations return WouldBlock rather than
/// waiting. Daemonizing launchers are outside this narrow naming contract.
pub fn spawn(command: &mut Command, limits: Limits) -> io::Result<Child> {
    validate(limits)?;
    #[cfg(unix)]
    {
        unix::spawn(command, limits)
    }
    #[cfg(windows)]
    {
        windows::spawn(command, limits)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (command, limits);
        Err(io::Error::other(
            "Local naming process limits are unavailable on this platform",
        ))
    }
}

pub(crate) fn validate(limits: Limits) -> io::Result<()> {
    if limits.address_bytes == 0
        || limits.memory_bytes == 0
        || limits.cpu_seconds == 0
        || limits.file_bytes == 0
    {
        return Err(io::Error::other("Invalid child resource limits"));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
