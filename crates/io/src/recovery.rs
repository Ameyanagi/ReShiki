use crate::document::Document;
use serde::{Deserialize, Serialize};
#[cfg(not(target_arch = "wasm32"))]
use std::time::{SystemTime, UNIX_EPOCH};
use std::{
    borrow::Cow,
    path::{Path, PathBuf},
};
#[cfg(target_arch = "wasm32")]
use web_time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub document: Document,
    pub source: Option<PathBuf>,
    pub saved_at: u64,
}
#[derive(Debug, Clone)]
pub struct Candidate {
    pub path: PathBuf,
    pub snapshot: Snapshot,
}
#[derive(Debug, Clone)]
pub struct Recovery {
    pub session: PathBuf,
}
/// What [`Recovery::scan`] found.
#[derive(Debug, Clone, Default)]
pub struct Scan {
    pub candidates: Vec<Candidate>,
    /// Abandoned drafts this version cannot read because a newer one made them.
    pub newer: usize,
}
/// A validated recovery offer without its document. Discovery drops documents
/// after collecting this metadata; only a selected offer is hydrated again.
#[derive(Debug, Clone)]
pub struct Draft {
    pub path: PathBuf,
    pub source: Option<PathBuf>,
    pub saved_at: u64,
    pub atoms: usize,
    identity: [u8; 32],
}

#[derive(Debug, Clone, Default)]
pub struct Discovery {
    pub drafts: Vec<Draft>,
    pub newer: usize,
}

impl Draft {
    fn from_candidate(candidate: Candidate, bytes: &[u8]) -> Self {
        Self {
            identity: identity(bytes),
            path: candidate.path,
            source: candidate.snapshot.source,
            saved_at: candidate.snapshot.saved_at,
            atoms: candidate.snapshot.document.atoms.len(),
        }
    }

    pub fn available(&self) -> bool {
        !owned_by_running_instance(&self.path)
    }

    /// Hydrate only the offered snapshot. Both ownership and identity are
    /// checked again, including migrations, before the caller can act on it.
    pub fn load(&self) -> Result<Candidate, String> {
        if !self.available() {
            return Err("The draft belongs to a running ReShiki instance".into());
        }
        #[cfg(not(target_arch = "wasm32"))]
        if !std::fs::symlink_metadata(&self.path)
            .map_err(|error| error.to_string())?
            .is_file()
        {
            return Err("The recovery draft is no longer a regular file".into());
        }
        let bytes = crate::storage::read(&self.path).map_err(|error| error.to_string())?;
        if identity(&bytes) != self.identity {
            return Err("The recovery draft changed; reopen ReShiki to review it again".into());
        }
        let mut snapshot: Snapshot =
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        snapshot.document.validate()?;
        snapshot.document.migrate();
        if !self.available() {
            return Err("The draft belongs to a running ReShiki instance".into());
        }
        Ok(Candidate {
            path: self.path.clone(),
            snapshot,
        })
    }
}

// Keep exact file identity without retaining a document or a second file-sized
// buffer. Replaced/edited files must be reviewed again before restore/discard.
fn identity(bytes: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes).into()
}

impl Candidate {
    /// Only an abandoned draft may be restored or discarded. Recheck before
    /// acting: a process ID may have been reused since the startup scan.
    pub fn available(&self) -> bool {
        !owned_by_running_instance(&self.path)
    }
}

pub(crate) fn owned_by_running_instance(path: &Path) -> bool {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(|stem| stem.split('-').next())
        .is_some_and(running)
}

impl Recovery {
    pub fn standard() -> Result<Self, String> {
        let root = crate::compatibility::data_directory()?;
        Self::in_directory(&root.join("recovery"))
    }
    /// A new draft file in `root`, named after this process (in the browser,
    /// this page). Each document tab has its own, so the count keeps drafts
    /// made within one clock tick apart.
    pub fn in_directory(root: &Path) -> Result<Self, String> {
        static COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let owner = owner()?;
        crate::storage::create_dir_all(root).map_err(|e| e.to_string())?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let count = COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(Self {
            session: root.join(format!("{owner}-{now}-{count}.json")),
        })
    }
    pub fn candidates(&self) -> Vec<Candidate> {
        self.scan().candidates
    }
    /// The abandoned drafts to offer, newest first, and how many more were
    /// made with a newer ReShiki.
    pub fn scan(&self) -> Scan {
        let mut candidates = Vec::new();
        let newer = self.scan_each(|candidate, _bytes| candidates.push(candidate));
        candidates.sort_by(|a, b| {
            b.snapshot
                .saved_at
                .cmp(&a.snapshot.saved_at)
                .then_with(|| a.path.cmp(&b.path))
        });
        Scan { candidates, newer }
    }

    /// Validate abandoned drafts one at a time, keeping only picker metadata.
    /// Native callers should run this filesystem/parse work on a worker.
    pub fn discover(&self) -> Discovery {
        let mut drafts = Vec::new();
        let newer =
            self.scan_each(|candidate, bytes| drafts.push(Draft::from_candidate(candidate, bytes)));
        drafts.sort_by(|a, b| {
            b.saved_at
                .cmp(&a.saved_at)
                .then_with(|| a.path.cmp(&b.path))
        });
        Discovery { drafts, newer }
    }

