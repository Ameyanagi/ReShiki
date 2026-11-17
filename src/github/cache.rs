//! GitHub's replies kept on disk between runs, one folder per account.
//!
//! It lies beneath the session's [`super::client::Replies`]. A folder per
//! account keeps one account from reading another's private repositories. Up
//! to [`CAP`] bytes in all, the least recently used going first. An entry that
//! cannot be read whole is a miss and is removed: nothing here is ever an error
//! the user sees.
use super::workcopy::hash;
use std::{
    collections::BTreeMap,
    io::Write,
    path::{Path, PathBuf},
    sync::{Mutex, PoisonError},
    time::SystemTime,
};

/// The most bytes kept, for every account together.
const CAP: u64 = 200 << 20;

/// Signed out. No GitHub login has an underscore, so no account shares it.
pub(super) const PUBLIC: &str = "_public";

/// `<cache>/github/`, or `github-override/` for a GitHub set by
/// `RESHIKI_GITHUB_API`, so a test server's replies never pass for GitHub's.
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
    pub(super) fn new(cache: &Path, overridden: bool) -> Self {
        let name = if overridden {
            "github-override"
        } else {
            "github"
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
        let (etag, bytes) = (etag.unwrap_or_default().to_owned(), bytes.to_vec());
        let _ = crate::task::spawn_blocking(move || {
            let head = format!("{}\n{etag}\n", hash(&bytes));
            if store(&file, &head, &bytes).is_ok() {
                disk.counted((head.len() + bytes.len()) as u64);
            }
        })
        .await;
    }

    /// Counts `added` bytes just written. Past the cap, the least recently
    /// used go until nine tenths of it is left, so the next writes list
    /// nothing.
    fn counted(&self, added: u64) {
        let mut used = USED.lock().unwrap_or_else(PoisonError::into_inner);
        let total = match used.get_mut(&self.root) {
            Some(total) => {
                *total += added;
                *total
            }
            // The listing holds this write already.
            None => entries(&self.root).iter().map(|(_, size, _)| size).sum(),
        };
        let total = if total > self.cap {
            self.evict()
        } else {
            total
        };
        used.insert(self.root.clone(), total);
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

/// An entry is `<SHA-256 of the reply>\n<ETag>\n<reply>`; its modified time
/// is when it was last used.
fn load(file: &Path) -> Option<Kept> {
    let kept = std::fs::read(file).ok().and_then(|data| parse(&data));
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
    let end = rest.iter().position(|&byte| byte == b'\n')?;
    let (etag, bytes) = (&rest[..end], &rest[end + 1..]);
    if sum != hash(bytes).as_bytes() {
        return None;
    }
    let etag = String::from_utf8(etag.to_vec()).ok()?;
    Some(Kept {
        etag: (!etag.is_empty()).then_some(etag),
        bytes: bytes.to_vec(),
    })
}

/// Written to a temporary file renamed over the entry, so a reader finds the
/// old entry or the new one. No `fsync`: an entry a crash cut short fails its
/// sum.
fn store(file: &Path, head: &str, bytes: &[u8]) -> std::io::Result<()> {
    let folder = file.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(folder)?;
    let mut temp = crate::storage::temporary_in(folder, 0o600)?;
    temp.write_all(head.as_bytes())?;
    temp.write_all(bytes)?;
    temp.as_file().set_modified(SystemTime::now())?;
    temp.persist(file)?;
    Ok(())
}

/// Every file in every account folder: when it was last used, its size.
fn entries(root: &Path) -> Vec<(SystemTime, u64, PathBuf)> {
    let mut found = Vec::new();
    for account in std::fs::read_dir(root).into_iter().flatten().flatten() {
        for entry in (std::fs::read_dir(account.path()).into_iter().flatten()).flatten() {
            if let Ok(metadata) = entry.metadata()
                && metadata.is_file()
                && let Ok(used) = metadata.modified()
            {
                found.push((used, metadata.len(), entry.path()));
            }
        }
    }
    found
}
