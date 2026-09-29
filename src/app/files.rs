use std::path::PathBuf;

/// Keep native file types and filename completion consistent across save dialogs.
pub(super) async fn save_path(title: &str, name: &str, extension: &str) -> Option<PathBuf> {
    let dialog = rfd::AsyncFileDialog::new()
        .set_title(title)
        .set_file_name(name);
    let dialog = if extension == reshiki::compatibility::NATIVE_EXTENSION {
        dialog.add_filter("ReShiki drawing", reshiki::compatibility::NATIVE_EXTENSIONS)
    } else {
        dialog.add_filter(extension, &[extension])
    };
    let file = dialog.save_file().await?;
    let path = reshiki::storage::with_default_extension(file.path(), extension);
    // Some platforms do not append the filter suffix. If completion changes the
    // destination, the native dialog has not confirmed overwriting that file.
    if path != file.path() && path.exists() {
        let replace = rfd::AsyncMessageDialog::new()
            .set_title("Replace existing file?")
            .set_description(format!("{} already exists. Replace it?", path.display()))
            .set_buttons(rfd::MessageButtons::YesNo)
            .show()
            .await;
        if replace != rfd::MessageDialogResult::Yes {
            return None;
        }
    }
    Some(path)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    serial: u64,
    epoch: u64,
    revision: u64,
}

#[derive(Default)]
pub(super) struct State {
    serial: u64,
    pub(super) saving: bool,
}

#[derive(Debug, Clone)]
pub enum Prepared {
    Native(Box<reshiki::document::Document>),
    Import {
        format: &'static str,
        contents: String,
    },
}

pub type Opened = Option<(PathBuf, Result<Prepared, String>)>;

fn prepare(path: &std::path::Path, contents: String) -> Result<Prepared, String> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if reshiki::compatibility::is_native_extension(&extension) {
        let mut doc: reshiki::document::Document =
            serde_json::from_str(&contents).map_err(|e| format!("Could not open document: {e}"))?;
        doc.validate()
            .map_err(|e| format!("Could not open document: {e}"))?;
        doc.version = doc.version.max(15);
        reshiki::atom_labels::clear_computed(&mut doc);
        Ok(Prepared::Native(Box::new(doc)))
    } else {
        let format = match extension.as_str() {
            "mol" => "mol",
            "rxn" => "rxn",
            "rsmi" => "rsmi",
            "cdxml" => "cdxml",
            "inchi" => "inchi",
            _ => "smiles",
        };
        Ok(Prepared::Import { format, contents })
    }
}

/// Reading, decoding and native validation all run on a blocking worker, never
/// on the event loop or an async executor's cooperative thread.
pub(super) async fn read(path: PathBuf) -> Opened {
    let source = path.clone();
    let result = tokio::task::spawn_blocking(move || {
        let contents = std::fs::read_to_string(&source).map_err(|e| e.to_string())?;
        prepare(&source, contents)
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r);
    Some((path, result))
}

pub(super) async fn prepare_contents(path: PathBuf, contents: Result<String, String>) -> Opened {
    let source = path.clone();
    let result = tokio::task::spawn_blocking(move || prepare(&source, contents?))
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r);
    Some((path, result))
}

impl super::App {
    pub(super) fn file_saved(
        &mut self,
        epoch: u64,
        snapshot: Box<reshiki::document::Document>,
        result: Result<Option<PathBuf>, String>,
    ) -> iced::Task<super::Message> {
        self.file_io.saving = false;
        if result.is_err() {
            self.autosave.cancel_close();
        }
        match result {
            Ok(Some(path)) => {
                if epoch != self.file_epoch {
                    self.status = "Previous document saved".into();
                    return iced::Task::none();
                }
                self.saved = *snapshot;
                self.path = Some(path);
                self.untitled_name = None;
                self.status = if self.office_document() {
                    "Drawing updated in Office. Save the Office document to keep it."
                } else {
                    "Document saved"
                }
                .into();
                self.error = false;
                if !self.dirty() {
                    self.clear_recovery();
                }
                if !self.dirty()
                    && let Some(action) = self.pending.take()
                {
                    return self.perform(action);
                }
            }
            Ok(None) => {}
            Err(e) => {
                if epoch == self.file_epoch {
                    self.status = e;
                    self.error = true;
                }
            }
        }
        iced::Task::none()
    }

