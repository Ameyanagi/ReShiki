use crate::document::{Document, VERSION};
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
pub struct Recovery {
    pub session: PathBuf,
}
impl Candidate {
    /// Only an abandoned draft may be restored or discarded. Recheck before
    /// acting: a process ID may have been reused since the startup scan.
    pub fn available(&self) -> bool {
        !owned_by_running_instance(&self.path)
    }
}

fn owned_by_running_instance(path: &Path) -> bool {
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
        let Some(parent) = self.session.parent() else {
            return vec![];
        };
        let Ok(entries) = crate::storage::read_dir(parent) else {
            return vec![];
        };
        let mut candidates: Vec<_> = entries
            .into_iter()
            .filter_map(|path| {
                if path == self.session || path.extension().and_then(|e| e.to_str()) != Some("json")
                {
                    return None;
                }
                // Do not offer another currently running instance's draft.
                if owned_by_running_instance(&path) {
                    return None;
                }
                #[cfg(not(target_arch = "wasm32"))]
                if !std::fs::symlink_metadata(&path).ok()?.is_file() {
                    return None;
                }
                let bytes = crate::storage::read(&path).ok()?;
                let mut snapshot: Snapshot = serde_json::from_slice(&bytes).ok()?;
                snapshot.document.validate().ok()?;
                snapshot.document.migrate();
                Some(Candidate { path, snapshot })
            })
            .collect();
        candidates.sort_by(|a, b| {
            b.snapshot
                .saved_at
                .cmp(&a.snapshot.saved_at)
                .then_with(|| a.path.cmp(&b.path))
        });
        candidates
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
        // Drafts carry this build's document version, so restoring never migrates again.
        let document = if document.version == VERSION {
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

#[cfg(not(target_arch = "wasm32"))]
fn process_alive(pid: u32) -> bool {
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
