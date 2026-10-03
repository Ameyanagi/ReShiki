//! An editable, unbound copy of the bundled reference in a document tab.
use super::{App, InspectorTab, Message};
use iced::Task;
use reshiki::{
    document::{Document, Point},
    style::DEFAULT as STYLE,
};

const DRAWING: &str = include_str!("../../assets/examples/shortcut-examples.rsk");
const NAME: &str = "Shortcut examples";

impl App {
    pub(super) fn open_shortcut_examples(&mut self) -> Task<Message> {
        self.help_open = false;
        if let Err(error) = self.load_shortcut_examples() {
            self.status = format!("Could not open shortcut examples: {error}");
            self.error = true;
        }
        Task::none()
    }

    pub(super) fn load_shortcut_examples(&mut self) -> Result<(), String> {
        let existing = self.strip().position(|tab| {
            tab.untitled_name == Some(NAME) && tab.path.is_none() && !self.edited(tab)
        });
        if let Some(index) = existing {
            self.select_tab(index);
            self.error = false;
            return Ok(());
        }
        let mut doc: Document = serde_json::from_str(DRAWING).map_err(|e| e.to_string())?;
        doc.validate()?;
        reshiki::atom_labels::clear_computed(&mut doc);
        self.target_tab();
        self.reset_tab();
        self.tab.doc = doc;
        self.tab.saved = self.tab.doc.clone();
        self.tab.untitled_name = Some(NAME);
        self.sync_drawing_defaults();
        self.inspector_open = false;
        self.inspector_tab = InspectorTab::Properties;
        // Start at the common groups at a readable scale on the ordinary canvas.
        self.tab.camera.center = Point::new(STYLE.world(270.), STYLE.world(210.));
        self.tab.camera.zoom = 0.5;
        self.tab.fit_to_view = false;
        self.status =
            "Shortcut examples · Pan or zoom to browse; double-click to select, then copy".into();
        self.error = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reshiki::document::Point;
    use std::path::PathBuf;

    #[test]
    fn gallery_is_an_unsaved_copy_on_the_normal_canvas() -> Result<(), String> {
        let (mut app, _) = App::new();
        app.load_shortcut_examples()?;
        assert_eq!(app.document_name(), "Shortcut examples");
        assert!(app.tab.path.is_none());
        assert!(!app.dirty());
        assert!(matches!(app.inspector_tab, InspectorTab::Properties));
        assert!(!app.inspector_open);
        assert!(app.tab.pages.fit.is_none());
        assert!(app.tab.doc.page_layout.is_none());
        let doc = app.tab.doc.clone();
        let _ = app.update(Message::Viewport(iced::Size::new(900., 600.)));
        let _ = app.update(Message::Fit);
        let (lo, hi) = app.tab.doc.bounds();
        assert!((hi.x - lo.x) * app.tab.camera.zoom <= 820.1);
        assert!((hi.y - lo.y) * app.tab.camera.zoom <= 520.1);
        assert_eq!(app.tab.doc, doc);
        app.tab.doc.add_atom("C", Point::default());
        assert!(app.dirty());
        let _ = app.update(Message::Discard);
        let _ = app.update(Message::New);
        assert_eq!(app.document_name(), "Untitled");
        Ok(())
    }

    #[test]
    fn opening_and_switching_preserve_working_tabs() {
        let (mut app, _) = App::new();
        let before = app.tab.doc.clone();
        app.tab.doc.add_atom("N", Point::new(30., 40.));
        app.changed(before);
        let original = app.tab.id;
        let drawing = app.tab.doc.clone();
        let epoch = app.tab.file_epoch;
        app.help_open = true;
        let _ = app.update(Message::OpenShortcutExamples);
        let examples = app.tab.id;
        assert_ne!(examples, original);
        assert_eq!(app.document_name(), NAME);
        assert!(!app.help_open);
        assert_eq!(app.tabs.background.len(), 1);
        assert_eq!(app.tabs.background[0].doc, drawing);
        assert_eq!(app.tabs.background[0].file_epoch, epoch);
        assert!(app.tabs.background[0].history.can_undo());
        app.tab.camera.zoom = 0.8;
        app.select_tab(0);
        let _ = app.update(Message::OpenShortcutExamples);
        assert_eq!(app.tab.id, examples);
        assert_eq!(app.tab.camera.zoom, 0.8);
        assert_eq!(app.tabs.background.len(), 1);
        let _ = app.update(Message::OpenShortcutExamples);
        assert_eq!(app.tab.id, examples);
        assert_eq!(app.tabs.background.len(), 1);
    }

    #[test]
    fn edited_examples_are_kept_and_another_copy_opens() {
        let (mut app, _) = App::new();
        let _ = app.update(Message::OpenShortcutExamples);
        let edited = app.tab.id;
        let before = app.tab.doc.clone();
        app.tab.doc.add_atom("N", Point::default());
        app.changed(before);
        let drawing = app.tab.doc.clone();
        let _ = app.update(Message::OpenShortcutExamples);
        assert_ne!(app.tab.id, edited);
        assert!(!app.dirty());
        assert_eq!(app.tabs.background[0].doc, drawing);
        assert!(app.tabs.background[0].edited);
        app.select_tab(0);
        let _ = app.update(Message::OpenShortcutExamples);
        assert_eq!(app.tabs.background.len(), 1);
        assert!(!app.dirty());
    }

    #[test]
    fn saving_examples_requires_a_personal_destination() {
        let (mut app, _) = App::new();
        let _ = app.update(Message::OpenShortcutExamples);
        assert_eq!(
            app.drawing_save_target(false),
            (None, "Shortcut examples.rsk".into())
        );
        let _ = app.update(Message::Save);
        assert!(app.file_io.saving);
        let epoch = app.tab.file_epoch;
        let snapshot = Box::new(app.tab.doc.clone());
        let _ = app.file_saved(epoch, snapshot, Ok(None));
        assert!(app.tab.path.is_none());
        let snapshot = Box::new(app.tab.doc.clone());
        let personal = PathBuf::from("My examples.rsk");
        let _ = app.file_saved(epoch, snapshot, Ok(Some(personal.clone())));
        assert_eq!(app.tab.untitled_name, None);
        assert_eq!(app.drawing_save_target(false).0, Some(personal));
        let saved = app.tab.id;
        let _ = app.update(Message::OpenShortcutExamples);
        assert_ne!(app.tab.id, saved);
        assert!(app.tab.path.is_none());
    }
}
