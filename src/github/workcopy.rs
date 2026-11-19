//! The files of a repository on this computer: the drawings the user opened,
//! what GitHub last had for each, and the saves made since the last send.
//!
//! ```text
//! <data>/github/<owner>/<repo>/
//!   state.json
//!   files/<repo path>          the drawings (a tab's path)
//!   versions/<key>/<id>.rsk    saves since the last send, newest id last
//!   old/<commit>/<repo path>   a drawing as it was at a branch or commit,
//!                              opened by link: never sent
//! ```
//! Whether a drawing is unsent is never stored: it is unsent when its file
//! differs from the bytes last synced, so a crash, an edit made elsewhere and
//! a save that changed nothing all come out right. Restoring the GitHub version
//! a drawing is based on counts the file Restore wrote as synced.
use super::RepoKey;
use crate::storage;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex, PoisonError},
    time::{SystemTime, UNIX_EPOCH},
};

const VERSION: u32 = 1;
const NEWER: &str = "Made by a newer ReShiki.";
/// Saves kept per drawing.
const KEPT: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoState {
    pub version: u32,
    pub repo_id: u64,
    /// GitHub's current names; the folder keeps the names it was made with.
    pub owner: String,
    pub repo: String,
    pub branch: String,
    pub private: bool,
    /// The user may not send to it: someone else's, or read signed out.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub read_only: bool,
    pub files: BTreeMap<String, FileState>,
}

/// What GitHub had for a drawing when it was last opened or sent.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileState {
    pub base_commit: String,
    /// `None`: the drawing is not on GitHub yet.
    pub base_blob: Option<String>,
    /// SHA-256 in hex of the bytes last synced or of the file Restore wrote
    /// for the same version in this ReShiki's format.
    pub base_hash: String,
    pub conflict: Option<ConflictMark>,
}

/// GitHub's version that a send found in the way.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictMark {
    pub commit: String,
    /// `None`: deleted on GitHub.
    pub blob: Option<String>,
}

/// One save kept in `versions/`. Ids are save times, so they sort by age.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveMeta {
    pub id: String,
    pub path: PathBuf,
}

impl RepoState {
    pub fn new(repo: &super::Repo) -> Self {
        Self {
            version: VERSION,
            repo_id: repo.id,
            owner: repo.key.owner.clone(),
            repo: repo.key.repo.clone(),
            branch: repo.default_branch.clone(),
            private: repo.private,
            read_only: !repo.push,
            files: BTreeMap::new(),
        }
    }

    /// The repository as GitHub names it now.
    pub fn key(&self) -> RepoKey {
        RepoKey {
            owner: self.owner.clone(),
            repo: self.repo.clone(),
        }
    }

    /// A tracked drawing whose name differs from `repo_path` only by case:
    /// macOS and Windows would keep both in one file.
    pub fn collides(&self, repo_path: &str) -> bool {
        self.files.keys().any(|tracked| {
            tracked != repo_path && tracked.to_lowercase() == repo_path.to_lowercase()
        })
    }
}

pub fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// `<data>/github`. Every other function takes it, so nothing else reads the
/// environment.
pub fn root() -> Result<PathBuf, String> {
    Ok(crate::compatibility::data_directory()?.join("github"))
}

fn repo_dir(root: &Path, key: &RepoKey) -> PathBuf {
    root.join(&key.owner).join(&key.repo)
}

/// Where the drawing at `repo_path` is kept on this computer.
pub fn working_path(root: &Path, key: &RepoKey, repo_path: &str) -> PathBuf {
    let mut path = repo_dir(root, key).join("files");
    path.extend(repo_path.split('/'));
    path
}

