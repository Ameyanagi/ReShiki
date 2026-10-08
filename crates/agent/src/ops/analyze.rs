//! The `analyze` tool: molecular properties of a session document or of the
//! atoms a selection covers.
use super::{
    budget::Budgets,
    catalog::arguments,
    error::{ErrorKind, OpError},
    exec::Context,
    ids,
    import::{AnalysisJson, warnings},
    policy::Access,
    result::ToolResult,
    store::Documents,
    wire::{DocHandle, ObjectId, Principal, envelope_json, handle_schema, ids_schema, nullable},
};
use crate::{
    document::Document,
    editing,
    engine::{LocalEngine, Request, Response},
    envelope::{Envelope, ValidationStatus, Versions, Warning},
    tool_spec::{Hints, ToolSpec},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

/// The app's message when the engine returns no analysis for a selection
/// (src/app/inspector.rs); a warning for the whole drawing.
pub const NO_PROPERTIES: &str = "No molecular properties were returned.";

/// A selection that covers no atom.
pub const NO_ATOMS: &str = "Select at least one atom";

pub const ANALYZE: ToolSpec = ToolSpec {
    name: "analyze",
    title: Some("Analyze chemistry"),
    description: "Compute the molecular properties of a session document: SMILES, formula, mass, exact mass, logP, TPSA, hydrogen-bond donors and acceptors, rings, unpaired electrons, InChI and InChIKey. Pass ids (or null for the whole drawing) to analyze part of it, as the app's inspector does: the selected atoms, where any abbreviation member pulls in the whole abbreviation; analyzed_atoms lists them, and the selection must cover at least one atom. Read-only: the document and its revision never change.",
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
            "ids": nullable(ids_schema()),
        },
        "required": ["document", "ids"],
        "additionalProperties": false,
    })
}

/// The `analyze` arguments.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Analyze {
    document: DocHandle,
    #[serde(deserialize_with = "Option::deserialize")]
    ids: Option<Vec<ObjectId>>,
}

pub(crate) fn decode(args: Value, budgets: &Budgets) -> Result<Analyze, OpError> {
    let analyze: Analyze = arguments(args)?;
    if let Some(ids) = &analyze.ids {
        ids::check_count(ids, budgets)?;
    }
    Ok(analyze)
}

/// What one analysis covers.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    /// The atoms analyzed, in document order.
    pub atoms: Vec<u64>,
    /// The drawing sent to the engine.
    pub doc: Document,
}

/// The part of `doc` that `ids` select, or the whole drawing for `None`.
///
/// - The whole drawing goes to the engine as it is, as the app's Analyze
///   command sends it (src/app/engine_jobs.rs `analyze_drawing`).
/// - A selection is first checked with [`ids::resolve`]. Then, as the app's
///   inspector analyzes a selection (src/app/inspector.rs
///   `property_document`), [`editing::analysis_atoms`] picks the atoms (an
///   abbreviation member pulls in the whole abbreviation) and
///   [`editing::analysis_document`] cuts them out without computed labels.
///   A selection without atoms is [`ErrorKind::InvalidArguments`].
#[doc(hidden)]
pub fn part(doc: &Document, ids: Option<&[ObjectId]>) -> Result<Part, OpError> {
    let Some(ids) = ids else {
        return Ok(Part {
            atoms: doc.atoms.iter().map(|atom| atom.id).collect(),
            doc: doc.clone(),
        });
    };
    let selected = ids::resolve(doc, ids)?;
    let atoms = editing::analysis_atoms(doc, &selected);
    if atoms.is_empty() {
        return Err(OpError::new(ErrorKind::InvalidArguments, NO_ATOMS));
    }
    let doc = editing::analysis_document(doc, &atoms);
    Ok(Part { atoms, doc })
}

/// The envelope value of an analysis.
#[derive(Debug, Serialize)]
struct Analyzed {
    analysis: Option<AnalysisJson>,
    analyzed_atoms: Vec<ObjectId>,
}

fn failed(message: impl AsRef<str>) -> OpError {
    OpError::new(ErrorKind::Failed, message)
}

/// The result for an engine `response` to the analysis of `atoms`.
///
/// The engine's warnings become envelope warnings. Without an analysis, a
/// selection fails with [`NO_PROPERTIES`] as the app's inspector does; the
/// whole drawing gets `analysis: null` and that message as a last warning.
fn analyzed(
    response: Response,
    whole: bool,
    atoms: Vec<u64>,
    versions: Versions,
) -> Result<ToolResult, OpError> {
    let Response {
        analysis,
        warnings: messages,
        ..
    } = response;
    let mut warnings = warnings(messages);
    if analysis.is_none() {
        if !whole {
            return Err(failed(NO_PROPERTIES));
        }
        warnings.push(Warning {
            message: NO_PROPERTIES.to_owned(),
        });
    }
    let value = envelope_json(&Envelope {
        value: Analyzed {
            analysis: analysis.map(AnalysisJson::from),
            analyzed_atoms: atoms.into_iter().map(ObjectId).collect(),
        },
        warnings,
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

/// Analyzes the document, or the [`part`] `ids` select, with read access.
///
/// Read-only: unlike `App::analysis_ready` (src/app/engine_jobs.rs), which
/// copies the engine's computed labels back into the open drawing, analyze
/// never changes the session document, so its revision stays the same and
/// its computed-label cache is not refreshed.
///
/// `{value: {analysis, analyzed_atoms}, warnings, validation, versions}`.
/// `analysis` has the fields of [`AnalysisJson`]; `analyzed_atoms` lists the
/// [`Part::atoms`]. An engine error is [`ErrorKind::Failed`] with its message.
pub(crate) async fn analyze(
    ctx: Context,
    store: Arc<dyn Documents>,
    engine: LocalEngine,
    who: Principal,
    versions: Versions,
    analyze: Analyze,
) -> Result<ToolResult, OpError> {
    let Analyze { document, ids } = analyze;
    let whole = ids.is_none();
    let Part { atoms, doc } = ctx
        .blocking(move || {
            let snapshot = store.snapshot(&who, &document, Access::Read)?;
            part(&snapshot.doc, ids.as_deref())
        })
        .await?;
    let response = engine
        .request(Request::molecule("analyze", doc))
        .await
        .map_err(failed);
    ctx.checkpoint()?;
    analyzed(response?, whole, atoms, versions)
}

#[cfg(test)]
mod tests;
