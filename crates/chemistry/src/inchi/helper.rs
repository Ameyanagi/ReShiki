//! Relaunch the current application in worker mode. An explicit development
//! override can select a test executable; discovery never searches PATH or the
//! working directory and never starts a compiler, downloader, or interpreter.
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Could not locate the running application: {0}")]
    Application(#[source] std::io::Error),
    #[error("RESHIKI_INCHI_HELPER must be an absolute path to a built native helper")]
    RelativeOverride,
    #[error("The application executable has no absolute parent directory")]
    ApplicationPath,
    #[error("InChI helper is missing or inaccessible at {path}: {source}")]
    Unavailable {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("InChI helper is not a regular executable file: {0}")]
    NotExecutable(PathBuf),
}

pub fn filename() -> &'static str {
    if cfg!(windows) {
        "reshiki-inchi-helper.exe"
    } else {
        "reshiki-inchi-helper"
    }
}

/// Development builds can explicitly select an audited, already built helper.
/// An invalid override is an error; it never silently falls back to another file.
pub fn discover() -> Result<PathBuf, Error> {
    let application = std::env::current_exe().map_err(Error::Application)?;
    resolve(
        &application,
        std::env::var_os("RESHIKI_INCHI_HELPER").as_deref(),
    )
}

fn resolve(application: &Path, supplied: Option<&OsStr>) -> Result<PathBuf, Error> {
    let candidate = if let Some(supplied) = supplied {
        let candidate = PathBuf::from(supplied);
        if !candidate.is_absolute() {
            return Err(Error::RelativeOverride);
        }
        candidate
    } else {
        if !application.is_absolute() {
            return Err(Error::ApplicationPath);
        }
        application.to_path_buf()
    };
    let metadata = candidate.metadata().map_err(|source| Error::Unavailable {
        path: candidate.clone(),
        source,
    })?;
    if !metadata.is_file() {
        return Err(Error::NotExecutable(candidate));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err(Error::NotExecutable(candidate));
        }
    }
    candidate
        .canonicalize()
        .map_err(|source| Error::Unavailable {
            path: candidate,
            source,
        })
}

#[cfg(test)]
mod tests;
