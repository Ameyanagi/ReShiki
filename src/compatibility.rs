//! Read earlier installations without changing their files.
use std::{ffi::OsString, io::Write, path::PathBuf};

/// Prefer the current setting; retain explicit overrides from older installations.
pub(crate) fn environment(suffix: &str) -> Option<OsString> {
    std::env::var_os(format!("RESHIKI_{suffix}"))
        .or_else(|| std::env::var_os(format!("MORUNO_{suffix}")))
}

pub const NATIVE_EXTENSION: &str = "rsk";
pub const NATIVE_EXTENSIONS: &[&str] = &[NATIVE_EXTENSION, "reshiki", "moruno"];

pub fn is_native_extension(extension: &str) -> bool {
    NATIVE_EXTENSIONS
        .iter()
        .any(|native| extension.eq_ignore_ascii_case(native))
}

pub(crate) fn data_directory() -> Result<PathBuf, String> {
    if let Some(path) = environment("DATA_DIR") {
        return Ok(PathBuf::from(path));
    }
    let root = directories_next::ProjectDirs::from("dev", "reshiki", "ReShiki")
        .ok_or("No application data directory")?
        .data_local_dir()
        .to_path_buf();
    if let Some(previous) = directories_next::ProjectDirs::from("dev", "moruno", "Moruno") {
        migrate_data(previous.data_local_dir(), &root)?;
    }
    Ok(root)
}

fn copy_if_missing(source: &std::path::Path, target: &std::path::Path) -> Result<(), String> {
    let metadata = match source.symlink_metadata() {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.to_string()),
    };
    if !metadata.is_file() || target.exists() {
        return Ok(());
    }
    let parent = target.parent().ok_or("No application data parent")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut staged = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    let mut input = std::fs::File::open(source).map_err(|e| e.to_string())?;
    std::io::copy(&mut input, &mut staged).map_err(|e| e.to_string())?;
    staged.flush().map_err(|e| e.to_string())?;
    staged.as_file().sync_all().map_err(|e| e.to_string())?;
    match staged.persist_noclobber(target) {
        Ok(_) => Ok(()),
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

fn migrate_data(previous: &std::path::Path, root: &std::path::Path) -> Result<(), String> {
    let marker = root.join(".legacy-data-imported");
    if marker.is_file() || !previous.is_dir() {
        return Ok(());
    }
    for name in ["assistant-preferences.json", "templates.json"] {
        copy_if_missing(&previous.join(name), &root.join(name))?;
    }
    match std::fs::read_dir(previous.join("recovery")) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry.map_err(|e| e.to_string())?;
                if entry.path().extension().is_some_and(|e| e == "json") {
                    copy_if_missing(
                        &entry.path(),
                        &root.join("recovery").join(entry.file_name()),
                    )?;
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.to_string()),
    }
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    crate::storage::write_atomic(&marker, b"1\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_preserves_originals_and_current_choices_and_does_not_resurrect_drafts()
    -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let old = directory.path().join("old");
        let new = directory.path().join("new");
        std::fs::create_dir_all(old.join("recovery"))?;
        std::fs::create_dir_all(&new)?;
        std::fs::write(old.join("templates.json"), b"templates")?;
        std::fs::write(old.join("assistant-preferences.json"), b"old choice")?;
        std::fs::write(new.join("assistant-preferences.json"), b"new choice")?;
        std::fs::write(old.join("recovery/draft.json"), b"drawing")?;
        migrate_data(&old, &new).map_err(anyhow::Error::msg)?;
        assert_eq!(std::fs::read(new.join("templates.json"))?, b"templates");
        assert_eq!(
            std::fs::read(new.join("assistant-preferences.json"))?,
            b"new choice"
        );
        assert_eq!(std::fs::read(new.join("recovery/draft.json"))?, b"drawing");
        assert_eq!(std::fs::read(old.join("recovery/draft.json"))?, b"drawing");
        std::fs::remove_file(new.join("recovery/draft.json"))?;
        migrate_data(&old, &new).map_err(anyhow::Error::msg)?;
        assert!(!new.join("recovery/draft.json").exists());
        Ok(())
    }

    #[test]
    fn native_extensions_accept_previous_drawings() {
        assert!(is_native_extension("rsk"));
        assert!(is_native_extension("RSK"));
        assert!(is_native_extension("reshiki"));
        assert!(is_native_extension("MORUNO"));
        assert!(!is_native_extension("mol"));
    }
}
