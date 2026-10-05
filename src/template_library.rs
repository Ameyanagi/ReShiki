//! User-authored templates and portable collections. Drawing documents are never
//! modified by library operations, and a failed load/write leaves the prior file intact.
use crate::{
    document::Document,
    storage::write_atomic,
    templates::{Anchor, LIBRARY, Template},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Library {
    pub version: u32,
    pub templates: Vec<Template>,
    #[serde(default)]
    pub favorites: Vec<String>,
}
impl Default for Library {
    fn default() -> Self {
        Self {
            version: 1,
            templates: vec![],
            favorites: vec![],
        }
    }
}
impl Library {
    pub fn get(&self, index: usize) -> Option<&Template> {
        if index < LIBRARY.len() {
            LIBRARY.get(index)
        } else {
            self.templates.get(index - LIBRARY.len())
        }
    }
    pub fn iter(&self) -> impl Iterator<Item = &Template> {
        LIBRARY.iter().chain(&self.templates)
    }
    pub fn favorite(&self, id: &str) -> bool {
        self.favorites.iter().any(|f| f == id)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("Unsupported template library version".into());
        }
        if self.templates.len() > 2048 {
            return Err("A collection can contain at most 2048 templates".into());
        }
        let mut ids = HashSet::new();
        for t in &self.templates {
            if t.id.is_empty()
                || t.id.len() > 200
                || t.id.starts_with("builtin:")
                || !ids.insert(&t.id)
            {
                return Err("Invalid or duplicate template identifier".into());
            }
            if t.name.trim().is_empty()
                || t.name.chars().count() > 100
                || t.group.trim().is_empty()
                || t.group.chars().count() > 64
            {
                return Err(
                    "Use a template name of 1–100 characters and a collection of 1–64 characters"
                        .into(),
                );
            }
            if t.note.chars().count() > 500
                || t.keywords.len() > 32
                || t.keywords.iter().any(|s| s.chars().count() > 100)
            {
                return Err("Template descriptions/keywords are too long".into());
            }
            t.document.validate()?;
            let count = t.document.object_ids().count();
            if count == 0 || count > 10000 {
                return Err("A template must contain 1–10000 drawing objects".into());
            }
            if !t.anchor.valid(&t.document) {
                return Err("Template attachment point is missing".into());
            }
        }
        Ok(())
    }
    pub fn add(
        &mut self,
        name: &str,
        group: &str,
        document: Document,
        anchor: Anchor,
    ) -> Result<usize, String> {
        let mut next = self.clone();
        let id = new_id();
        next.templates.push(Template {
            id,
            name: name.trim().into(),
            group: group.trim().into(),
            smiles: String::new(),
            keywords: vec![],
            note: String::new(),
            document,
            anchor,
        });
        next.validate()?;
        *self = next;
        Ok(LIBRARY.len() + self.templates.len() - 1)
    }
    /// Merge by stable identity; an independently edited entry is retained as a
    /// separate copy instead of replacing either version. Reimporting is idempotent.
    pub fn merge(&mut self, incoming: Self) -> Result<usize, String> {
        incoming.validate()?;
        let mut next = self.clone();
        let mut added = 0;
        for mut t in incoming.templates {
            let favorite = incoming.favorites.contains(&t.id);
            let original_id = std::mem::take(&mut t.id);
            let equal = next.templates.iter().find(|existing| {
                t.id.clone_from(&existing.id);
                **existing == t
            });
            t.id = original_id;
            let id = if let Some(existing) = equal {
                existing.id.clone()
            } else {
                if next.templates.iter().any(|e| e.id == t.id) {
                    t.id = new_id();
                }
                let id = t.id.clone();
                next.templates.push(t);
                added += 1;
                id
            };
            if favorite && !next.favorite(&id) {
                next.favorites.push(id);
            }
        }
        for id in incoming.favorites {
            if LIBRARY.iter().any(|t| t.id == id) && !next.favorite(&id) {
                next.favorites.push(id);
            }
        }
        next.validate()?;
        *self = next;
        Ok(added)
    }
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > 32 * 1024 * 1024 {
            return Err("Template collection exceeds 32 MB".into());
        }
        let library: Self = serde_json::from_slice(bytes)
            .map_err(|e| format!("Invalid template collection: {e}"))?;
        library.validate()?;
        Ok(library)
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        match std::fs::read(path) {
            Ok(bytes) => Self::from_bytes(&bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.to_string()),
        }
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        write_atomic(
            path,
            &serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?,
        )
    }
    /// Serialize concurrent local editors and reject stale state without losing
    /// another window's templates. The OS releases the lock if an app exits.
    pub fn save_checked(&self, path: &Path, expected: &Self) -> Result<SaveOutcome, String> {
        let lock_path = path.with_extension("json.lock");
        with_library_lock(&lock_path, || {
            if Self::load(path)? != *expected {
                return Err(
                    "The library changed in another window. Reload it before saving.".into(),
                );
            }
            self.save(path)
        })
    }
}

/// The replacement was committed. A cleanup warning must not invite a save retry.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SaveOutcome {
    pub release_warning: Option<String>,
}

fn with_library_lock(
    path: &Path,
    operation: impl FnOnce() -> Result<(), String>,
) -> Result<SaveOutcome, String> {
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    lock.try_lock().map_err(lock_error)?;
    // Closing one handle need not release a lock while a duplicate remains.
    // Normal completion records cleanup errors; Drop is the unwind fallback.
    let lock = LibraryLock(Some(lock));
    let result = operation();
    lock.finish(result, std::fs::File::unlock)
}

struct LibraryLock(Option<std::fs::File>);

impl LibraryLock {
    fn release(
        &mut self,
        unlock: impl FnOnce(&std::fs::File) -> std::io::Result<()>,
    ) -> std::io::Result<()> {
        let Some(file) = self.0.take() else {
            return Ok(());
        };
        // Closing the taken handle happens on both success and failure. Drop
        // cannot attempt a second unlock after this explicit release.
        unlock(&file)
    }

    fn finish(
        mut self,
        operation: Result<(), String>,
        unlock: impl FnOnce(&std::fs::File) -> std::io::Result<()>,
    ) -> Result<SaveOutcome, String> {
        let release = self.release(unlock);
        match (operation, release) {
            (Ok(()), Ok(())) => Ok(SaveOutcome::default()),
            (Ok(()), Err(error)) => Ok(SaveOutcome {
                release_warning: Some(format!(
                    "Templates saved, but the library lock could not be released ({:?}): {error}",
                    error.kind()
                )),
            }),
            (Err(error), Ok(())) => Err(error),
            (Err(error), Err(release)) => Err(format!(
                "{error}; could not release the template library lock ({:?}): {release}",
                release.kind()
            )),
        }
    }
}

impl Drop for LibraryLock {
    fn drop(&mut self) {
        let _ = self.release(std::fs::File::unlock);
    }
}

fn lock_error(error: std::fs::TryLockError) -> String {
    match error {
        std::fs::TryLockError::WouldBlock => {
            "Another window is saving templates. Try again.".into()
        }
        std::fs::TryLockError::Error(error) => format!(
            "Could not lock the template library ({:?}): {error}",
            error.kind()
        ),
    }
}
fn new_id() -> String {
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!(
        "custom:{time:x}-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}
pub fn standard_path() -> Result<PathBuf, String> {
    let root = crate::compatibility::data_directory()?;
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    Ok(root.join("templates.json"))
}

#[cfg(test)]
mod tests;
