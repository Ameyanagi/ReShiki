//! Session documents: process-local drawings created through the operation
//! API, each owned by one principal.
//!
//! # Handles
//!
//! A handle is `doc_` and 32 lowercase hex digits, keyed SipHash of a counter
//! under two random keys. It is opaque but NOT a capability: every call checks
//! the owner with [`policy::check`], and an unknown, expired or foreign handle
//! is the same [`ErrorKind::UnknownDocument`] error. SipHash under
//! [`RandomState`] keys is not a CSPRNG; that is enough while one stdio client
//! owns the process.
//!
//! # Expiry
//!
//! Every call first drops the documents idle for longer than
//! [`Budgets::idle_ttl`]. Expiry is a tool execution error, as MCP's stateful
//! tool guidance asks
//! (<https://modelcontextprotocol.io/specification/2026-07-28/server/tools>).
//!
//! # Commits
//!
//! [`Documents::commit`] runs entirely under the store's lock, in a fixed
//! order: it resolves and rechecks the owner, replays a stored receipt,
//! checks the revision and preflights the budgets, and only then mutates.
//! The mutation cannot fail, so no commit fails half-way.
use super::{
    budget::{Budgets, cost, objects, picture_memory},
    error::{ErrorKind, OpError},
    policy::{self, Access, HandleKind, UNKNOWN_DOCUMENT},
    result::ToolResult,
    store::{
        Applied, Change, Created, Documents, Edit, IdempotencyKey, Listed, Snapshot, StepTarget,
    },
    wire::{DocHandle, Principal, Revision},
};
use crate::{
    atom_labels::{self, refresh::Refresh},
    document::{Document, History},
};
use std::{
    collections::{HashMap, VecDeque},
    hash::{BuildHasher, RandomState},
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::Instant,
};

const STALE: &str = "The drawing changed; inspect it and retry";
const KEY_REUSED: &str = "idempotency_key was already used with different arguments";

fn unknown() -> OpError {
    OpError::new(ErrorKind::UnknownDocument, UNKNOWN_DOCUMENT)
}

fn budget(message: String) -> OpError {
    OpError::new(ErrorKind::Budget, message)
}

/// The session documents of every principal, under one lock.
pub struct SessionStore {
    inner: Mutex<Store>,
    budgets: Budgets,
}

struct Store {
    entries: HashMap<DocHandle, Entry>,
    /// Live handles in creation order.
    order: Vec<DocHandle>,
    keys: (RandomState, RandomState),
    /// Hashed into the next handle.
    counter: u64,
    /// The clock; tests inject their own.
    now: fn() -> Instant,
}

struct Entry {
    owner: Principal,
    doc: Arc<Document>,
    history: History,
    revision: u64,
    last_used: Instant,
    /// `(key, digest, receipt)`, oldest first, at most
    /// [`Budgets::idempotency_receipts`].
    receipts: VecDeque<(String, u64, Arc<ToolResult>)>,
    /// [`cost`] of the document and every undo and redo frame.
    weight: u64,
}

impl Entry {
    /// The document and every history frame.
    fn documents(&self) -> impl Iterator<Item = &Document> {
        std::iter::once(&*self.doc)
            .chain(self.history.undo_frames())
            .chain(self.history.redo_frames())
    }

    fn recount(&self) -> u64 {
        self.documents().map(cost).fold(0, u64::saturating_add)
    }

    /// The receipt stored under `key`, if any.
    fn receipt(&self, key: &IdempotencyKey) -> Result<Option<Arc<ToolResult>>, OpError> {
        match self
            .receipts
            .iter()
            .find(|(stored, _, _)| *stored == key.key)
        {
            None => Ok(None),
            Some((_, digest, receipt)) if *digest == key.digest => Ok(Some(Arc::clone(receipt))),
            Some(_) => Err(OpError::new(ErrorKind::InvalidArguments, KEY_REUSED)),
        }
    }
}

