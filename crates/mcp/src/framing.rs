//! Bounded line framing: one JSON-RPC message per line, read and written on
//! dedicated threads, with admission, backpressure and cancellation.
//!
//! [`start`] spawns two std threads for one connection:
//! - `reshiki-mcp-in` reads lines of at most [`Limits::max_line_bytes`]
//!   (every byte before the newline counts, including a trailing `\r`,
//!   which is then stripped). It classifies each line by its head, answers
//!   framing errors itself, admits requests with the [`Tracker`] and
//!   forwards them on [`Connection::inbound`].
//! - `reshiki-mcp-out` writes each [`Outbound`] line followed by `\n` and a
//!   flush, suppresses responses to cancelled requests and completes their
//!   slots.
//!
//! Both threads use `blocking_send`/`blocking_recv`, which must never run
//! inside a tokio runtime; the server side uses the async halves.
//!
//! # Error-code contract
//!
//! The `id` member is omitted, never null, when no valid id can be read
//! (`id?: RequestId` in the 2025-11-25 and 2026-07-28 schemas).
//! - Not UTF-8 or not JSON (including nesting deeper than serde_json's
//!   recursion limit): -32700 `Parse error`, no id.
//! - JSON that is not an object: -32600 `Invalid Request`, no id.
//! - An invalid id (a float, a number outside i64, a bool, null, an object,
//!   an array, or a string over [`MAX_ID_BYTES`] bytes): -32600, no id.
//! - `jsonrpc` other than "2.0" on a request, or `method` missing or not a
//!   string: -32600, echoing the id when it is valid.
//! - An oversize line: -32600 `Request exceeds N bytes`, with the id when
//!   the first [`PROBE_BYTES`] reveal it whole (a number id that the cut
//!   ends is omitted).
//! - A duplicate in-flight id: -32600 `Duplicate request id` with that id.
//! - Client responses and unknown or malformed notifications: no output.
//!
//! The protocol layer adds -32601 for an unknown method, -32602 for
//! malformed params of a known method, an unknown tool or a missing
//! `_meta`, -32022 with `data.supported` for an unsupported version, and
//! -32603 for a panicked tool task. Tool failures are `isError` results.
//!
//! # Backpressure
//!
//! At most [`Limits::max_outstanding`] requests, holding at most
//! [`Limits::max_retained_bytes`] of request lines, are admitted at once; a
//! single request is always admissible when nothing is outstanding. While
//! the limit is reached the reader stops reading, so a
//! `notifications/cancelled` for one of the pending requests is read only
//! once a slot frees. Cancellation is advisory in MCP and operation
//! deadlines bound pending work. Error replies wait for space in the
//! outbound queue, so a client that stops reading its output also stops
//! the reader.
//!
//! # Cancellation
//!
//! `notifications/cancelled` marks an outstanding request and calls the
//! `on_cancel` hook once; the mark stays until the slot completes, so a
//! repeated cancellation never calls it again. The writer then drops that
//! request's response, whenever it arrives, and still completes its slot. A
//! cancellation for an unknown or already answered id is ignored.
use crate::log::{Level, Log};
use head::{Class, Ignored, PARSE_ERROR, Reply};
use serde::{Serialize, Serializer};
use std::{
    io::{self, BufRead, BufReader, Read, Write},
    pin::pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver as DoneReceiver, SyncSender, sync_channel},
    },
    thread,
    time::Duration,
};
use tokio::sync::{Notify, mpsc};
pub use tracker::{Admit, Tracker};

mod head;
#[cfg(test)]
mod tests;
mod tracker;

/// The longest string request id, in bytes.
pub const MAX_ID_BYTES: usize = 256;
/// How much of an oversize line is kept to look for its id.
pub const PROBE_BYTES: usize = 64 * 1024;
/// The reader's buffer size, and so the most a line buffer may exceed
/// [`Limits::max_line_bytes`] by.
pub const READ_CHUNK: usize = 64 * 1024;
/// Admitted requests waiting for the server.
pub const INBOUND_QUEUE: usize = 8;
/// Lines waiting for the writer.
pub const OUTBOUND_QUEUE: usize = 16;

/// A JSON-RPC request id.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Int(i64),
    Str(Arc<str>),
}

impl Serialize for Key {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Int(value) => serializer.serialize_i64(*value),
            Self::Str(text) => serializer.serialize_str(text),
        }
    }
}

