#![deny(clippy::print_stdout, clippy::print_stderr)]
//! Headless entry points selected by the first command-line token.
//!
//! `reshiki --mcp ...` and `reshiki --cli ...` run without a window. Every
//! other argv keeps the existing worker, Office, engine-check and GUI routing.

use reshiki::updates::CURRENT_VERSION;
use reshiki_mcp::{
    framing::Limits,
    log::{Level, Log, UnknownLevel},
    server::{self, Identity, Quit, ServeError},
};
use std::{
    ffi::OsString,
    fmt,
    io::{self, IsTerminal, Write},
    process,
    time::{Duration, Instant},
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

const MCP_USAGE: &str = "\
Experimental: ReShiki's agent tools, schemas and results may change between releases.
Usage: reshiki --mcp [--log-level <level>]

Serves the Model Context Protocol on stdin and stdout; it is meant to be
launched by an MCP client. Diagnostics go to stderr.

Options:
  --log-level <level>  error, warn (default), info or debug
  -h, --help           Show this help
";

/// The startup banner, logged whatever the level.
const BANNER: &str = "ReShiki agent API (experimental)";

/// How long shutdown may wait for the output to drain and the log to stop.
const SHUTDOWN: Duration = Duration::from_secs(3);
/// The longest wait for the log to flush before exiting.
const LOG_GRACE: Duration = Duration::from_millis(200);
/// How long the runtime's leftover tasks get before the process exits.
const RUNTIME_GRACE: Duration = Duration::from_millis(500);

/// What `reshiki --mcp <args>` asks for.
#[derive(Debug, PartialEq, Eq)]
enum McpRequest {
    Serve { level: Level },
    Help,
}

/// Why `reshiki --mcp <args>` cannot start; each exits 2.
#[derive(Debug, PartialEq, Eq)]
enum McpUsageError {
    /// `--attach` is reserved for connecting to a running app.
    Attach,
    /// An option given more than once.
    Duplicate(&'static str),
    /// `--log-level` without a value.
    MissingLevel,
    /// `--log-level` with an unknown value, lossily decoded.
    InvalidLevel(String),
    /// Any other token, lossily decoded.
    Unknown(String),
}

impl fmt::Display for McpUsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Attach => f.write_str(
                "reshiki --mcp --attach connects to a running ReShiki app and is not available in \
                 this version",
            ),
            Self::Duplicate(option) => write!(f, "reshiki --mcp: {option} given more than once"),
            Self::MissingLevel => f.write_str("reshiki --mcp: --log-level needs a value"),
            Self::InvalidLevel(level) => {
                write!(
                    f,
                    "reshiki --mcp: unknown log level `{level}`; {UnknownLevel}"
                )
            }
            Self::Unknown(token) => write!(f, "reshiki --mcp: unknown option `{token}`"),
        }
    }
}

/// Parses the tokens after `--mcp`. Each option may appear at most once and
/// the first problem wins; `--help` counts only when nothing is wrong.
fn mcp_request(args: &[OsString]) -> Result<McpRequest, McpUsageError> {
    let mut level = None;
    let mut help = false;
    let mut tokens = args.iter();
    while let Some(token) = tokens.next() {
        if token == "--log-level" {
            if level.is_some() {
                return Err(McpUsageError::Duplicate("--log-level"));
            }
            let value = tokens.next().ok_or(McpUsageError::MissingLevel)?;
            let parsed = value.to_str().and_then(|text| text.parse().ok());
            level = Some(
                parsed
                    .ok_or_else(|| McpUsageError::InvalidLevel(value.to_string_lossy().into()))?,
            );
        } else if token == "--help" || token == "-h" {
            if help {
                return Err(McpUsageError::Duplicate("--help"));
            }
            help = true;
        } else if token == "--attach" {
            return Err(McpUsageError::Attach);
        } else {
            return Err(McpUsageError::Unknown(token.to_string_lossy().into()));
        }
    }
    Ok(if help {
        McpRequest::Help
    } else {
        McpRequest::Serve {
            level: level.unwrap_or(Level::Warn),
        }
    })
}