    pub(super) fn file_request_key(&mut self) -> Key {
        self.file_io.serial = self.file_io.serial.wrapping_add(1);
        Key {
            serial: self.file_io.serial,
            epoch: self.file_epoch,
            revision: self.revision,
        }
    }

    pub(super) fn file_prepared(&mut self, key: Key, opened: Opened) -> iced::Task<super::Message> {
        if self.updates.restarting {
            return iced::Task::none();
        }
        let Some((path, result)) = opened else {
            return iced::Task::none();
        };
        if key.serial != self.file_io.serial || key.epoch != self.file_epoch {
            return iced::Task::none();
        }
        // Opening is deliberately not a global busy state. An intervening edit,
        // text draft or chemistry operation wins over a slow read/parse result.
        if key.revision != self.revision
            || self.inline_text.is_some()
            || self.atom_text.is_some()
            || self.busy
            || self.cleanup.is_some()
            || self.erase_stroke
        {
            self.status = "Drawing changed while the file was opening. Open the file again.".into();
            self.error = true;
            return iced::Task::none();
        }
        match result {
            Err(error) => {
                self.status = error;
                self.error = true;
            }
            Ok(Prepared::Import { format, contents }) => {
                return self.run(
                    super::Request::import(format, &contents),
                    super::Job::ImportFile,
                );
            }
            Ok(Prepared::Native(doc)) => {
                self.clear_recovery();
                self.file_epoch = self.file_epoch.wrapping_add(1);
                self.doc = *doc;
                self.sync_drawing_defaults();
                self.styles.editor = None;
                self.theme_library.editor = None;
                self.saved = self.doc.clone();
                self.path = Some(path);
                self.untitled_name = None;
                self.history = super::History::default();
                self.revision = self.revision.wrapping_add(1);
                self.analysis = None;
                self.labels_dirty = true;
                self.selected.clear();
                self.pages = super::pages::State::default();
                if self.doc.page_layout.is_some() {
                    self.fit_pages(Some(0));
                } else {
                    self.fit();
                }
                self.status = "Document opened".into();
                self.error = false;
            }
        }
        iced::Task::none()
    }
}

