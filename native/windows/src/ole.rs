//! Out-of-process OLE embedding. COM interfaces and storage stay on an STA.
//! Office stores the complete drawing; an editor process works on a private
//! temporary copy, and explicit saves update the container through SaveObject.

mod object;
mod storage;

use super::{Result, clipboard};
use anyhow::Context;
use object::{DrawingObject, Factory};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::{Rc, Weak},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use storage::Drawing;
use windows::{
    Win32::{
        Foundation::*,
        System::{Com::StructuredStorage::*, Com::*, Ole::*, Registry::*},
        UI::WindowsAndMessaging::*,
    },
    core::*,
};

const CLSID: GUID = GUID::from_u128(0x3bac2b7e_73a2_4f3a_9ce7_5e9b438c59b4);
const CLSID_TEXT: &str = "{3BAC2B7E-73A2-4F3A-9CE7-5E9B438C59B4}";
const LIMIT: usize = 64 * 1024 * 1024;
static ENABLED: AtomicBool = AtomicBool::new(false);
type Render = fn(&[u8]) -> std::result::Result<super::OfficePreview, String>;
type CResult<T> = windows::core::Result<T>;
thread_local! {
    static OBJECTS: RefCell<Vec<Weak<State>>> = const { RefCell::new(Vec::new()) };
    static RENDER: Cell<Option<Render>> = const { Cell::new(None) };
    static LOCKS: Cell<u32> = const { Cell::new(0) };
}
pub(super) fn enable() {
    ENABLED.store(true, Ordering::Relaxed);
}
pub(super) fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}
fn error(message: impl ToString) -> Error {
    Error::new(E_FAIL, message.to_string())
}
fn trace(message: impl AsRef<str>) {
    if let Some(path) = std::env::var_os("RESHIKI_OLE_LOG") {
        use std::io::Write;
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(file, "{} {}", std::process::id(), message.as_ref());
        }
    }
}
struct Apartment;
impl Apartment {
    fn new() -> CResult<Self> {
        unsafe {
            OleInitialize(None)?;
        }
        Ok(Self)
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            OleUninitialize();
        }
    }
}

