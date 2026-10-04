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
#[cfg(not(any(windows, target_os = "linux")))]
use std::{path::PathBuf, process::Stdio, time::Duration};
#[cfg(not(any(windows, target_os = "linux")))]
use tokio::process::Command;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux::invoke;

const NATIVE: &str = "dev.reshiki.drawing";
const LIMIT: usize = 64 * 1024 * 1024;
const JSON_LIMIT: usize = LIMIT * 2;
mod copy_as;
pub use copy_as::{CopyFormat, PreparedCopy, prepare_as, selection_or_drawing, write_prepared};
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
}
impl Representation {
    fn new(kind: &str, data: &[u8]) -> Self {
        Self {
            kind: kind.into(),
            data: STANDARD.encode(data).into(),
        }
    }
    fn aliases(kinds: &[&str], data: Arc<String>) -> impl Iterator<Item = Self> {
        kinds.iter().map(move |kind| Self {
            kind: (*kind).into(),
            data: Arc::clone(&data),
        })
    }
    fn bytes(&self) -> Result<Vec<u8>, String> {
        if self.data.len() > LIMIT.div_ceil(3) * 4 {
            return Err("Clipboard data exceeds 64 MB".into());
        }
        STANDARD
            .decode(self.data.as_bytes())
            .map_err(|_| "Invalid clipboard encoding".into())
    }
}
#[derive(Serialize)]
struct CommandRequest<'a> {
    operation: &'a str,
    representations: &'a [Representation],
}

