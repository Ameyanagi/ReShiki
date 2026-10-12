//! The `import` tool: structure text or a native drawing as a new session
//! document.
use super::{
    budget::Budgets,
    catalog::arguments,
    documents::IMPORT_FORMATS,
    error::{ErrorKind, OpError},
    exec::Context,
    result::ToolResult,
    store::Documents,
    wire::{AnalysisJson, DocHandle, Principal, Revision, warnings},
};
use crate::{
    atom_labels,
    document::{Document, History},
    engine::{Analysis, LocalEngine, Request, Response, text_format},
    envelope::Versions,
    tool_spec::{Hints, ToolSpec},
    transaction::{self, Rejection},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

pub const IMPORT: ToolSpec = ToolSpec {
    name: "import",
    title: Some("Import structure"),
    description: "Import structure text as a new session document and return its handle, revision, object counts and molecular analysis. format is smiles, mol, rxn, rsmi (reaction SMILES), inchi, cdxml, cdx (the binary file as base64 text), reshiki (a native ReShiki drawing's JSON), or auto, which tells SMILES, MOL, RXN, reaction SMILES, InChI and CDXML apart. text is at most max_text_bytes, or max_cdx_base64 for cdx; info reports both. Every call creates another document, even for the same text; close the ones you no longer need with document_close.",
    input_schema: schema,
    hints: Some(Hints {
        read_only: false,
        destructive: false,
        idempotent: false,
        open_world: false,
    }),
};

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "format": {"type": "string", "enum": IMPORT_FORMATS},
            "text": {"type": "string"},
        },
        "required": ["format", "text"],
        "additionalProperties": false,
    })
}

/// An import `format`, exactly [`IMPORT_FORMATS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Format {
    Auto,
    Smiles,
    Mol,
    Rxn,
    Rsmi,
    Inchi,
    Cdxml,
    Cdx,
    Reshiki,
}

impl Format {
    /// The name in [`IMPORT_FORMATS`], which is also the engine's format name
    /// for every format but `auto` and `reshiki`.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Smiles => "smiles",
            Self::Mol => "mol",
            Self::Rxn => "rxn",
            Self::Rsmi => "rsmi",
            Self::Inchi => "inchi",
            Self::Cdxml => "cdxml",
            Self::Cdx => "cdx",
            Self::Reshiki => "reshiki",
        }
    }
}

/// The `import` arguments.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Import {
    pub(crate) format: Format,
    pub(crate) text: String,
}

/// Decodes the arguments and checks the text budget before any parse or
/// engine work: cdx text against [`Budgets::max_cdx_base64`] only, every other
/// format against [`Budgets::max_text_bytes`].
pub(crate) fn decode(args: Value, budgets: &Budgets) -> Result<Import, OpError> {
    let import: Import = arguments(args)?;
    let (limit, name) = if import.format == Format::Cdx {
        (budgets.max_cdx_base64, "max_cdx_base64")
    } else {
        (budgets.max_text_bytes, "max_text_bytes")
    };
    if import.text.len() > limit {
        return Err(OpError::new(
            ErrorKind::Budget,
            format!(
                "The text is {} bytes; {name} allows at most {limit}",
                import.text.len()
            ),
        ));
    }
    Ok(import)
}

/// Objects in an imported drawing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
struct Counts {
    atoms: usize,
    bonds: usize,
    arrows: usize,
    annotations: usize,
    graphics: usize,
}

impl Counts {
    fn of(doc: &Document) -> Self {
        Self {
            atoms: doc.atoms.len(),
            bonds: doc.bonds.len(),
            arrows: doc.arrows.len(),
            annotations: doc.annotations.len(),
            graphics: doc.graphics.len(),
        }
    }
}

/// The envelope value of a successful import.
#[derive(Debug, Serialize)]
struct Imported {
    document: DocHandle,
    revision: Revision,
    counts: Counts,
    analysis: Option<AnalysisJson>,
    /// Only for a document opened from a file.
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<Source>,
}

