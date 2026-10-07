//! Folders that can never be granted, and the startup entry point that
//! combines every grant source.
use super::{
    GrantError, Grants,
    config::AccessConfig,
    root::{Root, normalize, within},
};
use reshiki_io::compatibility::{
    data_directory_location, home_directory, legacy_data_directory_location,
};
use std::{
    io,
    path::{Component, Path, PathBuf},
};

/// Pseudo file systems that expose the system and other processes.
#[cfg(unix)]
const SYSTEM_TREES: [&str; 3] = ["/proc", "/sys", "/dev"];

/// Locations no root may be, contain or lie inside, projected to canonical
/// form even when they do not exist yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Protected {
    /// ReShiki's own data, earlier installations' data and the executable.
    pub(super) paths: Vec<PathBuf>,
    /// Refused itself and with every ancestor; folders inside it are allowed.
    pub(super) home: Option<PathBuf>,
    /// Windows: refused with everything inside it.
    pub(super) system_root: Option<PathBuf>,
}

impl Protected {
    /// The locations of this installation, process and user.
    pub fn current() -> Result<Self, GrantError> {
        let data = data_directory_location().map_err(GrantError::Config)?.path;
        let mut paths = vec![data];
        paths.extend(legacy_data_directory_location());
        let executable = std::env::current_exe().map_err(|error| {
            GrantError::Config(format!("cannot locate the ReShiki executable: {error}"))
        })?;
        // The canonical executable, so a symlinked launcher protects the
        // real installation folder.
        let executable = project(&executable)?;
        if let Some(folder) = executable.parent() {
            paths.push(folder.to_path_buf());
            #[cfg(target_os = "macos")]
            paths.extend(app_bundle(folder));
        }
        #[cfg(windows)]
        let system_root = std::env::var_os("SystemRoot").map(PathBuf::from);
        #[cfg(not(windows))]
        let system_root = None;
        Self::new(paths, home_directory(), system_root)
    }

    /// Protected locations from explicit paths, projected as
    /// [`Protected::current`] projects its own. For tests and embedders.
    ///
    /// A location that cannot be resolved (other than by being absent) is an
    /// error: its real place is unknown, so nothing could be checked against it.
    pub fn new(
        paths: Vec<PathBuf>,
        home: Option<PathBuf>,
        system_root: Option<PathBuf>,
    ) -> Result<Self, GrantError> {
        Ok(Self {
            paths: paths
                .iter()
                .map(|path| project(path))
                .collect::<Result<_, _>>()?,
            home: home.as_deref().map(project).transpose()?,
            system_root: system_root.as_deref().map(project).transpose()?,
        })
    }

    /// Refuse a root whose canonical path is too broad or protected.
    pub(super) fn check(&self, root: &Root) -> Result<(), GrantError> {
        let canonical = root.canonical.as_path();
        let too_broad = || GrantError::TooBroad(root.display.clone());
        let protected = || GrantError::Protected(root.display.clone());
        // A file system or drive root.
        if canonical.parent().is_none() {
            return Err(too_broad());
        }
        if self
            .home
            .as_deref()
            .is_some_and(|home| within(home, canonical))
        {
            return Err(too_broad());
        }
        if self
            .paths
            .iter()
            .any(|path| within(canonical, path) || within(path, canonical))
        {
            return Err(protected());
        }
        #[cfg(unix)]
        if SYSTEM_TREES
            .iter()
            .any(|tree| within(canonical, Path::new(tree)))
        {
            return Err(protected());
        }
        if self
            .system_root
            .as_deref()
            .is_some_and(|system| within(canonical, system))
        {
            return Err(protected());
        }
        Ok(())
    }
}

