use crate::{
    document::{Document, VERSION},
    storage::write_atomic,
};
use serde::{Deserialize, Serialize};
use std::{
    borrow::Cow,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

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
impl Recovery {
    pub fn standard() -> Result<Self, String> {
        let root = crate::compatibility::data_directory()?;
        Self::in_directory(&root.join("recovery"))
    }
    /// A new draft file in `root`. Each document tab has its own, so the
    /// count keeps drafts made within one clock tick apart.
    pub fn in_directory(root: &Path) -> Result<Self, String> {
        static COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let count = COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(Self {
            session: root.join(format!("{}-{now}-{count}.json", std::process::id())),
        })
    }
    pub fn candidates(&self) -> Vec<Candidate> {
        let Some(parent) = self.session.parent() else {
            return vec![];
        };
        let Ok(entries) = std::fs::read_dir(parent) else {
            return vec![];
        };
        let mut candidates: Vec<_> = entries
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                if path == self.session || path.extension().and_then(|e| e.to_str()) != Some("json")
                {
                    return None;
                }
                // Do not offer another currently running instance's draft.
                if let Some(pid) = path
                    .file_stem()?
                    .to_str()?
                    .split('-')
                    .next()
                    .and_then(|s| s.parse::<u32>().ok())
                    && process_alive(pid)
                {
                    return None;
                }
                let bytes = std::fs::read(&path).ok()?;
                let mut snapshot: Snapshot = serde_json::from_slice(&bytes).ok()?;
                snapshot.document.validate().ok()?;
                snapshot.document.migrate();
                Some(Candidate { path, snapshot })
            })
            .collect();
        candidates.sort_by_key(|c| std::cmp::Reverse(c.snapshot.saved_at));
        candidates
    }
    pub fn save(&self, document: &Document, source: Option<PathBuf>) -> Result<(), String> {
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
        write_atomic(
            &self.session,
            &serde_json::to_vec(&snapshot).map_err(|e| e.to_string())?,
        )
    }
    pub fn clear(&self) -> Result<(), String> {
        remove(&self.session)
    }
}
pub fn remove(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}
fn process_alive(pid: u32) -> bool {
    if pid == std::process::id() {
        return true;
    }
    #[cfg(unix)]
    {
        std::process::Command::new("/bin/kill")
            .args(["-0", &pid.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
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
