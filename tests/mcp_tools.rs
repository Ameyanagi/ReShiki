//! The operation tools served by `reshiki --mcp` through the real binary, in
//! the 2026-07-28 era and in the 2025-11-25 era through `initialize`.
//!
//! Every session runs under a 60 s watchdog with an empty data folder and
//! inherits RESHIKI_INCHI_HELPER, as CI sets it
//! (.github/workflows/checks.yml). Values that are the same on every machine
//! are asserted in code. Depiction and label metrics differ between systems
//! (tests/fixtures holds per-OS depiction data; fonts are system fonts), so
//! images and exported files are compared with an in-process HeadlessHost on
//! the same machine, and the goldens tools-modern.jsonl and
//! tools-legacy.jsonl snapshot structure only (see [`structure`]).
#[path = "common/headless.rs"]
mod headless;

use base64::{Engine, engine::general_purpose::STANDARD};
use headless::{McpSession, StderrMode};
use reshiki_agent::{
    Proposal,
    ops::{
        budget::Budgets,
        catalog::SPECS,
        headless::HeadlessHost,
        host::{Call, ToolHost},
        result::ToolResult,
        wire::{Principal, RequestId},
    },
};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::runtime::Runtime;

const LINE_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a cancelled call must stay unanswered.
const CANCEL_QUIET: Duration = Duration::from_secs(3);
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

/// How a client talks to the server.
#[derive(Clone, Copy, Debug)]
enum Era {
    /// 2026-07-28: every request carries `_meta`.
    Modern,
    /// 2025-11-25, negotiated by `initialize`.
    Legacy,
}

/// A client of one `reshiki --mcp` session.
struct Client {
    session: McpSession,
    era: Era,
    next: i64,
}

impl Client {
    /// Starts a session; a legacy client first completes `initialize`.
    fn start(era: Era) -> Self {
        let session = McpSession::start(&[], StderrMode::Captured);
        let mut client = Self {
            session,
            era,
            next: 1,
        };
        if let Era::Legacy = era {
            let reply = client.request(
                "initialize",
                json!({
                    "protocolVersion": "2025-11-25",
                    "capabilities": {},
                    "clientInfo": {"name": "mcp_tools", "version": "1.0.0"},
                }),
            );
            assert_eq!(reply["result"]["protocolVersion"], "2025-11-25", "{reply}");
            client
                .session
                .send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
        }
        client
    }

    /// Sends a request without waiting for its response; returns its id.
    fn send(&mut self, method: &str, mut params: Value) -> i64 {
        if let Era::Modern = self.era {
            params["_meta"] = headless::modern_meta();
        }
        let id = self.next;
        self.next += 1;
        self.session.send(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }));
        id
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.send(method, params);
        self.session.recv_for(Some(&json!(id)), LINE_TIMEOUT)
    }

    /// The result of a `tools/call`; its text copy must match
    /// `structuredContent`.
    fn call(&mut self, tool: &str, arguments: Value) -> Value {
        let reply = self.request("tools/call", json!({"name": tool, "arguments": arguments}));
        let result = reply
            .get("result")
            .unwrap_or_else(|| panic!("{tool}: {reply}"))
            .clone();
        let copy = result["content"][0]["text"].as_str().expect("a text copy");
        let copy: Value = serde_json::from_str(copy).expect("the copy is JSON");
        assert_eq!(copy, result["structuredContent"], "{tool}");
        result
    }

    /// The result of a call that must succeed.
    fn ok(&mut self, tool: &str, arguments: Value) -> Value {
        let result = self.call(tool, arguments);
        assert_eq!(result["isError"], false, "{tool}: {result}");
        result
    }

    /// The `{code, message}` of a call that must end in a tool error.
    fn tool_error(&mut self, tool: &str, arguments: Value) -> Value {
        let result = self.call(tool, arguments);
        assert_eq!(result["isError"], true, "{tool}: {result}");
        assert_eq!(result["content"].as_array().map(Vec::len), Some(1));
        result["structuredContent"]["error"].clone()
    }

    /// Ends the input: the server must exit 0 with nothing more written.
    fn finish(mut self) {
        self.session.close_stdin();
        assert_eq!(self.session.wait_exit(LINE_TIMEOUT).code(), Some(0));
        assert_eq!(self.session.rest(LINE_TIMEOUT), Vec::<Value>::new());
    }
}

/// An in-process HeadlessHost, to compare with on the same machine.
struct Local {
    runtime: Runtime,
    host: HeadlessHost,
    next: i64,
}

