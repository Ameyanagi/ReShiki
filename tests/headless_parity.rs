//! `reshiki --cli`, `reshiki --mcp`, an in-process HeadlessHost and the
//! library the app exports with give the same results.
//!
//! Depiction and label metrics come from system fonts, so every comparison
//! runs on one machine and no pixels are stored. PNGs are compared as
//! decoded pixels. The inputs are the SMILES CCO, the native drawing
//! tests/fixtures/ui-drawn-ethanol.reshiki and the esterification Proposal
//! of tests/assistant.rs. Every child has an empty data folder and a
//! watchdog, and inherits RESHIKI_INCHI_HELPER as CI sets it
//! (.github/workflows/checks.yml).
#[path = "common/headless.rs"]
mod headless;

use base64::{Engine, engine::general_purpose::STANDARD};
use headless::{McpSession, StderrMode};
use reshiki::{
    assistant::canvas_tools,
    document::{Document, Point},
    engine::LocalEngine,
    export,
};
use reshiki_agent::ops::{
    budget::Budgets,
    headless::HeadlessHost,
    host::{Call, ToolHost},
    result::ToolResult,
    wire::{Principal, RequestId},
};
use serde_json::{Value, json};
use std::{fs, path::Path, process::Output, time::Duration};
use tempfile::TempDir;
use tokio::runtime::Runtime;

/// Long enough for a debug build's first chemistry engine call on CI.
const RUN: Duration = Duration::from_secs(120);
/// How long one MCP response may take; a compose in a debug build is slow.
const LINE_TIMEOUT: Duration = Duration::from_secs(30);
const NATIVE: &str = include_str!("fixtures/ui-drawn-ethanol.reshiki");

/// An input as the CLI names it and as the import tool reads it.
struct Input {
    cli: &'static [&'static str],
    format: &'static str,
    text: &'static str,
}

/// The SMILES CCO and the native drawing, which the CLI reads from
/// `ethanol.reshiki` in the working folder.
const INPUTS: [Input; 2] = [
    Input {
        cli: &["--smiles", "CCO"],
        format: "smiles",
        text: "CCO",
    },
    Input {
        cli: &["ethanol.reshiki"],
        format: "reshiki",
        text: NATIVE,
    },
];

