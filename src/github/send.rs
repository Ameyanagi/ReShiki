//! Send: one commit of every changed drawing in a repository, each with an
//! SVG preview beside it. A drawing that changed on GitHub since it was opened
//! is left out and reported as a conflict; nothing is ever overwritten.
use super::{
    Error, Repo, RepoKey,
    api::{self, Folders},
    client::{Client, FILE_LIMIT},
    workcopy::{self, ConflictMark, FileState, RepoState},
};
use crate::{document::Document, storage};
use std::{collections::HashMap, path::Path};

/// A drawing to send, as it is saved on this computer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outgoing {
    pub repo_path: String,
    pub bytes: Vec<u8>,
    pub svg: Vec<u8>,
    /// What GitHub had when the drawing was last opened or sent.
    pub base_blob: Option<String>,
    pub hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendOutcome {
    /// The repository as GitHub names it now.
    pub repo: Repo,
    /// The head of the branch after the send; the sent drawings are based on it.
    pub commit: String,
    /// `(repo path, blob, hash)` of every drawing GitHub now has as sent.
    pub sent: Vec<(String, String, String)>,
    pub conflicts: Vec<Conflict>,
}

/// A drawing that changed on GitHub since it was opened here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub repo_path: String,
    pub commit: String,
    pub blob: Option<String>,
    /// `None`: deleted on GitHub.
    pub theirs: Option<Vec<u8>>,
    pub mine: Vec<u8>,
    /// The drawings and previews beside it on GitHub, for naming a copy.
    pub names: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    KeepMine,
    KeepTheirs,
    KeepBoth,
}

/// `Update a.rsk, b.rsk`, or `Update a.rsk and 3 more` past three.
pub fn default_message(paths: &[String]) -> String {
    let names: Vec<&str> = paths
        .iter()
        .map(|path| path.rsplit('/').next().unwrap_or(path))
        .collect();
    match names.as_slice() {
        [first, rest @ ..] if rest.len() >= 3 => format!("Update {first} and {} more", rest.len()),
        _ => format!("Update {}", names.join(", ")),
    }
}

/// The preview sits beside the drawing: `a.rsk` and `a.svg`.
fn svg_path(repo_path: &str) -> String {
    format!(
        "{}.svg",
        repo_path.strip_suffix(".rsk").unwrap_or(repo_path)
    )
}

/// Every unsent drawing of the repository, read from its saved file, with a
/// fresh preview. Blocking: it reads and renders.
pub fn outgoing(root: &Path, key: &RepoKey, state: &RepoState) -> Result<Vec<Outgoing>, String> {
    let mut files = Vec::new();
    for repo_path in workcopy::unsent(root, key, state)? {
        let bytes = std::fs::read(workcopy::working_path(root, key, &repo_path))
            .map_err(|error| error.to_string())?;
        let doc = Document::from_native_file(&bytes)?;
        files.push(Outgoing {
            svg: crate::export::drawing(&doc, "svg")?,
            base_blob: state
                .files
                .get(&repo_path)
                .and_then(|file| file.base_blob.clone()),
            hash: workcopy::hash(&bytes),
            repo_path,
            bytes,
        });
    }
    Ok(files)
}

/// One commit on the repository's default branch, fast-forward only.
pub async fn send(
    client: &Client,
    repo_id: u64,
    message: &str,
    files: &[Outgoing],
) -> Result<SendOutcome, Error> {
    for file in files {
        if file.bytes.len().max(file.svg.len()) > FILE_LIMIT {
            let name = file.repo_path.rsplit('/').next().unwrap_or_default();
            return Err(Error::TooLarge(Some(name.into())));
        }
    }
    // By id: the repository may have been renamed, or its default branch changed.
    let repo = api::repo(client, repo_id).await?;
    let (key, branch) = (&repo.key, repo.default_branch.as_str());
    let mut head = match api::head(client, key, branch).await {
        // An empty repository has no branch to build on.
        Err(Error::Conflict) => {
            api::init_readme(client, key).await?;
            api::head(client, key, branch).await
        }
        Err(Error::NotFound) => Err(Error::Rejected("Branch not found".into())),
        head => head,
    }?;
    let mut uploaded: HashMap<&str, [String; 2]> = HashMap::new();
    // The branch can move while we build on it: start again, a few times.
    for _ in 0..3 {
        let mut folders = Folders::new(client, key, head.tree.clone());
        let (mut clean, mut sent, mut conflicts) = (Vec::new(), Vec::new(), Vec::new());
        for file in files {
            let remote = folders.blob(&file.repo_path).await?;
            if remote == file.base_blob {
                clean.push(file);
                continue;
            }
            let theirs = match &remote {
                Some(sha) => Some(api::blob(client, key, sha).await?),
                None => None,
            };
            match remote {
                // GitHub already has these bytes (a send whose reply was lost).
                Some(sha) if theirs.as_ref() == Some(&file.bytes) => {
                    sent.push((file.repo_path.clone(), sha, file.hash.clone()));
                }
                blob => {
                    let folder = file
                        .repo_path
                        .rsplit_once('/')
                        .map_or("", |(folder, _)| folder);
                    let beside = folders.list(folder).await?.unwrap_or_default();
                    conflicts.push(Conflict {
                        repo_path: file.repo_path.clone(),
                        commit: head.commit.clone(),
                        blob,
                        theirs,
                        mine: file.bytes.clone(),
                        names: beside.iter().map(|entry| entry.name.clone()).collect(),
                    });
                }
            }
        }
        let outcome = |commit: String, sent| SendOutcome {
            repo: repo.clone(),
            commit,
            sent,
            conflicts,
        };
        if clean.is_empty() {
            return Ok(outcome(head.commit, sent));
        }
        let mut tree = Vec::new();
        for file in clean {
            let [drawing, preview] = match uploaded.get(file.repo_path.as_str()) {
                Some(blobs) => blobs.clone(),
                None => {
                    let blobs = [
                        api::create_blob(client, key, &file.bytes).await?,
                        api::create_blob(client, key, &file.svg).await?,
                    ];
                    uploaded.insert(file.repo_path.as_str(), blobs.clone());
                    blobs
                }
            };
            tree.push((file.repo_path.clone(), drawing.clone()));
            tree.push((svg_path(&file.repo_path), preview));
            sent.push((file.repo_path.clone(), drawing, file.hash.clone()));
        }
        let tree = api::create_tree(client, key, &head.tree, &tree).await?;
        let commit = api::create_commit(client, key, message, &tree, &head.commit).await?;
        match api::update_ref(client, key, branch, &commit).await {
            Ok(()) => return Ok(outcome(commit, sent)),
            Err(Error::Conflict) => {
                let now = api::head(client, key, branch).await?;
                if now == head {
                    return Err(Error::Conflict);
                }
                head = now;
            }
            Err(error) => return Err(error),
        }
    }
    Err(Error::Conflict)
}

