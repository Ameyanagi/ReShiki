use crate::{document::Document, storage::write_atomic};
use serde::{Deserialize, Serialize};
use std::{
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
    pub fn in_directory(root: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        Ok(Self {
            session: root.join(format!("{}-{now}.json", std::process::id())),
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
                let snapshot: Snapshot = serde_json::from_slice(&bytes).ok()?;
                snapshot.document.validate().ok()?;
                Some(Candidate { path, snapshot })
            })
            .collect();
        candidates.sort_by_key(|c| std::cmp::Reverse(c.snapshot.saved_at));
        candidates
    }
    pub fn save(&self, document: &Document, source: Option<PathBuf>) -> Result<(), String> {
        document.validate()?;
        let snapshot = Snapshot {
            document: document.clone(),
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
mod tests {
    use super::*;
    #[cfg(windows)]
    #[test]
    fn windows_does_not_recover_a_live_editors_draft() {
        use std::{
            os::windows::process::CommandExt,
            process::{Command, Stdio},
        };
        let mut child = Command::new(
            std::path::PathBuf::from(std::env::var_os("WINDIR").unwrap()).join("System32/ping.exe"),
        )
        .args(["-n", "30", "127.0.0.1"])
        .creation_flags(0x08000000)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = Recovery::in_directory(dir.path()).unwrap();
        let mut doc = Document::default();
        doc.add_atom("O", Default::default());
        store.save(&doc, None).unwrap();
        std::fs::rename(
            &store.session,
            dir.path().join(format!("{}-other.json", child.id())),
        )
        .unwrap();
        let while_running = store.candidates();
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(while_running.is_empty());
        assert_eq!(store.candidates().len(), 1);
    }
    #[test]
    fn draft_is_durable_and_corrupt_files_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let store = Recovery::in_directory(dir.path()).unwrap();
        let mut doc = Document::default();
        doc.add_atom("O", Default::default());
        store.save(&doc, None).unwrap();
        std::fs::rename(&store.session, dir.path().join("4294967294-test.json")).unwrap();
        std::fs::write(dir.path().join("broken.json"), b"partial json").unwrap();
        let candidates = store.candidates();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].snapshot.document, doc);
        store.save(&candidates[0].snapshot.document, None).unwrap();
        remove(&candidates[0].path).unwrap();
        assert!(store.session.exists());
        store.clear().unwrap();
        assert!(!store.session.exists());
    }
}
