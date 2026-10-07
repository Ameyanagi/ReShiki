//! `agent-access.json`: folders granted in the data directory.
//!
//! The file is user-owned. ReShiki never writes it in P1 (P2's Settings UI
//! will), and it is read once, at startup:
//!
//! ```json
//! { "version": 1, "read": ["/Users/me/Molecules"], "write": ["/Users/me/Figures"] }
//! ```
//!
//! `read` and `write` may be omitted; every path must be absolute.
use super::GrantError;
use reshiki_io::compatibility::data_directory_location;
use serde::Deserialize;
use std::{
    io::{self, Read},
    path::{Path, PathBuf},
};

/// The file name inside the data directory.
pub(super) const FILE_NAME: &str = "agent-access.json";
/// The only supported `version`.
const VERSION: u32 = 1;
/// Larger files are refused rather than parsed.
const MAX_BYTES: usize = 64 * 1024;

/// Folders granted by `agent-access.json`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccessConfig {
    pub(super) version: u32,
    #[serde(default)]
    pub(super) read: Vec<PathBuf>,
    #[serde(default)]
    pub(super) write: Vec<PathBuf>,
}

impl AccessConfig {
    /// No folders, as when the file does not exist.
    pub fn empty() -> Self {
        Self {
            version: VERSION,
            read: Vec::new(),
            write: Vec::new(),
        }
    }

    /// Read `agent-access.json` from the data directory. A missing data
    /// directory location is an error, never an empty grant set.
    pub fn load() -> Result<Self, GrantError> {
        let location = data_directory_location()
            .map_err(|error| GrantError::Config(format!("{FILE_NAME}: {error}")))?;
        Self::load_from(&location.path)
    }

    /// Read `<dir>/agent-access.json`. A missing file is an empty
    /// configuration; anything else that is not a valid regular file of at
    /// most 64 KiB is an error naming the file.
    ///
    /// Symlinks are followed: the data directory is user-owned.
    // The data directory is outside every grant, so this is one of the
    // access module's few ambient reads: a single bounded, non-blocking open.
    #[allow(clippy::disallowed_methods, clippy::disallowed_types)]
    pub fn load_from(dir: &Path) -> Result<Self, GrantError> {
        let file = dir.join(FILE_NAME);
        let fail = |reason: String| GrantError::Config(format!("{}: {reason}", file.display()));
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        // A FIFO opens without waiting for a writer and is then refused; a
        // terminal never becomes the controlling one.
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::custom_flags(
            &mut options,
            libc::O_NONBLOCK | libc::O_NOCTTY,
        );
        let opened = match options.open(&file) {
            Ok(opened) => opened,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::empty()),
            Err(error) => return Err(fail(error.to_string())),
        };
        let metadata = opened.metadata().map_err(|error| fail(error.to_string()))?;
        if !metadata.is_file() {
            return Err(fail("not a regular file".into()));
        }
        let mut bytes = Vec::new();
        opened
            .take(MAX_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| fail(error.to_string()))?;
        if bytes.len() > MAX_BYTES {
            return Err(fail(format!("larger than {MAX_BYTES} bytes")));
        }
        let config: Self =
            serde_json::from_slice(&bytes).map_err(|error| fail(error.to_string()))?;
        config.validate().map_err(fail)?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), String> {
        if self.version != VERSION {
            return Err(format!(
                "unsupported version {} (expected {VERSION})",
                self.version
            ));
        }
        match self
            .read
            .iter()
            .chain(&self.write)
            .find(|path| !path.is_absolute())
        {
            Some(path) => Err(format!(
                "granted folders must be absolute paths: {}",
                path.display()
            )),
            None => Ok(()),
        }
    }
}
