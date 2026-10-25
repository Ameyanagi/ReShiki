//! Application data in the browser (see `reshiki::storage::browser`): files by
//! key in one IndexedDB object store, loaded before the app starts and kept
//! after each change. A change is kept only if the stored file is still what
//! the tab held, so a tab never saves over another tab's newer file, and the
//! site's other tabs hear of it through a BroadcastChannel.
//!
//! Each page has a random session, which names its recovery drafts, and holds
//! a Web Lock for it while open, so a new page offers only drafts of pages
//! that have closed. Fonts the page adds are kept here too, and added again at
//! the next mount when text needs them, as the page's own fallback would be.
//! They stay out of the files held in memory and are read only then.
use super::error;
use reshiki::storage::browser;
use std::{cell::RefCell, collections::HashMap};
use wasm_bindgen::{JsCast, prelude::*};
use web_sys::{
    IdbDatabase, IdbKeyRange, IdbOpenDbRequest, IdbRequest, IdbTransaction, IdbTransactionMode,
};

const DATABASE: &str = "reshiki";
const FILES: &str = "files";
const CHANNEL: &str = "reshiki-storage";
/// The folder of kept fonts, each named by its SHA-256: their keys run from
/// here up to [`AFTER_FONTS`], as '0' follows '/'.
const FONTS: &str = "fonts/";
const AFTER_FONTS: &str = "fonts0";

#[derive(Default)]
struct Fonts {
    /// Faces of each font added, by its SHA-256, so a font added again (by
    /// the page at each mount, or kept from before) is added once.
    added: HashMap<String, usize>,
    /// Fonts added before storage opened, kept once it does; the copies are
    /// JavaScript's, not this module's memory.
    waiting: Vec<(String, js_sys::Uint8Array)>,
    open: bool,
    /// The SHA-256 of each font kept from earlier visits, which is read from
    /// storage once text needs it.
    kept: Vec<String>,
    /// Kept fonts are being read and are not added yet.
    adding: bool,
}

/// A file another tab kept: its key and contents, or none once removed.
type Heard = (String, Option<Vec<u8>>);

thread_local! {
    static FONTS_ADDED: RefCell<Fonts> = RefCell::default();
    /// Other tabs' changes heard while the files load, until they are in.
    static EARLY: RefCell<Option<Vec<Heard>>> = const { RefCell::new(None) };
    /// The open database, which fonts are kept in and read from directly.
    static OPENED: RefCell<Option<IdbDatabase>> = const { RefCell::new(None) };
}

