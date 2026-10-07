//! The `apply` tool: insert, delete, undo and redo on a session document.
//!
//! Every applied edit is ONE atomic store commit and one history step: an
//! insert as the app's assistant applies a draft (src/app/assistant/drafts.rs),
//! undo and redo as the app steps its history (src/app/history.rs), with the
//! label refresh the app runs afterwards computed beforehand. The app's UI
//! bookkeeping (selection, camera, status, recent molecules, keyboard
//! drawing) has no counterpart.
//!
//! # IDs
//!
//! `deleted` lists the object IDs an edit removed and `inserted` the IDs it
//! added. [`Document::next_id`] reuses a deleted maximum, so the two may share
//! IDs: a shared ID names a removed object before the edit and an inserted
//! one after it.
use super::{
    budget::Budgets,
    catalog::arguments,
    error::{ErrorKind, OpError},
    exec::Context,
    ids,
    import::{rejected, warnings},
    policy::{Access, Effect},
    result::ToolResult,
    store::{Applied, Change, Documents, Edit, IdempotencyKey, Receipt, Snapshot},
    wire::{
        DocHandle, IdRemapJson, ObjectId, Principal, Revision, envelope_json, handle_schema,
        ids_schema, nullable, revision_schema,
    },
};
use crate::{
    atom_labels::{self, refresh::Refresh},
    document::Document,
    envelope::{Envelope, IdRemap, ValidationStatus, Versions},
    tool_spec::{Hints, ToolSpec},
    transaction,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    hash::{DefaultHasher, Hash, Hasher},
    sync::Arc,
};

/// An insert without a source document.
pub const NO_SOURCE: &str = "insert needs a source document";

/// A delete with a source document.
pub const DELETE_SOURCE: &str = "delete takes no source; pass null";

/// A delete without IDs.
pub const NO_IDS: &str = "delete needs at least one object ID in ids";

/// An undo or redo with a source or IDs.
pub const STEP_ARGUMENTS: &str = "undo and redo take no source or ids; pass null for both";

/// A guarded edit without `base_revision`.
pub const NO_BASE: &str =
    "base_revision is required for delete, for insert with ids, and for undo and redo";

pub const APPLY: ToolSpec = ToolSpec {
    name: "apply",
    title: Some("Edit document"),
    description: "Edit a session document as one undo step. edit is insert, delete, undo or redo. insert copies every object of the source session document (made by import or compose, for example) into document: with ids it replaces those objects and puts the copy where they were; with ids null or empty it adds the copy below the drawing. delete removes ids. An abbreviation member in ids pulls in the whole abbreviation. undo and redo step through the document's history, which keeps history_depth undo steps; source and ids must be null for them. base_revision is the document revision you last saw. It is required for delete, for insert with ids, and for undo and redo; a different current revision gives stale and changes nothing. idempotency_key (at most 128 bytes, or null) identifies one exact request: repeating the request with the same key returns the first result instead of editing again, and the same key with different arguments is invalid_arguments; after stale, retry with a new key. Returns the new revision, recorded (false when nothing changed), the inserted IDs, id_remap from source to inserted IDs, and the deleted IDs. Deleted IDs can be reused, so inserted and deleted may share IDs.",
    input_schema: schema,
    hints: Some(Hints {
        read_only: false,
        destructive: true,
        idempotent: false,
        open_world: false,
    }),
};

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "document": handle_schema(),
            "edit": {"type": "string", "enum": ["insert", "delete", "undo", "redo"]},
            "source": nullable(handle_schema()),
            "ids": nullable(ids_schema()),
            "base_revision": nullable(revision_schema()),
            "idempotency_key": nullable(json!({
                "type": "string",
                "maxLength": Budgets::default().max_idempotency_key_bytes,
            })),
        },
        "required": ["document", "edit", "source", "ids", "base_revision", "idempotency_key"],
        "additionalProperties": false,
    })
}

/// An `edit` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Kind {
    Insert,
    Delete,
    Undo,
    Redo,
}