/// The repository folder and repository path of a working copy, or `None`
/// for any other file.
pub fn locate(root: &Path, path: &Path) -> Option<(RepoKey, String)> {
    let mut parts = path.strip_prefix(root).ok()?.iter();
    let owner = parts.next()?.to_str()?.to_owned();
    let repo = parts.next()?.to_str()?.to_owned();
    if parts.next()? != "files" {
        return None;
    }
    let rest: Option<Vec<&str>> = parts.map(|part| part.to_str()).collect();
    let repo_path = rest?.join("/");
    valid_repo_path(&repo_path).then_some((RepoKey { owner, repo }, repo_path))
}

/// Where the drawing at `repo_path` is kept as it was at `commit`.
pub fn old_path(root: &Path, key: &RepoKey, commit: &str, repo_path: &str) -> PathBuf {
    let short: String = commit.chars().take(12).collect();
    let mut path = repo_dir(root, key).join("old").join(short);
    path.extend(repo_path.split('/'));
    path
}

/// The repository folder and repository path of an old version, or `None`
/// for any other file.
pub fn locate_old(root: &Path, path: &Path) -> Option<(RepoKey, String)> {
    let mut parts = path.strip_prefix(root).ok()?.iter();
    let owner = parts.next()?.to_str()?.to_owned();
    let repo = parts.next()?.to_str()?.to_owned();
    if parts.next()? != "old" {
        return None;
    }
    parts.next()?;
    let rest: Option<Vec<&str>> = parts.map(|part| part.to_str()).collect();
    let repo_path = rest?.join("/");
    valid_repo_path(&repo_path).then_some((RepoKey { owner, repo }, repo_path))
}

/// One rule on every platform, so a working copy can be kept anywhere: no
/// way out of the folder, and no name Windows cannot hold.
pub fn valid_repo_path(path: &str) -> bool {
    !path.is_empty()
        && path.split('/').all(|name| {
            let stem = name
                .split('.')
                .next()
                .unwrap_or_default()
                .to_ascii_uppercase();
            let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
                || stem.len() == 4
                    && (stem.starts_with("COM") || stem.starts_with("LPT"))
                    && stem.ends_with(|c: char| ('1'..='9').contains(&c));
            !matches!(name, "" | "." | "..")
                && !reserved
                && !name.ends_with(['.', ' '])
                && !name
                    .chars()
                    .any(|c| c.is_control() || r#"\<>:"|?*"#.contains(c))
        })
}

fn read_state(path: &Path) -> Result<Option<RepoState>, String> {
    #[derive(Deserialize)]
    struct Probe {
        version: u32,
    }
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let probe: Probe = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    if probe.version > VERSION {
        return Err(NEWER.into());
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| error.to_string())
}

/// The folder that `repo`'s drawings went to when it was first opened here,
/// found by its id, so a rename on GitHub keeps them; for a repository new
/// here, its names now, or those with its id added while another
/// repository's folder has them.
pub fn folder(root: &Path, repo: &super::Repo) -> RepoKey {
    let names = |dir: &Path| -> Vec<String> {
        std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| entry.file_name().into_string().ok())
            .collect()
    };
    for owner in names(root) {
        for name in names(&root.join(&owner)) {
            let key = RepoKey {
                owner: owner.clone(),
                repo: name,
            };
            if load(root, &key).is_ok_and(|state| state.is_some_and(|s| s.repo_id == repo.id)) {
                return key;
            }
        }
    }
    // A folder under these names that holds another repository (renamed on
    // GitHub since, its old name now given to this one) is never shared.
    let mut key = repo.key.clone();
    while load(root, &key).is_ok_and(|state| state.is_some_and(|s| s.repo_id != repo.id)) {
        key.repo = format!("{}-{}", key.repo, repo.id);
    }
    key
}

/// Every repository with drawings kept here, as last known, with its folder.
pub fn known(root: &Path) -> Vec<(RepoKey, RepoState)> {
    let folders = |dir: &Path| {
        std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .collect::<Vec<_>>()
    };
    folders(root)
        .iter()
        .flat_map(|owner| folders(owner))
        .filter_map(|repo| {
            let state = read_state(&repo.join("state.json")).ok()??;
            let name = |path: &Path| Some(path.file_name()?.to_str()?.to_owned());
            let key = RepoKey {
                owner: name(repo.parent()?)?,
                repo: name(&repo)?,
            };
            Some((key, state))
        })
        .collect()
}

