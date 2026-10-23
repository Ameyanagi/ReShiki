//! Application data in the browser (see `reshiki::storage::browser`): files by
//! key in one IndexedDB object store, loaded before the app starts and kept
//! after each change. A change is kept only if the stored file is still what
//! the tab held, so a tab never saves over another tab's newer file, and the
//! site's other tabs hear of it through a BroadcastChannel.
//!
//! Fonts the page adds are kept here too, and added again at the next mount
//! when text needs them, as the page's own fallback would be.
use super::error;
use reshiki::storage::{self, browser};
use std::{cell::RefCell, collections::HashMap, path::PathBuf};
use wasm_bindgen::{JsCast, prelude::*};
use web_sys::{IdbDatabase, IdbOpenDbRequest, IdbRequest, IdbTransactionMode};

const DATABASE: &str = "reshiki";
const FILES: &str = "files";
const CHANNEL: &str = "reshiki-storage";
/// The folder of kept fonts, each named by its SHA-256.
const FONTS: &str = "fonts";

#[derive(Default)]
struct Fonts {
    /// Faces of each font added, by its SHA-256, so a font added again (by
    /// the page at each mount, or kept from before) is added once.
    added: HashMap<String, usize>,
    /// Fonts added before storage opened, kept once it does.
    waiting: Vec<(String, Vec<u8>)>,
    open: bool,
    /// Fonts kept from earlier visits, by SHA-256, until text needs them.
    kept: Vec<(String, Vec<u8>)>,
    /// Kept fonts were handed out and are not added yet.
    adding: bool,
}

/// A file another tab kept: its key and contents, or none once removed.
type Heard = (String, Option<Vec<u8>>);

thread_local! {
    static FONTS_ADDED: RefCell<Fonts> = RefCell::default();
    /// Other tabs' changes heard while the files load, until they are in.
    static EARLY: RefCell<Option<Vec<Heard>>> = const { RefCell::new(None) };
}

/// Open the page's storage, hand its files to the app and add the fonts kept
/// in it. Without storage, as in some private windows, the app starts with
/// none and its saves say that browser storage is unavailable.
pub(super) async fn open() {
    match load().await {
        Ok(()) => {
            // Added only once text needs them: a font added before the app
            // starts also becomes a fallback for the interface's own text.
            let kept = browser::files(FONTS)
                .into_iter()
                .filter_map(|(key, bytes)| {
                    let name = key.rsplit('/').next()?;
                    Some((name.strip_suffix(".font")?.to_owned(), bytes))
                })
                .collect();
            let waiting = FONTS_ADDED.with_borrow_mut(|fonts| {
                fonts.open = true;
                fonts.kept = kept;
                std::mem::take(&mut fonts.waiting)
            });
            for (hash, bytes) in waiting {
                keep(&hash, &bytes);
            }
            // Ask the browser not to evict drafts and settings under storage
            // pressure; it may decline.
            if let Some(window) = web_sys::window()
                && let Ok(asked) = window.navigator().storage().persist()
            {
                drop(wasm_bindgen_futures::JsFuture::from(asked));
            }
        }
        Err(message) => error(&format!("Browser storage is unavailable: {message}")),
    }
}

/// Add a TrueType or OpenType font for drawing, measuring and export, and
/// keep it for the next visit. Returns how many faces it holds.
pub(super) fn add_font(bytes: Vec<u8>) -> Result<usize, String> {
    use sha2::{Digest, Sha256};
    let hash = format!("{:x}", Sha256::digest(&bytes));
    if let Some(faces) = FONTS_ADDED.with_borrow(|fonts| fonts.added.get(&hash).copied()) {
        return Ok(faces);
    }
    let faces = add(hash.clone(), bytes.clone())?;
    let open = FONTS_ADDED.with_borrow_mut(|fonts| {
        if !fonts.open {
            fonts.waiting.push((hash.clone(), bytes.clone()));
        }
        fonts.open
    });
    if open {
        keep(&hash, &bytes);
    }
    Ok(faces)
}

/// Whether fonts kept from earlier visits wait to be added.
pub(super) fn has_kept() -> bool {
    FONTS_ADDED.with_borrow(|fonts| !fonts.kept.is_empty())
}

/// Whether kept fonts were handed out by [`take_kept`] and are not added yet,
/// so missing glyphs may still arrive with them.
pub(super) fn adding_kept() -> bool {
    FONTS_ADDED.with_borrow(|fonts| fonts.adding)
}

/// The kept fonts not added yet, by SHA-256, which [`add_kept`] adds; each
/// is handed out once.
pub(super) fn take_kept() -> Vec<(String, Vec<u8>)> {
    FONTS_ADDED.with_borrow_mut(|fonts| {
        fonts.adding = !fonts.kept.is_empty();
        std::mem::take(&mut fonts.kept)
    })
}

