#![deny(clippy::print_stdout, clippy::print_stderr)]
//! Headless entry points selected by the first command-line token.
//!
//! `reshiki --mcp ...` and `reshiki --cli ...` run without a window. Every
//! other argv keeps the existing worker, Office, engine-check and GUI routing.

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
/// headless mode and keeps the remaining tokens verbatim; anything else,
/// including no token, `--mcp=1`, `--MCP` or a later `--mcp`, is
/// [`Launch::Gui`].
///
/// `main` calls this after the worker dispatch (geometry, LibreOffice, InChI,
/// clipboard and print workers), so workers keep their first-token priority.
/// Existing launchers never put these tokens first: the Windows installer,
/// OLE registration, the Office and LibreOffice companions and the updater
/// scripts all start with `--open` or an Office flag.
pub(crate) fn mode(args: impl IntoIterator<Item = OsString>) -> Launch {
    let mut args = args.into_iter();
    match args.next() {
        Some(first) if first == "--mcp" => Launch::Mcp(args.collect()),
        Some(first) if first == "--cli" => Launch::Cli(args.collect()),
        _ => Launch::Gui,
    }
}

/// The headless runtime: two workers and at least two blocking threads.
pub(crate) fn runtime(blocking: usize) -> io::Result<Runtime> {
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
pub(crate) fn exit(runtime: Runtime, code: i32, grace: Duration) -> ! {
    runtime.shutdown_timeout(grace);
    process::exit(code)
}

/// `reshiki --cli`: runs one command and exits with its code.
pub(crate) fn cli(args: Vec<OsString>) -> ! {
    let runtime = match runtime(4) {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = writeln!(io::stderr(), "reshiki: could not start runtime: {error}");
            process::exit(1)
        }
    };
    let code = runtime.block_on(reshiki::cli::run(args));
    exit(runtime, code, Duration::from_secs(1))
}

/// `reshiki --mcp`: a placeholder until the stdio server lands.
pub(crate) fn mcp(_args: Vec<OsString>) -> ! {
    let _ = writeln!(io::stderr(), "reshiki --mcp: not available in this build");
    process::exit(2)
}

#[cfg(test)]
mod tests;
