//! Golden MCP transcripts served over pipes, on the scripted [`FakeHost`].
//!
//! In a transcript, `> ` lines are sent and `< ` lines are read back, each
//! within [`LINE_TIMEOUT`], and compared as JSON values; `#` lines are
//! comments. Each `< ` line is awaited before the next line is sent, so a
//! sent line followed by another sent line produced no output, and nothing
//! may follow the last expected line.
//!
//! The protocol transcripts run on a host without tools; the
//! `tools-fake-*` transcripts on the full scripted catalog.
#[path = "common/fake_host.rs"]
mod fake_host;

use fake_host::{FakeHost, SPECS};
use reshiki_agent::{
    ops::wire::{Principal, RequestId},
    tool_spec::ToolSpec,
};
use reshiki_mcp::{
    framing::Limits,
    log::Log,
    server::{self, EOF_GRACE, Finished, Identity, Quit, SUPPORTED, ServeError},
};
use serde::{Deserialize, de::IgnoredAny};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io::{self, BufRead, BufReader, PipeWriter, Write},
    sync::{
        Arc,
        atomic::Ordering,
        mpsc::{self, RecvTimeoutError},
    },
    thread,
    time::{Duration, Instant},
};
use tokio::{runtime::Runtime, task::JoinHandle};

const LINE_TIMEOUT: Duration = Duration::from_secs(10);
const MODERN_META: &str = r#""_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}}"#;

/// One served connection: its input pipe, and its output read line by
/// line on a thread.
struct Session {
    runtime: Runtime,
    input: Option<PipeWriter>,
    output: mpsc::Receiver<String>,
    served: Option<JoinHandle<Result<Finished, ServeError>>>,
}

/// The output pipe, with its first write held until the gate's sender is
/// dropped: a client that stops reading.
struct Gated {
    gate: Option<mpsc::Receiver<()>>,
    output: PipeWriter,
}

impl Write for Gated {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if let Some(gate) = self.gate.take() {
            let _ = gate.recv();
        }
        self.output.write(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.output.flush()
    }
}

/// A host that lists no tools.
fn no_tools() -> Arc<FakeHost> {
    Arc::new(FakeHost::with_catalog(&[]))
}

impl Session {
    fn start() -> Self {
        Self::with_gate(None)
    }

    fn with_gate(gate: Option<mpsc::Receiver<()>>) -> Self {
        Self::serve(no_tools(), gate, Limits::default())
    }

    fn with_host(host: Arc<FakeHost>, limits: Limits) -> Self {
        Self::serve(host, None, limits)
    }

    fn serve(host: Arc<FakeHost>, gate: Option<mpsc::Receiver<()>>, limits: Limits) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let (reader, input) = io::pipe().unwrap();
        let (output_reader, output_writer) = io::pipe().unwrap();
        let (lines, output) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(output_reader).lines() {
                let Ok(line) = line else { break };
                if lines.send(line).is_err() {
                    break;
                }
            }
        });
        let served = runtime.spawn(server::serve(
            host,
            Principal::local(),
            reader,
            move || Gated {
                gate,
                output: output_writer,
            },
            Identity {
                app_version: fake_host::APP_VERSION.into(),
            },
            limits,
            Log::silent(),
        ));
        Self {
            runtime,
            input: Some(input),
            output,
            served: Some(served),
        }
    }

    fn send(&mut self, line: &str) {
        let input = self.input.as_mut().unwrap();
        input.write_all(line.as_bytes()).unwrap();
        input.write_all(b"\n").unwrap();
    }

    /// Whether no output line arrives within `duration`.
    fn silent_for(&self, duration: Duration) -> bool {
        matches!(
            self.output.recv_timeout(duration),
            Err(RecvTimeoutError::Timeout)
        )
    }

    fn receive(&self, context: &str) -> Value {
        let line = self
            .output
            .recv_timeout(LINE_TIMEOUT)
            .unwrap_or_else(|error| panic!("{context}: no output line ({error})"));
        serde_json::from_str(&line).unwrap_or_else(|error| panic!("{context}: {error}: {line}"))
    }

    /// Closes the input and waits for `serve` to return, with the time it
    /// took.
    fn close(&mut self) -> (Finished, Duration) {
        drop(self.input.take());
        let started = Instant::now();
        let served = self.served.take().unwrap();
        let finished = self
            .runtime
            .block_on(async { tokio::time::timeout(LINE_TIMEOUT, served).await })
            .expect("serve returned in time")
            .unwrap()
            .unwrap();
        (finished, started.elapsed())
    }

    /// Output lines left once the writer finished.
    fn rest(&self) -> Vec<String> {
        let mut rest = Vec::new();
        loop {
            match self.output.recv_timeout(LINE_TIMEOUT) {
                Ok(line) => rest.push(line),
                Err(RecvTimeoutError::Disconnected) => return rest,
                Err(RecvTimeoutError::Timeout) => panic!("the output never closed"),
            }
        }
    }
}

