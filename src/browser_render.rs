//! Browser export jobs use the existing renderers in a separate worker memory.
//! The font envelope also serves clipboard jobs, including fonts loaded from
//! browser storage rather than through the JavaScript package wrapper.
use crate::{document::Document, engine::LocalEngine, export, formats};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::cell::Cell;

mod snapshot;
pub use snapshot::Snapshot;

pub const HEAP_BYTES: usize = 1536 * 1024 * 1024;
const MAX_REQUEST_BYTES: usize = 128 * 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 512 * 1024 * 1024;
const MAX_HEADER_BYTES: usize = 1024 * 1024;
#[cfg(target_arch = "wasm32")]
const JOB: &str = "render";

thread_local! {
    static INSTALLED_FONTS: Cell<usize> = const { Cell::new(0) };
    #[cfg(target_arch = "wasm32")]
    static ENABLED: Cell<bool> = const { Cell::new(true) };
}

/// A caller-supplied editor module has no verified matching render module.
#[cfg(target_arch = "wasm32")]
pub fn set_enabled(enabled: bool) {
    ENABLED.set(enabled);
}

#[cfg(target_arch = "wasm32")]
pub fn available() -> bool {
    ENABLED.get() && crate::browser_worker::available()
}

/// Prefix a job's bytes with its ordered font snapshot. Caller buffers are copied.
pub fn request_fonts(request: &[u8]) -> Result<Vec<u8>, String> {
    font_envelope(request).map_err(EnvelopeError::message)
}

#[derive(Debug)]
enum EnvelopeError {
    TooLarge,
    Invalid(String),
}
impl EnvelopeError {
    fn message(self) -> String {
        match self {
            Self::TooLarge => "Browser export request exceeds 128 MiB".into(),
            Self::Invalid(error) => error,
        }
    }
}

fn envelope_length(fonts: usize, request: usize) -> Result<usize, EnvelopeError> {
    fonts
        .checked_add(request)
        .and_then(|n| n.checked_add(4))
        .filter(|n| *n <= MAX_REQUEST_BYTES)
        .ok_or(EnvelopeError::TooLarge)
}

fn font_envelope(request: &[u8]) -> Result<Vec<u8>, EnvelopeError> {
    // Reject an oversized document before encoding fonts or allocating the
    // packed copy. Font bytes can also take an otherwise valid request over.
    envelope_length(0, request.len())?;
    let fonts: Vec<_> = crate::style::added_fonts(0)
        .into_iter()
        .filter_map(|source| {
            if let resvg::usvg::fontdb::Source::Binary(bytes) = source {
                Some(STANDARD.encode(bytes.as_ref().as_ref()))
            } else {
                None
            }
        })
        .collect();
    let fonts = serde_json::to_vec(&fonts).map_err(|e| EnvelopeError::Invalid(e.to_string()))?;
    let length = envelope_length(fonts.len(), request.len())?;
    let mut output = Vec::with_capacity(length);
    output.extend_from_slice(
        &u32::try_from(fonts.len())
            .map_err(|e| EnvelopeError::Invalid(e.to_string()))?
            .to_le_bytes(),
    );
    output.extend_from_slice(&fonts);
    output.extend_from_slice(request);
    Ok(output)
}

/// Install only newly added fonts, then hand the unchanged job bytes to its
/// responder. A replacement worker starts with a fresh font cursor.
pub fn install_fonts(request: &[u8]) -> Result<&[u8], String> {
    if request.len() > MAX_REQUEST_BYTES {
        return Err("Browser export request exceeds 128 MiB".into());
    }
    let (fonts, request) = split(request, MAX_REQUEST_BYTES)?;
    let fonts: Vec<String> = serde_json::from_slice(fonts).map_err(|e| e.to_string())?;
    let from = INSTALLED_FONTS.get();
    if fonts.len() < from {
        return Err("Browser export font snapshot is older than the worker".into());
    }
    for font in fonts.iter().skip(from) {
        crate::style::add_font(STANDARD.decode(font).map_err(|e| e.to_string())?)?;
        INSTALLED_FONTS.set(INSTALLED_FONTS.get() + 1);
    }
    Ok(request)
}

