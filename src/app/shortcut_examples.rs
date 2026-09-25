//! A fresh, editable copy of the bundled reference in its own document window.
use super::{App, InspectorTab, Message};
use iced::Task;
use reshiki::{
    document::{Document, Point},
    style::DEFAULT as STYLE,
};

const DRAWING: &str = include_str!("../../assets/examples/shortcut-examples.rsk");

impl App {
    // Used only by a new process, never to replace the caller's working document.
    pub(super) fn load_shortcut_examples(&mut self) -> Result<(), String> {
        let mut doc: Document = serde_json::from_str(DRAWING).map_err(|e| e.to_string())?;
        doc.validate()?;
        reshiki::atom_labels::clear_computed(&mut doc);
        self.doc = doc;
        self.saved = self.doc.clone();
        self.path = None;
        self.untitled_name = Some("Shortcut examples");
        self.file_epoch = self.file_epoch.wrapping_add(1);
        self.sync_drawing_defaults();
        self.recovered.clear();
        self.inspector_open = false;
        self.inspector_tab = InspectorTab::Properties;
        self.pages = Default::default();
        // Start at the common groups at a readable scale on the ordinary canvas.
        self.camera.center = Point::new(STYLE.world(270.), STYLE.world(210.));
        self.camera.zoom = 0.5;
        self.fit_to_view = false;
        self.status =
            "Shortcut examples · Pan or zoom to browse; double-click a structure to copy it".into();
        Ok(())
    }
}

pub(super) fn open() -> Task<Message> {
    Task::perform(
        async {
            tokio::task::spawn_blocking(launch)
                .await
                .map_err(|e| e.to_string())?
        },
        Message::ShortcutExamplesOpened,
    )
}

fn launch() -> Result<(), String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    // Launch Services gives a bundled macOS application an independent window.
    #[cfg(target_os = "macos")]
    if let Some(bundle) = executable
        .ancestors()
        .nth(3)
        .filter(|p| p.extension().is_some_and(|e| e == "app"))
    {
        let status = std::process::Command::new("/usr/bin/open")
            .args(["-n", "-a"])
            .arg(bundle)
            .args(["--args", "--shortcut-examples"])
            .status()
            .map_err(|e| e.to_string())?;
        return if status.success() {
            Ok(())
        } else {
            Err(format!("The new window could not be launched ({status})"))
        };
    }
    let mut child = std::process::Command::new(executable)
        .arg("--shortcut-examples")
        .spawn()
        .map_err(|e| e.to_string())?;
    std::thread::Builder::new()
        .name("shortcut-examples-window".into())
        .spawn(move || {
            let _ = child.wait();
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use reshiki::document::Point;

    #[test]
    fn gallery_is_an_unsaved_copy_on_the_normal_canvas() -> Result<(), String> {
        let (mut app, _) = App::new();
        app.load_shortcut_examples()?;
        assert_eq!(app.document_name(), "Shortcut examples");
        assert!(app.path.is_none());
        assert!(!app.dirty());
        assert!(matches!(app.inspector_tab, InspectorTab::Properties));
        assert!(!app.inspector_open);
        assert!(app.pages.fit.is_none());
        assert!(app.doc.page_layout.is_none());
        let doc = app.doc.clone();
        let _ = app.update(Message::Viewport(iced::Size::new(900., 600.)));
        let _ = app.update(Message::Fit);
        let (lo, hi) = app.doc.bounds();
        assert!((hi.x - lo.x) * app.camera.zoom <= 820.1);
        assert!((hi.y - lo.y) * app.camera.zoom <= 520.1);
        assert_eq!(app.doc, doc);
        app.doc.add_atom("C", Point::default());
        assert!(app.dirty());
        let _ = app.update(Message::Discard);
        let _ = app.perform(super::super::Pending::New);
        assert_eq!(app.document_name(), "Untitled");
        Ok(())
    }

    #[test]
    fn launch_completion_preserves_current_drawing_and_history() {
        let (mut app, _) = App::new();
        let before = app.doc.clone();
        app.doc.add_atom("N", Point::new(30., 40.));
        app.changed(before);
        let drawing = app.doc.clone();
        let epoch = app.file_epoch;
        for result in [Ok(()), Err("Launch unavailable".into())] {
            let failure = result.is_err();
            app.help_open = true;
            let _ = app.update(Message::ShortcutExamplesOpened(result));
            assert_eq!(app.doc, drawing);
            assert_eq!(app.file_epoch, epoch);
            assert!(app.dirty());
            assert!(app.history.can_undo());
            assert_eq!(app.error, failure);
            assert!(!app.help_open);
        }
    }
}
