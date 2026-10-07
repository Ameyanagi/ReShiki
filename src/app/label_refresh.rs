//! A single label calculation in flight, with exact snapshot identity checks.
use super::{App, Message};
use iced::Task;
use reshiki::atom_labels::refresh::Refresh;
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

pub(super) async fn calculate(
    source: reshiki::document::Document,
    previous: Arc<Refresh>,
) -> Result<Arc<Refresh>, String> {
    tokio::task::spawn_blocking(move || Refresh::calculate(&source, &previous))
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
                self.tab.chemistry_notice.clone_from(&computed.notice);
            }
            Err(error) => self.tab.chemistry_notice = Some(error),
        }
    }
}

#[cfg(test)]
mod tests;
