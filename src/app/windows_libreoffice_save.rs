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
mod tests;
