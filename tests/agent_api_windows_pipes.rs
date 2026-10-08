//! On Windows, `reshiki --mcp` keeps the client's pipes out of the worker
//! processes it starts. `std::process::Command` lets every child inherit
//! each inheritable handle, so without that a `--inchi-worker` still running
//! when the server exits would hold the client's stdout open.
//!
//! `import` and `analyze` start `--inchi-worker` for their InChI. Once the
//! input ends, stdout must reach end of file and the server exit within
//! [`exit_bound`]: after an answered `analyze`, and while a [`HeldWorker`]
//! that outlives the server still runs `import`'s InChI. Every session runs
//! under a 60 s watchdog with an empty data folder; the first inherits
//! RESHIKI_INCHI_HELPER when it is set. release.yml also runs this file in
//! the release profile, whose binary uses the GUI subsystem, on x64 and
//! ARM64.
#![cfg(windows)]

#[path = "common/headless.rs"]
mod headless;

use headless::{McpSession, StderrMode, modern_call};
use serde_json::{Value, json};
use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};
use tempfile::TempDir;

const LINE_TIMEOUT: Duration = Duration::from_secs(10);
const WATCHDOG: Duration = Duration::from_secs(60);
/// The debug line `reshiki --mcp` logs once its piped stdin, stdout and
/// stderr are no longer inheritable.
const PROTECTED: &str = "reshiki-mcp: debug: stdio: standard pipes protected from worker processes \
                         stdin=1 stdout=1 stderr=1\n";
const ETHANOL_INCHI: &str = "InChI=1S/C2H6O/c1-2-3/h3H,2H2,1H3";
/// The folder of the [`HeldWorker`] that [`held_worker`] serves.
const HELD: &str = "RESHIKI_TEST_HELD_WORKER";
/// Marked in that folder by the worker once it runs.
const STARTED: &str = "started";
/// Marked by the test to let the worker exit.
const RELEASE: &str = "release";
/// Marked by the worker when it sees [`RELEASE`].
const RELEASED: &str = "released";

/// How long stdout may take to reach end of file, and the server to exit,
/// once the input ended: 10 s, or 20 s on ARM64.
fn exit_bound() -> Duration {
    let arm = std::env::var("PROCESSOR_ARCHITECTURE")
        .is_ok_and(|architecture| architecture.eq_ignore_ascii_case("ARM64"));
    Duration::from_secs(if arm { 20 } else { 10 })
}

/// Starts `reshiki --mcp --log-level debug` with the variables `env` and
/// waits for the protection line.
fn start(env: &[(&str, &OsStr)]) -> McpSession {
    let mut session =
        McpSession::start_with_env(&["--log-level", "debug"], env, StderrMode::Captured);
    session.await_stderr(PROTECTED, WATCHDOG);
    session
}

/// Imports ethanol, whose analysis starts `--inchi-worker`.
fn import(id: i64) -> Value {
    modern_call(id, "import", json!({"format": "smiles", "text": "CCO"}))
}

fn analyze(id: i64, document: &str) -> Value {
    modern_call(id, "analyze", json!({"document": document, "ids": null}))
}

/// Asserts that `response` is the analysis of ethanol, with the InChI the
/// worker computed.
fn assert_ethanol(response: &Value) {
    let result = &response["result"];
    assert_eq!(result["isError"], false, "{response}");
    let analysis = &result["structuredContent"]["value"]["analysis"];
    assert_eq!(analysis["formula"], "C2H6O", "{analysis}");
    assert_eq!(analysis["inchi"], ETHANOL_INCHI, "{analysis}");
}

/// Closes stdin, then asserts that the server exits and stdout reaches end
/// of file within one [`exit_bound`]. Returns the exit code and the
/// messages not received before.
fn end_input(session: &mut McpSession) -> (Option<i32>, Vec<Value>) {
    let bound = exit_bound();
    session.close_stdin();
    let started = Instant::now();
    let status = session.wait_exit(bound);
    let rest = session.rest(bound.saturating_sub(started.elapsed()));
    (status.code(), rest)
}

