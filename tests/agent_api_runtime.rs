//! Runtime limits of `reshiki --mcp` through the real binary: admission and
//! cancellation invariants under a render burst, duplicate ids, oversize
//! lines, the text budget and the heap ceiling.
//!
//! The admission and cancellation checks hold whatever the timing: a fast
//! server only makes them weaker, never false. Every session runs under a
//! 60 s watchdog with an empty data folder.
#[path = "common/headless.rs"]
mod headless;

use headless::{McpSession, StderrMode, cancel, modern_call, modern_meta};
use reshiki_agent::ops::budget::Budgets;
use reshiki_mcp::framing::{Limits, PROBE_BYTES};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    ffi::OsStr,
    process::{Command, Output, Stdio},
    time::Duration,
};

const LINE_TIMEOUT: Duration = Duration::from_secs(10);
/// Long enough for every render of a burst on a slow machine.
const BURST_TIMEOUT: Duration = Duration::from_secs(40);
/// Long enough to read and answer a line of tens of MiB in a debug build.
const BIG_LINE_TIMEOUT: Duration = Duration::from_secs(30);
const MIB: usize = 1024 * 1024;

fn discover(id: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "server/discover",
        "params": {"_meta": modern_meta()},
    })
}

/// Sends a discover request and asserts its answer.
fn assert_responsive(session: &mut McpSession, id: &str, timeout: Duration) {
    session.send(discover(id));
    let reply = session.recv_for(Some(&json!(id)), timeout);
    assert!(reply.get("result").is_some(), "{reply}");
}

/// A session holding a drawing of 16 separate molecules, which renders at
/// the largest allowed size slowly enough for calls to overlap; returns
/// the session and the render arguments.
fn with_drawing() -> (McpSession, Value) {
    let mut session = McpSession::start(&[], StderrMode::Captured);
    let smiles = ["c1ccccc1CC(=O)O"; 16].join(".");
    session.send(modern_call(
        1,
        "import",
        json!({"format": "smiles", "text": smiles}),
    ));
    let imported = session.recv_for(Some(&json!(1)), BURST_TIMEOUT);
    let document = imported["result"]["structuredContent"]["value"]["document"].clone();
    assert!(document.is_string(), "{imported}");
    // The largest square box render.max_pixels allows.
    let render = Budgets::default().render;
    let side = render.max_pixels.isqrt().min(u64::from(render.max_side));
    let arguments = json!({
        "document": document,
        "format": "png",
        "max_width": side,
        "max_height": side,
        "ids": null,
    });
    (session, arguments)
}

/// Whether a render's reply is the `busy` tool error; any other reply must
/// be an image.
fn is_busy(reply: &Value) -> bool {
    let result = &reply["result"];
    if result["isError"] == true {
        assert_eq!(
            result["structuredContent"]["error"]["code"], "busy",
            "{reply}"
        );
        true
    } else {
        assert_eq!(result["isError"], false, "{reply}");
        assert_eq!(result["content"][1]["type"], "image", "{reply}");
        false
    }
}

/// The number of messages carrying each id.
fn id_counts(messages: &[Value]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for message in messages {
        *counts.entry(message["id"].to_string()).or_default() += 1;
    }
    counts
}

/// Receives a reply for each of `ids` not yet in `seen`, then everything
/// else already received, into `seen`.
fn collect(session: &mut McpSession, ids: &[i64], seen: &mut Vec<Value>, timeout: Duration) {
    for id in ids {
        if !seen.iter().any(|message| message["id"] == *id) {
            seen.push(session.recv_for(Some(&json!(id)), timeout));
        }
    }
    seen.extend(session.take_ready());
}

/// (a) Admission. First concurrency + queue (2 + 8) renders at the largest
/// size, sent back to back with nothing else outstanding: none may be
/// `busy`. Then 14 more back to back: each gets exactly one response, and
/// `busy` never answers more of them than were sent while the client
/// already had concurrency + queue unanswered.
///
/// The server holds only calls the client sent and has not seen answered,
/// so each refusal needs that many outstanding. The burst compares counts
/// rather than which calls were refused: each request runs on its own
/// task, so the executor may admit them in another order than they were
/// sent. A limit before the executor's below concurrency + queue refuses
/// a call of the first batch, or more of the burst, and fails this.
#[test]
fn busy_answers_only_calls_beyond_the_executor_queue() {
    const BURST: i64 = 14;
    let budgets = Budgets::default();
    let capacity = budgets.concurrency + budgets.queue;
    let (mut session, render) = with_drawing();

    let batch: Vec<i64> = (100..).take(capacity).collect();
    for id in &batch {
        session.send(modern_call(*id, "render", render.clone()));
    }
    let mut seen = Vec::new();
    collect(&mut session, &batch, &mut seen, BURST_TIMEOUT);
    for reply in &seen {
        assert!(!is_busy(reply), "{reply}");
    }

    let burst: Vec<i64> = (200..200 + BURST).collect();
    let mut answered = Vec::new();
    let mut beyond = 0;
    for (sent, id) in burst.iter().enumerate() {
        answered.extend(session.take_ready());
        if sent - answered.len() >= capacity {
            beyond += 1;
        }
        session.send(modern_call(*id, "render", render.clone()));
    }
    collect(&mut session, &burst, &mut answered, BURST_TIMEOUT);
    let busy = answered.iter().filter(|reply| is_busy(reply)).count();
    assert!(
        busy <= beyond,
        "{busy} busy replies; {beyond} calls sent beyond {capacity} outstanding"
    );
    assert_responsive(&mut session, "sentinel", LINE_TIMEOUT);
    session.close_stdin();
    assert_eq!(session.wait_exit(LINE_TIMEOUT).code(), Some(0));
    seen.extend(answered);
    seen.extend(session.rest(LINE_TIMEOUT));
    let expected: HashMap<String, usize> = batch
        .iter()
        .chain(&burst)
        .map(|id| (id.to_string(), 1))
        .collect();
    assert_eq!(id_counts(&seen), expected);
}