struct Session {
    // Keep the file if Office disconnects or rejects a save. Never delete a
    // document that an editor process may still be using.
    directory: std::path::PathBuf,
    child: std::process::Child,
    last: Vec<u8>,
    refresh_preview: bool,
}
struct State {
    drawing: RefCell<Drawing>,
    storage: RefCell<Option<IStorage>>,
    site: RefCell<Option<IOleClientSite>>,
    advise: IOleAdviseHolder,
    data_advise: IDataAdviseHolder,
    dirty: Cell<bool>,
    session: RefCell<Option<Session>>,
    pending_ack: Cell<bool>,
    last_save_attempt: Cell<Option<Instant>>,
}
impl State {
    fn new(drawing: Drawing) -> CResult<Rc<Self>> {
        let state = Rc::new(Self {
            drawing: RefCell::new(drawing),
            storage: RefCell::new(None),
            site: RefCell::new(None),
            advise: unsafe { CreateOleAdviseHolder()? },
            data_advise: unsafe { CreateDataAdviseHolder()? },
            dirty: Cell::new(false),
            session: RefCell::new(None),
            pending_ack: Cell::new(false),
            last_save_attempt: Cell::new(None),
        });
        OBJECTS.with_borrow_mut(|objects| objects.push(Rc::downgrade(&state)));
        Ok(state)
    }
    fn start(&self) -> CResult<()> {
        trace("Open editor");
        if self.session.borrow().is_some() {
            return Ok(());
        }
        let directory = tempfile::Builder::new()
            .prefix("ReShiki Office ")
            .tempdir()
            .map_err(error)?;
        let path = directory.path().join("Office drawing.reshiki");
        let document = self.drawing.borrow().document.clone();
        if document.is_empty() {
            return Err(error("The embedded drawing is empty"));
        }
        std::fs::write(&path, &document).map_err(error)?;
        write_ack(&path, &document)?;
        let child = std::process::Command::new(std::env::current_exe().map_err(error)?)
            .arg("--open")
            .arg(&path)
            .arg("--office-edit")
            .spawn()
            .map_err(error)?;
        *self.session.borrow_mut() = Some(Session {
            directory: directory.keep(),
            child,
            last: document,
            refresh_preview: true,
        });
        if let Some(site) = self.site.borrow().clone() {
            unsafe {
                site.OnShowWindow(true)?;
            }
        }
        Ok(())
    }
    fn poll(self: &Rc<Self>) {
        let mut updated = None;
        let mut closed = false;
        {
            let mut session = self.session.borrow_mut();
            if let Some(session) = session.as_mut() {
                let path = session.directory.join("Office drawing.reshiki");
                if !path.with_extension("office-saved").exists() {
                    self.pending_ack.set(true);
                }
                if let Ok(metadata) = path.metadata()
                    && metadata.len() <= LIMIT as u64
                    && let Ok(bytes) = std::fs::read(&path)
                    && (bytes != session.last
                        || (session.refresh_preview && self.pending_ack.get()))
                {
                    let rendered = RENDER.with(|render| {
                        render
                            .get()
                            .ok_or_else(|| "No Office renderer".to_owned())
                            .and_then(|render| render(&bytes))
                    });
                    match rendered.and_then(|preview| {
                        Drawing::new(bytes.clone(), preview.png, Some(preview.metafile))
                            .map_err(|e| e.to_string())
                    }) {
                        Ok(drawing) => {
                            session.last = bytes;
                            session.refresh_preview = false;
                            updated = Some(drawing);
                        }
                        Err(error) => {
                            trace(format!("Rejected editor update: {error}"));
                            let _ = std::fs::write(path.with_extension("office-error"), &error);
                            // The original drawing remains intact. Do not acknowledge
                            // bytes that the renderer could not validate.
                            return;
                        }
                    }
                }
                closed = session.child.try_wait().ok().flatten().is_some();
            }
        }
        if let Some(drawing) = updated {
            trace("Editor saved an update");
            *self.drawing.borrow_mut() = drawing;
            self.dirty.set(true);
            self.pending_ack.set(true);
            let data: IDataObject = DrawingObject {
                state: self.clone(),
            }
            .into();
            unsafe {
                if let Err(error) = self.data_advise.SendOnDataChange(&data, 0, 0) {
                    trace(format!("Data advise: {error}"));
                }
            }
        }
        if self.pending_ack.get()
            && self
                .last_save_attempt
                .get()
                .is_none_or(|at| at.elapsed() >= Duration::from_secs(1))
        {
            self.last_save_attempt.set(Some(Instant::now()));
            let _ = self.save_container();
        }
        if closed {
            trace("Editor closed");
            if self.dirty.get() || self.pending_ack.get() {
                let _ = self.save_container();
            }
            if let Some(session) = self.session.borrow_mut().take()
                && !self.pending_ack.get()
            {
                let _ = std::fs::remove_dir_all(session.directory);
            }
            if let Some(site) = self.site.borrow().clone() {
                unsafe {
                    let _ = site.OnShowWindow(false);
                }
            }
            unsafe {
                let _ = self.advise.SendOnClose();
            }
        }
    }
    fn save_container(&self) -> CResult<()> {
        let site = self.site.borrow().clone();
        let result = site
            .ok_or_else(|| error("Office has closed this drawing"))
            .and_then(|site| unsafe { site.SaveObject() });
        if let Some(session) = self.session.borrow().as_ref() {
            let path = session.directory.join("Office drawing.reshiki");
            match &result {
                Ok(()) => {
                    write_ack(&path, &self.drawing.borrow().document)?;
                    self.pending_ack.set(false);
                    let _ = std::fs::remove_file(path.with_extension("office-error"));
                }
                Err(error) => {
                    let _ = std::fs::write(path.with_extension("office-error"), error.to_string());
                }
            }
        }
        if result.is_ok() {
            unsafe {
                let _ = self.advise.SendOnSave();
            }
            trace("Container updated");
        }
        result
    }
}

fn write_ack(path: &std::path::Path, document: &[u8]) -> CResult<()> {
    use std::io::Write;
    let mut file = tempfile::NamedTempFile::new_in(
        path.parent()
            .ok_or_else(|| error("Missing Office directory"))?,
    )
    .map_err(error)?;
    file.write_all(document).map_err(error)?;
    file.persist(path.with_extension("office-saved"))
        .map_err(error)?;
    Ok(())
}

pub(super) fn prepare_save(path: &std::path::Path) {
    let _ = std::fs::remove_file(path.with_extension("office-saved"));
    let _ = std::fs::remove_file(path.with_extension("office-error"));
}

pub(super) fn wait_for_save(
    path: &std::path::Path,
    bytes: &[u8],
) -> std::result::Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if std::fs::read(path.with_extension("office-saved")).is_ok_and(|ack| ack == bytes) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(format!(
        "Office has not accepted this update. Keep Office open and try Save again, or use Save as. A local copy is at {}",
        path.display()
    ))
}

fn register() -> CResult<()> {
    let executable = std::env::current_exe().map_err(error)?;
    let command = format!("\"{}\" --ole-server", executable.display());
    let root = format!("Software\\Classes\\CLSID\\{CLSID_TEXT}");
    for (path, value) in [
        (root.clone(), "ReShiki drawing".to_owned()),
        (format!("{root}\\LocalServer32"), command),
        (format!("{root}\\InprocHandler32"), "ole32.dll".to_owned()),
        (
            format!("{root}\\ProgID"),
            "ReShiki.EmbeddedDrawing.1".to_owned(),
        ),
        (format!("{root}\\Verb\\0"), "Edit,0,2".to_owned()),
        (format!("{root}\\Verb\\1"), "Open,0,2".to_owned()),
        (
            "Software\\Classes\\ReShiki.EmbeddedDrawing.1".into(),
            "ReShiki drawing".to_owned(),
        ),
        (
            "Software\\Classes\\ReShiki.EmbeddedDrawing.1\\CLSID".into(),
            CLSID_TEXT.to_owned(),
        ),
    ] {
        let path = HSTRING::from(path);
        let bytes: Vec<_> = value
            .encode_utf16()
            .chain(Some(0))
            .flat_map(u16::to_le_bytes)
            .collect();
        unsafe {
            let mut key = HKEY::default();
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                &path,
                0,
                PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_WRITE,
                None,
                &mut key,
                None,
            )
            .ok()?;
            let result = RegSetValueExW(key, PCWSTR::null(), 0, REG_SZ, Some(&bytes)).ok();
            let _ = RegCloseKey(key);
            result?;
        }
    }
    Ok(())
}

