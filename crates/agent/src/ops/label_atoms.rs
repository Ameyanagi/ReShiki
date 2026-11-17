//! The `label_atoms` tool: predicted shifts written onto a session document
//! as atom numbers, as one undo step.
//!
//! It is bound to the revision `predict_nmr` returned, so rows never land on
//! atoms that an edit since then removed or renumbered, and it commits
//! through apply's receipt replay and atomic commit.
use super::{
    apply::{self, Prepared},
    budget::Budgets,
    catalog::arguments,
    error::{ErrorKind, OpError},
    exec::Context,
    policy::Effect,
    result::ToolResult,
    store::{Applied, Change, Documents, Edit, IdempotencyKey, Receipt},
    wire::{
        DocHandle, ObjectId, Principal, Revision, described, handle_schema, nullable,
        object_id_schema, revision_schema,
    },
};
use crate::{
    envelope::Versions,
    tool_spec::{Hints, ToolSpec},
    transaction,
};
use reshiki_io::nmr::{self, LabelFormat};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

pub const LABEL_ATOMS: ToolSpec = ToolSpec {
    name: "label_atoms",
    title: Some("Label atoms"),
    description: "Write predict_nmr rows onto a session document as atom numbers, replacing those atoms' numbers, as one undo step.",
    input_schema: schema,
    hints: Some(Hints {
        read_only: false,
        destructive: true,
        idempotent: true,
        open_world: false,
    }),
};

fn schema() -> Value {
    let row = json!({
        "type": "object",
        "properties": {"atom": object_id_schema(), "ppm": nullable(json!({"type": "number"}))},
        "required": ["atom", "ppm"],
        "additionalProperties": false,
    });
    json!({
        "type": "object",
        "properties": {
            "document": handle_schema(),
            "rows": described(json!({"type": "array", "maxItems": Budgets::default().max_ids, "items": row}), "predict_nmr's rows; a null ppm is skipped."),
            "format": described(json!({"type": "string", "enum": ["numbers", "ppm", "both"]}), "#3, 1.280 ppm or #3 · 1.280 ppm."),
            "base_revision": described(revision_schema(), "The revision predict_nmr returned."),
            "idempotency_key": described(nullable(json!({
                "type": "string",
                "maxLength": Budgets::default().max_idempotency_key_bytes,
            })), "As in apply."),
        },
        "required": ["document", "rows", "format", "base_revision", "idempotency_key"],
        "additionalProperties": false,
    })
}

/// One row as sent.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    atom: ObjectId,
    /// Required, as the schema says: an omitted Option is not null here.
    #[serde(deserialize_with = "Option::deserialize")]
    ppm: Option<f64>,
}

/// The `label_atoms` arguments as sent.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    document: DocHandle,
    rows: Vec<Row>,
    format: String,
    base_revision: Revision,
    #[serde(default)]
    idempotency_key: Option<String>,
}

/// The decoded `label_atoms` arguments.
#[derive(Debug)]
pub(crate) struct LabelAtoms {
    document: DocHandle,
    rows: Vec<(u64, Option<f64>)>,
    format: LabelFormat,
    base_revision: Revision,
    key: Option<IdempotencyKey>,
}

/// Decodes the arguments before any work runs: the shape, the idempotency
/// key, the row count, then the format.
pub(crate) fn decode(args: Value, budgets: &Budgets) -> Result<LabelAtoms, OpError> {
    let digest = apply::digest(&args);
    let Arguments {
        document,
        rows,
        format,
        base_revision,
        idempotency_key,
    } = arguments(args)?;
    let key = apply::checked_key(idempotency_key, digest, budgets)?;
    if rows.len() > budgets.max_ids {
        return Err(OpError::new(
            ErrorKind::Budget,
            format!(
                "{} rows were given; at most {} are allowed per request",
                rows.len(),
                budgets.max_ids
            ),
        )
        .param("rows"));
    }
    let format = LabelFormat::from_code(&format)
        .ok_or_else(|| OpError::invalid("format must be numbers, ppm or both").param("format"))?;
    Ok(LabelAtoms {
        document,
        rows: rows.into_iter().map(|row| (row.atom.0, row.ppm)).collect(),
        format,
        base_revision,
        key,
    })
}

/// The envelope value of a labelling.
#[derive(Debug, Serialize)]
struct Outcome<'a> {
    effect: Effect,
    document: &'a DocHandle,
    revision: Revision,
    recorded: bool,
    labeled: &'a [ObjectId],
}

/// The edit at `base_revision`: every row checked, then the numbers written
/// into a copy that replaces the snapshot; a copy equal to it commits
/// `recorded: false`, which still binds the key. An atom that is not in the
/// drawing is [`ErrorKind::UnknownObject`]; any other bad row is
/// [`ErrorKind::InvalidArguments`]. Either changes nothing.
fn edit(
    store: &dyn Documents,
    who: &Principal,
    versions: Versions,
    args: LabelAtoms,
) -> Result<Prepared, OpError> {
    let LabelAtoms {
        document,
        rows,
        format,
        base_revision,
        key,
    } = args;
    let snapshot = apply::current(store, who, &document, Some(base_revision))?;
    if let Some((atom, _)) = rows
        .iter()
        .find(|(atom, _)| snapshot.doc.atom(*atom).is_none())
    {
        return Err(OpError::new(
            ErrorKind::UnknownObject,
            format!("Atom {atom} is not in the drawing"),
        )
        .param("rows"));
    }
    let mut labelled = (*snapshot.doc).clone();
    let labeled = nmr::write_labels(&mut labelled, &rows, format)
        .map_err(|error| OpError::invalid(error).param("rows"))?;
    let labeled: Vec<ObjectId> = labeled.into_iter().map(ObjectId).collect();
    store.fits(who, &document, &snapshot.doc, &labelled)?;
    let receipt: Receipt = Box::new(move |applied: &Applied| {
        ToolResult::valid(
            Outcome {
                effect: Effect::Applied,
                document: &document,
                revision: applied.revision,
                recorded: applied.recorded,
                labeled: &labeled,
            },
            Vec::new(),
            versions,
        )
    });
    // Only atom numbers changed, and write_labels checked them, so the copy
    // replaces the snapshot as written: reconciliation would also normalize
    // unrelated parts of the drawing.
    let mut doc = (*snapshot.doc).clone();
    let reconciled = transaction::replace(&mut doc, labelled);
    Ok(Prepared::Commit(Change {
        base: snapshot.revision,
        edit: Edit::Replace { doc, reconciled },
        key,
        labels: None,
        receipt,
    }))
}

/// Writes the rows' labels onto a session document, at `base_revision` and
/// with edit access, as one history step.
///
/// The successful result contract is in [`super::catalog::RESULTS`].
pub(crate) async fn label_atoms(
    ctx: Context,
    store: Arc<dyn Documents>,
    who: Principal,
    versions: Versions,
    args: LabelAtoms,
) -> Result<ToolResult, OpError> {
    let document = args.document.clone();
    let key = args.key.clone();
    let handle = document.clone();
    apply::commit(ctx, store, who, document, move |store, who| {
        apply::prepare(store, who, &handle, key.as_ref(), || {
            edit(store, who, versions, args)
        })
    })
    .await
}

#[cfg(test)]
mod tests;
