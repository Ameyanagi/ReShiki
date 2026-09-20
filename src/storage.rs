use std::{io::Write, path::Path};

/// Replace only after the complete file has been written beside its destination.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    temp.write_all(bytes).map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    temp.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replacing_a_longer_document_does_not_leave_old_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("drawing.reshiki");
        write_atomic(&path, b"previous long document").unwrap();
        write_atomic(&path, b"{}").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"{}");
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }
}