/// (b) Six renders, three of them cancelled: a cancelled call gets at most
/// one response, the others exactly one, serving goes on and the end of
/// input still exits 0 within 10 s.
#[test]
fn cancelled_renders_get_at_most_one_response() {
    let (mut session, render) = with_drawing();
    let ids: Vec<i64> = (200..206).collect();
    for id in &ids {
        session.send(modern_call(*id, "render", render.clone()));
    }
    let cancelled = [201, 203, 205];
    for id in cancelled {
        session.send(cancel(id));
    }
    assert_responsive(&mut session, "sentinel", BURST_TIMEOUT);
    let answered: Vec<i64> = ids
        .iter()
        .copied()
        .filter(|id| !cancelled.contains(id))
        .collect();
    let mut seen = Vec::new();
    collect(&mut session, &answered, &mut seen, BURST_TIMEOUT);
    session.close_stdin();
    assert_eq!(session.wait_exit(LINE_TIMEOUT).code(), Some(0));
    seen.extend(session.rest(LINE_TIMEOUT));
    let counts = id_counts(&seen);
    for id in &answered {
        assert_eq!(counts.get(&id.to_string()), Some(&1), "{id}: {counts:?}");
    }
    for id in cancelled {
        assert!(
            counts.get(&id.to_string()).is_none_or(|n| *n <= 1),
            "{id}: {counts:?}"
        );
    }
    assert!(
        counts
            .keys()
            .all(|id| ids.iter().any(|sent| sent.to_string() == *id)),
        "{counts:?}"
    );
    for reply in &seen {
        assert!(!is_busy(reply), "{reply}");
    }
}

/// (c) A request reusing the id of one still running gets transport-2's
/// -32600 with that id; the running call is still answered.
///
/// Both lines go in one write, so the framing reads the second while the
/// render runs unless it is held up for the whole render. Then the
/// render's response comes first and the second request is answered: its
/// id was free again, a legal reuse, and the check is retried with a new
/// id, a few times at most.
#[test]
fn a_duplicate_in_flight_id_is_an_invalid_request() {
    const ATTEMPTS: i64 = 5;
    let (mut session, render) = with_drawing();
    let refused = (7..7 + ATTEMPTS).find(|&id| {
        let mut lines = modern_call(id, "render", render.clone()).to_string();
        lines.push('\n');
        lines.push_str(&headless::modern_list(id).to_string());
        lines.push('\n');
        session.send_raw(lines.as_bytes());
        // In the order they arrived.
        let first = session.recv_for(Some(&json!(id)), BURST_TIMEOUT);
        let second = session.recv_for(Some(&json!(id)), BURST_TIMEOUT);
        if first["result"]["content"].is_array() && second["result"]["tools"].is_array() {
            assert!(!is_busy(&first), "{first}");
            return false;
        }
        // The refusal is written at once, but no order is promised.
        let mut replies = [first, second];
        replies.sort_by_key(|reply| reply.get("result").is_some());
        let [duplicate, rendered] = replies;
        assert_eq!(
            duplicate,
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {"code": -32600, "message": "Duplicate request id"},
            })
        );
        assert!(!is_busy(&rendered), "{rendered}");
        true
    });
    assert!(
        refused.is_some(),
        "each render ended before its duplicate was read"
    );
    session.close_stdin();
    assert_eq!(session.wait_exit(LINE_TIMEOUT).code(), Some(0));
    assert_eq!(session.rest(LINE_TIMEOUT), Vec::<Value>::new());
}

