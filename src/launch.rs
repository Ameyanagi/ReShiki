#![deny(clippy::print_stdout, clippy::print_stderr)]
//! Headless entry points selected by the first command-line token.
//!
//! `reshiki --mcp ...` and `reshiki --cli ...` run without a window. Every
//! other argv keeps the existing worker, Office, engine-check and GUI routing.

mod mcp;

pub(crate) use mcp::mcp;
use reshiki::cli::{
    FAILURE, USAGE,
    heap::{DEFAULT_HEAP_MIB, HEAP_MIB, HEAP_VARIABLE},
};
use reshiki_agent::ops::budget::Budgets;
use std::{
    ffi::OsString,
    io::{self, Write},
    process,
    time::Duration,
};
use tokio::runtime::{Builder, Runtime};

/// How this process starts.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Launch {
    /// The existing routing: Windows flags, `--engine-check` and the GUI.
    Gui,
    /// `reshiki --mcp`, with the tokens after it.
    Mcp(Vec<OsString>),
    /// `reshiki --cli`, with the tokens after it.
    Cli(Vec<OsString>),
}

/// Selects the launch mode from the arguments after the executable name.
///
/// Only the first token is examined. Exactly `--mcp` or `--cli` selects a
/// headless mode and keeps the remaining tokens verbatim, and `--version`
/// runs as `reshiki --cli --version`; anything else,
/// including no token, `--mcp=1`, `--MCP` or a later `--mcp`, is
/// [`Launch::Gui`].
///
/// `main` calls this after the worker dispatch (EMF, naming, geometry,
/// LibreOffice, InChI, clipboard and print workers), so workers keep their
/// first-token priority.
/// Existing launchers never put these tokens first: the Windows installer,
/// OLE registration, the Office and LibreOffice companions and the updater
/// scripts all start with `--open` or an Office flag.
pub(crate) fn mode(args: impl IntoIterator<Item = OsString>) -> Launch {
    let mut args = args.into_iter();
    match args.next() {
        Some(first) if first == "--mcp" => Launch::Mcp(args.collect()),
        Some(first) if first == "--cli" => Launch::Cli(args.collect()),
        Some(first) if first == "--version" => {
            Launch::Cli(std::iter::once(first).chain(args).collect())
        }
        _ => Launch::Gui,
    }
}

/// Inspect GUI flags without treating a native filename consumed by `--open`
/// as a mode switch (and without converting native paths to UTF-8).
pub(crate) fn gui_flag(args: impl IntoIterator<Item = OsString>, flag: &str) -> bool {
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg == "--open" {
            let _ = args.next();
        } else if arg == flag {
            return true;
        }
    }
    false
}

const MIB: usize = 1024 * 1024;

/// The heap ceiling in bytes for `--mcp` and `--cli`, from the value of
/// [`HEAP_VARIABLE`]: 2 GiB when it is unset, else its whole number of MiB
/// from 256 to 16384.
///
/// Past the ceiling the bounded allocator prints `RESHIKI_HEAP_LIMIT ...` to
/// stderr and exits 75 (`reshiki_process_heap::RESOURCE_EXIT`).
pub(super) fn heap_budget(value: Option<OsString>) -> Result<usize, String> {
    let Some(value) = value else {
        return Ok(DEFAULT_HEAP_MIB * MIB);
    };
    value
        .to_str()
        .filter(|text| text.bytes().all(|byte| byte.is_ascii_digit()))
        .and_then(|text| text.parse::<usize>().ok())
        .filter(|mib| HEAP_MIB.contains(mib))
        .and_then(|mib| mib.checked_mul(MIB))
        .ok_or_else(|| {
            format!(
                "{HEAP_VARIABLE} must be a whole number of MiB from {} to {}",
                HEAP_MIB.start(),
                HEAP_MIB.end(),
            )
        })
}

/// The headless runtime: two workers and at least two blocking threads.
pub(super) fn runtime(blocking: usize) -> io::Result<Runtime> {
    Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(blocking.max(2))
        .thread_name("reshiki-headless")
        .enable_all()
        .build()
}

/// Shuts `runtime` down within `grace` and exits with `code`.
///
/// Dropping a `Runtime` would wait for its blocking tasks without a bound.
/// This never locks, writes or flushes stdout or stderr.
pub(super) fn exit(runtime: Runtime, code: i32, grace: Duration) -> ! {
    runtime.shutdown_timeout(grace);
    process::exit(code)
}

/// Keeps the standard handles out of the worker processes this mode starts
/// (`--inchi-worker`, `--geometry-worker`), so a worker still running when
/// this process exits cannot hold a client's pipe open. On failure prints
/// one stderr line and exits 1.
#[cfg(windows)]
pub(super) fn protect_standard_handles() -> reshiki_windows::Protected {
    match reshiki_windows::disinherit_standard_handles() {
        Ok(protected) => protected,
        Err(error) => {
            let _ = writeln!(
                io::stderr(),
                "reshiki: could not protect the client's pipes from worker processes: {error}"
            );
            process::exit(FAILURE)
        }
    }
}

/// `reshiki --cli`: runs one command and exits with its code.
///
/// On Windows the standard handles are first kept out of worker processes;
/// failing that exits 1. The command line is parsed next, so its usage
/// errors take precedence; then an invalid [`HEAP_VARIABLE`] is a usage
/// error too: one stderr line, exit 2.
pub(crate) fn cli(args: Vec<OsString>) -> ! {
    #[cfg(windows)]
    protect_standard_handles();
    let command = match reshiki::cli::parse(&args, &mut io::stderr()) {
        Ok(command) => command,
        Err(code) => process::exit(code),
    };
    match heap_budget(std::env::var_os(HEAP_VARIABLE)) {
        Ok(bytes) => reshiki_process_heap::begin(bytes),
        Err(error) => {
            let _ = writeln!(io::stderr(), "reshiki: {error}");
            process::exit(USAGE)
        }
    }
    // The blocking pool runs the in-process host's operations, with room
    // for two more.
    let runtime = match runtime(Budgets::default().concurrency.saturating_add(2)) {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = writeln!(io::stderr(), "reshiki: could not start runtime: {error}");
            process::exit(FAILURE)
        }
    };
    let code = runtime.block_on(reshiki::cli::run(command));
    exit(runtime, code, Duration::from_secs(1))
}

#[cfg(test)]
mod tests;
