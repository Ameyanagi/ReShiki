//! Explicit native Copy/Paste with editable and image representations.
use crate::{
    document::Document,
    editing,
    engine::{LocalEngine, Request},
    export,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
#[cfg(not(windows))]
use std::{path::PathBuf, process::Stdio, time::Duration};
#[cfg(not(windows))]
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
};

const NATIVE: &str = "dev.reshiki.drawing";
const LIMIT: usize = 64 * 1024 * 1024;
const JSON_LIMIT: usize = LIMIT * 2;
const CDX_TYPES: [&str; 3] = [
    "com.revvity.chemdraw.cdx-clipboard",
    "com.perkinelmer.chemdraw.cdx-clipboard",
    "com.cambridgesoft.cdx",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Representation {
    #[serde(rename = "type")]
    kind: String,
    data: String,
}
impl Representation {
    fn new(kind: &str, data: &[u8]) -> Self {
        Self {
            kind: kind.into(),
            data: STANDARD.encode(data),
        }
    }
    fn bytes(&self) -> Result<Vec<u8>, String> {
        if self.data.len() > LIMIT.div_ceil(3) * 4 {
            return Err("Clipboard data exceeds 64 MB".into());
        }
        STANDARD
            .decode(&self.data)
            .map_err(|_| "Invalid clipboard encoding".into())
    }
}
#[derive(Serialize)]
struct CommandRequest<'a> {
    operation: &'a str,
    representations: &'a [Representation],
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
}
impl From<Document> for PasteOutcome {
    fn from(document: Document) -> Self {
        Self {
            document,
            warnings: Vec::new(),
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
    let points_per_pixel = 72.0 / f64::from(crate::style::DEFAULT.png_dpi);
    let width = f64::from(reader.info().width) * points_per_pixel;
    let height = f64::from(reader.info().height) * points_per_pixel;
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
    cfg!(any(target_os = "macos", windows))
}

#[cfg(not(windows))]
fn helper() -> Result<PathBuf, String> {
    let bundled = std::env::current_exe().ok().and_then(|exe| {
        let path = exe.parent()?.join("reshiki-clipboard");
        path.is_file().then_some(path)
    });
    bundled
        .or_else(|| {
            option_env!("RESHIKI_CLIPBOARD_HELPER")
                .map(PathBuf::from)
                .filter(|p| p.is_file())
        })
        .ok_or_else(|| "Native clipboard helper is unavailable".into())
}

#[cfg(windows)]
async fn invoke(operation: &str, representations: &[Representation]) -> Result<Packet, String> {
    let input = serde_json::to_vec(&CommandRequest {
        operation,
        representations,
    })
    .map_err(|e| e.to_string())?;
    if input.len() > JSON_LIMIT {
        return Err("Clipboard request is too large".into());
    }
    let output = tokio::task::spawn_blocking(move || reshiki_windows::clipboard(&input))
        .await
        .map_err(|e| e.to_string())??;
    serde_json::from_slice(&output).map_err(|e| e.to_string())
}

#[cfg(not(windows))]
async fn invoke(operation: &str, representations: &[Representation]) -> Result<Packet, String> {
    let input = serde_json::to_vec(&CommandRequest {
        operation,
        representations,
    })
    .map_err(|e| e.to_string())?;
    if input.len() > JSON_LIMIT {
        return Err("Clipboard request is too large".into());
    }
    let mut command = Command::new(helper()?);
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Could not start clipboard helper: {e}"))?;
    let mut stdin = child.stdin.take().ok_or("Clipboard input is unavailable")?;
    let stdout = child
        .stdout
        .take()
        .ok_or("Clipboard output is unavailable")?;
    let stderr = child
        .stderr
        .take()
        .ok_or("Clipboard error output is unavailable")?;
    let operation = async {
        let write = async {
            stdin.write_all(&input).await.map_err(|e| e.to_string())?;
            stdin.shutdown().await.map_err(|e| e.to_string())?;
            drop(stdin);
            Ok::<_, String>(())
        };
        let read = async {
            let mut output = Vec::new();
            stdout
                .take(JSON_LIMIT as u64 + 1)
                .read_to_end(&mut output)
                .await
                .map_err(|e| e.to_string())?;
            if output.len() > JSON_LIMIT {
                return Err("Clipboard response is too large".into());
            }
            Ok(output)
        };
        let errors = async {
            let mut output = Vec::new();
            stderr
                .take(65536)
                .read_to_end(&mut output)
                .await
                .map_err(|e| e.to_string())?;
            Ok::<_, String>(output)
        };
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
                for kind in CDX_TYPES {
                    representations.push(Representation {
                        kind: kind.into(),
                        data: data.clone(),
                    });
                }
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
                // Editors that prefer CDX may reject the generic PDF/image
                // flavors. Supply a sized picture when editable exchange is
                // unavailable, without claiming it contains editable atoms.
                if (image_only || !outcome.external_editable) && format == "png" {
                    match image.bytes().and_then(|bytes| embedded_png(&bytes)) {
                        Ok(bytes) => {
                            for kind in CDX_TYPES {
                                representations.push(Representation::new(kind, &bytes));
                            }
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
        // Keep Copy Image pasteable inside ReShiki as one picture, with the
        // same physical dimensions as the exported 1200 dpi raster.
        let native = png.bytes().and_then(|bytes| {
            let picture = crate::pictures::Picture::import(&bytes)?;
            let width = crate::style::DEFAULT
                .world(picture.width() as f32 * 72. / crate::style::DEFAULT.png_dpi as f32);
            let height = crate::style::DEFAULT
                .world(picture.height() as f32 * 72. / crate::style::DEFAULT.png_dpi as f32);
            let mut doc = picture.document();
            if let Some(g) = doc.graphics.first_mut() {
                crate::pictures::resize(g, width, height)?;
            }
            let bytes = serde_json::to_vec(&doc).map_err(|e| e.to_string())?;
            Ok(Representation::new(NATIVE, &bytes))
        });
        images.push(("native picture", native));
    }
    images
}

fn native_document(data: &[u8]) -> Result<Document, String> {
    let doc: Document =
        serde_json::from_slice(data).map_err(|e| format!("Invalid drawing: {e}"))?;
    doc.validate()?;
    Ok(doc)
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
    let format = if text.trim_start().starts_with("InChI=") {
        "inchi"
    } else if text.trim_start().starts_with("$RXN") {
        "rxn"
    } else if text.contains("M  END") {
        "mol"
    } else if text.contains("<CDXML") {
        "cdxml"
    } else if text.replace("->", "").matches('>').count() == 2 {
        "rsmi"
    } else {
        "smiles"
    };
    Request::import(format, text)
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
        return tokio::task::spawn_blocking(move || native_document(&data))
            .await
            .map_err(|e| e.to_string())?
            .map(PasteOutcome::from);
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
            return native_document(json.as_bytes()).map(PasteOutcome::from);
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
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
            assert_eq!(back.bonds[0].color, theme.color([0; 3]));
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
        let native = native_document(&native.bytes().map_err(anyhow::Error::msg)?)
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
                native_document(&native.bytes().map_err(anyhow::Error::msg)?)
                    .map_err(anyhow::Error::msg)?
                    == doc
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
            native_document(&native.bytes().map_err(anyhow::Error::msg)?)
                .map_err(anyhow::Error::msg)?
                == source
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
            let restored = paste_packet(
                LocalEngine::default(),
                Packet {
                    representations: vec![Representation::new(kind, contents.as_bytes())],
                },
            )
            .await
            .map_err(anyhow::Error::msg)?;
            assert_eq!(restored, expected);
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
        let expected_width = f64::from(reader.info().width) * 72.0 / 1200.0;
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
}
