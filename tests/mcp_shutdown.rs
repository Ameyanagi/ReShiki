//! `reshiki --mcp` under load and at the end of its input: bounded exits,
//! backpressure on a client that stops reading, and admission bursts.
//!
//! Every session runs under a 60 s watchdog with an empty data folder. Pipe
//! buffers differ between systems, so every bound allows for a full OS
//! pipe, and the requests that must fill one carry the whole tool catalog.
#[path = "common/headless.rs"]
mod headless;

use headless::{McpSession, StderrMode, compose_32, modern_call, modern_list};
use reshiki_agent::ops::budget::Budgets;
use reshiki_mcp::framing::{Limits, OUTBOUND_QUEUE, READ_CHUNK};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io::Write,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

const LINE_TIMEOUT: Duration = Duration::from_secs(10);
const WATCHDOG: Duration = Duration::from_secs(60);
/// How long the server may take to exit once its input ended or its output
/// closed.
const SHUTDOWN_BOUND: Duration = Duration::from_secs(15);
/// The most an OS pipe buffers by default: 64 KiB on Linux and macOS, less
/// on Windows.
const PIPE_BUFFER: usize = 64 * 1024;
const BANNER: &str = "reshiki-mcp: info: ReShiki agent API (experimental) ";

fn limits() -> Limits {
    Limits::from_budgets(&Budgets::default())
}

/// A line of `message`, with its newline.
fn line(message: &Value) -> Vec<u8> {
    let mut line = message.to_string().into_bytes();
    line.push(b'\n');
    line
}

/// The number of times each id appears in `messages`.
fn id_counts(messages: &[Value]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for message in messages {
        *counts.entry(message["id"].to_string()).or_default() += 1;
    }
    counts
}

/// (a) The input ends while the client reads none of the responses to
/// `max_outstanding` tools/list requests, each holding the whole catalog:
/// the server gives up on delivering them and exits 1 within the bound.
/// What it did write ends at a line boundary, except for the line it was
/// writing when it exited.
#[test]
fn an_eof_with_stdout_unread_exits_1_within_the_bound() {
    let requests = limits().max_outstanding;
    let mut session = McpSession::with_held_stdout(&[], StderrMode::Captured);
    session.await_stderr(BANNER, WATCHDOG);
    for id in 0..requests {
        session.send(modern_list(id as i64));
    }
    session.close_stdin();
    let started = Instant::now();
    let status = session.wait_exit(SHUTDOWN_BOUND);
    assert_eq!(status.code(), Some(1), "after {:?}", started.elapsed());
    session.release_stdout();
    let written = session.complete_rest(LINE_TIMEOUT);
    assert!(written.len() < requests, "{} responses", written.len());
    for message in &written {
        assert!(message["result"]["tools"].is_array(), "{message}");
    }
}

/// (b) The input ends while a 32-molecule compose runs: the server exits
/// within the bound, 0 only if the compose was answered.
#[test]
fn an_eof_during_a_compose_exits_within_the_bound() {
    let mut session = McpSession::start(&[], StderrMode::Captured);
    session.await_stderr(BANNER, WATCHDOG);
    session.send(modern_call(1, "compose", compose_32()));
    // The service takes requests in order and starts each call on its own
    // task, so this answer shows the compose started.
    session.send(modern_list(2));
    session.recv_for(Some(&json!(2)), LINE_TIMEOUT);
    session.close_stdin();
    let started = Instant::now();
    let status = session.wait_exit(SHUTDOWN_BOUND);
    let rest = session.rest(LINE_TIMEOUT);
    let answered = rest.iter().any(|message| message["id"] == 1);
    assert_eq!(
        status.code(),
        Some(if answered { 0 } else { 1 }),
        "after {:?}: {rest:?}",
        started.elapsed()
    );
}

/// A writer thread sends `notices` unknown notifications, then floods
/// 1000 tools/list requests while stdout stays unread. 3 s after the
/// notifications are written it is blocked: the server holds at most the
/// admitted requests and the line waiting for admission (counted
/// generously as `max_outstanding + 8` lines), its reader's buffer and a
/// full OS pipe. Once stdout is read, every request is answered exactly
/// once.
///
/// Each id is padded to 200 bytes, so the flood is far larger than that
/// bound. The notifications need no admission, so the server reads them
/// all before the requests.
fn flood(session: &mut McpSession, notices: usize) {
    const REQUESTS: usize = 1000;
    let ids: Vec<String> = (0..REQUESTS).map(|n| format!("{n:0>200}")).collect();
    let lines: Vec<Vec<u8>> = ids
        .iter()
        .map(|id| {
            let mut request = modern_list(0);
            request["id"] = json!(id);
            line(&request)
        })
        .collect();
    let longest = lines.iter().map(Vec::len).max().unwrap_or_default();
    let notice = line(&json!({"jsonrpc": "2.0", "method": "notifications/reshiki/test"}));
    let mut stdin = session.take_stdin();
    let written = Arc::new(AtomicUsize::new(0));
    let (noticed, notices_written) = mpsc::channel();
    let writer = {
        let written = Arc::clone(&written);
        thread::spawn(move || {
            stdin
                .write_all(&notice.repeat(notices))
                .expect("write the notifications");
            let _ = noticed.send(());
            for line in &lines {
                stdin.write_all(line).expect("write a request");
                written.fetch_add(line.len(), Ordering::SeqCst);
            }
            // Dropping stdin ends the input.
        })
    };
    notices_written
        .recv_timeout(LINE_TIMEOUT)
        .expect("the server read the notifications");
    thread::sleep(Duration::from_secs(3));
    let blocked_at = written.load(Ordering::SeqCst);
    assert!(!writer.is_finished(), "the writer never blocked");
    let bound = (limits().max_outstanding + 8) * longest + READ_CHUNK + PIPE_BUFFER;
    assert!(
        blocked_at <= bound,
        "{blocked_at} bytes written; at most {bound} may be"
    );

    session.release_stdout();
    writer.join().expect("the writer finished");
    assert_eq!(session.wait_exit(WATCHDOG).code(), Some(0));
    let answered = session.rest(LINE_TIMEOUT);
    let expected: HashMap<String, usize> = ids
        .iter()
        .map(|id| (Value::from(id.as_str()).to_string(), 1))
        .collect();
    assert_eq!(id_counts(&answered), expected);
    for message in &answered {
        assert!(message["result"]["tools"].is_array(), "{message}");
    }
}