/// Connection limits. The defaults are provisional; a later step derives
/// them from the operation layer's budgets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// The longest line read, in bytes, before its newline.
    pub max_line_bytes: usize,
    /// Requests admitted and not yet answered.
    pub max_outstanding: usize,
    /// Request line bytes held by admitted requests.
    pub max_retained_bytes: usize,
    /// The largest response the protocol layer sends.
    pub max_result_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        const MIB: usize = 1024 * 1024;
        Self {
            max_line_bytes: 24 * MIB,
            max_outstanding: 12,
            max_retained_bytes: 48 * MIB,
            max_result_bytes: 48 * MIB,
        }
    }
}

/// A line forwarded to the server, without its newline.
#[derive(Debug, PartialEq, Eq)]
pub enum Inbound {
    /// An admitted request; its slot is held until its response is written
    /// or suppressed.
    Request { key: Key, line: Vec<u8> },
    /// `notifications/initialized` after an `initialize` request.
    Initialized { line: Vec<u8> },
}

/// A line for the writer. `line` holds one JSON message without `\n`;
/// `key` names the request it answers, if any.
#[derive(Debug)]
pub struct Outbound {
    pub key: Option<Key>,
    pub line: Vec<u8>,
}

/// Why a connection is ending.
#[derive(Default)]
pub struct Status {
    eof: AtomicBool,
    writer_failed: AtomicBool,
    changed: Notify,
}

impl Status {
    /// The input ended or failed to read.
    pub fn eof(&self) -> bool {
        self.eof.load(Ordering::SeqCst)
    }

    /// A write to the output failed; nothing more is admitted.
    pub fn writer_failed(&self) -> bool {
        self.writer_failed.load(Ordering::SeqCst)
    }

    /// Resolves once [`Status::eof`] or [`Status::writer_failed`] is set.
    pub async fn closed(&self) {
        loop {
            // Register before checking, so a change in between still wakes us.
            let mut changed = pin!(self.changed.notified());
            changed.as_mut().enable();
            if self.eof() || self.writer_failed() {
                return;
            }
            changed.await;
        }
    }

    fn set(&self, flag: &AtomicBool) {
        flag.store(true, Ordering::SeqCst);
        self.changed.notify_waiters();
    }
}

/// Called once for each newly cancelled outstanding request.
pub type OnCancel = Arc<dyn Fn(&Key) + Send + Sync>;

/// One framed connection.
pub struct Connection {
    pub inbound: mpsc::Receiver<Inbound>,
    pub outbound: mpsc::Sender<Outbound>,
    pub tracker: Arc<Tracker>,
    pub status: Arc<Status>,
    pub writer: WriterDone,
}

/// Signalled when the writer thread ends.
pub struct WriterDone(DoneReceiver<bool>);

impl WriterDone {
    /// Waits up to `timeout` for the writer to end. True only if everything
    /// queued was written and flushed: every outbound sender was dropped, no
    /// write or flush failed and no message was refused. Responses to
    /// cancelled requests are dropped by design and do not count.
    pub fn wait(self, timeout: Duration) -> bool {
        self.0.recv_timeout(timeout).unwrap_or(false)
    }
}

/// Starts the reader and writer threads of one connection. `make_writer`
/// runs on the writer thread, so a lock it takes is held for that thread's
/// lifetime.
pub fn start<R, W, F>(
    reader: R,
    make_writer: F,
    limits: Limits,
    log: Log,
    on_cancel: Option<OnCancel>,
) -> Connection
where
    R: Read + Send + 'static,
    F: FnOnce() -> W + Send + 'static,
    W: Write,
{
    let (inbound_sender, inbound) = mpsc::channel(INBOUND_QUEUE);
    let (outbound, outbound_receiver) = mpsc::channel(OUTBOUND_QUEUE);
    let (done_sender, done) = sync_channel(1);
    let tracker = Arc::new(Tracker::new(&limits));
    let status = Arc::new(Status::default());

    let writer = Writer {
        tracker: Arc::clone(&tracker),
        status: Arc::clone(&status),
        log: log.clone(),
    };
    let spawned = thread::Builder::new()
        .name("reshiki-mcp-out".into())
        .spawn(move || writer.run(make_writer(), outbound_receiver, &done_sender));
    if spawned.is_err() {
        status.set(&status.writer_failed);
        tracker.close();
        log.event(Level::Error, "output thread failed to start", &[]);
    }

    let lines = Lines::new(reader, limits.max_line_bytes);
    let reader = Reader {
        inbound: inbound_sender,
        outbound: outbound.clone(),
        tracker: Arc::clone(&tracker),
        status: Arc::clone(&status),
        log: log.clone(),
        max_line_bytes: limits.max_line_bytes,
        on_cancel,
    };
    let spawned = thread::Builder::new()
        .name("reshiki-mcp-in".into())
        .spawn(move || reader.run(lines));
    if spawned.is_err() {
        status.set(&status.eof);
        log.event(Level::Error, "input thread failed to start", &[]);
    }

    Connection {
        inbound,
        outbound,
        tracker,
        status,
        writer: WriterDone(done),
    }
}

