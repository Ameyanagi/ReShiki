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
            let count = t.document.all_ids().len();
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
            let equal = next.templates.iter().find(|existing| {
                let mut content = t.clone();
                content.id = existing.id.clone();
                **existing == content
            });
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
    pub fn save_checked(&self, path: &Path, expected: &Self) -> Result<(), String> {
        let lock_path = path.with_extension("json.lock");
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)
            .map_err(|e| e.to_string())?;
        lock.try_lock().map_err(lock_error)?;
        // A concurrent process spawn can briefly inherit the same open file
        // description. Release explicitly before close, including error paths.
        let _lock = LibraryLock(lock);
        if Self::load(path)? != *expected {
            return Err("The library changed in another window. Reload it before saving.".into());
        }
        self.save(path)
    }
}
struct LibraryLock(std::fs::File);

impl Drop for LibraryLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
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
mod tests {
    #[cfg(unix)]
    use super::LibraryLock;
    use super::lock_error;
    use std::{fs::TryLockError, io};

    #[cfg(unix)]
    #[test]
    fn guard_releases_lock_even_while_a_duplicate_description_survives() {
        for fail in [false, true] {
            let file = tempfile::NamedTempFile::new().unwrap();
            let lock = file.reopen().unwrap();
            lock.lock().unwrap();
            let retained = lock.try_clone().unwrap();
            let contender = file.reopen().unwrap();
            let result = (|| {
                let _lock = LibraryLock(lock);
                assert!(matches!(
                    contender.try_lock(),
                    Err(TryLockError::WouldBlock)
                ));
                if fail {
                    return Err("save failed");
                }
                Ok(())
            })();
            assert_eq!(result.is_err(), fail);
            contender
                .try_lock()
                .expect("guard must release the shared description");
            contender.unlock().unwrap();
            drop(retained);
        }
    }

    #[test]
    fn lock_errors_distinguish_contention_from_io_failures() {
        assert_eq!(
            lock_error(TryLockError::WouldBlock),
            "Another window is saving templates. Try again."
        );

        let error = io::Error::from_raw_os_error(5);
        let message = error.to_string();
        let kind = format!("{:?}", error.kind());
        let reported = lock_error(TryLockError::Error(error));
        assert!(reported.starts_with("Could not lock the template library"));
        assert!(reported.contains(&message));
        assert!(reported.contains(&kind));

        assert_eq!(
            lock_error(TryLockError::Error(io::Error::new(
                io::ErrorKind::Unsupported,
                "File locking is unavailable",
            ))),
            "Could not lock the template library (Unsupported): File locking is unavailable"
        );
    }
}
