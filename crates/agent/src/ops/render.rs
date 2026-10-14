//! The `render` tool: a preview image of a session document, as the
//! assistant's canvas tools show the drawing. Publication files come from
//! [`export`](super::export).
use super::{
    budget::{Budgets, RenderBudget},
    catalog::arguments,
    error::{ErrorKind, OpError},
    exec::Context,
    ids,
    policy::Access,
    result::{Blob, Image, ToolResult},
    store::Documents,
    wire::{DocHandle, ObjectId, Principal, handle_schema, ids_schema, nullable},
};
use crate::{
    canvas_tools, editing,
    envelope::Versions,
    scene,
    tool_spec::{Hints, ToolSpec},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

/// The `format` values the render operation accepts.
pub const FORMATS: &[&str] = &["png", "svg"];

pub const RENDER: ToolSpec = ToolSpec {
    name: "render",
    title: Some("Render preview"),
    description: "Render a session document, or the part ids select (null for the whole drawing), as a preview to look at. png is drawn on white, scaled up to 3x and down to fit within max_width × max_height pixels (null for 1600 × 1000; each side 1 to 8192, and at most render.max_pixels in all, as info reports). svg is the drawing with a transparent surround. This is the editor's preview, not a publication file; use export for files. Read-only.",
    input_schema: schema,
    hints: Some(Hints {
        read_only: true,
        destructive: false,
        idempotent: true,
        open_world: false,
    }),
};

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "document": handle_schema(),
            "format": {"type": "string", "enum": FORMATS},
            "max_width": nullable(side_schema()),
            "max_height": nullable(side_schema()),
            "ids": nullable(ids_schema()),
        },
        "required": ["document", "format", "max_width", "max_height", "ids"],
        "additionalProperties": false,
    })
}

/// The default [`RenderBudget`](super::budget::RenderBudget) side limits.
fn side_schema() -> Value {
    json!({
        "type": "integer",
        "minimum": RenderBudget::DEFAULT.min_side,
        "maximum": RenderBudget::DEFAULT.max_side,
    })
}

/// A render `format`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Format {
    Png,
    Svg,
}

/// The `render` arguments as sent.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    document: DocHandle,
    format: Format,
    #[serde(deserialize_with = "Option::deserialize")]
    max_width: Option<u32>,
    #[serde(deserialize_with = "Option::deserialize")]
    max_height: Option<u32>,
    #[serde(deserialize_with = "Option::deserialize")]
    ids: Option<Vec<ObjectId>>,
}

/// The decoded `render` arguments, with the default size filled in.
#[derive(Debug)]
pub(crate) struct Render {
    document: DocHandle,
    format: Format,
    width: u32,
    height: u32,
    ids: Option<Vec<ObjectId>>,
}

/// Decodes the arguments and checks the size before any work runs.
///
/// A null side takes the budget's default. Each side must be within
/// [`RenderBudget::min_side`](super::budget::RenderBudget::min_side) and
/// [`RenderBudget::max_side`](super::budget::RenderBudget::max_side)
/// ([`ErrorKind::InvalidArguments`]; the schema also forbids 0), and the
/// pixel count within
/// [`RenderBudget::max_pixels`](super::budget::RenderBudget::max_pixels)
/// ([`ErrorKind::Budget`]).
pub(crate) fn decode(args: Value, budgets: &Budgets) -> Result<Render, OpError> {
    let Arguments {
        document,
        format,
        max_width,
        max_height,
        ids,
    } = arguments(args)?;
    if let Some(ids) = &ids {
        ids::check_count(ids, budgets)?;
    }
    let render = &budgets.render;
    let side = |name: &str, value: Option<u32>, default: u32| {
        let value = value.unwrap_or(default);
        if (render.min_side..=render.max_side).contains(&value) {
            Ok(value)
        } else {
            Err(OpError::new(
                ErrorKind::InvalidArguments,
                format!(
                    "{name} must be from {} to {} pixels",
                    render.min_side, render.max_side
                ),
            ))
        }
    };
    let width = side("max_width", max_width, render.default_width)?;
    let height = side("max_height", max_height, render.default_height)?;
    let pixels = u64::from(width) * u64::from(height);
    if pixels > render.max_pixels {
        return Err(OpError::new(
            ErrorKind::Budget,
            format!(
                "{width} × {height} is {pixels} pixels; render.max_pixels allows at most {}",
                render.max_pixels
            ),
        ));
    }
    Ok(Render {
        document,
        format,
        width,
        height,
        ids,
    })
}

/// The envelope value of a render. `width` and `height` are the PNG's pixel
/// size, `null` for svg.
#[derive(Debug, Serialize)]
struct Rendered {
    width: Option<u32>,
    height: Option<u32>,
    byte_len: usize,
}

/// How to fit a preview over [`Budgets::max_output_bytes`].
const SHRINK: &str = "Reduce max_width and max_height, or pass ids to render part of the drawing";

/// The width and height in a PNG's IHDR chunk, which follows the 8-byte
/// signature.
fn png_size(png: &[u8]) -> Option<(u32, u32)> {
    let word = |at: usize| {
        png.get(at..at + 4)
            .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
            .map(u32::from_be_bytes)
    };
    if png.get(12..16) != Some(b"IHDR".as_slice()) {
        return None;
    }
    Some((word(16)?, word(20)?))
}

/// Renders the document with read access.
///
/// A selection is checked with [`ids::resolve`] and cut out with
/// [`editing::selection`]. png is [`canvas_tools::image_within`] at the
/// decoded size and comes back as an image; svg is [`scene::svg`] and comes
/// back as the file `drawing.svg`. Either must fit
/// [`Budgets::max_output_bytes`].
///
/// The successful result contract is in [`super::catalog::RESULTS`].
pub(crate) async fn render(
    ctx: Context,
    store: Arc<dyn Documents>,
    who: Principal,
    versions: Versions,
    budgets: Budgets,
    render: Render,
) -> Result<ToolResult, OpError> {
    let Render {
        document,
        format,
        width,
        height,
        ids,
    } = render;
    ctx.blocking(move || {
        let snapshot = store.snapshot(&who, &document, Access::Read)?;
        let part = match &ids {
            None => None,
            Some(ids) => Some(editing::selection(
                &snapshot.doc,
                &ids::resolve(&snapshot.doc, ids)?,
            )),
        };
        let doc = part.as_ref().unwrap_or(&snapshot.doc);
        let (bytes, size, name) = match format {
            Format::Png => {
                let png =
                    canvas_tools::image_within(doc, width, height).map_err(OpError::failed)?;
                let size = png_size(&png).ok_or_else(|| OpError::failed("internal error"))?;
                (png, Some(size), "png")
            }
            Format::Svg => (scene::svg(doc).into_bytes(), None, "svg"),
        };
        if bytes.len() > budgets.max_output_bytes {
            return Err(OpError::new(
                ErrorKind::Budget,
                format!(
                    "The {name} preview is {} bytes; max_output_bytes allows at most {}. {SHRINK}",
                    bytes.len(),
                    budgets.max_output_bytes
                ),
            ));
        }
        let result = ToolResult::valid(
            Rendered {
                width: size.map(|(width, _)| width),
                height: size.map(|(_, height)| height),
                byte_len: bytes.len(),
            },
            Vec::new(),
            versions,
        );
        Ok(match format {
            Format::Png => result.with_images(vec![Image {
                mime: "image/png",
                bytes,
            }]),
            Format::Svg => result.with_files(vec![Blob {
                mime: "image/svg+xml",
                name: "drawing.svg".to_owned(),
                bytes,
            }]),
        })
    })
    .await
}

#[cfg(test)]
mod tests;