fn encode_request(operation: &str, representations: &[Representation]) -> Result<Vec<u8>, String> {
    let input = serde_json::to_vec(&CommandRequest {
        operation,
        representations,
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
        cfg!(any(target_os = "macos", windows))
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
fn helper() -> Result<PathBuf, String> {
    if !cfg!(target_os = "macos") {
        return Err("Native clipboard is unavailable on this platform".into());
    }
    std::env::current_exe().map_err(|e| format!("Could not locate the application: {e}"))
}

#[cfg(windows)]
async fn invoke(operation: &str, representations: &[Representation]) -> Result<Packet, String> {
    let input = encode_request(operation, representations)?;
    let output = tokio::task::spawn_blocking(move || reshiki_windows::clipboard(&input))
        .await
        .map_err(|e| e.to_string())??;
    serde_json::from_slice(&output).map_err(|e| e.to_string())
}

#[cfg(not(any(windows, target_os = "linux")))]
async fn invoke(operation: &str, representations: &[Representation]) -> Result<Packet, String> {
    let input = encode_request(operation, representations)?;
    let mut command = Command::new(helper()?);
    command.arg("--clipboard-worker");
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
                message.chars().take(500).collect()
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
    let doc = Document {
        version: crate::document::VERSION,
        ..doc
    };
    let mut representations = Vec::new();
    if !image_only {
        representations.push(Representation::new(
            NATIVE,
            &serde_json::to_vec(&doc).map_err(|e| e.to_string())?,
        ));
        // External editors receive explicit visible ink colors, without the
        // source page background. Native data above retains the original theme.
        let exchange_doc =
            crate::canvas_theme::for_paste(doc.clone(), crate::canvas_theme::CanvasTheme::Light);
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
                let compatible = tokio::task::spawn_blocking(move || {
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
    let images = tokio::task::spawn_blocking(move || copy_images(&doc, image_only))
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
                if (image_only || !outcome.external_editable) && format == "png" {
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
) -> Vec<(&'static str, Result<Representation, String>)> {
    let mut images: Vec<_> = [
        ("com.adobe.pdf", "pdf"),
        ("public.png", "png"),
        ("public.svg-image", "svg"),
    ]
    .into_iter()
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
    if !image_only {
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

/// Import format of typed or pasted structure text, recognized by cheap markers.
pub fn text_format(text: &str) -> &'static str {
    if text.trim_start().starts_with("InChI=") {
        "inchi"
    } else if text.trim_start().starts_with("$RXN") {
        "rxn"
    } else if text.contains("M  END") {
        "mol"
    } else if text.contains("<CDXML") {
        "cdxml"
    } else if text.contains("V2000") || text.contains("V3000") {
        "mol"
    } else if text.replace("->", "").matches('>').count() == 2 {
        "rsmi"
    } else {
        "smiles"
    }
}

pub async fn paste(engine: LocalEngine, image_only: bool) -> Result<Document, String> {
    Ok(paste_with_warnings(engine, image_only).await?.document)
}

pub async fn paste_with_warnings(
    engine: LocalEngine,
    image_only: bool,
) -> Result<PasteOutcome, String> {
    let packet = invoke(if image_only { "read_picture" } else { "read" }, &[]).await?;
    paste_packet_with_warnings(engine, packet).await
}

/// Read only an explicitly requested clipboard image, without inserting it or
/// falling back to chemical/text interpretation.
pub async fn picture() -> Result<Option<crate::pictures::Picture>, String> {
    let packet = invoke("read_picture", &[]).await?;
    let Some(item) = packet.representations.first() else {
        return Ok(None);
    };
    let data = item.bytes()?;
    tokio::task::spawn_blocking(move || crate::pictures::Picture::import(&data).map(Some))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
async fn paste_packet(engine: LocalEngine, packet: Packet) -> Result<Document, String> {
    Ok(paste_packet_with_warnings(engine, packet).await?.document)
}

async fn paste_packet_with_warnings(
    engine: LocalEngine,
    packet: Packet,
) -> Result<PasteOutcome, String> {
    let item = packet
        .representations
        .first()
        .ok_or("No supported drawing on the clipboard")?;
    let data = item.bytes()?;
    if item.kind == NATIVE || item.kind == "dev.moruno.drawing" {
        return tokio::task::spawn_blocking(move || Document::from_json(&data))
            .await
            .map_err(|e| e.to_string())?
            .map(PasteOutcome::native);
    }
    if matches!(
        item.kind.as_str(),
        "public.png" | "public.tiff" | "public.jpeg" | "org.webmproject.webp" | "public.webp"
    ) {
        return tokio::task::spawn_blocking(move || crate::pictures::clipboard_document(&data))
            .await
            .map_err(|e| e.to_string())?
            .map(PasteOutcome::from);
    }
    let request = if item.kind.contains("cdxml") {
        Request::import(
            "cdxml",
            std::str::from_utf8(&data).map_err(|_| "Invalid XML text encoding")?,
        )
    } else if item.kind.contains("cdx") {
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
mod tests {
    use super::*;

    #[derive(Serialize, Deserialize)]
    struct LegacyRepresentation {
        #[serde(rename = "type")]
        kind: String,
        data: String,
    }
    #[derive(Serialize)]
    struct LegacyRequest<'a> {
        operation: &'static str,
        representations: &'a [LegacyRepresentation],
    }

    #[test]
    fn cdx_aliases_share_payload_without_changing_serialized_protocol() {
        let bytes = b"CDX\0exact binary\xffpayload";
        let encoded: Arc<String> = STANDARD.encode(bytes).into();
        let representations: Vec<_> = Representation::aliases(&CDX_TYPES, encoded).collect();
        assert_eq!(
            representations
                .iter()
                .map(|item| item.kind.as_str())
                .collect::<Vec<_>>(),
            CDX_TYPES
        );
        for item in &representations {
            assert!(Arc::ptr_eq(&representations[0].data, &item.data));
            assert_eq!(item.bytes().unwrap(), bytes);
        }
        let legacy: Vec<_> = CDX_TYPES
            .iter()
            .map(|kind| LegacyRepresentation {
                kind: (*kind).into(),
                data: STANDARD.encode(bytes),
            })
            .collect();
        let packet = encode_request("write", &representations).unwrap();
        assert_eq!(
            packet,
            serde_json::to_vec(&LegacyRequest {
                operation: "write",
                representations: &legacy,
            })
            .unwrap()
        );
        let decoded: Packet = serde_json::from_slice(&packet).unwrap();
        for (before, after) in representations.iter().zip(decoded.representations) {
            assert_eq!(before.kind, after.kind);
            assert_eq!(before.bytes().unwrap(), after.bytes().unwrap());
        }
    }

    #[test]
    #[ignore = "process-wide allocation counters: run alone with --test-threads=1 --nocapture"]
    fn cdx_alias_allocation_metrics() {
        use crate::allocation_metrics;
        use std::time::Instant;
        fn measure<T>(create: impl FnOnce() -> T) -> (T, [usize; 4], std::time::Duration) {
            let baseline = allocation_metrics::reset();
            let started = Instant::now();
            let output = create();
            let elapsed = started.elapsed();
            let measured = allocation_metrics::snapshot();
            (
                output,
                [
                    measured.live_bytes - baseline,
                    measured.peak_bytes - baseline,
                    measured.allocated_bytes,
                    measured.allocation_count,
                ],
                elapsed,
            )
        }
        let bytes = vec![0xab; 1024 * 1024];
        let encoded = STANDARD.encode(&bytes);
        let (old_editable, old_editable_cost, old_editable_time) = measure(|| {
            let data = encoded.clone();
            CDX_TYPES
                .iter()
                .map(|kind| LegacyRepresentation {
                    kind: (*kind).into(),
                    data: data.clone(),
                })
                .collect::<Vec<_>>()
        });
        let (shared_editable, shared_editable_cost, shared_editable_time) = measure(|| {
            Representation::aliases(&CDX_TYPES, encoded.clone().into()).collect::<Vec<_>>()
        });
        let (old_image, old_image_cost, old_image_time) = measure(|| {
            CDX_TYPES
                .iter()
                .map(|kind| LegacyRepresentation {
                    kind: (*kind).into(),
                    data: STANDARD.encode(&bytes),
                })
                .collect::<Vec<_>>()
        });
        let (shared_image, shared_image_cost, shared_image_time) = measure(|| {
            Representation::aliases(&CDX_TYPES, STANDARD.encode(&bytes).into()).collect::<Vec<_>>()
        });
        let single_wire = serde_json::to_vec(&LegacyRepresentation {
            kind: CDX_TYPES[0].into(),
            data: encoded.clone(),
        })
        .unwrap();
        let (old_read, old_read_cost, old_read_time) =
            measure(|| serde_json::from_slice::<LegacyRepresentation>(&single_wire).unwrap());
        let (shared_read, shared_read_cost, shared_read_time) =
            measure(|| serde_json::from_slice::<Representation>(&single_wire).unwrap());
        assert_eq!(old_read.data, shared_read.data.as_str());
        assert!(shared_read_cost[2].saturating_sub(old_read_cost[2]) < 1024);
        let packet = encode_request("write", &shared_editable).unwrap();
        assert_eq!(packet, encode_request("write", &shared_image).unwrap());
        for legacy in [&old_editable, &old_image] {
            assert_eq!(
                packet,
                serde_json::to_vec(&LegacyRequest {
                    operation: "write",
                    representations: legacy,
                })
                .unwrap()
            );
        }
        for (old, shared) in [
            (old_editable_cost, shared_editable_cost),
            (old_image_cost, shared_image_cost),
        ] {
            assert!(shared[0] < old[0], "retained bytes: {shared:?} vs {old:?}");
            assert!(shared[1] < old[1], "peak bytes: {shared:?} vs {old:?}");
            assert!(shared[2] < old[2], "allocated bytes: {shared:?} vs {old:?}");
        }
        println!(
            "raw={} encoded={} alias_payloads={} wire={}",
            bytes.len(),
            encoded.len(),
            3 * encoded.len(),
            packet.len()
        );
        for (name, cost, elapsed, encodes) in [
            ("editable-old", old_editable_cost, old_editable_time, 0),
            (
                "editable-shared",
                shared_editable_cost,
                shared_editable_time,
                0,
            ),
            ("image-old", old_image_cost, old_image_time, 3),
            ("image-shared", shared_image_cost, shared_image_time, 1),
            ("single-read-old", old_read_cost, old_read_time, 0),
            ("single-read-shared", shared_read_cost, shared_read_time, 0),
        ] {
            println!(
                "{name}: retained={} peak={} allocated={} allocations={} encodes={encodes} generation={elapsed:?}",
                cost[0], cost[1], cost[2], cost[3]
            );
        }
    }

    #[test]
    fn typed_text_formats_follow_cheap_markers() {
        for (text, format) in [
            ("  InChI=1S/C2H6O/c1-2-3/h3H,2H2,1H3", "inchi"),
            ("$RXN\n\n  ReShiki\n\n  1  1\n$MOL\nM  END", "rxn"),
            (
                "ethanol\n\n\n  3  2  0  0  0  0  0  0  0  0999 V2000\nM  END",
                "mol",
            ),
            ("\n\n\n  0  0  0     0  0            999 V3000\n", "mol"),
            ("<?xml version=\"1.0\"?><CDXML><page/></CDXML>", "cdxml"),
            ("CCO>>CC=O", "rsmi"),
            ("CCO>O=O>CC=O", "rsmi"),
            ("C->C", "smiles"),
            ("c1ccccc1", "smiles"),
        ] {
            assert_eq!(text_format(text), format, "{text}");
            assert_eq!(text_request(text).format.as_deref(), Some(format));
        }
    }

    #[tokio::test]
    async fn both_canvas_modes_copy_visible_ink_without_background_objects() {
        use crate::{canvas_theme::CanvasTheme, document::Point};
        for theme in CanvasTheme::ALL {
            let mut doc = Document {
                canvas_theme: theme,
                ..Default::default()
            };
            let c = doc.add_atom("C", Point::default());
            let o = doc.add_atom("O", Point::new(42., 0.));
            doc.add_bond(c, o, 2, "plain");
            let (outcome, representations) = prepare_copy(Default::default(), doc.clone(), false)
                .await
                .unwrap();
            assert!(outcome.external_editable, "{:?}", outcome.notices);
            let native = representations.iter().find(|r| r.kind == NATIVE).unwrap();
            let original: Document = serde_json::from_slice(&native.bytes().unwrap()).unwrap();
            assert_eq!(original.canvas_theme, theme);
            assert_eq!(original.drawing_style, doc.drawing_style);
            assert!(original.graphics.is_empty());
            let binary = representations
                .iter()
                .find(|r| r.kind == CDX_TYPES[0])
                .unwrap()
                .clone();
            let back = paste_packet(
                Default::default(),
                Packet {
                    representations: vec![binary],
                },
            )
            .await
            .unwrap();
            assert_eq!(back.atoms.len(), 2);
            assert_eq!(back.bonds.len(), 1);
            assert!(
                back.graphics.is_empty(),
                "Editable copies must not add a canvas rectangle"
            );
            assert_eq!(
                back.bonds[0].color,
                crate::palette::Color::imported(theme.color([0; 3]))
            );
            for (_, image) in copy_images(&doc, true) {
                let image = image.unwrap();
                if image.kind == "public.png" {
                    let raster = image::load_from_memory(&image.bytes().unwrap())
                        .unwrap()
                        .into_rgba8();
                    assert_eq!(raster.get_pixel(0, 0)[3], 0);
                    let ink = theme.color([0; 3]);
                    assert!(
                        raster
                            .pixels()
                            .any(|p| p.0 == [ink[0], ink[1], ink[2], 255])
                    );
                } else if image.kind == "public.svg-image" {
                    let bytes = image.bytes().unwrap();
                    let tree =
                        roxmltree::Document::parse(std::str::from_utf8(&bytes).unwrap()).unwrap();
                    assert!(!tree.descendants().any(|n| n.has_tag_name("rect")));
                }
            }
        }
    }

    #[tokio::test]
    async fn internal_condensed_labels_keep_editable_text_and_formula_formatting()
    -> anyhow::Result<()> {
        use anyhow::{Context, ensure};
        let source: Document =
            serde_json::from_str(include_str!("../docs/changes/fixtures/internal-labels.rsk"))?;
        let (outcome, representations) = prepare_copy(Default::default(), source, false)
            .await
            .map_err(anyhow::Error::msg)?;
        ensure!(outcome.external_editable && !outcome.image_only);
        let binary = representations
            .iter()
            .find(|r| r.kind == CDX_TYPES[0])
            .context("CDX")?;
        let xml = crate::exchange::from_cdx(&binary.bytes().map_err(anyhow::Error::msg)?)
            .map_err(anyhow::Error::msg)?;
        let tree = roxmltree::Document::parse(&xml)?;
        ensure!(!tree.descendants().any(|n| n.has_tag_name("embeddedobject")));
        for label in ["CCl2", "CF2", "NMe"] {
            let runs: Vec<_> = tree
                .descendants()
                .filter(|n| n.has_tag_name("s") && n.text() == Some(label))
                .collect();
            ensure!(runs.len() == 4, "Missing {label} orientations");
            ensure!(
                runs.iter().all(|n| n
                    .attribute("face")
                    .and_then(|s| s.parse::<u8>().ok())
                    .is_some_and(|f| f & 96 == 96)),
                "{label} lost formula typography"
            );
        }
        for (format, data) in [("cdx", binary.data.as_str()), ("cdxml", xml.as_str())] {
            let back = LocalEngine::default()
                .request(Request::import(format, data))
                .await
                .map_err(anyhow::Error::msg)?
                .document
                .context("Imported drawing")?;
            ensure!(back.atoms.len() == 60 && back.bonds.len() == 40 && back.graphics.is_empty());
            for label in ["CCl2", "CF2", "NMe"] {
                ensure!(
                    back.atoms
                        .iter()
                        .filter(|a| a.display.variable.as_deref() == Some(label))
                        .count()
                        == 4,
                    "{format} lost {label}: {:?}",
                    back.atoms
                        .iter()
                        .filter(|a| a.display.variable.is_some() || a.element == "*")
                        .map(|a| (&a.element, &a.display.variable))
                        .collect::<Vec<_>>()
                );
            }
        }
        Ok(())
    }

    #[tokio::test]
    async fn projected_arene_with_bold_edge_has_editable_clipboard_and_native_depth()
    -> anyhow::Result<()> {
        use anyhow::{Context, ensure};
        let source: Document =
            serde_json::from_str(include_str!("../docs/changes/fixtures/arene-bold-join.rsk"))?;
        let (outcome, representations) = prepare_copy(Default::default(), source.clone(), false)
            .await
            .map_err(anyhow::Error::msg)?;
        ensure!(
            outcome.external_editable && outcome.notices.is_empty(),
            "{:?}",
            outcome.notices
        );
        let native = representations
            .iter()
            .find(|r| r.kind == NATIVE)
            .context("Native drawing")?;
        let native = Document::from_json(&native.bytes().map_err(anyhow::Error::msg)?)
            .map_err(anyhow::Error::msg)?;
        for (a, b) in source.atoms.iter().zip(&native.atoms) {
            assert_eq!((a.position, a.depth), (b.position, b.depth));
        }
        assert_eq!(source.bonds, native.bonds);
        let binary = representations
            .iter()
            .find(|r| r.kind == CDX_TYPES[0])
            .context("Binary drawing")?;
        let back = paste_packet(
            Default::default(),
            Packet {
                representations: vec![binary.clone()],
            },
        )
        .await
        .map_err(anyhow::Error::msg)?;
        ensure!(back.atoms.len() == 7 && back.bonds.len() == 7 && back.graphics.is_empty());
        ensure!(back.atoms.iter().all(|a| a.stereo.is_none()));
        Ok(())
    }

    #[tokio::test]
    async fn projected_double_bonds_keep_bold_rails_in_editable_clipboard() -> anyhow::Result<()> {
        use anyhow::{Context, ensure};
        let mut source: Document = serde_json::from_str(include_str!(
            "../tests/fixtures/tilted-fused-double-bonds.rsk"
        ))?;
        let ids = source.all_ids();
        crate::projection::depth_bonds(&mut source, &ids);
        let (outcome, representations) = prepare_copy(Default::default(), source.clone(), false)
            .await
            .map_err(anyhow::Error::msg)?;
        ensure!(outcome.external_editable && !outcome.image_only);
        let native = representations
            .iter()
            .find(|r| r.kind == NATIVE)
            .context("Native drawing")?;
        assert_eq!(
            Document::from_json(&native.bytes().map_err(anyhow::Error::msg)?)
                .map_err(anyhow::Error::msg)?,
            source.current()
        );
        let binary = representations
            .iter()
            .find(|r| r.kind == CDX_TYPES[0])
            .context("CDX drawing")?;
        let back = paste_packet(
            Default::default(),
            Packet {
                representations: vec![binary.clone()],
            },
        )
        .await
        .map_err(anyhow::Error::msg)?;
        assert_eq!(
            (back.atoms.len(), back.bonds.len()),
            (source.atoms.len(), source.bonds.len())
        );
        for (before, after) in source
            .bonds
            .iter()
            .zip(&back.bonds)
            .filter(|(b, _)| b.order == 2)
        {
            assert_eq!(
                (
                    before.order,
                    &before.display,
                    before
                        .secondary_display
                        .as_deref()
                        .unwrap_or(&before.display)
                ),
                (
                    after.order,
                    &after.display,
                    after.secondary_display.as_deref().unwrap_or(&after.display)
                )
            );
        }
        ensure!(back.atoms.iter().all(|a| a.stereo.is_none()));
        Ok(())
    }

    #[tokio::test]
    async fn copying_unvalidated_rings_keeps_editable_exchange_and_paste_warning()
    -> anyhow::Result<()> {
        use anyhow::{Context, ensure};
        let source = Representation::new(
            CDX_TYPES[0],
            include_bytes!("../tests/fixtures/aromatic-five-attachment.cdx"),
        );
        let pasted = paste_packet_with_warnings(
            Default::default(),
            Packet {
                representations: vec![source],
            },
        )
        .await
        .map_err(anyhow::Error::msg)?;
        ensure!(!pasted.warnings.is_empty());
        let (outcome, representations) =
            prepare_copy(Default::default(), pasted.document.clone(), false)
                .await
                .map_err(anyhow::Error::msg)?;
        ensure!(outcome.external_editable && !outcome.image_only);
        let binary = representations
            .iter()
            .find(|r| r.kind == CDX_TYPES[0])
            .context("Missing editable drawing")?;
        let back = paste_packet_with_warnings(
            Default::default(),
            Packet {
                representations: vec![binary.clone()],
            },
        )
        .await
        .map_err(anyhow::Error::msg)?;
        ensure!(back.document.atoms.len() == pasted.document.atoms.len());
        ensure!(!back.warnings.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn simplified_appearance_keeps_editable_atoms_and_the_native_original()
    -> anyhow::Result<()> {
        use anyhow::{Context, ensure};
        let mut doc = Document::default();
        let id = doc.add_atom("*", crate::document::Point::default());
        doc.atom_mut(id).context("Missing label")?.display.variable = Some("M".into());
        let mut cases = vec![doc];
        for (key, element) in [("j", "Fe"), ("J", "Ru")] {
            let mut source = Document::default();
            let id = source.add_atom(element, crate::document::Point::default());
            cases.push(
                crate::hotkeys::atom_edit(&source, id, key, 42.)
                    .context("Ligand shortcut")?
                    .map_err(anyhow::Error::msg)?
                    .0,
            );
        }
        for doc in cases {
            let (outcome, representations) = prepare_copy(Default::default(), doc.clone(), false)
                .await
                .map_err(anyhow::Error::msg)?;
            ensure!(outcome.external_editable, "{:?}", outcome.notices);
            let native = representations
                .iter()
                .find(|r| r.kind == NATIVE)
                .context("Missing native drawing")?;
            ensure!(
                Document::from_json(&native.bytes().map_err(anyhow::Error::msg)?)
                    .map_err(anyhow::Error::msg)?
                    == doc.current()
            );
            let binary = representations
                .iter()
                .find(|r| r.kind == CDX_TYPES[0])
                .context("Missing editable copy")?;
            let back = paste_packet(
                Default::default(),
                Packet {
                    representations: vec![binary.clone()],
                },
            )
            .await
            .map_err(anyhow::Error::msg)?;
            ensure!(back.atoms.len() == doc.atoms.len() && back.bonds.len() == doc.bonds.len());
            ensure!(back.graphics.is_empty());
        }
        Ok(())
    }

    #[tokio::test]
    async fn complete_shortcut_gallery_has_editable_cdx_without_altering_the_native_copy()
    -> anyhow::Result<()> {
        use anyhow::{Context, ensure};
        let source: Document =
            serde_json::from_str(include_str!("../assets/examples/shortcut-examples.rsk"))?;
        let (outcome, representations) = prepare_copy(Default::default(), source.clone(), false)
            .await
            .map_err(anyhow::Error::msg)?;
        ensure!(outcome.external_editable, "{:?}", outcome.notices);
        ensure!(
            outcome.notices.iter().any(|n| n.contains("variable label")),
            "Missing variable-label notice"
        );
        let native = representations
            .iter()
            .find(|r| r.kind == NATIVE)
            .context("Native drawing")?;
        ensure!(
            Document::from_json(&native.bytes().map_err(anyhow::Error::msg)?)
                .map_err(anyhow::Error::msg)?
                == source.current()
        );
        let cdx = representations
            .iter()
            .find(|r| r.kind == CDX_TYPES[0])
            .context("Editable CDX")?;
        let xml = crate::exchange::from_cdx(&cdx.bytes().map_err(anyhow::Error::msg)?)
            .map_err(anyhow::Error::msg)?;
        let tree = roxmltree::Document::parse(&xml)?;
        ensure!(
            !tree.descendants().any(|n| n.has_tag_name("embeddedobject")),
            "Gallery must not be flattened into a picture"
        );
        for label in ["R", "X"] {
            ensure!(tree.descendants().any(|n| {
                n.has_tag_name("n")
                    && n.attribute("Element") == Some("0")
                    && n.descendants()
                        .any(|s| s.has_tag_name("s") && s.text() == Some(label))
            }));
        }
        let back = paste_packet(
            Default::default(),
            Packet {
                representations: vec![cdx.clone()],
            },
        )
        .await
        .map_err(anyhow::Error::msg)?;
        ensure!(
            back.atoms.len() == source.atoms.len(),
            "Atom count: {} / {}",
            back.atoms.len(),
            source.atoms.len()
        );
        ensure!(back.bonds.len() == source.bonds.len());
        ensure!(back.annotations.len() == source.annotations.len());
        ensure!(
            back.atoms.iter().map(|a| a.charge).sum::<i32>()
                == source.atoms.iter().map(|a| a.charge).sum::<i32>()
        );
        // This native CDX self-round-trip keeps the complete current gallery,
        // including captions and pi ligands together. Genuine external
        // captures are covered separately in pi_ligand_exchange/chemdraw_captions.
        ensure!(back.atoms.iter().filter(|a| a.attachment.is_some()).count() == 5);
        ensure!(back.bonds.iter().filter(|b| b.order == 4).count() == 33);
        ensure!(
            back.atoms
                .iter()
                .filter(|a| a.charge == -1 && a.display.hide_charge)
                .count()
                == 3
        );
        ensure!(
            back.atoms
                .iter()
                .any(|a| a.element == "Fe" && a.charge == 2)
        );
        let mut expected_text: Vec<_> = source.annotations.iter().map(|a| &a.text).collect();
        let mut returned_text: Vec<_> = back.annotations.iter().map(|a| &a.text).collect();
        expected_text.sort();
        returned_text.sort();
        ensure!(returned_text == expected_text);
        for caption in &back.annotations {
            ensure!(
                source.annotations.iter().any(|original| {
                    original.text == caption.text
                        && original.format.style == caption.format.style
                        // The binary format rounds to the nearest 0.05pt.
                        && (original.format.style.size_pt * original.format.line_spacing
                            - caption.format.style.size_pt * caption.format.line_spacing)
                            .abs()
                            <= 0.0251
                }),
                "Caption style or spacing changed: {}",
                caption.text
            );
        }
        Ok(())
    }

    #[tokio::test]
    async fn previous_native_and_text_clipboards_remain_editable() -> anyhow::Result<()> {
        let json = include_str!("../tests/fixtures/legacy-drawing.moruno");
        let expected: Document = serde_json::from_str(json)?;
        expected.validate().map_err(anyhow::Error::msg)?;
        for (kind, contents) in [
            ("dev.moruno.drawing", json.to_owned()),
            (NATIVE, json.to_owned()),
            (
                "public.utf8-plain-text",
                format!("MORUNO_DRAWING_V1\n{json}"),
            ),
            (
                "public.utf8-plain-text",
                format!("{}{json}", editing::CLIPBOARD_PREFIX),
            ),
        ] {
            let restored = paste_packet_with_warnings(
                LocalEngine::default(),
                Packet {
                    representations: vec![Representation::new(kind, contents.as_bytes())],
                },
            )
            .await
            .map_err(anyhow::Error::msg)?;
            assert!(restored.native, "{kind}");
            assert_eq!(restored.document, expected);
        }
        Ok(())
    }

    #[tokio::test]
    async fn old_dark_reshiki_clipboards_convert_colors_once() -> anyhow::Result<()> {
        use crate::{canvas_theme::CanvasTheme, document::Point, palette::Color};
        let mut doc = Document {
            canvas_theme: CanvasTheme::Dark,
            ..Default::default()
        };
        let c = doc.add_atom("C", Point::default());
        let o = doc.add_atom("O", Point::new(42., 0.));
        doc.add_bond(c, o, 1, "plain");
        doc.bonds[0].color = Color::Custom([10, 120, 200]);
        assert!(doc.version < crate::document::VERSION);
        let (_, representations) = prepare_copy(Default::default(), doc, false)
            .await
            .map_err(anyhow::Error::msg)?;
        let copied = representations
            .iter()
            .find(|r| r.kind == NATIVE)
            .ok_or_else(|| anyhow::anyhow!("no native data"))?
            .bytes()
            .map_err(anyhow::Error::msg)?;
        // The previous release's dark drawing showed [10, 120, 200] flipped.
        let old = include_bytes!("../tests/fixtures/palette/legacy-dark.rsk");
        for (data, bond, color) in [
            (copied.as_slice(), 0, [10, 120, 200]),
            (old.as_slice(), 1, [55, 165, 245]),
        ] {
            let pasted = paste_packet_with_warnings(
                LocalEngine::default(),
                Packet {
                    representations: vec![Representation::new(NATIVE, data)],
                },
            )
            .await
            .map_err(anyhow::Error::msg)?;
            assert_eq!(pasted.document.bonds[bond].color, Color::Custom(color));
        }
        Ok(())
    }

    #[tokio::test]
    async fn only_reshiki_clipboards_keep_palette_references() -> anyhow::Result<()> {
        use crate::palette::{Color, Hue, Palette, Row};
        let mut doc = Document::default();
        let c = doc.add_atom("C", crate::document::Point::default());
        let n = doc.add_atom("N", crate::document::Point::new(42., 0.));
        let o = doc.add_atom("O", crate::document::Point::new(84., 0.));
        doc.add_bond(c, n, 1, "plain");
        doc.add_bond(n, o, 1, "plain");
        doc.bonds[0].color = Color::Palette(Hue::Red, Row::Strong);
        let red = Palette::of(&doc).rgb(doc.bonds[0].color);
        let native = serde_json::to_vec(&doc)?;
        let xml = crate::exchange::drawing::write(&doc, Default::default())?;
        for (kind, data, native) in [
            (NATIVE, native.as_slice(), true),
            ("com.perkinelmer.chemdraw.cdxml", xml.as_bytes(), false),
        ] {
            let pasted = paste_packet_with_warnings(
                LocalEngine::default(),
                Packet {
                    representations: vec![Representation::new(kind, data)],
                },
            )
            .await
            .map_err(anyhow::Error::msg)?;
            assert_eq!(pasted.native, native, "{kind}");
            let colors: Vec<_> = pasted.document.bonds.iter().map(|b| b.color).collect();
            if native {
                assert_eq!(colors, [Color::Palette(Hue::Red, Row::Strong), Color::Ink]);
            } else {
                // ChemDraw data keeps exact colors; its black becomes Ink.
                assert!(colors.contains(&Color::Custom(red)), "{colors:?}");
                assert!(colors.contains(&Color::Ink), "{colors:?}");
            }
        }
        Ok(())
    }

    #[tokio::test]
    async fn raster_paste_and_copy_image_preserve_pixels_and_physical_size_without_a_clipboard_write()
     {
        let doc: Document =
            serde_json::from_str(include_str!("../tests/fixtures/ui-drawn-ethanol.reshiki"))
                .unwrap();
        let images = copy_images(&doc, true);
        #[cfg(windows)]
        {
            let svg = images
                .iter()
                .find(|(format, _)| *format == "svg")
                .unwrap()
                .1
                .as_ref()
                .unwrap()
                .bytes()
                .unwrap();
            let text = std::str::from_utf8(&svg).unwrap();
            assert!(!text.contains("<text"), "Office needs outlined labels");
            assert!(text.contains("<path"));
        }
        let native = images
            .iter()
            .find(|(format, _)| *format == "native picture")
            .unwrap()
            .1
            .as_ref()
            .unwrap()
            .clone();
        let png = images
            .iter()
            .find(|(format, _)| *format == "png")
            .unwrap()
            .1
            .as_ref()
            .unwrap()
            .clone();
        let restored = paste_packet(
            LocalEngine::default(),
            Packet {
                representations: vec![native],
            },
        )
        .await
        .unwrap();
        let raster = paste_packet(
            LocalEngine::default(),
            Packet {
                representations: vec![png],
            },
        )
        .await
        .unwrap();
        assert!(restored.atoms.is_empty());
        assert_eq!(restored.graphics.len(), 1);
        assert_eq!(raster.graphics[0].picture, restored.graphics[0].picture);
        assert!((raster.graphics[0].axis_x.x - restored.graphics[0].axis_x.x).abs() < 0.01);
        assert!((raster.graphics[0].axis_y.y - restored.graphics[0].axis_y.y).abs() < 0.01);
        let invalid = Representation::new("public.png", b"not a PNG");
        assert!(
            paste_packet(
                LocalEngine::default(),
                Packet {
                    representations: vec![invalid]
                }
            )
            .await
            .is_err()
        );
        for (kind, format) in [
            ("public.jpeg", image::ImageFormat::Jpeg),
            ("public.tiff", image::ImageFormat::Tiff),
            ("org.webmproject.webp", image::ImageFormat::WebP),
        ] {
            let mut data = std::io::Cursor::new(Vec::new());
            image::DynamicImage::new_rgb8(12, 8)
                .write_to(&mut data, format)
                .unwrap();
            let result = paste_packet(
                LocalEngine::default(),
                Packet {
                    representations: vec![Representation::new(kind, &data.into_inner())],
                },
            )
            .await
            .unwrap();
            let picture = result.graphics[0].picture.as_ref().unwrap();
            assert_eq!((picture.width(), picture.height()), (12, 8));
        }
    }

    #[test]
    fn mol_line_framing_preserves_empty_title_and_refuses_truncation() {
        let text = "\n  example\n\n  0  0\nM  END\n";
        let data: Vec<u8> = text
            .lines()
            .flat_map(|line| std::iter::once(line.len() as u8).chain(line.bytes()))
            .collect();
        assert_eq!(mol_text(&data).unwrap(), text);
        assert_eq!(mol_text(text.as_bytes()).unwrap(), text);
        assert!(mol_text(&[10, b'x']).is_err());
        assert!(mol_text(&[1, 255]).is_err());
    }

    #[test]
    fn raster_clipboard_object_preserves_bytes_and_publication_size() {
        let doc: Document =
            serde_json::from_str(include_str!("../tests/fixtures/ui-drawn-ethanol.reshiki"))
                .unwrap();
        let png = export::drawing(&doc, "png").unwrap();
        let drawing = embedded_png(&png).unwrap();
        // Independent inspection of the tagged stream: image-only object,
        // physical bounding box, original PNG bytes, and balanced terminators.
        assert_eq!(
            &drawing[..22],
            b"VjCD0100\x04\x03\x02\x01\0\0\0\0\0\0\0\0\0\0"
        );
        assert_eq!(
            u16::from_le_bytes(drawing[54..56].try_into().unwrap()),
            0x8009
        );
        assert_eq!(
            u16::from_le_bytes(drawing[60..62].try_into().unwrap()),
            0x0204
        );
        let right = i32::from_le_bytes(drawing[76..80].try_into().unwrap()) as f64 / 65536.0;
        let reader = png::Decoder::new(std::io::Cursor::new(&png))
            .read_info()
            .unwrap();
        let expected_width = f64::from(reader.info().width) * 72.0
            / (f64::from(reader.info().pixel_dims.unwrap().xppu) * 0.0254);
        assert!((right - 30.0 - expected_width).abs() < 0.0001);
        assert_eq!(
            u16::from_le_bytes(drawing[80..82].try_into().unwrap()),
            0x0a70
        );
        let start = if png.len() < 65535 { 84 } else { 88 };
        assert_eq!(&drawing[start..drawing.len() - 8], &png);
        assert_eq!(&drawing[drawing.len() - 8..], &[0; 8]);
        assert!(embedded_png(b"invalid image").is_err());
        assert!(embedded_png(&png[..20]).is_err());
        assert!(embedded_png(&vec![0; 16 * 1024 * 1024 + 1]).is_err());
    }

    #[test]
    fn adaptive_raster_wrappers_keep_publication_size_from_actual_resolution() {
        for dpi in [1200, 600, 300, 150, 96, 72] {
            // The same one-inch figure at each of the preview resolutions.
            let (width, height) = (dpi, dpi / 2);
            let mut bytes = Vec::new();
            let mut encoder = png::Encoder::new(&mut bytes, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_pixel_dims(Some(png::PixelDimensions {
                xppu: (f64::from(dpi) / 0.0254).round() as u32,
                yppu: (f64::from(dpi) / 0.0254).round() as u32,
                unit: png::Unit::Meter,
            }));
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&vec![0; width as usize * height as usize * 4])
                .unwrap();
            let drawing = embedded_png(&bytes).unwrap();
            let coordinate = |offset| {
                f64::from(i32::from_le_bytes(
                    drawing[offset..offset + 4].try_into().unwrap(),
                )) / 65536.
            };
            let (cdx_width, cdx_height) = (
                coordinate(76) - coordinate(68),
                coordinate(72) - coordinate(64),
            );
            let native = crate::pictures::clipboard_document(&bytes).unwrap();
            let graphic = &native.graphics[0];
            let style = &*crate::style::DEFAULT;
            // The PNG pHYs field is integer pixels/meter, hence a small physical
            // quantization at the lowest resolutions. All wrappers must agree.
            assert!((cdx_width - 72.).abs() < 0.02, "{dpi} DPI width");
            assert!((cdx_height - 36.).abs() < 0.02, "{dpi} DPI height");
            assert!(
                (f64::from(graphic.axis_x.x * style.points_per_world()) - cdx_width).abs() < 0.001
            );
            assert!(
                (f64::from(graphic.axis_y.y * style.points_per_world()) - cdx_height).abs() < 0.001
            );
        }
    }
}
