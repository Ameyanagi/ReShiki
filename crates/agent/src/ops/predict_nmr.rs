//! The `predict_nmr` tool: offline ¹H or ¹³C shifts for one molecule of a
//! session document.
use super::{
    budget::{Budgets, MAX_IDS},
    catalog::arguments,
    error::{ErrorKind, OpError},
    exec::Context,
    ids,
    policy::Access,
    result::ToolResult,
    store::Documents,
    wire::{
        DocHandle, ObjectId, Principal, Revision, described, handle_schema, nullable,
        object_id_schema,
    },
};
use crate::{
    envelope::Versions,
    tool_spec::{Hints, ToolSpec},
};
use reshiki_io::nmr::{self, Nucleus, Shifts};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

pub const PREDICT_NMR: ToolSpec = ToolSpec {
    name: "predict_nmr",
    title: Some("Predict NMR shifts"),
    description: "Predict ¹H or ¹³C NMR chemical shifts offline for one molecule of a session document, as the app's Predict NMR does. Cite the returned credit with published values.",
    input_schema: schema,
    hints: Some(Hints {
        read_only: true,
        destructive: false,
        idempotent: true,
        open_world: false,
    }),
};

fn schema() -> Value {
    // ids_schema() with at least one ID.
    let ids =
        json!({"type": "array", "minItems": 1, "maxItems": MAX_IDS, "items": object_id_schema()});
    json!({
        "type": "object",
        "properties": {
            "document": handle_schema(),
            "nucleus": {"type": "string", "enum": ["1H", "13C"]},
            "ids": described(nullable(ids), "null for the whole drawing, which must be one molecule; otherwise atoms of one molecule, which is predicted whole."),
        },
        "required": ["document", "nucleus", "ids"],
        "additionalProperties": false,
    })
}

/// The `predict_nmr` arguments.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    document: DocHandle,
    nucleus: String,
    #[serde(default)]
    ids: Option<Vec<ObjectId>>,
}

/// Decoded `predict_nmr` arguments.
#[derive(Debug)]
pub(crate) struct PredictNmr {
    document: DocHandle,
    nucleus: Nucleus,
    ids: Option<Vec<ObjectId>>,
}

pub(crate) fn decode(args: Value, budgets: &Budgets) -> Result<PredictNmr, OpError> {
    let Arguments {
        document,
        nucleus,
        ids,
    } = arguments(args)?;
    let nucleus = Nucleus::from_code(&nucleus).ok_or_else(|| {
        OpError::new(ErrorKind::InvalidArguments, "nucleus must be 1H or 13C").param("nucleus")
    })?;
    if let Some(ids) = &ids {
        if ids.is_empty() {
            return Err(OpError::new(
                ErrorKind::InvalidArguments,
                "ids must list at least one atom, or be null for the whole drawing",
            )
            .param("ids"));
        }
        ids::check_count(ids, budgets)?;
    }
    Ok(PredictNmr {
        document,
        nucleus,
        ids,
    })
}

/// The envelope value: the prediction and the revision it was made from.
#[derive(Debug, Serialize)]
struct Predicted {
    revision: Revision,
    #[serde(flatten)]
    shifts: Shifts,
}

/// Predicts shifts for the molecule `ids` select, or the whole drawing, with
/// read access. Unknown IDs are [`ErrorKind::UnknownObject`]; a drawing or
/// selection that is not one molecule, unsupported chemistry and a molecule
/// without sites for the nucleus fail. The document never changes.
///
/// The successful result contract is in [`super::catalog::RESULTS`].
pub(crate) async fn predict_nmr(
    ctx: Context,
    store: Arc<dyn Documents>,
    who: Principal,
    versions: Versions,
    args: PredictNmr,
) -> Result<ToolResult, OpError> {
    let PredictNmr {
        document,
        nucleus,
        ids,
    } = args;
    let predicted = ctx
        .blocking(move || {
            let snapshot = store.snapshot(&who, &document, Access::Read)?;
            let ids = ids
                .map(|ids| ids::resolve(&snapshot.doc, &ids))
                .transpose()?;
            let shifts = nmr::predict_drawing(&snapshot.doc, ids.as_deref(), nucleus)
                .map_err(OpError::failed)?;
            Ok(Predicted {
                revision: snapshot.revision,
                shifts,
            })
        })
        .await?;
    Ok(ToolResult::valid(predicted, Vec::new(), versions))
}

#[cfg(test)]
mod tests;
