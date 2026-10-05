use super::Message;
use iced::Task;
use std::path::PathBuf;

const SAVE: &str = "Save";
const DONT_SAVE: &str = "Don’t Save";

/// Asks with the system dialog whether to save `name` before continuing a
/// pending action; the answer arrives as Save, Discard or Cancel.
pub(super) fn ask_to_save(name: String) -> Task<Message> {
    iced::window::latest().then(move |window| {
        let question = format!("Do you want to save the changes you made to “{name}”?");
        let consequence = "Your changes will be lost if you don’t save them.";
        // macOS alerts show the title in bold above the description; other
        // platforms use the title as the window caption.
        let (title, description) = if cfg!(target_os = "macos") {
            (question, consequence.to_owned())
        } else {
            (
                "Save changes?".to_owned(),
                format!("{question} {consequence}"),
            )
        };
        let dialog = rfd::AsyncMessageDialog::new()
            .set_level(rfd::MessageLevel::Warning)
            .set_title(title)
            .set_description(description)
            .set_buttons(rfd::MessageButtons::YesNoCancelCustom(
                SAVE.into(),
                DONT_SAVE.into(),
                "Cancel".into(),
            ));
        match window {
            Some(id) => iced::window::run(id, move |window| dialog.set_parent(&window).show())
                .then(|answer| Task::perform(answer, save_answer)),
            None => Task::perform(dialog.show(), save_answer),
        }
    })
}

/// Platforms without custom buttons answer Yes/No/Cancel in the same order.
fn save_answer(result: rfd::MessageDialogResult) -> Message {
    use rfd::MessageDialogResult as Answer;
    match result {
        Answer::Yes => Message::Save,
        Answer::No => Message::Discard,
        Answer::Custom(label) if label == SAVE => Message::Save,
        Answer::Custom(label) if label == DONT_SAVE => Message::Discard,
        _ => Message::Cancel,
    }
}

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

#[derive(Default)]
pub(super) struct State {
    pub(super) saving: bool,
    /// The tab whose save is in progress.
    pub(super) saving_tab: Option<super::document_tab::TabId>,
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

fn prepare(path: &std::path::Path, contents: Vec<u8>) -> Result<Prepared, String> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if reshiki::compatibility::is_native_extension(&extension) {
        let mut doc = reshiki::document::Document::from_json(&contents)
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
            "cdx" => "cdx",
            "inchi" => "inchi",
            _ => "smiles",
        };
        Ok(Prepared::Import {
            format,
            contents: super::import::contents(format, contents)?,
        })
    }
}

/// Reading, decoding and native validation all run on a blocking worker, never
/// on the event loop or an async executor's cooperative thread.
pub(super) async fn read(path: PathBuf) -> Opened {
    let source = path.clone();
    let result = tokio::task::spawn_blocking(move || {
        let contents = std::fs::read(&source).map_err(|e| e.to_string())?;
        prepare(&source, contents)
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r);
    Some((path, result))
}

/// Reads files one after another, so they open as tabs in this order.
pub(super) fn open_paths(paths: Vec<PathBuf>) -> Task<Message> {
    paths.into_iter().fold(Task::none(), |task, path| {
        task.chain(Task::perform(read(path), Message::FilePrepared))
    })
}

pub(super) async fn prepare_contents(path: PathBuf, contents: Result<Vec<u8>, String>) -> Opened {
    let source = path.clone();
    let result = tokio::task::spawn_blocking(move || prepare(&source, contents?))
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r);
    Some((path, result))
}

impl super::App {
    pub(super) fn drawing_save_target(&self, save_as: bool) -> (Option<PathBuf>, String) {
        let path = if save_as { None } else { self.tab.path.clone() };
        let name = if self.tab.path.is_some() {
            self.document_name()
        } else {
            format!(
                "{}.{}",
                self.document_name(),
                reshiki::compatibility::NATIVE_EXTENSION
            )
        };
        (path, name)
    }

    pub(super) fn file_saved(
        &mut self,
        epoch: u64,
        snapshot: Box<reshiki::document::Document>,
        result: Result<Option<PathBuf>, String>,
    ) -> iced::Task<super::Message> {
        self.file_io.saving = false;
        // The write still releases the serial save slot, but its dialog and
        // error belong to the document that started it.
        if epoch != self.tab.file_epoch {
            return iced::Task::none();
        }
        if result.is_err() {
            self.cancel_close();
        }
        match result {
            Ok(Some(path)) => {
                // Continue the save dialog's action only when nothing is left unsaved.
                self.tab.saved = *snapshot;
                self.tab.path = Some(path);
                self.tab.untitled_name = None;
                self.status = if self.office_document() {
                    if self.office_host == "LibreOffice" {
                        "Drawing saved for LibreOffice. Return to the document to check the update."
                    } else {
                        "Drawing updated in Office. Save the Office document to keep it."
                    }
                } else {
                    "Document saved"
                }
                .into();
                self.error = false;
                if !self.dirty() {
                    self.clear_recovery();
                    if let Some(action) = self.pending.take() {
                        return self.perform(action);
                    }
                } else if self.pending.is_some() {
                    // Edits made during the write need another answer before
                    // New, Open or Close can continue.
                    return ask_to_save(self.document_name());
                }
            }
            // A cancelled Save As or a failed save also cancels the dialog's action.
            Ok(None) => self.pending = None,
            Err(e) => {
                self.pending = None;
                self.status = e;
                self.error = true;
            }
        }
        iced::Task::none()
    }

    /// Opens a read file in a new tab, or in the tab in front if it is an
    /// unchanged empty Untitled drawing. A file that is already open brings
    /// its tab to the front.
    pub(super) fn file_prepared(&mut self, opened: Opened) -> iced::Task<super::Message> {
        if self.updates.restarting {
            return iced::Task::none();
        }
        let Some((path, result)) = opened else {
            return iced::Task::none();
        };
        match result {
            Err(error) => {
                self.status = error;
                self.error = true;
            }
            Ok(Prepared::Import { format, contents }) => {
                self.open_target(None);
                return self.run(
                    super::Request::import(format, &contents),
                    super::Job::ImportFile,
                );
            }
            Ok(Prepared::Native(doc)) => {
                if !self.open_target(Some(&path)) {
                    return iced::Task::none();
                }
                self.clear_recovery();
                self.tab.file_epoch = self.next_epoch();
                self.tab.doc = *doc;
                self.sync_drawing_defaults();
                self.tab.styles.editor = None;
                self.theme_library.editor = None;
                self.tab.saved = self.tab.doc.clone();
                self.tab.path = Some(path);
                self.tab.untitled_name = None;
                self.tab.history = super::History::default();
                self.tab.revision = self.tab.revision.wrapping_add(1);
                self.tab.analysis = None;
                self.tab.labels_dirty = true;
                self.tab.selected.clear();
                self.tab.pages = super::pages::State::default();
                if self.tab.doc.page_layout.is_some() {
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
/// Opened event.
#[cfg(test)]
pub(super) fn finish_dispatched_open(
    app: &mut super::App,
    path: PathBuf,
    contents: Result<Vec<u8>, String>,
) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let opened = runtime.block_on(prepare_contents(path, contents));
    let _ = app.update(super::Message::FilePrepared(opened));
}

#[cfg(test)]
mod tests;

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
