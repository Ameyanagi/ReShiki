//! Files the app writes for itself and for the user. Application data (settings,
//! templates, themes, abbreviations, recovery drafts) goes through [`read`],
//! [`read_dir`], [`write_atomic`] and the other functions here, which are the
//! file system on the desktop. The browser has no files: there, paths under
//! [`BROWSER_ROOT`] live in the page's storage (see `browser`), and every
//! other path fails as `std::fs` does in a browser.
use std::{
    io::{self, Write},
    path::{Path, PathBuf},
};

/// The application data folder in the browser, as messages name it.
pub const BROWSER_ROOT: &str = "Browser storage";

/// [`BROWSER_ROOT`] in the browser; none on the desktop, which keeps its
/// application data in the platform's folder.
pub fn browser_data_directory() -> Option<PathBuf> {
    cfg!(target_arch = "wasm32").then(|| PathBuf::from(BROWSER_ROOT))
}

/// Preserve explicit suffixes; supply the format suffix when a save name has none.
pub fn with_default_extension(path: &Path, extension: &str) -> PathBuf {
    if path.extension().is_none_or(|suffix| suffix.is_empty()) {
        path.with_extension(extension)
    } else {
        path.to_path_buf()
    }
}

/// Replace only after the complete file has been written beside its destination.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    if let Some(key) = browser::key(path) {
        return browser::write(&key, bytes);
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    temp.write_all(bytes).map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    temp.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}

/// [`write_atomic`] `path`, then remove `old` when given; an `old` already
/// gone is fine. In the browser both are kept together or not at all, so a
/// failed write never loses `old`.
pub fn write_replacing(path: &Path, bytes: &[u8], old: Option<&Path>) -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    if let Some(key) = browser::key(path) {
        let old = old
            .and_then(browser::key)
            .filter(|old| *old != key && browser::read(old).is_ok());
        return match old {
            Some(old) => browser::apply(&[(&key, Some(bytes)), (&old, None)]),
            None => browser::write(&key, bytes),
        };
    }
    write_atomic(path, bytes)?;
    match old.map(remove_file) {
        Some(Err(error)) if error.kind() != io::ErrorKind::NotFound => Err(error.to_string()),
        _ => Ok(()),
    }
}

/// A file's contents, as [`std::fs::read`].
pub fn read(path: &Path) -> io::Result<Vec<u8>> {
    #[cfg(target_arch = "wasm32")]
    if let Some(key) = browser::key(path) {
        return browser::read(&key);
    }
    std::fs::read(path)
}

/// The paths in a folder, skipping entries that cannot be read, as
/// [`std::fs::read_dir`].
pub fn read_dir(path: &Path) -> io::Result<Vec<PathBuf>> {
    #[cfg(target_arch = "wasm32")]
    if let Some(key) = browser::key(path) {
        return Ok(browser::list(&key)
            .into_iter()
            .map(|name| path.join(name))
            .collect());
    }
    Ok(std::fs::read_dir(path)?
        .flatten()
        .map(|entry| entry.path())
        .collect())
}

/// As [`std::fs::create_dir_all`]; browser storage has no folders to make.
pub fn create_dir_all(path: &Path) -> io::Result<()> {
    #[cfg(target_arch = "wasm32")]
    if browser::key(path).is_some() {
        return Ok(());
    }
    std::fs::create_dir_all(path)
}

/// As [`std::fs::remove_file`].
pub fn remove_file(path: &Path) -> io::Result<()> {
    #[cfg(target_arch = "wasm32")]
    if let Some(key) = browser::key(path) {
        return browser::remove(&key);
    }
    std::fs::remove_file(path)
}

/// As [`std::fs::rename`] for a file.
pub fn rename(from: &Path, to: &Path) -> io::Result<()> {
    #[cfg(target_arch = "wasm32")]
    if let (Some(from), Some(to)) = (browser::key(from), browser::key(to)) {
        return browser::rename(&from, &to);
    }
    std::fs::rename(from, to)
}