/// Add fonts from [`take_kept`].
pub(super) fn add_kept(kept: Vec<(String, Vec<u8>)>) {
    for (hash, bytes) in kept {
        if let Err(message) = add(hash, bytes) {
            error(&format!("Could not add a kept font: {message}"));
        }
    }
    FONTS_ADDED.with_borrow_mut(|fonts| fonts.adding = false);
}

fn add(hash: String, bytes: Vec<u8>) -> Result<usize, String> {
    if let Some(faces) = FONTS_ADDED.with_borrow(|fonts| fonts.added.get(&hash).copied()) {
        return Ok(faces);
    }
    let faces = reshiki::style::add_font(bytes)?;
    FONTS_ADDED.with_borrow_mut(|fonts| fonts.added.insert(hash, faces));
    Ok(faces)
}

fn keep(hash: &str, bytes: &[u8]) {
    let path = PathBuf::from(storage::BROWSER_ROOT)
        .join(FONTS)
        .join(format!("{hash}.font"));
    if let Err(message) = storage::write_atomic(&path, bytes) {
        error(&format!("Could not keep the font: {message}"));
    }
}

fn describe(thrown: JsValue) -> String {
    thrown
        .dyn_ref::<web_sys::DomException>()
        .map(|exception| exception.message())
        .or_else(|| thrown.as_string())
        .unwrap_or_else(|| format!("{thrown:?}"))
}

/// The result of `request` once it finishes.
async fn finished(request: &IdbRequest) -> Result<JsValue, String> {
    let done = js_sys::Promise::new(&mut |resolve, reject| {
        request.set_onsuccess(Some(&resolve));
        request.set_onerror(Some(&reject));
    });
    wasm_bindgen_futures::JsFuture::from(done)
        .await
        .map_err(|_| match request.error() {
            Ok(Some(exception)) => exception.message(),
            _ => "The request failed".into(),
        })?;
    request.result().map_err(describe)
}

/// Open the database, read every file and install them with a sink that keeps
/// each change and tells other tabs.
async fn load() -> Result<(), String> {
    let window = web_sys::window().ok_or("The page has no window")?;
    let factory = window
        .indexed_db()
        .map_err(describe)?
        .ok_or("This browser has no IndexedDB")?;
    let request: IdbOpenDbRequest = factory.open_with_u32(DATABASE, 1).map_err(describe)?;
    let upgrade = Closure::<dyn FnMut()>::new({
        let request = request.clone();
        move || {
            if let Some(database) = request
                .result()
                .ok()
                .and_then(|database| database.dyn_into::<IdbDatabase>().ok())
                && !database.object_store_names().contains(FILES)
                && let Err(thrown) = database.create_object_store(FILES)
            {
                error(&format!("Could not set up browser storage: {thrown:?}"));
            }
        }
    });
    request.set_onupgradeneeded(Some(upgrade.as_ref().unchecked_ref()));
    let database: IdbDatabase = finished(&request)
        .await?
        .dyn_into()
        .map_err(|_| "Not a database")?;
    request.set_onupgradeneeded(None);
    drop(upgrade);

    // Listen before reading, and hold what other tabs keep meanwhile until
    // the files are installed, so no change falls between the two.
    EARLY.set(Some(Vec::new()));
    let channel = web_sys::BroadcastChannel::new(CHANNEL).ok();
    if let Some(channel) = &channel {
        let heard = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(heard);
        channel.set_onmessage(Some(heard.as_ref().unchecked_ref()));
        // The channel lives as long as the page.
        heard.forget();
    }

    // Both requests go to the transaction before either finishes, which
    // would end it.
    let store = database
        .transaction_with_str(FILES)
        .and_then(|transaction| transaction.object_store(FILES))
        .map_err(describe)?;
    let keys = store.get_all_keys().map_err(describe)?;
    let values = store.get_all().map_err(describe)?;
    let keys = js_sys::Array::from(&finished(&keys).await?);
    let values = js_sys::Array::from(&finished(&values).await?);
    let files: Vec<(String, Vec<u8>)> = keys
        .iter()
        .zip(values.iter())
        .filter_map(|(key, value)| {
            let bytes = value.dyn_into::<js_sys::Uint8Array>().ok()?.to_vec();
            Some((key.as_string()?, bytes))
        })
        .collect();

    browser::install(
        files,
        Box::new(move |changes| {
            let changes = changes
                .iter()
                .map(|change| Change {
                    key: change.key.to_owned(),
                    bytes: change.bytes.map(<[u8]>::to_vec),
                    previous: change.previous.map(<[u8]>::to_vec),
                })
                .collect();
            commit(&database, changes, channel.clone());
        }),
    );
    for (key, bytes) in EARLY.take().unwrap_or_default() {
        browser::changed(&key, bytes);
    }
    Ok(())
}

/// A write (new bytes) or removal (none) of one file, and what the page held
/// before it.
struct Change {
    key: String,
    bytes: Option<Vec<u8>>,
    previous: Option<Vec<u8>>,
}

