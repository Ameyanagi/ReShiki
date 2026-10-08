//! A scripted [`ToolHost`] for the transport's tests, shared by the crate's
//! unit tests and its integration tests.
//!
//! - `echo` returns its arguments as the value; `png` an image, `pdf` a
//!   binary file and `svg` a text file.
//! - `fail_<code>` returns an [`OpError`] of the [`ErrorKind`] with that
//!   code, one tool per kind.
//! - `slow` runs a blocking job that meets the test at [`FakeHost::started`]
//!   and then waits for it at [`FakeHost::release`]; it returns `cancelled`
//!   once the host was asked to cancel it.
//! - `panic` panics inside the call.
//! - `big` returns `{text}` with `n` characters of quote-heavy text.
// Each test crate that includes this module uses a subset of it.
#![allow(dead_code)]
use reshiki_agent::{
    envelope::Versions,
    ops::{
        error::{ErrorKind, OpError},
        host::{Call, ToolHost},
        result::{Blob, Image, ToolResult},
        wire::{Principal, RequestId},
    },
    tool_spec::{Hints, ToolSpec},
};
use serde_json::{Map, Value, json};
use std::sync::{
    Arc, Barrier, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

/// The application version the tests serve.
pub const APP_VERSION: &str = "0.0.0-test";

/// The png tool's image: not a real PNG, but bytes base64 must keep.
pub const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\xff\xfe";
/// The pdf tool's file, with bytes that are not UTF-8.
pub const PDF: &[u8] = b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n\x00\xff";
/// The svg tool's file: UTF-8 text with characters JSON escapes.
pub const SVG: &str =
    "<svg xmlns=\"http://www.w3.org/2000/svg\"><title>\"\u{3b1}\" \\ &amp;\n</title></svg>";

/// Every error kind, in declaration order, with its scripted tool.
pub const FAILURES: [(ErrorKind, &str); 13] = [
    (ErrorKind::UnknownTool, "fail_unknown_tool"),
    (ErrorKind::InvalidArguments, "fail_invalid_arguments"),
    (ErrorKind::UnknownDocument, "fail_unknown_document"),
    (ErrorKind::UnknownObject, "fail_unknown_object"),
    (ErrorKind::Stale, "fail_stale"),
    (ErrorKind::Busy, "fail_busy"),
    (ErrorKind::Budget, "fail_budget"),
    (ErrorKind::Timeout, "fail_timeout"),
    (ErrorKind::Rejected, "fail_rejected"),
    (ErrorKind::Unsupported, "fail_unsupported"),
    (ErrorKind::Access, "fail_access_denied"),
    (ErrorKind::Failed, "fail_failed"),
    (ErrorKind::Cancelled, "fail_cancelled"),
];

fn any_object() -> Value {
    json!({"type": "object"})
}

fn no_arguments() -> Value {
    json!({"type": "object", "properties": {}, "additionalProperties": false})
}

fn big_schema() -> Value {
    json!({
        "type": "object",
        "properties": {"n": {"type": "integer", "minimum": 0}},
        "required": ["n"],
        "additionalProperties": false,
    })
}

const fn tool(name: &'static str, input_schema: fn() -> Value, hints: Option<Hints>) -> ToolSpec {
    ToolSpec {
        name,
        title: None,
        description: "A scripted test tool.",
        input_schema,
        hints,
    }
}

const fn hints(read_only: bool, destructive: bool, idempotent: bool, open_world: bool) -> Hints {
    Hints {
        read_only,
        destructive,
        idempotent,
        open_world,
    }
}

/// The scripted tools, in the order they are listed.
pub const SPECS: &[ToolSpec] = &[
    ToolSpec {
        name: "echo",
        title: Some("Echo"),
        description: "Returns its arguments.",
        input_schema: any_object,
        hints: Some(hints(true, false, true, false)),
    },
    tool("png", no_arguments, Some(hints(false, true, false, true))),
    tool("pdf", no_arguments, Some(hints(false, false, true, false))),
    tool("svg", no_arguments, Some(hints(true, true, false, true))),
    tool("fail_unknown_tool", no_arguments, None),
    tool("fail_invalid_arguments", no_arguments, None),
    tool("fail_unknown_document", no_arguments, None),
    tool("fail_unknown_object", no_arguments, None),
    tool("fail_stale", no_arguments, None),
    tool("fail_busy", no_arguments, None),
    tool("fail_budget", no_arguments, None),
    tool("fail_timeout", no_arguments, None),
    tool("fail_rejected", no_arguments, None),
    tool("fail_unsupported", no_arguments, None),
    tool("fail_access_denied", no_arguments, None),
    tool("fail_failed", no_arguments, None),
    tool("fail_cancelled", no_arguments, None),
    tool("slow", no_arguments, None),
    tool("panic", no_arguments, None),
    tool("big", big_schema, None),
];

/// The versions the scripted results report.
pub fn versions() -> Versions {
    Versions::current(APP_VERSION)
}

/// One call as the host received it.
#[derive(Debug, Clone, PartialEq)]
pub struct Received {
    pub principal: Principal,
    pub request: RequestId,
    pub tool: String,
    pub arguments: Value,
}

pub struct FakeHost {
    specs: &'static [ToolSpec],
    /// Every call, in arrival order.
    pub calls: Mutex<Vec<Received>>,
    /// Every cancellation, in arrival order.
    pub cancels: Mutex<Vec<(Principal, RequestId)>>,
    /// Met by each `slow` job and the test once the job runs.
    pub started: Arc<Barrier>,
    /// Met by each `slow` job and the test to let the job end.
    pub release: Arc<Barrier>,
    /// `slow` jobs still running.
    pub live_jobs: Arc<AtomicUsize>,
    /// Set if a `slow` call's future was dropped while its job ran.
    pub dropped_while_live: Arc<AtomicBool>,
}

impl Default for FakeHost {
    fn default() -> Self {
        Self::with_catalog(SPECS)
    }
}

impl FakeHost {
    /// A host listing `specs`; its calls run the scripted tools by name.
    pub fn with_catalog(specs: &'static [ToolSpec]) -> Self {
        Self {
            specs,
            calls: Mutex::default(),
            cancels: Mutex::default(),
            started: Arc::new(Barrier::new(2)),
            release: Arc::new(Barrier::new(2)),
            live_jobs: Arc::default(),
            dropped_while_live: Arc::default(),
        }
    }

    fn cancelled(&self, who: &Principal, id: &RequestId) -> bool {
        self.cancels
            .lock()
            .unwrap()
            .iter()
            .any(|(principal, request)| principal == who && request == id)
    }

    async fn slow(&self, who: &Principal, id: &RequestId) -> Result<ToolResult, OpError> {
        self.live_jobs.fetch_add(1, Ordering::SeqCst);
        let mut watch = Watch {
            live: Arc::clone(&self.live_jobs),
            dropped_while_live: Arc::clone(&self.dropped_while_live),
            finished: false,
        };
        let started = Arc::clone(&self.started);
        let release = Arc::clone(&self.release);
        let live = Arc::clone(&self.live_jobs);
        let job = tokio::task::spawn_blocking(move || {
            started.wait();
            release.wait();
            live.fetch_sub(1, Ordering::SeqCst);
        });
        let joined = job.await;
        watch.finished = true;
        if joined.is_err() {
            return Err(OpError::new(ErrorKind::Failed, "The job failed"));
        }
        if self.cancelled(who, id) {
            return Err(OpError::new(ErrorKind::Cancelled, "Cancelled"));
        }
        Ok(value(json!({"slow": "done"})))
    }
}

/// Records whether the call future it lives in was dropped while its job
/// still ran.
struct Watch {
    live: Arc<AtomicUsize>,
    dropped_while_live: Arc<AtomicBool>,
    finished: bool,
}

impl Drop for Watch {
    fn drop(&mut self) {
        if !self.finished && self.live.load(Ordering::SeqCst) > 0 {
            self.dropped_while_live.store(true, Ordering::SeqCst);
        }
    }
}

fn value(value: Value) -> ToolResult {
    let Value::Object(value) = value else {
        panic!("tool values are objects");
    };
    ToolResult {
        value,
        images: Vec::new(),
        files: Vec::new(),
        is_error: false,
    }
}

/// `n` characters of text that JSON escapes heavily.
pub fn quote_heavy(n: usize) -> String {
    ['"', '\\', 'a', '\n', '\u{1}']
        .into_iter()
        .cycle()
        .take(n)
        .collect()
}

impl ToolHost for FakeHost {
    fn catalog(&self) -> &'static [ToolSpec] {
        self.specs
    }

    async fn call(&self, call: Call) -> Result<ToolResult, OpError> {
        let Call {
            principal,
            request,
            tool,
            arguments,
            ..
        } = call;
        self.calls.lock().unwrap().push(Received {
            principal: principal.clone(),
            request: request.clone(),
            tool: tool.clone(),
            arguments: arguments.clone(),
        });
        if let Some((kind, _)) = FAILURES.iter().find(|(_, name)| *name == tool) {
            return Err(OpError::new(*kind, format!("Scripted {}", kind.code())));
        }
        match tool.as_str() {
            "echo" => Ok(value(arguments)),
            "png" => Ok(ToolResult {
                images: vec![Image {
                    mime: "image/png",
                    bytes: PNG.to_vec(),
                }],
                ..value(json!({"width": 1, "height": 1}))
            }),
            "pdf" => Ok(ToolResult {
                files: vec![Blob {
                    mime: "application/pdf",
                    name: "drawing.pdf".into(),
                    bytes: PDF.to_vec(),
                }],
                ..value(json!({"format": "pdf"}))
            }),
            "svg" => Ok(ToolResult {
                files: vec![Blob {
                    mime: "image/svg+xml",
                    name: "drawing.svg".into(),
                    bytes: SVG.as_bytes().to_vec(),
                }],
                ..value(json!({"format": "svg"}))
            }),
            "slow" => self.slow(&principal, &request).await,
            "panic" => panic!("RESHIKI-CANARY-panic {arguments}"),
            "big" => {
                let n = arguments["n"].as_u64().unwrap();
                let mut text = Map::new();
                text.insert("text".into(), quote_heavy(n as usize).into());
                Ok(value(Value::Object(text)))
            }
            _ => Err(OpError::new(ErrorKind::UnknownTool, "Unknown tool")),
        }
    }

    fn cancel(&self, who: &Principal, id: &RequestId) {
        self.cancels.lock().unwrap().push((who.clone(), id.clone()));
    }

    async fn drained(&self) {}
}
