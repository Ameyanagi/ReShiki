//! Atomic writes inside write roots: a temporary file beside the destination,
//! then a no-clobber hard link (CreateNew) or a rename (Replace).
use super::{
    AccessError, Grants, invalid, io_error,
    path::{check_extension, parse},
    resolution_error,
    root::Kind,
};
use cap_std::fs::{Dir, File, OpenOptions};
use std::{
    ffi::OsStr,
    io::{self, Write},
    sync::atomic::{AtomicU64, Ordering},
};

const MAX_NAME_BYTES: usize = 255;
const TEMP_ATTEMPTS: usize = 16;
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// How the destination name is published.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteMode {
    /// Fail with `file_exists` if anything already has the name.
    CreateNew,
    /// Replace a regular file, or create one.
    Replace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteReceipt {
    pub path: String,
    pub bytes: usize,
    /// Whether a regular file had the name before the write.
    pub replaced: bool,
    pub warnings: Vec<String>,
}

/// Deterministic interception points for race tests; empty outside tests.
#[derive(Default)]
pub(super) struct Hooks<'a> {
    /// Runs after the temporary file is complete, just before publication.
    #[cfg(test)]
    pub(super) before_publish: Option<&'a dyn Fn()>,
    /// Leaves the temporary file behind, as an interrupted call would.
    #[cfg(test)]
    pub(super) skip_temp_removal: bool,
    #[cfg(not(test))]
    _none: std::marker::PhantomData<&'a ()>,
}

impl Hooks<'_> {
    fn before_publish(&self) {
        #[cfg(test)]
        if let Some(hook) = self.before_publish {
            hook();
        }
    }

    #[cfg(test)]
    fn keep_temp(&self) -> bool {
        self.skip_temp_removal
    }

    #[cfg(not(test))]
    fn keep_temp(&self) -> bool {
        false
    }
}

impl Grants {
    /// Write `bytes` to a file whose extension is one of `extensions`.
    ///
    /// The destination name only ever appears with complete contents. On
    /// failure the destination is left as it was; only this call's temporary
    /// file is removed (best effort). The folder itself is not synced, as in
    /// `reshiki_model::storage::write_atomic`.
    pub fn write_atomic(
        &self,
        path: &str,
        bytes: &[u8],
        mode: WriteMode,
        extensions: &[&str],
    ) -> Result<WriteReceipt, AccessError> {
        self.write_with(path, bytes, mode, extensions, &Hooks::default())
    }

    pub(super) fn write_with(
        &self,
        path: &str,
        bytes: &[u8],
        mode: WriteMode,
        extensions: &[&str],
        hooks: &Hooks<'_>,
    ) -> Result<WriteReceipt, AccessError> {
        let request = parse(path)?;
        let echo = request.echo.as_str();
        check_extension(&request, extensions)?;
        let name = request
            .path
            .file_name()
            .ok_or_else(|| invalid(path, "trailing_separator"))?;
        check_name(name, path)?;
        let (root, rel) = self.locate(Kind::Write, &request)?;
        // Never create folders: the parent must already exist.
        let opened;
        let parent = match rel.parent().filter(|parent| !parent.as_os_str().is_empty()) {
            Some(parent) => {
                opened = root
                    .dir
                    .open_dir(parent)
                    .map_err(|error| resolution_error(error, echo))?;
                &opened
            }
            None => &*root.dir,
        };
        let (temp, file) = create_temp(parent, echo)?;
        let staged = Staged {
            parent,
            temp: &temp,
            hooks,
        };
        if let Err(error) = fill(file, bytes) {
            staged.discard();
            return Err(io_error(error, echo));
        }
        let published = match mode {
            WriteMode::CreateNew => staged.link(name, echo),
            WriteMode::Replace => staged.rename(name, echo),
        };
        published.map(|(replaced, warnings)| WriteReceipt {
            path: path.to_owned(),
            bytes: bytes.len(),
            replaced,
            warnings,
        })
    }
}

