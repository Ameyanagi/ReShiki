use super::{Job, Message, Pending};
use crate::session::Host;
use iced::Task;
use reshiki::engine::Request;
use std::path::PathBuf;

const SAVE: &str = "Save";
const DONT_SAVE: &str = "Don’t Save";
const ADDIN_SAVE_STATUS: &str =
    "Saving recovery draft and waiting for Microsoft 365 to confirm the drawing update…";
const OFFICE_SESSION_ENDED: &str =
    "The Office edit session has ended; save this drawing to another file";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SaveBackend {
    Addin,
    OfficeOle,
    LibreOfficeRetry,
    Atomic,
}

/// Native Office backends on Windows; other platforms use atomic writes
/// for every host except the Microsoft 365 add-in.
pub(super) fn save_backend(host: Option<Host>) -> SaveBackend {
    match host {
        Some(Host::Microsoft365) => SaveBackend::Addin,
        Some(Host::Office) => SaveBackend::OfficeOle,
        Some(Host::LibreOffice) => SaveBackend::LibreOfficeRetry,
        None => SaveBackend::Atomic,
    }
}

/// Asks with the system dialog whether to save `name` before continuing a
/// pending action; the answer arrives as Save, Discard or Cancel. The browser
/// asks in the app instead (prompt.rs).
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
            // Browser dialogs have no parent window to attach to.
            #[cfg(not(target_arch = "wasm32"))]
            Some(id) => iced::window::run(id, move |window| dialog.set_parent(&window).show())
                .then(|answer| Task::perform(answer, save_answer)),
            _ => Task::perform(dialog.show(), save_answer),
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
#[cfg(not(target_arch = "wasm32"))]
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

/// The browser downloads saved files under the suggested name; see [`write`].
#[cfg(target_arch = "wasm32")]
pub(super) async fn save_path(_title: &str, name: &str, extension: &str) -> Option<PathBuf> {
    Some(reshiki::storage::with_default_extension(
        std::path::Path::new(name),
        extension,
    ))
}

/// Write a saved file atomically in place.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn write(path: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    reshiki::storage::write_atomic(path, bytes)
}

/// The browser has no files to write; a save downloads under the file's name.
#[cfg(target_arch = "wasm32")]
pub(super) fn write(path: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    crate::web::download(path, bytes)
}

/// A chosen or dropped file: its path on the desktop, or just its name in
/// the browser, which has no paths, and its contents.
#[derive(Clone)]
pub struct Picked {
    pub path: PathBuf,
    pub contents: Vec<u8>,
}

impl std::fmt::Debug for Picked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({} bytes)", self.path.display(), self.contents.len())
    }
}

/// How many bytes of a file to read. A loader with a size limit refuses a file
/// read to one byte past it, so a huge file is never read whole.
pub(super) type Limit = fn(&std::path::Path) -> u64;

/// How much of a `size`-byte file to read under `limit`: all of it, or one
/// byte past the limit, which is enough for the loader to refuse it.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub(crate) fn read_length(size: u64, limit: u64) -> u64 {
    size.min(limit.saturating_add(1))
}

/// No limit, for loaders that take a file of any size.
pub(super) fn unlimited(_: &std::path::Path) -> u64 {
    u64::MAX
}

/// Files the user chooses, read up to `limit`. An empty list when they
/// cancel. `filter` names a kind of file and its extensions; the dialog then
/// offers just those.
#[cfg(not(target_arch = "wasm32"))]
pub(super) async fn pick(
    title: &str,
    filter: Option<(&str, &[&str])>,
    multiple: bool,
    limit: Limit,
) -> Result<Vec<Picked>, String> {
    let mut dialog = rfd::AsyncFileDialog::new().set_title(title);
    if let Some((name, extensions)) = filter {
        dialog = dialog.add_filter(name, extensions);
    }
    let files = if multiple {
        dialog.pick_files().await
    } else {
        dialog.pick_file().await.map(|file| vec![file])
    };
    let paths = files
        .unwrap_or_default()
        .iter()
        .map(|file| file.path().to_path_buf())
        .collect();
    read_paths(paths, limit).await
}

/// The browser's own file chooser, with no dialog of its own; see
/// [`crate::web::pick`]. Its files are already in memory.
#[cfg(target_arch = "wasm32")]
pub(super) async fn pick(
    _title: &str,
    filter: Option<(&str, &[&str])>,
    multiple: bool,
    limit: Limit,
) -> Result<Vec<Picked>, String> {
    let extensions = filter.map_or(&[][..], |(_, extensions)| extensions);
    Ok(browser_files(
        crate::web::pick(extensions, multiple, limit).await?,
    ))
}

/// Browser files, by name with their contents.
#[cfg(target_arch = "wasm32")]
pub(super) fn browser_files(files: Vec<(String, Vec<u8>)>) -> Vec<Picked> {
    files
        .into_iter()
        .map(|(name, contents)| Picked {
            path: name.into(),
            contents,
        })
        .collect()
}