/// `reshiki --mcp`: serves MCP on stdin and stdout until stdin ends or
/// stdout fails, then exits 0 if every response was delivered, else 1.
///
/// Stdout carries MCP messages only: the framing's writer thread takes its
/// lock for the whole connection, and nothing else here writes it. Stderr
/// gets usage errors before the log starts and only the bounded,
/// content-free log after, so neither stream can block shutdown once the
/// input ended: stdin is read on its own thread, every wait below shares
/// one deadline, and [`exit`] never locks a std stream.
///
/// A client that stops reading stdout also stops the reader (the framing's
/// backpressure), so input it sent after that point, including its end, is
/// read only once it reads stdout again or closes it; closing it fails the
/// writer and exits 1.
pub(crate) fn mcp(args: Vec<OsString>) -> ! {
    let level = match mcp_request(&args) {
        Ok(McpRequest::Serve { level }) => level,
        Ok(McpRequest::Help) => {
            let _ = io::stderr().write_all(MCP_USAGE.as_bytes());
            process::exit(0)
        }
        Err(error) => {
            let mut stderr = io::stderr().lock();
            let _ = writeln!(stderr, "{error}");
            if error != McpUsageError::Attach {
                let _ = stderr.write_all(MCP_USAGE.as_bytes());
            }
            process::exit(2)
        }
    };
    let (log, log_done) = Log::start(io::stderr, level);
    reshiki_mcp::install_panic_hook(log.clone());
    log.notice(BANNER, CURRENT_VERSION);
    if io::stdin().is_terminal() {
        log.event(
            Level::Warn,
            "reshiki --mcp speaks MCP on stdin/stdout and is meant to be launched by an MCP client",
            &[],
        );
    }
    let runtime = match runtime(4) {
        Ok(runtime) => runtime,
        Err(_) => {
            log.event(Level::Error, "could not start the runtime", &[]);
            log_done.finish(LOG_GRACE);
            process::exit(1)
        }
    };
    let result = runtime.block_on(server::serve(
        io::stdin(),
        || io::stdout().lock(),
        Identity {
            app_version: CURRENT_VERSION.into(),
        },
        Limits::default(),
        log.clone(),
    ));
    // One deadline, set before any waiting, bounds the rest of shutdown.
    let started = Instant::now();
    let remaining = || SHUTDOWN.saturating_sub(started.elapsed());
    let code = match result {
        Ok(finished) => {
            let delivered = finished.writer.wait(remaining());
            exit_code(
                finished.reason,
                delivered,
                finished.tracker.unanswered(),
                &log,
            )
        }
        // `serve` already logged the failure at error level.
        Err(ServeError::Init) => 1,
    };
    log_done.finish(remaining().min(LOG_GRACE));
    exit(runtime, code, RUNTIME_GRACE)
}

/// The exit code once serving ended: 0 only when the input ended, the writer
/// finished with everything queued written (`delivered`) and no request the
/// client did not cancel is `unanswered`. Once the writer finished, nothing
/// can answer such a request: rmcp abandoned its response while stdout was
/// blocked, or dropped the request with the service.
fn exit_code(reason: Quit, delivered: bool, unanswered: usize, log: &Log) -> i32 {
    match reason {
        Quit::WriterFailed => 1,
        Quit::Eof if !delivered => {
            log.event(Level::Warn, "output not delivered before exit", &[]);
            1
        }
        Quit::Eof if unanswered > 0 => {
            let requests = u64::try_from(unanswered).unwrap_or(u64::MAX);
            log.event(
                Level::Warn,
                "requests left unanswered",
                &[("requests", requests)],
            );
            1
        }
        Quit::Eof => 0,
    }
}

#[cfg(test)]
mod tests;