/// A working folder with `ethanol.reshiki` and `proposal.json`.
fn folder() -> TempDir {
    let dir = tempfile::tempdir().expect("working folder");
    fs::write(dir.path().join("ethanol.reshiki"), NATIVE).expect("ethanol.reshiki");
    let proposal = headless::esterification().to_string();
    fs::write(dir.path().join("proposal.json"), proposal).expect("proposal.json");
    dir
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn cli(dir: &Path, args: &[&str]) -> Output {
    headless::run_cli(dir, args, b"", RUN)
}

/// The stdout of `reshiki --cli` with `args`, which must succeed without a
/// warning or error.
fn cli_ok(dir: &Path, args: &[&str]) -> Vec<u8> {
    let output = cli(dir, args);
    let stderr = text(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{args:?}: {stderr}");
    assert!(stderr.is_empty(), "{args:?}: {stderr}");
    output.stdout
}

/// `input`'s CLI tokens followed by `args`.
fn with(input: &Input, args: &[&'static str]) -> Vec<&'static str> {
    [&args[..1], input.cli, &args[1..]].concat()
}

/// One `reshiki --mcp` session in the 2026-07-28 era.
struct Mcp {
    session: McpSession,
    next: i64,
}

impl Mcp {
    fn start() -> Self {
        Self {
            session: McpSession::start(&[], StderrMode::Captured),
            next: 1,
        }
    }

    /// The result of a `tools/call` that must succeed.
    fn ok(&mut self, tool: &str, arguments: Value) -> Value {
        let id = self.next;
        self.next += 1;
        self.session
            .send(headless::modern_call(id, tool, arguments));
        let reply = self.session.recv_for(Some(&json!(id)), LINE_TIMEOUT);
        let result = reply
            .get("result")
            .unwrap_or_else(|| panic!("{tool}: {reply}"))
            .clone();
        assert_eq!(result["isError"], false, "{tool}: {result}");
        result
    }

    fn import(&mut self, input: &Input) -> Value {
        let imported = self.ok(
            "import",
            json!({"format": input.format, "text": input.text}),
        );
        imported["structuredContent"]["value"]["document"].clone()
    }

    /// The SVG text an export of `document` returns.
    fn svg(&mut self, document: &Value) -> Vec<u8> {
        let exported = self.ok(
            "export",
            json!({"document": document, "format": "svg", "pages": null}),
        );
        let resource = &exported["content"][1]["resource"];
        assert_eq!(resource["mimeType"], "image/svg+xml");
        resource["text"]
            .as_str()
            .expect("SVG text")
            .as_bytes()
            .to_vec()
    }
}

/// An in-process HeadlessHost.
struct Local {
    runtime: Runtime,
    host: HeadlessHost,
    next: i64,
}

impl Local {
    fn new() -> Self {
        Self {
            runtime: runtime(),
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

fn runtime() -> Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("runtime")
}

/// A PNG's size and its pixels as 8-bit RGBA.
fn rgba(png: &[u8]) -> (u32, u32, Vec<u8>) {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().expect("a PNG");
    assert_eq!(
        reader.output_color_type(),
        (png::ColorType::Rgba, png::BitDepth::Eight)
    );
    let mut pixels = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut pixels).expect("PNG pixels");
    pixels.truncate(frame.buffer_size());
    (frame.width, frame.height, pixels)
}

/// Asserts two PNGs have the same pixels, without printing them.
fn assert_same_pixels(left: &[u8], right: &[u8], context: &str) {
    let (left, right) = (rgba(left), rgba(right));
    assert_eq!((left.0, left.1), (right.0, right.1), "{context}: size");
    assert!(left.2 == right.2, "{context}: the pixels differ");
}

/// `value` with each `versions` object and each `document` handle replaced
/// by a placeholder.
fn normalized(mut value: Value) -> Value {
    match &mut value {
        Value::Object(map) => {
            for (key, member) in map.iter_mut() {
                match key.as_str() {
                    "versions" => *member = json!("<versions>"),
                    "document" if member.is_string() => *member = json!("<handle>"),
                    _ => *member = normalized(member.take()),
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                *item = normalized(item.take());
            }
        }
        _ => {}
    }
    value
}

#[test]
fn convert_svg_is_the_same_over_cli_mcp_and_in_process() {
    let dir = folder();
    let mut mcp = Mcp::start();
    let mut local = Local::new();
    for input in &INPUTS {
        let cli = cli_ok(dir.path(), &with(input, &["convert", "--to", "svg"]));
        assert!(cli.starts_with(b"<svg"), "{}", text(&cli));
        let document = mcp.import(input);
        assert_eq!(cli, mcp.svg(&document), "{}: CLI and MCP", input.format);
        let imported = local.ok(
            "import",
            json!({"format": input.format, "text": input.text}),
        );
        let document = &imported.value["value"]["document"];
        let exported = local.ok(
            "export",
            json!({"document": document, "format": "svg", "pages": null}),
        );
        let [file] = exported.files.as_slice() else {
            panic!("one file: {:?}", exported.value)
        };
        assert_eq!(cli, file.bytes, "{}: CLI and in-process", input.format);
    }
}

#[test]
fn analyze_json_is_the_same_over_cli_and_mcp() {
    let dir = folder();
    let mut mcp = Mcp::start();
    for input in &INPUTS {
        let stdout = cli_ok(dir.path(), &with(input, &["analyze"]));
        let mut cli: Value = serde_json::from_slice(&stdout).expect("analyze JSON");
        let labels = cli.as_object_mut().expect("analyze object");
        let api = labels.remove("api").expect("api");
        assert_eq!(api["stability"], "experimental");
        assert_eq!(labels.remove("experimental"), Some(Value::Bool(true)));
        let document = mcp.import(input);
        let analyzed = mcp.ok("analyze", json!({"document": document, "ids": null}));
        let structured = analyzed["structuredContent"].clone();
        assert_eq!(cli["value"]["analysis"]["formula"], "C2H6O", "{cli}");
        assert_eq!(normalized(cli), normalized(structured), "{}", input.format);
    }
}

#[test]
fn render_is_the_same_over_cli_and_mcp() {
    let dir = folder();
    let mut mcp = Mcp::start();
    for input in &INPUTS {
        let cli_png = cli_ok(dir.path(), &with(input, &["render", "--to", "png"]));
        let cli_svg = cli_ok(dir.path(), &with(input, &["render", "--to", "svg"]));
        let document = mcp.import(input);
        let render = |format: &str| {
            json!({
                "document": document,
                "format": format,
                "max_width": null,
                "max_height": null,
                "ids": null,
            })
        };
        let rendered = mcp.ok("render", render("png"));
        let image = &rendered["content"][1];
        assert_eq!(image["mimeType"], "image/png");
        let png = STANDARD
            .decode(image["data"].as_str().expect("image data"))
            .expect("base64 image data");
        assert_same_pixels(&cli_png, &png, input.format);
        let rendered = mcp.ok("render", render("svg"));
        let svg = rendered["content"][1]["resource"]["text"]
            .as_str()
            .expect("SVG text");
        assert_eq!(text(&cli_svg), svg, "{}", input.format);
    }
}

#[test]
fn compose_svg_is_the_same_over_cli_and_mcp() {
    let dir = folder();
    let cli = cli_ok(dir.path(), &["compose", "proposal.json", "--to", "svg"]);
    assert!(cli.starts_with(b"<svg"), "{}", text(&cli));
    let mut mcp = Mcp::start();
    let composed = mcp.ok(
        "compose",
        json!({"proposal": headless::esterification(), "style_document": null}),
    );
    let document = composed["structuredContent"]["value"]["document"].clone();
    assert!(
        text(&cli) == text(&mcp.svg(&document)),
        "CLI and MCP differ"
    );
}

#[test]
fn native_figures_match_the_library() {
    let dir = folder();
    let doc = Document::from_native_file(NATIVE.as_bytes()).expect("the native drawing");
    let runtime = runtime();
    let engine = LocalEngine::default();
    let budget = Budgets::default().export_png_pixels;
    for format in ["svg", "png"] {
        let library = runtime
            .block_on(export::publication(
                &engine,
                doc.clone(),
                format,
                false,
                budget,
            ))
            .expect("a publication figure");
        let output = cli(dir.path(), &["convert", "ethanol.reshiki", "--to", format]);
        assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
        // A PNG's size is its one detail, which the CLI reports as a warning.
        let warnings: String = library
            .details
            .iter()
            .map(|detail| format!("reshiki: warning: {detail}\n"))
            .collect();
        assert_eq!(text(&output.stderr), warnings, "{format}");
        // Compare without printing raster bytes on a mismatch.
        assert!(
            output.stdout == library.bytes,
            "{format}: CLI and library differ"
        );
    }
    let cli = cli_ok(dir.path(), &["render", "ethanol.reshiki", "--to", "png"]);
    let library = canvas_tools::image(&doc).expect("a canvas image");
    assert_same_pixels(&cli, &library, "render");
}

#[test]
fn figure_warnings_come_in_ops_order() {
    let dir = tempfile::tempdir().expect("working folder");
    // The aromatic 5-ring of tests/figure_exports.rs stays unresolved, so
    // the export notes the PNG size and then that chemistry needs review.
    let mut ring = Document::default();
    reshiki::editing::ring(&mut ring, Point::default(), 5, true, 42.);
    let file = ring.file_json().expect("native JSON");
    fs::write(dir.path().join("ring.rsk"), &file).expect("ring.rsk");
    let doc = Document::from_native_file(&file).expect("the native drawing");
    let library = runtime()
        .block_on(export::publication(
            &LocalEngine::default(),
            doc,
            "png",
            false,
            Budgets::default().export_png_pixels,
        ))
        .expect("a publication figure");
    let [size, notice] = library.details.as_slice() else {
        panic!("PNG details: {:?}", library.details);
    };
    assert!(size.starts_with("PNG: "), "{size}");
    assert!(
        notice.starts_with("Drawing preserved; chemistry needs review: "),
        "{notice}"
    );
    let output = cli(dir.path(), &["convert", "ring.rsk", "-o", "ring.png"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert!(output.stdout.is_empty(), "{}", text(&output.stdout));
    assert_eq!(
        text(&output.stderr),
        format!("reshiki: warning: {size}\nreshiki: warning: {notice}\n")
    );
    let png = fs::read(dir.path().join("ring.png")).expect("ring.png");
    assert!(png == library.bytes, "CLI and library differ");
}

#[test]
fn a_page_pdf_needs_publication_pages() {
    let dir = folder();
    let output = cli(
        dir.path(),
        &[
            "convert",
            "ethanol.reshiki",
            "--pages",
            "--to",
            "pdf",
            "-o",
            "pages.pdf",
        ],
    );
    assert_eq!(output.status.code(), Some(1), "{}", text(&output.stderr));
    assert!(output.stdout.is_empty(), "{}", text(&output.stdout));
    assert_eq!(
        text(&output.stderr),
        "reshiki: error: Set up publication pages before exporting a page PDF.\n"
    );
    assert!(!dir.path().join("pages.pdf").exists());
}