impl Local {
    fn new() -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("runtime");
        Self {
            runtime,
            host: HeadlessHost::new(env!("CARGO_PKG_VERSION"), Budgets::default()),
            next: 1,
        }
    }

    fn ok(&mut self, tool: &str, arguments: Value) -> ToolResult {
        let call = Call {
            principal: Principal::local(),
            request: RequestId::Int(self.next),
            tool: tool.into(),
            arguments,
            progress: None,
        };
        self.next += 1;
        let result = self
            .runtime
            .block_on(self.host.call(call))
            .unwrap_or_else(|error| panic!("{tool}: {error}"));
        assert!(!result.is_error, "{tool}: {:?}", result.value);
        result
    }
}

/// The document handle a result's value carries.
fn handle(value: &Value) -> String {
    value["document"].as_str().expect("a handle").to_owned()
}

fn molecule(smiles: &str, label: &str) -> Value {
    json!({"smiles": smiles, "label": label, "coefficient": 1, "rotation": 0, "compact": false})
}

/// The esterification of tests/assistant.rs, with every field spelled out.
fn reaction() -> Value {
    json!({
        "explanation": "Esterification",
        "replace_ids": [],
        "molecules": [],
        "reactions": [{
            "reactants": [molecule("CC(=O)O", "Acetic acid"), molecule("CCO", "Ethanol")],
            "products": [molecule("CCOC(C)=O", "Ethyl acetate"), molecule("O", "Water")],
            "conditions": "H₂SO₄\nheat",
            "arrow": "forward",
            "title": "",
            "role": "main",
            "direction": null,
        }],
        "composition": {"arrangement": "rows", "columns": 2, "width_pt": 540, "preserve_details": false},
        "sketch": null,
    })
}

/// A PNG's width and height, from its IHDR chunk.
fn png_size(png: &[u8]) -> (u32, u32) {
    assert!(png.starts_with(PNG_SIGNATURE), "not a PNG");
    assert_eq!(png.get(12..16), Some(&b"IHDR"[..]));
    let side = |at: usize| u32::from_be_bytes(png[at..at + 4].try_into().expect("4 bytes"));
    (side(16), side(20))
}

/// A canonical decimal object ID: digits only, without a leading zero.
fn assert_decimal(id: &Value) {
    let text = id
        .as_str()
        .unwrap_or_else(|| panic!("{id} is not a string"));
    assert!(
        !text.is_empty() && !text.starts_with('0') && text.bytes().all(|b| b.is_ascii_digit()),
        "{text} is not a decimal ID"
    );
}

/// Asserts every `id` in `value`, and each bond's ends, are decimal IDs;
/// returns how many were checked.
fn assert_decimal_ids(value: &Value) -> usize {
    match value {
        Value::Object(map) => map
            .iter()
            .map(|(key, member)| match (key.as_str(), member) {
                ("id", _) => {
                    assert_decimal(member);
                    1
                }
                ("bonds", Value::Array(bonds)) => {
                    for bond in bonds {
                        assert_decimal(&bond["a"]);
                        assert_decimal(&bond["b"]);
                    }
                    2 * bonds.len() + assert_decimal_ids(member)
                }
                _ => assert_decimal_ids(member),
            })
            .sum(),
        Value::Array(items) => items.iter().map(assert_decimal_ids).sum(),
        _ => 0,
    }
}

