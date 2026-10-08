//! Runs the reshiki binary for headless tests and kills it if it hangs.
// Each test crate that includes this module uses a subset of it.
#![allow(dead_code)]
use serde_json::Value;
use std::{
    collections::VecDeque,
    io::{BufRead, BufReader, Read, Write},
    process::{Child, ChildStderr, ChildStdin, Command, ExitStatus, Output, Stdio},
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use tempfile::TempDir;

/// Starts the reshiki binary with `args`, piping stdout and stderr.
pub fn spawn(args: &[&str], stdin: Stdio) -> Child {
    Command::new(env!("CARGO_BIN_EXE_reshiki"))
        .args(args)
        .stdin(stdin)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start reshiki")
}

/// Collects `child`'s output; kills it and panics if it runs past `limit`.
///
/// The limit also covers the output pipes, which a process `child` started
/// can hold open after `child` exits.
pub fn wait_with_watchdog(mut child: Child, limit: Duration) -> Output {
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let deadline = Instant::now() + limit;
    loop {
        let status = child.try_wait().expect("poll reshiki");
        if let Some(status) = status.filter(|_| stdout.is_finished() && stderr.is_finished()) {
            return Output {
                status,
                stdout: stdout.join().expect("read stdout"),
                stderr: stderr.join().expect("read stderr"),
            };
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("reshiki did not exit and close its output within {limit:?}");
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn drain(pipe: Option<impl Read + Send + 'static>) -> JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        bytes
    })
}

/// How long one [`McpSession`] may live before its child is killed.
pub const MCP_WATCHDOG: Duration = Duration::from_secs(60);

/// What an [`McpSession`] does with the child's stderr.
pub enum StderrMode {
    /// Read on a thread; see [`McpSession::stderr`].
    Captured,
    /// Piped and never read, like a client that ignores stderr.
    Unread,
}

/// What an [`McpSession`] does with the child's stdout.
enum StdoutMode {
    Read,
    /// Read only once [`McpSession::release_stdout`] is called.
    Held,
    /// The read end is closed at once.
    Closed,
}

enum StderrPipe {
    /// Chunks as they are read, and what arrived so far.
    Captured(Receiver<Vec<u8>>, Vec<u8>),
    /// Held open until [`McpSession::unread_stderr`] takes it.
    Unread(Option<ChildStderr>),
}

/// An empty data folder for one child, so a developer's own
/// `agent-access.json` never grants it anything.
pub fn data_dir() -> TempDir {
    tempfile::tempdir().expect("temporary data folder")
}

/// `reshiki --mcp` with piped stdin and stdout. Every stdout line is
/// asserted to be one JSON-RPC 2.0 object when it is received. A watchdog
/// kills the child after [`MCP_WATCHDOG`], so a hung server fails the test
/// instead of stalling it. The child's `RESHIKI_DATA_DIR` is an empty
/// temporary folder.
pub struct McpSession {
    child: Arc<Mutex<Child>>,
    stdin: Option<ChildStdin>,
    lines: Receiver<Vec<u8>>,
    unclaimed: VecDeque<Value>,
    stderr: StderrPipe,
    /// Dropping it lets stdout be read.
    stdout_hold: Option<Sender<()>>,
    /// Dropping it stops the watchdog.
    _watchdog: Sender<()>,
    /// The child's data folder.
    _data: TempDir,
}

impl McpSession {
    /// Starts `reshiki --mcp` followed by `args`.
    pub fn start(args: &[&str], stderr: StderrMode) -> Self {
        Self::spawn(args, stderr, StdoutMode::Read)
    }

    /// Starts `reshiki --mcp` followed by `args` without reading its stdout
    /// until [`McpSession::release_stdout`], like a client that stalls, or
    /// until [`McpSession::close_held_stdout`] closes it unread.
    pub fn with_held_stdout(args: &[&str], stderr: StderrMode) -> Self {
        Self::spawn(args, stderr, StdoutMode::Held)
    }

    /// Starts `reshiki --mcp` followed by `args` and closes the read end of
    /// its stdout at once, like a client that went away.
    pub fn without_stdout(args: &[&str], stderr: StderrMode) -> Self {
        Self::spawn(args, stderr, StdoutMode::Closed)
    }

    fn spawn(args: &[&str], stderr: StderrMode, stdout_mode: StdoutMode) -> Self {
        let data = data_dir();
        let mut child = Command::new(env!("CARGO_BIN_EXE_reshiki"))
            .arg("--mcp")
            .args(args)
            .env("RESHIKI_DATA_DIR", data.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start reshiki --mcp");
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().expect("piped stdout");
        let pipe = child.stderr.take().expect("piped stderr");
        let stderr = match stderr {
            StderrMode::Captured => {
                let (sender, receiver) = mpsc::channel();
                thread::spawn(move || {
                    let mut pipe = pipe;
                    let mut chunk = [0; 4096];
                    while let Ok(read @ 1..) = pipe.read(&mut chunk) {
                        if sender.send(chunk[..read].to_vec()).is_err() {
                            break;
                        }
                    }
                });
                StderrPipe::Captured(receiver, Vec::new())
            }
            StderrMode::Unread => StderrPipe::Unread(Some(pipe)),
        };
        let (sender, lines) = mpsc::channel();
        let (hold, held) = mpsc::channel::<()>();
        let read_stdout = !matches!(stdout_mode, StdoutMode::Closed);
        thread::spawn(move || {
            if !read_stdout {
                return;
            }
            // Ok once the session closes stdout, Err once it drops `hold`.
            if held.recv().is_ok() {
                return;
            }
            let mut stdout = BufReader::new(stdout);
            loop {
                let mut line = Vec::new();
                match stdout.read_until(b'\n', &mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        if sender.send(line).is_err() {
                            break;
                        }
                    }
                }
            }
        });
        let child = Arc::new(Mutex::new(child));
        let (watchdog, stop) = mpsc::channel::<()>();
        let watched = Arc::clone(&child);
        thread::spawn(move || {
            if stop.recv_timeout(MCP_WATCHDOG) == Err(RecvTimeoutError::Timeout) {
                let _ = watched.lock().expect("child lock").kill();
            }
        });
        Self {
            child,
            stdin,
            lines,
            unclaimed: VecDeque::new(),
            stderr,
            stdout_hold: matches!(stdout_mode, StdoutMode::Held).then_some(hold),
            _watchdog: watchdog,
            _data: data,
        }
    }

    /// Sends `message` as one line.
    pub fn send(&mut self, message: Value) {
        let mut line = message.to_string().into_bytes();
        line.push(b'\n');
        self.send_raw(&line);
    }

    /// Writes `bytes` to stdin as they are, without adding a newline.
    pub fn send_raw(&mut self, bytes: &[u8]) {
        let stdin = self.stdin.as_mut().expect("stdin is open");
        stdin.write_all(bytes).expect("write to reshiki --mcp");
        stdin.flush().expect("flush reshiki --mcp stdin");
    }

    /// Waits up to `timeout` for the message whose `id` is `id`, or for the
    /// next one without an `id` when `id` is None. Other messages are kept
    /// for later calls.
    pub fn recv_for(&mut self, id: Option<&Value>, timeout: Duration) -> Value {
        let matches = |message: &Value| message.get("id") == id;
        if let Some(index) = self.unclaimed.iter().position(matches) {
            return self.unclaimed.remove(index).expect("unclaimed message");
        }
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let line = self
                .lines
                .recv_timeout(left)
                .unwrap_or_else(|error| panic!("no message with id {id:?} ({error})"));
            let message = parse_message(&line);
            if matches(&message) {
                return message;
            }
            self.unclaimed.push_back(message);
        }
    }

    /// Asserts that no message with `id` arrives within `duration`.
    pub fn assert_no_message_for(&mut self, id: Option<&Value>, duration: Duration) {
        let deadline = Instant::now() + duration;
        loop {
            assert!(
                self.unclaimed.iter().all(|message| message.get("id") != id),
                "unexpected message with id {id:?}: {:?}",
                self.unclaimed
            );
            let left = deadline.saturating_duration_since(Instant::now());
            match self.lines.recv_timeout(left) {
                Ok(line) => self.unclaimed.push_back(parse_message(&line)),
                Err(_) => return,
            }
        }
    }

    /// Starts reading stdout held by [`McpSession::with_held_stdout`].
    pub fn release_stdout(&mut self) {
        drop(self.stdout_hold.take());
    }

    /// Closes the read end of stdout held by [`McpSession::with_held_stdout`]
    /// without reading it, like a client that went away.
    pub fn close_held_stdout(&mut self) {
        let hold = self.stdout_hold.take().expect("stdout is held");
        hold.send(()).expect("close stdout");
    }

    /// Closes stdin: end of input for the server.
    pub fn close_stdin(&mut self) {
        drop(self.stdin.take());
    }

    /// Takes stdin, for a writer that may block; dropping it closes stdin.
    pub fn take_stdin(&mut self) -> ChildStdin {
        self.stdin.take().expect("stdin is open")
    }

    /// Whether the child is still running.
    pub fn is_running(&mut self) -> bool {
        let status = self.child.lock().expect("child lock").try_wait();
        status.expect("poll reshiki --mcp").is_none()
    }

    /// Waits up to `timeout` for the child to exit; kills it and panics if
    /// it does not.
    pub fn wait_exit(&mut self, timeout: Duration) -> ExitStatus {
        let deadline = Instant::now() + timeout;
        loop {
            let status = self.child.lock().expect("child lock").try_wait();
            if let Some(status) = status.expect("poll reshiki --mcp") {
                return status;
            }
            if Instant::now() >= deadline {
                let mut child = self.child.lock().expect("child lock");
                let _ = child.kill();
                let _ = child.wait();
                panic!("reshiki --mcp did not exit within {timeout:?}");
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    /// Every message not yet received, once stdout closed within `timeout`.
    pub fn rest(&mut self, timeout: Duration) -> Vec<Value> {
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.lines.recv_timeout(left) {
                Ok(line) => self.unclaimed.push_back(parse_message(&line)),
                Err(RecvTimeoutError::Disconnected) => return self.unclaimed.drain(..).collect(),
                Err(RecvTimeoutError::Timeout) => panic!("stdout did not close within {timeout:?}"),
            }
        }
    }

    /// Every message on a complete line not yet received, once stdout closed
    /// within `timeout`. A last line the server could not finish before it
    /// exited is skipped.
    pub fn complete_rest(&mut self, timeout: Duration) -> Vec<Value> {
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.lines.recv_timeout(left) {
                Ok(line) if line.ends_with(b"\n") => {
                    self.unclaimed.push_back(parse_message(&line));
                }
                // Only the last line can lack its newline.
                Ok(_) => {}
                Err(RecvTimeoutError::Disconnected) => return self.unclaimed.drain(..).collect(),
                Err(RecvTimeoutError::Timeout) => panic!("stdout did not close within {timeout:?}"),
            }
        }
    }

    /// Waits up to `timeout` until the captured stderr contains `text`.
    pub fn await_stderr(&mut self, text: &str, timeout: Duration) {
        let StderrPipe::Captured(chunks, seen) = &mut self.stderr else {
            panic!("stderr is not captured");
        };
        let deadline = Instant::now() + timeout;
        while !String::from_utf8_lossy(seen).contains(text) {
            let left = deadline.saturating_duration_since(Instant::now());
            let chunk = chunks
                .recv_timeout(left)
                .unwrap_or_else(|error| panic!("stderr never showed {text:?} ({error})"));
            seen.extend(chunk);
        }
    }

    /// The whole captured stderr, once it closed within `timeout`.
    pub fn stderr(&mut self, timeout: Duration) -> String {
        let StderrPipe::Captured(chunks, seen) = &mut self.stderr else {
            panic!("stderr is not captured");
        };
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match chunks.recv_timeout(left) {
                Ok(chunk) => seen.extend(chunk),
                Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => panic!("stderr did not close within {timeout:?}"),
            }
        }
        String::from_utf8(seen.clone()).expect("UTF-8 stderr")
    }

    /// Everything the child left in the stderr nobody read
    /// ([`StderrMode::Unread`]), once it exited and the pipe closed within
    /// `timeout`.
    pub fn unread_stderr(&mut self, timeout: Duration) -> String {
        let StderrPipe::Unread(pipe) = &mut self.stderr else {
            panic!("stderr is captured");
        };
        let mut pipe = pipe.take().expect("stderr not read yet");
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = pipe.read_to_end(&mut bytes);
            let _ = sender.send(bytes);
        });
        let bytes = receiver
            .recv_timeout(timeout)
            .unwrap_or_else(|error| panic!("stderr did not close within {timeout:?} ({error})"));
        String::from_utf8(bytes).expect("UTF-8 stderr")
    }

    /// `message` with the app version and each tool's definition reduced to
    /// stable placeholders: `serverInfo.version` becomes `<app>` and each
    /// tool becomes its name. In a tool result, `versions.app` becomes
    /// `<app>` and `versions.engine_protocol` `<engine>`, and the text copy
    /// of `structuredContent`, once checked, becomes `<structuredContent>`.
    pub fn normalize(mut message: Value) -> Value {
        if let Some(result) = message.get_mut("result") {
            let modern = result
                .get_mut("_meta")
                .and_then(|meta| meta.get_mut("io.modelcontextprotocol/serverInfo"));
            if let Some(version) = modern.and_then(|info| info.get_mut("version")) {
                *version = Value::from("<app>");
            }
            let legacy = result.get_mut("serverInfo");
            if let Some(version) = legacy.and_then(|info| info.get_mut("version")) {
                *version = Value::from("<app>");
            }
            if let Some(Value::Array(tools)) = result.get_mut("tools") {
                for tool in tools {
                    *tool = tool["name"].clone();
                }
            }
            if let Some(structured) = result.get("structuredContent").cloned() {
                let copy = result["content"][0]["text"]
                    .as_str()
                    .expect("a text copy of structuredContent comes first");
                let copy: Value = serde_json::from_str(copy).expect("the copy is JSON");
                assert_eq!(copy, structured, "the text copy matches structuredContent");
                result["content"][0]["text"] = Value::from("<structuredContent>");
                if let Some(versions) = result["structuredContent"].get_mut("versions") {
                    versions["app"] = Value::from("<app>");
                    versions["engine_protocol"] = Value::from("<engine>");
                }
            }
        }
        message
    }
}

