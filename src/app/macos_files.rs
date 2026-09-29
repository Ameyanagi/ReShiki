//! Deliver Finder requests without replacing another drawing or an active edit.
use super::{App, Message};
use iced::{Subscription, Task, futures::SinkExt};
use std::{path::PathBuf, sync::Mutex};
use tokio::sync::mpsc::UnboundedReceiver;

static EVENTS: Mutex<Option<UnboundedReceiver<reshiki_macos::OpenRequest>>> = Mutex::new(None);

#[derive(Debug, Clone)]
pub enum Action {
    Open(reshiki_macos::OpenRequest),
    Loaded(PathBuf, Result<String, String>),
    Prepared(super::files::Key, super::files::Opened),
    Launched(Result<(), String>),
}

pub(crate) fn install_document_events(events: UnboundedReceiver<reshiki_macos::OpenRequest>) {
    if let Ok(mut slot) = EVENTS.lock() {
        *slot = Some(events);
    }
}

pub(super) fn subscription() -> Subscription<Message> {
    Subscription::run(|| {
        iced::stream::channel(16, async |mut output| {
            let events = EVENTS.lock().ok().and_then(|mut slot| slot.take());
            if let Some(mut events) = events {
                while let Some(request) = events.recv().await {
                    if output
                        .send(Message::MacFiles(Action::Open(request)))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            }
        })
    })
}

impl App {
    fn can_open_in_startup_window(&self) -> bool {
        self.path.is_none()
            && self.file_epoch == 0
            && self.revision == 0
            && self.doc.all_ids().is_empty()
            && !self.dirty()
            && !self.busy
            && !self.native_opening
            && !self.autosave.closing()
            && !self.updates.restarting
            && self.atom_text.is_none()
            && self.inline_text.is_none()
            && self.pending.is_none()
            && !self.import_open
            && self.styles.editor.is_none()
    }

    pub(super) fn mac_file_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Open(Ok(paths)) => {
                let mut tasks = Vec::new();
                let mut other_windows = Vec::new();
                for path in paths {
                    if self.can_open_in_startup_window() {
                        self.native_opening = true;
                        tasks.push(Task::perform(
                            async move {
                                let contents = tokio::fs::read_to_string(&path)
                                    .await
                                    .map_err(|e| e.to_string());
                                Action::Loaded(path, contents)
                            },
                            Message::MacFiles,
                        ));
                    } else {
                        other_windows.push(path);
                    }
                }
                if !other_windows.is_empty() {
                    tasks.push(launch(other_windows));
                }
                Task::batch(tasks)
            }
            Action::Loaded(path, contents) => {
                self.native_opening = false;
                if self.can_open_in_startup_window() {
                    // Native validation now runs in a second worker stage.
                    // Keep this window reserved until that stage completes.
                    self.native_opening = true;
                    self.update(Message::Opened(Some((path, contents))))
                } else {
                    // A user may have started drawing while the file was read.
                    launch(vec![path])
                }
            }
            Action::Prepared(key, opened) => {
                self.native_opening = false;
                if self.can_open_in_startup_window() && self.file_request_is_current(key) {
                    self.update(Message::FilePrepared(key, opened))
                } else if let Some((path, _)) = opened {
                    // Finder requests still open when an edit or a newer menu
                    // open takes ownership of this window during parsing.
                    launch(vec![path])
                } else {
                    Task::none()
                }
            }
            Action::Open(Err(error)) | Action::Launched(Err(error)) => {
                self.error = true;
                self.status = format!("Could not open document: {error}");
                Task::none()
            }
            Action::Launched(Ok(())) => Task::none(),
        }
    }
}