pub(super) fn copy(mut formats: BTreeMap<u32, Vec<u8>>) -> Result<()> {
    // A dedicated thread avoids entering an STA on a Tokio MTA worker.
    std::thread::spawn(move || -> Result<()> {
        let _apartment = Apartment::new()?;
        register()?;
        // Word prefers standalone images to Embed Source, irrespective of
        // enumeration order. Pictures are available through Copy Image;
        // regular Copy carries an OLE preview inside the editable object.
        formats.remove(&clipboard::format("image/svg+xml")?);
        let document = formats
            .get(&clipboard::format("dev.reshiki.drawing")?)
            .context("Missing drawing")?
            .clone();
        let png = formats
            .get(&clipboard::format("PNG")?)
            .context("Missing drawing preview")?
            .clone();
        let metafile = formats
            .remove(&clipboard::format("dev.reshiki.office-metafile")?)
            .context("Missing Office vector preview")?;
        let drawing = Drawing::new(document, png, Some(metafile))?;
        formats.remove(&clipboard::format("PNG")?);
        formats.remove(&8); // CF_DIB
        let data: IDataObject = object::ClipboardObject { drawing, formats }.into();
        unsafe {
            OleSetClipboard(&data)?;
            OleFlushClipboard()?;
        }
        Ok(())
    })
    .join()
    .map_err(|_| anyhow::anyhow!("Office clipboard thread failed"))?
}

pub(super) fn read_own(picture_only: bool) -> Result<Option<Vec<u8>>> {
    let source = clipboard::format("Embed Source")?;
    let embedded = clipboard::format("Embedded Object")?;
    unsafe {
        use windows::Win32::System::DataExchange::IsClipboardFormatAvailable;
        if IsClipboardFormatAvailable(source).is_err()
            && IsClipboardFormatAvailable(embedded).is_err()
        {
            return Ok(None);
        }
    }
    std::thread::spawn(move || -> Result<Option<Vec<u8>>> {
        let _apartment = Apartment::new()?;
        let data = unsafe { OleGetClipboard()? };
        for id in [source, embedded] {
            let format = storage::format_etc(id, TYMED_ISTORAGE);
            if unsafe { data.QueryGetData(&format) }.is_err() {
                continue;
            }
            let medium = storage::Medium(unsafe { data.GetData(&format)? });
            if medium.0.tymed != TYMED_ISTORAGE.0 as u32 {
                continue;
            }
            let storage =
                unsafe { medium.0.u.pstg.as_ref() }.context("Missing embedded storage")?;
            if unsafe { ReadClassStg(storage)? } == CLSID {
                let drawing = Drawing::load(storage)?;
                return Ok(Some(if picture_only {
                    drawing.png
                } else {
                    drawing.document
                }));
            }
        }
        Ok(None)
    })
    .join()
    .map_err(|_| anyhow::anyhow!("Office clipboard read thread failed"))?
}

pub(super) fn run(render: Render) -> Result<()> {
    let _apartment = Apartment::new()?;
    RENDER.with(|cell| cell.set(Some(render)));
    let factory: IClassFactory = Factory.into();
    let cookie = unsafe {
        CoRegisterClassObject(&CLSID, &factory, CLSCTX_LOCAL_SERVER, REGCLS_MULTIPLEUSE)?
    };
    trace("OLE server registered");
    let mut last_busy = Instant::now();
    loop {
        let mut message = MSG::default();
        unsafe {
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                if message.message == WM_QUIT {
                    CoRevokeClassObject(cookie)?;
                    return Ok(());
                }
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        let objects = OBJECTS.with_borrow_mut(|objects| {
            objects.retain(|object| object.strong_count() > 0);
            objects.iter().filter_map(Weak::upgrade).collect::<Vec<_>>()
        });
        for object in &objects {
            object.poll();
        }
        if !objects.is_empty() || LOCKS.with(Cell::get) > 0 {
            last_busy = Instant::now();
        }
        if last_busy.elapsed() > Duration::from_secs(30) {
            break;
        }
        // This is an OLE helper process, not the editor event loop.
        std::thread::sleep(Duration::from_millis(100));
    }
    unsafe {
        CoRevokeClassObject(cookie)?;
    }
    Ok(())
}
