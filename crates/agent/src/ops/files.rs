//! The `file_open` and `file_save` tools: session documents from and to
//! files inside the folders the user granted at startup ([`crate::access`]).
//!
//! Every file request goes through [`Grants`];
//! [`FILE_FORMATS`](crate::access::extensions::FILE_FORMATS) maps each
//! extension to its import and export format. The tools are listed even when
//! no folder is granted, so the catalog stays the same; their calls then fail
//! with `access_denied` and say how folders are granted.
use super::{
    budget::Budgets,
    catalog::arguments,
    documents::IMPORT_FORMATS,
    error::{ErrorKind, OpError},
    exec::Context,
    export::{self, export_bytes},
    import::{self, Import, Source, import_document, warnings},
    policy::Access,
    result::ToolResult,
    store::Documents,
    wire::{DocHandle, Principal, envelope_json, handle_schema, nullable},
};
use crate::{
    access::{
        Grants, WriteMode,
        extensions::{FileFormat, READ, WRITE, extension_not_allowed},
    },
    document::Document,
    engine::LocalEngine,
    envelope::{Envelope, ExportReceipt, ValidationStatus, Versions},
    tool_spec::{Hints, ToolSpec},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

/// Paths longer than this many bytes are refused, as the grants refuse them.
const MAX_PATH_BYTES: usize = 4096;

pub const FILE_OPEN: ToolSpec = ToolSpec {
    name: "file_open",
    title: Some("Open file"),
    description: "Open a structure file or a native ReShiki drawing as a new session document and return what import returns, plus source {path, bytes}. path must be an absolute path inside a folder the user granted for reading when the server started; info lists the granted folders and the extensions files open from. format auto follows the extension (.rsk, .reshiki and .moruno drawings; .mol, .rxn, .rsmi, .cdxml, .cdx, .smi, .smiles and .inchi structures); any other format must be the extension's own. Files are at most max_text_bytes. Every call creates another document; close the ones you no longer need with document_close.",
    input_schema: open_schema,
    hints: Some(Hints {
        read_only: false,
        destructive: false,
        idempotent: false,
        open_world: false,
    }),
};

pub const FILE_SAVE: ToolSpec = ToolSpec {
    name: "file_save",
    title: Some("Save file"),
    description: "Save a session document as a file: a native ReShiki drawing (reshiki, .rsk), svg, pdf or png figures, or cdxml, mol, smiles or inchi chemistry, made exactly as export makes them. Every file is at most max_output_bytes. Use an absolute path inside a folder the user granted for writing when the server started; info lists the granted folders and the extensions files save to. The parent folder must exist; folders are never created. format null follows the extension; any other format must be the extension's own. Existing files are kept unless overwrite is true, which replaces them. pages: true saves every publication page as one PDF; pass null or false otherwise. Returns a receipt {path, format, byte_len, replaced, detail}.",
    input_schema: save_schema,
    hints: Some(Hints {
        read_only: false,
        destructive: true,
        idempotent: false,
        open_world: false,
    }),
};

/// The `format` values file_save accepts besides `null`.
const SAVE_FORMATS: &[&str] = &[
    "reshiki", "svg", "pdf", "png", "cdxml", "mol", "smiles", "inchi",
];

fn path_schema() -> Value {
    json!({"type": "string", "maxLength": MAX_PATH_BYTES})
}

fn open_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "path": path_schema(),
            "format": {"type": "string", "enum": IMPORT_FORMATS},
        },
        "required": ["path", "format"],
        "additionalProperties": false,
    })
}

fn save_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "document": handle_schema(),
            "path": path_schema(),
            "format": nullable(json!({"type": "string", "enum": SAVE_FORMATS})),
            "overwrite": {"type": "boolean"},
            "pages": nullable(json!({"type": "boolean"})),
        },
        "required": ["document", "path", "format", "overwrite", "pages"],
        "additionalProperties": false,
    })
}

/// The `file_open` arguments as sent.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenArgs {
    path: String,
    format: import::Format,
}

/// Decoded `file_open` arguments: the format is never `auto`.
#[derive(Debug)]
pub(crate) struct Open {
    path: String,
    format: import::Format,
}

/// The import formats a file can be read as.
const OPEN_FORMATS: [import::Format; 8] = [
    import::Format::Reshiki,
    import::Format::Mol,
    import::Format::Rxn,
    import::Format::Rsmi,
    import::Format::Inchi,
    import::Format::Cdxml,
    import::Format::Cdx,
    import::Format::Smiles,
];

/// A `file_save` `format`, exactly [`SAVE_FORMATS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum SaveFormat {
    Reshiki,
    Svg,
    Pdf,
    Png,
    Cdxml,
    Mol,
    Smiles,
    Inchi,
}

