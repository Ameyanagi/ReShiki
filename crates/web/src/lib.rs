#![forbid(unsafe_code)]
//! Headless ReShiki for the browser: structure import and export, figures and
//! previews, through wasm-bindgen. Each call reads and writes as the CLI and
//! the MCP tools do, so the results match the desktop's. packages/editor
//! wraps and tests these exports.
use base64::{Engine, engine::general_purpose::STANDARD};
use reshiki_io::{
    compatibility,
    engine::{LocalEngine, Request, text_format},
    export::{self, Publication},
};
use reshiki_model::{
    atom_labels,
    document::{Document, History},
    scene, transaction,
};
use wasm_bindgen::prelude::*;

/// The most PNG pixels a figure export may have, as the MCP `export` tool's
/// default budget allows.
const EXPORT_PNG_PIXELS: u64 = 16_000_000;
/// A preview is never drawn larger than three times the drawing's size.
const MAX_UPSCALE: f32 = 3.;
/// The largest preview side, as the MCP `render` tool allows.
const MAX_SIDE: u32 = 8192;

/// Read `data` as `format`: `rsk` (a native drawing), `mol`, `smiles`, `inchi`,
/// `cdxml`, `cdx` (the binary file), `rxn`, `rsmi` or `auto`, which tells the
/// text formats apart.
async fn read(data: Vec<u8>, format: &str) -> Result<Document, String> {
    if matches!(format, "rsk" | "reshiki") {
        return Document::from_native_file(&data);
    }
    let text = compatibility::import_text(format, data)?;
    let format = match format {
        "auto" => text_format(&text),
        format => format,
    };
    let mut request = Request::import(format, "");
    request.text = Some(text);
    let response = LocalEngine::default().request(request).await?;
    let document = response.document.ok_or("Import returned no drawing")?;
    finalize(&document)
}

/// The engine's drawing as the editor keeps it, as the `import` tool stores it.
fn finalize(response: &Document) -> Result<Document, String> {
    let mut doc = response.clone();
    transaction::reconcile(&mut doc, Document::default())
        .map_err(|rejection| rejection.message().to_owned())?
        .commit(&mut doc, &mut History::default(), false);
    atom_labels::refresh_computed(&mut doc, response);
    Ok(doc)
}

/// Write `doc` as `format`: `rsk`, a figure (`svg`, `png` or `pdf`, as the
/// app's Export makes it) or a chemistry format (`mol`, `smiles`, `inchi`,
/// `cdxml`, `cdx`, `rxn` or `rsmi`).
async fn write(doc: Document, format: &str) -> Result<Vec<u8>, String> {
    let engine = LocalEngine::default();
    if let Some(figure) = ["svg", "png", "pdf"].into_iter().find(|f| *f == format) {
        return export::publication(&engine, doc, figure, false, EXPORT_PNG_PIXELS)
            .await
            .map(|Publication { bytes, .. }| bytes);
    }
    match format {
        "rsk" | "reshiki" => doc.file_json(),
        "mol" | "smiles" | "inchi" | "cdxml" | "cdx" | "rxn" | "rsmi" => {
            let mut request = Request::molecule("export", doc);
            request.format = Some(format.into());
            let output = engine
                .request(request)
                .await?
                .output
                .ok_or_else(|| format!("The chemistry engine returned no {format} output"))?;
            if format == "cdx" {
                STANDARD
                    .decode(output)
                    .map_err(|_| "Invalid binary drawing base64".to_owned())
            } else {
                Ok(output.into_bytes())
            }
        }
        format => Err(format!("Unknown output format {format}")),
    }
}

/// The editor's SVG preview of a drawing, with a transparent surround.
fn preview_svg(doc: &Document) -> String {
    scene::svg(doc)
}

/// The editor's PNG preview on white, scaled up to three times and down to fit
/// within `max_width` × `max_height` pixels and the export pixel budget, as the
/// `render` tool draws it.
fn preview_png(doc: &Document, max_width: u32, max_height: u32) -> Result<Vec<u8>, String> {
    if !(1..=MAX_SIDE).contains(&max_width) || !(1..=MAX_SIDE).contains(&max_height) {
        return Err(format!("Preview sides must be 1 to {MAX_SIDE} pixels"));
    }
    let tree = export::parse_svg(scene::svg(doc))?;
    let size = tree.size();
    // Within the export pixel budget too, as the desktop render path is.
    let budget = (EXPORT_PNG_PIXELS as f32 / (size.width() * size.height())).sqrt();
    let scale = (max_width as f32 / size.width())
        .min(max_height as f32 / size.height())
        .min(MAX_UPSCALE)
        .min(budget);
    let width = (tree.size().width() * scale)
        .ceil()
        .clamp(1., max_width as f32) as u32;
    let height = (tree.size().height() * scale)
        .ceil()
        .clamp(1., max_height as f32) as u32;
    let mut pixels =
        resvg::tiny_skia::Pixmap::new(width, height).ok_or("Could not create canvas image")?;
    pixels.fill(resvg::tiny_skia::Color::WHITE);
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixels.as_mut(),
    );
    pixels.encode_png().map_err(|e| e.to_string())
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn error(message: &str);
}

/// Report panics on the console; the browser build aborts on them.
#[wasm_bindgen(start)]
fn start() {
    std::panic::set_hook(Box::new(|info| error(&info.to_string())));
}

fn text(bytes: Vec<u8>) -> Result<String, String> {
    String::from_utf8(bytes).map_err(|e| e.to_string())
}

fn js(error: String) -> JsError {
    JsError::new(&error)
}

/// Convert `data` read as `from` into the format `to`. The formats are those of
/// [`read`] and [`write`].
#[wasm_bindgen]
pub async fn convert(data: Vec<u8>, from: String, to: String) -> Result<Vec<u8>, JsError> {
    write(read(data, &from).await.map_err(js)?, &to)
        .await
        .map_err(js)
}

/// The editor's SVG preview of `data` read as `format`.
#[wasm_bindgen(js_name = renderSvg)]
pub async fn render_svg(data: Vec<u8>, format: String) -> Result<String, JsError> {
    Ok(preview_svg(&read(data, &format).await.map_err(js)?))
}

/// The editor's PNG preview of `data` read as `format`, within
/// `max_width` × `max_height` (1600 × 1000 when not given).
#[wasm_bindgen(js_name = renderPng)]
pub async fn render_png(
    data: Vec<u8>,
    format: String,
    max_width: Option<u32>,
    max_height: Option<u32>,
) -> Result<Vec<u8>, JsError> {
    let doc = read(data, &format).await.map_err(js)?;
    preview_png(&doc, max_width.unwrap_or(1600), max_height.unwrap_or(1000)).map_err(js)
}

/// The SMILES of `data` read as `format`.
#[wasm_bindgen]
pub async fn smiles(data: Vec<u8>, format: String) -> Result<String, JsError> {
    let doc = read(data, &format).await.map_err(js)?;
    text(write(doc, "smiles").await.map_err(js)?).map_err(js)
}

/// The standard InChI of `data` read as `format`.
#[wasm_bindgen]
pub async fn inchi(data: Vec<u8>, format: String) -> Result<String, JsError> {
    let doc = read(data, &format).await.map_err(js)?;
    text(write(doc, "inchi").await.map_err(js)?).map_err(js)
}

/// This library's version.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").into()
}
