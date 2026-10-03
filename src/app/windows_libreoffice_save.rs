//! A LibreOffice watcher briefly opens the edit file while checking for saves.
//! Windows MoveFileEx cannot replace that destination until the reader closes.
//! Only native Microsoft Office sessions use the OLE acknowledgement protocol.

use std::{
    io::{self, Write},
    path::Path,
    time::{Duration, Instant},
};

pub(super) fn save(path: &Path, bytes: &[u8], host: Option<&str>) -> Result<(), String> {
    match host {
        Some("Office") => {
            reshiki_windows::prepare_office_save(path);
            reshiki::storage::write_atomic(path, bytes)?;
            reshiki_windows::wait_for_office_save(path, bytes)
        }
        Some("LibreOffice") => write_atomic(path, bytes).map_err(|error| error.to_string()),
        _ => reshiki::storage::write_atomic(path, bytes),
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
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
    fn libreoffice_and_ordinary_saves_do_not_require_or_touch_ole_receipts() {
        for host in [Some("LibreOffice"), None] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("drawing.rsk");
            let receipt = path.with_extension("office-saved");
            let error = path.with_extension("office-error");
            std::fs::write(&path, b"previous drawing").unwrap();
            std::fs::write(&receipt, b"unrelated OLE receipt").unwrap();
            std::fs::write(&error, b"unrelated OLE error").unwrap();
            save(&path, b"new drawing", host).unwrap();
            assert_eq!(std::fs::read(&path).unwrap(), b"new drawing");
            assert_eq!(std::fs::read(&receipt).unwrap(), b"unrelated OLE receipt");
            assert_eq!(std::fs::read(&error).unwrap(), b"unrelated OLE error");
        }
    }

    #[test]
    fn native_office_save_waits_for_matching_ole_receipt() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("drawing.rsk");
        let receipt = path.with_extension("office-saved");
        let error = path.with_extension("office-error");
        let native = b"updated OLE drawing";
        std::fs::write(&receipt, native).unwrap();
        std::fs::write(&error, b"previous OLE failure").unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        let destination = path.clone();
        let worker = std::thread::spawn(move || {
            sender
                .send(save(&destination, native, Some("Office")))
                .unwrap();
        });
        let deadline = Instant::now() + Duration::from_secs(2);
        while !path.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(std::fs::read(&path).unwrap(), native);
        assert!(
            !receipt.exists(),
            "Invalidate even a matching receipt from an older save"
        );
        assert!(!error.exists());
        assert!(matches!(
            receiver.recv_timeout(Duration::from_millis(150)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        ));
        std::fs::write(&receipt, b"wrong drawing").unwrap();
        assert!(matches!(
            receiver.recv_timeout(Duration::from_millis(150)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        ));
        std::fs::write(&receipt, native).unwrap();
        receiver
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap();
        worker.join().unwrap();
    }

    #[test]
    fn native_office_save_does_not_accept_a_libreoffice_receipt() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("drawing.rsk");
        std::fs::write(
            directory.path().join("accepted.json"),
            br#"{"version":1,"accepted_sha256":"9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"}"#,
        ).unwrap();
        let error = save(&path, b"test", Some("Office")).unwrap_err();
        assert!(error.contains("Office has not accepted this update"));
        assert_eq!(std::fs::read(&path).unwrap(), b"test");
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
        save(&path, b"accepted drawing", Some("LibreOffice")).unwrap();
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
