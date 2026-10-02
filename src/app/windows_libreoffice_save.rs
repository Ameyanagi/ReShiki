//! A LibreOffice watcher briefly opens the edit file while checking for saves.
//! Windows MoveFileEx cannot replace that destination until the reader closes.

use std::{
    io::{self, Write},
    path::Path,
    time::{Duration, Instant},
};

pub(super) fn is_embedded_save(host: &str, embedded: bool, save_existing: bool) -> bool {
    host == "LibreOffice" && embedded && save_existing
}

pub(super) fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    persist(temp, path, Duration::from_secs(2))
}

fn persist(mut temp: tempfile::NamedTempFile, path: &Path, budget: Duration) -> io::Result<()> {
    let deadline = Instant::now() + budget;
    loop {
        match temp.persist(path) {
            Ok(_) => return Ok(()),
            Err(error) => {
                // A reader produces ACCESS_DENIED on the tested Windows build;
                // other Windows/filesystem combinations report SHARING_VIOLATION.
                if !matches!(error.error.raw_os_error(), Some(5 | 32)) {
                    return Err(error.error);
                }
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(error.error);
                }
                std::thread::sleep(remaining.min(Duration::from_millis(10)));
                if Instant::now() >= deadline {
                    return Err(error.error);
                }
                // PersistError returns the same fully written temporary file.
                // Never delete or rewrite the destination to work around a lock.
                temp = error.file;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_saving_the_existing_libreoffice_embedding_uses_waiting_persist() {
        assert!(is_embedded_save("LibreOffice", true, true));
        for (host, embedded, save_existing) in [
            ("LibreOffice", false, true), // ordinary drawing in the same process
            ("LibreOffice", true, false), // Save As
            ("Office", true, true),       // native OLE session
            ("", false, true),            // normal save
        ] {
            assert!(!is_embedded_save(host, embedded, save_existing));
        }
    }

    #[test]
    fn an_overlapping_reader_can_close_before_the_atomic_save_deadline() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("drawing.rsk");
        std::fs::write(&path, b"previous drawing").unwrap();
        let reader = std::fs::File::open(&path).unwrap();
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            drop(reader);
        });
        write_atomic(&path, b"accepted drawing").unwrap();
        release.join().unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"accepted drawing");
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn a_reader_that_remains_open_exhausts_the_budget_without_losing_native_data() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("drawing.rsk");
        std::fs::write(&path, b"previous drawing").unwrap();
        let reader = std::fs::File::open(&path).unwrap();
        let mut temp = tempfile::NamedTempFile::new_in(directory.path()).unwrap();
        temp.write_all(b"unaccepted drawing").unwrap();
        temp.as_file().sync_all().unwrap();
        let started = Instant::now();
        let error = persist(temp, &path, Duration::from_millis(100)).unwrap_err();
        assert!(matches!(error.raw_os_error(), Some(5 | 32)));
        assert!(started.elapsed() >= Duration::from_millis(100));
        assert!(started.elapsed() < Duration::from_secs(1));
        assert_eq!(std::fs::read(&path).unwrap(), b"previous drawing");
        drop(reader);
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn a_nonretryable_destination_error_returns_without_waiting_or_changing_data() {
        let directory = tempfile::tempdir().unwrap();
        let original = directory.path().join("drawing.rsk");
        std::fs::write(&original, b"previous drawing").unwrap();
        let mut temp = tempfile::NamedTempFile::new_in(directory.path()).unwrap();
        temp.write_all(b"unaccepted drawing").unwrap();
        temp.as_file().sync_all().unwrap();
        let started = Instant::now();
        let error = persist(
            temp,
            &original.join("invalid-child"),
            Duration::from_secs(2),
        )
        .unwrap_err();
        assert!(!matches!(error.raw_os_error(), Some(5 | 32)));
        assert!(started.elapsed() < Duration::from_secs(1));
        assert_eq!(std::fs::read(&original).unwrap(), b"previous drawing");
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }
}
