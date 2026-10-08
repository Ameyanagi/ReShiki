//! On Windows, `reshiki --mcp` keeps the client's pipes out of the worker
//! processes it starts. `std::process::Command` lets every child inherit
//! each inheritable handle, so without that a `--inchi-worker` still running
//! when the server exits would hold the client's stdout open.
//!
//! `analyze` starts `--inchi-worker` for its InChI. Once the input ends,
//! stdout must reach end of file and the server exit within [`exit_bound`],
//! whether the call was answered or still running. Every session runs under
//! a 60 s watchdog with an empty data folder and inherits
//! RESHIKI_INCHI_HELPER when it is set. release.yml also runs this file in
//! the release profile, whose binary uses the GUI subsystem, on x64 and
//! ARM64.
#![cfg(windows)]

#[path = "common/headless.rs"]
mod headless;

use headless::{McpSession, StderrMode, modern_call, modern_list};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

const LINE_TIMEOUT: Duration = Duration::from_secs(10);
const WATCHDOG: Duration = Duration::from_secs(60);
/// The debug line `reshiki --mcp` logs once its piped stdin, stdout and
/// stderr are no longer inheritable.
const PROTECTED: &str = "reshiki-mcp: debug: stdio: standard pipes protected from worker processes \
                         stdin=1 stdout=1 stderr=1\n";
const ETHANOL_INCHI: &str = "InChI=1S/C2H6O/c1-2-3/h3H,2H2,1H3";

/// How long stdout may take to reach end of file, and the server to exit,
/// once the input ended: 10 s, or 20 s on ARM64.
fn exit_bound() -> Duration {
    let arm = std::env::var("PROCESSOR_ARCHITECTURE")
        .is_ok_and(|architecture| architecture.eq_ignore_ascii_case("ARM64"));
    Duration::from_secs(if arm { 20 } else { 10 })
}

/// Starts `reshiki --mcp --log-level debug`, waits for the protection line
/// and imports ethanol; returns the session and the document's handle.
fn start() -> (McpSession, String) {
    let mut session = McpSession::start(&["--log-level", "debug"], StderrMode::Captured);
    session.await_stderr(PROTECTED, WATCHDOG);
    session.send(modern_call(
        1,
        "import",
        json!({"format": "smiles", "text": "CCO"}),
    ));
    let imported = session.recv_for(Some(&json!(1)), LINE_TIMEOUT);
    let document = imported["result"]["structuredContent"]["value"]["document"]
        .as_str()
        .unwrap_or_else(|| panic!("a document handle: {imported}"))
        .to_owned();
    (session, document)
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

#[test]
fn stdout_ends_after_a_call_that_started_a_worker() {
    let (mut session, document) = start();
    session.send(analyze(2, &document));
    assert_ethanol(&session.recv_for(Some(&json!(2)), LINE_TIMEOUT));
    let (code, rest) = end_input(&mut session);
    assert_eq!(code, Some(0), "{rest:?}");
    assert_eq!(rest, Vec::<Value>::new());
}

/// The input ends while `analyze` runs: the server exits 0 only if it
/// answered the call.
#[test]
fn stdout_ends_when_the_input_ends_during_a_call() {
    let (mut session, document) = start();
    session.send(analyze(2, &document));
    // The service takes requests in order and starts each call on its own
    // task, so this answer shows the analysis started.
    session.send(modern_list(3));
    session.recv_for(Some(&json!(3)), LINE_TIMEOUT);
    let (code, rest) = end_input(&mut session);
    let answered = rest.iter().find(|message| message["id"] == 2);
    if let Some(response) = answered {
        assert_ethanol(response);
    }
    assert_eq!(
        code,
        Some(if answered.is_some() { 0 } else { 1 }),
        "{rest:?}"
    );
}