/// Take a send's result into the state: new bases for what was sent, a mark
/// on every conflict, and the repository's current names.
pub fn apply(state: &mut RepoState, outcome: &SendOutcome) {
    state.owner.clone_from(&outcome.repo.key.owner);
    state.repo.clone_from(&outcome.repo.key.repo);
    state.branch.clone_from(&outcome.repo.default_branch);
    state.private = outcome.repo.private;
    for (path, blob, hash) in &outcome.sent {
        state.files.insert(
            path.clone(),
            FileState {
                base_commit: outcome.commit.clone(),
                base_blob: Some(blob.clone()),
                base_hash: hash.clone(),
                conflict: None,
            },
        );
    }
    for conflict in &outcome.conflicts {
        if let Some(file) = state.files.get_mut(&conflict.repo_path) {
            file.conflict = Some(ConflictMark {
                commit: conflict.commit.clone(),
                blob: conflict.blob.clone(),
            });
        }
    }
}

/// [`apply`], then write the state, and only once that is written drop the
/// sent drawings' saves made before `sent_at` (a `workcopy::now_id` taken
/// when the send began). `key` is the repository's folder here.
pub async fn record(
    root: &Path,
    key: &RepoKey,
    state: &mut RepoState,
    seq: u64,
    outcome: &SendOutcome,
    sent_at: &str,
) -> Result<(), String> {
    apply(state, outcome);
    workcopy::persist(root, key, state.clone(), seq).await?;
    for (path, ..) in &outcome.sent {
        workcopy::prune_sent(root, key, path, sent_at)?;
    }
    Ok(())
}

/// Settle a conflict on this computer; the caller then sends again. "Mine" is
/// the drawing as saved now, not as it was when the conflict was found.
/// Returns the path of the copy that `KeepBoth` made. Blocking.
pub fn resolve(
    root: &Path,
    key: &RepoKey,
    state: &mut RepoState,
    conflict: &Conflict,
    choice: Choice,
) -> Result<Option<String>, String> {
    let path = workcopy::working_path(root, key, &conflict.repo_path);
    let theirs_hash = conflict
        .theirs
        .as_deref()
        .map(workcopy::hash)
        .unwrap_or_default();
    if choice == Choice::KeepMine {
        // The next send builds on GitHub's version, so it goes over it.
        let based = FileState {
            base_commit: conflict.commit.clone(),
            base_blob: conflict.blob.clone(),
            base_hash: theirs_hash,
            conflict: None,
        };
        state.files.insert(conflict.repo_path.clone(), based);
        return Ok(None);
    }
    let mine = std::fs::read(&path).map_err(|error| error.to_string())?;
    let copy = if choice == Choice::KeepBoth {
        let stem = conflict
            .repo_path
            .strip_suffix(".rsk")
            .unwrap_or(&conflict.repo_path);
        let folder = stem
            .rsplit_once('/')
            .map_or(0, |(folder, _)| folder.len() + 1);
        let free = |copy: &String| {
            [copy.clone(), svg_path(copy)].iter().all(|name| {
                let (_, file) = name.split_at(folder);
                !conflict.names.iter().any(|taken| taken == file)
                    && !state.files.contains_key(name)
                    && !workcopy::working_path(root, key, name).exists()
            })
        };
        let copy = (2..).map(|n| format!("{stem} ({n}).rsk")).find(free);
        let copy = copy.ok_or("No free name")?;
        storage::write_saved(&workcopy::working_path(root, key, &copy), &mine)?;
        state.files.insert(copy.clone(), FileState::default());
        Some(copy)
    } else {
        // What was here stays in Versions.
        workcopy::save_version(root, key, &conflict.repo_path, &mine, None)?;
        None
    };
    match &conflict.theirs {
        Some(theirs) => {
            storage::write_saved(&path, theirs)?;
            let based = FileState {
                base_commit: conflict.commit.clone(),
                base_blob: conflict.blob.clone(),
                base_hash: theirs_hash,
                conflict: None,
            };
            state.files.insert(conflict.repo_path.clone(), based);
        }
        None => {
            std::fs::remove_file(&path).map_err(|error| error.to_string())?;
            state.files.remove(&conflict.repo_path);
        }
    }
    Ok(copy)
}