impl Store {
    fn new(now: fn() -> Instant) -> Self {
        Self {
            entries: HashMap::new(),
            order: Vec::new(),
            keys: (RandomState::new(), RandomState::new()),
            counter: 0,
            now,
        }
    }

    /// Drops every document idle for longer than [`Budgets::idle_ttl`].
    fn expire(&mut self, budgets: &Budgets) {
        let now = (self.now)();
        let before = self.entries.len();
        self.entries
            .retain(|_, entry| now.saturating_duration_since(entry.last_used) <= budgets.idle_ttl);
        if self.entries.len() != before {
            let entries = &self.entries;
            self.order.retain(|handle| entries.contains_key(handle));
        }
    }

    /// The principal's document `handle`, checked for `access` and marked as
    /// used.
    fn resolve(
        &mut self,
        who: &Principal,
        handle: &DocHandle,
        access: Access,
    ) -> Result<&mut Entry, OpError> {
        let now = (self.now)();
        let entry = self.entries.get_mut(handle).ok_or_else(unknown)?;
        policy::check(HandleKind::Session, access, entry.owner == *who)?;
        entry.last_used = now;
        Ok(entry)
    }

    /// The principal's documents other than `except`.
    fn owned<'a>(
        &'a self,
        who: &'a Principal,
        except: Option<&'a DocHandle>,
    ) -> impl Iterator<Item = &'a Entry> {
        self.entries
            .iter()
            .filter(move |(handle, entry)| entry.owner == *who && Some(*handle) != except)
            .map(|(_, entry)| entry)
    }

    fn weight(&self, who: &Principal, except: Option<&DocHandle>) -> u64 {
        self.owned(who, except)
            .map(|entry| entry.weight)
            .fold(0, u64::saturating_add)
    }

    /// A fresh handle: `doc_` and 32 lowercase hex digits.
    fn mint(&mut self) -> DocHandle {
        loop {
            let n = self.counter;
            self.counter = self.counter.wrapping_add(1);
            let text = format!(
                "doc_{:016x}{:016x}",
                self.keys.0.hash_one((n, 0_u8)),
                self.keys.1.hash_one((n, 1_u8))
            );
            if let Some(handle) =
                DocHandle::new(text).filter(|handle| !self.entries.contains_key(handle))
            {
                return handle;
            }
        }
    }
}

/// What the read-only part of a commit decided.
enum Decision {
    Replay(Arc<ToolResult>),
    Apply,
}

impl SessionStore {
    pub fn new(budgets: Budgets) -> Self {
        Self::with_clock(budgets, Instant::now)
    }

    fn with_clock(budgets: Budgets, now: fn() -> Instant) -> Self {
        Self {
            inner: Mutex::new(Store::new(now)),
            budgets,
        }
    }

