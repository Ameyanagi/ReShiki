//! Read earlier installations and share file-import compatibility rules.
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

/// The structure import format for a known extension, ignoring ASCII case.
pub fn structure_format(extension: &str) -> Option<&'static str> {
    if is_native_extension(extension) {
        Some("reshiki")
    } else {
        ["mol", "rxn", "rsmi", "cdxml", "cdx", "inchi"]
            .into_iter()
            .find(|format| extension.eq_ignore_ascii_case(format))
    }
}

/// The format the app opens or imports a file as: [`structure_format`], a
/// crystal file by its extension, or a VASP file named POSCAR or CONTCAR
/// without one by its name.
pub fn file_format(path: &Path) -> Option<&'static str> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    structure_format(extension)
        .or_else(|| crate::crystal::extension_format(extension))
        .or_else(|| {
            path.file_name()
                .and_then(|n| n.to_str())
                .and_then(crate::crystal::format_of)
        })
}

/// Structure file contents as import text: binary CDX as standard base64.
/// Only the exact format name `cdx` is binary; every other name uses UTF-8,
/// without one leading byte-order mark.
pub fn import_text(format: &str, bytes: Vec<u8>) -> Result<String, String> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    if format == "cdx" {
        Ok(STANDARD.encode(bytes))
    } else {
        let text = String::from_utf8(bytes)
            .map_err(|error| format!("The file is not valid UTF-8: {error}"))?;
        // A byte-order mark is not content; it hides `$RXN` and `InChI=`.
        Ok(match text.strip_prefix('\u{feff}') {
            Some(rest) => rest.into(),
            None => text,
        })
    }
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
        match home_data_directory() {
            Some(home) => Some(current_or_earlier(home, project_data_directory())),
            None => project_data_directory().or_else(crate::storage::browser_data_directory),
        },
    )
}

/// `~/.reshiki`, the data folder on macOS and Linux; none elsewhere.
fn home_data_directory() -> Option<PathBuf> {
    if cfg!(any(target_os = "macos", target_os = "linux")) {
        directories_next::BaseDirs::new().map(|base| base.home_dir().join(".reshiki"))
    } else {
        None
    }
}

/// The platform's per-user data folder: the data folder on Windows, and the
/// one macOS and Linux used before `~/.reshiki`.
fn project_data_directory() -> Option<PathBuf> {
    directories_next::ProjectDirs::from("dev", "reshiki", "ReShiki")
        .map(|project| project.data_local_dir().to_path_buf())
}

/// The data folder of earlier (Moruno) installations.
fn moruno_data_directory() -> Option<PathBuf> {
    directories_next::ProjectDirs::from("dev", "moruno", "Moruno")
        .map(|previous| previous.data_local_dir().to_path_buf())
}

/// `new` once anything is there; until then `old`, if it is a folder, so data
/// that has not moved (see [`move_data_directory`]) is still used where it is.
fn current_or_earlier(new: PathBuf, old: Option<PathBuf>) -> PathBuf {
    match old {
        Some(old) if !present(&new) && old.is_dir() => old,
        _ => new,
    }
}

/// Whether anything, even a broken link, is at `path`.
fn present(path: &Path) -> bool {
    path.symlink_metadata().is_ok()
}

/// On macOS and Linux, moves the earlier data folder to `~/.reshiki` when
/// no override is set, nothing is at `~/.reshiki` yet and the earlier folder
/// exists. The GUI calls this once at startup, before anything reads or
/// writes data. `Err` is the status-bar message when the move fails; the data
/// then stays where it was and is used there, as [`data_directory_location`]
/// finds it.
pub fn move_data_directory() -> Result<(), String> {
    move_data(
        environment("DATA_DIR"),
        project_data_directory(),
        home_data_directory(),
        |from, to| std::fs::rename(from, to),
    )
}

/// One rename, so the data is wholly in one place or the other: nothing is
/// copied or deleted, and a failed rename leaves both paths as they were.
fn move_data(
    override_path: Option<OsString>,
    old: Option<PathBuf>,
    new: Option<PathBuf>,
    rename: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
) -> Result<(), String> {
    let (None, Some(old), Some(new)) = (override_path, old, new) else {
        return Ok(());
    };
    if present(&new) || !old.is_dir() {
        return Ok(());
    }
    match rename(&old, &new) {
        Ok(()) => Ok(()),
        // Another ReShiki moved it first; `~/.reshiki` is used.
        Err(_) if present(&new) => Ok(()),
        Err(error) => Err(format!(
            "Kept your data in {}: could not move it to ~/.reshiki ({error})",
            old.display()
        )),
    }
}

/// Where ReShiki keeps caches it can rebuild: `cache` in the data folder
/// (`~/.reshiki/cache` on macOS and Linux), or the platform's cache folder on
/// Windows when no override is set. Inside the data folder in use, so a cache
/// never creates `~/.reshiki` before the data has moved there.
pub fn cache_directory() -> Result<PathBuf, String> {
    let windows = cfg!(windows).then(|| {
        directories_next::ProjectDirs::from("dev", "reshiki", "ReShiki")
            .map(|project| project.cache_dir().to_path_buf())
    });
    Ok(cache_in(data_directory_location()?, windows.flatten()))
}

fn cache_in(data: DataLocation, platform: Option<PathBuf>) -> PathBuf {
    match (data.origin, platform) {
        (DataLocationOrigin::Default, Some(platform)) => platform,
        _ => data.path.join("cache"),
    }
}

/// Every default data folder of this and earlier installations, which may not
/// exist: `~/.reshiki` and the folder it moved from on macOS and Linux (the
/// data folder itself on Windows), and Moruno's.
pub fn known_data_directories() -> Vec<PathBuf> {
    home_data_directory()
        .into_iter()
        .chain(project_data_directory())
        .chain(moruno_data_directory())
        .collect()
}

/// Every folder that may be the user's home: the platform's (on Windows the
/// profile known folder; on Unix `HOME` or the user's account entry) and the
/// environment's ([`std::env::home_dir`]: `HOME` on Unix, `USERPROFILE` on
/// Windows). Callers refuse every one, so an overridden `USERPROFILE` and the
/// real profile are both covered.
pub fn home_directories() -> Vec<PathBuf> {
    distinct_homes([platform_home(), std::env::home_dir()])
}

/// The profile known folder, which needs no other known folder (`BaseDirs`
/// would also need the AppData folders).
#[cfg(windows)]
fn platform_home() -> Option<PathBuf> {
    directories_next::UserDirs::new().map(|user| user.home_dir().to_path_buf())
}

/// `HOME` or the user's account entry, without reading any file (`UserDirs`
/// would also parse Linux's `user-dirs.dirs`, which can panic or block).
#[cfg(not(windows))]
fn platform_home() -> Option<PathBuf> {
    directories_next::BaseDirs::new().map(|base| base.home_dir().to_path_buf())
}

/// The absolute candidates, each once, in order. An empty path is not
/// absolute.
fn distinct_homes(candidates: impl IntoIterator<Item = Option<PathBuf>>) -> Vec<PathBuf> {
    let mut homes = Vec::new();
    for home in candidates.into_iter().flatten() {
        if home.is_absolute() && !homes.contains(&home) {
            homes.push(home);
        }
    }
    homes
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
    finish_data_directory(
        data_directory_location()?,
        |root| match moruno_data_directory() {
            Some(previous) => migrate_data(&previous, root),
            None => Ok(()),
        },
    )
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
