//! The `convert` tool: structure text straight to a file or a preview,
//! without keeping a session document.
//!
//! It reads as [`import`](super::import) reads and writes as
//! [`export`](super::export) or [`render`](super::render) write, so one call
//! replaces import, export or render, and document_close.
use super::{
    budget::Budgets,
    catalog::arguments,
    error::OpError,
    exec::Context,
    export::{self, export_bytes},
    import::{self, Import, Read},
    naming::Hook,
    render,
    result::{Blob, Image, ToolResult},
    wire::{
        AnalysisJson, ExportReceiptJson, SvgText, described, nullable, svg_text_schema, warnings,
    },
};
use crate::{
    engine::LocalEngine,
    envelope::{ExportReceipt, Versions},
    tool_spec::{Hints, ToolSpec},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The `to` value for the editor's PNG preview, as render draws it.
const PREVIEW: &str = "preview";

pub const CONVERT: ToolSpec = ToolSpec {
    name: "convert",
    title: Some("Convert structure"),
    description: "Convert structure text in one call, keeping no session document: read input as import reads it, then return a file as export makes it, or with to preview the PNG preview render draws, attached as an image to look at. Returns a receipt {format, byte_len, detail}, the preview's width and height, and the molecular analysis. Use import when you will edit, inspect or reuse the drawing.",
    input_schema: schema,
    hints: Some(Hints {
        read_only: true,
        destructive: false,
        idempotent: true,
        open_world: false,
    }),
};

/// The export formats and [`PREVIEW`].
fn targets() -> Vec<&'static str> {
    let mut targets = vec![PREVIEW];
    targets.extend(super::documents::EXPORT_FORMATS);
    targets
}

fn schema() -> Value {
    // import's own descriptions are in the tool list once already.
    let mut import = import::schema();
    for property in ["format", "text"] {
        if let Some(schema) = import
            .pointer_mut(&format!("/properties/{property}"))
            .and_then(Value::as_object_mut)
        {
            schema.remove("description");
        }
    }
    json!({
        "type": "object",
        "properties": {
            "input": described(import, "The structure to read: import's format and text, with the same meanings and limits."),
            "to": {"type": "string", "enum": targets(), "description": "preview is the editor's PNG preview on white; every other value is an export format."},
            "pages": described(nullable(json!({"type": "boolean"})), "true exports every publication page as one PDF; it needs to pdf. null or false otherwise."),
            "svg_text": svg_text_schema(),
            "max_width": described(nullable(render::side_schema()), "The preview's width limit in pixels; null for 1600. Only preview uses it."),
            "max_height": described(nullable(render::side_schema()), "The preview's height limit in pixels; null for 1000. Only preview uses it."),
        },
        "required": ["input", "to", "pages", "svg_text", "max_width", "max_height"],
        "additionalProperties": false,
    })
}

/// The `convert` arguments as sent.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    input: Value,
    to: String,
    #[serde(default)]
    pages: Option<bool>,
    #[serde(default)]
    svg_text: Option<SvgText>,
    #[serde(default)]
    max_width: Option<u32>,
    #[serde(default)]
    max_height: Option<u32>,
}

/// What to write.
#[derive(Debug)]
enum Output {
    Preview {
        width: u32,
        height: u32,
    },
    Export {
        format: export::Format,
        pages: bool,
        svg_text: SvgText,
    },
}

/// The decoded `convert` arguments.
#[derive(Debug)]
pub(crate) struct Convert {
    input: Import,
    output: Output,
}

/// Decodes the arguments with import's text budget, render's size rules for
/// a preview and export's `pages` rule, before any work runs. A size given
/// with an export format is invalid, so a mistaken call never returns a
/// file of another size than asked for.
pub(crate) fn decode(args: Value, budgets: &Budgets) -> Result<Convert, OpError> {
    let Arguments {
        input,
        to,
        pages,
        svg_text,
        max_width,
        max_height,
    } = arguments(args)?;
    let input = import::decode(input, budgets)?;
    let pages = pages.unwrap_or(false);
    let format = if to == PREVIEW {
        None
    } else {
        Some(export::Format::named(&to).ok_or_else(|| {
            OpError::invalid(format!("to must be one of {}", targets().join(", "))).param("to")
        })?)
    };
    let output = match format {
        None => {
            if pages {
                return Err(OpError::invalid("pages is valid only with to pdf").param("pages"));
            }
            let (width, height) = render::size(max_width, max_height, budgets)?;
            Output::Preview { width, height }
        }
        Some(format) => {
            for (name, side) in [("max_width", max_width), ("max_height", max_height)] {
                if side.is_some() {
                    return Err(
                        OpError::invalid(format!("{name} is valid only with to preview"))
                            .param(name),
                    );
                }
            }
            if pages && format != export::Format::Pdf {
                return Err(OpError::invalid("pages is valid only with to pdf").param("pages"));
            }
            Output::Export {
                format,
                pages,
                svg_text: svg_text.unwrap_or_default(),
            }
        }
    };
    Ok(Convert { input, output })
}

/// The envelope value of a convert. `width` and `height` are the preview's
/// pixel size, `null` for a file.
#[derive(Debug, Serialize)]
struct Converted {
    receipt: ExportReceiptJson,
    width: Option<u32>,
    height: Option<u32>,
    analysis: Option<AnalysisJson>,
}

/// Reads the input and writes the output; nothing is stored.
///
/// The import's warnings come first, then the export's. An engine drawing
/// that validation rejects is a rejected envelope, as import returns it.
///
/// The successful result contract is in [`super::catalog::RESULTS`].
pub(crate) async fn convert(
    ctx: Context,
    engine: LocalEngine,
    naming: Hook,
    versions: Versions,
    budgets: Budgets,
    convert: Convert,
) -> Result<ToolResult, OpError> {
    let Convert { input, output } = convert;
    let Read {
        doc,
        warnings: messages,
        analysis,
    } = import::read(&ctx, &engine, &naming, input).await?;
    let doc = match doc {
        Ok(doc) => doc,
        Err(rejection) => return Ok(ToolResult::rejected(rejection, messages, &versions)),
    };
    budgets.check_objects(&doc)?;
    let mut warnings = warnings(messages);
    let analysis = analysis.map(AnalysisJson::from);
    match output {
        Output::Preview { width, height } => {
            let limits = budgets.clone();
            let render::Preview { bytes, size } = ctx
                .blocking(move || {
                    let format = render::Format::Png;
                    render::preview(&doc, format, width, height, SvgText::Outline, &limits)
                })
                .await?;
            let receipt = ExportReceipt {
                format: "png".to_owned(),
                byte_len: bytes.len(),
                detail: None,
            };
            Ok(ToolResult::valid(
                Converted {
                    receipt: ExportReceiptJson::from(&receipt),
                    width: size.map(|(width, _)| width),
                    height: size.map(|(_, height)| height),
                    analysis,
                },
                warnings,
                versions,
            )
            .with_images(vec![Image {
                mime: "image/png",
                bytes,
            }]))
        }
        Output::Export {
            format,
            pages,
            svg_text,
        } => {
            let (bytes, receipt, more) =
                export_bytes(&ctx, &engine, doc, format, pages, svg_text, &budgets).await?;
            warnings.extend(more);
            Ok(ToolResult::valid(
                Converted {
                    receipt: ExportReceiptJson::from(&receipt),
                    width: None,
                    height: None,
                    analysis,
                },
                warnings,
                versions,
            )
            .with_files(vec![Blob {
                mime: format.mime(),
                name: format!("drawing.{}", format.name()),
                bytes,
            }]))
        }
    }
}

#[cfg(test)]
mod tests;
