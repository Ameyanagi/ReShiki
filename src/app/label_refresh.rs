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
}

#[cfg(test)]
impl State {
    pub(super) fn pending_key(&self) -> Option<Key> {
        self.pending
    }
}

pub(super) async fn calculate(
    source: Document,
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
                let clean = !self.tab.dirty();
                computed.apply(&mut self.tab.doc);
                settle(&mut self.tab.doc);
                // The settled geometry follows from labels, not from an edit.
                if clean {
                    self.tab.saved.clone_from(&self.tab.doc);
                }
                // An open Move & attach or Coordinate preparation builds its
                // result from these copies. Keep them equal to the drawing, so
                // placing neither reverts the labels nor adds an Undo step.
                if let Some(state) = &mut self.tab.joining {
                    computed.apply(&mut state.prepared.original);
                    settle(&mut state.prepared.original);
                    computed.apply(&mut state.prepared.fragment);
                    computed.apply(&mut state.prepared.base);
                }
                self.tab.label_generation = self.tab.label_generation.wrapping_add(1);
                self.tab.chemistry_notice.clone_from(&computed.notice);
            }
            Err(error) => self.tab.chemistry_notice = Some(error),
        }
    }
}

impl App {
    /// The drawing a finished save wrote, as `labels_ready` would have
    /// settled it if the labels arrived while the save was open. Used only
    /// when that is the drawing now, so the save leaves it saved.
    pub(super) fn settled_save(&self, written: Document) -> Document {
        let mut settled = written.clone();
        self.tab.label_refresh.cache.apply(&mut settled);
        settle(&mut settled);
        if super::same_drawing(&settled, &self.tab.doc) {
            settled
        } else {
            written
        }
    }
}

/// New label ink (an H appearing on OH) moves what keeps clear of it, such
/// as attached arrow ends, as the next edit would. A rejected repair leaves
/// the drawing as it is.
fn settle(doc: &mut Document) {
    let before = doc.clone();
    let _ = reshiki::transaction::reconcile(doc, before);
}

#[cfg(test)]
mod tests;