/// One line read from the input, without its newline.
#[derive(Debug, PartialEq, Eq)]
enum Line {
    Whole(Vec<u8>),
    /// Longer than the limit: only the first [`PROBE_BYTES`] are kept.
    Oversize(Vec<u8>),
}

/// Splits input into lines with bounded buffers: a line's buffer never
/// holds more than `max_line_bytes + READ_CHUNK` bytes of capacity.
struct Lines<R> {
    input: BufReader<R>,
    max_line_bytes: usize,
    ended: bool,
}

impl<R: Read> Lines<R> {
    fn new(input: R, max_line_bytes: usize) -> Self {
        Self {
            input: BufReader::with_capacity(READ_CHUNK, input),
            max_line_bytes,
            ended: false,
        }
    }

    /// The next non-empty line, with one trailing `\r` stripped. An
    /// unterminated last line counts; None once the input ended or failed.
    fn next_line(&mut self) -> Option<Line> {
        loop {
            match self.raw()? {
                Line::Whole(mut bytes) => {
                    if bytes.last() == Some(&b'\r') {
                        bytes.pop();
                    }
                    if !bytes.is_empty() {
                        return Some(Line::Whole(bytes));
                    }
                }
                oversize @ Line::Oversize(_) => return Some(oversize),
            }
        }
    }

    fn raw(&mut self) -> Option<Line> {
        if self.ended {
            return None;
        }
        let max_line_bytes = self.max_line_bytes;
        let mut bytes = Vec::new();
        let mut oversize = false;
        loop {
            let chunk = match self.input.fill_buf() {
                Ok(chunk) => chunk,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                // A read error ends the input like EOF.
                Err(_) => &[],
            };
            if chunk.is_empty() {
                self.ended = true;
                return (oversize || !bytes.is_empty()).then(|| Line::new(bytes, oversize));
            }
            let newline = chunk.iter().position(|&byte| byte == b'\n');
            let piece = newline.and_then(|end| chunk.get(..end)).unwrap_or(chunk);
            let used = piece.len();
            append(&mut bytes, &mut oversize, piece, max_line_bytes);
            self.input.consume(used + usize::from(newline.is_some()));
            if newline.is_some() {
                return Some(Line::new(bytes, oversize));
            }
        }
    }
}

impl Line {
    fn new(bytes: Vec<u8>, oversize: bool) -> Self {
        if oversize {
            Self::Oversize(bytes)
        } else {
            Self::Whole(bytes)
        }
    }
}

/// Accumulates a line up to `max_line_bytes`; past that keeps only the
/// first [`PROBE_BYTES`] and discards the rest.
fn append(bytes: &mut Vec<u8>, oversize: &mut bool, piece: &[u8], max_line_bytes: usize) {
    let piece = if *oversize {
        piece
            .get(..PROBE_BYTES.saturating_sub(bytes.len()))
            .unwrap_or(piece)
    } else {
        piece
    };
    // Grow geometrically, but never past the documented bound.
    let needed = bytes.len().saturating_add(piece.len());
    if needed > bytes.capacity() {
        let ceiling = max_line_bytes.saturating_add(READ_CHUNK).max(needed);
        let target = bytes.capacity().saturating_mul(2).max(needed).min(ceiling);
        bytes.reserve_exact(target.saturating_sub(bytes.len()));
    }
    bytes.extend_from_slice(piece);
    if !*oversize && bytes.len() > max_line_bytes {
        *oversize = true;
        bytes.truncate(PROBE_BYTES);
    }
}

struct Reader {
    inbound: mpsc::Sender<Inbound>,
    outbound: mpsc::Sender<Outbound>,
    tracker: Arc<Tracker>,
    status: Arc<Status>,
    log: Log,
    max_line_bytes: usize,
    on_cancel: Option<OnCancel>,
}

impl Reader {
    /// Reads until EOF, a read error, a failed writer or a gone server.
    /// Dropping `self` then drops the inbound sender.
    fn run<R: Read>(self, mut lines: Lines<R>) {
        let mut initialize_seen = false;
        while let Some(line) = lines.next_line() {
            if self.status.writer_failed() || !self.handle(line, &mut initialize_seen) {
                return;
            }
        }
        self.status.set(&self.status.eof);
    }

