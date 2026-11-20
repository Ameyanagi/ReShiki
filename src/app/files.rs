use super::{Job, Message, Pending};
use crate::session::Host;
use iced::Task;
use reshiki::engine::Request;
use std::path::PathBuf;

/// The name of Save As in Export, the toolbar and the palette. The browser
/// saves .rsk only.
pub(super) const SAVE_AS: &str = if cfg!(target_arch = "wasm32") {
    "Save as .rsk…"
} else {
    "Save as…"
};

/// The formats Save as offers, by extension, with their names in its list.
pub(super) const SAVE_FORMATS: [(&str, &str); 4] = [
    (reshiki::compatibility::NATIVE_EXTENSION, "ReShiki (.rsk)"),
    ("cdxml", "ChemDraw (.cdxml)"),
    ("cdx", "ChemDraw (.cdx)"),
    ("mol", "MOL"),
];

/// The format of a ChemDraw or MOL file, which Save can write back in place.
pub(super) fn kept_format(path: &std::path::Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?;
    SAVE_FORMATS[1..]
        .iter()
        .map(|(format, _)| *format)
        .find(|format| extension.eq_ignore_ascii_case(format))
}

/// What a format other than .rsk keeps of a drawing.
pub(super) fn format_keeps(format: &str) -> &'static str {
    if format == "mol" {
        "MOL keeps the structure only."
    } else {
        "ChemDraw keeps most of the drawing."
    }
}

/// What the first Save of a ChemDraw file says: what keeping the format loses
/// from the file (`loss`) and from the drawing, or what the format keeps when
/// that is nothing.
pub(super) fn keep_format_text(
    format: &str,
    loss: reshiki::formats::ChemdrawLoss,
    doc: &reshiki::document::Document,
) -> String {
    let mut lost = Vec::new();
    if format != "mol" {
        for (count, one, many) in [
            (loss.schemes, "reaction scheme", "reaction schemes"),
            (loss.steps, "step", "steps"),
            (loss.pages, "page", "pages"),
            (
                loss.unsupported,
                "object ReShiki draws differently or leaves out",
                "objects ReShiki draws differently or leaves out",
            ),
        ] {
            match count {
                0 => {}
                1 => lost.push(format!("1 {one}")),
                _ => lost.push(format!("{count} {many}")),
            }
        }
        if !doc.reactions.is_empty() {
            lost.push("reaction roles".into());
        }
        if reshiki::arrow_anchors::export_notice(doc).is_some()
            || reshiki::arrow_labels::export_notice(doc).is_some()
        {
            lost.push("arrow links".into());
        }
        if reshiki::arrows::shapes_notice(doc).is_some() {
            lost.push("editable arrow looks and callouts".into());
        }
    }
    // ReShiki reads panels and purpose back from its own CDXML only.
    if format == "cdx" && doc.graphics.iter().any(|g| g.panel.is_some()) {
        lost.push("panels".into());
    }
    if format != "cdxml" && reshiki::purpose::used(doc) {
        lost.push("object purpose".into());
    }
    let Some(last) = lost.pop() else {
        return format_keeps(format).into();
    };
    let lost = if lost.is_empty() {
        last
    } else {
        format!("{} and {last}", lost.join(", "))
    };
    format!(
        "Keep as {} loses {lost}. Save as ReShiki leaves the ChemDraw file unchanged.",
        format.to_uppercase()
    )
}
#[cfg(not(target_arch = "wasm32"))]
const SAVE: &str = "Save";
#[cfg(not(target_arch = "wasm32"))]
const DONT_SAVE: &str = "Don’t Save";
#[cfg(not(target_arch = "wasm32"))]
const ADDIN_SAVE_STATUS: &str = "Saving draft · Waiting for Microsoft 365 to confirm…";
#[cfg(not(target_arch = "wasm32"))]
const OFFICE_SESSION_ENDED: &str =
    "The Office edit session has ended; save this drawing to another file";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(not(target_arch = "wasm32"))]
