//! The `export` tool: a session document as a publication or chemistry
//! file, made by the same code as the app's export.
use super::{
    budget::Budgets,
    catalog::arguments,
    documents::EXPORT_FORMATS,
    error::{ErrorKind, OpError},
    exec::Context,
    import::warnings,
    policy::Access,
    result::{Blob, ToolResult},
    store::Documents,
    wire::{DocHandle, ExportReceiptJson, Principal, envelope_json, handle_schema, nullable},
};
use crate::{
    document::Document,
    engine::{LocalEngine, Request},
    envelope::{Envelope, ExportReceipt, ValidationStatus, Versions, Warning},
    export::Publication,
    tool_spec::{Hints, ToolSpec},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

pub const EXPORT: ToolSpec = ToolSpec {
    name: "export",
    title: Some("Export file"),
    description: "Export a session document as a file, exactly as the app's Export command makes it: svg, pdf or png figures on the drawing's background, or cdxml, mol, smiles or inchi chemistry. pages: true exports every publication page as one PDF; it needs format pdf and a drawing with publication pages. Pass null or false otherwise. PNG resolution steps down from the style's dpi until the image fits export_png_pixels. The file comes back inline, at most max_output_bytes (info reports both budgets), with a receipt {format, byte_len, detail}. Read-only.",
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
            "format": {"type": "string", "enum": EXPORT_FORMATS},
            "pages": nullable(json!({"type": "boolean"})),
        },
        "required": ["document", "format", "pages"],
        "additionalProperties": false,
    })
}

/// An export `format`, exactly [`EXPORT_FORMATS`]. EMF and CDX output are
/// not offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Format {
    Svg,
    Pdf,
    Png,
    Cdxml,
    Mol,
    Smiles,
    Inchi,
}

impl Format {
    /// The name in [`EXPORT_FORMATS`], which is also the app's format name
    /// and file extension.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Svg => "svg",
            Self::Pdf => "pdf",
            Self::Png => "png",
            Self::Cdxml => "cdxml",
            Self::Mol => "mol",
            Self::Smiles => "smiles",
            Self::Inchi => "inchi",
        }
    }

    pub(crate) fn mime(self) -> &'static str {
        match self {
            Self::Svg => "image/svg+xml",
            Self::Pdf => "application/pdf",
            Self::Png => "image/png",
            Self::Cdxml => "chemical/x-cdxml",
            Self::Mol => "chemical/x-mdl-molfile",
            Self::Smiles => "chemical/x-daylight-smiles",
            Self::Inchi => "chemical/x-inchi",
        }
    }

    /// A figure the app renders itself; the other formats come from the
    /// chemistry engine.
    fn figure(self) -> bool {
        matches!(self, Self::Svg | Self::Pdf | Self::Png)
    }
}

/// The `export` arguments.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Export {
    document: DocHandle,
    format: Format,
    #[serde(deserialize_with = "Option::deserialize")]
    pages: Option<bool>,
}

/// Decodes the arguments. `pages: true` needs format pdf; `null` and
/// `false` are accepted with every format.
pub(crate) fn decode(args: Value) -> Result<Export, OpError> {
    let export: Export = arguments(args)?;
    if export.pages == Some(true) && export.format != Format::Pdf {
        return Err(OpError::new(
            ErrorKind::InvalidArguments,
            "pages is valid only with format pdf",
        ));
    }
    Ok(export)
}

/// The envelope value of an export.
#[derive(Debug, Serialize)]
struct Exported {
    receipt: ExportReceiptJson,
}

fn failed(message: impl AsRef<str>) -> OpError {
    OpError::new(ErrorKind::Failed, message)
}

/// The file bytes, receipt and warnings of `doc` as `format`, made as the app
/// exports a drawing.
///
/// - svg, pdf and png: [`publication`](crate::export::publication) with
///   [`Budgets::export_png_pixels`], as `App::export_figure`
///   (src/app/figure_export.rs) calls it with `FILE_PIXELS`. Its details
///   become the warnings in order: the PNG size first, then the notice that
///   the drawing was preserved for review. The receipt's detail is the PNG
///   size.
/// - cdxml, mol, smiles and inchi: an engine `export` request with the
///   format set, as `App::export_drawing` (src/app/files.rs) sends it. Its
///   warnings become the warnings. No output is [`ErrorKind::Failed`]; the
///   app would save an empty file (src/app/engine_jobs.rs `engine_done`).
///
/// Every error is [`ErrorKind::Failed`] with the app's message, except a file
/// over [`Budgets::max_output_bytes`], which is [`ErrorKind::Budget`].
pub(crate) async fn export_bytes(
    ctx: &Context,
    engine: &LocalEngine,
    doc: Document,
    format: Format,
    pages: bool,
    budgets: &Budgets,
) -> Result<(Vec<u8>, ExportReceipt, Vec<Warning>), OpError> {
    let name = format.name();
    ctx.checkpoint()?;
    let made = if format.figure() {
        crate::export::publication(engine, doc, name, pages, budgets.export_png_pixels)
            .await
            .map(|Publication { bytes, details }| {
                let detail = if format == Format::Png {
                    details.first().cloned()
                } else {
                    None
                };
                (bytes, detail, details)
            })
    } else {
        let mut request = Request::molecule("export", doc);
        request.format = Some(name.into());
        engine.request(request).await.and_then(|response| {
            let output = response
                .output
                .ok_or_else(|| format!("The chemistry engine returned no {name} output"))?;
            Ok((output.into_bytes(), None, response.warnings))
        })
    };
    ctx.checkpoint()?;
    let (bytes, detail, messages) = made.map_err(failed)?;
    if bytes.len() > budgets.max_output_bytes {
        return Err(OpError::new(
            ErrorKind::Budget,
            format!(
                "The {name} file is {} bytes; max_output_bytes allows at most {}. Use svg or pdf, or reduce the drawing",
                bytes.len(),
                budgets.max_output_bytes
            ),
        ));
    }
    let receipt = ExportReceipt {
        format: name.to_owned(),
        byte_len: bytes.len(),
        detail,
    };
    Ok((bytes, receipt, warnings(messages)))
}

/// Exports the document with read access, through [`export_bytes`].
///
/// The file is `drawing.<format>` with the format's MIME type.
/// `{value: {receipt: {format, byte_len, detail}}, warnings, validation,
/// versions}`.
pub(crate) async fn export(
    ctx: Context,
    store: Arc<dyn Documents>,
    engine: LocalEngine,
    who: Principal,
    versions: Versions,
    budgets: Budgets,
    export: Export,
) -> Result<ToolResult, OpError> {
    let Export {
        document,
        format,
        pages,
    } = export;
    let doc = ctx
        .blocking(move || {
            let snapshot = store.snapshot(&who, &document, Access::Read)?;
            Ok(Arc::unwrap_or_clone(snapshot.doc))
        })
        .await?;
    let (bytes, receipt, warnings) =
        export_bytes(&ctx, &engine, doc, format, pages.unwrap_or(false), &budgets).await?;
    let value = envelope_json(&Envelope {
        value: Exported {
            receipt: ExportReceiptJson::from(&receipt),
        },
        warnings,
        validation: ValidationStatus::Valid,
        versions,
    });
    Ok(ToolResult {
        value,
        images: Vec::new(),
        files: vec![Blob {
            mime: format.mime(),
            name: format!("drawing.{}", format.name()),
            bytes,
        }],
        is_error: false,
    })
}

#[cfg(test)]
mod tests;