    fn scan_each(&self, mut accept: impl FnMut(Candidate, &[u8])) -> usize {
        let Some(parent) = self.session.parent() else {
            return 0;
        };
        let Ok(entries) = crate::storage::read_dir(parent) else {
            return 0;
        };
        let mut newer = 0;
        for path in entries {
            if path == self.session
                || path.extension().and_then(|e| e.to_str()) != Some("json")
                || owned_by_running_instance(&path)
            {
                continue;
            }
            #[cfg(not(target_arch = "wasm32"))]
            if !std::fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.is_file()) {
                continue;
            }
            let Ok(bytes) = crate::storage::read(&path) else {
                continue;
            };
            let snapshot = serde_json::from_slice::<Snapshot>(&bytes)
                .ok()
                .filter(|snapshot| snapshot.document.validate().is_ok());
            let Some(mut snapshot) = snapshot else {
                #[derive(Deserialize)]
                struct Probe {
                    document: Version,
                }
                #[derive(Deserialize)]
                struct Version {
                    version: u64,
                }
                if serde_json::from_slice::<Probe>(&bytes)
                    .is_ok_and(|probe| probe.document.version > u64::from(crate::document::VERSION))
                {
                    newer += 1;
                }
                continue;
            };
            snapshot.document.migrate();
            accept(Candidate { path, snapshot }, &bytes);
        }
        newer
    }
    pub fn save(&self, document: &Document, source: Option<PathBuf>) -> Result<(), String> {
        self.save_replacing(document, source, None)
    }
    /// [`Self::save`], then remove the draft `old` this one replaces; in the
    /// browser both happen together, so a failed save never loses `old`.
    pub fn save_replacing(
        &self,
        document: &Document,
        source: Option<PathBuf>,
        old: Option<&Path>,
    ) -> Result<(), String> {
        document.validate()?;
        #[derive(Serialize)]
        struct BorrowedSnapshot<'a> {
            document: Cow<'a, Document>,
            source: Option<PathBuf>,
            saved_at: u64,
        }
        // Drafts use the required file version and current presentation, so
        // restoring never repeats a migration or requires an unused feature.
        let document = if document.version == document.file_version() {
            Cow::Borrowed(document)
        } else {
            Cow::Owned(document.current())
        };
        let snapshot = BorrowedSnapshot {
            document,
            source,
            saved_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };
        crate::storage::write_replacing(
            &self.session,
            &serde_json::to_vec(&snapshot).map_err(|e| e.to_string())?,
            old,
        )
    }
    pub fn clear(&self) -> Result<(), String> {
        remove(&self.session)
    }
}

/// Move one offered draft to the operating system's Trash. A live owner's
/// file is protected even if it was offered by an earlier scan. There is no
/// permanent-delete fallback when the Trash is unavailable.
#[cfg(not(target_arch = "wasm32"))]
pub fn discard(candidate: &Candidate) -> Result<(), String> {
    if !candidate.available() {
        return Err("The draft belongs to a running ReShiki instance".into());
    }
    let metadata = match std::fs::symlink_metadata(&candidate.path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.to_string()),
    };
    if !metadata.is_file() {
        return Err("The recovery draft is no longer a regular file".into());
    }
    // Do not discard a draft replaced or edited after the offer was made.
    let bytes = crate::storage::read(&candidate.path).map_err(|error| error.to_string())?;
    let mut current: Snapshot =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    current.document.validate()?;
    current.document.migrate();
    if current.saved_at != candidate.snapshot.saved_at
        || current.source != candidate.snapshot.source
        || current.document != candidate.snapshot.document
    {
        return Err("The recovery draft changed; reopen ReShiki to review it again".into());
    }
    if !candidate.available() {
        return Err("The draft belongs to a running ReShiki instance".into());
    }
    #[cfg(windows)]
    {
        reshiki_windows::trash_recovery_draft(&candidate.path)
    }
    #[cfg(not(windows))]
    {
        let context = trash::TrashContext::new();
        #[cfg(target_os = "macos")]
        let context = {
            use trash::macos::{DeleteMethod, TrashContextExtMacos};
            let mut context = context;
            // Use the filesystem API, with no Finder automation permission.
            context.set_delete_method(DeleteMethod::NsFileManager);
            context
        };
        context
            .delete(&candidate.path)
            .map_err(|error| error.to_string())
    }
}
pub fn remove(path: &Path) -> Result<(), String> {
    match crate::storage::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}
/// What names this instance's drafts: the process ID, or in the browser the
/// page's session, which needs browser storage.
#[cfg(not(target_arch = "wasm32"))]
fn owner() -> Result<String, String> {
    Ok(std::process::id().to_string())
}
#[cfg(target_arch = "wasm32")]
fn owner() -> Result<String, String> {
    crate::storage::browser::session().ok_or_else(|| {
        "Recovery saving is unavailable: this page does not hold its session lock".into()
    })
}

/// Whether the instance that named a draft `owner` is still running.
#[cfg(not(target_arch = "wasm32"))]
fn running(owner: &str) -> bool {
    owner.parse::<u32>().is_ok_and(process_alive)
}
/// Another open page of the site keeps its own drafts.
#[cfg(target_arch = "wasm32")]
fn running(owner: &str) -> bool {
    crate::storage::browser::open(owner)
}

/// Whether a process with this ID is running, or cannot be asked.
#[cfg(not(target_arch = "wasm32"))]
pub fn process_alive(pid: u32) -> bool {
    if pid == std::process::id() {
        return true;
    }
    #[cfg(unix)]
    {
        let Some(pid) = i32::try_from(pid)
            .ok()
            .and_then(rustix::process::Pid::from_raw)
        else {
            return false;
        };
        // An inaccessible or otherwise unqueryable process is protected.
        rustix::process::test_kill_process(pid) != Err(rustix::io::Errno::SRCH)
    }
    #[cfg(windows)]
    {
        reshiki_windows::process_alive(pid)
    }
    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}
#[cfg(test)]
mod tests;