fn launch(paths: Vec<PathBuf>) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let executable = std::env::current_exe().map_err(|e| e.to_string())?;
                let bundle = executable
                    .ancestors()
                    .nth(3)
                    .filter(|p| p.extension().is_some_and(|s| s == "app"));
                for path in paths {
                    if let Some(bundle) = bundle {
                        let status = std::process::Command::new("/usr/bin/open")
                            .args(["-n", "-a"])
                            .arg(bundle)
                            .args(["--args", "--open"])
                            .arg(path)
                            .status()
                            .map_err(|e| e.to_string())?;
                        if !status.success() {
                            return Err("macOS could not launch another drawing window".into());
                        }
                    } else {
                        let mut child = std::process::Command::new(&executable)
                            .arg("--open")
                            .arg(path)
                            .spawn()
                            .map_err(|e| e.to_string())?;
                        std::thread::Builder::new()
                            .name("document-window".into())
                            .spawn(move || {
                                let _ = child.wait();
                            })
                            .map_err(|e| e.to_string())?;
                    }
                }
                Ok(())
            })
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r)
        },
        |result| Message::MacFiles(Action::Launched(result)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use reshiki::document::{Document, Point};

    #[test]
    fn finder_loads_a_native_drawing_into_the_untouched_startup_window() -> Result<(), String> {
        let (mut app, _) = App::new();
        let mut doc = Document::default();
        doc.add_atom("N", Point::default());
        let path = PathBuf::from("/tmp/日本語 drawing.rsk");
        app.native_opening = true;
        let contents = serde_json::to_string(&doc).map_err(|e| e.to_string())?;
        let task = app.mac_file_action(Action::Loaded(path.clone(), Ok(contents.clone())));
        assert!(task.units() > 0);
        assert!(
            app.path.is_none(),
            "Finder dispatch waits for the parse worker"
        );
        assert!(
            app.native_opening,
            "Reserve the window through native parsing"
        );
        super::super::files::finish_dispatched_open(&mut app, path.clone(), Ok(contents));
        assert!(!app.native_opening);
        assert_eq!(app.path, Some(path));
        assert_eq!(app.doc.atoms.len(), 1);
        assert!(!app.error);
        Ok(())
    }

    #[test]
    fn a_second_finder_request_cannot_take_a_window_reserved_for_parsing() -> Result<(), String> {
        let (mut app, _) = App::new();
        let path = PathBuf::from("/tmp/first.rsk");
        let mut document = Document::default();
        document.add_atom("N", Point::default());
        let contents = serde_json::to_string(&document).map_err(|e| e.to_string())?;
        app.native_opening = true;
        let _ = app.mac_file_action(Action::Loaded(path.clone(), Ok(contents.clone())));
        let key = super::super::files::dispatched_open_key(&app);
        assert!(!app.can_open_in_startup_window());

        // Dropping this task avoids launching a real process in the test.
        let second = app.mac_file_action(Action::Open(Ok(vec!["/tmp/second.rsk".into()])));
        assert!(second.units() > 0);
        assert!(app.native_opening);
        assert!(app.file_request_is_current(key));
        super::super::files::finish_dispatched_open(&mut app, path.clone(), Ok(contents));
        assert_eq!(app.path, Some(path));
        assert_eq!(app.doc.atoms[0].element, "N");
        assert!(!app.native_opening);
        Ok(())
    }

    #[test]
    fn edits_and_newer_open_requests_during_parsing_send_finder_file_to_another_window()
    -> Result<(), String> {
        for edit in [false, true] {
            let (mut app, _) = App::new();
            let path = PathBuf::from("/tmp/finder.rsk");
            let mut document = Document::default();
            document.add_atom("O", Point::default());
            let contents = serde_json::to_string(&document).map_err(|e| e.to_string())?;
            app.native_opening = true;
            let _ = app.mac_file_action(Action::Loaded(path.clone(), Ok(contents.clone())));
            let key = super::super::files::dispatched_open_key(&app);
            if edit {
                let before = app.doc.clone();
                app.doc.add_atom("C", Point::default());
                app.changed(before);
            } else {
                let _ = app.file_request_key();
            }
            let original = app.doc.clone();
            app.status = "Current drawing".into();
            let runtime = tokio::runtime::Runtime::new().unwrap();
            let opened =
                runtime.block_on(super::super::files::prepare_contents(path, Ok(contents)));
            let followup = app.mac_file_action(Action::Prepared(key, opened));
            assert!(
                followup.units() > 0,
                "Dispatch a separate window for the Finder file"
            );
            assert_eq!(app.doc, original);
            assert!(app.path.is_none());
            assert_eq!(app.status, "Current drawing");
            assert!(!app.error);
            assert!(!app.native_opening);
        }
        Ok(())
    }

    #[test]
    fn pending_or_edited_windows_are_never_replaced_by_a_finder_request() {
        let (mut app, _) = App::new();
        assert!(app.can_open_in_startup_window());
        app.native_opening = true;
        assert!(!app.can_open_in_startup_window());
        app.native_opening = false;
        app.doc.add_atom("C", Point::default());
        let original = app.doc.clone();
        let _ = app.mac_file_action(Action::Loaded(
            PathBuf::from("/tmp/other.rsk"),
            Ok("{}".into()),
        ));
        assert_eq!(app.doc, original);
        assert!(app.path.is_none());
    }

    #[test]
    fn finder_completion_preserves_active_drafts_and_closing_windows() -> Result<(), String> {
        for context in ["inline draft", "atom draft", "closing", "restarting"] {
            let (mut app, _) = App::new();
            let path = PathBuf::from("/tmp/finder.rsk");
            let mut document = Document::default();
            document.add_atom("O", Point::default());
            let contents = serde_json::to_string(&document).map_err(|e| e.to_string())?;
            app.native_opening = true;
            let _ = app.update(Message::MacFiles(Action::Loaded(
                path.clone(),
                Ok(contents.clone()),
            )));
            let key = super::super::files::dispatched_open_key(&app);
            let dir = tempfile::tempdir().unwrap();
            match context {
                "inline draft" => {
                    let _ = app.inline_action(super::super::inline_text::Action::Begin(
                        None,
                        Point::default(),
                    ));
                    app.caption = "Uncommitted caption".into();
                }
                "atom draft" => {
                    let id = app.doc.add_atom("N", Point::default());
                    let _ = app.atom_text_action(super::super::atom_text::Action::Begin(Some(id)));
                    let _ =
                        app.atom_text_action(super::super::atom_text::Action::Input("OH".into()));
                }
                "closing" => {
                    app.recovery = Some(reshiki::recovery::Recovery {
                        session: dir.path().join("session.json"),
                    });
                    let _ = app.close_after_recovery(iced::window::Id::unique());
                    assert!(app.autosave.closing());
                }
                "restarting" => app.updates.restarting = true,
                _ => unreachable!(),
            }
            let original = app.doc.clone();
            app.status = "Current drawing".into();
            let runtime = tokio::runtime::Runtime::new().unwrap();
            let opened =
                runtime.block_on(super::super::files::prepare_contents(path, Ok(contents)));
            // Use the public dispatcher to exercise the editor/close gates too.
            let followup = app.update(Message::MacFiles(Action::Prepared(key, opened)));
            assert!(
                followup.units() > 0,
                "Open Finder file separately: {context}"
            );
            assert_eq!(app.doc, original, "{context}");
            assert_eq!(app.inline_text.is_some(), context == "inline draft");
            assert_eq!(app.atom_text.is_some(), context == "atom draft");
            if context == "inline draft" {
                assert_eq!(app.caption, "Uncommitted caption");
            }
            assert!(app.path.is_none());
            assert_eq!(app.status, "Current drawing");
            assert!(!app.native_opening);
        }
        Ok(())
    }

    #[test]
    fn invalid_native_files_report_errors_without_panicking_or_changing_the_drawing() {
        let (mut app, _) = App::new();
        let original = app.doc.clone();
        let path = PathBuf::from("/tmp/bad.rsk");
        let task = app.mac_file_action(Action::Loaded(path.clone(), Ok("not json".into())));
        assert!(task.units() > 0);
        assert_eq!(app.doc, original);
        assert!(!app.error, "Validation has not run on the event loop");
        super::super::files::finish_dispatched_open(&mut app, path, Ok("not json".into()));
        assert_eq!(app.doc, original);
        assert!(app.error);
        assert!(app.path.is_none());
        assert!(!app.native_opening);
    }
}