/// Waits up to [`LINE_TIMEOUT`] until `path` exists, else panics with
/// `failure`.
fn await_file(path: &Path, failure: &str) {
    let deadline = Instant::now() + LINE_TIMEOUT;
    while !path.exists() {
        assert!(Instant::now() < deadline, "{failure}");
        thread::sleep(Duration::from_millis(20));
    }
}

/// A stand-in `--inchi-worker` that never answers and outlives the server.
///
/// RESHIKI_INCHI_HELPER names `worker.cmd` in its folder, which runs
/// [`held_worker`] from this test executable. The server can end only its
/// child, `cmd.exe`; the test executable is its grandchild and keeps every
/// handle it inherited until [`HeldWorker::release`], or until the folder is
/// removed when a failed test drops it.
struct HeldWorker {
    dir: TempDir,
    script: PathBuf,
}

impl HeldWorker {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("a folder for the held worker");
        let path = std::env::current_exe().expect("the test executable");
        let exe = path
            .to_str()
            .filter(|exe| !exe.contains('%'))
            .unwrap_or_else(|| panic!("cmd.exe cannot run {}", path.display()));
        let script = dir.path().join("worker.cmd");
        fs::write(
            &script,
            format!("@\"{exe}\" --exact held_worker --ignored > nul 2> nul\r\n"),
        )
        .expect("write worker.cmd");
        Self { dir, script }
    }

    /// The server's variables that select this worker.
    fn env(&self) -> [(&'static str, &OsStr); 2] {
        [
            ("RESHIKI_INCHI_HELPER", self.script.as_os_str()),
            (HELD, self.dir.path().as_os_str()),
        ]
    }

    /// Waits until the worker runs.
    fn await_start(&self) {
        await_file(
            &self.dir.path().join(STARTED),
            "the held worker never started",
        );
    }

    /// Lets the worker exit, and asserts that it was still running.
    fn release(&self) {
        fs::write(self.dir.path().join(RELEASE), b"").expect("release the held worker");
        await_file(
            &self.dir.path().join(RELEASED),
            "the held worker did not outlive the server",
        );
    }
}

#[test]
fn stdout_ends_after_a_call_that_started_a_worker() {
    let mut session = start(&[]);
    session.send(import(1));
    let imported = session.recv_for(Some(&json!(1)), LINE_TIMEOUT);
    let document = imported["result"]["structuredContent"]["value"]["document"]
        .as_str()
        .unwrap_or_else(|| panic!("a document handle: {imported}"));
    session.send(analyze(2, document));
    assert_ethanol(&session.recv_for(Some(&json!(2)), LINE_TIMEOUT));
    let (code, rest) = end_input(&mut session);
    assert_eq!(code, Some(0), "{rest:?}");
    assert_eq!(rest, Vec::<Value>::new());
}

/// The input ends while `import` waits on a worker that then outlives the
/// server: the call stays unanswered, so the server exits 1 once its 6 s
/// end-of-input grace (`reshiki_mcp::server::EOF_GRACE`) runs out, and
/// stdout still reaches end of file in time.
#[test]
fn stdout_ends_while_a_worker_outlives_the_server() {
    let worker = HeldWorker::new();
    let mut session = start(&worker.env());
    session.send(import(1));
    worker.await_start();
    let (code, rest) = end_input(&mut session);
    assert_eq!(code, Some(1), "{rest:?}");
    assert_eq!(rest, Vec::<Value>::new());
    worker.release();
}

/// [`HeldWorker`]'s process: marks [`STARTED`], then waits for [`RELEASE`]
/// while its folder exists, for at most [`WATCHDOG`]. It does nothing
/// unless `worker.cmd` started it.
#[test]
#[ignore = "worker.cmd runs it as a stand-in InChI worker"]
fn held_worker() {
    let Some(dir) = std::env::var_os(HELD).map(PathBuf::from) else {
        return;
    };
    fs::write(dir.join(STARTED), b"").expect("mark the start");
    let deadline = Instant::now() + WATCHDOG;
    while Instant::now() < deadline && dir.exists() {
        if dir.join(RELEASE).exists() {
            fs::write(dir.join(RELEASED), b"").expect("mark the release");
            return;
        }
        thread::sleep(Duration::from_millis(20));
    }
}