fn the_tools_work_end_to_end(era: Era) {
    let mut client = Client::start(era);
    let mut local = Local::new();

    let listed = client.request("tools/list", json!({}));
    let tools = listed["result"]["tools"].as_array().expect("tools");
    let names: Vec<&str> = tools
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    let catalog: Vec<&str> = SPECS.iter().map(|spec| spec.name).collect();
    assert_eq!(names, catalog);
    for (tool, spec) in tools.iter().zip(SPECS) {
        assert_eq!(tool["inputSchema"], (spec.input_schema)(), "{}", spec.name);
        let hints = spec.hints.map(|hints| {
            json!({
                "readOnlyHint": hints.read_only,
                "destructiveHint": hints.destructive,
                "idempotentHint": hints.idempotent,
                "openWorldHint": hints.open_world,
            })
        });
        assert_eq!(tool.get("annotations"), hints.as_ref(), "{}", spec.name);
    }

    let info = client.ok("info", json!({}));
    let info = &info["structuredContent"];
    assert_eq!(info["versions"]["document"], reshiki::document::VERSION);
    assert_eq!(info["operation_api"], 1);

    let import = json!({"format": "smiles", "text": "CCO"});
    let imported = client.ok("import", import.clone());
    let document = handle(&imported["structuredContent"]["value"]);
    assert_eq!(imported["structuredContent"]["value"]["revision"], "0");
    let local_document = handle(&local.ok("import", import).value["value"]);

    let analyzed = client.ok("analyze", json!({"document": document, "ids": null}));
    let analysis = &analyzed["structuredContent"]["value"]["analysis"];
    assert_eq!(analysis["formula"], "C2H6O", "{analysis}");
    assert_eq!(analysis["smiles"], "CCO", "{analysis}");

    let inspected = client.ok("inspect", json!({"document": document, "ids": null}));
    assert!(assert_decimal_ids(&inspected["structuredContent"]) >= 7);

    let render = |document: &str| {
        json!({
            "document": document,
            "format": "png",
            "max_width": null,
            "max_height": null,
            "ids": null,
        })
    };
    let rendered = client.ok("render", render(&document));
    let content = rendered["content"].as_array().expect("content");
    assert_eq!(content.len(), 2, "{content:?}");
    assert_eq!(content[1]["type"], "image");
    assert_eq!(content[1]["mimeType"], "image/png");
    let png = STANDARD
        .decode(content[1]["data"].as_str().expect("image data"))
        .expect("base64 image data");
    let (width, height) = png_size(&png);
    assert!(width <= 1600 && height <= 1000, "{width} × {height}");
    let value = &rendered["structuredContent"]["value"];
    assert_eq!(value["width"], width);
    assert_eq!(value["height"], height);
    assert_eq!(value["byte_len"], png.len());
    let local_render = local.ok("render", render(&local_document));
    let [image] = local_render.images.as_slice() else {
        panic!("one image: {:?}", local_render.value)
    };
    assert!(png == image.bytes, "the transport altered the PNG");

    let export = |document: &str| json!({"document": document, "format": "svg", "pages": null});
    let exported = client.ok("export", export(&document));
    let content = exported["content"].as_array().expect("content");
    assert_eq!(content.len(), 2, "{content:?}");
    let resource = &content[1]["resource"];
    assert_eq!(content[1]["type"], "resource");
    assert_eq!(resource["uri"], "reshiki:result/drawing.svg");
    assert_eq!(resource["mimeType"], "image/svg+xml");
    let local_export = local.ok("export", export(&local_document));
    let [file] = local_export.files.as_slice() else {
        panic!("one file: {:?}", local_export.value)
    };
    assert_eq!(
        resource["text"].as_str().expect("SVG text").as_bytes(),
        file.bytes.as_slice()
    );

    let composed = client.ok(
        "compose",
        json!({"proposal": reaction(), "style_document": null}),
    );
    let value = &composed["structuredContent"]["value"];
    assert_eq!(
        (&value["atoms"], &value["bonds"], &value["arrows"]),
        (&json!(14), &json!(10), &json!(1)),
        "{value}"
    );
    assert!(value["review_issues"].is_array() && value["changes"].is_array());
    let source = handle(value);
    let applied = client.ok(
        "apply",
        json!({
            "document": document,
            "edit": "insert",
            "source": source,
            "ids": null,
            "base_revision": "0",
            "idempotency_key": null,
        }),
    );
    let value = &applied["structuredContent"]["value"];
    assert_eq!(value["effect"], "applied", "{value}");
    assert_eq!(value["revision"], "1", "{value}");

    let mut off_grid = reaction();
    off_grid["reactions"][0]["reactants"][0]["rotation"] = json!(45);
    let expected = serde_json::from_value::<Proposal>(off_grid.clone())
        .expect("a Proposal")
        .validate()
        .expect_err("45 degrees is off the 30-degree grid");
    let error = client.tool_error(
        "compose",
        json!({"proposal": off_grid, "style_document": null}),
    );
    assert_eq!(
        error,
        json!({"code": "invalid_arguments", "message": expected})
    );

    let error = client.tool_error("inspect", json!({"document": "doc_0", "ids": null}));
    assert_eq!(error["code"], "unknown_document", "{error}");

    // The cancellation follows at once; the layout takes far longer.
    let id = client.send(
        "tools/call",
        json!({"name": "compose", "arguments": headless::compose_32()}),
    );
    client.session.send(headless::cancel(id));
    client
        .session
        .assert_no_message_for(Some(&json!(id)), CANCEL_QUIET);
    let analyzed = client.ok("analyze", json!({"document": document, "ids": null}));
    assert!(
        analyzed["structuredContent"]["value"]["analysis"].is_object(),
        "{analyzed}"
    );

    client.finish();
}

