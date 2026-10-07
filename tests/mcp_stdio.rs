//! `reshiki --mcp` served over the real process's stdin and stdout.
//!
//! Every session runs under a 60 s watchdog. Stdout must carry only
//! JSON-RPC 2.0 objects, one per line, and stderr never carries client
//! content.
#[path = "common/headless.rs"]
mod headless;

use headless::{McpSession, StderrMode};
use serde_json::{Value, json};
use std::{
    process::{Output, Stdio},
    thread,
    time::{Duration, Instant},
};

const LINE_TIMEOUT: Duration = Duration::from_secs(10);
const WATCHDOG: Duration = Duration::from_secs(60);
const CANARY: &str = "RESHIKI-CANARY-7f3a";
const BANNER: &str = "reshiki-mcp: info: ReShiki agent API (experimental) ";

fn modern_meta() -> Value {
    json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientCapabilities": {},
    })
}

fn discover(id: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "server/discover",
        "params": {"_meta": modern_meta()},
    })
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Runs `reshiki <args>` with no input and collects its output.
fn run(args: &[&str]) -> Output {
    headless::wait_with_watchdog(headless::spawn(args, Stdio::null()), WATCHDOG)
}

/// Runs a fixture: `> ` lines are sent as they are and each `< ` line is
/// compared, after normalization, with the response carrying its id. The
/// server must then exit 0 on EOF with nothing else written.
fn run_fixture(name: &str, transcript: &str) {
    let mut session = McpSession::start(&[], StderrMode::Captured);
    for (index, line) in transcript.lines().enumerate() {
        let context = format!("{name}:{}", index + 1);
        if let Some(sent) = line.strip_prefix("> ") {
            session.send_raw(format!("{sent}\n").as_bytes());
        } else if let Some(expected) = line.strip_prefix("< ") {
            let expected: Value = serde_json::from_str(expected)
                .unwrap_or_else(|error| panic!("{context}: bad golden: {error}"));
            let actual = session.recv_for(expected.get("id"), LINE_TIMEOUT);
            assert_eq!(McpSession::normalize(actual), expected, "{context}");
        } else {
            assert!(
                line.is_empty() || line.starts_with('#'),
                "{context}: unknown line"
            );
        }
    }
    session.close_stdin();
    assert_eq!(session.wait_exit(LINE_TIMEOUT).code(), Some(0), "{name}");
    assert_eq!(session.rest(LINE_TIMEOUT), Vec::<Value>::new(), "{name}");
    let stderr = session.stderr(LINE_TIMEOUT);
    assert_eq!(
        stderr,
        format!("{BANNER}{}\n", env!("CARGO_PKG_VERSION")),
        "{name}"
    );
}

#[test]
fn modern_fixture() {
    run_fixture(
        "stdio-modern.jsonl",
        include_str!("fixtures/mcp/stdio-modern.jsonl"),
    );
}

#[test]
fn legacy_fixture() {
    run_fixture(
        "stdio-legacy.jsonl",
        include_str!("fixtures/mcp/stdio-legacy.jsonl"),
    );
}

/// Timed from the startup banner, so a slow first launch of a freshly
/// built binary does not count.
#[test]
fn an_idle_eof_exits_promptly() {
    let mut session = McpSession::start(&[], StderrMode::Captured);
    session.await_stderr(BANNER, WATCHDOG);
    session.close_stdin();
    assert_eq!(session.wait_exit(Duration::from_secs(3)).code(), Some(0));
    assert_eq!(session.rest(LINE_TIMEOUT), Vec::<Value>::new());
}

#[test]
fn a_ping_right_before_eof_is_answered() {
    let mut session = McpSession::start(&[], StderrMode::Captured);
    session.await_stderr(BANNER, WATCHDOG);
    session.send(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "ping",
        "params": {"_meta": modern_meta()},
    }));
    session.close_stdin();
    let started = Instant::now();
    let status = session.wait_exit(Duration::from_secs(8));
    assert_eq!(status.code(), Some(0), "after {:?}", started.elapsed());
    // Before any lifecycle started, rmcp answers ping itself.
    let response = session.recv_for(Some(&json!(7)), LINE_TIMEOUT);
    assert_eq!(response, json!({"jsonrpc": "2.0", "id": 7, "result": {}}));
    assert_eq!(session.rest(LINE_TIMEOUT), Vec::<Value>::new());
}