/// `None`: nothing from this repository has been opened here.
pub fn load(root: &Path, key: &RepoKey) -> Result<Option<RepoState>, String> {
    read_state(&repo_dir(root, key).join("state.json"))
}

/// Write `state.json`. Writes may finish out of order, so each carries the
/// caller's rising `seq`, and one older than the last written is dropped.
/// A state file from a newer ReShiki is never replaced.
pub async fn persist(root: &Path, key: &RepoKey, state: RepoState, seq: u64) -> Result<(), String> {
    static WRITTEN: LazyLock<tokio::sync::Mutex<HashMap<PathBuf, u64>>> =
        LazyLock::new(Default::default);
    let dir = repo_dir(root, key);
    let mut written = WRITTEN.lock().await;
    if written.get(&dir).is_some_and(|last| *last > seq) {
        return Ok(());
    }
    let path = dir.join("state.json");
    let write = move || {
        read_state(&path)?;
        std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
        let bytes = serde_json::to_vec_pretty(&state).map_err(|error| error.to_string())?;
        storage::write_atomic(&path, &bytes)?;
        Ok::<_, String>(dir)
    };
    let dir = crate::task::spawn_blocking(write)
        .await
        .map_err(|error| error.to_string())??;
    written.insert(dir, seq);
    Ok(())
}

/// The drawings whose file here differs from what was last synced. A missing
/// file is not one of them: it is never taken for a deletion.
pub fn unsent(root: &Path, key: &RepoKey, state: &RepoState) -> Result<Vec<String>, String> {
    let mut unsent = Vec::new();
    for (path, file) in &state.files {
        match std::fs::read(working_path(root, key, path)) {
            Ok(bytes) if hash(&bytes) != file.base_hash => unsent.push(path.clone()),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            // A drawing that cannot be read now must not look sent.
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(unsent)
}

/// What each of `paths` holds here now, as its [`hash`]; `None`: no such
/// file. One that cannot be read is left out. Blocking.
pub fn hashes(root: &Path, key: &RepoKey, paths: Vec<String>) -> Vec<(String, Option<String>)> {
    paths
        .into_iter()
        .filter_map(|path| match std::fs::read(working_path(root, key, &path)) {
            Ok(bytes) => Some((path, Some(hash(&bytes)))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Some((path, None)),
            Err(_) => None,
        })
        .collect()
}

fn versions_dir(root: &Path, key: &RepoKey, repo_path: &str) -> PathBuf {
    let name: String = hash(repo_path.as_bytes()).chars().take(16).collect();
    repo_dir(root, key).join("versions").join(name)
}

/// The saves of a drawing since its last send, oldest first.
pub fn snapshots(root: &Path, key: &RepoKey, repo_path: &str) -> Result<Vec<SaveMeta>, String> {
    let entries = match std::fs::read_dir(versions_dir(root, key, repo_path)) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.to_string()),
    };
    let mut saves = Vec::new();
    for entry in entries {
        let path = entry.map_err(|error| error.to_string())?.path();
        let id = path.file_stem().and_then(|stem| stem.to_str());
        if let Some(id) = id.filter(|_| path.extension().is_some_and(|ext| ext == "rsk")) {
            saves.push(SaveMeta {
                id: id.to_owned(),
                path,
            });
        }
    }
    saves.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(saves)
}

/// The id a save made now would get.
pub fn now_id() -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH);
    format!("{:013}", now.map_or(0, |since| since.as_millis()))
}

