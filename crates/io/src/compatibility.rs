//! Read earlier installations without changing their files.
use std::{
    ffi::OsString,
    io::Write,
    path::{Path, PathBuf},
};

/// Prefer the current setting; retain explicit overrides from older installations.
pub fn environment(suffix: &str) -> Option<OsString> {
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

/// Where the application data directory comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataLocationOrigin {
    /// `RESHIKI_DATA_DIR` (or the earlier `MORUNO_DATA_DIR`).
    Override,
    /// The platform's per-user data directory.
    Default,
}

/// The application data directory and where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataLocation {
    pub path: PathBuf,
    pub origin: DataLocationOrigin,
}

/// An override wins over the platform directory; neither is an error.
fn resolve_data_location(
    override_path: Option<OsString>,
    project: Option<PathBuf>,
) -> Result<DataLocation, String> {
    if let Some(path) = override_path {
        return Ok(DataLocation {
            path: PathBuf::from(path),
            origin: DataLocationOrigin::Override,
        });
    }
    let path = project.ok_or("No application data directory")?;
    Ok(DataLocation {
        path,
        origin: DataLocationOrigin::Default,
    })
}

/// The data directory [`data_directory`] uses, without migrating or creating
/// anything.
pub fn data_directory_location() -> Result<DataLocation, String> {
    resolve_data_location(
        environment("DATA_DIR"),
        directories_next::ProjectDirs::from("dev", "reshiki", "ReShiki")
            .map(|project| project.data_local_dir().to_path_buf()),
    )
}

/// The data directory of earlier (Moruno) installations, which may not exist.
pub fn legacy_data_directory_location() -> Option<PathBuf> {
    directories_next::ProjectDirs::from("dev", "moruno", "Moruno")
        .map(|previous| previous.data_local_dir().to_path_buf())
}

pub fn home_directory() -> Option<PathBuf> {
    directories_next::BaseDirs::new().map(|base| base.home_dir().to_path_buf())
}

/// An override is used as given; the platform directory first imports data
/// from earlier installations.
fn finish_data_directory(
    location: DataLocation,
    migrate: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<PathBuf, String> {
    match location.origin {
        DataLocationOrigin::Override => Ok(location.path),
        DataLocationOrigin::Default => {
            migrate(&location.path)?;
            Ok(location.path)
        }
    }
}

pub fn data_directory() -> Result<PathBuf, String> {
    finish_data_directory(data_directory_location()?, |root| {
        match directories_next::ProjectDirs::from("dev", "moruno", "Moruno") {
            Some(previous) => migrate_data(previous.data_local_dir(), root),
            None => Ok(()),
        }
    })
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
mod tests;