/// The file a document was opened from: the requested path and its size.
#[derive(Debug, Serialize)]
pub(crate) struct Source {
    pub(crate) path: String,
    pub(crate) bytes: usize,
}

/// Finalizes an engine import's drawing exactly as `App::structure_replaced`
/// (src/app/engine_jobs.rs) does for a new, empty drawing, minus the history
/// step: reconcile against an empty document, commit into a discarded
/// [`History`] (which clears computed labels when the chemistry changed),
/// then copy the response's computed labels back.
#[doc(hidden)]
pub fn finalize(response: &Document) -> Result<Document, Rejection> {
    let mut doc = response.clone();
    transaction::reconcile(&mut doc, Document::default())?.commit(
        &mut doc,
        &mut History::default(),
        false,
    );
    atom_labels::refresh_computed(&mut doc, response);
    Ok(doc)
}

/// The drawing to store, or the rejection of an engine drawing.
struct Read {
    doc: Result<Document, Rejection>,
    warnings: Vec<String>,
    analysis: Option<Analysis>,
}

/// Reads `text` as `format` without touching the store.
async fn read(ctx: &Context, engine: &LocalEngine, import: Import) -> Result<Read, OpError> {
    let Import { format, text } = import;
    if format == Format::Reshiki {
        // As the app opens a native file (src/app/files.rs).
        let doc = ctx
            .blocking(move || {
                Document::from_native_file(text.as_bytes())
                    .map_err(|error| OpError::failed(format!("Could not open document: {error}")))
            })
            .await?;
        return Ok(Read {
            doc: Ok(doc),
            warnings: Vec::new(),
            analysis: None,
        });
    }
    let name = match format {
        Format::Auto => text_format(&text),
        format => format.name(),
    };
    let mut request = Request::import(name, "");
    request.text = Some(text);
    ctx.checkpoint()?;
    let response = engine.request(request).await.map_err(OpError::failed);
    ctx.checkpoint()?;
    let Response {
        document,
        analysis,
        warnings,
        ..
    } = response?;
    let document = document.ok_or_else(|| OpError::failed("Import returned no drawing"))?;
    let doc = ctx.blocking(move || Ok(finalize(&document))).await?;
    Ok(Read {
        doc,
        warnings,
        analysis,
    })
}

/// Imports into a new session document, through [`import_document`].
///
/// `{value: {document, revision, counts: {atoms, bonds, arrows, annotations,
/// graphics}, analysis}, warnings, validation, versions}`; `analysis` is
/// `null` when the engine returned none.
pub(crate) async fn import(
    ctx: Context,
    store: Arc<dyn Documents>,
    engine: LocalEngine,
    who: Principal,
    versions: Versions,
    import: Import,
) -> Result<ToolResult, OpError> {
    import_document(ctx, store, engine, who, versions, import, None).await
}

/// Reads `import` into a new session document; `source`, when given, joins
/// the result's value.
///
/// `reshiki` text opens as the app opens a native file; every other format
/// goes to the engine, and its drawing is [`finalize`]d. The store's create is
/// the effect. It is not idempotent: a cancel that arrives after it leaves a
/// listed document the caller can close.
pub(crate) async fn import_document(
    mut ctx: Context,
    store: Arc<dyn Documents>,
    engine: LocalEngine,
    who: Principal,
    versions: Versions,
    import: Import,
    source: Option<Source>,
) -> Result<ToolResult, OpError> {
    let Read {
        doc,
        warnings: messages,
        analysis,
    } = read(&ctx, &engine, import).await?;
    let doc = match doc {
        Ok(doc) => doc,
        Err(rejection) => return Ok(ToolResult::rejected(rejection, messages, &versions)),
    };
    let counts = Counts::of(&doc);
    let created = ctx.effect(move || store.create(&who, doc)).await?;
    Ok(ToolResult::valid(
        Imported {
            document: created.handle,
            revision: created.revision,
            counts,
            analysis: analysis.map(AnalysisJson::from),
            source,
        },
        warnings(messages),
        versions,
    ))
}

#[cfg(test)]
mod tests;