/// Keep a save just written to the working copy as a version, unless it is
/// what GitHub already has (`base_hash`) or what the newest version holds.
/// At most 50 are kept per drawing. Blocking.
pub fn save_version(
    root: &Path,
    key: &RepoKey,
    repo_path: &str,
    bytes: &[u8],
    base_hash: Option<&str>,
) -> Result<Option<SaveMeta>, String> {
    // One at a time: two saves that read the list together would take the
    // same id, and one would replace the other.
    static SAVING: Mutex<()> = Mutex::new(());
    let _saving = SAVING.lock().unwrap_or_else(PoisonError::into_inner);
    let saves = snapshots(root, key, repo_path)?;
    let newest = saves.last().and_then(|save| std::fs::read(&save.path).ok());
    if base_hash == Some(hash(bytes).as_str()) || newest.is_some_and(|newest| newest == bytes) {
        return Ok(None);
    }
    let dir = versions_dir(root, key, repo_path);
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    // Two saves in one millisecond still get rising ids.
    let after = saves.last().and_then(|save| save.id.parse::<u64>().ok());
    let id = match after {
        Some(last) if format!("{last:013}") >= now_id() => format!("{:013}", last + 1),
        _ => now_id(),
    };
    let path = dir.join(format!("{id}.rsk"));
    storage::write_atomic(&path, bytes)?;
    for old in saves.iter().rev().skip(KEPT - 1) {
        std::fs::remove_file(&old.path).map_err(|error| error.to_string())?;
    }
    Ok(Some(SaveMeta { id, path }))
}

/// [`save_version`] off the async executor.
pub async fn snapshot(
    root: &Path,
    key: &RepoKey,
    repo_path: &str,
    bytes: Vec<u8>,
    base_hash: Option<String>,
) -> Result<Option<SaveMeta>, String> {
    let (root, key, repo_path) = (root.to_owned(), key.clone(), repo_path.to_owned());
    let save = move || save_version(&root, &key, &repo_path, &bytes, base_hash.as_deref());
    crate::task::spawn_blocking(save)
        .await
        .map_err(|error| error.to_string())?
}

/// After a send: drop the saves made up to `sent_at` (a [`now_id`] taken when
/// the send began). Later saves stay.
pub fn prune_sent(
    root: &Path,
    key: &RepoKey,
    repo_path: &str,
    sent_at: &str,
) -> Result<(), String> {
    for save in snapshots(root, key, repo_path)? {
        if save.id.as_str() <= sent_at {
            std::fs::remove_file(&save.path).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

/// Write a drawing just downloaded as the working copy, and take it as the
/// base. The caller keeps an unsent working copy instead of calling this.
pub fn checkout(
    root: &Path,
    key: &RepoKey,
    state: &mut RepoState,
    repo_path: &str,
    download: &super::api::Download,
) -> Result<PathBuf, String> {
    if !valid_repo_path(repo_path) || state.collides(repo_path) {
        return Err("Can't open this name here".into());
    }
    let path = working_path(root, key, repo_path);
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
    }
    storage::write_saved(&path, &download.bytes)?;
    state.files.insert(
        repo_path.to_owned(),
        FileState {
            base_commit: download.commit.clone(),
            base_blob: Some(download.blob.clone()),
            base_hash: hash(&download.bytes),
            conflict: None,
        },
    );
    Ok(path)
}

/// [`checkout`] on a copy of `state`, for running off the UI thread, and
/// only over a file that has nothing unsent: one that is missing, or holds
/// what was last synced. Returns where it was written and its new base.
/// Blocking.
pub fn checkout_synced(
    root: &Path,
    key: &RepoKey,
    mut state: RepoState,
    repo_path: &str,
    download: &super::api::Download,
) -> Result<(PathBuf, FileState), String> {
    let synced = state
        .files
        .get(repo_path)
        .map(|file| file.base_hash.clone());
    match std::fs::read(working_path(root, key, repo_path)) {
        Ok(bytes) if synced != Some(hash(&bytes)) => {
            return Err("It changed on this computer".into());
        }
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            return Err(error.to_string());
        }
        _ => {}
    }
    let path = checkout(root, key, &mut state, repo_path, download)?;
    Ok((path, state.files.remove(repo_path).unwrap_or_default()))
}