/// Drive the real parse worker after a unit test dispatches a legacy/Finder
/// Opened event. The request serial must already have been allocated by dispatch.
#[cfg(test)]
pub(super) fn finish_dispatched_open(
    app: &mut super::App,
    path: PathBuf,
    contents: Result<String, String>,
) {
    assert_ne!(app.file_io.serial, 0, "An open must be dispatched first");
    let key = Key {
        serial: app.file_io.serial,
        epoch: app.file_epoch,
        revision: app.revision,
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let opened = runtime.block_on(prepare_contents(path, contents));
    let _ = app.update(super::Message::FilePrepared(key, opened));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, Message, Pending, inline_text};
    use reshiki::document::{Document, Point};

    fn prepared() -> Opened {
        let mut document = Document::default();
        document.add_atom("O", Point::default());
        Some((
            "oxygen.rsk".into(),
            Ok(Prepared::Native(Box::new(document))),
        ))
    }

    #[test]
    fn slow_open_rejects_intervening_edits_and_inline_drafts() {
        for inline in [false, true] {
            let (mut app, _) = App::new();
            let key = app.file_request_key();
            if inline {
                let _ = app.inline_action(inline_text::Action::Begin(None, Point::default()));
            } else {
                let before = app.doc.clone();
                app.doc.add_atom("N", Point::default());
                app.changed(before);
            }
            let expected = app.doc.clone();
            let _ = app.update(Message::FilePrepared(key, prepared()));
            assert_eq!(app.doc, expected);
            assert_eq!(app.inline_text.is_some(), inline);
            assert!(app.error);
            assert!(app.status.contains("Open the file again"));
            assert!(app.path.is_none());
        }
    }

    #[test]
    fn superseded_open_results_and_errors_leave_new_context_untouched() {
        for replace_document in [false, true] {
            for failed in [false, true] {
                let (mut app, _) = App::new();
                let key = app.file_request_key();
                if replace_document {
                    let _ = app.perform(Pending::New);
                } else {
                    let _ = app.file_request_key();
                }
                app.status = "Current context".into();
                let old_result = if failed {
                    Some(("old.rsk".into(), Err("Old error".into())))
                } else {
                    prepared()
                };
                let _ = app.update(Message::FilePrepared(key, old_result));
                assert_eq!(app.status, "Current context");
                assert!(app.doc.atoms.is_empty());
                assert!(app.path.is_none());
            }
        }
    }

    #[test]
    fn native_worker_rejects_invalid_drawing_and_preserves_import_format() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let (_, invalid) = runtime
            .block_on(prepare_contents("bad.rsk".into(), Ok("not JSON".into())))
            .unwrap();
        assert!(invalid.unwrap_err().contains("Could not open document"));
        let (_, imported) = runtime
            .block_on(prepare_contents(
                "example.CDXML".into(),
                Ok("<CDXML/>".into()),
            ))
            .unwrap();
        assert!(
            matches!(imported.unwrap(), Prepared::Import { format: "cdxml", contents } if contents == "<CDXML/>")
        );
    }

    #[test]
    fn saves_are_serial_and_cancelled_save_releases_pending_slot() {
        let (mut app, _) = App::new();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("drawing.rsk");
        app.path = Some(path.clone());
        let task = app.update(Message::Save);
        assert!(task.units() > 0);
        assert!(app.file_io.saving);
        assert!(
            !path.exists(),
            "dispatch cannot perform filesystem work synchronously"
        );
        let second = app.update(Message::Save);
        assert_eq!(second.units(), 0);
        let _ = app.update(Message::Saved(
            app.file_epoch,
            Box::new(app.doc.clone()),
            Ok(None),
        ));
        assert!(!app.file_io.saving);
        assert!(!path.exists());
    }
}

#[cfg(test)]
mod performance {
    use reshiki::{document::Document, template_library::Library, templates::Anchor};
    use std::{hint::black_box, time::Instant};

    fn measure(name: &str, mut operation: impl FnMut()) {
        for _ in 0..3 {
            operation();
        }
        let mut elapsed = Vec::new();
        for _ in 0..30 {
            let start = Instant::now();
            operation();
            elapsed.push(start.elapsed().as_secs_f64() * 1000.);
        }
        elapsed.sort_by(f64::total_cmp);
        eprintln!(
            "{name}: median {:.3} ms, p95 {:.3} ms",
            elapsed[15], elapsed[28]
        );
    }

    #[test]
    #[ignore = "release-mode filesystem and parsing benchmark"]
    fn file_library_workloads() {
        let drawing: Document =
            serde_json::from_str(include_str!("../../assets/examples/shortcut-examples.rsk"))
                .unwrap();
        let text = serde_json::to_string_pretty(&drawing).unwrap();
        measure("native_parse_validate_1x", || {
            let doc: Document = serde_json::from_str(&text).unwrap();
            doc.validate().unwrap();
            black_box(doc);
        });
        measure("native_serialize_1x", || {
            black_box(serde_json::to_vec_pretty(&drawing).unwrap());
        });
        let mut library = Library::default();
        let mut fragment = Document::default();
        fragment.add_atom("O", Default::default());
        // Construct in linear time so setup does not dominate the benchmark.
        library
            .add("Template 0", "Mine", fragment, Anchor::Auto)
            .unwrap();
        let first = library.templates[0].clone();
        for index in 1..512 {
            let mut template = first.clone();
            template.id = format!("benchmark-{index}");
            template.name = format!("Template {index}");
            library.templates.push(template);
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("templates.json");
        library.save(&path).unwrap();
        measure("library_load_512", || {
            black_box(Library::load(&path).unwrap());
        });
        measure("library_save_checked_512", || {
            library.save_checked(&path, &library).unwrap();
        });
        let bytes = serde_json::to_vec(&library).unwrap();
        measure("library_parse_validate_512", || {
            black_box(Library::from_bytes(&bytes).unwrap());
        });
    }
}
