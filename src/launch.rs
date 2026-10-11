#![deny(clippy::print_stdout, clippy::print_stderr)]
//! Headless entry points selected by the first command-line token.
//!
//! `reshiki --mcp ...` and `reshiki --cli ...` run without a window. Every
//! other argv keeps the existing worker, Office, engine-check and GUI routing.

use reshiki::updates::CURRENT_VERSION;
use reshiki_agent::{
    access::{self, GRANT_EXIT_CODE, GrantSummary, Grants},
    ops::{budget::Budgets, headless::HeadlessHost, host::ToolHost, wire::Principal},
};
use reshiki_mcp::{
    framing::Limits,
    log::{Level, Log, UnknownLevel},
    server::{self, Identity, Quit, ServeError},
};
use std::{
    ffi::OsString,
    fmt,
    io::{self, IsTerminal, Write},
    ops::RangeInclusive,
    path::PathBuf,
    process,
    sync::Arc,
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

/// The environment variable that sets the heap ceiling of `--mcp` and
/// `--cli`.
const HEAP_VARIABLE: &str = "RESHIKI_AGENT_HEAP_MB";
const MIB: usize = 1024 * 1024;
/// The heap ceiling without [`HEAP_VARIABLE`]: 2 GiB.
const DEFAULT_HEAP_MIB: usize = 2048;
/// The ceilings [`HEAP_VARIABLE`] may set, in MiB.
const HEAP_MIB: RangeInclusive<usize> = 256..=16384;

/// The heap ceiling in bytes for `--mcp` and `--cli`, from the value of
/// [`HEAP_VARIABLE`]: 2 GiB when it is unset, else its whole number of MiB
/// from 256 to 16384.
///
/// Past the ceiling the bounded allocator prints `RESHIKI_HEAP_LIMIT ...` to
/// stderr and exits 75 (`reshiki_process_heap::RESOURCE_EXIT`).
fn heap_budget(value: Option<OsString>) -> Result<usize, String> {
    let Some(value) = value else {
        return Ok(DEFAULT_HEAP_MIB * MIB);
    };
    value
        .to_str()
        .filter(|text| text.bytes().all(|byte| byte.is_ascii_digit()))
        .and_then(|text| text.parse::<usize>().ok())
        .filter(|mib| HEAP_MIB.contains(mib))
        .and_then(|mib| mib.checked_mul(MIB))
        .ok_or_else(|| format!("{HEAP_VARIABLE} must be a whole number of MiB from 256 to 16384"))
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

/// Keeps the standard handles out of the worker processes this mode starts
/// (`--inchi-worker`, `--geometry-worker`), so a worker still running when
/// this process exits cannot hold a client's pipe open. On failure prints
/// one stderr line and exits 1.
#[cfg(windows)]
fn protect_standard_handles() -> reshiki_windows::Protected {
    match reshiki_windows::disinherit_standard_handles() {
        Ok(protected) => protected,
        Err(error) => {
            let _ = writeln!(
                io::stderr(),
                "reshiki: could not protect the client's pipes from worker processes: {error}"
            );
            process::exit(1)
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
            process::exit(reshiki::cli::USAGE)
        }
    }
    // The blocking pool runs the in-process host's operations, with room
    // for two more.
    let runtime = match runtime(Budgets::default().concurrency.saturating_add(2)) {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = writeln!(io::stderr(), "reshiki: could not start runtime: {error}");
            process::exit(1)
        }
    };
    let code = runtime.block_on(reshiki::cli::run(command));
    exit(runtime, code, Duration::from_secs(1))
}

const MCP_USAGE: &str = "\
Experimental: ReShiki's agent tools, schemas and results may change between releases.
Usage: reshiki --mcp [--log-level <level>] [--allow-read <folder>]... [--allow-write <folder>]...

Serves the Model Context Protocol on stdin and stdout; it is meant to be
launched by an MCP client. Diagnostics go to stderr.

Options:
  --log-level <level>     error, warn (default), info or debug
  --allow-read <folder>   Let file_open read files in <folder>; repeatable
  --allow-write <folder>  Let file_save write files in <folder>; repeatable
  -h, --help              Show this help

Folders listed in agent-access.json in ReShiki's data folder are granted too.
";

/// The startup banner, logged whatever the level.
const BANNER: &str = "ReShiki agent API (experimental)";

/// How long shutdown may wait for running calls to end, the output to
/// drain and the log to stop.
const SHUTDOWN: Duration = Duration::from_secs(3);
/// The longest wait for running calls to end once serving stopped.
const DRAIN_GRACE: Duration = Duration::from_secs(1);
/// The longest wait for the log to flush before exiting.
const LOG_GRACE: Duration = Duration::from_millis(200);
/// How long the runtime's leftover tasks get before the process exits.
const RUNTIME_GRACE: Duration = Duration::from_millis(500);

/// What `reshiki --mcp <args>` asks for.
#[derive(Debug, PartialEq, Eq)]
enum McpRequest {
    Serve {
        level: Level,
        /// `--allow-read` folders, in order.
        read: Vec<PathBuf>,
        /// `--allow-write` folders, in order.
        write: Vec<PathBuf>,
    },
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
    /// `--allow-read` or `--allow-write` without a folder, or with an empty
    /// one.
    MissingFolder(&'static str),
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
            Self::MissingFolder(option) => write!(f, "reshiki --mcp: {option} needs a folder"),
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

/// Parses the tokens after `--mcp`. `--allow-read` and `--allow-write` may
/// repeat, every other option appears at most once, and the first problem
/// wins; `--help` counts only when nothing is wrong.
fn mcp_request(args: &[OsString]) -> Result<McpRequest, McpUsageError> {
    let mut level = None;
    let mut help = false;
    let mut read = Vec::new();
    let mut write = Vec::new();
    let mut tokens = args.iter();
    while let Some(token) = tokens.next() {
        let folders = if token == "--allow-read" {
            Some(("--allow-read", &mut read))
        } else if token == "--allow-write" {
            Some(("--allow-write", &mut write))
        } else {
            None
        };
        if let Some((option, folders)) = folders {
            let folder = tokens.next().filter(|folder| !folder.is_empty());
            folders.push(PathBuf::from(
                folder.ok_or(McpUsageError::MissingFolder(option))?,
            ));
        } else if token == "--log-level" {
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
            read,
            write,
        }
    })
}

/// The folders granted on the command line and in `agent-access.json`;
/// relative command-line folders resolve against the current folder.
fn load_grants(read: Vec<PathBuf>, write: Vec<PathBuf>) -> Result<Grants, String> {
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(_) if read.iter().chain(&write).all(|folder| folder.is_absolute()) => PathBuf::new(),
        Err(error) => return Err(format!("cannot resolve relative folders: {error}")),
    };
    access::load(read, write, cwd).map_err(|error| error.to_string())
}

/// The startup banner's value: the version and the granted folders, counted,
/// or listed at debug level. Paths never reach stderr otherwise.
fn banner(version: &str, grants: &GrantSummary, level: Level) -> String {
    if level == Level::Debug {
        // Debug formatting escapes control characters, keeping one line.
        format!(
            "{version}; granted folders: read {:?}, write {:?}",
            grants.read, grants.write
        )
    } else {
        format!(
            "{version}; granted folders: {} read, {} write",
            grants.read.len(),
            grants.write.len()
        )
    }
}

/// `reshiki --mcp`: serves MCP on stdin and stdout until stdin ends or
/// stdout fails, then exits 0 if every response was delivered, else 1.
///
/// On Windows the standard handles are first kept out of worker processes;
/// failing that prints one stderr line and exits 1.
///
/// The heap ceiling starts and the granted folders are opened before
/// anything is read: an invalid [`HEAP_VARIABLE`], a folder that cannot be
/// granted, or an unreadable `agent-access.json`, prints one stderr line and
/// exits 2 ([`GRANT_EXIT_CODE`]).
///
/// Stdout carries MCP messages only: the framing's writer thread takes its
/// lock for the whole connection, and nothing else here writes it. Stderr
/// gets usage and grant errors before the log starts and only the bounded,
/// content-free log after, so neither stream can block shutdown once the
/// input ended: stdin is read on its own thread, every wait below shares
/// one deadline, and [`exit`] never locks a std stream.
///
/// A client that stops reading stdout also stops the reader (the framing's
/// backpressure), so input it sent after that point, including its end, is
/// read only once it reads stdout again or closes it; closing it fails the
/// writer and exits 1.
pub(crate) fn mcp(args: Vec<OsString>) -> ! {
    #[cfg(windows)]
    let protected = protect_standard_handles();
    let (level, read, write) = match mcp_request(&args) {
        Ok(McpRequest::Serve { level, read, write }) => (level, read, write),
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
    match heap_budget(std::env::var_os(HEAP_VARIABLE)) {
        Ok(bytes) => reshiki_process_heap::begin(bytes),
        Err(error) => {
            let _ = writeln!(io::stderr(), "reshiki --mcp: {error}");
            process::exit(2)
        }
    }
    let grants = match load_grants(read, write) {
        Ok(grants) => grants,
        Err(error) => {
            // One line, even for a folder name with a line break in it.
            let error: String = error
                .chars()
                .map(|c| if c.is_control() { ' ' } else { c })
                .collect();
            let _ = writeln!(io::stderr(), "reshiki --mcp: {error}");
            process::exit(GRANT_EXIT_CODE)
        }
    };
    let (log, log_done) = Log::start(io::stderr, level);
    reshiki_mcp::install_panic_hook(log.clone());
    log.notice(BANNER, &banner(CURRENT_VERSION, &grants.summary(), level));
    #[cfg(windows)]
    log.event(
        Level::Debug,
        "stdio: standard pipes protected from worker processes",
        &[
            ("stdin", u64::from(protected.stdin)),
            ("stdout", u64::from(protected.stdout)),
            ("stderr", u64::from(protected.stderr)),
        ],
    );
    if io::stdin().is_terminal() {
        log.event(
            Level::Warn,
            "reshiki --mcp speaks MCP on stdin/stdout and is meant to be launched by an MCP client",
            &[],
        );
    }
    let budgets = Budgets::default();
    // The blocking pool runs the host's operations, with room for two more;
    // stdin and stdout have their own threads and never use it.
    let runtime = match runtime(budgets.concurrency.saturating_add(2)) {
        Ok(runtime) => runtime,
        Err(_) => {
            log.event(Level::Error, "could not start the runtime", &[]);
            log_done.finish(LOG_GRACE);
            process::exit(1)
        }
    };
    let limits = Limits::from_budgets(&budgets);
    let host = Arc::new(HeadlessHost::new(CURRENT_VERSION, budgets).with_grants(Arc::new(grants)));
    let result = runtime.block_on(server::serve(
        Arc::clone(&host),
        Principal::local(),
        io::stdin(),
        || io::stdout().lock(),
        Identity {
            app_version: CURRENT_VERSION.into(),
        },
        limits,
        log.clone(),
    ));
    // One deadline, set before any waiting, bounds the rest of shutdown.
    let started = Instant::now();
    let remaining = || SHUTDOWN.saturating_sub(started.elapsed());
    // Running calls are cancelled; their blocking work may outlast this.
    let drain = remaining().min(DRAIN_GRACE);
    let drained =
        runtime.block_on(async { tokio::time::timeout(drain, host.drained()).await.is_ok() });
    if !drained {
        log.event(Level::Warn, "calls still running at exit", &[]);
    }
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
        Err(ServeError::Init | ServeError::Catalog) => 1,
    };
    log_done.finish(remaining().min(LOG_GRACE));
    exit(runtime, code, RUNTIME_GRACE)
}

/// The exit code once serving ended: 0 only when the input ended, the writer
/// finished with everything queued written (`delivered`) and no request the
/// client did not cancel is `unanswered`. Once the writer finished, nothing
/// can answer such a request: rmcp abandoned its response while stdout was
/// blocked, the transport abandoned the error reply it owed, or the request
/// was dropped with the service.
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
