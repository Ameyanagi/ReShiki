//! The CLI's file and standard-input access: [`read_input`] and
//! [`write_output`] and [`write_skill`] keep filesystem calls in `src/cli` here.
//!
//! Paths given on the command line carry the user's own authority: unlike
//! the MCP file tools, there are no folder grants and no extension
//! allowlist.
use reshiki_agent::ops::budget::Budgets;
use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;

/// The most bytes read for an import `format`: [`Budgets::max_text_bytes`],
/// or for cdx the raw size whose base64 fits [`Budgets::max_cdx_base64`].
pub(crate) fn limit(format: &str, budgets: &Budgets) -> usize {
    if format == "cdx" {
        budgets.max_cdx_base64 / 4 * 3
    } else {
        budgets.max_text_bytes
    }
}

/// Reads the file at `path`, or standard input for `None`, refusing more
/// than `limit` bytes.
pub(crate) fn read_input(path: Option<&Path>, limit: usize) -> Result<Vec<u8>, String> {
    let cap = u64::try_from(limit).unwrap_or(u64::MAX).saturating_add(1);
    let mut bytes = Vec::new();
    let read = match path {
        Some(path) => File::open(path).and_then(|file| file.take(cap).read_to_end(&mut bytes)),
        None => io::stdin().lock().take(cap).read_to_end(&mut bytes),
    };
    read.map_err(|error| match path {
        Some(path) => format!("cannot read {}: {error}", path.display()),
        None => format!("cannot read standard input: {error}"),
    })?;
    if bytes.len() > limit {
        return Err(format!("input exceeds {limit} bytes"));
    }
    Ok(bytes)
}

/// Input bytes through [`crate::compatibility::import_text`]: cdx as base64,
/// anything else as UTF-8.
pub(crate) fn text(format: &str, bytes: Vec<u8>) -> Result<String, String> {
    crate::compatibility::import_text(format, bytes)
}

/// Writes `bytes` to `path` through a synced temporary file beside it, so
/// `path` is either untouched or complete. Without `force` an existing
/// `path` is kept and is an error; with it, the file replaces `path`.
pub(crate) fn write_output(path: &Path, bytes: &[u8], force: bool) -> Result<(), String> {
    let failed = |error: io::Error| format!("cannot write {}: {error}", path.display());
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut file = temporary_in(parent).map_err(failed)?;
    file.write_all(bytes).map_err(failed)?;
    file.as_file().sync_all().map_err(failed)?;
    let persisted = if force {
        file.persist(path)
    } else {
        file.persist_noclobber(path)
    };
    // A failed persist drops the temporary file, which deletes it.
    match persisted {
        Ok(_) => Ok(()),
        Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => Err(format!(
            "{} exists; pass --force to replace",
            path.display()
        )),
        Err(error) => Err(failed(error.error)),
    }
}

/// Writes the skill `files` (paths relative to the skill, `/`-separated)
/// into `folder/name` and returns that path.
///
/// `folder` must be an existing directory and is never created. Without
/// `force` an existing `folder/name` is an error and nothing is written.
/// With it, `folder/name` and each generated subfolder that exists must be
/// a real directory, never a symbolic link, checked before any file is
/// written: [`write_output`] writes through the lexical parent, which a
/// symbolic link would redirect. Missing folders are made one level at a
/// time and each file goes through [`write_output`], so a failure part-way
/// leaves the files already written. Nothing is ever deleted.
pub(crate) fn write_skill(
    folder: &Path,
    name: &str,
    files: &[(&str, Vec<u8>)],
    force: bool,
) -> Result<PathBuf, String> {
    let metadata = fs::metadata(folder)
        .map_err(|error| format!("cannot open the folder {}: {error}", folder.display()))?;
    if !metadata.is_dir() {
        return Err(format!("{} is not a folder", folder.display()));
    }
    let root = folder.join(name);
    let mut directories = vec![root.clone()];
    for (path, _) in files {
        let mut directory = root.clone();
        let mut parts: Vec<_> = path.split('/').collect();
        parts.pop();
        for part in parts {
            directory.push(part);
            if !directories.contains(&directory) {
                directories.push(directory.clone());
            }
        }
    }
    let mut missing = Vec::new();
    for directory in &directories {
        match fs::symlink_metadata(directory) {
            Ok(_) if directory == &root && !force => {
                return Err(format!(
                    "{} exists; pass --force to replace",
                    root.display()
                ));
            }
            Ok(metadata) if !metadata.file_type().is_dir() => {
                return Err(format!(
                    "{} is a symbolic link or not a folder; refusing to write the skill",
                    directory.display()
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => missing.push(directory),
            Err(error) => return Err(format!("cannot open {}: {error}", directory.display())),
        }
    }
    for directory in missing {
        fs::create_dir(directory)
            .map_err(|error| format!("cannot create {}: {error}", directory.display()))?;
    }
    for (path, bytes) in files {
        let target = path
            .split('/')
            .fold(root.clone(), |target, part| target.join(part));
        write_output(&target, bytes, force)?;
    }
    Ok(root)
}

/// An empty temporary file in `parent`. On Unix its mode is 0666 less the
/// umask, the mode `File::create` gives a new file, rather than tempfile's
/// private 0600, so the published output gets the user's usual mode.
#[cfg(unix)]
fn temporary_in(parent: &Path) -> io::Result<NamedTempFile> {
    use std::{fs::Permissions, os::unix::fs::PermissionsExt};
    tempfile::Builder::new()
        .permissions(Permissions::from_mode(0o666))
        .tempfile_in(parent)
}

/// An empty temporary file in `parent`.
#[cfg(not(unix))]
fn temporary_in(parent: &Path) -> io::Result<NamedTempFile> {
    NamedTempFile::new_in(parent)
}