impl Drop for McpSession {
    fn drop(&mut self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// The `_meta` every MCP 2026-07-28 request carries.
pub fn modern_meta() -> Value {
    serde_json::json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientCapabilities": {},
    })
}

/// A 2026-07-28 `tools/call` of `tool` with `arguments`.
pub fn modern_call(id: i64, tool: &str, arguments: Value) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": {"name": tool, "arguments": arguments, "_meta": modern_meta()},
    })
}

/// A 2026-07-28 `tools/list`.
pub fn modern_list(id: i64) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/list",
        "params": {"_meta": modern_meta()},
    })
}

/// `notifications/cancelled` for the request `id`.
pub fn cancel(id: i64) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/cancelled",
        "params": {"requestId": id, "reason": "test"},
    })
}

/// `compose` arguments for a grid of 32 molecules, the most a Proposal
/// allows: long enough a layout for a cancellation or an end of input to
/// arrive while it runs.
pub fn compose_32() -> Value {
    let molecules: Vec<Value> = (1..=32)
        .map(|n| {
            serde_json::json!({
                "smiles": "c1ccc2ccccc2c1C(=O)NCCc1ccccc1",
                "label": format!("Amide {n}"),
                "coefficient": 1,
                "rotation": 0,
                "compact": false,
            })
        })
        .collect();
    serde_json::json!({
        "proposal": {
            "explanation": "Thirty-two amides",
            "replace_ids": [],
            "molecules": molecules,
            "reactions": [],
            "composition": {"arrangement": "grid", "columns": 3, "width_pt": 540, "preserve_details": false},
            "sketch": null,
        },
        "style_document": null,
    })
}

/// One stdout line: a single JSON-RPC 2.0 object ending in a newline.
fn parse_message(line: &[u8]) -> Value {
    let text = std::str::from_utf8(line)
        .unwrap_or_else(|error| panic!("{error}: {}", String::from_utf8_lossy(line)));
    let body = text
        .strip_suffix('\n')
        .unwrap_or_else(|| panic!("unterminated stdout line: {text:?}"));
    let message: Value =
        serde_json::from_str(body).unwrap_or_else(|error| panic!("{error}: {text:?}"));
    assert!(message.is_object(), "not an object: {text:?}");
    assert_eq!(message["jsonrpc"], "2.0", "{text:?}");
    message
}