/// Final-name rules beyond the lexical path checks.
fn check_name(name: &OsStr, path: &str) -> Result<(), AccessError> {
    let name = name.to_string_lossy();
    let rule = if name.starts_with('.') {
        "leading_dot"
    } else if name.len() > MAX_NAME_BYTES {
        "name_too_long"
    } else if name.chars().any(char::is_control) {
        "control_character"
    } else if cfg!(windows) && name.contains(['<', '>', ':', '"', '|', '?', '*']) {
        "reserved_character"
    } else {
        return Ok(());
    };
    Err(invalid(path, rule))
}

/// Create `.reshiki-<pid>-<n>.tmp` in `parent`, never reusing a name.
fn create_temp(parent: &Dir, echo: &str) -> Result<(String, File), AccessError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    for _ in 0..TEMP_ATTEMPTS {
        let name = format!(
            ".reshiki-{}-{}.tmp",
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        match parent.open_with(&name, &options) {
            Ok(file) => return Ok((name, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(resolution_error(error, echo)),
        }
    }
    Err(AccessError::Io {
        path: echo.into(),
        message: "could not create a unique temporary file".into(),
    })
}

/// Write, sync, then close.
fn fill(mut file: File, bytes: &[u8]) -> io::Result<()> {
    file.write_all(bytes)?;
    file.sync_all()
}

/// A complete temporary file awaiting publication.
struct Staged<'a> {
    parent: &'a Dir,
    temp: &'a str,
    hooks: &'a Hooks<'a>,
}

impl Staged<'_> {
    /// Best-effort removal of this call's temporary file, never the destination.
    fn discard(&self) {
        if !self.hooks.keep_temp() {
            let _ = self.parent.remove_file(self.temp);
        }
    }

    /// Publish without clobbering: the link fails if anything has the name.
    fn link(&self, name: &OsStr, echo: &str) -> Result<(bool, Vec<String>), AccessError> {
        self.hooks.before_publish();
        if let Err(error) = self.parent.hard_link(self.temp, self.parent, name) {
            self.discard();
            return Err(match error.kind() {
                io::ErrorKind::AlreadyExists => AccessError::FileExists { path: echo.into() },
                _ if no_hard_links(&error) => {
                    AccessError::NoClobberUnsupported { path: echo.into() }
                }
                _ => resolution_error(error, echo),
            });
        }
        let mut warnings = Vec::new();
        if !self.hooks.keep_temp()
            && let Err(error) = self.parent.remove_file(self.temp)
        {
            warnings.push(format!(
                "the temporary file {} could not be removed: {error}",
                self.temp
            ));
        }
        Ok((false, warnings))
    }

    /// Replace a regular file (or create one) by renaming over the name.
    fn rename(&self, name: &OsStr, echo: &str) -> Result<(bool, Vec<String>), AccessError> {
        let replaced = match self.parent.symlink_metadata(name) {
            Ok(existing) if existing.is_file() => true,
            Ok(_) => {
                self.discard();
                return Err(AccessError::NotARegularFile { path: echo.into() });
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => false,
            Err(error) => {
                self.discard();
                return Err(resolution_error(error, echo));
            }
        };
        self.hooks.before_publish();
        if let Err(error) = self.parent.rename(self.temp, self.parent, name) {
            self.discard();
            return Err(resolution_error(error, echo));
        }
        Ok((replaced, Vec::new()))
    }
}

/// FAT and exFAT have no hard links.
fn no_hard_links(error: &io::Error) -> bool {
    if error.kind() == io::ErrorKind::Unsupported {
        return true;
    }
    let Some(code) = error.raw_os_error() else {
        return false;
    };
    #[cfg(unix)]
    {
        [libc::EPERM, libc::ENOTSUP, libc::EOPNOTSUPP].contains(&code)
    }
    #[cfg(windows)]
    {
        /// ERROR_INVALID_FUNCTION
        const INVALID_FUNCTION: i32 = 1;
        code == INVALID_FUNCTION
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = code;
        false
    }
}