impl SaveFormat {
    const ALL: [Self; 8] = [
        Self::Reshiki,
        Self::Svg,
        Self::Pdf,
        Self::Png,
        Self::Cdxml,
        Self::Mol,
        Self::Smiles,
        Self::Inchi,
    ];

    fn name(self) -> &'static str {
        match self.export() {
            Some(format) => format.name(),
            None => "reshiki",
        }
    }

    /// The export format, or `None` for a native drawing.
    fn export(self) -> Option<export::Format> {
        Some(match self {
            Self::Reshiki => return None,
            Self::Svg => export::Format::Svg,
            Self::Pdf => export::Format::Pdf,
            Self::Png => export::Format::Png,
            Self::Cdxml => export::Format::Cdxml,
            Self::Mol => export::Format::Mol,
            Self::Smiles => export::Format::Smiles,
            Self::Inchi => export::Format::Inchi,
        })
    }
}

/// The `file_save` arguments as sent.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveArgs {
    document: DocHandle,
    path: String,
    #[serde(deserialize_with = "Option::deserialize")]
    format: Option<SaveFormat>,
    overwrite: bool,
    #[serde(deserialize_with = "Option::deserialize")]
    pages: Option<bool>,
}

/// Decoded `file_save` arguments: the format is resolved.
#[derive(Debug)]
pub(crate) struct Save {
    document: DocHandle,
    path: String,
    format: SaveFormat,
    overwrite: bool,
    pages: bool,
}

fn invalid(message: impl AsRef<str>) -> OpError {
    OpError::new(ErrorKind::InvalidArguments, message)
}

fn check_length(path: &str) -> Result<(), OpError> {
    if path.len() > MAX_PATH_BYTES {
        return Err(invalid(format!(
            "path_invalid: the path is {} bytes; at most {MAX_PATH_BYTES} are allowed",
            path.len()
        )));
    }
    Ok(())
}

/// The format `path`'s extension maps to in `direction`, which `requested`
/// must equal unless it is `None` (follow the extension). An extension
/// without such a format is `extension_not_allowed`, as the grants report it.
fn resolve(
    path: &str,
    requested: Option<&'static str>,
    direction: fn(&FileFormat) -> Option<&'static str>,
    allowed: &[&str],
    verb: &str,
) -> Result<&'static str, OpError> {
    let Some((row, format)) = FileFormat::of(path).and_then(|row| Some((row, direction(row)?)))
    else {
        return Err(extension_not_allowed(path, allowed).into());
    };
    match requested {
        Some(requested) if requested != format => Err(invalid(format!(
            "format {requested} does not match the .{} extension, which {verb} as {format}",
            row.ext
        ))),
        _ => Ok(format),
    }
}

/// Decodes `file_open`: the path fits, its extension opens, and an explicit
/// format is the extension's own. `auto` becomes the extension's format.
pub(crate) fn decode_open(args: Value) -> Result<Open, OpError> {
    let OpenArgs { path, format } = arguments(args)?;
    check_length(&path)?;
    let requested = (format != import::Format::Auto).then(|| format.name());
    let name = resolve(&path, requested, |row| row.import, &READ, "opens")?;
    let format = OPEN_FORMATS
        .into_iter()
        .find(|format| format.name() == name)
        .ok_or_else(|| invalid(format!("format {name} cannot be opened")))?;
    Ok(Open { path, format })
}

/// Decodes `file_save`: the path fits, its extension saves, and an explicit
/// format is the extension's own. `pages: true` needs format pdf.
pub(crate) fn decode_save(args: Value) -> Result<Save, OpError> {
    let SaveArgs {
        document,
        path,
        format,
        overwrite,
        pages,
    } = arguments(args)?;
    check_length(&path)?;
    let requested = format.map(SaveFormat::name);
    let name = resolve(&path, requested, |row| row.export, &WRITE, "saves")?;
    let format = SaveFormat::ALL
        .into_iter()
        .find(|format| format.name() == name)
        .ok_or_else(|| invalid(format!("format {name} cannot be saved")))?;
    let pages = pages.unwrap_or(false);
    if pages && format != SaveFormat::Pdf {
        return Err(invalid("pages is valid only with format pdf"));
    }
    Ok(Save {
        document,
        path,
        format,
        overwrite,
        pages,
    })
}

/// File contents as import text, exactly as the app reads a structure file
/// (src/app/import.rs `contents`): binary CDX as base64, everything else as
/// UTF-8.
fn contents(format: import::Format, bytes: Vec<u8>) -> Result<String, OpError> {
    if format == import::Format::Cdx {
        Ok(STANDARD.encode(bytes))
    } else {
        String::from_utf8(bytes).map_err(|error| {
            OpError::new(
                ErrorKind::Failed,
                format!("The file is not valid UTF-8: {error}"),
            )
        })
    }
}

