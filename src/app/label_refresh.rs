//! A single label calculation in flight, with exact snapshot identity checks.
use super::{App, Message};
use iced::Task;
use reshiki::atom_labels::refresh::Refresh;
use reshiki::document::Document;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    revision: u64,
    epoch: u64,
}

#[derive(Default)]
pub(super) struct State {
    pending: Option<Key>,
    cache: Arc<Refresh>,
    /// Labels landed while a preview or job keyed on the revision was open,
    /// so the layout repair waits until it closes.
    unsettled: bool,
    /// The snapshot an in-flight save writes, and that snapshot repaired
    /// for labels that landed meanwhile, which the save marks as saved.
    saving: Option<Arc<Document>>,
    saving_repaired: Option<Document>,
}

#[cfg(test)]
impl State {
    pub(super) fn pending_key(&self) -> Option<Key> {
        self.pending
    }
}

pub(super) async fn calculate(
    source: reshiki::document::Document,
    previous: Arc<Refresh>,
) -> Result<Arc<Refresh>, String> {
    reshiki::task::spawn_blocking(move || Refresh::calculate(&source, &previous))
        .await
        .map_err(|e| format!("Label calculation failed: {e}"))?
        .map(Arc::new)
}

impl App {
    pub(super) fn start_label_refresh(&mut self) -> Task<Message> {
        if !self.tab.labels_dirty
            || self.tab.label_refresh.pending.is_some()
            || self.tab.busy
            || self.tab.cleanup.is_some()
            || self.tab.erase_stroke
        {
            return Task::none();
        }
        self.tab.labels_dirty = false;
        if self.tab.doc.atoms.is_empty() {
            self.tab.label_refresh.cache = Arc::default();
            return Task::none();
        }
        let key = Key {
            revision: self.tab.revision,
            epoch: self.tab.file_epoch,
        };
        self.tab.label_refresh.pending = Some(key);
        let previous = Arc::clone(&self.tab.label_refresh.cache);
        let source = self.tab.doc.clone();
        Task::perform(calculate(source, previous), move |result| {
            Message::LabelsReady(key, result)
        })
    }

    pub(super) fn labels_ready(&mut self, key: Key, result: Result<Arc<Refresh>, String>) {
        if self.tab.label_refresh.pending != Some(key) {
            return;
        }
        self.tab.label_refresh.pending = None;
        if key.epoch != self.tab.file_epoch {
            self.tab.label_refresh.cache = Arc::default();
            return;
        }
        if let Ok(computed) = &result {
            // A stale calculation may seed exact component-cache hits, but it
            // can never publish labels or notices into a newer drawing.
            self.tab.label_refresh.cache = Arc::clone(computed);
        }
        if key.revision != self.tab.revision {
            self.tab.labels_dirty = true;
            return;
        }
        match result {
            Ok(computed) => {
                computed.apply(&mut self.tab.doc);
                // An open Move & attach or Coordinate preparation builds its
                // result from these copies. Keep them equal to the drawing, so
                // placing neither reverts the labels nor adds an Undo step.
                if let Some(state) = &mut self.tab.joining {
                    computed.apply(&mut state.prepared.original);
                    computed.apply(&mut state.prepared.fragment);
                    computed.apply(&mut state.prepared.base);
                }
                self.tab.label_generation = self.tab.label_generation.wrapping_add(1);
                self.tab.chemistry_notice.clone_from(&computed.notice);
                self.settle_label_layout();
            }
            Err(error) => self.tab.chemistry_notice = Some(error),
        }
    }

    /// Runs the edit pipeline's reconcile again after computed labels
    /// changed their ink, so arrow ends clear new hydrogens and linked name
    /// captions clear stereo labels without waiting for the next edit.
    ///
    /// The repair adds no Undo step: Undo still returns to the drawing before
    /// the edit that changed the labels. A saved drawing, or the snapshot an
    /// in-flight save writes, that is the same drawing as the one repaired
    /// gets the same repair, so labels alone never leave a drawing unsaved.
    pub(super) fn settle_label_layout(&mut self) {
        let tab = &self.tab;
        // These keep results for the current revision; their own edit
        // reconciles, and a cancel settles on the next update.
        self.tab.label_refresh.unsettled = tab.joining.is_some()
            || tab.cleanup.is_some()
            || tab.optimization.is_some()
            || tab.busy
            || tab.erase_stroke
            || tab.inline_text.is_some();
        if self.tab.label_refresh.unsettled {
            return;
        }
        let before = self.tab.doc.clone();
        let Ok(reconciled) = reshiki::transaction::reconcile(&mut self.tab.doc, before) else {
            return;
        };
        let before = reconciled.before();
        if self.tab.doc == *before {
            return;
        }
        self.tab.revision = self.tab.revision.wrapping_add(1);
        if super::same_drawing(before, &self.tab.saved) {
            self.tab.mark_saved(self.tab.doc.clone());
        }
        let state = &mut self.tab.label_refresh;
        if let Some(saving) = &state.saving
            && super::same_drawing(before, saving)
        {
            state.saving_repaired = Some(self.tab.doc.clone());
        }
        let ids: std::collections::HashSet<_> = self.tab.doc.all_ids().into_iter().collect();
        self.tab.selected.retain(|id| ids.contains(id));
    }

    /// Settles a layout repair that waited for a preview or job to close.
    pub(super) fn settle_deferred_label_layout(&mut self) {
        if self.tab.label_refresh.unsettled {
            self.settle_label_layout();
        }
    }

    /// Whether a layout repair waits for a preview or job to close.
    pub(super) fn label_layout_waiting(&self) -> bool {
        self.tab.label_refresh.unsettled
    }

    /// Edits reconcile the whole drawing, which settles any waiting repair.
    pub(super) fn label_layout_settled(&mut self) {
        self.tab.label_refresh.unsettled = false;
    }

    /// Remembers the snapshot a save starts writing.
    pub(super) fn saving(&mut self, snapshot: &Arc<Document>) {
        self.tab.label_refresh.saving = Some(Arc::clone(snapshot));
        self.tab.label_refresh.saving_repaired = None;
    }

    /// The drawing a finished save marks as saved: its snapshot, or the
    /// snapshot repaired for labels that landed while it was written.
    pub(super) fn saved_snapshot(&mut self, snapshot: Document) -> Document {
        self.tab.label_refresh.saving = None;
        self.tab
            .label_refresh
            .saving_repaired
            .take()
            .unwrap_or(snapshot)
    }
}

#[cfg(test)]
mod tests;
