//! Browser clipboard preparation uses the existing pipeline and ordered flavors.
//! Rendering errors are encoded replies, distinct from transport/startup errors.
use super::copy_as::{CopyFormat, PreparedCopy};
use super::{CopyOutcome, Document, Pictures, Representation, export, prepare_copy_with_limit};
use crate::browser_render::Snapshot;
use serde::{Deserialize, Serialize};

const VERSION: u32 = 1;
const REQUEST_BYTES: usize = 128 * 1024 * 1024;
const RESPONSE_BYTES: usize = 512 * 1024 * 1024;
type Prepared = (CopyOutcome, Vec<Representation>);

#[derive(Clone, Copy, Serialize, Deserialize)]
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

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PictureOptions {
    background: Background,
    dpi: u32,
    editable: bool,
}

impl From<Pictures> for PictureOptions {
    fn from(value: Pictures) -> Self {
        Self {
            background: value.background.into(),
            dpi: value.dpi,
            editable: value.editable,
        }
    }
}

impl From<PictureOptions> for Pictures {
    fn from(value: PictureOptions) -> Self {
        Self {
            background: value.background.into(),
            dpi: value.dpi,
            editable: value.editable,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request<'a> {
    version: u32,
    document: Snapshot<'a>,
    operation: Operation,
    pictures: PictureOptions,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Operation {
    Copy { image_only: bool },
    CopyAs { format: CopyFormat },
    ChemicalText { format: CopyFormat },
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    version: u32,
    result: Result<PreparedReply, String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum PreparedReply {
    Copy {
        outcome: CopyOutcome,
        representations: Vec<Representation>,
    },
    CopyAs {
        prepared: PreparedCopy,
    },
}

#[cfg(test)]
fn request(document: &Document, image_only: bool, pictures: Pictures) -> Result<Vec<u8>, String> {
    request_operation(document, Operation::Copy { image_only }, pictures)
}

fn request_operation(
    document: &Document,
    operation: Operation,
    pictures: Pictures,
) -> Result<Vec<u8>, String> {
    let bytes = serde_json::to_vec(&Request {
        version: VERSION,
        document: Snapshot::borrowed(document),
        operation,
        pictures: pictures.into(),
    })
    .map_err(|error| error.to_string())?;
    if bytes.len() > REQUEST_BYTES {
        return Err("Browser clipboard request exceeds 128 MiB".into());
    }
    Ok(bytes)
}

#[cfg(any(target_arch = "wasm32", test))]
fn decode(bytes: &[u8]) -> Result<PreparedReply, String> {
    if bytes.len() > RESPONSE_BYTES {
        return Err("Browser clipboard response exceeds 512 MiB".into());
    }
    let reply: Reply = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    if reply.version != VERSION {
        return Err("Unsupported browser clipboard response version".into());
    }
    reply.result
}

fn copy_result(reply: PreparedReply) -> Result<Prepared, String> {
    match reply {
        PreparedReply::Copy {
            outcome,
            representations,
        } => Ok((outcome, representations)),
        PreparedReply::CopyAs { .. } => {
            Err("Mismatched browser clipboard response operation".into())
        }
    }
}

fn as_result(reply: PreparedReply) -> Result<PreparedCopy, String> {
    match reply {
        PreparedReply::CopyAs { prepared } => Ok(prepared),
        PreparedReply::Copy { .. } => Err("Mismatched browser clipboard response operation".into()),
    }
}

/// Worker-side preparation only. The page keeps the existing ClipboardItem
/// publication and partial-format fallbacks. Even invalid input/rendering
/// failures are ordinary replies, so the transport cannot retry them locally.
pub async fn respond(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let result = respond_inner(bytes).await;
    let bytes = serde_json::to_vec(&Reply {
        version: VERSION,
        result,
    })
    .map_err(|error| error.to_string())?;
    if bytes.len() > RESPONSE_BYTES {
        return Err("Browser clipboard response exceeds 512 MiB".into());
    }
    Ok(bytes)
}

async fn respond_inner(bytes: &[u8]) -> Result<PreparedReply, String> {
    if bytes.len() > REQUEST_BYTES {
        return Err("Browser clipboard request exceeds 128 MiB".into());
    }
    let request: Request<'_> = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    if request.version != VERSION {
        return Err("Unsupported browser clipboard request version".into());
    }
    let engine = crate::engine::LocalEngine::default();
    let document = request.document.into_document();
    match request.operation {
        Operation::Copy { image_only } => prepare_copy_with_limit(
            engine,
            document,
            image_only,
            true,
            crate::payload::EMBED_LIMIT,
            request.pictures.into(),
        )
        .await
        .map(|(outcome, representations)| PreparedReply::Copy {
            outcome,
            representations,
        }),
        Operation::CopyAs { format } => {
            super::copy_as::prepare_as_local(engine, document, format, request.pictures.into())
                .await
                .map(|prepared| PreparedReply::CopyAs { prepared })
        }
        Operation::ChemicalText { format } => {
            super::copy_as::prepare_chemical_text_local(engine, document, format)
                .await
                .map(|prepared| PreparedReply::CopyAs { prepared })
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub(super) async fn prepare(
    document: &Document,
    image_only: bool,
    pictures: Pictures,
) -> Result<Option<Prepared>, String> {
    exchange(document, Operation::Copy { image_only }, pictures)
        .await?
        .map(copy_result)
        .transpose()
}

#[cfg(target_arch = "wasm32")]
pub(super) async fn prepare_as(
    document: &Document,
    format: CopyFormat,
    pictures: Pictures,
) -> Result<Option<PreparedCopy>, String> {
    exchange(document, Operation::CopyAs { format }, pictures)
        .await?
        .map(as_result)
        .transpose()
}

#[cfg(target_arch = "wasm32")]
pub(super) async fn prepare_chemical_text(
    document: &Document,
    format: CopyFormat,
) -> Result<Option<PreparedCopy>, String> {
    exchange(
        document,
        Operation::ChemicalText { format },
        Pictures::default(),
    )
    .await?
    .map(as_result)
    .transpose()
}

#[cfg(target_arch = "wasm32")]
async fn exchange(
    document: &Document,
    operation: Operation,
    pictures: Pictures,
) -> Result<Option<PreparedReply>, String> {
    use crate::browser_worker::{self, Failure};
    if !crate::browser_render::available() {
        return Ok(None);
    }
    let bytes = request_operation(document, operation, pictures)?;
    let cancel = browser_worker::Cancel::default();
    match crate::browser_render::exchange("clipboard", &bytes, &cancel).await {
        Ok(reply) => decode(&reply).map(Some),
        Err(Failure::Unavailable(_)) => Ok(None),
        Err(Failure::Failed(error)) => Err(error),
        Err(Failure::Cancelled) => Err("Browser clipboard preparation was canceled".into()),
    }
}

#[cfg(test)]
mod tests;