/// The `apply` arguments as sent.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    document: DocHandle,
    edit: Kind,
    #[serde(deserialize_with = "Option::deserialize")]
    source: Option<DocHandle>,
    #[serde(deserialize_with = "Option::deserialize")]
    ids: Option<Vec<ObjectId>>,
    #[serde(deserialize_with = "Option::deserialize")]
    base_revision: Option<Revision>,
    #[serde(deserialize_with = "Option::deserialize")]
    idempotency_key: Option<String>,
}

/// What an apply does.
#[derive(Debug)]
enum Action {
    /// Copies `source` into the document, replacing `ids` when there are any.
    Insert {
        source: DocHandle,
        ids: Vec<ObjectId>,
    },
    /// Deletes `ids`, never empty.
    Delete { ids: Vec<ObjectId> },
    /// Undoes, or with `redo` redoes, one history step.
    Step { redo: bool },
}

/// The decoded `apply` arguments.
#[derive(Debug)]
pub(crate) struct Apply {
    document: DocHandle,
    action: Action,
    base_revision: Option<Revision>,
    key: Option<IdempotencyKey>,
}

fn invalid(message: impl AsRef<str>) -> OpError {
    OpError::new(ErrorKind::InvalidArguments, message)
}

/// The digest of an idempotent request: [`DefaultHasher`] over the canonical
/// arguments, serde_json's compact text, whose object keys are sorted
/// (serde_json without `preserve_order`).
fn digest(args: &Value) -> u64 {
    let mut hasher = DefaultHasher::new();
    args.to_string().hash(&mut hasher);
    hasher.finish()
}

/// Decodes the arguments before any work runs, in a fixed order: the shape,
/// the idempotency key's [`Budgets::max_idempotency_key_bytes`] and the ID
/// count, then the rules between fields:
///
/// - insert needs `source`; its `ids` are the objects to replace.
/// - delete takes no `source` and needs at least one ID.
/// - undo and redo take neither `source` nor `ids`.
/// - `base_revision` is required for delete, for insert with IDs, and for
///   undo and redo. A replacement is guarded by its revision while an addition
///   may land on whatever the drawing is now; a deleted ID can be reused, and
///   a blind retried undo must not undo twice.
pub(crate) fn decode(args: Value, budgets: &Budgets) -> Result<Apply, OpError> {
    let digest = digest(&args);
    let Arguments {
        document,
        edit,
        source,
        ids,
        base_revision,
        idempotency_key,
    } = arguments(args)?;
    let max_key = budgets.max_idempotency_key_bytes;
    if let Some(key) = &idempotency_key
        && key.len() > max_key
    {
        return Err(invalid(format!(
            "idempotency_key is {} bytes; at most {max_key} are allowed",
            key.len()
        )));
    }
    if let Some(ids) = &ids {
        ids::check_count(ids, budgets)?;
    }
    let action = match (edit, source, ids) {
        (Kind::Insert, None, _) => return Err(invalid(NO_SOURCE)),
        (Kind::Insert, Some(source), ids) => Action::Insert {
            source,
            ids: ids.unwrap_or_default(),
        },
        (Kind::Delete, Some(_), _) => return Err(invalid(DELETE_SOURCE)),
        (Kind::Delete, None, Some(ids)) if !ids.is_empty() => Action::Delete { ids },
        (Kind::Delete, None, _) => return Err(invalid(NO_IDS)),
        (Kind::Undo | Kind::Redo, None, None) => Action::Step {
            redo: edit == Kind::Redo,
        },
        (Kind::Undo | Kind::Redo, _, _) => return Err(invalid(STEP_ARGUMENTS)),
    };
    let guarded = match &action {
        Action::Insert { ids, .. } => !ids.is_empty(),
        Action::Delete { .. } | Action::Step { .. } => true,
    };
    if guarded && base_revision.is_none() {
        return Err(invalid(NO_BASE));
    }
    Ok(Apply {
        document,
        action,
        base_revision,
        key: idempotency_key.map(|key| IdempotencyKey { key, digest }),
    })
}

