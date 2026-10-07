use std::{
    io::Write,
    path::{Path, PathBuf},
};

/// Preserve explicit suffixes; supply the format suffix when a save name has none.
pub fn with_default_extension(path: &Path, extension: &str) -> PathBuf {
    if path.extension().is_none_or(|suffix| suffix.is_empty()) {
        path.with_extension(extension)
    } else {
        path.to_path_buf()
    }
}

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
mod tests;
