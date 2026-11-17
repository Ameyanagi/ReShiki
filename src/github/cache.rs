//! GitHub's replies kept on disk between runs, one folder per account.
//!
//! It lies beneath the session's [`super::client::Replies`]. A folder per
//! account keeps one account from reading another's private repositories. Up
//! to [`CAP`] bytes in all, the least recently used going first. An entry that
//! cannot be read whole is a miss and is removed: nothing here is ever an error
//! the user sees.
use super::{config::Endpoints, workcopy::hash};
use std::{
    collections::BTreeMap,
    io::Write,
    path::{Path, PathBuf},
    sync::{Mutex, PoisonError},
    time::SystemTime,
};

/// The most bytes kept, for every account together.
const CAP: u64 = 200 << 20;

/// Signed out. No GitHub login starts with an underscore, and a client never
/// keeps one by this name, so no account shares it.
pub(super) const PUBLIC: &str = "_public";

/// `<cache>/github/`, or `github-<hash of the API address>/` for a GitHub
/// set by `RESHIKI_GITHUB_API` or `RESHIKI_GITHUB_WEB`, so a test server's
/// replies never pass for GitHub's or another server's (and an address's
/// credentials never name a folder).
#[derive(Clone)]
pub(super) struct Disk {
    root: PathBuf,
    cap: u64,
}

/// A reply as kept, with its ETag when it may change and is asked again.
pub(super) struct Kept {
    pub etag: Option<String>,
    pub bytes: Vec<u8>,
}

/// Bytes kept under each root, as far as this process knows: listed at the
/// first write, then counted. Another ReShiki's writes are found at the next
/// eviction, which lists them again.
static USED: Mutex<BTreeMap<PathBuf, u64>> = Mutex::new(BTreeMap::new());

impl Disk {
    pub(super) fn new(cache: &Path, ends: &Endpoints) -> Self {
        let name = match ends.overridden {
            false => "github".to_owned(),
            true => format!("github-{}", &hash(ends.api.as_bytes())[..16]),
        };
        Self {
            root: cache.join(name),
            cap: CAP,
        }
    }

    #[cfg(test)]
    pub(super) fn with_cap(mut self, cap: u64) -> Self {
        self.cap = cap;
        self
    }

    /// The bytes this process counts under the root.
    #[cfg(test)]
    pub(super) fn counted_bytes(&self) -> Option<u64> {
        USED.lock().ok()?.get(&self.root).copied()
    }

    fn file(&self, account: &str, path: &str) -> PathBuf {
        self.root.join(account).join(hash(path.as_bytes()))
    }

    /// The reply kept for `path`, read off the window's thread.
    pub(super) async fn load(&self, account: &str, path: &str) -> Option<Kept> {
        let file = self.file(account, path);
        crate::task::spawn_blocking(move || load(&file))
            .await
            .ok()
            .flatten()
    }

    /// Keeps `bytes` for `path`; a write that fails keeps nothing.
    pub(super) async fn store(&self, account: &str, path: &str, etag: Option<&str>, bytes: &[u8]) {
        let (disk, file) = (self.clone(), self.file(account, path));
        let rest = [etag.unwrap_or_default().as_bytes(), b"\n", bytes].concat();
        let _ = crate::task::spawn_blocking(move || disk.keep(&file, &rest)).await;
    }

    /// Writes a temporary file, then renames it over the entry, so a reader
    /// finds the old entry or the new one. No `fsync`: an entry a crash cut
    /// short fails its sum. The rename and its count happen under one lock,
    /// so each write is counted once. Past the cap, the least recently used
    /// go until nine tenths of it is left, so the next writes list nothing.
    fn keep(&self, file: &Path, rest: &[u8]) -> std::io::Result<()> {
        let folder = file.parent().unwrap_or(Path::new("."));
        std::fs::create_dir_all(folder)?;
        let mut temp = crate::storage::temporary_in(folder, 0o600)?;
        let head = format!("{}\n", hash(rest));
        temp.write_all(head.as_bytes())?;
        temp.write_all(rest)?;
        temp.as_file().set_modified(SystemTime::now())?;
        let mut used = USED.lock().unwrap_or_else(PoisonError::into_inner);
        let total = (used.entry(self.root.clone()))
            .or_insert_with(|| entries(&self.root).iter().map(|(_, size, _)| size).sum());
        let replaced = std::fs::metadata(file).map_or(0, |entry| entry.len());
        temp.persist(file)?;
        *total = (*total + (head.len() + rest.len()) as u64).saturating_sub(replaced);
        if *total > self.cap {
            *total = self.evict();
        }
        Ok(())
    }

    fn evict(&self) -> u64 {
        let mut entries = entries(&self.root);
        let mut total = entries.iter().map(|(_, size, _)| size).sum::<u64>();
        entries.sort_by_key(|(used, ..)| *used);
        for (_, size, file) in entries {
            if total <= self.cap / 10 * 9 {
                break;
            }
            if std::fs::remove_file(file).is_ok() {
                total -= size;
            }
        }
        total
    }
}

/// An entry is `<SHA-256 of the rest>\n<ETag>\n<reply>`; its modified time
/// is when it was last used.
fn load(file: &Path) -> Option<Kept> {
    let kept = match std::fs::read(file) {
        // Nothing to remove: one written meanwhile is left alone.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        read => read.ok().and_then(|data| parse(&data)),
    };
    match kept {
        Some(_) => {
            let _ = std::fs::File::options()
                .write(true)
                .open(file)
                .and_then(|file| file.set_modified(SystemTime::now()));
        }
        // Gone already, or not whole: either way, not kept.
        None => {
            let _ = std::fs::remove_file(file);
        }
    }
    kept
}

fn parse(data: &[u8]) -> Option<Kept> {
    let (sum, rest) = data.split_at_checked(64)?;
    let rest = rest.strip_prefix(b"\n")?;
    if sum != hash(rest).as_bytes() {
        return None;
    }
    let mut parts = rest.splitn(2, |&byte| byte == b'\n');
    let (etag, bytes) = (parts.next()?, parts.next()?);
    let etag = String::from_utf8(etag.to_vec()).ok()?;
    Some(Kept {
        etag: (!etag.is_empty()).then_some(etag),
        bytes: bytes.to_vec(),
    })
}

/// Every entry in every account folder: when it was last used, its size.
/// Links are never followed, so nothing outside the cache is ever removed,
/// and a temporary file still being written is not an entry.
fn entries(root: &Path) -> Vec<(SystemTime, u64, PathBuf)> {
    let mut found = Vec::new();
    let folders = std::fs::read_dir(root).into_iter().flatten().flatten();
    for account in folders.filter(|folder| folder.file_type().is_ok_and(|kind| kind.is_dir())) {
        for entry in (std::fs::read_dir(account.path()).into_iter().flatten()).flatten() {
            let name = entry.file_name();
            let named = (name.to_str()).is_some_and(|name| name.len() == 64);
            if let Ok(metadata) = entry.metadata()
                && named
                && metadata.is_file()
                && let Ok(used) = metadata.modified()
            {
                found.push((used, metadata.len(), entry.path()));
            }
        }
    }
    found
}
