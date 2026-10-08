//! The CLI's file and standard-input access: [`read_input`] and
//! [`write_output`] are the only filesystem calls in `src/cli`.
//!
//! Paths given on the command line carry the user's own authority: unlike
//! the MCP file tools, there are no folder grants and no extension
//! allowlist.
use reshiki_agent::ops::budget::Budgets;
use std::{
    fs::File,
    io::{self, Read, Write},
    path::Path,
};

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

/// Input bytes as import text, as the app reads a structure file
/// (src/app/import.rs `contents`): cdx as base64, anything else as UTF-8.
pub(crate) fn text(format: &str, bytes: Vec<u8>) -> Result<String, String> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    if format == "cdx" {
        Ok(STANDARD.encode(bytes))
    } else {
        String::from_utf8(bytes).map_err(|error| format!("The file is not valid UTF-8: {error}"))
    }
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
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(failed)?;
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