/// The id of a request line, when the framing accepts it: an integer or a
/// string of at most 256 bytes. Params are skipped unread, like the framing
/// does, so a request whose params fail to parse still counts.
fn request_id(line: &str) -> Option<String> {
    #[derive(Deserialize)]
    struct Head {
        id: Option<Value>,
        method: Option<IgnoredAny>,
    }
    let head: Head = serde_json::from_str(line).ok()?;
    head.method?;
    let id = head.id?;
    let valid = id.is_i64() || id.as_str().is_some_and(|text| text.len() <= 256);
    valid.then(|| id.to_string())
}

/// Runs a transcript on a fresh connection to a host without tools.
fn run(name: &str, transcript: &str) {
    run_on(no_tools(), name, transcript);
}

/// Runs a transcript on a fresh connection to `host`, closes it, and checks
/// that every request with a valid id got exactly one response, that
/// nothing stays outstanding and that nothing else was written.
fn run_on(host: Arc<FakeHost>, name: &str, transcript: &str) {
    let mut session = Session::with_host(host, Limits::default());
    let mut requests = Vec::new();
    let mut responses: HashMap<String, usize> = HashMap::new();
    for (index, line) in transcript.lines().enumerate() {
        let context = format!("{name}:{}", index + 1);
        if let Some(sent) = line.strip_prefix("> ") {
            requests.extend(request_id(sent));
            session.send(sent);
        } else if let Some(expected) = line.strip_prefix("< ") {
            let expected: Value = serde_json::from_str(expected)
                .unwrap_or_else(|error| panic!("{context}: bad golden: {error}"));
            let actual = session.receive(&context);
            assert_eq!(actual, expected, "{context}");
            if let Some(id) = actual.get("id") {
                *responses.entry(id.to_string()).or_default() += 1;
            }
        } else {
            assert!(
                line.is_empty() || line.starts_with('#'),
                "{context}: unknown line"
            );
        }
    }
    let (finished, _) = session.close();
    assert_eq!(finished.reason, Quit::Eof, "{name}");
    let tracker = finished.tracker;
    assert!(finished.writer.wait(Duration::from_secs(1)), "{name}");
    assert_eq!(tracker.outstanding(), 0, "{name}");
    assert_eq!(tracker.unanswered(), 0, "{name}");
    assert_eq!(
        session.rest(),
        Vec::<String>::new(),
        "{name}: unexpected output"
    );
    let mut answered = responses.clone();
    for id in &requests {
        assert_eq!(
            answered.remove(id),
            Some(1),
            "{name}: request {id} needs exactly one response"
        );
    }
    assert!(
        answered.is_empty(),
        "{name}: responses to no request: {answered:?}"
    );
}

#[test]
fn modern_transcript() {
    run("modern.jsonl", include_str!("transcripts/modern.jsonl"));
}

#[test]
fn error_transcript() {
    run("errors.jsonl", include_str!("transcripts/errors.jsonl"));
}

#[test]
fn legacy_2025_11_25_transcript() {
    run(
        "legacy-2025-11-25.jsonl",
        include_str!("transcripts/legacy-2025-11-25.jsonl"),
    );
}