/// Application data in the browser: files by their path under
/// [`BROWSER_ROOT`], held in memory for the app's synchronous reads. The page
/// loads them from its storage before the app starts, keeps each later change
/// (see the app's `web/storage.rs`) and passes on other tabs' changes. Until
/// then, or when the browser offers no storage, writes fail.
#[cfg(any(target_arch = "wasm32", test))]
pub mod browser {
    use std::{
        cell::RefCell,
        collections::{BTreeMap, BTreeSet},
        io,
        path::Path,
    };

    /// One file's change: its new contents, or none to remove it, and what
    /// the page held before, so a change another page kept first is not
    /// overwritten (see [`settle`]).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Change<'a> {
        pub key: &'a str,
        pub bytes: Option<&'a [u8]>,
        pub previous: Option<&'a [u8]>,
    }

    /// Keeps changes, all of them or none, in the page's storage.
    pub type Sink = Box<dyn Fn(&[Change<'_>])>;

    #[derive(Default)]
    struct Store {
        files: BTreeMap<String, Vec<u8>>,
        sink: Option<Sink>,
        /// This page's session, which names its recovery drafts.
        session: String,
        /// The sessions of the site's other open pages, whose drafts are not
        /// offered for recovery.
        open: BTreeSet<String>,
        /// Without a successful Web Locks query, other pages' drafts stay
        /// protected instead of guessing that their owners have closed.
        owners_verified: bool,
        /// Querying other owners does not revoke this page's held lock.
        session_locked: bool,
    }

    thread_local! {
        static STORE: RefCell<Store> = RefCell::default();
    }

    /// Start with `files` from the page's storage, by key, and keep each later
    /// change with `sink`. `session` names this page, and `open` the site's
    /// other pages that are still open.
    pub fn install(
        files: impl IntoIterator<Item = (String, Vec<u8>)>,
        sink: Sink,
        session: String,
        open: impl IntoIterator<Item = String>,
    ) {
        STORE.with_borrow_mut(|store| {
            store.files = files.into_iter().collect();
            store.sink = Some(sink);
            store.session = session;
            store.open = open.into_iter().collect();
            store.owners_verified = true;
            store.session_locked = true;
        });
    }

    /// This page's protected recovery owner, or none without its held lock.
    pub fn session() -> Option<String> {
        STORE.with_borrow(|store| {
            Some(store.session.clone()).filter(|s| store.session_locked && !s.is_empty())
        })
    }

    /// Whether `session` is this page or another open page of the site.
    pub fn open(session: &str) -> bool {
        STORE.with_borrow(|store| {
            !store.owners_verified || store.session == session || store.open.contains(session)
        })
    }

    /// Refresh the known live pages. A failed query protects all owners.
    pub fn set_open_sessions(open: Option<Vec<String>>) {
        STORE.with_borrow_mut(|store| {
            store.owners_verified = open.is_some();
            store.open = open.unwrap_or_default().into_iter().collect();
        });
    }

    pub fn owners_verified() -> bool {
        STORE.with_borrow(|store| store.owners_verified)
    }

    /// Set only after this page's own acquisition succeeds or fails.
    pub fn set_session_lock(locked: bool) {
        STORE.with_borrow_mut(|store| store.session_locked = locked);
    }

    /// A change another tab made and kept, which this one only remembers.
    pub fn changed(key: &str, bytes: Option<Vec<u8>>) {
        STORE.with_borrow_mut(|store| match bytes {
            Some(bytes) => store.files.insert(key.to_owned(), bytes),
            None => store.files.remove(key),
        });
    }

    /// A change that could not be kept: the file goes back to `kept`, what
    /// the page's storage holds, unless a later change replaced `failed`.
    pub fn settle(key: &str, failed: Option<&[u8]>, kept: Option<Vec<u8>>) {
        STORE.with_borrow_mut(|store| {
            if store.files.get(key).map(Vec::as_slice) != failed {
                return;
            }
            match kept {
                Some(bytes) => store.files.insert(key.to_owned(), bytes),
                None => store.files.remove(key),
            };
        });
    }

    /// The keys and contents of the files anywhere under the folder `prefix`
    /// ("" for all).
    pub fn files(prefix: &str) -> Vec<(String, Vec<u8>)> {
        STORE.with_borrow(|store| {
            store
                .files
                .iter()
                .filter(|(key, _)| under(key, prefix).is_some())
                .map(|(key, bytes)| (key.clone(), bytes.clone()))
                .collect()
        })
    }

    /// `path`'s key, as '/'-separated names under [`super::BROWSER_ROOT`]
    /// ("" for the root itself); none for any other path.
    pub fn key(path: &Path) -> Option<String> {
        let rest = path.strip_prefix(super::BROWSER_ROOT).ok()?;
        let mut names = Vec::new();
        for component in rest.components() {
            match component {
                std::path::Component::Normal(name) => names.push(name.to_str()?),
                std::path::Component::CurDir => {}
                _ => return None,
            }
        }
        Some(names.join("/"))
    }

    /// `key` relative to the folder `prefix`, when it lies inside it.
    fn under<'a>(key: &'a str, prefix: &str) -> Option<&'a str> {
        if prefix.is_empty() {
            return Some(key);
        }
        key.strip_prefix(prefix)?.strip_prefix('/')
    }

    fn missing() -> io::Error {
        io::Error::new(io::ErrorKind::NotFound, "Not in browser storage")
    }

    const UNAVAILABLE: &str = "Browser storage is unavailable";

    pub(super) fn read(key: &str) -> io::Result<Vec<u8>> {
        STORE.with_borrow(|store| store.files.get(key).cloned().ok_or_else(missing))
    }

    /// Make `changes` (new contents, or none to remove a file) here at once
    /// and hand them to the sink to keep together.
    pub(super) fn apply(changes: &[(&str, Option<&[u8]>)]) -> Result<(), String> {
        if changes.iter().any(|(key, _)| key.is_empty()) {
            return Err("Not a file in browser storage".into());
        }
        STORE.with_borrow_mut(|store| {
            let sink = store.sink.as_ref().ok_or(UNAVAILABLE)?;
            if !store.session_locked && changes.iter().any(|(key, bytes)| key.starts_with("recovery/") && bytes.is_some()) {
                return Err("Recovery saving is unavailable because this page does not hold its session lock".into());
            }
            let previous: Vec<_> = changes
                .iter()
                .map(|(key, _)| store.files.get(*key).cloned())
                .collect();
            let kept: Vec<_> = changes
                .iter()
                .zip(&previous)
                .map(|((key, bytes), previous)| Change {
                    key,
                    bytes: *bytes,
                    previous: previous.as_deref(),
                })
                .collect();
            sink(&kept);
            for (key, bytes) in changes {
                match bytes {
                    Some(bytes) => store.files.insert((*key).to_owned(), bytes.to_vec()),
                    None => store.files.remove(*key),
                };
            }
            Ok(())
        })
    }

    pub(super) fn write(key: &str, bytes: &[u8]) -> Result<(), String> {
        apply(&[(key, Some(bytes))])
    }

    pub(super) fn remove(key: &str) -> io::Result<()> {
        read(key)?;
        apply(&[(key, None)]).map_err(io::Error::other)
    }

    /// Both changes are kept together, so the file is never lost or doubled.
    pub(super) fn rename(from: &str, to: &str) -> io::Result<()> {
        let bytes = read(from)?;
        apply(&[(to, Some(&bytes)), (from, None)]).map_err(io::Error::other)
    }

    /// The names of the files directly in the folder `prefix`.
    pub(super) fn list(prefix: &str) -> Vec<String> {
        STORE.with_borrow(|store| {
            store
                .files
                .keys()
                .filter_map(|key| under(key, prefix))
                .filter(|name| !name.contains('/'))
                .map(str::to_owned)
                .collect()
        })
    }
}

#[cfg(test)]
mod tests;