pub(super) enum SaveBackend {
    Addin,
    OfficeOle,
    LibreOfficeRetry,
    Atomic,
}

/// Native Office backends on Windows; other platforms use atomic writes
/// for every host except the Microsoft 365 add-in.
#[cfg(not(target_arch = "wasm32"))]
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
#[cfg(not(target_arch = "wasm32"))]
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
            _ => Task::perform(dialog.show(), save_answer),
        }
    })
}

/// Platforms without custom buttons answer Yes/No/Cancel in the same order.
#[cfg(not(target_arch = "wasm32"))]
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
    reshiki::storage::write_saved(path, bytes)
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
    #[cfg(target_arch = "wasm32")]
    pub handle: Option<wasm_bindgen::JsValue>,
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
    crate::web::pick(extensions, multiple, limit).await
}

/// Browser files, by name with their contents.
#[cfg(target_arch = "wasm32")]
pub(super) fn browser_files(files: Vec<(String, Vec<u8>)>) -> Vec<Picked> {
    files
        .into_iter()
        .map(|(name, contents)| Picked {
            path: name.into(),
            contents,
            handle: None,
        })
        .collect()
}

/// Opens `path` for reading. Anything but a regular file is refused before it
/// is opened: opening a named pipe with no writer would wait forever.
fn open(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    if !std::fs::metadata(path)?.is_file() {
        return Err(std::io::Error::other("not a regular file"));
    }
    std::fs::File::open(path)
}