#[test]
fn legacy_2025_06_18_transcript() {
    run(
        "legacy-2025-06-18.jsonl",
        include_str!("transcripts/legacy-2025-06-18.jsonl"),
    );
}

#[test]
fn an_unknown_notification_first_is_dropped() {
    let transcript = format!(
        "> {{\"jsonrpc\":\"2.0\",\"method\":\"notifications/foo\"}}\n\
         > {{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\",\"params\":{{{MODERN_META}}}}}\n\
         < {}\n",
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "resultType": "complete",
                "ttlMs": 0,
                "cacheScope": "private",
                "tools": [],
            },
        }),
    );
    run("unknown notification first", &transcript);
}

/// Revisions without their own handshake are answered with the newest
/// supported one that has it: 2025-03-26 is not accepted.
#[test]
fn initialize_falls_back_to_2025_11_25() {
    assert_eq!(
        SUPPORTED.map(|version| version.to_string()),
        ["2026-07-28", "2025-11-25", "2025-06-18"]
    );
    for requested in ["2024-11-05", "2025-03-26", "2099-01-01"] {
        let mut session = Session::start();
        session.send(
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": requested,
                    "capabilities": {},
                    "clientInfo": {"name": "golden", "version": "1.0.0"},
                },
            })
            .to_string(),
        );
        let response = session.receive(requested);
        assert_eq!(
            response["result"]["protocolVersion"], "2025-11-25",
            "{requested}"
        );
        let (finished, _) = session.close();
        assert_eq!(finished.reason, Quit::Eof);
    }
}

#[test]
fn an_idle_close_returns_promptly() {
    // Before any request, and after the 2026-07-28 lifecycle started.
    for opening in [None, Some(1)] {
        let mut session = Session::start();
        if let Some(id) = opening {
            session.send(&format!(
                r#"{{"jsonrpc":"2.0","id":{id},"method":"tools/list","params":{{{MODERN_META}}}}}"#
            ));
            session.receive("opening");
        }
        let (finished, elapsed) = session.close();
        assert_eq!(finished.reason, Quit::Eof);
        assert!(elapsed < Duration::from_secs(1), "{opening:?}: {elapsed:?}");
        assert!(finished.writer.wait(Duration::from_secs(1)));
    }
}

#[test]
fn a_ping_right_before_eof_is_answered() {
    let mut session = Session::start();
    session.send(r#"{"jsonrpc":"2.0","id":7,"method":"ping"}"#);
    let (finished, _) = session.close();
    assert_eq!(finished.reason, Quit::Eof);
    assert!(finished.writer.wait(Duration::from_secs(1)));
    let rest = session.rest();
    let answers: Vec<Value> = rest
        .iter()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(answers, [json!({"jsonrpc": "2.0", "id": 7, "result": {}})]);
}

/// A client that stops reading cannot hold `serve` past [`EOF_GRACE`], even
/// before a lifecycle started. The blocked writer holds one line and the
/// outbound queue 16, so of ten -32700 replies and eight pre-init pongs the
/// last pong never fits, and rmcp's bootstrap waits on that send.
#[test]
fn a_blocked_writer_cannot_hold_the_bootstrap_past_the_grace() {
    let (open, gate) = mpsc::channel();
    let mut session = Session::with_gate(Some(gate));
    for _ in 0..10 {
        session.send("garbage");
    }
    for id in 0..8 {
        session.send(&format!(r#"{{"jsonrpc":"2.0","id":{id},"method":"ping"}}"#));
    }
    let (finished, elapsed) = session.close();
    assert_eq!(finished.reason, Quit::Eof);
    // An unblocked close returns within a second, so this waited out the
    // grace.
    assert!(elapsed > EOF_GRACE - Duration::from_secs(1), "{elapsed:?}");
    assert!(elapsed < EOF_GRACE + Duration::from_secs(3), "{elapsed:?}");
    // `serve` dropped every sender, so the writer ends once released.
    drop(open);
    assert!(finished.writer.wait(LINE_TIMEOUT));
    assert_eq!(session.rest().len(), 17);
}

/// Once the lifecycle started, rmcp's own drain gives up 5 s after EOF,
/// before [`EOF_GRACE`] runs out, and abandons a response it could not
/// queue: the writer still finishes, with that request outstanding. The
/// blocked writer and the outbound queue hold seventeen -32700 replies, so
/// the tools/list that starts the lifecycle is never answered.
#[test]
fn a_blocked_writer_after_startup_leaves_the_abandoned_request_outstanding() {
    let (open, gate) = mpsc::channel();
    let mut session = Session::with_gate(Some(gate));
    for _ in 0..17 {
        session.send("garbage");
    }
    session.send(&format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{{{MODERN_META}}}}}"#
    ));
    let (finished, elapsed) = session.close();
    assert_eq!(finished.reason, Quit::Eof);
    // rmcp's drain ended the service, not the grace.
    assert!(elapsed > Duration::from_secs(4), "{elapsed:?}");
    assert!(elapsed < EOF_GRACE, "{elapsed:?}");
    drop(open);
    assert!(finished.writer.wait(LINE_TIMEOUT));
    assert_eq!(finished.tracker.outstanding(), 1);
    assert_eq!(finished.tracker.unanswered(), 1);
    let rest = session.rest();
    assert_eq!(rest.len(), 17);
    for line in &rest {
        let reply: Value = serde_json::from_str(line).unwrap();
        assert_eq!(reply["error"]["code"], -32700, "{line}");
        assert!(reply.get("id").is_none(), "{line}");
    }
}