#[test]
fn framing_errors_omit_the_id_and_serving_continues() {
    let mut session = McpSession::start(&[], StderrMode::Captured);
    session.send_raw(b"garbage\n");
    session.send_raw(b"[1]\n");
    session.send(discover(json!(1)));
    for code in [-32700, -32600] {
        let error = session.recv_for(None, LINE_TIMEOUT);
        assert_eq!(error["error"]["code"], code, "{error}");
    }
    let response = session.recv_for(Some(&json!(1)), LINE_TIMEOUT);
    assert_eq!(
        response["result"]["_meta"]["io.modelcontextprotocol/serverInfo"]["name"], "reshiki",
        "{response}"
    );
    session.close_stdin();
    assert_eq!(session.wait_exit(LINE_TIMEOUT).code(), Some(0));
    assert_eq!(session.rest(LINE_TIMEOUT), Vec::<Value>::new());
}

#[test]
fn usage_errors_exit_2_without_output() {
    for args in [
        &["--mcp", "--attach"][..],
        &["--mcp", "--bogus"],
        &["--mcp", "--cli"],
        &["--mcp", "--log-level"],
        &["--mcp", "--log-level", "loud"],
        &["--mcp", "--log-level", "warn", "--log-level", "debug"],
    ] {
        let output = run(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(
            output.stdout.is_empty(),
            "{args:?}: {}",
            text(&output.stdout)
        );
        assert!(!output.stderr.is_empty(), "{args:?}");
    }
    let output = run(&["--mcp", "--attach"]);
    assert_eq!(
        text(&output.stderr),
        "reshiki --mcp --attach connects to a running ReShiki app and is not available in this \
         version\n"
    );
    let output = run(&["--mcp", "--bogus"]);
    let stderr = text(&output.stderr);
    assert!(
        stderr.starts_with("reshiki --mcp: unknown option `--bogus`\n"),
        "{stderr}"
    );
    assert!(stderr.contains("Usage: reshiki --mcp"), "{stderr}");
}

#[test]
fn help_goes_to_stderr() {
    for flag in ["--help", "-h"] {
        let output = run(&["--mcp", flag]);
        assert_eq!(output.status.code(), Some(0), "{flag}");
        assert!(output.stdout.is_empty(), "{flag}: {}", text(&output.stdout));
        let stderr = text(&output.stderr);
        assert!(stderr.starts_with("Experimental: "), "{flag}: {stderr}");
        assert!(stderr.contains("Usage: reshiki --mcp"), "{flag}: {stderr}");
    }
}

/// Client text never reaches stderr, even at debug level: not in a line
/// that fails to parse, a tool name or a request id.
#[test]
fn stderr_never_carries_client_content() {
    let mut session = McpSession::start(&["--log-level", "debug"], StderrMode::Captured);
    session.send_raw(format!("{CANARY} is not JSON\n").as_bytes());
    session.send(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {"name": CANARY, "arguments": {}, "_meta": modern_meta()},
    }));
    session.send(discover(json!(CANARY)));
    assert_eq!(
        session.recv_for(None, LINE_TIMEOUT)["error"]["code"],
        -32700
    );
    let unknown = session.recv_for(Some(&json!(1)), LINE_TIMEOUT);
    assert_eq!(unknown["error"]["code"], -32602, "{unknown}");
    let answered = session.recv_for(Some(&json!(CANARY)), LINE_TIMEOUT);
    assert!(answered.get("result").is_some(), "{answered}");
    session.close_stdin();
    assert_eq!(session.wait_exit(LINE_TIMEOUT).code(), Some(0));
    let stderr = session.stderr(LINE_TIMEOUT);
    assert!(
        stderr.contains("reshiki-mcp: debug: parse error\n"),
        "{stderr}"
    );
    assert!(!stderr.contains(CANARY), "{stderr}");
}

/// A client that never reads stderr cannot stall serving or shutdown, even
/// when every line logs at debug level. 4000 lines log about 128 KB, more
/// than a default pipe holds.
#[test]
fn an_unread_stderr_never_blocks() {
    let mut session = McpSession::start(&["--log-level", "debug"], StderrMode::Unread);
    for _ in 0..4000 {
        session.send_raw(b"garbage\n");
    }
    session.send(discover(json!("last")));
    let response = session.recv_for(Some(&json!("last")), Duration::from_secs(30));
    assert!(response.get("result").is_some(), "{response}");
    session.close_stdin();
    assert_eq!(session.wait_exit(Duration::from_secs(8)).code(), Some(0));
    let rest = session.rest(LINE_TIMEOUT);
    assert_eq!(rest.len(), 4000);
    assert!(rest.iter().all(|error| error["error"]["code"] == -32700));
}