/// The envelope value of an apply.
#[derive(Debug, Serialize)]
struct Outcome<'a> {
    effect: Effect,
    document: &'a DocHandle,
    revision: Revision,
    recorded: bool,
    inserted: &'a [ObjectId],
    id_remap: &'a IdRemapJson,
    deleted: &'a [ObjectId],
}

/// What an apply reports besides the commit's [`Applied`], worked out before
/// the commit so its receipt only encodes it.
struct Report {
    document: DocHandle,
    versions: Versions,
    inserted: Vec<ObjectId>,
    id_remap: IdRemapJson,
    deleted: Vec<ObjectId>,
}

fn object_ids(ids: Vec<u64>) -> Vec<ObjectId> {
    ids.into_iter().map(ObjectId).collect()
}

impl Report {
    /// A report of no inserted or deleted objects, as for undo and redo.
    fn new(document: DocHandle, versions: Versions) -> Self {
        Self {
            document,
            versions,
            inserted: Vec::new(),
            id_remap: IdRemapJson(Vec::new()),
            deleted: Vec::new(),
        }
    }

    /// `{value: {effect: "applied", document, revision, recorded, inserted,
    /// id_remap, deleted}, warnings, validation, versions}`, with the label
    /// refresh's notice as a warning.
    fn result(self, applied: &Applied) -> ToolResult {
        let value = envelope_json(&Envelope {
            value: Outcome {
                effect: Effect::Applied,
                document: &self.document,
                revision: applied.revision,
                recorded: applied.recorded,
                inserted: &self.inserted,
                id_remap: &self.id_remap,
                deleted: &self.deleted,
            },
            warnings: warnings(applied.label_notice.iter().cloned().collect()),
            validation: ValidationStatus::Valid,
            versions: self.versions,
        });
        ToolResult {
            value,
            images: Vec::new(),
            files: Vec::new(),
            is_error: false,
        }
    }

    fn receipt(self) -> Receipt {
        Box::new(move |applied: &Applied| self.result(applied))
    }
}

/// What the stage before the effect decided.
#[expect(
    clippy::large_enum_variant,
    reason = "one Prepared is built per call and moved out once"
)]
enum Prepared {
    /// The result, with no commit: a replayed receipt, a rejection, or an
    /// unkeyed undo or redo with nothing to step.
    Done(ToolResult),
    /// The change the effect commits.
    Commit(Change),
}

/// Fails with [`ErrorKind::Stale`] when `base` is given and differs from
/// `current`.
fn check_base(base: Option<Revision>, current: Revision) -> Result<(), OpError> {
    match base {
        Some(base) if base != current => Err(OpError::new(
            ErrorKind::Stale,
            format!(
                "base_revision {base} is not the current revision {current}; inspect the document and retry with a new idempotency_key"
            ),
        )),
        _ => Ok(()),
    }
}

/// The computed labels of `doc` as a commit leaves it, when the chemistry
/// changed and atoms remain: the commit clears computed labels and then
/// applies these, as the app's label refresh does once the edit is recorded
/// (src/app/label_refresh.rs). Without atoms the app computes nothing.
fn labels(doc: &Document, chemistry_changed: bool) -> Option<Result<Refresh, String>> {
    (chemistry_changed && !doc.atoms.is_empty()).then(|| {
        let mut view = doc.clone();
        atom_labels::clear_computed(&mut view);
        Refresh::calculate(&view, &Refresh::default())
    })
}

/// `base` after [`Document::delete`] of `ids`, and the object IDs that
/// removes, in `base` order. Nothing is deleted for no IDs.
fn without(base: &Document, ids: &[u64]) -> (Document, Vec<u64>) {
    let mut doc = base.clone();
    if ids.is_empty() {
        return (doc, Vec::new());
    }
    doc.delete(ids);
    let kept: HashSet<u64> = doc.object_ids().collect();
    let deleted = base.object_ids().filter(|id| !kept.contains(id)).collect();
    (doc, deleted)
}

/// An edited document before reconciliation, with what the edit inserted and
/// deleted.
struct Edited {
    doc: Document,
    inserted: Vec<u64>,
    id_remap: IdRemap,
    deleted: Vec<u64>,
}