#[cfg(target_arch = "wasm32")]
pub async fn exchange(
    job: &'static str,
    request: &[u8],
    cancel: &crate::browser_worker::Cancel,
) -> Result<Vec<u8>, crate::browser_worker::Failure> {
    use crate::browser_worker::{self, Failure};
    if !available() {
        return Err(Failure::Unavailable(
            "This page has no matching local render worker".into(),
        ));
    }
    let request = font_envelope(request).map_err(|error| match error {
        EnvelopeError::TooLarge => Failure::Unavailable(error.message()),
        EnvelopeError::Invalid(error) => Failure::Failed(error),
    })?;
    browser_worker::exchange(job, &request, cancel, web_time::Duration::from_secs(60)).await
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum Background {
    Transparent,
    Canvas,
    Color([u8; 3]),
}
impl From<export::Background> for Background {
    fn from(value: export::Background) -> Self {
        match value {
            export::Background::Transparent => Self::Transparent,
            export::Background::Canvas => Self::Canvas,
            export::Background::Color(rgb) => Self::Color(rgb),
        }
    }
}
impl From<Background> for export::Background {
    fn from(value: Background) -> Self {
        match value {
            Background::Transparent => Self::Transparent,
            Background::Canvas => Self::Canvas,
            Background::Color(rgb) => Self::Color(rgb),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
enum Request<'a> {
    Save {
        document: Snapshot<'a>,
        format: String,
    },
    Publication {
        document: Snapshot<'a>,
        format: String,
        pages: bool,
        editable: bool,
        background: Background,
        dpi: u32,
        max_pixels: u64,
    },
}
#[derive(Serialize, Deserialize)]
struct Reply {
    error: Option<String>,
    details: Vec<String>,
}

fn split(bytes: &[u8], max_header: usize) -> Result<(&[u8], &[u8]), String> {
    let (length, rest) = bytes
        .split_at_checked(4)
        .ok_or("Truncated browser export response")?;
    let length = u32::from_le_bytes(
        length
            .try_into()
            .map_err(|_| "Invalid browser export header")?,
    ) as usize;
    if length > max_header {
        return Err("Browser export header exceeds its limit".into());
    }
    rest.split_at_checked(length)
        .ok_or_else(|| "Truncated browser export response".into())
}
fn reply(result: Result<export::Publication, String>) -> Result<Vec<u8>, String> {
    let (reply, bytes) = match result {
        Ok(output) => (
            Reply {
                error: None,
                details: output.details,
            },
            output.bytes,
        ),
        Err(error) => (
            Reply {
                error: Some(error),
                details: vec![],
            },
            vec![],
        ),
    };
    let header = serde_json::to_vec(&reply).map_err(|e| e.to_string())?;
    if header.len() > MAX_HEADER_BYTES || bytes.len() > MAX_RESPONSE_BYTES {
        return Err("Browser export response exceeds its limit".into());
    }
    let mut output = Vec::with_capacity(4 + header.len() + bytes.len());
    output.extend_from_slice(
        &u32::try_from(header.len())
            .map_err(|e| e.to_string())?
            .to_le_bytes(),
    );
    output.extend_from_slice(&header);
    output.extend_from_slice(&bytes);
    Ok(output)
}
#[cfg(any(target_arch = "wasm32", test))]
fn decode(bytes: Vec<u8>) -> Result<export::Publication, String> {
    if bytes.len() > MAX_RESPONSE_BYTES + MAX_HEADER_BYTES + 4 {
        return Err("Browser export response exceeds its limit".into());
    }
    let (header, body) = split(&bytes, MAX_HEADER_BYTES)?;
    let reply: Reply = serde_json::from_slice(header).map_err(|e| e.to_string())?;
    match reply.error {
        Some(error) => Err(error),
        None => Ok(export::Publication {
            bytes: body.to_vec(),
            details: reply.details,
        }),
    }
}

/// Worker-side entry point. Operation failures are ordinary replies; they must
/// not be mistaken for startup failure and rerun on the page thread.
pub async fn respond(request: &[u8]) -> Result<Vec<u8>, String> {
    let result = respond_inner(request).await;
    reply(result)
}
async fn respond_inner(request: &[u8]) -> Result<export::Publication, String> {
    if request.len() > MAX_REQUEST_BYTES {
        return Err("Browser export request exceeds 128 MiB".into());
    }
    let request: Request<'_> = serde_json::from_slice(request).map_err(|e| e.to_string())?;
    let engine = LocalEngine::default();
    match request {
        Request::Save { document, format } => Ok(export::Publication {
            bytes: formats::write(document.into_document(), &format).await?,
            details: vec![],
        }),
        Request::Publication {
            document,
            format,
            pages,
            editable,
            background,
            dpi,
            max_pixels,
        } => {
            let format = match format.as_str() {
                "svg" => "svg",
                "svg-office" => "svg-office",
                "png" => "png",
                "pdf" => "pdf",
                _ => return Err(format!("Unknown figure format {format}")),
            };
            export::publication(
                &engine,
                document.into_document(),
                format,
                pages,
                editable,
                background.into(),
                dpi,
                max_pixels,
            )
            .await
        }
    }
}

#[cfg(target_arch = "wasm32")]
async fn dispatch(
    request: &Request<'_>,
    cancel: &crate::browser_worker::Cancel,
) -> Result<Option<export::Publication>, String> {
    use crate::browser_worker::Failure;
    if !available() {
        return Ok(None);
    }
    let bytes = serde_json::to_vec(request).map_err(|e| e.to_string())?;
    match exchange(JOB, &bytes, cancel).await {
        Ok(bytes) => decode(bytes).map(Some),
        Err(Failure::Unavailable(_)) => Ok(None),
        Err(Failure::Failed(error)) => Err(error),
        Err(Failure::Cancelled) => Err("Browser export was canceled".into()),
    }
}

#[cfg(target_arch = "wasm32")]
pub async fn save(
    document: Document,
    format: &str,
    cancel: &crate::browser_worker::Cancel,
) -> Result<Vec<u8>, String> {
    if matches!(
        format,
        "png" | "svg" | "svg-office" | "pdf" | "cdxml" | "cdx"
    ) {
        let request = Request::Save {
            document: Snapshot::borrowed(&document),
            format: format.into(),
        };
        if let Some(output) = dispatch(&request, cancel).await? {
            return Ok(output.bytes);
        }
    }
    formats::write(document, format).await
}

#[cfg(target_arch = "wasm32")]
#[allow(clippy::too_many_arguments)]
pub async fn publication(
    engine: &LocalEngine,
    document: Document,
    format: &'static str,
    pages: bool,
    editable: bool,
    background: export::Background,
    dpi: u32,
    max_pixels: u64,
) -> Result<export::Publication, String> {
    let request = Request::Publication {
        document: Snapshot::borrowed(&document),
        format: format.into(),
        pages,
        editable,
        background: background.into(),
        dpi,
        max_pixels,
    };
    if let Some(output) = dispatch(&request, &Default::default()).await? {
        return Ok(output);
    }
    export::publication(
        engine, document, format, pages, editable, background, dpi, max_pixels,
    )
    .await
}

#[cfg(test)]
mod tests;
