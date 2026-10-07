use super::{
    head::{Class, INVALID_REQUEST, Ignored, PARSE_ERROR, Reply, classify, probe_id},
    *,
};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    io::{Cursor, PipeWriter},
    sync::{Barrier, Mutex, atomic::AtomicUsize, mpsc as std_mpsc},
    thread::JoinHandle,
    time::Instant,
};
use tokio::{runtime::Runtime, sync::Semaphore};

const TIMEOUT: Duration = Duration::from_secs(10);

// --- head classification -------------------------------------------------

fn reply(code: i64, message: &'static str, id: Option<Key>) -> Class {
    Class::Reply(Reply {
        code,
        message: message.into(),
        id,
    })
}

fn parse_error() -> Class {
    reply(PARSE_ERROR, "Parse error", None)
}

fn invalid(id: Option<Key>) -> Class {
    reply(INVALID_REQUEST, "Invalid Request", id)
}

fn int(value: i64) -> Option<Key> {
    Some(Key::Int(value))
}

fn text(value: &str) -> Key {
    Key::Str(value.into())
}

fn request(key: Key) -> Class {
    Class::Request {
        key,
        is_initialize: false,
    }
}

fn with_id(id: &str) -> String {
    format!(r#"{{"jsonrpc":"2.0","id":{id},"method":"ping"}}"#)
}

fn cancelled(params: &str) -> String {
    format!(r#"{{"jsonrpc":"2.0","method":"notifications/cancelled","params":{params}}}"#)
}

const NOTIFICATION: Class = Class::Ignore(Ignored::Notification);
const RESPONSE: Class = Class::Ignore(Ignored::Response);

#[test]
fn classify_follows_the_framing_table() {
    let deep = format!("{}{}", "[".repeat(200), "]".repeat(200));
    let long_id = format!(r#""{}""#, "x".repeat(300));
    let max_id = "x".repeat(MAX_ID_BYTES);
    let pad = "p".repeat(4096);
    let rows: Vec<(String, Class)> = vec![
        // Not JSON, including nesting past serde_json's recursion limit.
        ("not json".into(), parse_error()),
        (r#"{"jsonrpc":"2.0","id":1"#.into(), parse_error()),
        (format!("{} {{}}", with_id("1")), parse_error()),
        (
            r#"{"jsonrpc":"2.0","id":1,"id":2,"method":"m"}"#.into(),
            parse_error(),
        ),
        (deep.clone(), parse_error()),
        (
            format!(r#"{{"jsonrpc":"2.0","id":1,"method":"m","params":{deep}}}"#),
            parse_error(),
        ),
        // Valid JSON that is not an object.
        ("[1]".into(), invalid(None)),
        (" 42".into(), invalid(None)),
        (r#""text""#.into(), invalid(None)),
        ("null".into(), invalid(None)),
        // Invalid ids.
        (with_id("1.5"), invalid(None)),
        (with_id("true"), invalid(None)),
        (with_id("null"), invalid(None)),
        (with_id("{}"), invalid(None)),
        (with_id("[1]"), invalid(None)),
        (with_id("18446744073709551616"), invalid(None)),
        (with_id("9223372036854775808"), invalid(None)),
        (with_id(&long_id), invalid(None)),
        // Valid ids; params are never read.
        (with_id("-9223372036854775808"), request(Key::Int(i64::MIN))),
        (with_id(r#""abc""#), request(text("abc"))),
        (with_id(&format!(r#""{max_id}""#)), request(text(&max_id))),
        (
            format!(r#"  {{"method":"m","params":"{pad}","id":3,"jsonrpc":"2.0"}}"#),
            request(Key::Int(3)),
        ),
        // Params scalars are skipped without conversion or decoding: a
        // number outside f64 and a lone surrogate escape, which converting
        // or decoding rejects, and brackets inside strings do not nest.
        (
            r#"{"jsonrpc":"2.0","id":7,"method":"m","params":{"x":1e400}}"#.into(),
            request(Key::Int(7)),
        ),
        (
            r#"{"jsonrpc":"2.0","id":7,"method":"m","params":{"\ud800":"\udc00\ud800"}}"#.into(),
            request(Key::Int(7)),
        ),
        (
            format!(
                r#"{{"jsonrpc":"2.0","id":7,"method":"m","params":["\"\\{}\\","{}"]}}"#,
                "[".repeat(200),
                "{".repeat(200)
            ),
            request(Key::Int(7)),
        ),
        (
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#.into(),
            Class::Request {
                key: Key::Int(1),
                is_initialize: true,
            },
        ),
        // A bad version or method echoes a valid id.
        (
            r#"{"jsonrpc":"1.0","id":7,"method":"m"}"#.into(),
            invalid(int(7)),
        ),
        (
            r#"{"jsonrpc":2.0,"id":7,"method":"m"}"#.into(),
            invalid(int(7)),
        ),
        (r#"{"id":7,"method":"m"}"#.into(), invalid(int(7))),
        (
            r#"{"jsonrpc":"2.0","id":7,"method":5}"#.into(),
            invalid(int(7)),
        ),
        (
            r#"{"jsonrpc":"2.0","id":7,"method":null}"#.into(),
            invalid(int(7)),
        ),
        (
            r#"{"jsonrpc":"2.0","id":1.5,"method":5}"#.into(),
            invalid(None),
        ),
        (r#"{"jsonrpc":"2.0","id":3}"#.into(), invalid(int(3))),
        ("{}".into(), invalid(None)),
        // Client responses.
        (r#"{"jsonrpc":"2.0","id":1,"result":{}}"#.into(), RESPONSE),
        (
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":1,"message":"x"}}"#.into(),
            RESPONSE,
        ),
        // Notifications.
        (
            r#"{"jsonrpc":"2.0","method":"notifications/unknown"}"#.into(),
            NOTIFICATION,
        ),
        (
            cancelled(r#"{"requestId":4,"reason":"user"}"#),
            Class::Cancel(Key::Int(4)),
        ),
        (
            cancelled(r#"{"reason":{},"requestId":"c4"}"#),
            Class::Cancel(text("c4")),
        ),
        (cancelled(r#"{"requestId":1.5}"#), NOTIFICATION),
        (cancelled("{}"), NOTIFICATION),
        (cancelled("[4]"), NOTIFICATION),
        (cancelled("null"), NOTIFICATION),
        (
            r#"{"jsonrpc":"2.0","method":"notifications/cancelled"}"#.into(),
            NOTIFICATION,
        ),
    ];
    for (line, expected) in rows {
        assert_eq!(classify(line.as_bytes(), false), expected, "{line}");
    }
    let not_utf8 = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"\xff\"}";
    assert_eq!(classify(not_utf8, false), parse_error());
    let initialized = br#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
    assert_eq!(classify(initialized, false), NOTIFICATION);
    assert_eq!(classify(initialized, true), Class::Initialized);
}

#[test]
fn nesting_fails_exactly_where_serde_json_fails() {
    for depth in 120..=136 {
        let nested = format!("{}{}", "[".repeat(depth), "]".repeat(depth));
        let line = format!(r#"{{"jsonrpc":"2.0","id":1,"method":"m","params":{nested}}}"#);
        let expected = match serde_json::from_str::<Value>(&line) {
            Ok(_) => request(Key::Int(1)),
            Err(_) => parse_error(),
        };
        assert_eq!(classify(line.as_bytes(), false), expected, "depth {depth}");
    }
}

#[test]
fn probe_id_reads_the_top_level_id_from_a_truncated_prefix() {
    let pad = "x".repeat(2 * PROBE_BYTES);
    let prefix = |line: String| {
        line.into_bytes()
            .into_iter()
            .take(PROBE_BYTES)
            .collect::<Vec<_>>()
    };
    let rows = [
        (
            format!(r#"{{"jsonrpc":"2.0","id":9,"method":"m","params":"{pad}"}}"#),
            int(9),
        ),
        (
            format!(r#"{{"params":{{"id":5}},"id":"late-but-early","p":"{pad}"}}"#),
            Some(text("late-but-early")),
        ),
        (
            format!(r#"{{"jsonrpc":"2.0","method":"m","params":"{pad}","id":9}}"#),
            None,
        ),
        (format!(r#"{{"id":"{pad}"}}"#), None),
        (format!(r#"{{"id":1.5,"params":"{pad}"}}"#), None),
        (format!(r#"[{{"id":1}},"{pad}"]"#), None),
        (format!("garbage {pad}"), None),
    ];
    for (line, expected) in rows {
        assert_eq!(probe_id(&prefix(line.clone())), expected, "{}", &line[..40]);
    }
    // The prefix keeps the first `kept` bytes of the id: a number it ends
    // may continue past the cut, so only a terminated one is echoed.
    let straddle = |value: &str, kept: usize| {
        let pad = "x".repeat(PROBE_BYTES - r#"{"p":"","id":"#.len() - kept);
        format!(r#"{{"p":"{pad}","id":{value},"q":0}}"#)
    };
    let rows = [
        ("1234", 2, None),
        ("12.5", 2, None),
        ("12e3", 2, None),
        ("-12", 2, None),
        ("92233720368547758070", 19, None),
        ("1234", 4, None),
        ("1234", 5, int(1234)),
        (r#""ab""#, 4, Some(text("ab"))),
    ];
    for (value, kept, expected) in rows {
        let line = straddle(value, kept);
        assert_eq!(
            probe_id(&prefix(line)),
            expected,
            "{value} cut after {kept}"
        );
    }
}

#[test]
fn error_lines_omit_a_missing_id() {
    let line = |reply| String::from_utf8(error_line(&reply).unwrap()).unwrap();
    assert_eq!(
        line(Reply::duplicate(Key::Int(7))),
        r#"{"jsonrpc":"2.0","id":7,"error":{"code":-32600,"message":"Duplicate request id"}}"#
    );
    assert_eq!(
        line(Reply::oversize(10, Some(text("a\"b")))),
        r#"{"jsonrpc":"2.0","id":"a\"b","error":{"code":-32600,"message":"Request exceeds 10 bytes"}}"#
    );
    let Class::Reply(parse) = parse_error() else {
        unreachable!()
    };
    assert_eq!(
        line(parse),
        r#"{"jsonrpc":"2.0","error":{"code":-32700,"message":"Parse error"}}"#
    );
}

// --- line splitting ------------------------------------------------------

/// Returns at most a few KiB per read, so lines straddle many chunks.
struct Trickle<R> {
    inner: R,
    reads: usize,
}

impl<R: Read> Read for Trickle<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.reads += 1;
        let len = buf.len().min(1 + self.reads * 7919 % 4093);
        self.inner.read(&mut buf[..len])
    }
}

fn split(input: &[u8], max_line_bytes: usize) -> Vec<Line> {
    let mut lines = Lines::new(Cursor::new(input.to_vec()), max_line_bytes);
    std::iter::from_fn(|| lines.next_line()).collect()
}

fn whole(bytes: &[u8]) -> Line {
    Line::Whole(bytes.to_vec())
}

#[test]
fn lines_strip_one_cr_skip_blank_lines_and_keep_an_unterminated_tail() {
    assert_eq!(
        split(b"a\r\n\r\n\nb\r\r\n  \n\nc", 100),
        [whole(b"a"), whole(b"b\r"), whole(b"  "), whole(b"c")]
    );
    assert_eq!(split(b"", 100), []);
    assert_eq!(split(b"\n\r\n", 100), []);
}

#[test]
fn oversize_lines_keep_only_the_probe_prefix() {
    assert_eq!(
        split(b"0123456789\n01234567890\nok", 10),
        [
            whole(b"0123456789"),
            Line::Oversize(b"01234567890".to_vec()),
            whole(b"ok")
        ]
    );
    let long: Vec<u8> = (0..3 * READ_CHUNK + 5)
        .map(|n| b'a' + (n % 26) as u8)
        .collect();
    let mut input = long.clone();
    input.extend_from_slice(b"\nnext");
    assert_eq!(
        split(&input, 1000),
        [Line::Oversize(long[..PROBE_BYTES].to_vec()), whole(b"next")]
    );
}

/// The test hook for the buffer bound: a returned line still has the
/// capacity its buffer grew to, so no line ever held more than
/// `max_line_bytes + READ_CHUNK`.
#[test]
fn line_buffers_never_exceed_the_limit_plus_one_chunk() {
    for max in [1, 100, READ_CHUNK - 1, READ_CHUNK, 3 * READ_CHUNK + 7] {
        let lengths = [1, max, max + 1, max + READ_CHUNK, 4 * READ_CHUNK + 3];
        let mut input = Vec::new();
        for len in lengths {
            input.extend(std::iter::repeat_n(b'x', len));
            input.push(b'\n');
        }
        let mut lines = Lines::new(
            Trickle {
                inner: Cursor::new(input),
                reads: 0,
            },
            max,
        );
        for len in lengths {
            let (bytes, oversize) = match lines.next_line().unwrap() {
                Line::Whole(bytes) => (bytes, false),
                Line::Oversize(bytes) => (bytes, true),
            };
            assert_eq!(oversize, len > max, "max {max}, len {len}");
            assert!(
                bytes.capacity() <= max + READ_CHUNK,
                "max {max}, len {len}: capacity {}",
                bytes.capacity()
            );
        }
        assert_eq!(lines.next_line(), None);
    }
}

// --- tracker -------------------------------------------------------------

fn limits(max_outstanding: usize, max_retained_bytes: usize) -> Limits {
    Limits {
        max_outstanding,
        max_retained_bytes,
        ..Limits::default()
    }
}

#[test]
fn the_tracker_admits_refuses_duplicates_and_counts() {
    let tracker = Tracker::new(&limits(2, 100));
    assert_eq!(tracker.admit(&Key::Int(1), 10), Admit::Ok);
    assert_eq!(tracker.admit(&Key::Int(1), 10), Admit::Duplicate);
    assert_eq!(tracker.admit(&text("1"), 10), Admit::Ok);
    assert_eq!(tracker.outstanding(), 2);
    tracker.complete(&Key::Int(1));
    tracker.complete(&Key::Int(1));
    assert_eq!(tracker.outstanding(), 1);
    assert_eq!(tracker.high_water(), 2);
    tracker.close();
    assert_eq!(tracker.admit(&Key::Int(5), 1), Admit::Closed);
    // Zero outstanding requests would never admit anything.
    assert_eq!(
        Tracker::new(&limits(0, 0)).admit(&Key::Int(1), 9),
        Admit::Ok
    );
}

#[test]
fn the_tracker_marks_cancellation_once() {
    let tracker = Tracker::new(&limits(4, 100));
    assert!(!tracker.cancel(&Key::Int(1)));
    assert_eq!(tracker.admit(&Key::Int(1), 1), Admit::Ok);
    assert!(!tracker.is_cancelled(&Key::Int(1)));
    assert!(tracker.cancel(&Key::Int(1)));
    assert!(!tracker.cancel(&Key::Int(1)));
    // The writer's check leaves the mark, so a repeated cancellation
    // before the slot completes never counts as new.
    assert!(tracker.is_cancelled(&Key::Int(1)));
    assert!(!tracker.cancel(&Key::Int(1)));
    tracker.complete(&Key::Int(1));
    assert!(!tracker.cancel(&Key::Int(1)));
    assert!(!tracker.is_cancelled(&Key::Int(1)));
}

#[test]
fn cancelled_requests_are_not_unanswered() {
    let tracker = Tracker::new(&limits(4, 100));
    assert_eq!(tracker.admit(&Key::Int(1), 1), Admit::Ok);
    assert_eq!(tracker.admit(&text("2"), 1), Admit::Ok);
    assert_eq!(tracker.unanswered(), 2);
    assert!(tracker.cancel(&Key::Int(1)));
    assert_eq!((tracker.outstanding(), tracker.unanswered()), (2, 1));
    tracker.complete(&text("2"));
    assert_eq!((tracker.outstanding(), tracker.unanswered()), (1, 0));
}

/// Admits `key` on another thread and reports the outcome.
fn admit_later(tracker: &Arc<Tracker>, key: Key, bytes: usize) -> std_mpsc::Receiver<Admit> {
    let (sender, receiver) = std_mpsc::channel();
    let tracker = Arc::clone(tracker);
    thread::spawn(move || sender.send(tracker.admit(&key, bytes)).unwrap());
    receiver
}

#[test]
fn admission_waits_for_slots_and_retained_bytes() {
    let tracker = Arc::new(Tracker::new(&limits(2, 100)));
    // One request is admissible on its own, whatever its size.
    assert_eq!(tracker.admit(&Key::Int(1), 1000), Admit::Ok);
    let second = admit_later(&tracker, Key::Int(2), 60);
    assert!(second.recv_timeout(Duration::from_millis(50)).is_err());
    tracker.complete(&Key::Int(1));
    assert_eq!(second.recv_timeout(TIMEOUT), Ok(Admit::Ok));

    // 60 + 60 bytes exceed the budget; then the slot limit applies too.
    let third = admit_later(&tracker, Key::Int(3), 60);
    assert!(third.recv_timeout(Duration::from_millis(50)).is_err());
    tracker.complete(&Key::Int(2));
    assert_eq!(third.recv_timeout(TIMEOUT), Ok(Admit::Ok));
    assert_eq!(tracker.admit(&Key::Int(4), 1), Admit::Ok);
    let fifth = admit_later(&tracker, Key::Int(5), 1);
    assert!(fifth.recv_timeout(Duration::from_millis(50)).is_err());
    tracker.close();
    assert_eq!(fifth.recv_timeout(TIMEOUT), Ok(Admit::Closed));
    assert_eq!(tracker.high_water(), 2);
}

// --- end to end ----------------------------------------------------------

/// Blocks the first write until the test releases it.
struct Gate {
    armed: AtomicBool,
    entered: Barrier,
    release: Barrier,
}

impl Gate {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            armed: AtomicBool::new(true),
            entered: Barrier::new(2),
            release: Barrier::new(2),
        })
    }
}

/// The writer's output, shared with the test.
#[derive(Clone, Default)]
struct Sink {
    bytes: Arc<Mutex<Vec<u8>>>,
    gate: Option<Arc<Gate>>,
    broken: bool,
    flush_fails: bool,
}

impl Sink {
    fn gated(gate: &Arc<Gate>) -> Self {
        Self {
            gate: Some(Arc::clone(gate)),
            ..Self::default()
        }
    }

    fn broken() -> Self {
        Self {
            broken: true,
            ..Self::default()
        }
    }

    /// Every written line, each checked to be a JSON-RPC 2.0 object.
    fn lines(&self) -> Vec<Value> {
        let bytes = self.bytes.lock().unwrap().clone();
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.is_empty() || text.ends_with('\n'));
        text.lines()
            .map(|line| {
                let value: Value = serde_json::from_str(line).unwrap();
                assert_eq!(value["jsonrpc"], "2.0", "{line}");
                assert!(value.is_object());
                value
            })
            .collect()
    }
}

impl Write for Sink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.broken {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        if let Some(gate) = &self.gate
            && gate.armed.swap(false, Ordering::SeqCst)
        {
            gate.entered.wait();
            gate.release.wait();
        }
        self.bytes.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.flush_fails {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        Ok(())
    }
}

/// A connection over a pipe, served by a fake server task that answers
/// each request once an answer permit is available.
struct Harness {
    runtime: Runtime,
    input: Option<PipeWriter>,
    sink: Sink,
    tracker: Arc<Tracker>,
    status: Arc<Status>,
    writer: Option<WriterDone>,
    server: Option<tokio::task::JoinHandle<()>>,
    /// Requests the server received.
    seen: Arc<AtomicUsize>,
    /// Initialized notifications the server received.
    initialized: Arc<AtomicUsize>,
    /// One permit per answer.
    answers: Arc<Semaphore>,
    /// Keys whose answers the server queued.
    answered: std_mpsc::Receiver<Key>,
    /// Keys passed to on_cancel.
    cancels: std_mpsc::Receiver<Key>,
}

const OPEN: usize = 1 << 20;

impl Harness {
    fn new(limits: Limits, sink: Sink, permits: usize) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let (reader, input) = io::pipe().unwrap();
        let (cancel_sender, cancels) = std_mpsc::channel();
        let on_cancel: OnCancel =
            Arc::new(move |key: &Key| cancel_sender.send(key.clone()).unwrap());
        let writer_sink = sink.clone();
        let Connection {
            mut inbound,
            outbound,
            tracker,
            status,
            writer,
        } = start(
            reader,
            move || writer_sink,
            limits,
            Log::silent(),
            Some(on_cancel),
        );
        let seen = Arc::new(AtomicUsize::new(0));
        let initialized = Arc::new(AtomicUsize::new(0));
        let answers = Arc::new(Semaphore::new(permits));
        let (answered_sender, answered) = std_mpsc::channel();
        let server = runtime.spawn({
            let (seen, initialized, answers) = (seen.clone(), initialized.clone(), answers.clone());
            async move {
                while let Some(message) = inbound.recv().await {
                    let key = match message {
                        Inbound::Request { key, .. } => key,
                        Inbound::Initialized { .. } => {
                            initialized.fetch_add(1, Ordering::SeqCst);
                            continue;
                        }
                    };
                    seen.fetch_add(1, Ordering::SeqCst);
                    let (outbound, answers, answered) =
                        (outbound.clone(), answers.clone(), answered_sender.clone());
                    tokio::spawn(async move {
                        answers.acquire().await.unwrap().forget();
                        let line = serde_json::to_vec(&json!({
                            "jsonrpc": "2.0",
                            "id": key,
                            "result": {},
                        }))
                        .unwrap();
                        let answer = Outbound {
                            key: Some(key.clone()),
                            line,
                        };
                        if outbound.send(answer).await.is_ok() {
                            let _ = answered.send(key);
                        }
                    });
                }
            }
        });
        Self {
            runtime,
            input: Some(input),
            sink,
            tracker,
            status,
            writer: Some(writer),
            server: Some(server),
            seen,
            initialized,
            answers,
            answered,
            cancels,
        }
    }

    fn send_raw(&mut self, bytes: &[u8]) {
        self.input.as_mut().unwrap().write_all(bytes).unwrap();
    }

    fn send(&mut self, message: &Value) {
        let mut line = serde_json::to_vec(message).unwrap();
        line.push(b'\n');
        self.send_raw(&line);
    }

    /// Closes the input, answers everything still pending and waits for
    /// the writer to flush. Returns every written line.
    fn finish(&mut self) -> Vec<Value> {
        drop(self.input.take());
        self.answers.add_permits(OPEN);
        let server = self.server.take().unwrap();
        self.runtime.block_on(server).unwrap();
        assert!(self.writer.take().unwrap().wait(TIMEOUT));
        assert!(self.status.eof());
        assert!(!self.status.writer_failed());
        self.sink.lines()
    }
}

fn ping(id: impl Into<Value>) -> Value {
    json!({"jsonrpc": "2.0", "id": id.into(), "method": "ping"})
}

fn cancel(id: impl Into<Value>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "notifications/cancelled",
        "params": {"requestId": id.into()},
    })
}

fn wait_until(what: &str, done: impl Fn() -> bool) {
    let started = Instant::now();
    while !done() {
        assert!(started.elapsed() < TIMEOUT, "timed out waiting for {what}");
        thread::sleep(Duration::from_millis(1));
    }
}

/// How many result lines each id got; panics on any other line.
fn results(lines: &[Value]) -> HashMap<Value, usize> {
    let mut counts = HashMap::new();
    for line in lines {
        assert_eq!(line["result"], json!({}), "{line}");
        *counts.entry(line["id"].clone()).or_insert(0) += 1;
    }
    counts
}

#[test]
fn framing_accepts_crlf_blank_lines_and_an_unterminated_last_line() {
    let mut harness = Harness::new(Limits::default(), Sink::default(), OPEN);
    harness.send_raw(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\r\n\r\n\n");
    harness.send_raw(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"ping\"}");
    let lines = harness.finish();
    assert_eq!(
        results(&lines),
        HashMap::from([(json!(1), 1), (json!(2), 1)])
    );
    let status = Arc::clone(&harness.status);
    harness.runtime.block_on(status.closed());
}

#[test]
fn framing_errors_are_answered_and_responses_and_notifications_dropped() {
    let mut harness = Harness::new(Limits::default(), Sink::default(), OPEN);
    harness.send_raw(b"garbage\n[1]\n");
    harness.send(&json!({"jsonrpc": "2.0", "id": 1, "result": {}}));
    harness.send(&json!({"jsonrpc": "2.0", "method": "notifications/unknown"}));
    harness.send(&json!({"jsonrpc": "1.0", "id": "v", "method": "ping"}));
    harness.send(&ping(5));
    let lines = harness.finish();
    assert_eq!(
        lines,
        [
            json!({"jsonrpc": "2.0", "error": {"code": -32700, "message": "Parse error"}}),
            json!({"jsonrpc": "2.0", "error": {"code": -32600, "message": "Invalid Request"}}),
            json!({"jsonrpc": "2.0", "id": "v", "error": {"code": -32600, "message": "Invalid Request"}}),
            json!({"jsonrpc": "2.0", "id": 5, "result": {}}),
        ]
    );
}

#[test]
fn oversize_lines_get_an_error_and_later_requests_still_run() {
    let limits = Limits {
        max_line_bytes: 1024,
        ..Limits::default()
    };
    let mut harness = Harness::new(limits, Sink::default(), OPEN);
    let pad = "x".repeat(4096);
    harness.send(&json!({"jsonrpc": "2.0", "id": "big", "method": "ping", "params": {"pad": pad}}));
    harness.send(&json!({"jsonrpc": "2.0", "method": "ping", "params": {"pad": pad}}));
    let late = format!(
        r#"{{"jsonrpc":"2.0","method":"ping","params":{{"pad":"{}"}},"id":6}}"#,
        "x".repeat(PROBE_BYTES)
    );
    harness.send_raw(format!("{late}\n").as_bytes());
    harness.send(&ping(3));
    let lines = harness.finish();
    let error = |id: Option<Value>| {
        let mut line = json!({
            "jsonrpc": "2.0",
            "error": {"code": -32600, "message": "Request exceeds 1024 bytes"},
        });
        if let Some(id) = id {
            line["id"] = id;
        }
        line
    };
    assert_eq!(
        lines,
        [
            error(Some(json!("big"))),
            error(None),
            error(None),
            json!({"jsonrpc": "2.0", "id": 3, "result": {}}),
        ]
    );
}

#[test]
fn every_request_is_answered_exactly_once() {
    let mut harness = Harness::new(Limits::default(), Sink::default(), OPEN);
    let mut expected = HashMap::new();
    for n in 0..100 {
        let id = if n % 2 == 0 {
            json!(n)
        } else {
            json!(format!("s{n}"))
        };
        harness.send(&ping(id.clone()));
        expected.insert(id, 1);
    }
    let lines = harness.finish();
    assert_eq!(results(&lines), expected);
    assert_eq!(harness.tracker.outstanding(), 0);
    assert!(harness.tracker.high_water() <= Limits::default().max_outstanding);
}

#[test]
fn a_duplicate_in_flight_id_is_refused_and_the_original_answered_once() {
    let mut harness = Harness::new(Limits::default(), Sink::default(), 0);
    harness.send(&ping(1));
    harness.send(&ping(1));
    wait_until("the duplicate error", || harness.sink.lines().len() == 1);
    let lines = harness.finish();
    assert_eq!(
        lines,
        [
            json!({"jsonrpc": "2.0", "id": 1, "error": {"code": -32600, "message": "Duplicate request id"}}),
            json!({"jsonrpc": "2.0", "id": 1, "result": {}}),
        ]
    );
    assert_eq!(harness.tracker.outstanding(), 0);
}

#[test]
fn a_cancel_before_the_answer_suppresses_it() {
    let mut harness = Harness::new(Limits::default(), Sink::default(), 0);
    harness.send(&ping(7));
    harness.send(&cancel(7));
    assert_eq!(harness.cancels.recv_timeout(TIMEOUT), Ok(Key::Int(7)));
    harness.send(&ping(8));
    let lines = harness.finish();
    assert_eq!(results(&lines), HashMap::from([(json!(8), 1)]));
    assert_eq!(harness.tracker.outstanding(), 0);
    assert!(harness.cancels.try_recv().is_err());
}

#[test]
fn a_cancel_after_the_answer_or_for_an_unknown_id_does_nothing() {
    let mut harness = Harness::new(Limits::default(), Sink::default(), OPEN);
    harness.send(&ping(1));
    wait_until("the answer", || {
        harness.sink.lines().len() == 1 && harness.tracker.outstanding() == 0
    });
    harness.send(&cancel(1));
    harness.send(&cancel(99));
    harness.send(&cancel("unknown"));
    harness.send(&ping(2));
    let lines = harness.finish();
    assert_eq!(
        results(&lines),
        HashMap::from([(json!(1), 1), (json!(2), 1)])
    );
    assert!(harness.cancels.try_recv().is_err());
}

#[test]
fn a_cancel_after_enqueue_but_before_the_write_suppresses_it() {
    let gate = Gate::new();
    let mut harness = Harness::new(Limits::default(), Sink::gated(&gate), 0);
    harness.send(&ping("a"));
    harness.send(&ping("b"));
    // The first queued answer blocks the writer; the second waits behind it.
    harness.answers.add_permits(1);
    let first = harness.answered.recv_timeout(TIMEOUT).unwrap();
    gate.entered.wait();
    harness.answers.add_permits(1);
    let second = harness.answered.recv_timeout(TIMEOUT).unwrap();
    harness.send(&cancel(json!(second)));
    assert_eq!(harness.cancels.recv_timeout(TIMEOUT), Ok(second));
    gate.release.wait();
    let lines = harness.finish();
    assert_eq!(results(&lines), HashMap::from([(json!(first), 1)]));
    assert_eq!(harness.tracker.outstanding(), 0);
}

#[test]
fn request_cancel_cycles_leave_nothing_outstanding() {
    let mut harness = Harness::new(Limits::default(), Sink::default(), OPEN);
    for n in 0..500 {
        harness.send(&ping(n));
        harness.send(&cancel(n));
    }
    let lines = harness.finish();
    assert_eq!(harness.tracker.outstanding(), 0);
    let answered = results(&lines);
    assert!(answered.values().all(|&count| count == 1));
    // A request without a response was cancelled while outstanding.
    let cancelled: HashSet<Value> = harness.cancels.try_iter().map(|key| json!(key)).collect();
    for n in 0..500 {
        assert!(answered.contains_key(&json!(n)) || cancelled.contains(&json!(n)));
    }
}

#[test]
fn a_flood_waits_for_free_slots_and_every_request_is_answered_once() {
    let limits = Limits::default();
    let gate = Gate::new();
    let mut harness = Harness::new(limits, Sink::gated(&gate), OPEN);
    let mut input = harness.input.take().unwrap();
    let flood: JoinHandle<()> = thread::spawn(move || {
        for n in 0..200 {
            let mut line = serde_json::to_vec(&ping(n)).unwrap();
            line.push(b'\n');
            input.write_all(&line).unwrap();
        }
    });
    gate.entered.wait();
    let seen = Arc::clone(&harness.seen);
    wait_until("admitted requests", || {
        seen.load(Ordering::SeqCst) >= limits.max_outstanding
    });
    // Give the reader time to (wrongly) admit more while the writer is blocked.
    thread::sleep(Duration::from_millis(50));
    assert!(harness.seen.load(Ordering::SeqCst) <= limits.max_outstanding + INBOUND_QUEUE);
    assert!(harness.tracker.high_water() <= limits.max_outstanding);
    gate.release.wait();
    flood.join().unwrap();
    let lines = harness.finish();
    let expected: HashMap<Value, usize> = (0..200).map(|n| (json!(n), 1)).collect();
    assert_eq!(results(&lines), expected);
    assert!(harness.tracker.high_water() <= limits.max_outstanding);
    assert_eq!(harness.tracker.outstanding(), 0);
}

#[test]
fn initialized_is_forwarded_only_after_initialize() {
    let mut harness = Harness::new(Limits::default(), Sink::default(), OPEN);
    let initialized = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
    harness.send(&initialized);
    harness.send(&json!({"jsonrpc": "2.0", "id": 1, "method": "initialize"}));
    harness.send(&initialized);
    let lines = harness.finish();
    assert_eq!(results(&lines), HashMap::from([(json!(1), 1)]));
    assert_eq!(harness.initialized.load(Ordering::SeqCst), 1);
}

#[test]
fn a_broken_pipe_closes_admission_and_stops_the_reader() {
    let mut harness = Harness::new(Limits::default(), Sink::broken(), OPEN);
    harness.send(&ping(1));
    let status = Arc::clone(&harness.status);
    wait_until("the write failure", || status.writer_failed());
    harness.runtime.block_on(status.closed());
    assert_eq!(harness.tracker.admit(&Key::Int(99), 1), Admit::Closed);
    // The next line stops the reader although the input stays open.
    harness.send(&ping(2));
    let server = harness.server.take().unwrap();
    harness.runtime.block_on(server).unwrap();
    assert!(!harness.writer.take().unwrap().wait(TIMEOUT));
    assert!(!harness.status.eof());
    assert!(harness.sink.lines().is_empty());
}

#[test]
fn the_writer_refuses_multi_line_messages_and_completes_their_slot() {
    let sink = Sink::default();
    let writer_sink = sink.clone();
    let Connection {
        inbound,
        outbound,
        tracker,
        status,
        writer,
    } = start(
        Cursor::new(Vec::new()),
        move || writer_sink,
        Limits::default(),
        Log::silent(),
        None,
    );
    assert_eq!(tracker.admit(&Key::Int(1), 8), Admit::Ok);
    let refused = Outbound {
        key: Some(Key::Int(1)),
        line: b"{\"jsonrpc\":\n\"2.0\"}".to_vec(),
    };
    let kept = Outbound {
        key: None,
        line: br#"{"jsonrpc":"2.0","id":2,"result":{}}"#.to_vec(),
    };
    outbound.blocking_send(refused).unwrap();
    outbound.blocking_send(kept).unwrap();
    drop(outbound);
    // The refused message was not delivered, but nothing failed.
    assert!(!writer.wait(TIMEOUT));
    assert_eq!(
        sink.lines(),
        [json!({"jsonrpc": "2.0", "id": 2, "result": {}})]
    );
    assert_eq!(tracker.outstanding(), 0);
    assert!(!status.writer_failed());
    drop(inbound);
    wait_until("end of input", || status.eof());
}

#[test]
fn a_failed_closing_flush_is_a_writer_failure() {
    let sink = Sink {
        flush_fails: true,
        ..Sink::default()
    };
    let Connection {
        outbound,
        tracker,
        status,
        writer,
        ..
    } = start(
        Cursor::new(Vec::new()),
        move || sink,
        Limits::default(),
        Log::silent(),
        None,
    );
    // Nothing is queued, so only the closing flush runs.
    drop(outbound);
    assert!(!writer.wait(TIMEOUT));
    assert!(status.writer_failed());
    assert_eq!(tracker.admit(&Key::Int(1), 1), Admit::Closed);
}
