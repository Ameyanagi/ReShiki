//! Locate an already installed helper. Discovery never searches PATH or the
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
        application
            .parent()
            .filter(|p| p.is_absolute())
            .ok_or(Error::ApplicationPath)?
            .join(filename())
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
mod tests {
    use super::{Error, filename, resolve};
    use std::{ffi::OsStr, path::Path};

    fn executable(path: &Path) -> anyhow::Result<()> {
        std::fs::write(path, b"fixture; discovery must not execute this file")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
        }
        Ok(())
    }

    #[test]
    fn resolves_native_helper_beside_application_with_spaces_and_unicode() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        for folder in ["ReShiki portable 日本語", "ReShiki.app/Contents/MacOS"] {
            let directory = root.path().join(folder);
            std::fs::create_dir_all(&directory)?;
            let helper = directory.join(filename());
            executable(&helper)?;
            assert_eq!(
                resolve(&directory.join("reshiki"), None)?,
                helper.canonicalize()?
            );
        }
        Ok(())
    }

    #[test]
    fn explicit_development_override_must_exist_and_be_absolute() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let helper = root.path().join(filename());
        executable(&helper)?;
        let application = root.path().join("different/reshiki");
        assert_eq!(
            resolve(&application, Some(helper.as_os_str()))?,
            helper.canonicalize()?
        );
        for supplied in ["", "reshiki-inchi-helper", "../helper"] {
            assert!(matches!(
                resolve(&application, Some(OsStr::new(supplied))),
                Err(Error::RelativeOverride)
            ));
        }
        assert!(matches!(
            resolve(&application, Some(root.path().join("missing").as_os_str())),
            Err(Error::Unavailable { .. })
        ));
        assert!(matches!(
            resolve(&application, Some(root.path().as_os_str())),
            Err(Error::NotExecutable(_))
        ));
        Ok(())
    }

    #[test]
    fn missing_sibling_is_an_error_without_working_directory_fallback() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        executable(&root.path().join(filename()))?;
        assert!(matches!(
            resolve(&root.path().join("other/reshiki"), None),
            Err(Error::Unavailable { .. })
        ));
        assert!(matches!(
            resolve(Path::new("reshiki"), None),
            Err(Error::ApplicationPath)
        ));
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn rejects_nonexecutable_files() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        std::fs::write(root.path().join(filename()), b"not executable")?;
        assert!(matches!(
            resolve(&root.path().join("reshiki"), None),
            Err(Error::NotExecutable(_))
        ));
        Ok(())
    }
}