    /// Acts on one line; false stops the reader.
    fn handle(&self, line: Line, initialize_seen: &mut bool) -> bool {
        let (class, bytes) = match line {
            Line::Whole(bytes) => (head::classify(&bytes, *initialize_seen), bytes),
            Line::Oversize(prefix) => {
                let id = head::probe_id(&prefix);
                (
                    Class::Reply(Reply::oversize(self.max_line_bytes, id)),
                    prefix,
                )
            }
        };
        match class {
            Class::Reply(reply) => self.reply(&reply),
            Class::Request { key, is_initialize } => {
                match self.tracker.admit(&key, bytes.len()) {
                    Admit::Ok => {}
                    Admit::Duplicate => return self.reply(&Reply::duplicate(key)),
                    Admit::Closed => return false,
                }
                let request = Inbound::Request {
                    key: key.clone(),
                    line: bytes,
                };
                if self.inbound.blocking_send(request).is_err() {
                    self.tracker.complete(&key);
                    return false;
                }
                *initialize_seen |= is_initialize;
                true
            }
            Class::Initialized => self
                .inbound
                .blocking_send(Inbound::Initialized { line: bytes })
                .is_ok(),
            Class::Cancel(key) => {
                let marked = self.tracker.cancel(&key);
                if let Some(on_cancel) = self.on_cancel.as_ref().filter(|_| marked) {
                    on_cancel(&key);
                }
                self.log
                    .event(Level::Debug, "cancel", &[("matched", u64::from(marked))]);
                true
            }
            Class::Ignore(Ignored::Notification) => {
                self.log.event(Level::Debug, "ignored notification", &[]);
                true
            }
            Class::Ignore(Ignored::Response) => {
                self.log.event(Level::Debug, "ignored client response", &[]);
                true
            }
        }
    }

    /// Queues an error response, waiting for space; false once the writer
    /// is gone.
    fn reply(&self, reply: &Reply) -> bool {
        let what = if reply.code == PARSE_ERROR {
            "parse error"
        } else {
            "invalid request"
        };
        self.log.event(Level::Debug, what, &[]);
        let Some(line) = error_line(reply) else {
            return true;
        };
        self.outbound
            .blocking_send(Outbound { key: None, line })
            .is_ok()
    }
}

/// `{"jsonrpc":"2.0","id":…,"error":{"code":…,"message":…}}`, with `id`
/// omitted when there is none.
fn error_line(reply: &Reply) -> Option<Vec<u8>> {
    #[derive(Serialize)]
    struct ErrorLine<'a> {
        jsonrpc: &'static str,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<&'a Key>,
        error: ErrorBody<'a>,
    }
    #[derive(Serialize)]
    struct ErrorBody<'a> {
        code: i64,
        message: &'a str,
    }
    serde_json::to_vec(&ErrorLine {
        jsonrpc: "2.0",
        id: reply.id.as_ref(),
        error: ErrorBody {
            code: reply.code,
            message: &reply.message,
        },
    })
    .ok()
}

struct Writer {
    tracker: Arc<Tracker>,
    status: Arc<Status>,
    log: Log,
}

impl Writer {
    fn run<W: Write>(
        self,
        mut out: W,
        mut queue: mpsc::Receiver<Outbound>,
        done: &SyncSender<bool>,
    ) {
        let mut failed = false;
        // A refused message is dropped, but the writer keeps going.
        let mut refused = false;
        while let Some(Outbound { key, mut line }) = queue.blocking_recv() {
            let cancelled = key
                .as_ref()
                .is_some_and(|key| self.tracker.is_cancelled(key));
            if cancelled {
                // The client cancelled this request: send nothing for it.
            } else if line.contains(&b'\n') {
                // serde_json's compact output never contains one.
                self.log
                    .event(Level::Error, "refused multi-line message", &[]);
                refused = true;
            } else {
                line.push(b'\n');
                if out.write_all(&line).and_then(|()| out.flush()).is_err() {
                    self.fail();
                    failed = true;
                }
            }
            if let Some(key) = &key {
                self.tracker.complete(key);
            }
            if failed {
                break;
            }
        }
        if !failed && out.flush().is_err() {
            self.fail();
            failed = true;
        }
        let _ = done.send(!failed && !refused);
    }

    /// A write or flush failed: nothing more is admitted. Called before the
    /// failed message's slot completes, so a blocked admission wakes to the
    /// close rather than to the free slot.
    fn fail(&self) {
        self.status.set(&self.status.writer_failed);
        self.tracker.close();
        self.log.event(Level::Error, "output write failed", &[]);
    }
}
