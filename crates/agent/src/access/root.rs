//! Granted roots: opened once from their canonical path, then matched lexically.
use super::{AccessError, path::RequestPath};
use cap_std::fs::Dir;
use std::{
    fmt, io,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

/// The process exit code for a grant that cannot be honored at startup.
pub const GRANT_EXIT_CODE: i32 = 2;

/// One granted folder: the spelling the user gave, its canonical path and the
/// handle every request under it is resolved against.
#[derive(Debug, Clone)]
pub(crate) struct Root {
    pub(super) display: PathBuf,
    pub(super) canonical: PathBuf,
    pub(super) dir: Arc<Dir>,
}

impl Root {
    /// Open an absolute folder path. Callers resolve relative values first.
    pub(crate) fn open(path: &Path) -> Result<Self, GrantError> {
        Self::open_inner(path, &|| {})
    }

    /// [`Root::open`] with `hook` run after canonicalization, before the open.
    #[cfg(test)]
    pub(crate) fn open_with_hook(path: &Path, hook: &dyn Fn()) -> Result<Self, GrantError> {
        Self::open_inner(path, hook)
    }

    fn open_inner(path: &Path, hook: &dyn Fn()) -> Result<Self, GrantError> {
        if !path.is_absolute() {
            return Err(GrantError::Config(format!(
                "granted folders must be absolute paths: {}",
                path.display()
            )));
        }
        let canonical = std::fs::canonicalize(path).map_err(|error| open_error(path, error))?;
        let canonical = normalize(canonical)?;
        hook();
        // The crate's only ambient open, always of the canonical path.
        let dir = Dir::open_ambient_dir(&canonical, cap_std::ambient_authority())
            .map_err(|error| open_error(path, error))?;
        let unchanged = std::fs::canonicalize(&canonical)
            .ok()
            .and_then(|again| normalize(again).ok())
            .is_some_and(|again| again == canonical);
        if !unchanged || !same_directory(&dir, &canonical) {
            return Err(GrantError::Changed(path.to_path_buf()));
        }
        Ok(Self {
            display: path.to_path_buf(),
            canonical,
            dir: Arc::new(dir),
        })
    }

    fn display_string(&self) -> String {
        self.display.to_string_lossy().into_owned()
    }
}

fn open_error(path: &Path, error: io::Error) -> GrantError {
    match error.kind() {
        io::ErrorKind::NotFound => GrantError::NotFound(path.to_path_buf()),
        io::ErrorKind::NotADirectory => GrantError::NotADirectory(path.to_path_buf()),
        _ => GrantError::Config(format!("cannot open {}: {error}", path.display())),
    }
}

/// The opened handle must still be the directory at the canonical path.
#[cfg(unix)]
fn same_directory(dir: &Dir, canonical: &Path) -> bool {
    use cap_std::fs::MetadataExt as _;
    use std::os::unix::fs::MetadataExt as _;
    match (dir.dir_metadata(), std::fs::metadata(canonical)) {
        (Ok(opened), Ok(named)) => opened.dev() == named.dev() && opened.ino() == named.ino(),
        _ => false,
    }
}

/// cap-std exposes no handle identity on Windows (a documented residual risk).
#[cfg(not(unix))]
fn same_directory(_dir: &Dir, _canonical: &Path) -> bool {
    true
}

/// `canonicalize` returns extended-length syntax on Windows
/// (<https://doc.rust-lang.org/std/fs/fn.canonicalize.html>). Keep the plain
/// drive spelling, and refuse network and device folders.
#[cfg(windows)]
pub(super) fn normalize(canonical: PathBuf) -> Result<PathBuf, GrantError> {
    use std::path::Prefix;
    let mut components = canonical.components();
    let drive = match components.next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::VerbatimDisk(drive) => Some(drive),
            Prefix::Disk(_) => None,
            _ => return Err(GrantError::Unsupported(canonical.clone())),
        },
        _ => return Err(GrantError::Unsupported(canonical.clone())),
    };
    let Some(drive) = drive else {
        return Ok(canonical);
    };
    let mut plain = PathBuf::from(format!("{}:\\", char::from(drive.to_ascii_uppercase())));
    plain.extend(components.filter(|part| !matches!(part, Component::RootDir)));
    Ok(plain)
}