/// The canonical form of `path`, whether it exists or not: its longest
/// existing ancestor canonicalized, then the missing components appended
/// lexically. Windows paths get [`Root::open`]'s plain drive spelling.
///
/// Only absence (not found, or under a file) moves on to a shorter
/// ancestor. Any other failure, such as a folder without search permission,
/// may hide an existing symlink whose target is unknown, so it is an error.
// Protected locations lie outside every grant by definition, so they are
// resolved with ambient authority; nothing is opened or created.
#[allow(clippy::disallowed_methods)]
pub(super) fn project(path: &Path) -> Result<PathBuf, GrantError> {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let components: Vec<Component<'_>> = absolute.components().collect();
    for existing in (1..=components.len()).rev() {
        let Some((known, missing)) = components.split_at_checked(existing) else {
            continue;
        };
        let known: PathBuf = known.iter().collect();
        let canonical = match std::fs::canonicalize(&known) {
            Ok(canonical) => canonical,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
                ) =>
            {
                continue;
            }
            Err(error) => {
                return Err(GrantError::Config(format!(
                    "cannot resolve the protected location {}: {error}",
                    path.display()
                )));
            }
        };
        // A network or device location stays as the operating system spells
        // it; no grantable root can match it either way.
        let mut projected = normalize(canonical.clone()).unwrap_or(canonical);
        for part in missing {
            match part {
                Component::ParentDir => {
                    projected.pop();
                }
                Component::Normal(name) => projected.push(name),
                Component::CurDir | Component::RootDir | Component::Prefix(_) => {}
            }
        }
        return Ok(projected);
    }
    Ok(absolute)
}

/// The `.app` bundle around a macOS executable folder `X.app/Contents/MacOS`.
#[cfg(any(target_os = "macos", test))]
pub(super) fn app_bundle(folder: &Path) -> Option<PathBuf> {
    if !folder.ends_with("Contents/MacOS") {
        return None;
    }
    let bundle = folder.parent()?.parent()?;
    bundle
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
        .then(|| bundle.to_path_buf())
}

/// Every source of grants for one process.
#[derive(Debug, Clone)]
pub struct GrantSources {
    pub config: AccessConfig,
    pub cli_read: Vec<PathBuf>,
    pub cli_write: Vec<PathBuf>,
    /// Relative command-line folders resolve against this.
    pub cwd: PathBuf,
}

impl Grants {
    /// Open the union of the configuration's and the command line's folders,
    /// once per canonical path, refusing any root `protected` rules out.
    pub fn from_sources(sources: &GrantSources, protected: &Protected) -> Result<Self, GrantError> {
        let resolve = |cli: &[PathBuf]| -> Vec<PathBuf> {
            cli.iter().map(|path| sources.cwd.join(path)).collect()
        };
        let read = sources
            .config
            .read
            .iter()
            .cloned()
            .chain(resolve(&sources.cli_read));
        let write = sources
            .config
            .write
            .iter()
            .cloned()
            .chain(resolve(&sources.cli_write));
        Ok(Self {
            read: open_unique(read, protected)?,
            write: open_unique(write, protected)?,
        })
    }
}

fn open_unique(
    paths: impl Iterator<Item = PathBuf>,
    protected: &Protected,
) -> Result<Vec<Root>, GrantError> {
    let mut roots: Vec<Root> = Vec::new();
    for path in paths {
        let root = Root::open(&path)?;
        protected.check(&root)?;
        if roots.iter().all(|kept| kept.canonical != root.canonical) {
            roots.push(root);
        }
    }
    Ok(roots)
}

/// The grants for `reshiki --mcp`: `agent-access.json` in the data directory
/// plus the command line's `--allow-read` and `--allow-write` folders
/// (relative ones resolve against `cwd`), checked against
/// [`Protected::current`].
///
/// Caller contract: on `Err`, print one line to stderr and exit with
/// [`GRANT_EXIT_CODE`](super::GRANT_EXIT_CODE) before reading stdin. A
/// configuration that cannot be read is an error, never "no grants".
pub fn load(
    cli_read: Vec<PathBuf>,
    cli_write: Vec<PathBuf>,
    cwd: PathBuf,
) -> Result<Grants, GrantError> {
    let sources = GrantSources {
        config: AccessConfig::load()?,
        cli_read,
        cli_write,
        cwd,
    };
    Grants::from_sources(&sources, &Protected::current()?)
}
