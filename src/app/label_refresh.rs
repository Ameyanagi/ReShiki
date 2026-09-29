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
        if !self.labels_dirty
            || self.label_refresh.pending.is_some()
            || self.busy
            || self.cleanup.is_some()
            || self.erase_stroke
        {
            return Task::none();
        }
        self.labels_dirty = false;
        if self.doc.atoms.is_empty() {
            self.label_refresh.cache = Arc::default();
            return Task::none();
        }
        let key = Key {
            revision: self.revision,
            epoch: self.file_epoch,
        };
        self.label_refresh.pending = Some(key);
        let previous = Arc::clone(&self.label_refresh.cache);
        let source = self.doc.clone();
        Task::perform(calculate(source, previous), move |result| {
            Message::LabelsReady(key, result)
        })
    }

    pub(super) fn labels_ready(&mut self, key: Key, result: Result<Arc<Refresh>, String>) {
        if self.label_refresh.pending != Some(key) {
            return;
        }
        self.label_refresh.pending = None;
        if key.epoch != self.file_epoch {
            self.label_refresh.cache = Arc::default();
            return;
        }
        if let Ok(computed) = &result {
            // A stale calculation may seed exact component-cache hits, but it
            // can never publish labels or notices into a newer drawing.
            self.label_refresh.cache = Arc::clone(computed);
        }
        if key.revision != self.revision {
            self.labels_dirty = true;
            return;
        }
        match result {
            Ok(computed) => {
                computed.apply(&mut self.doc);
                self.chemistry_notice.clone_from(&computed.notice);
            }
            Err(error) => self.chemistry_notice = Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Pending, same_drawing};
    use reshiki::document::{Document, Point};

    fn fixture() -> (App, u64) {
        let (mut app, _) = App::new();
        app.doc = Document::default();
        let carbon = app.doc.add_atom("C", Point::default());
        let oxygen = app.doc.add_atom("C", Point::new(42., 0.));
        app.doc.add_bond(carbon, oxygen, 1, "plain");
        app.selected = vec![oxygen];
        (app, oxygen)
    }

    fn compute(app: &App) -> Result<Arc<Refresh>, String> {
        Refresh::calculate(&app.doc, &app.label_refresh.cache).map(Arc::new)
    }

    #[test]
    fn oxygen_starts_immediately_and_labels_do_not_edit_history_or_selection() {
        let (mut app, oxygen) = fixture();
        let task = app.update(Message::ContextKey("o".into()));
        assert!(task.units() > 0, "No timer before starting chemistry");
        assert!(!app.busy, "Labels must not disable editing commands");
        let key = app.label_refresh.pending.unwrap();
        let drawing = app.doc.clone();
        app.saved = drawing.clone();
        let selected = app.selected.clone();
        app.edit(crate::canvas::Edit::ContextMenu {
            position: iced::Point::ORIGIN,
            selected: selected.clone(),
        });
        let labels = compute(&app);
        let _ = app.update(Message::LabelsReady(key, labels));
        assert!(
            app.context_menu.is_some(),
            "Background labels leave menus open"
        );
        assert_eq!(app.doc.atom(oxygen).unwrap().label_h, 1);
        assert_eq!(app.selected, selected);
        assert_eq!(app.revision, key.revision);
        assert!(same_drawing(&drawing, &app.doc));
        assert!(!app.dirty());
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc.atom(oxygen).unwrap().element, "C");
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc.atom(oxygen).unwrap().label_h, 1);
    }

    #[test]
    fn rapid_edits_coalesce_and_stale_success_and_errors_cannot_publish() {
        for failure in [false, true] {
            let (mut app, id) = fixture();
            let _ = app.update(Message::ContextKey("o".into()));
            let old = app.label_refresh.pending.unwrap();
            let result = if failure {
                Err("stale error".into())
            } else {
                compute(&app)
            };
            let _ = app.update(Message::ContextKey("n".into()));
            let _ = app.update(Message::ContextKey("s".into()));
            assert_eq!(app.label_refresh.pending, Some(old));
            let latest = app.revision;
            let _ = app.update(Message::LabelsReady(old, result));
            assert_eq!(app.doc.atom(id).unwrap().element, "S");
            assert_eq!(app.doc.atom(id).unwrap().label_h, 0);
            assert!(app.chemistry_notice.is_none());
            let key = app.label_refresh.pending.unwrap();
            assert_eq!(key.revision, latest);
            let result = compute(&app);
            let _ = app.update(Message::LabelsReady(key, result));
            assert_eq!(app.doc.atom(id).unwrap().label_h, 1);
            assert!(app.label_refresh.pending.is_none());
        }
    }

    #[test]
    fn document_replacement_rejects_old_labels_even_with_reused_ids_and_revision() {
        let (mut app, _) = fixture();
        let _ = app.update(Message::ContextKey("o".into()));
        let old = app.label_refresh.pending.unwrap();
        let result = compute(&app);
        let _ = app.perform(Pending::New);
        let id = app.doc.add_atom("N", Point::default());
        app.revision = old.revision;
        let _ = app.update(Message::LabelsReady(old, result));
        assert_eq!(app.doc.atom(id).unwrap().label_h, 0);
        assert!(app.chemistry_notice.is_none());
    }
}
