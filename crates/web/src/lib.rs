#![forbid(unsafe_code)]
//! Headless ReShiki for the browser: structure import and export, figures and
//! previews, through wasm-bindgen. Each call reads and writes as the CLI and
//! the MCP tools do, so the results match the desktop's. packages/editor
//! wraps and tests these exports.
use reshiki_io::{
    engine::LocalEngine,
    export,
    formats::{self, EXPORT_PNG_PIXELS, SvgText},
    preview,
};
use reshiki_model::{document::Document, scene};
use wasm_bindgen::prelude::*;

/// The largest preview side, as the MCP `render` tool allows.
const MAX_SIDE: u32 = 8192;

/// The editor's SVG preview of a drawing, with a transparent surround and
/// text outlined unless live text is asked for, as the `render` tool draws it.
fn preview_svg(doc: &Document, svg_text: SvgText) -> Result<String, Error> {
    let svg = scene::svg(doc);
    match svg_text {
        SvgText::Live => Ok(svg),
        SvgText::Outline => text(export::outlined_svg(svg).map_err(failed)?),
    }
}

/// The editor's PNG preview on white, scaled up to three times and down to fit
/// within `max_width` × `max_height` pixels and the export pixel budget:
/// [`preview::png`], the code the `render` tool draws with.
fn preview_png(doc: &Document, max_width: u32, max_height: u32) -> Result<Vec<u8>, Error> {
    if !(1..=MAX_SIDE).contains(&max_width) || !(1..=MAX_SIDE).contains(&max_height) {
        return Err(invalid(format!(
            "Preview sides must be 1 to {MAX_SIDE} pixels"
        )));
    }
    preview::png(doc, max_width, max_height, EXPORT_PNG_PIXELS).map_err(failed)
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

/// A JavaScript `Error` whose `code` is the operation error code the MCP
/// tools and the CLI report for the same failure: `invalid_arguments` for an
/// unknown format or option, `failed` for everything else.
type Error = JsValue;

fn coded(code: &str, message: String) -> Error {
    let error = js_sys::Error::new(&message);
    // Setting a property on a new Error cannot fail.
    let _ = js_sys::Reflect::set(&error, &"code".into(), &code.into());
    error.into()
}

fn failed(message: String) -> Error {
    coded("failed", message)
}

fn invalid(message: String) -> Error {
    coded("invalid_arguments", message)
}

fn text(bytes: Vec<u8>) -> Result<String, Error> {
    String::from_utf8(bytes).map_err(|e| failed(e.to_string()))
}

/// `format` when [`formats::read`] accepts it.
fn input(format: &str) -> Result<&str, Error> {
    if formats::INPUT.contains(&format) {
        Ok(format)
    } else {
        Err(invalid(format!("Unknown input format {format}")))
    }
}

/// The SVG text mode: `outline` (also when not given) or `live`.
fn svg_text(mode: Option<String>) -> Result<SvgText, Error> {
    match mode {
        None => Ok(SvgText::Outline),
        Some(mode) => SvgText::named(&mode)
            .ok_or_else(|| invalid("svgText must be \"outline\" or \"live\"".to_owned())),
    }
}

/// Reads `data` as `format`.
async fn document(data: Vec<u8>, format: &str) -> Result<Document, Error> {
    formats::read(data, input(format)?).await.map_err(failed)
}

/// A native drawing and the warnings reported while importing it.
#[wasm_bindgen(getter_with_clone)]
pub struct ReadResult {
    pub drawing: String,
    pub warnings: Vec<String>,
}

/// Read `data` as `format`, keeping the native drawing and import warnings.
#[wasm_bindgen(js_name = read)]
pub async fn read(data: Vec<u8>, format: String) -> Result<ReadResult, Error> {
    let (doc, warnings) =
        formats::read_with_warnings(data, input(&format)?, &LocalEngine::default())
            .await
            .map_err(failed)?;
    let drawing = text(doc.file_json().map_err(failed)?)?;
    Ok(ReadResult { drawing, warnings })
}

/// Convert `data` read as `from` into the format `to`. The formats are those of
/// [`reshiki_io::formats`]; `svg_text` is `outline` (the default) or `live`.
#[wasm_bindgen]
pub async fn convert(
    data: Vec<u8>,
    from: String,
    to: String,
    svg_text_mode: Option<String>,
) -> Result<Vec<u8>, Error> {
    let mode = svg_text(svg_text_mode)?;
    if !formats::OUTPUT.contains(&to.as_str()) {
        return Err(invalid(format!("Unknown output format {to}")));
    }
    let doc = document(data, &from).await?;
    formats::write_with(doc, &to, &LocalEngine::default(), mode)
        .await
        .map_err(failed)
}

/// The editor's SVG preview of `data` read as `format`; `svg_text` is
/// `outline` (the default) or `live`.
#[wasm_bindgen(js_name = renderSvg)]
pub async fn render_svg(
    data: Vec<u8>,
    format: String,
    svg_text_mode: Option<String>,
) -> Result<String, Error> {
    let mode = svg_text(svg_text_mode)?;
    preview_svg(&document(data, &format).await?, mode)
}

/// The editor's PNG preview of `data` read as `format`, within
/// `max_width` × `max_height` (1600 × 1000 when not given).
#[wasm_bindgen(js_name = renderPng)]
pub async fn render_png(
    data: Vec<u8>,
    format: String,
    max_width: Option<u32>,
    max_height: Option<u32>,
) -> Result<Vec<u8>, Error> {
    let doc = document(data, &format).await?;
    preview_png(&doc, max_width.unwrap_or(1600), max_height.unwrap_or(1000))
}

/// The SMILES of `data` read as `format`.
#[wasm_bindgen]
pub async fn smiles(data: Vec<u8>, format: String) -> Result<String, Error> {
    let doc = document(data, &format).await?;
    text(formats::write(doc, "smiles").await.map_err(failed)?)
}

/// The standard InChI of `data` read as `format`.
#[wasm_bindgen]
pub async fn inchi(data: Vec<u8>, format: String) -> Result<String, Error> {
    let doc = document(data, &format).await?;
    text(formats::write(doc, "inchi").await.map_err(failed)?)
}

/// Add a TrueType or OpenType font (or collection) for figures and previews.
/// Text whose own font lacks a glyph, such as Japanese, falls back to it.
/// Returns how many faces it holds.
#[wasm_bindgen(js_name = addFont)]
pub fn add_font(bytes: Vec<u8>) -> Result<usize, Error> {
    reshiki_model::style::add_font(bytes).map_err(failed)
}

/// The ReShiki release version, shared with the application and npm package.
#[wasm_bindgen]
pub fn version() -> String {
    env!("RESHIKI_VERSION").into()
}