/// Rust ignores SIGPIPE, so a closed stdout surfaces as a write error.
#[test]
fn a_closed_stdout_exits_1() {
    let mut session = McpSession::without_stdout(&[], StderrMode::Captured);
    session.await_stderr(BANNER, WATCHDOG);
    session.send(json!({"jsonrpc": "2.0", "id": 1, "method": "ping"}));
    assert_eq!(session.wait_exit(Duration::from_secs(10)).code(), Some(1));
    let stderr = session.stderr(LINE_TIMEOUT);
    assert!(
        stderr.contains("reshiki-mcp: error: output write failed\n"),
        "{stderr}"
    );
}

/// A response rmcp abandons because stdout stayed full for 5 s after EOF
/// makes the exit 1, though the writer later delivers everything queued.
///
/// On 64 KiB pipes (the usual Linux and macOS size) the pipe, the line the
/// writer holds and the outbound queue take 1009 -32700 replies, so after
/// 1003 of them only six of the twelve tools/list responses fit; the reader
/// still reaches EOF, as twelve requests stay within admission. A smaller
/// pipe (Linux caps new pipes at two pages once a user exceeds
/// `pipe-user-pages-soft`) stalls the reader before EOF instead, and then
/// every request is answered. Either way exit 0 must mean every request was
/// answered; the crate's blocked-writer transcripts check abandonment
/// whatever the pipe size.
#[test]
fn a_response_abandoned_behind_a_full_stdout_exits_1() {
    const REQUESTS: i64 = 12;
    let mut session = McpSession::with_held_stdout(&[], StderrMode::Captured);
    session.await_stderr(BANNER, WATCHDOG);
    session.send_raw(&b"garbage\n".repeat(1003));
    for id in 0..REQUESTS {
        session.send(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/list",
            "params": {"_meta": modern_meta()},
        }));
    }
    session.close_stdin();
    // rmcp gives up 5 s after EOF; the writer then has 3 s to finish.
    thread::sleep(Duration::from_millis(6500));
    session.release_stdout();
    let code = session.wait_exit(LINE_TIMEOUT).code();
    let rest = session.rest(LINE_TIMEOUT);
    let answered: Vec<&Value> = rest.iter().filter_map(|line| line.get("id")).collect();
    let every = (0..REQUESTS).all(|id| answered.iter().filter(|seen| ***seen == id).count() == 1);
    assert_eq!(code, Some(if every { 0 } else { 1 }), "{answered:?}");
    let stderr = session.stderr(LINE_TIMEOUT);
    assert_eq!(
        stderr.contains("reshiki-mcp: warn: requests left unanswered requests="),
        !every,
        "{stderr}"
    );
}

/// The rmcp client discovers the server, lists its tools and, once
/// dropped, closes stdin so the server exits 0.
#[test]
fn the_rmcp_client_interoperates() {
    use rmcp::{ClientLifecycleMode, model::ProtocolVersion, serve_client_with_lifecycle};
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("runtime");
    runtime.block_on(async {
        let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_reshiki"))
            .arg("--mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .expect("start reshiki --mcp");
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let session = async {
            let client = serve_client_with_lifecycle(
                (),
                (stdout, stdin),
                ClientLifecycleMode::Discover {
                    preferred_versions: vec![ProtocolVersion::V_2026_07_28],
                },
            )
            .await
            .expect("discover");
            let info = client.peer_info().expect("server info");
            assert_eq!(info.protocol_version, ProtocolVersion::V_2026_07_28);
            let server = info.server_info.as_ref().expect("server identity");
            assert_eq!(server.name, "reshiki");
            assert_eq!(server.version, env!("CARGO_PKG_VERSION"));
            let tools = client.list_tools(None).await.expect("tools/list");
            assert!(tools.tools.is_empty(), "{:?}", tools.tools);
            drop(client);
            child.wait().await.expect("wait for reshiki --mcp")
        };
        let status = tokio::time::timeout(WATCHDOG, session)
            .await
            .expect("the session finished in time");
        assert_eq!(status.code(), Some(0));
    });
}