/// Copies `fragment` into `base`, replacing `ids` with their whole
/// abbreviations, as the assistant applies a draft: [`crate::candidate`] with
/// the replacement [`Document::expand_abbreviation_selection`] expands
/// (canvas_tools.rs `replacement`).
///
/// The deleted IDs are worked out before the insert: the candidate deletes
/// first and the insert may reuse the deleted maximum, so a diff afterwards
/// would miss removed IDs that the insert reused. A candidate error is
/// [`ErrorKind::InvalidArguments`].
fn insert(base: &Document, fragment: &Document, ids: &[u64]) -> Result<Edited, OpError> {
    let replace = base.expand_abbreviation_selection(ids);
    let (_, deleted) = without(base, &replace);
    let (doc, inserted) = crate::candidate(base, fragment, &replace).map_err(invalid)?;
    let id_remap = IdRemap {
        pairs: fragment
            .all_ids()
            .into_iter()
            .zip(inserted.clone())
            .collect(),
    };
    Ok(Edited {
        doc,
        inserted,
        id_remap,
        deleted,
    })
}

/// Deletes `ids` as the app deletes a selection
/// (src/app/selection_edits.rs `delete_selection`): [`Document::delete`],
/// which pulls in whole abbreviations.
fn delete(base: &Document, ids: &[u64]) -> Edited {
    let (doc, deleted) = without(base, ids);
    Edited {
        doc,
        inserted: Vec::new(),
        id_remap: IdRemap { pairs: Vec::new() },
        deleted,
    }
}

/// The principal's document `handle` for editing, at `base` when given.
fn current(
    store: &dyn Documents,
    who: &Principal,
    handle: &DocHandle,
    base: Option<Revision>,
) -> Result<Snapshot, OpError> {
    let snapshot = store.snapshot(who, handle, Access::Edit)?;
    check_base(base, snapshot.revision)?;
    Ok(snapshot)
}

/// The stored receipt for `key`, as the result.
fn replay(
    store: &dyn Documents,
    who: &Principal,
    document: &DocHandle,
    key: &IdempotencyKey,
) -> Result<Option<Prepared>, OpError> {
    Ok(store
        .receipt(who, document, key)?
        .map(|receipt| Prepared::Done(Arc::unwrap_or_clone(receipt))))
}

/// Everything before the effect, on the blocking pool: [`edit`] between two
/// receipt lookups for a keyed call.
///
/// Both lookups are fast paths, as the commit checks the key again under its
/// lock. The second one serves a call that ends without a commit: another
/// call with the same key may have committed since the first lookup, and its
/// receipt (or the key's reuse) then wins over what this call made of the
/// changed document, such as [`ErrorKind::Stale`], as it does in the commit.
fn prepare(
    store: &dyn Documents,
    who: &Principal,
    versions: Versions,
    apply: Apply,
) -> Result<Prepared, OpError> {
    let Some(key) = apply.key.clone() else {
        return edit(store, who, versions, apply);
    };
    let document = apply.document.clone();
    if let Some(replayed) = replay(store, who, &document, &key)? {
        return Ok(replayed);
    }
    match edit(store, who, versions, apply) {
        Ok(Prepared::Commit(change)) => Ok(Prepared::Commit(change)),
        outcome => match replay(store, who, &document, &key)? {
            Some(replayed) => Ok(replayed),
            None => outcome,
        },
    }
}

