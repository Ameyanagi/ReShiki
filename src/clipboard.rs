//! Explicit native Copy/Paste with editable and image representations.
pub mod libreoffice;
use crate::{
    document::Document,
    editing,
    engine::{LocalEngine, Request},
    export,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
#[cfg(not(any(windows, target_os = "linux", target_arch = "wasm32")))]
use std::{path::PathBuf, process::Stdio, time::Duration};
#[cfg(not(any(windows, target_os = "linux", target_arch = "wasm32")))]
use tokio::process::Command;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux::invoke_encoded;

#[cfg(any(target_arch = "wasm32", test))]
mod browser;
mod priority;

const NATIVE: &str = "dev.reshiki.drawing";
const LIMIT: usize = 64 * 1024 * 1024;
const JSON_LIMIT: usize = LIMIT * 2;
#[cfg(test)]
mod browser_tests;
mod copy_as;
pub use copy_as::{
    CopyFormat, PreparedCopy, chemical_snapshot, chemical_text_snapshot, prepare_as,
    prepare_chemical_text, selection_or_drawing, write_prepared,
};
const CDX_TYPES: [&str; 3] = [
    "com.revvity.chemdraw.cdx-clipboard",
    "com.perkinelmer.chemdraw.cdx-clipboard",
    "com.cambridgesoft.cdx",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Representation {
    #[serde(rename = "type")]
    kind: String,
    data: Arc<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    read_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    embedded: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source: Option<String>,
}
impl Representation {
    fn new(kind: &str, data: &[u8]) -> Self {
        Self {
            kind: kind.into(),
            data: STANDARD.encode(data).into(),
            read_error: None,
            embedded: None,
            source: None,
        }
    }
    fn aliases(kinds: &[&str], data: Arc<String>) -> impl Iterator<Item = Self> {
        kinds.iter().map(move |kind| Self {
            kind: (*kind).into(),
            data: Arc::clone(&data),
            read_error: None,
            embedded: None,
            source: None,
        })
    }
    fn bytes(&self) -> Result<Vec<u8>, String> {
        if let Some(error) = &self.read_error {
            return Err(error.chars().take(1000).collect());
        }
        if self.data.len() > LIMIT.div_ceil(3) * 4 {
            return Err("Clipboard data exceeds 64 MB".into());
        }
        let bytes = STANDARD
            .decode(self.data.as_bytes())
            .map_err(|_| "Invalid clipboard encoding".to_owned())?;
        if bytes.len() > LIMIT {
            return Err("Clipboard data exceeds 64 MB".into());
        }
        Ok(bytes)
    }
}
#[derive(Serialize)]
struct CommandRequest<'a> {
    operation: &'a str,
    representations: &'a [Representation],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    formats: &'a [String],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    skip_sources: &'a [String],
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    skip_embedded: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation: Option<&'a str>,
}

fn encode_request(operation: &str, representations: &[Representation]) -> Result<Vec<u8>, String> {
    encode_ordered_request(operation, representations, &[], &[], false, None)
}

fn encode_ordered_request(
    operation: &str,
    representations: &[Representation],
    formats: &[String],
    skip_sources: &[String],
    skip_embedded: bool,
    generation: Option<&str>,
) -> Result<Vec<u8>, String> {
    priority::read_formats(formats, operation == "read_picture")?;
    priority::validate_skip_sources(skip_sources)?;
    priority::validate_generation(generation)?;
    let input = serde_json::to_vec(&CommandRequest {
        operation,
        representations,
        formats,
        skip_sources,
        skip_embedded,
        generation,
    })
    .map_err(|e| e.to_string())?;
    if input.len() > JSON_LIMIT {
        return Err("Clipboard request is too large".into());
    }
    Ok(input)
}

#[derive(Deserialize)]
struct Packet {
    representations: Vec<Representation>,
    #[serde(default)]
    generation: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CopyOutcome {
    pub external_editable: bool,
    pub image_only: bool,
    pub notices: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PasteOutcome {
    pub document: Document,
    pub warnings: Vec<String>,
    /// ReShiki's own drawing data, whose palette colors can follow the target.
    pub native: bool,
}
impl PasteOutcome {
    pub fn native(document: Document) -> Self {
        Self {
            native: true,
            ..document.into()
        }
    }
}
impl From<Document> for PasteOutcome {
    fn from(document: Document) -> Self {
        Self {
            document,
            warnings: Vec::new(),
            native: false,
        }
    }
}

/// A raster-only drawing object carries physical bounds for consumers that
/// ignore PNG resolution metadata. It contains no editable chemical structure.
fn embedded_png(png: &[u8]) -> Result<Vec<u8>, String> {
    if png.len() > 16 * 1024 * 1024 {
        return Err("Sized clipboard image exceeds 16 MB".into());
    }
    let reader = png::Decoder::new(std::io::Cursor::new(png))
        .read_info()
        .map_err(|e| format!("Invalid clipboard PNG: {e}"))?;
    let dimensions = reader
        .info()
        .pixel_dims
        .filter(|d| d.unit == png::Unit::Meter && d.xppu > 0 && d.yppu > 0);
    let (x_dpi, y_dpi) = dimensions
        .map(|d| (f64::from(d.xppu) * 0.0254, f64::from(d.yppu) * 0.0254))
        .unwrap_or_else(|| {
            let dpi = f64::from(crate::style::DEFAULT.png_dpi);
            (dpi, dpi)
        });
    let width = f64::from(reader.info().width) * 72.0 / x_dpi;
    let height = f64::from(reader.info().height) * 72.0 / y_dpi;
    if !width.is_finite()
        || !height.is_finite()
        || width <= 0.0
        || height <= 0.0
        || width > 32_000.0
        || height > 32_000.0
    {
        return Err("Clipboard image dimensions exceed the drawing format range".into());
    }
    let mut bounds = Vec::with_capacity(20);
    bounds.extend_from_slice(&0x0204_u16.to_le_bytes());
    bounds.extend_from_slice(&16_u16.to_le_bytes());
    // CDX rectangles store top, left, bottom, right in 16.16 points.
    for coordinate in [30.0, 30.0, 30.0 + height, 30.0 + width] {
        bounds.extend_from_slice(&((coordinate * 65536.0).round() as i32).to_le_bytes());
    }
    let mut output = b"VjCD0100\x04\x03\x02\x01\0\0\0\0\0\0\0\0\0\0".to_vec();
    for (tag, id) in [(0x8000_u16, 0_u32), (0x8001, 1), (0x8009, 2)] {
        output.extend_from_slice(&tag.to_le_bytes());
        output.extend_from_slice(&id.to_le_bytes());
        if tag != 0x8001 {
            output.extend_from_slice(&bounds);
        }
    }
    output.extend_from_slice(&0x0a70_u16.to_le_bytes());
    if let Ok(length) = u16::try_from(png.len())
        && length < u16::MAX
    {
        output.extend_from_slice(&length.to_le_bytes());
    } else {
        output.extend_from_slice(&u16::MAX.to_le_bytes());
        output.extend_from_slice(
            &u32::try_from(png.len())
                .map_err(|_| "Clipboard image is too large")?
                .to_le_bytes(),
        );
    }
    output.extend_from_slice(png);
    // Embedded object, page, document, and end of stream.
    output.extend_from_slice(&[0; 8]);
    Ok(output)
}

pub fn available() -> bool {
    #[cfg(target_os = "linux")]
    {
        reshiki_linux::clipboard_available()
    }
    #[cfg(not(target_os = "linux"))]
    {
        cfg!(any(target_os = "macos", windows, target_arch = "wasm32"))
    }
}

#[cfg(not(any(windows, target_os = "linux", target_arch = "wasm32")))]
fn helper() -> Result<PathBuf, String> {
    if !cfg!(target_os = "macos") {
        return Err("Native clipboard is unavailable on this platform".into());
    }
    std::env::current_exe().map_err(|e| format!("Could not locate the application: {e}"))
}

async fn invoke(operation: &str, representations: &[Representation]) -> Result<Packet, String> {
    #[cfg(target_arch = "wasm32")]
    if operation.starts_with("write") {
        return browser::write(representations).await;
    }
    let input = encode_request(operation, representations)?;
    invoke_encoded(operation, input).await
}

#[cfg(windows)]
async fn invoke_encoded(_operation: &str, input: Vec<u8>) -> Result<Packet, String> {
    let output = tokio::task::spawn_blocking(move || reshiki_windows::clipboard(&input))
        .await
        .map_err(|e| e.to_string())??;
    serde_json::from_slice(&output).map_err(|e| e.to_string())
}

/// The browser writes and reads through `browser`; nothing else reaches here.
#[cfg(target_arch = "wasm32")]
async fn invoke_encoded(_operation: &str, _input: Vec<u8>) -> Result<Packet, String> {
    Err("The browser cannot do this clipboard operation".into())
}

#[cfg(not(any(windows, target_os = "linux", target_arch = "wasm32")))]
async fn invoke_encoded(_operation: &str, input: Vec<u8>) -> Result<Packet, String> {
    let mut command = Command::new(helper()?);
    command.arg(crate::workers::CLIPBOARD_WORKER);
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Could not start clipboard helper: {e}"))?;
    let stdin = child.stdin.take().ok_or("Clipboard input is unavailable")?;
    let stdout = child
        .stdout
        .take()
        .ok_or("Clipboard output is unavailable")?;
    let stderr = child
        .stderr
        .take()
        .ok_or("Clipboard error output is unavailable")?;
    let operation = async {
        let write = crate::native_process::write(stdin, &input);
        let read = async {
            let output = crate::native_process::capture(stdout, JSON_LIMIT + 1).await?;
            if output.len() > JSON_LIMIT {
                return Err("Clipboard response is too large".into());
            }
            Ok(output)
        };
        let errors = crate::native_process::capture(stderr, 65536);
        let wait = async { child.wait().await.map_err(|e| e.to_string()) };
        let (_, output, errors, status) = tokio::try_join!(write, read, errors, wait)?;
        if !status.success() {
            let message = String::from_utf8_lossy(&errors);
            return Err(if message.trim().is_empty() {
                "Native clipboard operation failed".into()
            } else {
                message.trim().chars().take(500).collect()
            });
        }
        serde_json::from_slice(&output).map_err(|_| "Invalid clipboard response".into())
    };
    tokio::time::timeout(Duration::from_secs(10), operation)
        .await
        .map_err(|_| "Clipboard operation timed out".to_string())?
}

pub async fn copy(
    engine: LocalEngine,
    original: Document,
    image_only: bool,
) -> Result<CopyOutcome, String> {
    let (outcome, representations) = prepare_copy(engine, original, image_only).await?;
    let operation = if cfg!(windows) && !image_only {
        "write_embedded"
    } else {
        "write"
    };
    invoke(operation, &representations).await?;
    Ok(outcome)
}

async fn prepare_copy(
    engine: LocalEngine,
    original: Document,
    image_only: bool,
) -> Result<(CopyOutcome, Vec<Representation>), String> {
    prepare_copy_for(engine, original, image_only, cfg!(target_arch = "wasm32")).await
}

/// A browser can hold neither CDX nor PDF, so its copy has SMILES text for
/// other apps instead, when the drawing has one.
async fn prepare_copy_for(
    engine: LocalEngine,
    original: Document,
    image_only: bool,
    browser: bool,
) -> Result<(CopyOutcome, Vec<Representation>), String> {
    original.validate()?;
    if original.all_ids().is_empty() {
        return Err("There is nothing to copy".into());
    }
    let mut outcome = CopyOutcome {
        external_editable: false,
        image_only,
        notices: vec![],
    };
    let doc = match export::checked_document(&engine, original.clone()).await {
        Ok(doc) => doc,
        Err(error) => {
            outcome
                .notices
                .push(format!("Chemistry could not be refreshed: {error}"));
            original
        }
    };
    // Marked like a saved file, so pasting it never migrates its colors again.
    let doc = doc.current();
    let mut representations = Vec::new();
    if !image_only {
        representations.push(Representation::new(
            NATIVE,
            &serde_json::to_vec(&doc).map_err(|e| e.to_string())?,
        ));
    }
    if !image_only && browser {
        let mut request = Request::molecule("export", doc.clone());
        request.format = Some("smiles".into());
        if !doc.atoms.is_empty()
            && let Ok(smiles) = engine.request(request).await
            && let Some(smiles) = smiles.output.filter(|smiles| !smiles.trim().is_empty())
        {
            representations.push(Representation::new(
                "public.utf8-plain-text",
                smiles.as_bytes(),
            ));
        }
    } else if !image_only {
        // External editors receive explicit visible ink colors, without the
        // source page background. Native data above retains the original theme.
        // Resolve depth paint against the source paper before changing the
        // external snapshot's canvas; a dark drawing fades toward black.
        let exchange_doc = crate::canvas_theme::for_paste(
            crate::depth_appearance::materialize(&doc).into_owned(),
            crate::canvas_theme::CanvasTheme::Light,
        );
        let mut request = Request::molecule("export", exchange_doc.clone());
        request.format = Some("cdx".into());
        let direct = engine.request(request).await.and_then(|response| {
            response
                .output
                .ok_or_else(|| "Missing binary drawing".into())
        });
        let editable = match direct {
            Ok(data) => Ok(data),
            Err(original_error) => {
                let snapshot = exchange_doc;
                let compatible = crate::task::spawn_blocking(move || {
                    let (xml, notices) = crate::exchange::drawing::write_clipboard(&snapshot)
                        .map_err(|e| e.to_string())?;
                    Ok::<_, String>((STANDARD.encode(crate::exchange::to_cdx(&xml)?), notices))
                })
                .await
                .map_err(|e| e.to_string())?;
                match compatible {
                    Ok((data, notices)) => {
                        outcome.notices.extend(notices);
                        Ok(data)
                    }
                    Err(error) => Err(format!(
                        "{original_error} · Compatible copy unavailable: {error}"
                    )),
                }
            }
        };
        match editable {
            Ok(data) => {
                // Include the current and legacy native aliases on one item.
                representations.extend(Representation::aliases(&CDX_TYPES, data.into()));
                outcome.external_editable = true;
            }
            Err(error) => outcome
                .notices
                .push(format!("Editable exchange unavailable: {error}")),
        }
    }
    let images = crate::task::spawn_blocking(move || copy_images(&doc, image_only, browser))
        .await
        .map_err(|e| format!("Could not render clipboard images: {e}"))?;
    let mut image_count = 0;
    for (format, result) in images {
        match result {
            Ok(image) => {
                if format == "png" {
                    let dpi = image
                        .bytes()
                        .ok()
                        .and_then(|bytes| {
                            png::Decoder::new(std::io::Cursor::new(bytes))
                                .read_info()
                                .ok()?
                                .info()
                                .pixel_dims
                        })
                        .filter(|d| d.unit == png::Unit::Meter)
                        .map(|d| (f64::from(d.xppu) * 0.0254).round() as u32);
                    if let Some(dpi) = dpi
                        && dpi < crate::style::DEFAULT.png_dpi
                    {
                        outcome.notices.push(format!(
                            "Raster preview uses {dpi} dpi to fit the clipboard; vector formats retain full quality"
                        ));
                    }
                }
                // Editors that prefer CDX may reject the generic PDF/image
                // flavors. Supply a sized picture when editable exchange is
                // unavailable, without claiming it contains editable atoms.
                if (image_only || !outcome.external_editable) && format == "png" && !browser {
                    match image.bytes().and_then(|bytes| embedded_png(&bytes)) {
                        Ok(bytes) => {
                            representations.extend(Representation::aliases(
                                &CDX_TYPES,
                                STANDARD.encode(&bytes).into(),
                            ));
                            if !image_only {
                                outcome.notices.push("Other drawing editors will receive a picture; ReShiki retains the editable original".into());
                            }
                        }
                        Err(error) => outcome
                            .notices
                            .push(format!("Sized image unavailable: {error}")),
                    }
                }
                representations.push(image);
                if format != "native picture" {
                    image_count += 1;
                }
            }
            Err(error) => outcome
                .notices
                .push(format!("{} unavailable: {error}", format.to_uppercase())),
        }
    }
    if image_count == 0 && image_only {
        return Err(outcome.notices.join(" · "));
    }
    Ok((outcome, representations))
}

fn copy_images(
    doc: &Document,
    image_only: bool,
    browser: bool,
) -> Vec<(&'static str, Result<Representation, String>)> {
    let mut images: Vec<_> = [
        ("com.adobe.pdf", "pdf"),
        ("public.png", "png"),
        ("public.svg-image", "svg"),
    ]
    .into_iter()
    .filter(|(_, format)| !browser || *format != "pdf")
    .map(|(kind, format)| {
        #[cfg(windows)]
        let image = match format {
            "svg" => export::clipboard_svg(doc),
            "png" => export::clipboard_png(doc),
            _ => export::clipboard_drawing(doc, format),
        };
        #[cfg(not(windows))]
        let image = if format == "png" {
            export::clipboard_png(doc)
        } else {
            export::clipboard_drawing(doc, format)
        };
        (format, image.map(|bytes| Representation::new(kind, &bytes)))
    })
    .collect();
    #[cfg(windows)]
    if !image_only && !browser {
        images.push((
            "Office preview",
            crate::native_windows::office_metafile(doc)
                .map(|bytes| Representation::new("dev.reshiki.office-metafile", &bytes)),
        ));
    }
    if image_only && let Some((_, Ok(png))) = images.iter().find(|(format, _)| *format == "png") {
        // Honor the actual PNG resolution, including bounded Windows previews.
        let native = png.bytes().and_then(|bytes| {
            let doc = crate::pictures::clipboard_document(&bytes)?;
            let bytes = serde_json::to_vec(&doc).map_err(|e| e.to_string())?;
            Ok(Representation::new(NATIVE, &bytes))
        });
        images.push(("native picture", native));
    }
    images
}

fn mol_text(data: &[u8]) -> Result<String, String> {
    if let Ok(text) = std::str::from_utf8(data)
        && text.contains("M  END")
        && text.contains('\n')
    {
        return Ok(text.to_owned());
    }
    // Native MOL clipboard data may use length-prefixed MacRoman lines.
    // MOL's supported structural fields are ASCII; reject foreign bytes.
    let mut remaining = data;
    let mut output = String::new();
    while let Some((&length, rest)) = remaining.split_first() {
        let length = usize::from(length);
        let line = rest.get(..length).ok_or("Truncated MOL clipboard line")?;
        if !line.is_ascii() {
            return Err("Unsupported MOL clipboard text encoding".into());
        }
        output.push_str(std::str::from_utf8(line).map_err(|_| "Invalid MOL clipboard text")?);
        output.push('\n');
        remaining = rest.get(length..).ok_or("Invalid MOL clipboard line")?;
    }
    if !output.contains("M  END") {
        return Err("Invalid MOL clipboard data".into());
    }
    Ok(output)
}

pub fn text_request(text: &str) -> Request {
    Request::import(text_format(text), text)
}

pub use crate::engine::text_format;

pub async fn paste(engine: LocalEngine, image_only: bool) -> Result<Document, String> {
    Ok(paste_with_warnings(engine, image_only).await?.document)
}

pub async fn paste_with_warnings(
    engine: LocalEngine,
    image_only: bool,
) -> Result<PasteOutcome, String> {
    read_candidates(image_only, read_packet, move |item| {
        paste_representation(engine.clone(), item)
    })
    .await?
    .ok_or_else(|| "No supported drawing on the clipboard".into())
}

async fn read_packet(
    formats: Vec<String>,
    skip_sources: Vec<String>,
    skip_embedded: bool,
    generation: Option<String>,
) -> Result<Packet, String> {
    // The browser reads its few web types again for each request.
    #[cfg(target_arch = "wasm32")]
    if !formats.is_empty() {
        return browser::read(&formats).await;
    }
    // Explicit picture requests contain only picture formats, including retries.
    let picture_only = formats
        .iter()
        .all(|kind| priority::PICTURES.contains(&kind.as_str()));
    let operation = if picture_only { "read_picture" } else { "read" };
    let input = encode_ordered_request(
        operation,
        &[],
        &formats,
        &skip_sources,
        skip_embedded,
        generation.as_deref(),
    )?;
    invoke_encoded(operation, input).await
}

/// Fetch one candidate at a time so lazy clipboard providers never render an
/// unused picture, and every request/response retains the existing byte limits.
async fn read_candidates<T, Read, ReadFuture, Import, ImportFuture>(
    picture_only: bool,
    mut read: Read,
    mut import: Import,
) -> Result<Option<T>, String>
where
    Read: FnMut(Vec<String>, Vec<String>, bool, Option<String>) -> ReadFuture,
    ReadFuture: std::future::Future<Output = Result<Packet, String>>,
    Import: FnMut(Representation) -> ImportFuture,
    ImportFuture: std::future::Future<Output = Result<T, String>>,
{
    let mut formats: Vec<_> = priority::read_formats(&[], picture_only)?
        .into_iter()
        .map(str::to_owned)
        .collect();
    let mut failure = None;
    let mut skip_sources = Vec::new();
    let mut skip_embedded = false;
    let mut generation: Option<String> = None;
    while !formats.is_empty() {
        let embedded_allowed = !skip_embedded;
        let packet = match read(
            formats.clone(),
            skip_sources.clone(),
            skip_embedded,
            generation.clone(),
        )
        .await
        {
            Ok(packet) => packet,
            Err(error) => {
                if error == priority::CLIPBOARD_CHANGED {
                    return Err(error);
                }
                // A clipboard-owner failure cannot identify another candidate,
                // but must not replace a more useful drawing/update diagnostic.
                remember_failure(&mut failure, "", error);
                break;
            }
        };
        let current = packet
            .generation
            .as_deref()
            .ok_or("Missing clipboard generation")?;
        priority::validate_generation(Some(current))?;
        if generation.as_deref().is_some_and(|first| first != current) {
            return Err(priority::CLIPBOARD_CHANGED.into());
        }
        generation = packet.generation;
        skip_embedded = true;
        if packet.representations.len() > 1 {
            return Err("Invalid clipboard read response".into());
        }
        let Some(item) = packet.representations.into_iter().next() else {
            break;
        };
        if !formats.contains(&item.kind) {
            return Err("The clipboard returned an unrequested representation".into());
        }
        if item.embedded == Some(true) && !embedded_allowed {
            return Err("Invalid repeated Office clipboard response".into());
        }
        // An Office object's failed drawing must still permit standalone data
        // of that same format. The next request skips only the Office object.
        if let Some(source) = &item.source {
            if skip_sources.contains(source) {
                return Err("The clipboard repeated an already read representation".into());
            }
            skip_sources.push(source.clone());
            priority::validate_skip_sources(&skip_sources)?;
        } else if item.embedded != Some(true) {
            formats.retain(|kind| *kind != item.kind);
        }
        let kind = item.kind.clone();
        match import(item).await {
            Ok(value) => return Ok(Some(value)),
            Err(error) => remember_failure(&mut failure, &kind, error),
        }
    }
    match failure {
        Some((_, error)) => Err(error),
        None => Ok(None),
    }
}

fn remember_failure(failure: &mut Option<(usize, String)>, kind: &str, error: String) {
    let rank = if error.contains("newer version of ReShiki") {
        0
    } else if matches!(kind, NATIVE | "dev.moruno.drawing") {
        1
    } else if kind.contains("cdx") || kind == "application/x-chemdraw" {
        2
    } else if matches!(kind, "com.mdli.molfile" | "org.opensmiles.smiles") {
        3
    } else if priority::PICTURES.contains(&kind) {
        4
    } else if kind == "public.utf8-plain-text" {
        5
    } else {
        6
    };
    if failure
        .as_ref()
        .is_none_or(|(previous, _)| rank < *previous)
    {
        *failure = Some((rank, error));
    }
}

/// Read only an explicitly requested clipboard image, without inserting it or
/// falling back to chemical/text interpretation.
pub async fn picture() -> Result<Option<crate::pictures::Picture>, String> {
    read_candidates(true, read_packet, |item| async move {
        let data = item.bytes()?;
        crate::task::spawn_blocking(move || crate::pictures::Picture::import(&data))
            .await
            .map_err(|e| e.to_string())?
    })
    .await
}

#[cfg(test)]
async fn paste_packet(engine: LocalEngine, packet: Packet) -> Result<Document, String> {
    Ok(paste_packet_with_warnings(engine, packet).await?.document)
}

#[cfg(test)]
async fn paste_packet_with_warnings(
    engine: LocalEngine,
    packet: Packet,
) -> Result<PasteOutcome, String> {
    read_candidates(
        false,
        |formats, _, _, _| {
            let item = formats.iter().find_map(|kind| {
                packet
                    .representations
                    .iter()
                    .find(|item| &item.kind == kind)
                    .cloned()
            });
            std::future::ready(Ok(Packet {
                representations: item.into_iter().collect(),
                generation: Some("test".into()),
            }))
        },
        move |item| paste_representation(engine.clone(), item),
    )
    .await?
    .ok_or_else(|| "No supported drawing on the clipboard".into())
}

async fn paste_representation(
    engine: LocalEngine,
    item: Representation,
) -> Result<PasteOutcome, String> {
    let outcome = import_representation(engine, item).await?;
    if outcome.document.all_ids().is_empty() {
        return Err("The clipboard drawing is empty".into());
    }
    Ok(outcome)
}

async fn import_representation(
    engine: LocalEngine,
    item: Representation,
) -> Result<PasteOutcome, String> {
    let data = item.bytes()?;
    if item.kind == NATIVE || item.kind == "dev.moruno.drawing" {
        return crate::task::spawn_blocking(move || Document::from_json(&data))
            .await
            .map_err(|e| e.to_string())?
            .map(PasteOutcome::native);
    }
    if matches!(
        item.kind.as_str(),
        "public.png" | "public.tiff" | "public.jpeg" | "org.webmproject.webp" | "public.webp"
    ) {
        return crate::task::spawn_blocking(move || crate::pictures::clipboard_document(&data))
            .await
            .map_err(|e| e.to_string())?
            .map(PasteOutcome::from);
    }
    let request = if item.kind.contains("cdxml") {
        Request::import(
            "cdxml",
            std::str::from_utf8(&data).map_err(|_| "Invalid XML text encoding")?,
        )
    } else if item.kind.contains("cdx") || item.kind == "application/x-chemdraw" {
        Request::import("cdx", &item.data)
    } else if item.kind == "com.mdli.molfile" {
        Request::import("mol", &mol_text(&data)?)
    } else if matches!(
        item.kind.as_str(),
        "public.utf8-plain-text" | "org.opensmiles.smiles"
    ) {
        let text = std::str::from_utf8(&data).map_err(|_| "Invalid clipboard text encoding")?;
        if let Some(json) = editing::clipboard_json(text) {
            return Document::from_json(json.as_bytes()).map(PasteOutcome::native);
        }
        text_request(text)
    } else {
        return Err(
            "Paste supports PNG, JPEG, TIFF and WebP pictures. Export this PDF or SVG to PNG first"
                .into(),
        );
    };
    let response = engine.request(request).await?;
    let doc = response
        .document
        .ok_or("No drawing returned from the clipboard")?;
    doc.validate()?;
    Ok(PasteOutcome {
        document: doc,
        warnings: response.warnings,
        native: false,
    })
}

#[cfg(test)]
mod tests;