/// The transport answers a request rmcp cannot parse (1e400 overflows f64)
/// itself, once the outbound queue has room. Behind seventeen -32700
/// replies the blocked writer leaves none, so the bootstrap waits on that
/// reply until [`EOF_GRACE`] drops the transport with it: the slot frees,
/// but the request still counts as unanswered.
#[test]
fn a_reply_the_transport_abandons_counts_as_unanswered() {
    let (open, gate) = mpsc::channel();
    let mut session = Session::with_gate(Some(gate));
    for _ in 0..17 {
        session.send("garbage");
    }
    session.send(r#"{"jsonrpc":"2.0","id":7,"method":"tools/list","params":{"n":1e400}}"#);
    let (finished, elapsed) = session.close();
    assert_eq!(finished.reason, Quit::Eof);
    assert!(elapsed > EOF_GRACE - Duration::from_secs(1), "{elapsed:?}");
    drop(open);
    assert!(finished.writer.wait(LINE_TIMEOUT));
    assert_eq!(finished.tracker.outstanding(), 0);
    assert_eq!(finished.tracker.unanswered(), 1);
    let rest = session.rest();
    assert_eq!(rest.len(), 17);
    assert!(
        rest.iter().all(|line| !line.contains(r#""id""#)),
        "{rest:?}"
    );
}

#[test]
fn fake_tools_modern_transcript() {
    run_on(
        Arc::new(FakeHost::default()),
        "tools-fake-modern.jsonl",
        include_str!("transcripts/tools-fake-modern.jsonl"),
    );
}

#[test]
fn fake_tools_legacy_transcript() {
    run_on(
        Arc::new(FakeHost::default()),
        "tools-fake-legacy.jsonl",
        include_str!("transcripts/tools-fake-legacy.jsonl"),
    );
}

/// How a client talks to the server: 2026-07-28 with `_meta` on every
/// request, or an earlier revision through `initialize`.
#[derive(Clone, Copy, Debug)]
enum Era {
    Modern,
    Legacy(&'static str),
}

const ERAS: [Era; 3] = [
    Era::Modern,
    Era::Legacy("2025-11-25"),
    Era::Legacy("2025-06-18"),
];

impl Era {
    /// Starts the lifecycle; a modern client needs no handshake.
    fn open(self, session: &mut Session) {
        if let Self::Legacy(version) = self {
            session.send(
                &json!({
                    "jsonrpc": "2.0",
                    "id": "init",
                    "method": "initialize",
                    "params": {
                        "protocolVersion": version,
                        "capabilities": {},
                        "clientInfo": {"name": "test", "version": "1.0.0"},
                    },
                })
                .to_string(),
            );
            let reply = session.receive("initialize");
            assert_eq!(reply["result"]["protocolVersion"], version, "{reply}");
            session.send(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
        }
    }

    fn call(self, id: i64, name: &str, arguments: Value) -> String {
        let mut params = json!({"name": name, "arguments": arguments});
        if let Self::Modern = self {
            params["_meta"] = json!({
                "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                "io.modelcontextprotocol/clientCapabilities": {},
            });
        }
        json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": params}).to_string()
    }
}

fn cancel(id: i64) -> String {
    json!({
        "jsonrpc": "2.0",
        "method": "notifications/cancelled",
        "params": {"requestId": id, "reason": "test"},
    })
    .to_string()
}

/// Waits up to [`LINE_TIMEOUT`] for `condition`.
fn wait_until(what: &str, condition: impl Fn() -> bool) {
    let deadline = Instant::now() + LINE_TIMEOUT;
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        thread::sleep(Duration::from_millis(2));
    }
}

/// Closes the session and checks that every slot completed, every job
/// ended and nothing else was written.
fn finish(mut session: Session, host: &FakeHost, context: &str) {
    let (finished, _) = session.close();
    assert_eq!(finished.reason, Quit::Eof, "{context}");
    assert!(finished.writer.wait(LINE_TIMEOUT), "{context}");
    assert_eq!(finished.tracker.outstanding(), 0, "{context}");
    assert_eq!(finished.tracker.unanswered(), 0, "{context}");
    assert_eq!(session.rest(), Vec::<String>::new(), "{context}");
    assert_eq!(host.live_jobs.load(Ordering::SeqCst), 0, "{context}");
    assert!(!host.dropped_while_live.load(Ordering::SeqCst), "{context}");
}

#[test]
fn structured_content_is_an_object_in_every_era() {
    for era in ERAS {
        let host = Arc::new(FakeHost::default());
        let mut session = Session::with_host(Arc::clone(&host), Limits::default());
        era.open(&mut session);
        for (id, name, arguments) in [
            (1, "echo", json!({"a": [1, 2]})),
            (2, "echo", Value::Null),
            (3, "svg", json!({})),
            (4, "fail_busy", json!({})),
        ] {
            session.send(&era.call(id, name, arguments));
            let reply = session.receive(name);
            let result = &reply["result"];
            assert!(result["structuredContent"].is_object(), "{era:?}: {reply}");
            let copy: Value =
                serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
            assert_eq!(copy, result["structuredContent"], "{era:?}");
            assert_eq!(result["isError"], name == "fail_busy", "{era:?}: {reply}");
            let modern = matches!(era, Era::Modern);
            assert_eq!(
                result.get("resultType").is_some(),
                modern,
                "{era:?}: {reply}"
            );
        }
        // The host saw the connection's principal and each request's id.
        let calls = host.calls.lock().unwrap();
        assert_eq!(calls.len(), 4, "{era:?}");
        assert!(
            calls
                .iter()
                .all(|call| call.principal == Principal::local())
        );
        assert_eq!(calls[0].request, RequestId::Int(1));
        assert_eq!(calls[1].arguments, json!({}), "null arguments are {{}}");
        drop(calls);
        finish(session, &host, &format!("{era:?}"));
    }
}

/// A cancelled call keeps running to its end, its response is never
/// written, the next call is answered and a late cancellation is ignored.
#[test]
fn a_cancelled_call_sends_nothing_and_runs_to_its_end() {
    for era in ERAS {
        let context = format!("{era:?}");
        let host = Arc::new(FakeHost::default());
        let mut session = Session::with_host(Arc::clone(&host), Limits::default());
        era.open(&mut session);
        session.send(&era.call(1, "slow", json!({})));
        host.started.wait();
        session.send(&cancel(1));
        wait_until("the cancellation", || {
            !host.cancels.lock().unwrap().is_empty()
        });
        assert_eq!(
            *host.cancels.lock().unwrap(),
            [(Principal::local(), RequestId::Int(1))],
            "{context}"
        );
        assert!(session.silent_for(Duration::from_millis(500)), "{context}");
        // The call's future is still waiting on its job.
        assert_eq!(host.live_jobs.load(Ordering::SeqCst), 1, "{context}");
        assert!(!host.dropped_while_live.load(Ordering::SeqCst), "{context}");
        host.release.wait();
        session.send(&era.call(2, "echo", json!({"after": "cancel"})));
        let reply = session.receive("echo after the cancel");
        assert_eq!(reply["id"], 2, "{context}: {reply}");
        assert_eq!(
            reply["result"]["structuredContent"],
            json!({"after": "cancel"})
        );
        // Once 3 is answered, the writer has completed 2, so cancelling 2
        // is a late cancellation.
        session.send(&era.call(3, "echo", json!({})));
        assert_eq!(session.receive("echo 3")["id"], 3, "{context}");
        session.send(&cancel(2));
        session.send(&era.call(4, "echo", json!({})));
        assert_eq!(session.receive("echo 4")["id"], 4, "{context}");
        assert_eq!(host.cancels.lock().unwrap().len(), 1, "{context}");
        finish(session, &host, &context);
    }
}

#[test]
fn two_hundred_cancel_cycles_leave_nothing_behind() {
    let host = Arc::new(FakeHost::default());
    let mut session = Session::with_host(Arc::clone(&host), Limits::default());
    let era = Era::Modern;
    for cycle in 0..200 {
        let id = 1000 + cycle;
        session.send(&era.call(id, "slow", json!({})));
        host.started.wait();
        session.send(&cancel(id));
        // The tracker marks the request before the host hears of it, so
        // the writer drops whatever the call returns.
        let seen = usize::try_from(cycle).unwrap() + 1;
        wait_until("the cancellation", || {
            host.cancels.lock().unwrap().len() == seen
        });
        host.release.wait();
    }
    session.send(&era.call(1, "echo", json!({})));
    assert_eq!(session.receive("echo")["id"], 1);
    let cancels = host.cancels.lock().unwrap().clone();
    let expected: Vec<(Principal, RequestId)> = (1000..1200)
        .map(|id| (Principal::local(), RequestId::Int(id)))
        .collect();
    assert_eq!(cancels, expected);
    finish(session, &host, "200 cycles");
}

#[test]
fn a_result_over_the_cap_is_a_budget_error() {
    let host = Arc::new(FakeHost::default());
    let limits = Limits {
        max_result_bytes: 4096,
        ..Limits::default()
    };
    let mut session = Session::with_host(Arc::clone(&host), limits);
    let era = Era::Modern;
    session.send(&era.call(1, "big", json!({"n": 100})));
    let small = session.receive("small");
    assert_eq!(small["result"]["isError"], false, "{small}");
    session.send(&era.call(2, "big", json!({"n": 4000})));
    let big = session.receive("big");
    assert_eq!(big["result"]["isError"], true, "{big}");
    assert_eq!(
        big["result"]["structuredContent"]["error"],
        json!({
            "code": "budget",
            "message": "Result exceeds 4096 bytes; request a smaller render or fewer objects",
        })
    );
    finish(session, &host, "big");
}

static DUPLICATE: [ToolSpec; 2] = [SPECS[0], SPECS[0]];

/// An invalid catalog ends `serve` before anything is read or written.
#[test]
fn an_invalid_catalog_is_refused_before_serving() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let (reader, input) = io::pipe().unwrap();
    let (output_reader, output_writer) = io::pipe().unwrap();
    let served = runtime.block_on(async {
        tokio::time::timeout(
            LINE_TIMEOUT,
            server::serve(
                Arc::new(FakeHost::with_catalog(&DUPLICATE)),
                Principal::local(),
                reader,
                move || output_writer,
                Identity {
                    app_version: fake_host::APP_VERSION.into(),
                },
                Limits::default(),
                Log::silent(),
            ),
        )
        .await
    });
    assert!(matches!(served, Ok(Err(ServeError::Catalog))));
    // The input is still open, and nothing was written.
    drop(input);
    let mut output = String::new();
    io::Read::read_to_string(&mut BufReader::new(output_reader), &mut output).unwrap();
    assert_eq!(output, "");
}