/// Keep `changes` in one transaction, all or none, and only if each stored
/// file is still what the page held; then tell other tabs. Transactions on
/// one store run in the order they start, so this page's own changes keep
/// their order.
fn commit(
    database: &IdbDatabase,
    changes: Vec<Change>,
    channel: Option<web_sys::BroadcastChannel>,
) {
    let started = database
        .transaction_with_str_and_mode(FILES, IdbTransactionMode::Readwrite)
        .and_then(|transaction| {
            let store = transaction.object_store(FILES)?;
            let stored = changes
                .iter()
                .map(|change| store.get(&change.key.as_str().into()))
                .collect::<Result<Vec<_>, _>>()?;
            Ok((transaction, store, stored))
        });
    let (transaction, store, stored) = match started {
        Ok(started) => started,
        Err(thrown) => return failed(changes, &describe(thrown)),
    };
    let done =
        wasm_bindgen_futures::JsFuture::from(js_sys::Promise::new(&mut |resolve, reject| {
            transaction.set_oncomplete(Some(&resolve));
            transaction.set_onabort(Some(&reject));
        }));
    wasm_bindgen_futures::spawn_local(async move {
        // This runs in each read's success event, while the transaction is
        // still open for the writes.
        let mut held = Vec::with_capacity(stored.len());
        for request in &stored {
            match finished(request).await {
                Ok(value) => held.push(
                    value
                        .dyn_into::<js_sys::Uint8Array>()
                        .ok()
                        .map(|bytes| bytes.to_vec()),
                ),
                Err(message) => {
                    let _ = transaction.abort();
                    let _ = done.await;
                    return failed(changes, &message);
                }
            }
        }
        if changes
            .iter()
            .zip(&held)
            .any(|(change, held)| change.previous != *held)
        {
            let _ = transaction.abort();
            let _ = done.await;
            let key = changes
                .first()
                .map(|change| change.key.clone())
                .unwrap_or_default();
            let message = format!("Not saved: another tab changed {key} first. Try again.");
            for (change, held) in changes.iter().zip(held) {
                browser::settle(&change.key, change.bytes.as_deref(), held);
                notify(&change.key, Some(message.clone()));
            }
            return;
        }
        for change in &changes {
            let key = JsValue::from(change.key.as_str());
            let written = match &change.bytes {
                Some(bytes) => store
                    .put_with_key(&js_sys::Uint8Array::from(bytes.as_slice()), &key)
                    .map(drop),
                None => store.delete(&key).map(drop),
            };
            if let Err(thrown) = written {
                let _ = transaction.abort();
                let _ = done.await;
                return failed(changes, &describe(thrown));
            }
        }
        if done.await.is_err() {
            let reason = transaction
                .error()
                .map(|exception| exception.message())
                .unwrap_or_else(|| "the browser refused it".into());
            return failed(changes, &reason);
        }
        if let Some(channel) = channel {
            for change in &changes {
                let _ = channel.post_message(&message(&change.key, change.bytes.as_deref()));
            }
        }
    });
}

/// Changes that were not kept, as when storage is full: the page goes back
/// to what it held, and the editor says so.
fn failed(changes: Vec<Change>, reason: &str) {
    let key = changes
        .first()
        .map(|change| change.key.clone())
        .unwrap_or_default();
    let message = format!("Could not keep {key} in browser storage: {reason}");
    for change in changes {
        browser::settle(&change.key, change.bytes.as_deref(), change.previous);
        notify(&change.key, Some(message.clone()));
    }
}

/// Tell the editor that the file `key` changed under it, with a message for
/// the status bar when one of this page's own changes was not kept.
fn notify(key: &str, failure: Option<String>) {
    if let Some(failure) = &failure {
        error(failure);
    }
    let _ = super::send(super::Request::Stored {
        key: key.to_owned(),
        failure,
    });
}

/// `{ key, bytes }`, with null bytes for a removal.
fn message(key: &str, bytes: Option<&[u8]>) -> JsValue {
    let message = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&message, &"key".into(), &key.into());
    let bytes = bytes.map_or(JsValue::NULL, |bytes| {
        js_sys::Uint8Array::from(bytes).into()
    });
    let _ = js_sys::Reflect::set(&message, &"bytes".into(), &bytes);
    message.into()
}

/// Another tab's change, which it has already kept.
fn heard(event: web_sys::MessageEvent) {
    let data = event.data();
    let Some(key) = js_sys::Reflect::get(&data, &"key".into())
        .ok()
        .and_then(|key| key.as_string())
    else {
        return;
    };
    let bytes = js_sys::Reflect::get(&data, &"bytes".into())
        .ok()
        .and_then(|bytes| bytes.dyn_into::<js_sys::Uint8Array>().ok())
        .map(|bytes| bytes.to_vec());
    let early = EARLY.with_borrow_mut(|early| {
        early
            .as_mut()
            .map(|early| early.push((key.clone(), bytes.clone())))
            .is_some()
    });
    if !early {
        browser::changed(&key, bytes);
        notify(&key, None);
    }
}
