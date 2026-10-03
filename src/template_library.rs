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
mod tests {
    use super::{LibraryLock, SaveOutcome, lock_error, with_library_lock};
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
            let lock = LibraryLock(Some(lock));
            assert!(matches!(
                contender.try_lock(),
                Err(TryLockError::WouldBlock)
            ));
            let operation = if fail {
                Err("save failed".into())
            } else {
                Ok(())
            };
            let result = lock.finish(operation, std::fs::File::unlock);
            assert_eq!(result.is_err(), fail);
            contender
                .try_lock()
                .expect("guard must release the shared description");
            contender.unlock().unwrap();
            drop(retained);
        }
    }

    #[test]
    fn normal_finish_distinguishes_committed_warnings_from_operation_failures() {
        for operation_failed in [false, true] {
            for release_failed in [false, true] {
                let file = tempfile::NamedTempFile::new().unwrap();
                let lock = file.reopen().unwrap();
                lock.lock().unwrap();
                let calls = std::cell::Cell::new(0);
                let operation = if operation_failed {
                    Err("injected write failure before commit".to_owned())
                } else {
                    Ok(())
                };
                let error = io::Error::from_raw_os_error(5);
                let diagnostic = format!("({:?}): {error}", error.kind());
                let result = LibraryLock(Some(lock)).finish(operation, |file| {
                    calls.set(calls.get() + 1);
                    file.unlock()?;
                    // Only the reported cleanup result is injected. No real
                    // operating-system unlock failure is claimed by this test.
                    if release_failed { Err(error) } else { Ok(()) }
                });
                assert_eq!(calls.get(), 1);
                match (operation_failed, release_failed) {
                    (false, false) => assert_eq!(result.unwrap(), SaveOutcome::default()),
                    (false, true) => {
                        let warning = result.unwrap().release_warning.unwrap();
                        assert!(warning.starts_with("Templates saved, but"));
                        assert!(warning.contains(&diagnostic));
                    }
                    (true, false) => {
                        assert_eq!(result.unwrap_err(), "injected write failure before commit");
                    }
                    (true, true) => {
                        let error = result.unwrap_err();
                        assert!(error.starts_with("injected write failure before commit;"));
                        assert!(error.contains(&diagnostic));
                        assert!(!error.contains("Templates saved"));
                    }
                }
                let contender = file.reopen().unwrap();
                contender.try_lock().unwrap();
                contender.unlock().unwrap();
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn failed_explicit_release_closes_once_without_a_drop_retry() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let lock = file.reopen().unwrap();
        lock.lock().unwrap();
        let retained = lock.try_clone().unwrap();
        let mut guard = LibraryLock(Some(lock));
        let error = guard
            .release(|_| Err(io::Error::other("injected unlock failure")))
            .unwrap_err();
        assert_eq!(error.to_string(), "injected unlock failure");
        assert!(guard.0.is_none());
        drop(guard);
        let contender = file.reopen().unwrap();
        assert!(matches!(
            contender.try_lock(),
            Err(TryLockError::WouldBlock)
        ));
        // The injection deliberately did not release the OS lock. A Drop retry
        // would have released this shared description and failed the assertion.
        retained.unlock().unwrap();
        contender.try_lock().unwrap();
        contender.unlock().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn guard_unwind_releases_a_retained_description_without_masking_the_panic() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let lock = file.reopen().unwrap();
        lock.lock().unwrap();
        let retained = lock.try_clone().unwrap();
        let result = std::panic::catch_unwind(move || {
            let _lock = LibraryLock(Some(lock));
            panic!("injected operation panic");
        });
        assert_eq!(
            result.unwrap_err().downcast_ref::<&str>(),
            Some(&"injected operation panic")
        );
        let contender = file.reopen().unwrap();
        contender.try_lock().unwrap();
        contender.unlock().unwrap();
        drop(retained);
    }

    #[test]
    fn library_operation_holds_lock_through_persist_and_releases_on_write_failure() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("templates.json");
        let lock_path = path.with_extension("json.lock");
        std::fs::write(&path, b"before").unwrap();
        for write_fails in [true, false] {
            let before = std::fs::read(&path).unwrap();
            let outcome = with_library_lock(&lock_path, || {
                let contender = std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&lock_path)
                    .unwrap();
                assert!(matches!(
                    contender.try_lock(),
                    Err(TryLockError::WouldBlock)
                ));
                if write_fails {
                    return Err("injected write failure before persist".into());
                }
                crate::storage::write_atomic(&path, b"committed")?;
                assert_eq!(std::fs::read(&path).unwrap(), b"committed");
                assert!(matches!(
                    contender.try_lock(),
                    Err(TryLockError::WouldBlock)
                ));
                Ok(())
            });
            if write_fails {
                assert_eq!(
                    outcome.unwrap_err(),
                    "injected write failure before persist"
                );
                assert_eq!(std::fs::read(&path).unwrap(), before);
            } else {
                assert_eq!(outcome.unwrap(), SaveOutcome::default());
            }
            let contender = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&lock_path)
                .unwrap();
            contender.try_lock().unwrap();
            contender.unlock().unwrap();
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