#[cfg(not(windows))]
pub(super) fn normalize(canonical: PathBuf) -> Result<PathBuf, GrantError> {
    Ok(canonical)
}

/// Folders granted for reading and, separately, for writing. Clones share
/// the opened handles.
#[derive(Debug, Clone)]
pub struct Grants {
    pub(super) read: Vec<Root>,
    pub(super) write: Vec<Root>,
}

/// Granted roots as the user spelled them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantSummary {
    pub read: Vec<String>,
    pub write: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Read,
    Write,
}

impl Grants {
    /// No folders: every file request is refused.
    pub fn none() -> Self {
        Self {
            read: Vec::new(),
            write: Vec::new(),
        }
    }

    /// Open every absolute folder once. Write roots never imply read.
    pub fn open(read: &[PathBuf], write: &[PathBuf]) -> Result<Self, GrantError> {
        let open = |paths: &[PathBuf]| -> Result<Vec<Root>, GrantError> {
            paths.iter().map(|path| Root::open(path)).collect()
        };
        Ok(Self {
            read: open(read)?,
            write: open(write)?,
        })
    }

    pub fn summary(&self) -> GrantSummary {
        GrantSummary {
            read: self.read.iter().map(Root::display_string).collect(),
            write: self.write.iter().map(Root::display_string).collect(),
        }
    }

    /// The root of `kind` with the longest whole-component prefix of the
    /// request, matched against both its display and canonical spelling, and
    /// the non-empty relative remainder.
    pub(super) fn locate(
        &self,
        kind: Kind,
        request: &RequestPath,
    ) -> Result<(&Root, PathBuf), AccessError> {
        let (roots, access) = match kind {
            Kind::Read => (&self.read, "reading"),
            Kind::Write => (&self.write, "writing"),
        };
        let mut best: Option<(usize, &Root, PathBuf)> = None;
        for root in roots {
            for spelling in [&root.display, &root.canonical] {
                if let Some((depth, rest)) = strip(spelling, &request.path)
                    && best.as_ref().is_none_or(|(longest, ..)| depth > *longest)
                {
                    best = Some((depth, root, rest));
                }
            }
        }
        best.map(|(_, root, rest)| (root, rest))
            .ok_or_else(|| AccessError::PathNotGranted {
                path: request.echo.clone(),
                access,
                roots: roots.iter().map(Root::display_string).collect(),
            })
    }
}

/// The number of `root` components and the remainder, if `root` is a proper
/// whole-component prefix of `path`.
fn strip(root: &Path, path: &Path) -> Option<(usize, PathBuf)> {
    let mut rest = path.components();
    let mut depth = 0;
    for part in root.components() {
        if !same_component(part, rest.next()?) {
            return None;
        }
        depth += 1;
    }
    let rest: PathBuf = rest.collect();
    (!rest.as_os_str().is_empty()).then_some((depth, rest))
}

/// Unix compares exactly; a case or Unicode-normalization variant fails closed.
#[cfg(not(windows))]
fn same_component(root: Component<'_>, request: Component<'_>) -> bool {
    root == request
}

/// Windows compares drive letters and names ASCII-case-insensitively.
#[cfg(windows)]
fn same_component(root: Component<'_>, request: Component<'_>) -> bool {
    root.as_os_str().eq_ignore_ascii_case(request.as_os_str())
}

/// Why a folder cannot be granted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GrantError {
    NotFound(PathBuf),
    NotADirectory(PathBuf),
    Changed(PathBuf),
    Unsupported(PathBuf),
    TooBroad(PathBuf),
    Protected(PathBuf),
    Config(String),
}

impl fmt::Display for GrantError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(path) => write!(f, "granted folder not found: {}", path.display()),
            Self::NotADirectory(path) => {
                write!(f, "granted path is not a folder: {}", path.display())
            }
            Self::Changed(path) => write!(
                f,
                "the folder changed while it was being opened: {}",
                path.display()
            ),
            Self::Unsupported(path) => write!(
                f,
                "network and device folders cannot be granted: {}",
                path.display()
            ),
            Self::TooBroad(path) => {
                write!(f, "folder is too broad to grant: {}", path.display())
            }
            Self::Protected(path) => write!(
                f,
                "folder is protected and cannot be granted: {}",
                path.display()
            ),
            Self::Config(text) => write!(f, "invalid grant configuration: {text}"),
        }
    }
}

impl std::error::Error for GrantError {}