#[test]
fn the_tools_work_end_to_end_modern() {
    the_tools_work_end_to_end(Era::Modern);
}

#[test]
fn the_tools_work_end_to_end_legacy() {
    the_tools_work_end_to_end(Era::Legacy);
}

/// `message` reduced to what every machine sends, on top of
/// [`McpSession::normalize`]:
/// - each document handle becomes `<doc#n>`, numbered in order of first
///   appearance in `handles`;
/// - every point `{x, y}` becomes `<point>`, as depiction differs between
///   systems;
/// - image data becomes `<png>` and an exported file's text or blob
///   `<file>`, and a render's `width`, `height` and `byte_len` and an export
///   receipt's `byte_len` become `<size>`: label metrics come from system
///   fonts;
/// - a compose's `review_issues` and `changes` become `<layout>`, as they
///   depend on both.
fn structure(message: Value, handles: &mut Vec<String>) -> Value {
    let mut message = McpSession::normalize(message);
    let Some(result) = message.get_mut("result") else {
        return message;
    };
    if let Some(Value::Array(content)) = result.get_mut("content") {
        for block in content {
            if block["type"] == "image" {
                block["data"] = json!("<png>");
            }
            if let Some(resource) = block.get_mut("resource") {
                for kind in ["text", "blob"] {
                    if let Some(contents) = resource.get_mut(kind) {
                        *contents = json!("<file>");
                    }
                }
            }
        }
    }
    if let Some(structured) = result.get_mut("structuredContent") {
        reduce(structured, handles);
    }
    message
}

fn reduce(value: &mut Value, handles: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            let is_point = map.len() == 2
                && map.get("x").is_some_and(Value::is_number)
                && map.get("y").is_some_and(Value::is_number);
            if is_point {
                *value = json!("<point>");
                return;
            }
            for (key, member) in map.iter_mut() {
                match (key.as_str(), &*member) {
                    ("document", Value::String(handle)) => {
                        *member = json!(placeholder(handle, handles));
                    }
                    ("width" | "height" | "byte_len", Value::Number(_)) => {
                        *member = json!("<size>");
                    }
                    ("review_issues" | "changes", Value::Array(_)) => {
                        *member = json!("<layout>");
                    }
                    _ => reduce(member, handles),
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                reduce(item, handles);
            }
        }
        _ => {}
    }
}

/// `<doc#n>` for the n-th handle seen, counting from 1.
fn placeholder(handle: &str, handles: &mut Vec<String>) -> String {
    let index = match handles.iter().position(|seen| seen == handle) {
        Some(index) => index,
        None => {
            handles.push(handle.to_owned());
            handles.len() - 1
        }
    };
    format!("<doc#{}>", index + 1)
}

/// Runs a golden: `> ` lines are sent, each `<doc#n>` replaced by the n-th
/// handle the server returned, and each `< ` line is compared with the
/// response carrying its id after [`structure`]; `#` lines are comments.
/// The server must then exit 0 on EOF with nothing else written.
fn run_golden(name: &str, transcript: &str) {
    let mut session = McpSession::start(&[], StderrMode::Captured);
    let mut handles: Vec<String> = Vec::new();
    for (index, line) in transcript.lines().enumerate() {
        let context = format!("{name}:{}", index + 1);
        if let Some(sent) = line.strip_prefix("> ") {
            let mut sent = sent.to_owned();
            for (index, handle) in handles.iter().enumerate() {
                sent = sent.replace(&format!("<doc#{}>", index + 1), handle);
            }
            assert!(!sent.contains("<doc#"), "{context}: unknown handle");
            session.send_raw(format!("{sent}\n").as_bytes());
        } else if let Some(expected) = line.strip_prefix("< ") {
            let expected: Value = serde_json::from_str(expected)
                .unwrap_or_else(|error| panic!("{context}: bad golden: {error}"));
            let actual = session.recv_for(expected.get("id"), LINE_TIMEOUT);
            assert_eq!(structure(actual, &mut handles), expected, "{context}");
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
}

#[test]
fn modern_golden() {
    run_golden(
        "tools-modern.jsonl",
        include_str!("fixtures/mcp/tools-modern.jsonl"),
    );
}

#[test]
fn legacy_golden() {
    run_golden(
        "tools-legacy.jsonl",
        include_str!("fixtures/mcp/tools-legacy.jsonl"),
    );
}