/// (d) A line 1 MiB longer than the cap gets -32600 with the id its first
/// 64 KiB reveal, or without an id when they reveal none; the next request
/// is answered either way.
#[test]
fn oversize_lines_are_refused_and_serving_continues() {
    let max_line_bytes = Limits::from_budgets(&Budgets::default()).max_line_bytes;
    let length = max_line_bytes + MIB;
    let refusal = |id: Option<i64>| {
        let mut reply = json!({
            "jsonrpc": "2.0",
            "error": {"code": -32600, "message": format!("Request exceeds {max_line_bytes} bytes")},
        });
        if let Some(id) = id {
            reply["id"] = json!(id);
        }
        reply
    };
    let mut session = McpSession::start(&[], StderrMode::Captured);

    let head = r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"import","arguments":{"format":"smiles","text":""#;
    assert!(head.len() < PROBE_BYTES);
    let tail = r#""}}}"#;
    let mut line = Vec::with_capacity(length + 1);
    line.extend_from_slice(head.as_bytes());
    line.resize(length - tail.len(), b'C');
    line.extend_from_slice(tail.as_bytes());
    line.push(b'\n');
    session.send_raw(&line);
    let reply = session.recv_for(Some(&json!(7)), BIG_LINE_TIMEOUT);
    assert_eq!(reply, refusal(Some(7)));
    assert_responsive(&mut session, "after-id", BIG_LINE_TIMEOUT);

    line.clear();
    line.resize(length, b'x');
    line.push(b'\n');
    session.send_raw(&line);
    // transport-2's framing table answers an oversize line even when no id
    // can be read; it omits the id.
    let reply = session.recv_for(None, BIG_LINE_TIMEOUT);
    assert_eq!(reply, refusal(None));
    assert_responsive(&mut session, "after-junk", BIG_LINE_TIMEOUT);

    session.close_stdin();
    assert_eq!(session.wait_exit(LINE_TIMEOUT).code(), Some(0));
    assert_eq!(session.rest(LINE_TIMEOUT), Vec::<Value>::new());
}

/// (e) Import text past max_text_bytes on a line within the cap is the
/// operation's `budget` error, passed through unchanged.
#[test]
fn import_text_past_the_budget_is_a_budget_error() {
    let budgets = Budgets::default();
    let text = "C".repeat(budgets.max_text_bytes + 1);
    let call = modern_call(1, "import", json!({"format": "smiles", "text": text}));
    let line = call.to_string();
    assert!(line.len() < Limits::from_budgets(&budgets).max_line_bytes);
    let mut session = McpSession::start(&[], StderrMode::Captured);
    session.send_raw(format!("{line}\n").as_bytes());
    let reply = session.recv_for(Some(&json!(1)), BIG_LINE_TIMEOUT);
    let result = &reply["result"];
    assert_eq!(result["isError"], true, "{reply}");
    assert_eq!(result["structuredContent"]["error"]["code"], "budget");
    session.close_stdin();
    assert_eq!(session.wait_exit(LINE_TIMEOUT).code(), Some(0));
}

/// Runs `reshiki --cli args` with an empty data folder and
/// RESHIKI_AGENT_HEAP_MB set to `heap`.
fn cli_with_heap(args: &[&str], heap: &str) -> Output {
    let data = headless::data_dir();
    let child = Command::new(env!("CARGO_BIN_EXE_reshiki"))
        .arg("--cli")
        .args(args)
        .env("RESHIKI_DATA_DIR", data.path())
        .env(HEAP_VARIABLE, heap)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start reshiki --cli");
    headless::wait_with_watchdog(child, LINE_TIMEOUT)
}

const HEAP_VARIABLE: &str = "RESHIKI_AGENT_HEAP_MB";

/// (f) RESHIKI_AGENT_HEAP_MB outside 256 to 16384 MiB is refused before
/// anything is read, by `--mcp` and `--cli` alike, once the command line
/// parsed: a usage error in it is reported instead. 256 MiB serves.
#[test]
fn the_heap_ceiling_is_validated_and_applied() {
    let refusal = format!("{HEAP_VARIABLE} must be a whole number of MiB from 256 to 16384\n");
    let tiny = [(HEAP_VARIABLE, OsStr::new("1"))];
    let mut session = McpSession::start_with_env(&[], &tiny, StderrMode::Captured);
    assert_eq!(session.wait_exit(LINE_TIMEOUT).code(), Some(2));
    assert_eq!(session.rest(LINE_TIMEOUT), Vec::<Value>::new());
    let stderr = session.stderr(LINE_TIMEOUT);
    assert_eq!(stderr, format!("reshiki --mcp: {refusal}"));

    let mut session = McpSession::start_with_env(&["--bogus"], &tiny, StderrMode::Captured);
    assert_eq!(session.wait_exit(LINE_TIMEOUT).code(), Some(2));
    assert_eq!(session.rest(LINE_TIMEOUT), Vec::<Value>::new());
    let stderr = session.stderr(LINE_TIMEOUT);
    assert_eq!(
        stderr.lines().next(),
        Some("reshiki --mcp: unknown option `--bogus`"),
        "{stderr}"
    );

    let output = cli_with_heap(&["info"], "16385");
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(stderr, format!("reshiki: {refusal}"));

    let output = cli_with_heap(&["convert", "--bogus"], "1");
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(stderr, "reshiki: unknown option `--bogus`\n");

    let fits = [(HEAP_VARIABLE, OsStr::new("256"))];
    let mut session = McpSession::start_with_env(&[], &fits, StderrMode::Captured);
    assert_responsive(&mut session, "ceiling", LINE_TIMEOUT);
    session.close_stdin();
    assert_eq!(session.wait_exit(LINE_TIMEOUT).code(), Some(0));
}