/// Read files up to `limit` on a blocking worker.
pub(super) async fn read_paths(paths: Vec<PathBuf>, limit: Limit) -> Result<Vec<Picked>, String> {
    use std::io::Read;
    reshiki::task::spawn_blocking(move || {
        paths
            .into_iter()
            .map(|path| {
                let mut contents = Vec::new();
                open(&path)
                    .and_then(|file| {
                        file.take(limit(&path).saturating_add(1))
                            .read_to_end(&mut contents)
                    })
                    .map_err(|e| {
                        let name = path.file_name().unwrap_or(path.as_os_str());
                        format!("Could not read {}: {e}", name.to_string_lossy())
                    })?;
                Ok(Picked {
                    path,
                    contents,
                    #[cfg(target_arch = "wasm32")]
                    handle: None,
                })
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
    /// A native drawing and the view it was saved in.
    Native(
        Box<reshiki::document::Document>,
        Option<reshiki::document::View>,
    ),
    /// A picture's embedded drawing, or the picture itself with the reason
    /// its drawing could not be read.
    Picture(Box<reshiki::document::Document>, Option<String>),
    Import {
        format: &'static str,
        contents: String,
        /// What a save in ChemDraw format loses from this file.
        loss: reshiki::formats::ChemdrawLoss,
    },
    /// An SD file's records, read.
    Records(Box<super::import::Batch>),
    #[cfg(target_arch = "wasm32")]
    Browser {
        handle: wasm_bindgen::JsValue,
        duplicate: Option<super::document_tab::TabId>,
        prepared: Box<Prepared>,
    },
}

#[cfg(not(target_arch = "wasm32"))]
pub type Saved = PathBuf;

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone)]
pub struct Saved {
    pub path: PathBuf,
    /// None means a download handoff, which does not establish a saved file.
    pub handle: Option<wasm_bindgen::JsValue>,
}

pub type Opened = Option<(PathBuf, Result<Prepared, String>)>;

/// The browser's status while it prepares chosen files on the page's thread.
#[cfg(target_arch = "wasm32")]
pub(super) const OPENING: &str = "Opening…";

/// Ex file commands use native drawings, retaining an explicit native suffix.
pub(super) fn native_drawing_name(name: &str) -> String {
    if std::path::Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(reshiki::compatibility::is_native_extension)
    {
        name.into()
    } else {
        format!("{name}.{}", reshiki::compatibility::NATIVE_EXTENSION)
    }
}

fn prepare(path: &std::path::Path, contents: Vec<u8>) -> Result<Prepared, String> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(extension.as_str(), "png" | "svg" | "pdf")
        && let Some(payload) = reshiki::payload::read(&contents)
    {
        Ok(match payload {
            Ok(drawing) => Prepared::Picture(Box::new(drawing), None),
            Err(error) if extension == "png" => Prepared::Picture(
                Box::new(reshiki::pictures::clipboard_document(&contents)?),
                Some(format!("its editable drawing could not be read ({error})")),
            ),
            // ReShiki has no SVG or PDF picture to fall back to.
            Err(error) => {
                return Err(format!(
                    "This {} carries a ReShiki drawing that could not be read: {error}",
                    extension.to_ascii_uppercase()
                ));
            }
        })
    } else if reshiki::compatibility::is_native_extension(&extension) {
        let doc = reshiki::document::Document::from_native_file(&contents)
            .map_err(|e| format!("Could not open document: {e}"))?;
        Ok(Prepared::Native(
            Box::new(doc),
            reshiki::document::View::read(&contents),
        ))
    } else {
        // An SD file's records are read next, in `records`. Its extension
        // comes first, as for Insert: POSCAR is told by its name.
        let format = (extension == "sdf")
            .then_some("sdf")
            .or_else(|| reshiki::compatibility::file_format(path))
            .or_else(|| matches!(extension.as_str(), "smi" | "smiles").then_some("smiles"))
            .ok_or_else(|| super::import::unsupported_file(path))?;
        if contents.len() as u64 > super::import::limit(path) {
            return Err("File exceeds 16 MB".into());
        }
        let contents = super::import::contents(format, contents)?;
        Ok(Prepared::Import {
            format,
            // The browser has no file to write back.
            loss: if cfg!(target_arch = "wasm32") {
                Default::default()
            } else {
                reshiki::formats::chemdraw_loss(format, &contents)
            },
            contents,
        })
    }
}

/// Every record of an SD file, read as Insert reads them, before the file
/// is reported: files then still open in the order they were chosen.
async fn records(
    path: &std::path::Path,
    prepared: Result<Prepared, String>,
) -> Result<Prepared, String> {
    let Ok(Prepared::Import {
        format: "sdf",
        contents,
        ..
    }) = prepared
    else {
        return prepared;
    };
    let file = Picked {
        path: path.to_path_buf(),
        contents: contents.into_bytes(),
        #[cfg(target_arch = "wasm32")]
        handle: None,
    };
    let batch = super::import::load(
        reshiki::engine::LocalEngine::default(),
        vec![file],
        Default::default(),
        reshiki::document::Document::default().canvas_theme,
    )
    .await?;
    Ok(Prepared::Records(Box::new(batch)))
}

/// Reading, decoding and native validation all run on a blocking worker, never
/// on the event loop or an async executor's cooperative thread.
pub(super) async fn read(path: PathBuf) -> Opened {
    let source = path.clone();
    let result = reshiki::task::spawn_blocking(move || {
        let mut contents = Vec::new();
        open(&source)
            .and_then(|file| {
                use std::io::Read;
                file.take(super::import::open_limit(&source).saturating_add(1))
                    .read_to_end(&mut contents)
            })
            .map_err(|e| e.to_string())?;
        prepare(&source, contents)
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r);
    let result = records(&path, result).await;
    Some((path, result))
}

/// The Open dialog's title, which names what it opens: it has no filter.
const OPEN_TITLE: &str = "Open a ReShiki, editable PNG/SVG/PDF, MOL, SDF, RXN, CDXML, CDX, CDS, SMILES, CIF, PDB, POSCAR or XYZ file";

/// Opens the files the user chooses, each in a tab.
pub(super) fn open_dialog(
    #[cfg(target_arch = "wasm32")] handles: Vec<(
        super::document_tab::TabId,
        wasm_bindgen::JsValue,
    )>,
) -> Task<Message> {
    // Accept supported extensions without relying on macOS type
    // registration; the worker validates the content.
    Task::future(pick(OPEN_TITLE, None, false, super::import::open_limit)).then(move |files| {
        open_picked(
            files,
            #[cfg(target_arch = "wasm32")]
            handles.clone(),
        )
    })
}

/// Opens read files as tabs in this order, or reports why they could not be read.
pub(super) fn open_picked(
    files: Result<Vec<Picked>, String>,
    #[cfg(target_arch = "wasm32")] handles: Vec<(
        super::document_tab::TabId,
        wasm_bindgen::JsValue,
    )>,
) -> Task<Message> {
    match files {
        Ok(files) => files.into_iter().fold(Task::none(), |task, file| {
            #[cfg(target_arch = "wasm32")]
            let handles = handles.clone();
            // The browser prepares each file on the page's thread, which holds the page.
            #[cfg(target_arch = "wasm32")]
            let task = task.chain(Task::done(Message::Opening));
            task.chain(Task::perform(
                async move {
                    let (path, result) = prepare_contents(file.path, Ok(file.contents)).await?;
                    #[cfg(target_arch = "wasm32")]
                    let result = match (file.handle, result) {
                        (Some(handle), Ok(prepared))
                            // Neither a picture nor an SD file stays the tab's file.
                            if !matches!(
                                &prepared,
                                Prepared::Picture(..) | Prepared::Records(..)
                            ) =>
                        {
                            let mut duplicate = None;
                            for (id, other) in handles {
                                if crate::web::same_entry(&handle, &other).await {
                                    duplicate = Some(id);
                                    break;
                                }
                            }
                            Ok(Prepared::Browser {
                                handle,
                                duplicate,
                                prepared: Box::new(prepared),
                            })
                        }
                        (_, result) => result,
                    };
                    Some((path, result))
                },
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
    // Let "Opening…" show before this holds the page's thread. Two frames:
    // the first can be drawn before the app has taken the `Opening` message.
    #[cfg(target_arch = "wasm32")]
    for _ in 0..2 {
        crate::web::after_paint().await;
    }
    let result = reshiki::task::spawn_blocking(move || prepare(&source, contents?))
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r);
    let result = records(&path, result).await;
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
        let name = self.document_name();
        // A file Save writes back keeps its name; an imported "co2.cif", or
        // an "a.sdf" opened from GitHub, suggests "co2.rsk".
        let writable = self.tab.path.as_deref().is_some_and(|path| {
            kept_format(path).is_some()
                || path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(reshiki::compatibility::is_native_extension)
        });
        let name = if writable {
            name
        } else {
            let stem = std::path::Path::new(&name)
                .file_stem()
                .map_or(name.clone(), |s| s.to_string_lossy().into_owned());
            format!("{stem}.{}", reshiki::compatibility::NATIVE_EXTENSION)
        };
        (path, name)
    }

    /// A named edit opens beside the saved drawing without asking for a path.
    pub(super) fn open_drawing_named(&mut self, name: String) -> Task<Message> {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(path) = self
            .tab
            .path
            .as_ref()
            .and_then(|path| path.parent())
            .map(|parent| parent.join(&name))
            .filter(|path| path.is_file())
        {
            return Task::perform(read(path), Message::FilePrepared);
        }
        self.status = format!("{name} not found next to the current drawing");
        self.error = true;
        Task::none()
    }

    /// Every finished save of a tab still open, in front or behind it, comes
    /// here, with that tab in front.
    pub(super) fn file_saved(
        &mut self,
        epoch: u64,
        snapshot: Box<reshiki::document::Document>,
        result: Result<Option<Saved>, String>,
    ) -> iced::Task<super::Message> {
        // A GitHub drawing keeps what this save wrote as a version.
        #[cfg(not(target_arch = "wasm32"))]
        let written = match &result {
            Ok(Some(path)) if epoch == self.tab.file_epoch => self
                .github
                .locate(Some(path))
                .map(|_| (path.clone(), (*snapshot).clone())),
            _ => None,
        };
        #[cfg(not(target_arch = "wasm32"))]
        let saved = matches!(&result, Ok(Some(_))) && epoch == self.tab.file_epoch;
        // Before a close the save goes on with asks whether to send it.
        #[cfg(not(target_arch = "wasm32"))]
        if let Some((path, _)) = &written {
            self.github_unread(path);
        }
        let task = self.drawing_saved(epoch, snapshot, result);
        #[cfg(not(target_arch = "wasm32"))]
        let task = iced::Task::batch([task, self.github_saved(saved, written)]);
        task
    }

    fn drawing_saved(
        &mut self,
        epoch: u64,
        snapshot: Box<reshiki::document::Document>,
        result: Result<Option<Saved>, String>,
    ) -> iced::Task<super::Message> {
        self.file_io.saving = false;
        let snapshot = *snapshot;
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
            Ok(Some(saved)) => {
                #[cfg(target_arch = "wasm32")]
                let path = {
                    let Some(handle) = saved.handle else {
                        self.pending = None;
                        self.status = download_status(
                            &saved.path.file_name().unwrap_or_default().to_string_lossy(),
                        );
                        self.error = false;
                        return Task::none();
                    };
                    self.tab.file_handle = Some(handle);
                    saved.path
                };
                #[cfg(not(target_arch = "wasm32"))]
                let path = saved;
                // Continue the save dialog's action only when nothing is left unsaved.
                self.tab.mark_saved(snapshot);
                if self
                    .tab
                    .office
                    .as_ref()
                    .is_some_and(|binding| !binding.same_path(&path))
                {
                    self.detach_office();
                }
                self.tab.path = Some(path);
                // A file saved in a format keeps it without asking.
                self.tab.keep_format = true;
                self.tab.untitled_name = None;
                self.status = if self.office_document() {
                    if self
                        .tab
                        .office
                        .as_ref()
                        .is_some_and(|binding| binding.host == Host::LibreOffice)
                    {
                        "Saved for LibreOffice · Check the update in the document"
                    } else {
                        "Drawing updated in Office · Save the Office document"
                    }
                    .into()
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
                    #[cfg(target_arch = "wasm32")]
                    {
                        self.prompt = Some(super::prompt::Prompt::Save(self.document_name()));
                    }
                    #[cfg(not(target_arch = "wasm32"))]
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
        // The file may open in another tab, which would park this status.
        #[cfg(target_arch = "wasm32")]
        if self.status == OPENING {
            self.status = super::READY.into();
        }
        let Some((path, result)) = opened else {
            return iced::Task::none();
        };
        match result {
            #[cfg(target_arch = "wasm32")]
            Ok(Prepared::Browser {
                handle,
                duplicate,
                prepared,
            }) => {
                if self.open_browser_duplicate(duplicate) {
                    return Task::none();
                }
                let task = self.file_prepared(Some((path, Ok(*prepared))));
                self.tab.file_handle = Some(handle);
                return task;
            }
            Err(error) => {
                self.status = error;
                self.error = true;
            }
            // Side by side, as an unsaved drawing with the file's name: Save
            // cannot write an SD file back.
            Ok(Prepared::Records(batch)) => {
                let name = super::import::name(&path);
                let Some((doc, placed, total)) = super::import::side_by_side(batch.drawings) else {
                    self.status = format!("Could not open {name}: it has no structure");
                    self.error = true;
                    return Task::none();
                };
                self.open_target(None);
                self.show_opened(doc, None);
                self.tab.untitled_name = Some(name);
                self.tab.mark_saved(reshiki::document::Document::default());
                // A record that no longer fits in one drawing is left out, and
                // the status says so in amber, as an open from GitHub does.
                if placed < total {
                    self.status =
                        format!("Opened {placed} of {total} structures · The rest don't fit");
                }
                for warning in batch.warnings {
                    self.status.push_str(" · ");
                    self.status.push_str(&warning);
                }
                #[cfg(not(target_arch = "wasm32"))]
                if placed < total {
                    self.github.amber = Some(self.status.clone());
                }
            }
            Ok(Prepared::Import {
                format,
                contents,
                loss,
            }) => {
                // A ChemDraw or MOL file stays the tab's file, as a drawing does.
                let tabs = self.strip().count();
                if !self
                    .open_target(Some(path.as_path()).filter(|path| kept_format(path).is_some()))
                {
                    return Task::none();
                }
                if self.strip().count() > tabs {
                    self.tabs.opening.push((self.tab.id, self.tab.file_epoch));
                }
                self.tab.chemdraw_loss = loss;
                let task = self.run(
                    super::Request::import(format, &contents),
                    super::Job::ImportFile(path),
                );
                #[cfg(any(windows, target_os = "linux"))]
                self.desktop_import_started();
                return task;
            }
            Ok(Prepared::Native(doc, view)) => {
                #[cfg(not(target_arch = "wasm32"))]
                let kept = self.github_opened(&path);
                #[cfg(target_arch = "wasm32")]
                let kept = iced::Task::none();
                if self.open_target(Some(&path)) {
                    self.show_opened(*doc, Some(path));
                    if let Some(view) = view {
                        self.restore_view(view);
                    }
                }
                return kept;
            }
            Ok(Prepared::Picture(doc, warning)) => {
                self.open_target(None);
                self.show_opened(*doc, None);
                self.tab.untitled_name = path
                    .file_stem()
                    .map(|name| name.to_string_lossy().into_owned());
                self.tab.mark_saved(reshiki::document::Document::default());
                let name = path
                    .file_name()
                    .unwrap_or(path.as_os_str())
                    .to_string_lossy();
                self.status = match warning {
                    None => format!("Opened the editable drawing from {name}"),
                    Some(warning) => format!("Opened {name} as a picture: {warning}"),
                };
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
        self.tab.mark_saved(self.tab.doc.clone());
        self.tab.path = path;
        self.tab.keep_format = false;
        #[cfg(target_arch = "wasm32")]
        {
            self.tab.file_handle = None;
        }
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
        let incompatible = self
            .tab
            .doc
            .reactions
            .iter()
            .filter(|reaction| {
                reshiki::reactions::Role::ALL
                    .into_iter()
                    .any(|role| !reaction.participants(role).is_empty())
                    && self.tab.doc.arrows.iter().any(|arrow| {
                        arrow.id == reaction.arrow
                            && reshiki::arrows::Preset::from_kind(&arrow.kind)
                                .reaction_role_refusal()
                                .is_some()
                    })
            })
            .count();
        if incompatible > 0 {
            let (noun, verb) = if incompatible == 1 {
                ("arrow", "has")
            } else {
                ("arrows", "have")
            };
            self.status.push_str(&format!(
                " · Warning: {incompatible} {noun} {verb} incompatible reaction roles"
            ));
        }
        self.error = false;
    }

    /// Show the opened drawing as it was saved, unless that view shows none
    /// of it (or of its pages): then it stays fitted.
    fn restore_view(&mut self, view: reshiki::document::View) {
        let camera = crate::canvas::Camera {
            center: view.center,
            // The range the camera itself keeps; reveals go down to 0.001.
            zoom: view.zoom.clamp(0.001, 5.),
        };
        let paper = self.guides.paper(iced::Rectangle::with_size(self.viewport));
        let (half_x, half_y) = (
            camera.world_len(paper.width / 2.),
            camera.world_len(paper.height / 2.),
        );
        let layout = self.tab.doc.page_layout.as_ref();
        let pages: Vec<_> = layout
            .map(|layout| {
                (0..layout.count())
                    .filter_map(|i| layout.bounds(i))
                    .collect()
            })
            .unwrap_or_default();
        let shown = |(lo, hi): &(reshiki::document::Point, reshiki::document::Point)| {
            (lo.x - half_x..=hi.x + half_x).contains(&camera.center.x)
                && (lo.y - half_y..=hi.y + half_y).contains(&camera.center.y)
        };
        if !std::iter::once(self.tab.doc.bounds())
            .chain(pages.iter().copied())
            .any(|bounds| shown(&bounds))
        {
            return;
        }
        // Page controls act on the page nearest the middle of the view.
        let away = |(lo, hi): &(reshiki::document::Point, reshiki::document::Point)| {
            lo.lerp(*hi, 0.5).distance(camera.center)
        };
        if let Some((page, _)) = pages
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| away(a).total_cmp(&away(b)))
        {
            self.tab.pages.active = page;
        }
        self.tab.camera = camera;
        self.tab.pages.fit = None;
        self.tab.fit_to_view = false;
    }

    /// Save, or Save as in `format`. The first Save of a ChemDraw or MOL file
    /// asks whether to keep its format.
    pub(super) fn save_drawing(&mut self, save_as: Option<&'static str>) -> Task<Message> {
        self.front_pending();
        if save_as.is_none()
            && !self.tab.keep_format
            && let Some(format) = self.tab.path.as_deref().and_then(kept_format)
        {
            self.prompt = Some(super::prompt::Prompt::KeepFormat(format));
            return Task::none();
        }
        self.save_drawing_to(
            save_as.is_some(),
            None,
            save_as.unwrap_or(reshiki::compatibility::NATIVE_EXTENSION),
            #[cfg(target_arch = "wasm32")]
            None,
        )
    }

    /// Save as asks for the format first; the browser saves .rsk only.
    pub(super) fn save_as(&mut self) -> Task<Message> {
        if cfg!(target_arch = "wasm32") {
            return self.save_drawing(Some(reshiki::compatibility::NATIVE_EXTENSION));
        }
        self.prompt = Some(super::prompt::Prompt::SaveAs);
        Task::none()
    }

    /// Save writes this ChemDraw or MOL file in its own format from now on.
    pub(super) fn keep_format(&mut self) -> Task<Message> {
        self.front_pending();
        self.tab.keep_format = true;
        self.save_drawing(None)
    }

    pub(super) fn save_drawing_named(&mut self, name: String) -> Task<Message> {
        self.save_drawing_to(
            true,
            Some(name),
            reshiki::compatibility::NATIVE_EXTENSION,
            #[cfg(target_arch = "wasm32")]
            None,
        )
    }

    #[cfg(target_arch = "wasm32")]
    pub(super) fn save_download(&mut self, name: String) -> Task<Message> {
        self.save_drawing_to(
            true,
            None,
            reshiki::compatibility::NATIVE_EXTENSION,
            Some(name),
        )
    }

    fn save_drawing_to(
        &mut self,
        save_as: bool,
        name: Option<String>,
        #[cfg_attr(target_arch = "wasm32", expect(unused_variables))] format: &'static str,
        #[cfg(target_arch = "wasm32")] download_name: Option<String>,
    ) -> Task<Message> {
        #[cfg(not(target_arch = "wasm32"))]
        let office = self.tab.office.clone();
        #[cfg(not(target_arch = "wasm32"))]
        let office_host = office
            .as_ref()
            .filter(|_| self.office_document() && !save_as)
            .map(|binding| binding.host);
        let (path, suggested_name) = match name {
            Some(name) => (None, name),
            None => self.drawing_save_target(save_as),
        };
        // Save as from "caffeine.cdxml", or in another format, renames the suggestion.
        #[cfg(not(target_arch = "wasm32"))]
        let suggested_name = if path.is_none()
            && (format != reshiki::compatibility::NATIVE_EXTENSION
                || kept_format(std::path::Path::new(&suggested_name)).is_some())
        {
            std::path::Path::new(&suggested_name)
                .with_extension(format)
                .to_string_lossy()
                .into_owned()
        } else {
            suggested_name
        };
        #[cfg(not(target_arch = "wasm32"))]
        let engine = self.engine.clone();
        if self.file_io.saving {
            // Only this tab's own save can go on with the dialog's action.
            if self.file_io.saving_tab != Some(self.tab.id) {
                self.pending = None;
            }
            self.status = "A document save is already in progress".into();
            return Task::none();
        }
        #[cfg(target_arch = "wasm32")]
        if download_name.is_none() && !crate::web::can_save_file() && (save_as || path.is_none()) {
            self.prompt = Some(super::prompt::Prompt::Download {
                name: suggested_name,
                action: super::prompt::Download::Drawing,
            });
            return iced::widget::operation::focus("download-name");
        }
        self.file_io.saving = true;
        self.file_io.saving_tab = Some(self.tab.id);
        #[cfg(not(target_arch = "wasm32"))]
        if office_host == Some(Host::Microsoft365) {
            self.status = ADDIN_SAVE_STATUS.into();
            self.error = false;
        }
        let snapshot = std::sync::Arc::new(self.tab.doc.clone());
        let save_snapshot = std::sync::Arc::clone(&snapshot);
        #[cfg(not(target_arch = "wasm32"))]
        let github_root = self.github.root.clone();
        let view = Some(reshiki::document::View {
            center: self.tab.camera.center,
            zoom: self.tab.camera.zoom,
        });
        let epoch = self.tab.file_epoch;
        #[cfg(target_arch = "wasm32")]
        let handle = if !save_as && path.is_some() {
            self.tab.file_handle.clone()
        } else {
            None
        };
        Task::perform(
            async move {
                #[cfg(target_arch = "wasm32")]
                {
                    let handle = if let Some(handle) = handle {
                        Some(handle)
                    } else if download_name.is_none() && crate::web::can_save_file() {
                        let Some(handle) = crate::web::save_file_picker(&suggested_name).await?
                        else {
                            return Ok(None);
                        };
                        Some(handle)
                    } else {
                        None
                    };
                    let path = if let Some(handle) = &handle {
                        use wasm_bindgen::JsValue;
                        js_sys::Reflect::get(handle, &JsValue::from_str("name"))
                            .ok()
                            .and_then(|name| name.as_string())
                            .map(PathBuf::from)
                            .ok_or("The save file has no name")?
                    } else {
                        reshiki::storage::with_default_extension(
                            std::path::Path::new(
                                download_name.as_deref().unwrap_or(&suggested_name),
                            ),
                            reshiki::compatibility::NATIVE_EXTENSION,
                        )
                    };
                    let bytes = save_snapshot.file_json_with_view(view)?;
                    if let Some(handle) = &handle {
                        crate::web::write_file(handle, &bytes).await?;
                    } else {
                        write(&path, &bytes)?;
                    }
                    Ok(Some(Saved { path, handle }))
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let path = if let Some(p) = path {
                        p
                    } else {
                        let Some(path) = save_path("Save drawing", &suggested_name, format).await
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
                    // A drawing kept in a GitHub repository is shared: each
                    // person's view stays out of its commits and versions.
                    let view = view.filter(|_| {
                        github_root.as_deref().is_none_or(|root| {
                            reshiki::github::workcopy::locate(root, &path).is_none()
                        })
                    });
                    // A ChemDraw or MOL file is written by the chemistry engine.
                    let exported = match kept_format(&path) {
                        Some(format) => Some(
                            reshiki::formats::write_with_engine(
                                (*save_snapshot).clone(),
                                format,
                                &engine,
                            )
                            .await?,
                        ),
                        None => None,
                    };
                    reshiki::task::spawn_blocking(move || {
                        let bytes = match exported {
                            Some(bytes) => bytes,
                            None => save_snapshot.file_json_with_view(view)?,
                        };
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
                }
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
            return self.export_figure(format, false, self.appearance.export_editable_drawing);
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
                self.status =
                    export_status(&saved.path, &saved.details, cfg!(target_arch = "wasm32"));
                self.error = false;
            }
            Ok(None) => {
                self.status = "Export canceled".into();
                self.error = false;
            }
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

/// A browser download can still be blocked or cancelled after this handoff.
fn download_status(filename: &str) -> String {
    format!("Downloaded {filename}")
}

pub(super) fn export_status(path: &std::path::Path, details: &[String], browser: bool) -> String {
    let filename = path.file_name().unwrap_or_default().to_string_lossy();
    let mut status = if browser {
        download_status(&filename)
    } else {
        format!("Exported {filename}")
    };
    for detail in details {
        status.push_str(" · ");
        status.push_str(detail);
    }
    status
}
