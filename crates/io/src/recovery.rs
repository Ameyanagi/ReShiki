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
    identity: [u8; 32],
}
/// The exact abandoned file a restored tab may retire after saving its copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub path: PathBuf,
    identity: [u8; 32],
}

/// A successful replacement save, including any original that was kept.
#[derive(Debug, Clone, Default)]
pub struct Replacement {
    pub original: Option<Source>,
    pub kept: bool,
    pub offer: Option<Draft>,
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
            identity: self.identity,
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
    pub fn source(&self) -> Source {
        Source {
            path: self.path.clone(),
            identity: self.identity,
        }
    }

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

impl Draft {
    pub fn source(&self) -> Source {
        Source {
            path: self.path.clone(),
            identity: self.identity,
        }
    }
}

impl Source {
    fn available(&self) -> bool {
        !owned_by_running_instance(&self.path)
    }

    fn matches_path(&self, path: &Path) -> bool {
        if !self.available() {
            return false;
        }
        #[cfg(not(target_arch = "wasm32"))]
        let matches = {
            if !std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file()) {
                return false;
            }
            use sha2::{Digest, Sha256};
            use std::io::Read;
            let digest = (|| -> std::io::Result<[u8; 32]> {
                let mut file = std::fs::File::open(path)?;
                let mut digest = Sha256::new();
                let mut buffer = [0; 64 * 1024];
                loop {
                    let read = file.read(&mut buffer)?;
                    if read == 0 {
                        return Ok(digest.finalize().into());
                    }
                    digest.update(&buffer[..read]);
                }
            })();
            digest.is_ok_and(|digest| digest == self.identity)
        };
        #[cfg(target_arch = "wasm32")]
        let matches =
            crate::storage::read(path).is_ok_and(|bytes| identity(&bytes) == self.identity);
        matches && self.available()
    }

    #[cfg(target_arch = "wasm32")]
    fn matches(&self) -> bool {
        self.matches_path(&self.path)
    }