/// Opens a granted file of at most [`Budgets::max_text_bytes`] as a new
/// session document, through [`import_document`] as `import` reads text.
///
/// The import result with `source: {path, bytes}` in its value.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn file_open(
    ctx: Context,
    store: Arc<dyn Documents>,
    engine: LocalEngine,
    grants: Arc<Grants>,
    who: Principal,
    versions: Versions,
    budgets: Budgets,
    open: Open,
) -> Result<ToolResult, OpError> {
    let Open { path, format } = open;
    let limit = budgets.max_text_bytes;
    let (text, bytes) = ctx
        .blocking({
            let path = path.clone();
            move || {
                let bytes = grants.read(&path, limit, &READ)?;
                let len = bytes.len();
                Ok((contents(format, bytes)?, len))
            }
        })
        .await?;
    let import = Import { format, text };
    let source = Source { path, bytes };
    import_document(ctx, store, engine, who, versions, import, Some(source)).await
}

/// The receipt of a saved file.
#[derive(Debug, Serialize)]
struct SaveReceipt {
    path: String,
    format: String,
    byte_len: usize,
    replaced: bool,
    detail: Option<String>,
}

/// The envelope value of a save.
#[derive(Debug, Serialize)]
struct Saved {
    receipt: SaveReceipt,
}

/// The native file of `doc`, `Document::file_json` as the app saves it, at
/// most `limit` ([`Budgets::max_output_bytes`]) bytes, as [`export_bytes`]
/// bounds every other format.
fn native_bytes(doc: &Document, limit: usize) -> Result<Vec<u8>, OpError> {
    let bytes = doc
        .file_json()
        .map_err(|error| OpError::new(ErrorKind::Failed, error))?;
    if bytes.len() > limit {
        return Err(OpError::new(
            ErrorKind::Budget,
            format!(
                "The reshiki file is {} bytes; max_output_bytes allows at most {limit}. Reduce the drawing",
                bytes.len()
            ),
        ));
    }
    Ok(bytes)
}

/// Saves a snapshot of the document to a granted file.
///
/// - reshiki: the native file, [`native_bytes`];
/// - every other format: [`export_bytes`].
///
/// Both are at most [`Budgets::max_output_bytes`], as export's files are.
///
/// The write is the effect, after the last cancellation point: a new file is
/// published without clobbering anything, and `overwrite: true` replaces a
/// regular file. `{value: {receipt: {path, format, byte_len, replaced,
/// detail}}, warnings, validation, versions}`; the producer's warnings come
/// first, then the write's.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn file_save(
    mut ctx: Context,
    store: Arc<dyn Documents>,
    engine: LocalEngine,
    grants: Arc<Grants>,
    who: Principal,
    versions: Versions,
    budgets: Budgets,
    save: Save,
) -> Result<ToolResult, OpError> {
    let Save {
        document,
        path,
        format,
        overwrite,
        pages,
    } = save;
    let doc = ctx
        .blocking(move || {
            let snapshot = store.snapshot(&who, &document, Access::Read)?;
            Ok(Arc::unwrap_or_clone(snapshot.doc))
        })
        .await?;
    let (bytes, receipt, mut messages) = match format.export() {
        Some(export) => export_bytes(&ctx, &engine, doc, export, pages, &budgets).await?,
        None => {
            let limit = budgets.max_output_bytes;
            let bytes = ctx.blocking(move || native_bytes(&doc, limit)).await?;
            let receipt = ExportReceipt {
                format: format.name().to_owned(),
                byte_len: bytes.len(),
                detail: None,
            };
            (bytes, receipt, Vec::new())
        }
    };
    let mode = if overwrite {
        WriteMode::Replace
    } else {
        WriteMode::CreateNew
    };
    let written = ctx
        .effect(move || Ok(grants.write_atomic(&path, &bytes, mode, &WRITE)?))
        .await?;
    messages.extend(warnings(written.warnings));
    let value = envelope_json(&Envelope {
        value: Saved {
            receipt: SaveReceipt {
                path: written.path,
                format: receipt.format,
                byte_len: written.bytes,
                replaced: written.replaced,
                detail: receipt.detail,
            },
        },
        warnings: messages,
        validation: ValidationStatus::Valid,
        versions,
    });
    Ok(ToolResult {
        value,
        images: Vec::new(),
        files: Vec::new(),
        is_error: false,
    })
}

#[cfg(test)]
mod tests;