/// Open the page's storage, hand its files to the app and add the fonts kept
/// in it. Without storage, as in some private windows, the app starts with
/// none and its saves say that browser storage is unavailable.
pub(super) async fn open() {
    match load().await {
        // Added only once text needs them: a font added before the app starts
        // also becomes a fallback for the interface's own text.
        Ok(kept) => {
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
    let copy = js_sys::Uint8Array::from(bytes.as_slice());
    let faces = add(hash.clone(), bytes)?;
    let open = FONTS_ADDED.with_borrow_mut(|fonts| {
        if !fonts.open {
            fonts.waiting.push((hash.clone(), copy.clone()));
        }
        fonts.open
    });
    if open {
        keep(&hash, &copy);
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

/// Read the kept fonts not added yet from storage, each once, and hand them
/// by SHA-256 to the app, which adds them with [`add_kept`].
pub(super) fn read_kept() {
    let kept = FONTS_ADDED.with_borrow_mut(|fonts| {
        fonts.adding = !fonts.kept.is_empty();
        std::mem::take(&mut fonts.kept)
    });
    if kept.is_empty() {
        return;
    }
    wasm_bindgen_futures::spawn_local(async move {
        let fonts = read_fonts(&kept).await.unwrap_or_else(|message| {
            error(&format!("Could not read the kept fonts: {message}"));
            vec![]
        });
        if super::send(super::Request::AddFonts(fonts)).is_err() {
            FONTS_ADDED.with_borrow_mut(|fonts| fonts.adding = false);
        }
    });
}

/// The fonts kept as `hashes` that storage still holds, by SHA-256.
async fn read_fonts(hashes: &[String]) -> Result<Vec<(String, Vec<u8>)>, String> {
    let database = OPENED.with_borrow(Clone::clone).ok_or("Not open")?;
    let store = database
        .transaction_with_str(FILES)
        .and_then(|transaction| transaction.object_store(FILES))
        .map_err(describe)?;
    let requests = hashes
        .iter()
        .map(|hash| store.get(&font_key(hash).into()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(describe)?;
    let mut fonts = Vec::with_capacity(hashes.len());
    for (hash, request) in hashes.iter().zip(&requests) {
        if let Ok(bytes) = finished(request).await?.dyn_into::<js_sys::Uint8Array>() {
            fonts.push((hash.clone(), bytes.to_vec()));
        }
    }
    Ok(fonts)
}

/// Add fonts from [`read_kept`].
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

fn font_key(hash: &str) -> String {
    format!("{FONTS}{hash}.font")
}

/// Keep a font in storage, unless it is kept already. Its key names its
/// contents, so it is written as is, without the checks of [`commit`].
fn keep(hash: &str, bytes: &js_sys::Uint8Array) {
    let kept =
        FONTS_ADDED.with_borrow_mut(|fonts| crate::kept_fonts::forget(&mut fonts.kept, hash));
    let Some(database) = OPENED.with_borrow(Clone::clone).filter(|_| !kept) else {
        return;
    };
    let written = database
        .transaction_with_str_and_mode(FILES, IdbTransactionMode::Readwrite)
        .and_then(|transaction| {
            let store = transaction.object_store(FILES)?;
            store.put_with_key(bytes, &font_key(hash).into())?;
            Ok(transaction)
        });
    match written {
        Ok(transaction) => wasm_bindgen_futures::spawn_local(async move {
            if let Err(message) = completed(&transaction).await {
                error(&format!("Could not keep the font: {message}"));
            }
        }),
        Err(thrown) => error(&format!("Could not keep the font: {}", describe(thrown))),
    }
}

/// Once `transaction` completes, or why it did not.
async fn completed(transaction: &IdbTransaction) -> Result<(), String> {
    let done = js_sys::Promise::new(&mut |resolve, reject| {
        transaction.set_oncomplete(Some(&resolve));
        transaction.set_onabort(Some(&reject));
    });
    wasm_bindgen_futures::JsFuture::from(done)
        .await
        .map(drop)
        .map_err(|_| {
            transaction
                .error()
                .map(|exception| exception.message())
                .unwrap_or_else(|| "the browser refused it".into())
        })
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

/// Open the database, read every file but the kept fonts and install them
/// with a sink that keeps each change and tells other tabs. Returns the
/// SHA-256 of each kept font.
async fn load() -> Result<Vec<String>, String> {
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

    // The files before and after the fonts, and the fonts' keys only. Every
    // request goes to the transaction before any finishes, which would end
    // it.
    let (others, fonts) = database
        .transaction_with_str(FILES)
        .and_then(|transaction| transaction.object_store(FILES))
        .and_then(|store| {
            let (fonts, after) = (JsValue::from(FONTS), JsValue::from(AFTER_FONTS));
            let others = [
                IdbKeyRange::upper_bound_with_open(&fonts, true)?,
                IdbKeyRange::lower_bound(&after)?,
            ]
            .iter()
            .map(|range| {
                Ok((
                    store.get_all_keys_with_key(range)?,
                    store.get_all_with_key(range)?,
                ))
            })
            .collect::<Result<Vec<_>, JsValue>>()?;
            let range =
                IdbKeyRange::bound_with_lower_open_and_upper_open(&fonts, &after, false, true)?;
            Ok((others, store.get_all_keys_with_key(&range)?))
        })
        .map_err(describe)?;
    let mut files = Vec::new();
    for (keys, values) in &others {
        let keys = js_sys::Array::from(&finished(keys).await?);
        let values = js_sys::Array::from(&finished(values).await?);
        files.extend(keys.iter().zip(values.iter()).filter_map(|(key, value)| {
            let bytes = value.dyn_into::<js_sys::Uint8Array>().ok()?.to_vec();
            Some((key.as_string()?, bytes))
        }));
    }
    let kept = js_sys::Array::from(&finished(&fonts).await?)
        .iter()
        .filter_map(|key| {
            let key = key.as_string()?;
            Some(key.strip_prefix(FONTS)?.strip_suffix(".font")?.to_owned())
        })
        .collect();
    OPENED.set(Some(database.clone()));

    let session = session();
    let open = open_sessions(&window, &session, true).await;
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
        session,
        open.clone().unwrap_or_default(),
    );
    let verified = open.is_some();
    browser::set_session_lock(verified);
    browser::set_open_sessions(open);
    if !verified {
        notify("recovery/ownership", Some(LOCK_WARNING.into()));
    }
    for (key, bytes) in EARLY.take().unwrap_or_default() {
        browser::changed(&key, bytes);
    }
    Ok(kept)
}

/// The prefix of the Web Lock each open page holds for its session.
const SESSION_LOCK: &str = "reshiki-session:";
const LOCK_WARNING: &str = "Recovery saving is unavailable because the browser could not keep this page’s session lock. Save drawings to keep a copy.";

/// A new random session, in hexadecimal so it fits a draft's name.
fn session() -> String {
    let part = || (js_sys::Math::random() * 2f64.powi(52)) as u64;
    format!("{:013x}{:013x}", part(), part())
}

/// The sessions of the site's other open pages, which hold their session's
/// Web Lock until they close, and then this page's lock for `session`. A
/// browser without Web Locks protects every draft. `hold` registers this page
/// once at startup; later checks only query the locks.
async fn open_sessions(window: &web_sys::Window, session: &str, hold: bool) -> Option<Vec<String>> {
    use js_sys::{Array, Function, Promise, Reflect};
    let locks = Reflect::get(&window.navigator(), &"locks".into())
        .ok()
        .filter(|locks| locks.is_object())?;
    let method = |name: &str| {
        Reflect::get(&locks, &name.into())
            .ok()
            .and_then(|method| method.dyn_into::<Function>().ok())
    };
    let (Some(query), Some(request)) = (method("query"), method("request")) else {
        return None;
    };
    let held = match query
        .call0(&locks)
        .map(|promise| promise.dyn_into::<Promise>())
    {
        Ok(Ok(promise)) => wasm_bindgen_futures::JsFuture::from(promise).await.ok(),
        _ => None,
    };
    let open = held?;
    let open = Array::from(&Reflect::get(&open, &"held".into()).ok()?)
        .to_vec()
        .into_iter()
        .filter_map(|lock| Reflect::get(&lock, &"name".into()).ok()?.as_string())
        .filter_map(|name| name.strip_prefix(SESSION_LOCK).map(str::to_owned))
        .collect();
    // Hold this page's lock until it closes: the callback's promise never
    // settles.
    if hold {
        let (acquired, ready) = iced::futures::channel::oneshot::channel();
        let acquired = std::rc::Rc::new(RefCell::new(Some(acquired)));
        let failed = std::rc::Rc::new(std::cell::Cell::new(false));
        let callback = Closure::<dyn FnMut() -> Promise>::new({
            let acquired = acquired.clone();
            move || {
                if let Some(acquired) = acquired.borrow_mut().take() {
                    let _ = acquired.send(true);
                }
                Promise::new(&mut |_, _| {})
            }
        });
        let pending = request
            .call2(
                &locks,
                &format!("{SESSION_LOCK}{session}").into(),
                callback.as_ref(),
            )
            .ok()?
            .dyn_into::<Promise>()
            .ok()?;
        let rejected = Closure::<dyn FnMut(JsValue)>::new({
            let failed = failed.clone();
            move |_| {
                failed.set(true);
                browser::set_session_lock(false);
                browser::set_open_sessions(None);
                if let Some(acquired) = acquired.borrow_mut().take() {
                    let _ = acquired.send(false);
                }
                notify("recovery/ownership", Some(LOCK_WARNING.into()));
            }
        });
        // Consume asynchronous rejection too; a request returning a Promise
        // is not evidence that its callback acquired the lock.
        let _ = pending.catch(&rejected);
        callback.forget();
        rejected.forget();
        if !ready.await.ok()? || failed.get() {
            return None;
        }
    }
    Some(open)
}

/// A destructive recovery action waits for IndexedDB, unlike routine queued
/// writes. Keep the offered copies in memory and in the UI on any failure.
pub(super) async fn discard_recovery(
    candidates: &[reshiki::recovery::Candidate],
) -> Result<Vec<std::path::PathBuf>, String> {
    refresh_recovery_owners().await?;
    discard_verified(candidates).await
}

/// A failed query protects unknown owners without revoking our own held lock.
pub(super) async fn refresh_recovery_owners() -> Result<(), String> {
    let window = web_sys::window().ok_or("The page has no window")?;
    let session = browser::session().ok_or("Browser storage is unavailable")?;
    let open = open_sessions(&window, &session, false).await;
    browser::set_open_sessions(open.clone());
    open.ok_or("Could not verify which ReShiki pages are open; no recovery drafts were deleted")?;
    Ok(())
}

async fn discard_verified(
    candidates: &[reshiki::recovery::Candidate],
) -> Result<Vec<std::path::PathBuf>, String> {
    let database = OPENED
        .with_borrow(Clone::clone)
        .ok_or("Browser storage is unavailable")?;
    let mut copies = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        if !candidate.available() {
            return Err(
                "A recovery draft belongs to an open ReShiki page; no drafts were deleted".into(),
            );
        }
        let key = browser::key(&candidate.path)
            .filter(|key| key.starts_with("recovery/"))
            .ok_or("Not a recovery draft in browser storage")?;
        let bytes = reshiki::storage::read(&candidate.path).map_err(|error| error.to_string())?;
        let mut snapshot: reshiki::recovery::Snapshot =
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        snapshot.document.validate()?;
        snapshot.document.migrate();
        if snapshot.saved_at != candidate.snapshot.saved_at
            || snapshot.source != candidate.snapshot.source
            || snapshot.document != candidate.snapshot.document
        {
            return Err("A recovery draft changed; reload ReShiki to review it again".into());
        }
        copies.push((candidate.path.clone(), key, bytes));
    }
    let transaction = database
        .transaction_with_str_and_mode(FILES, IdbTransactionMode::Readwrite)
        .map_err(describe)?;
    let store = transaction.object_store(FILES).map_err(describe)?;
    let requests = copies
        .iter()
        .map(|(_, key, _)| store.get(&key.as_str().into()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(describe)?;
    let done =
        wasm_bindgen_futures::JsFuture::from(js_sys::Promise::new(&mut |resolve, reject| {
            transaction.set_oncomplete(Some(&resolve));
            transaction.set_onabort(Some(&reject));
        }));
    for ((_, _, previous), request) in copies.iter().zip(&requests) {
        let stored = match finished(request).await {
            Ok(value) => value
                .dyn_into::<js_sys::Uint8Array>()
                .ok()
                .map(|bytes| bytes.to_vec()),
            Err(error) => {
                let _ = transaction.abort();
                let _ = done.await;
                return Err(error);
            }
        };
        if stored.as_deref() != Some(previous.as_slice()) {
            let _ = transaction.abort();
            let _ = done.await;
            return Err("Another page changed a recovery draft; no drafts were deleted. Reload to review them again".into());
        }
    }
    for (_, key, _) in &copies {
        if let Err(error) = store.delete(&key.as_str().into()) {
            let _ = transaction.abort();
            let _ = done.await;
            return Err(describe(error));
        }
    }
    if done.await.is_err() {
        return Err(transaction
            .error()
            .map(|error| error.message())
            .unwrap_or_else(|| "The browser refused deletion".into()));
    }
    let channel = web_sys::BroadcastChannel::new(CHANNEL).ok();
    for (_, key, _) in &copies {
        browser::changed(key, None);
        if let Some(channel) = &channel {
            let _ = channel.post_message(&message(key, None));
        }
    }
    Ok(copies.into_iter().map(|(path, _, _)| path).collect())
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
    // Kept fonts stay out of memory; an earlier version still tells of them.
    if key.starts_with(FONTS) {
        return;
    }
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