/// (c) A flood while stdout is unread blocks the client at a bounded point
/// and loses nothing.
#[test]
fn a_flood_with_stdout_unread_is_bounded_and_fully_answered() {
    let mut session = McpSession::with_held_stdout(&[], StderrMode::Captured);
    session.await_stderr(BANNER, WATCHDOG);
    flood(&mut session, 0);
}

/// (e) The same flood with debug logging to a stderr nobody reads never
/// deadlocks. Each notification before it logs one debug line, four
/// pipes' worth in all, so stderr is full before stdout fills. Reading
/// that many notifications also shows the server started.
#[test]
fn a_flood_with_stdout_and_debug_stderr_unread_never_deadlocks() {
    const EVENT: &str = "reshiki-mcp: debug: ignored notification\n";
    let notices = 4 * PIPE_BUFFER / EVENT.len() + 1;
    let mut session = McpSession::with_held_stdout(&["--log-level", "debug"], StderrMode::Unread);
    flood(&mut session, notices);
    // The log wrote some events and dropped the rest behind its full pipe.
    let logged = session.unread_stderr(LINE_TIMEOUT).matches(EVENT).count();
    assert!(
        logged > 0 && logged < notices,
        "{logged} of {notices} debug events written"
    );
}

/// (d) 30 renders sent at once on a 32-molecule document: each gets
/// exactly one response, an image or a `busy` tool error from the
/// executor's admission.
#[test]
fn a_render_burst_gets_one_response_each() {
    const RENDERS: i64 = 30;
    let mut session = McpSession::start(&[], StderrMode::Captured);
    session.send(modern_call(1, "compose", compose_32()));
    let composed = session.recv_for(Some(&json!(1)), LINE_TIMEOUT);
    let document = composed["result"]["structuredContent"]["value"]["document"].clone();
    assert!(document.is_string(), "{composed}");
    let render = json!({
        "document": document,
        "format": "png",
        "max_width": null,
        "max_height": null,
        "ids": null,
    });
    for id in 0..RENDERS {
        session.send(modern_call(100 + id, "render", render.clone()));
    }
    let (mut rendered, mut busy) = (0, 0);
    for id in 0..RENDERS {
        let reply = session.recv_for(Some(&json!(100 + id)), Duration::from_secs(40));
        let result = &reply["result"];
        if result["isError"] == true {
            let error = &result["structuredContent"]["error"];
            assert_eq!(error["code"], "busy", "{error}");
            busy += 1;
        } else {
            assert_eq!(result["isError"], false, "{reply}");
            assert_eq!(result["content"][1]["type"], "image");
            rendered += 1;
        }
    }
    assert_eq!(rendered + busy, RENDERS);
    assert!(rendered > 0);
    session.close_stdin();
    assert_eq!(session.wait_exit(LINE_TIMEOUT).code(), Some(0));
    assert_eq!(session.rest(LINE_TIMEOUT), Vec::<Value>::new());
}

/// No output stall ends the server: a client that floods it with lines
/// whose error replies it never reads, then closes stdin, leaves it
/// blocked before it can see the end of input. Closing the read end of
/// stdout is the way out: the write fails and the server exits 1.
#[test]
fn closing_an_unread_stdout_ends_a_stalled_server() {
    /// The parse error answering each garbage line, with its newline.
    const REPLY: usize =
        r#"{"jsonrpc":"2.0","error":{"code":-32700,"message":"Parse error"}}"#.len() + 1;
    /// Two-byte lines: a 4 KiB pipe takes them all, so stdin closes while
    /// the server is blocked, and their replies are more than a 64 KiB
    /// pipe, the writer's line, the outbound queue and the reader's
    /// pending reply hold.
    const GARBAGE: usize = 1800;
    const _: () = assert!(GARBAGE * 2 <= 4096);
    const _: () = assert!(GARBAGE * REPLY > PIPE_BUFFER + (OUTBOUND_QUEUE + 2) * REPLY);
    let mut session = McpSession::with_held_stdout(&[], StderrMode::Captured);
    session.await_stderr(BANNER, WATCHDOG);
    let mut stdin = session.take_stdin();
    let (closed, stdin_closed) = mpsc::channel();
    thread::spawn(move || {
        let written = stdin.write_all(&b"x\n".repeat(GARBAGE));
        drop(stdin);
        let _ = closed.send(written.is_ok());
    });
    assert_eq!(
        stdin_closed.recv_timeout(LINE_TIMEOUT),
        Ok(true),
        "stdin was not written whole and closed"
    );
    // Longer than an exit on the end of input would take.
    thread::sleep(Duration::from_secs(5));
    assert!(session.is_running(), "the server saw the end of its input");
    session.close_held_stdout();
    let started = Instant::now();
    let status = session.wait_exit(SHUTDOWN_BOUND);
    assert_eq!(status.code(), Some(1), "after {:?}", started.elapsed());
    let stderr = session.stderr(LINE_TIMEOUT);
    assert!(
        stderr.contains("reshiki-mcp: error: output write failed\n"),
        "{stderr}"
    );
}