    fn preserved(&self, path: &Path) -> Replacement {
        let offer = if self.available() {
            #[cfg(not(target_arch = "wasm32"))]
            let regular = std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file());
            #[cfg(target_arch = "wasm32")]
            let regular = true;
            regular
                .then(|| crate::storage::read(path).ok())
                .flatten()
                .and_then(|bytes| {
                    let mut snapshot: Snapshot = serde_json::from_slice(&bytes).ok()?;
                    snapshot.document.validate().ok()?;
                    snapshot.document.migrate();
                    let candidate = Candidate {
                        path: path.to_path_buf(),
                        snapshot,
                        identity: identity(&bytes),
                    };
                    self.available()
                        .then(|| Draft::from_candidate(candidate, &bytes))
                })
        } else {
            None
        };
        Replacement {
            original: Some(self.clone()),
            kept: true,
            offer,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn retire(&self, replacement: &Path) -> Replacement {
        if self.path == replacement || !self.available() {
            return self.preserved(&self.path);
        }
        if std::fs::symlink_metadata(&self.path)
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
        {
            return Replacement {
                original: Some(self.clone()),
                ..Replacement::default()
            };
        }
        if !self.matches_path(&self.path) {
            return self.preserved(&self.path);
        }
        self.retire_matching()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn retire_matching(&self) -> Replacement {
        // Claim the name before the final digest check. A writer replacing the
        // original path afterwards cannot have its new file unlinked by us.
        // The claim is a persistent .json in the recovery folder: a crash or
        // failed restore of its name must never clean up unverified bytes.
        let Some(claim) = self.claim() else {
            return self.preserved(&self.path);
        };
        self.retire_claimed(claim)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn claim(&self) -> Option<PathBuf> {
        let stem = self.path.file_stem()?.to_str()?;
        let parent = self.path.parent()?;
        let (file, path) = tempfile::Builder::new()
            .prefix(&format!("{stem}-retiring-"))
            .suffix(".json")
            .tempfile_in(parent)
            .ok()?
            .keep()
            .ok()?;
        drop(file);
        std::fs::remove_file(&path).ok()?;
        if !self.available() || std::fs::rename(&self.path, &path).is_err() {
            return None;
        }
        Some(path)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn retire_claimed(&self, claim: PathBuf) -> Replacement {
        if self.matches_path(&claim) && std::fs::remove_file(&claim).is_ok() {
            return if std::fs::symlink_metadata(&self.path).is_ok() {
                self.preserved(&self.path)
            } else {
                Replacement {
                    original: Some(self.clone()),
                    ..Replacement::default()
                }
            };
        }
        // Restore without clobbering a concurrent writer. If it reclaimed the
        // original name, the claimed copy stays separately recoverable.
        let preserved = self.restore_claim(&claim);
        self.preserved(&preserved)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn restore_claim(&self, claim: &Path) -> PathBuf {
        #[cfg(unix)]
        if std::fs::hard_link(claim, &self.path).is_ok()
            && self.path.parent().is_some_and(|parent| {
                std::fs::File::open(parent)
                    .and_then(|directory| directory.sync_all())
                    .is_ok()
            })
        {
            // The restored name is durable before the fallback name is dropped.
            let _ = std::fs::remove_file(claim);
            return self.path.clone();
        }
        #[cfg(windows)]
        if tempfile::TempPath::try_from_path(claim).is_ok_and(|mut path| {
            // Even a failed no-clobber rename must never clean up changed bytes.
            path.disable_cleanup(true);
            path.persist_noclobber(&self.path).is_ok()
        }) {
            return self.path.clone();
        }
        claim.to_path_buf()
    }
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
            accept(
                Candidate {
                    path,
                    snapshot,
                    identity: identity(&bytes),
                },
                &bytes,
            );
        }
        newer
    }
    pub fn save(&self, document: &Document, source: Option<PathBuf>) -> Result<(), String> {
        self.save_replacing(document, source, None).map(|_| ())
    }
    /// Save a restored copy, retiring only the exact abandoned source offered.
    /// A changed/live source stays recoverable. Browser writes retain the
    /// existing guarded two-key transaction, so a failed save never loses it.
    pub fn save_replacing(
        &self,
        document: &Document,
        source: Option<PathBuf>,
        old: Option<&Source>,
    ) -> Result<Replacement, String> {
        let same_path = old.is_some_and(|old| old.path == self.session);
        #[cfg(not(target_arch = "wasm32"))]
        let same_path = same_path
            || old.is_some_and(|old| {
                std::fs::symlink_metadata(&self.session)
                    .is_ok_and(|metadata| metadata.file_type().is_symlink())
                    || std::fs::canonicalize(&self.session)
                        .ok()
                        .zip(std::fs::canonicalize(&old.path).ok())
                        .is_some_and(|(destination, original)| destination == original)
            });
        if same_path {
            return Err("A recovered copy must use a different recovery path".into());
        }
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
        let bytes = serde_json::to_vec(&snapshot).map_err(|e| e.to_string())?;
        #[cfg(not(target_arch = "wasm32"))]
        {
            crate::storage::write_atomic(&self.session, &bytes)?;
            Ok(old.map_or_else(Replacement::default, |old| old.retire(&self.session)))
        }
        #[cfg(target_arch = "wasm32")]
        {
            let retire = old.filter(|old| old.path != self.session && old.matches());
            // No await separates validation from this batch. The browser sink
            // compares both previous byte strings again in its durable transaction.
            crate::storage::write_replacing(
                &self.session,
                &bytes,
                retire.map(|old| old.path.as_path()),
            )?;
            Ok(match old {
                Some(old) if retire.is_some() => Replacement {
                    original: Some(old.clone()),
                    ..Replacement::default()
                },
                Some(old) => old.preserved(&old.path),
                None => Replacement::default(),
            })
        }
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
