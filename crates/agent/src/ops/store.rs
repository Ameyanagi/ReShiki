//! The documents store: the seam between the operations and where documents
//! live.
//!
//! [`SessionStore`](super::session::SessionStore) implements it over session
//! documents. In P2 an app host implements it over live tabs, with
//! [`Documents::snapshot`] limited to [`Access::Read`]; in P3 its commit
//! proposes the change for review instead of applying it.
//!
//! Every method is synchronous and may block on the store's lock, so
//! operations call them only inside
//! [`Context::blocking`](super::exec::Context::blocking), or
//! [`Context::effect`](super::exec::Context::effect) for mutations.
use super::{
    error::OpError,
    policy::{Access, HandleKind},
    result::ToolResult,
    wire::{DocHandle, Principal, Revision},
};
use crate::{atom_labels::refresh::Refresh, document::Document, transaction::Reconciled};
use std::sync::Arc;

/// Identifies one exact request, so a retry replays its receipt instead of
/// applying the edit again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdempotencyKey {
    /// The caller's key, at most
    /// [`Budgets::max_idempotency_key_bytes`](super::budget::Budgets::max_idempotency_key_bytes)
    /// (128) bytes, checked when the arguments are decoded.
    pub key: String,
    /// A digest of the request's arguments. Reusing `key` with a different
    /// digest is an error.
    pub digest: u64,
}

/// A newly created document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Created {
    pub handle: DocHandle,
    pub revision: Revision,
}

/// A document as of one revision.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub handle: DocHandle,
    pub kind: HandleKind,
    pub doc: Arc<Document>,
    pub revision: Revision,
}

/// What an undo, or with `redo` a redo, would restore.
#[derive(Debug, Clone)]
pub struct StepTarget {
    /// The revision the step applies to.
    pub revision: Revision,
    /// The document now.
    pub current: Arc<Document>,
    /// The document the step restores; `None` when there is nothing to undo
    /// or redo.
    pub target: Option<Document>,
}

/// One live document in [`Documents::list`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub handle: DocHandle,
    pub revision: Revision,
    /// [`objects`](super::budget::objects) in the current document.
    pub objects: usize,
}

/// The mutation a [`Change`] makes.
#[expect(
    clippy::large_enum_variant,
    reason = "one Edit is built per commit and moved into it once"
)]
pub enum Edit {
    /// Replaces the document with `doc`, already reconciled by
    /// [`transaction::reconcile`](crate::transaction::reconcile) against the
    /// snapshot at [`Change::base`]. The commit records one history step.
    Replace {
        doc: Document,
        reconciled: Reconciled,
    },
    /// Undoes, or with `redo` redoes, one history step. `chemistry_changed`
    /// is [`transaction::chemistry_changed`](crate::transaction::chemistry_changed)
    /// from the current document to the [`StepTarget::target`].
    Step { redo: bool, chemistry_changed: bool },
}

/// Builds the result of a commit from what it did. It runs under the store's
/// lock, so it must be cheap: JSON from data computed beforehand.
pub type Receipt = Box<dyn FnOnce(&Applied) -> ToolResult + Send>;

/// One atomic mutation of a document.
pub struct Change {
    /// The revision the change was computed from. Any other current revision
    /// makes the commit [`Stale`](super::error::ErrorKind::Stale).
    pub base: Revision,
    pub edit: Edit,
    /// When set, the commit stores its receipt under this key, and a repeated
    /// commit with the same key and digest returns it unchanged.
    pub key: Option<IdempotencyKey>,
    /// Computed atom labels for the exact post-commit document, applied when
    /// the chemistry changed. `Err` carries the notice of a failed refresh.
    pub labels: Option<Result<Refresh, String>>,
    pub receipt: Receipt,
}

/// What a commit did, as its [`Receipt`] sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    /// The revision after the commit; unchanged when nothing was recorded.
    pub revision: Revision,
    /// History recorded a step; `false` when the document did not change.
    pub recorded: bool,
    /// The chemistry changed, so computed labels were cleared and refreshed.
    pub chemistry_changed: bool,
    /// The label refresh's notice or error.
    pub label_notice: Option<String>,
}

/// Where documents live. Each call resolves `handle` for the principal `who`
/// and checks [`policy::check`](super::policy::check).
///
/// The handle is not a capability: an unknown, expired or foreign handle is
/// the same [`UnknownDocument`](super::error::ErrorKind::UnknownDocument)
/// error.
pub trait Documents: Send + Sync {
    /// Stores `doc` as a new document owned by `who`, within the
    /// principal's document, object, weight and picture budgets.
    fn create(&self, who: &Principal, doc: Document) -> Result<Created, OpError>;

    /// The current document, for `access`.
    fn snapshot(
        &self,
        who: &Principal,
        handle: &DocHandle,
        access: Access,
    ) -> Result<Snapshot, OpError>;

    /// What an undo, or with `redo` a redo, would restore. Needs
    /// [`Access::Edit`].
    fn step_target(
        &self,
        who: &Principal,
        handle: &DocHandle,
        redo: bool,
    ) -> Result<StepTarget, OpError>;

    /// The stored receipt for `key`, if any; the same key with a different
    /// digest is [`InvalidArguments`](super::error::ErrorKind::InvalidArguments).
    /// This is a fast path only: [`Documents::commit`] checks again.
    fn receipt(
        &self,
        who: &Principal,
        handle: &DocHandle,
        key: &IdempotencyKey,
    ) -> Result<Option<Arc<ToolResult>>, OpError>;

    /// Applies `change` atomically and returns its receipt.
    fn commit(
        &self,
        who: &Principal,
        handle: &DocHandle,
        change: Change,
    ) -> Result<Arc<ToolResult>, OpError>;

    /// Discards the document and its history.
    fn close(&self, who: &Principal, handle: &DocHandle) -> Result<(), OpError>;

    /// The principal's live documents, in creation order.
    fn list(&self, who: &Principal) -> Vec<Listed>;
}