    /// Recovers from poisoning: a commit mutates only after every fallible
    /// step, so a panicking receipt can leave at most its own receipt unset.
    fn lock(&self) -> MutexGuard<'_, Store> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Locks the store and drops expired documents.
    fn store(&self) -> MutexGuard<'_, Store> {
        let mut store = self.lock();
        store.expire(&self.budgets);
        store
    }

    fn check_objects(&self, doc: &Document) -> Result<(), OpError> {
        let count = objects(doc);
        if count > self.budgets.max_objects {
            return Err(budget(format!(
                "The drawing has {count} objects; the limit is {}",
                self.budgets.max_objects
            )));
        }
        Ok(())
    }

    fn check_weight(&self, weight: u64) -> Result<(), OpError> {
        if weight > self.budgets.max_session_weight {
            return Err(budget(format!(
                "Session documents and their undo history would exceed the session weight limit of {}; close documents you no longer need",
                self.budgets.max_session_weight
            )));
        }
        Ok(())
    }

    fn check_pictures<'a>(
        &self,
        docs: impl IntoIterator<Item = &'a Document>,
    ) -> Result<(), OpError> {
        if picture_memory(docs) > self.budgets.max_session_picture_bytes {
            return Err(budget(format!(
                "Embedded pictures would exceed the session limit of {} bytes; close documents you no longer need",
                self.budgets.max_session_picture_bytes
            )));
        }
        Ok(())
    }

    /// Fails unless `doc` may replace the entry's document, with
    /// `frame` pushed as the newest undo frame. Mutates nothing.
    ///
    /// The prospective entry holds `doc`, `frame` and the undo frames, less
    /// the oldest one when the push exceeds the history limit; the redo
    /// frames are dropped.
    fn preflight(
        &self,
        store: &Store,
        who: &Principal,
        handle: &DocHandle,
        entry: &Entry,
        doc: &Document,
        frame: &Document,
    ) -> Result<(), OpError> {
        self.check_objects(doc)?;
        let undo_len = entry.history.undo_frames().len();
        let dropped = usize::from(undo_len.saturating_add(1) > entry.history.limit());
        let kept = || entry.history.undo_frames().skip(dropped);
        let weight = [doc, frame]
            .into_iter()
            .chain(kept())
            .map(cost)
            .fold(store.weight(who, Some(handle)), u64::saturating_add);
        self.check_weight(weight)?;
        self.check_pictures(
            store
                .owned(who, Some(handle))
                .flat_map(Entry::documents)
                .chain([doc, frame])
                .chain(kept()),
        )
    }

    /// Steps (1) to (4) of a commit: resolve, replay, revision and preflight.
    fn decide(
        &self,
        store: &mut Store,
        who: &Principal,
        handle: &DocHandle,
        change: &Change,
    ) -> Result<Decision, OpError> {
        store.resolve(who, handle, Access::Edit)?;
        let store = &*store;
        let entry = store.entries.get(handle).ok_or_else(unknown)?;
        if let Some(key) = &change.key
            && let Some(receipt) = entry.receipt(key)?
        {
            return Ok(Decision::Replay(receipt));
        }
        if entry.revision != change.base.0 {
            return Err(OpError::new(ErrorKind::Stale, STALE));
        }
        // A step only moves frames between the document and its history, so
        // it changes no budget.
        if let Edit::Replace { doc, reconciled } = &change.edit {
            self.preflight(store, who, handle, entry, doc, reconciled.before())?;
        }
        Ok(Decision::Apply)
    }
}

/// Step (5) of a commit: the history mutation and the label refresh. It
/// cannot fail.
fn mutate(entry: &mut Entry, edit: Edit, labels: Option<Result<Refresh, String>>) -> Applied {
    let (mut doc, recorded, chemistry_changed) = match edit {
        Edit::Replace {
            mut doc,
            reconciled,
        } => {
            let committed = reconciled.commit(&mut doc, &mut entry.history, false);
            (doc, committed.recorded, committed.chemistry_changed)
        }
        // As the app's undo and redo (src/app/history.rs).
        Edit::Step {
            redo,
            chemistry_changed,
        } => {
            let mut doc = (*entry.doc).clone();
            let changed = if redo {
                entry.history.redo(&mut doc)
            } else {
                entry.history.undo(&mut doc)
            };
            let chemistry_changed = changed && chemistry_changed;
            if chemistry_changed {
                atom_labels::clear_computed(&mut doc);
            }
            (doc, changed, chemistry_changed)
        }
    };
    let mut label_notice = None;
    if chemistry_changed {
        match labels {
            Some(Ok(refresh)) => {
                refresh.apply(&mut doc);
                label_notice = refresh.notice;
            }
            Some(Err(error)) => label_notice = Some(error),
            None => {}
        }
    }
    if recorded {
        entry.revision = entry.revision.wrapping_add(1);
        entry.doc = Arc::new(doc);
    }
    Applied {
        revision: Revision(entry.revision),
        recorded,
        chemistry_changed,
        label_notice,
    }
}

