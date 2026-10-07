//! Golden MCP transcripts served over pipes.
//!
//! In a transcript, `> ` lines are sent and `< ` lines are read back, each
//! within [`LINE_TIMEOUT`], and compared as JSON values; `#` lines are
//! comments. Each `< ` line is awaited before the next line is sent, so a
//! sent line followed by another sent line produced no output, and nothing
//! may follow the last expected line.
use reshiki_mcp::{
    framing::Limits,
    log::Log,
    server::{self, Finished, Identity, Quit, SUPPORTED, ServeError},
};
use serde::{Deserialize, de::IgnoredAny};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io::{self, BufRead, BufReader, PipeWriter, Write},
    sync::mpsc::{self, RecvTimeoutError},
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

impl Session {
    fn start() -> Self {
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
            reader,
            move || output_writer,
            Identity {
                app_version: "0.0.0-test".into(),
            },
            Limits::default(),
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

/// Runs a transcript on a fresh connection, closes it, and checks that
/// every request with a valid id got exactly one response, that nothing
/// stays outstanding and that nothing else was written.
fn run(name: &str, transcript: &str) {
    let mut session = Session::start();
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