/// Read files up to `limit` on a blocking worker.
pub(super) async fn read_paths(paths: Vec<PathBuf>, limit: Limit) -> Result<Vec<Picked>, String> {
    use std::io::Read;
    reshiki::task::spawn_blocking(move || {
        paths
            .into_iter()
            .map(|path| {
                let mut contents = Vec::new();
                std::fs::File::open(&path)
                    .and_then(|file| {
                        file.take(limit(&path).saturating_add(1))
                            .read_to_end(&mut contents)
                    })
                    .map_err(|e| {
                        let name = path.file_name().unwrap_or(path.as_os_str());
                        format!("Could not read {}: {e}", name.to_string_lossy())
                    })?;
                Ok(Picked { path, contents })
            })
            .collect()
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Save bytes through the native dialog, preserving cancellation and write errors.
pub(super) async fn save_bytes(
    title: &str,
    suggested_name: &str,
    extension: &str,
    bytes: &[u8],
) -> Result<Option<PathBuf>, String> {
    let Some(path) = save_path(title, suggested_name, extension).await else {
        return Ok(None);
    };
    write(&path, bytes)?;
    Ok(Some(path))
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
        let doc = reshiki::document::Document::from_native_file(&contents)
            .map_err(|e| format!("Could not open document: {e}"))?;
        Ok(Prepared::Native(Box::new(doc)))
    } else {
        let format = reshiki::compatibility::file_format(path).unwrap_or("smiles");
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
    let result = reshiki::task::spawn_blocking(move || {
        let contents = std::fs::read(&source).map_err(|e| e.to_string())?;
        prepare(&source, contents)
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r);
    Some((path, result))
}

/// Opens the files the user chooses, each in a tab.
pub(super) fn open_dialog() -> Task<Message> {
    // Accept supported extensions without relying on macOS type
    // registration; the worker validates the content.
    Task::future(pick(
        "Open a ReShiki, MOL, RXN, CDXML, CDX, SMILES, CIF, PDB, POSCAR or XYZ file",
        None,
        false,
        unlimited,
    ))
    .then(open_picked)
}

/// Opens read files as tabs in this order, or reports why they could not be read.
pub(super) fn open_picked(files: Result<Vec<Picked>, String>) -> Task<Message> {
    match files {
        Ok(files) => files.into_iter().fold(Task::none(), |task, file| {
            task.chain(Task::perform(
                prepare_contents(file.path, Ok(file.contents)),
                Message::FilePrepared,
            ))
        }),
        Err(error) => Task::done(Message::FilePrepared(Some((PathBuf::new(), Err(error))))),
    }
}

/// Reads files one after another, so they open as tabs in this order.
pub(super) fn open_paths(paths: Vec<PathBuf>) -> Task<Message> {
    paths.into_iter().fold(Task::none(), |task, path| {
        task.chain(Task::perform(read(path), Message::FilePrepared))
    })
}

pub(super) async fn prepare_contents(path: PathBuf, contents: Result<Vec<u8>, String>) -> Opened {
    let source = path.clone();
    let result = reshiki::task::spawn_blocking(move || prepare(&source, contents?))
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r);
    Some((path, result))
}

pub(super) fn open_contents(file: Option<(PathBuf, Result<Vec<u8>, String>)>) -> Task<Message> {
    if let Some((path, contents)) = file {
        return Task::perform(prepare_contents(path, contents), Message::FilePrepared);
    }
    Task::none()
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
        if !matches!(&result, Ok(Some(_))) {
            self.office.closing.retain(|id| *id != self.tab.id);
        }
        if result.is_err() {
            self.cancel_close();
        }
        match result {
            Ok(Some(path)) => {
                // Continue the save dialog's action only when nothing is left unsaved.
                self.tab.saved = self.settled_save(*snapshot);
                if self
                    .tab
                    .office
                    .as_ref()
                    .is_some_and(|binding| !binding.same_path(&path))
                {
                    self.detach_office();
                }
                self.tab.path = Some(path);
                self.tab.untitled_name = None;
                self.status = if self.office_document() {
                    if self
                        .tab
                        .office
                        .as_ref()
                        .is_some_and(|binding| binding.host == Host::LibreOffice)
                    {
                        "Drawing saved for LibreOffice. Return to the document to check the update."
                    } else {
                        "Drawing updated in Office. Save the Office document to keep it."
                    }
                    .into()
                } else if cfg!(target_arch = "wasm32") {
                    // The browser only starts a download, which can still be cancelled.
                    format!("Downloaded {}", self.document_name())
                } else {
                    "Document saved".into()
                };
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
                let task = self.run(
                    super::Request::import(format, &contents),
                    super::Job::ImportFile,
                );
                #[cfg(any(windows, target_os = "linux"))]
                self.desktop_import_started();
                return task;
            }
            Ok(Prepared::Native(doc)) => {
                if self.open_target(Some(&path)) {
                    self.show_opened(*doc, Some(path));
                }
            }
        }
        iced::Task::none()
    }

    /// Show `doc`, read from `path`, in the target tab with a fresh history.
    pub(super) fn show_opened(&mut self, doc: reshiki::document::Document, path: Option<PathBuf>) {
        self.clear_recovery();
        self.begin_file_epoch();
        self.tab.doc = doc;
        self.sync_drawing_defaults();
        self.tab.styles.editor = None;
        self.theme_library.editor = None;
        self.tab.saved = self.tab.doc.clone();
        self.tab.path = path;
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

    pub(super) fn save_drawing(&mut self, save_as: bool) -> Task<Message> {
        if !save_as {
            self.front_pending();
        }
        let office = self.tab.office.clone();
        let office_host = office
            .as_ref()
            .filter(|_| self.office_document() && !save_as)
            .map(|binding| binding.host);
        let (path, suggested_name) = self.drawing_save_target(save_as);
        if self.file_io.saving {
            // Only this tab's own save can go on with the dialog's action.
            if self.file_io.saving_tab != Some(self.tab.id) {
                self.pending = None;
            }
            self.status = "A document save is already in progress".into();
            return Task::none();
        }
        self.file_io.saving = true;
        self.file_io.saving_tab = Some(self.tab.id);
        if office_host == Some(Host::Microsoft365) {
            self.status = ADDIN_SAVE_STATUS.into();
            self.error = false;
        }
        let snapshot = std::sync::Arc::new(self.tab.doc.clone());
        let save_snapshot = std::sync::Arc::clone(&snapshot);
        let epoch = self.tab.file_epoch;
        Task::perform(
            async move {
                let path = if let Some(p) = path {
                    p
                } else {
                    let extension = reshiki::compatibility::NATIVE_EXTENSION;
                    let Some(path) = save_path("Save drawing", &suggested_name, extension).await
                    else {
                        return Ok(None);
                    };
                    path
                };
                // A Save As spelling that names the same host file still uses
                // its original authorized path and acknowledgement protocol.
                let path = office
                    .as_ref()
                    .filter(|binding| binding.same_path(&path))
                    .map_or(path.clone(), |binding| binding.path.clone());
                let save_path = path.clone();
                reshiki::task::spawn_blocking(move || {
                    let bytes = save_snapshot.file_json()?;
                    #[cfg(any(windows, target_os = "linux"))]
                    let _target_owner = if office.as_ref().is_some_and(|binding| {
                        binding.lease.is_some() && binding.same_path(&save_path)
                    }) {
                        None // The GUI owns this lease's fence, including this clone.
                    } else {
                        Some(crate::desktop::write_target(&save_path)?)
                    };
                    let office_host = if let Some(binding) = office
                        .as_ref()
                        .filter(|binding| binding.same_path(&save_path))
                    {
                        if !binding.writable() {
                            return Err(OFFICE_SESSION_ENDED.to_owned());
                        }
                        Some(binding.host)
                    } else {
                        None
                    };
                    if save_backend(office_host) == SaveBackend::Addin {
                        reshiki::office_addin::save(&save_path, &bytes)?;
                    } else {
                        #[cfg(windows)]
                        super::windows_libreoffice_save::save(&save_path, &bytes, office_host)?;
                        #[cfg(not(windows))]
                        write(&save_path, &bytes)?;
                    }
                    Ok::<_, String>(())
                })
                .await
                .map_err(|error| error.to_string())??;
                Ok(Some(path))
            },
            move |result| {
                Message::Saved(
                    epoch,
                    Box::new(std::sync::Arc::unwrap_or_clone(snapshot)),
                    result,
                )
            },
        )
    }

    pub(super) fn discard_pending(&mut self) -> Task<Message> {
        self.front_pending();
        match self.pending.take() {
            Some(Pending::CloseTab(id)) if id == self.tab.id => {
                return self.close_active_tab();
            }
            Some(Pending::CloseWindow(window, id, mut discarded)) => {
                discarded.push(id);
                return self.close_window(window, discarded);
            }
            _ => {}
        }
        Task::none()
    }

    pub(super) fn export_drawing(&mut self, format: &'static str) -> Task<Message> {
        if ["svg", "svg-office", "pdf", "png"].contains(&format) || cfg!(windows) && format == "emf"
        {
            return self.export_figure(format, false);
        }
        let mut request = Request::molecule("export", self.tab.doc.clone());
        request.format = Some(format.into());
        self.run(request, Job::Export(format))
    }

    pub(super) fn structure_exported(
        &mut self,
        result: Result<Option<super::figure_export::Saved>, String>,
    ) {
        match result {
            Ok(Some(saved)) => {
                self.status = format!(
                    "Exported {}",
                    saved.path.file_name().unwrap_or_default().to_string_lossy()
                );
                for detail in saved.details {
                    self.status.push_str(" · ");
                    self.status.push_str(&detail);
                }
                self.error = false;
            }
            Ok(None) => {}
            Err(e) => {
                self.status = e;
                self.error = true;
            }
        }
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
mod performance;