/// The revision and ID checks, the edit, its reconciliation and the label
/// precompute.
fn edit(
    store: &dyn Documents,
    who: &Principal,
    versions: Versions,
    apply: Apply,
) -> Result<Prepared, OpError> {
    let Apply {
        document,
        action,
        base_revision,
        key,
    } = apply;
    let (snapshot, edited) = match action {
        Action::Step { redo } => {
            let step = store.step_target(who, &document, redo)?;
            check_base(base_revision, step.revision)?;
            let report = Report::new(document, versions);
            let (chemistry_changed, labels) = match &step.target {
                // As the app's undo and redo, which clear computed labels
                // when the chemistry changed and refresh them afterwards.
                Some(target) => {
                    let changed = transaction::chemistry_changed(&step.current, target);
                    (changed, labels(target, changed))
                }
                // Nothing to step. A keyed call still commits, which changes
                // nothing but binds its key to this request.
                None if key.is_none() => {
                    return Ok(Prepared::Done(report.result(&Applied {
                        revision: step.revision,
                        recorded: false,
                        chemistry_changed: false,
                        label_notice: None,
                    })));
                }
                None => (false, None),
            };
            return Ok(Prepared::Commit(Change {
                base: step.revision,
                edit: Edit::Step {
                    redo,
                    chemistry_changed,
                },
                key,
                labels,
                receipt: report.receipt(),
            }));
        }
        Action::Insert { source, ids } => {
            let snapshot = current(store, who, &document, base_revision)?;
            let ids = ids::resolve(&snapshot.doc, &ids)?;
            let fragment = store.snapshot(who, &source, Access::Read)?.doc;
            let edited = insert(&snapshot.doc, &fragment, &ids)?;
            (snapshot, edited)
        }
        Action::Delete { ids } => {
            let snapshot = current(store, who, &document, base_revision)?;
            let ids = ids::resolve(&snapshot.doc, &ids)?;
            let edited = delete(&snapshot.doc, &ids);
            (snapshot, edited)
        }
    };
    let Edited {
        mut doc,
        inserted,
        id_remap,
        deleted,
    } = edited;
    let reconciled = match transaction::reconcile(&mut doc, (*snapshot.doc).clone()) {
        Ok(reconciled) => reconciled,
        Err(rejection) => return Ok(Prepared::Done(rejected(rejection, Vec::new(), &versions))),
    };
    // The inputs `Reconciled::commit` decides with.
    let chemistry_changed = transaction::chemistry_changed(reconciled.before(), &doc);
    let labels = labels(&doc, chemistry_changed);
    let report = Report {
        document,
        versions,
        inserted: object_ids(inserted),
        id_remap: IdRemapJson::from(&id_remap),
        deleted: object_ids(deleted),
    };
    Ok(Prepared::Commit(Change {
        base: snapshot.revision,
        edit: Edit::Replace { doc, reconciled },
        key,
        labels,
        receipt: report.receipt(),
    }))
}

/// Applies one edit to a session document.
///
/// - Insert and delete: [`insert`] or [`delete`] on an edit snapshot,
///   [`transaction::reconcile`] against it, and the label precompute, all on
///   the blocking pool. A rejection is an `is_error` result with validation
///   `rejected` and no commit.
/// - Undo and redo: the store's step target, checked against
///   `base_revision`; nothing to step gives `recorded: false`, without a
///   commit unless the call is keyed.
/// - A keyed call looks for its receipt before this stage and, when it ends
///   without a commit, again after it (see [`prepare`]).
/// - The store's commit is the single effect: one history step, with the
///   revision, owner and idempotency key checked again under its lock.
///   Nothing runs after it. A cancel that arrives after it ends the call as
///   [`ErrorKind::Cancelled`] (no response), and a retry with the same key
///   replays the stored receipt.
///
/// `{value: {effect: "applied", document, revision, recorded, inserted,
/// id_remap: [{source, inserted}], deleted}, warnings, validation,
/// versions}`. `inserted` and `deleted` may share IDs (see the module docs);
/// both are empty for undo and redo.
pub(crate) async fn apply(
    mut ctx: Context,
    store: Arc<dyn Documents>,
    who: Principal,
    versions: Versions,
    apply: Apply,
) -> Result<ToolResult, OpError> {
    let document = apply.document.clone();
    let prepared = {
        let (store, who) = (Arc::clone(&store), who.clone());
        ctx.blocking(move || prepare(store.as_ref(), &who, versions, apply))
            .await?
    };
    let change = match prepared {
        Prepared::Done(result) => return Ok(result),
        Prepared::Commit(change) => change,
    };
    let receipt = ctx
        .effect(move || store.commit(&who, &document, change))
        .await?;
    Ok(Arc::unwrap_or_clone(receipt))
}

#[cfg(test)]
mod tests;
