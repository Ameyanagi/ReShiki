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
mod tests {
    use super::*;
    use crate::app::same_drawing;
    use reshiki::document::{Document, Point};

    fn fixture() -> (App, u64) {
        let (mut app, _) = App::new();
        app.tab.doc = Document::default();
        let carbon = app.tab.doc.add_atom("C", Point::default());
        let oxygen = app.tab.doc.add_atom("C", Point::new(42., 0.));
        app.tab.doc.add_bond(carbon, oxygen, 1, "plain");
        app.tab.selected = vec![oxygen];
        (app, oxygen)
    }

    fn compute(app: &App) -> Result<Arc<Refresh>, String> {
        Refresh::calculate(&app.tab.doc, &app.tab.label_refresh.cache).map(Arc::new)
    }

    #[test]
    fn background_labels_refresh_without_adding_an_undo_step() {
        use crate::app::tabs::{Action, tests::Front};
        let (mut app, oxygen) = fixture();
        let before = app.tab.doc.clone();
        let _ = app.update(Message::ContextKey("o".into()));
        let (id, key) = (app.tab.id, app.tab.label_refresh.pending.unwrap());
        let result = compute(&app);
        let front = Front::new(&mut app);
        let _ = app.update(Message::Tab(
            id,
            Box::new(Message::LabelsReady(key, result)),
        ));
        front.assert_unchanged(&app);
        let tab = &app.tabs.background[0];
        assert!(tab.label_refresh.pending.is_none());
        assert_eq!(tab.doc.atom(oxygen).unwrap().label_h, 1);
        assert_eq!(tab.revision, key.revision);
        let _ = app.update(Message::Tabs(Action::Select(id)));
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before, "Undo still reverts the original edit");
        assert!(!app.tab.history.can_undo());
    }

    #[test]
    fn stale_background_labels_schedule_their_next_refresh_in_the_same_tab() {
        use crate::app::tabs::tests::Front;
        let (mut app, oxygen) = fixture();
        let _ = app.update(Message::ContextKey("o".into()));
        let (id, key) = (app.tab.id, app.tab.label_refresh.pending.unwrap());
        let result = compute(&app);
        let _ = app.update(Message::ContextKey("n".into()));
        let revision = app.tab.revision;
        let front = Front::new(&mut app);
        let task = app.update(Message::Tab(
            id,
            Box::new(Message::LabelsReady(key, result)),
        ));
        assert!(task.units() > 0);
        front.assert_unchanged(&app);
        assert!(app.tab.label_refresh.pending.is_none());
        let tab = &app.tabs.background[0];
        assert_eq!(tab.doc.atom(oxygen).unwrap().element, "N");
        assert_eq!(tab.doc.atom(oxygen).unwrap().label_h, 0);
        let next = tab.label_refresh.pending.unwrap();
        assert_eq!(next.revision, revision);
        assert_eq!(next.epoch, key.epoch);
        let result = app.in_tab(id, |app| compute(app)).unwrap();
        let _ = app.update(Message::Tab(
            id,
            Box::new(Message::LabelsReady(next, result)),
        ));
        front.assert_unchanged(&app);
        assert_eq!(app.tabs.background[0].doc.atom(oxygen).unwrap().label_h, 2);
        assert!(app.tabs.background[0].label_refresh.pending.is_none());
    }

    #[test]
    fn oxygen_starts_immediately_and_labels_do_not_edit_history_or_selection() {
        let (mut app, oxygen) = fixture();
        let task = app.update(Message::ContextKey("o".into()));
        assert!(task.units() > 0, "No timer before starting chemistry");
        assert!(!app.tab.busy, "Labels must not disable editing commands");
        let key = app.tab.label_refresh.pending.unwrap();
        let drawing = app.tab.doc.clone();
        app.tab.saved = drawing.clone();
        let selected = app.tab.selected.clone();
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
        assert_eq!(app.tab.doc.atom(oxygen).unwrap().label_h, 1);
        assert_eq!(app.tab.selected, selected);
        assert_eq!(app.tab.revision, key.revision);
        assert!(same_drawing(&drawing, &app.tab.doc));
        assert!(!app.dirty());
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc.atom(oxygen).unwrap().element, "C");
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc.atom(oxygen).unwrap().label_h, 1);
    }

    #[test]
    fn rapid_edits_coalesce_and_stale_success_and_errors_cannot_publish() {
        for failure in [false, true] {
            let (mut app, id) = fixture();
            let _ = app.update(Message::ContextKey("o".into()));
            let old = app.tab.label_refresh.pending.unwrap();
            let result = if failure {
                Err("stale error".into())
            } else {
                compute(&app)
            };
            let _ = app.update(Message::ContextKey("n".into()));
            let _ = app.update(Message::ContextKey("s".into()));
            assert_eq!(app.tab.label_refresh.pending, Some(old));
            let latest = app.tab.revision;
            let _ = app.update(Message::LabelsReady(old, result));
            assert_eq!(app.tab.doc.atom(id).unwrap().element, "S");
            assert_eq!(app.tab.doc.atom(id).unwrap().label_h, 0);
            assert!(app.tab.chemistry_notice.is_none());
            let key = app.tab.label_refresh.pending.unwrap();
            assert_eq!(key.revision, latest);
            let result = compute(&app);
            let _ = app.update(Message::LabelsReady(key, result));
            assert_eq!(app.tab.doc.atom(id).unwrap().label_h, 1);
            assert!(app.tab.label_refresh.pending.is_none());
        }
    }

    #[test]
    fn document_replacement_rejects_old_labels_even_with_reused_ids_and_revision() {
        let (mut app, _) = fixture();
        let _ = app.update(Message::ContextKey("o".into()));
        let old = app.tab.label_refresh.pending.unwrap();
        let result = compute(&app);
        let _ = app.update(Message::New);
        let id = app.tab.doc.add_atom("N", Point::default());
        app.tab.revision = old.revision;
        let _ = app.update(Message::LabelsReady(old, result));
        assert_eq!(app.tab.doc.atom(id).unwrap().label_h, 0);
        assert!(app.tab.chemistry_notice.is_none());
    }
}