impl Documents for SessionStore {
    fn create(&self, who: &Principal, doc: Document) -> Result<Created, OpError> {
        let mut store = self.store();
        let max = self.budgets.max_documents;
        if store.owned(who, None).count() >= max {
            return Err(budget(format!(
                "At most {max} session documents can be open; close one with document_close"
            )));
        }
        self.check_objects(&doc)?;
        let weight = cost(&doc);
        self.check_weight(store.weight(who, None).saturating_add(weight))?;
        self.check_pictures(
            store
                .owned(who, None)
                .flat_map(Entry::documents)
                .chain([&doc]),
        )?;
        let handle = store.mint();
        let now = (store.now)();
        store.entries.insert(
            handle.clone(),
            Entry {
                owner: who.clone(),
                doc: Arc::new(doc),
                history: History::with_limit(self.budgets.history_depth),
                revision: 0,
                last_used: now,
                receipts: VecDeque::new(),
                weight,
            },
        );
        store.order.push(handle.clone());
        Ok(Created {
            handle,
            revision: Revision(0),
        })
    }

    fn snapshot(
        &self,
        who: &Principal,
        handle: &DocHandle,
        access: Access,
    ) -> Result<Snapshot, OpError> {
        let mut store = self.store();
        let entry = store.resolve(who, handle, access)?;
        Ok(Snapshot {
            handle: handle.clone(),
            kind: HandleKind::Session,
            doc: Arc::clone(&entry.doc),
            revision: Revision(entry.revision),
        })
    }

    fn step_target(
        &self,
        who: &Principal,
        handle: &DocHandle,
        redo: bool,
    ) -> Result<StepTarget, OpError> {
        let mut store = self.store();
        let entry = store.resolve(who, handle, Access::Edit)?;
        Ok(StepTarget {
            revision: Revision(entry.revision),
            current: Arc::clone(&entry.doc),
            target: entry.history.peek(redo).cloned(),
        })
    }

    fn receipt(
        &self,
        who: &Principal,
        handle: &DocHandle,
        key: &IdempotencyKey,
    ) -> Result<Option<Arc<ToolResult>>, OpError> {
        let mut store = self.store();
        store.resolve(who, handle, Access::Edit)?.receipt(key)
    }

    fn commit(
        &self,
        who: &Principal,
        handle: &DocHandle,
        change: Change,
    ) -> Result<Arc<ToolResult>, OpError> {
        let mut store = self.store();
        // (1) to (4): nothing is mutated until every check has passed.
        if let Decision::Replay(receipt) = self.decide(&mut store, who, handle, &change)? {
            return Ok(receipt);
        }
        let entry = store.entries.get_mut(handle).ok_or_else(unknown)?;
        let Change {
            base: _,
            edit,
            key,
            labels,
            receipt,
        } = change;
        // (5) The mutation, which cannot fail.
        let applied = mutate(entry, edit, labels);
        // (6) The receipt.
        entry.weight = entry.recount();
        let result = Arc::new(receipt(&applied));
        if let Some(key) = key {
            entry
                .receipts
                .push_back((key.key, key.digest, Arc::clone(&result)));
            while entry.receipts.len() > self.budgets.idempotency_receipts {
                entry.receipts.pop_front();
            }
        }
        Ok(result)
    }

    fn close(&self, who: &Principal, handle: &DocHandle) -> Result<(), OpError> {
        let mut store = self.store();
        store.resolve(who, handle, Access::Edit)?;
        store.entries.remove(handle);
        store.order.retain(|live| live != handle);
        Ok(())
    }

    fn list(&self, who: &Principal) -> Vec<Listed> {
        let store = self.store();
        store
            .order
            .iter()
            .filter_map(|handle| {
                let entry = store.entries.get(handle)?;
                (entry.owner == *who).then(|| Listed {
                    handle: handle.clone(),
                    revision: Revision(entry.revision),
                    objects: objects(&entry.doc),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
