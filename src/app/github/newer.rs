//! Newer on GitHub: on window focus, with the access re-check and its
//! throttle, the open drawings are compared with GitHub. One someone changed
//! there waits for Update while it has no changes here; with changes here it
//! is the conflict Review, as a send would have found it.
use super::{Action, perform, versions};
use crate::app::{App, Message, document_tab::DocumentTab};
use iced::Task;
use reshiki::document::Document;
use reshiki::github::{
    Repo, RepoKey, api,
    client::Client,
    send,
    workcopy::{self, ConflictMark},
};
use std::path::Path;

/// A drawing someone changed on GitHub.
#[derive(Debug, Clone)]
pub struct Found {
    /// The commit the drawing here was based on when GitHub was asked: the
    /// rest holds only while it still is. Every send and every download
    /// moves it, even one back to the same bytes.
    pub(super) base: String,
    /// GitHub's version, as a send would find it in the way.
    pub(super) theirs: send::Conflict,
    /// The commit that last changed it.
    pub(super) by: Option<api::RemoteVersion>,
}

impl App {
    /// Asks GitHub whether someone changed an open drawing of `key` since
    /// it was opened or sent here. Not while it sends: the send finds out.
    pub(super) fn github_check_newer(&self, client: Client, key: RepoKey) -> Task<Message> {
        let Some(state) = self.github.states.get(&key) else {
            return Task::none();
        };
        let (files, bases): (Vec<(String, String)>, Vec<String>) = self
            .strip()
            .filter_map(|tab| {
                let (other, repo_path) = self.github.locate(tab.path.as_deref())?;
                let file = (state.files.get(&repo_path))
                    .filter(|file| other == key && file.conflict.is_none())?;
                Some((
                    (repo_path, file.base_blob.clone()?),
                    file.base_commit.clone(),
                ))
            })
            .unzip();
        if files.is_empty() || self.github_busy(&key) {
            return Task::none();
        }
        let (id, generation) = (state.repo_id, self.github.generation);
        perform(
            async move {
                let (repo, found) = send::newer(&client, id, &files).await?;
                let base = |path: &str| {
                    let (_, base) = files.iter().zip(&bases).find(|((at, _), _)| at == path)?;
                    Some(base.clone())
                };
                let found: Vec<Found> = found
                    .into_iter()
                    .filter_map(|(theirs, by)| {
                        Some(Found {
                            base: base(&theirs.repo_path)?,
                            theirs,
                            by,
                        })
                    })
                    .collect();
                Ok((repo, found))
            },
            move |result| Action::Newer(generation, key, result),
        )
    }

    /// Whether a Send of `key` is saving its drawings or sending them.
    fn github_busy(&self, key: &RepoKey) -> bool {
        self.github.sending.contains(key)
            || (self.github.after_save.as_ref()).is_some_and(|(at, ..)| at == key)
    }

    /// What a check of `key` found. A reply that lands while the repository
    /// sends, after it turned read only, or after a drawing's base moved, is
    /// dropped.
    pub(super) fn github_newer_found(
        &mut self,
        key: RepoKey,
        repo: Repo,
        found: Vec<Found>,
    ) -> Task<Message> {
        if self.github_busy(&key) || self.github.read_only(&key) {
            return Task::none();
        }
        // GitHub's names and default branch now, for History, as a send keeps them.
        let mut renamed = false;
        if let Some(state) = self.github.states.get_mut(&key)
            && state.repo_id == repo.id
            && (state.key() != repo.key || state.branch != repo.default_branch)
        {
            state.owner.clone_from(&repo.key.owner);
            state.repo.clone_from(&repo.key.repo);
            state.branch.clone_from(&repo.default_branch);
            renamed = true;
        }
        self.github.newer.retain(|(other, _), _| *other != key);
        let mut marked = false;
        for found in found {
            let file = (self.github.states.get(&key))
                .and_then(|state| state.files.get(&found.theirs.repo_path));
            if file.is_none_or(|file| file.base_commit != found.base || file.conflict.is_some()) {
                continue;
            }
            let at = (key.clone(), found.theirs.repo_path.clone());
            self.github.newer.insert(at.clone(), found);
            if self.github_changed(&at, |tab| self.edited(tab)) {
                marked |= self.github_mark_conflict(&at);
            }
        }
        if marked || renamed {
            self.github_persist(&key)
        } else {
            Task::none()
        }
    }

    /// Whether the drawing at `at` has changes here: unsent, or edited in
    /// its tab (`edited` tells which tabs are).
    fn github_changed(
        &self,
        at: &(RepoKey, String),
        edited: impl Fn(&DocumentTab) -> bool,
    ) -> bool {
        let (key, repo_path) = at;
        let path =
            (self.github.root.as_deref()).map(|root| workcopy::working_path(root, key, repo_path));
        self.github.unsent(key).contains(repo_path)
            || (self.strip()).any(|tab| tab.path.is_some() && tab.path == path && edited(tab))
    }

    /// GitHub's newer version of the drawing at `path`, while the drawing is
    /// still based on what GitHub was asked against, no send found it in
    /// conflict since and the user may still send to it, and whether it has
    /// changes here.
    fn github_found(
        &self,
        path: Option<&Path>,
        edited: impl Fn(&DocumentTab) -> bool,
    ) -> Option<(&Found, bool)> {
        // Not while the repository sends: Update would change what it sends.
        let at = (self.github.locate(path))
            .filter(|(key, _)| !self.github.read_only(key) && !self.github_busy(key))?;
        let found = self.github.newer.get(&at)?;
        let file = (self.github.states.get(&at.0)).and_then(|state| state.files.get(&at.1))?;
        (file.base_commit == found.base && file.conflict.is_none())
            .then(|| (found, self.github_changed(&at, edited)))
    }

    /// For the window: GitHub's newer version of the drawing at `path`,
    /// while it has no changes here.
    pub(super) fn github_newer(&self, path: Option<&Path>) -> Option<&Found> {
        let (found, changed) = self.github_found(path, |tab| self.shown_edited(tab))?;
        (!changed).then_some(found)
    }

    /// For the window: the drawing at `path` changed on GitHub and here too.
    pub(super) fn github_newer_conflict(&self, path: Option<&Path>) -> bool {
        self.github_found(path, |tab| self.shown_edited(tab))
            .is_some_and(|(_, changed)| changed)
    }

    /// Before Review: the drawing in front, changed on GitHub since it was
    /// checked and here too, becomes a conflict, as a send would find it.
    pub(super) fn github_promote(&mut self) -> Task<Message> {
        let front = self.tab.path.clone();
        let Some((key, repo_path)) = self.github.locate(front.as_deref()) else {
            return Task::none();
        };
        let changed = self.github_found(front.as_deref(), |tab| self.edited(tab));
        if changed.is_some_and(|(_, changed)| changed)
            && self.github_mark_conflict(&(key.clone(), repo_path))
        {
            return self.github_persist(&key);
        }
        Task::none()
    }

    /// Marks the known newer version at `at` as a conflict to review.
    fn github_mark_conflict(&mut self, at: &(RepoKey, String)) -> bool {
        let (Some(root), Some(found)) = (self.github.root.clone(), self.github.newer.remove(at))
        else {
            return false;
        };
        let (key, repo_path) = at;
        let Some(file) =
            (self.github.states.get_mut(key)).and_then(|state| state.files.get_mut(repo_path))
        else {
            return false;
        };
        let mut theirs = found.theirs;
        file.conflict = Some(ConflictMark {
            commit: theirs.commit.clone(),
            blob: theirs.blob.clone(),
        });
        theirs.mine =
            std::fs::read(workcopy::working_path(&root, key, repo_path)).unwrap_or_default();
        self.github
            .conflicts
            .entry(key.clone())
            .or_default()
            .push(theirs);
        true
    }

    /// Update: GitHub's newer version replaces the drawing in front as one
    /// undoable edit and becomes what it is based on, once it is written
    /// over the working copy off the UI thread. That write is refused if
    /// the file changed here meanwhile, as in another app.
    pub(super) fn github_update(&mut self) -> Task<Message> {
        let front = self.tab.path.clone();
        let found = self.github_found(front.as_deref(), |tab| self.edited(tab));
        let (Some(root), Some((key, repo_path)), Some((found, false))) = (
            self.github.root.clone(),
            self.github.locate(front.as_deref()),
            found.map(|(found, changed)| (found.clone(), changed)),
        ) else {
            return Task::none();
        };
        let (Some(blob), Some(bytes)) = (found.theirs.blob.clone(), found.theirs.theirs.clone())
        else {
            return Task::none();
        };
        let doc = match Document::from_native_file(&bytes) {
            Ok(doc) => doc,
            Err(error) => {
                self.status = format!("Couldn't open GitHub’s version: {error}");
                self.error = true;
                return Task::none();
            }
        };
        if !self.replaceable() {
            return Task::none();
        }
        let download = api::Download {
            commit: found.theirs.commit.clone(),
            blob,
            bytes,
        };
        let Some(state) = self.github_state(&key).cloned() else {
            return Task::none();
        };
        let revision = self.tab.revision;
        // Taken while it is written, so a second Update does nothing.
        self.github.newer.remove(&(key.clone(), repo_path.clone()));
        let write = {
            let (key, repo_path) = (key.clone(), repo_path.clone());
            move || workcopy::checkout_synced(&root, &key, state, &repo_path, &download)
        };
        self.github_write(
            move || write().map(|(_, file)| file),
            move |result| {
                let updating = Box::new(Updating {
                    doc,
                    found,
                    revision,
                });
                Action::Updated(key, repo_path, updating, result)
            },
        )
    }

    /// Update's write landed: the drawing's tab shows GitHub's version.
    pub(super) fn github_updated(
        &mut self,
        key: RepoKey,
        repo_path: String,
        updating: Updating,
        written: Result<workcopy::FileState, String>,
    ) -> Task<Message> {
        let file = match written {
            Ok(file) => file,
            // Changed here meanwhile, as in another app: that change is read,
            // and GitHub's version waits, now for Review.
            Err(error) => {
                self.status = format!("Couldn't update: {error}");
                self.error = true;
                let at = (key.clone(), repo_path.clone());
                self.github.newer.insert(at, updating.found);
                let read = self.github_scan(&key, Some(repo_path));
                return Task::batch([read, self.github_wrote()]);
            }
        };
        let (Some(root), Some(state)) = (self.github.root.clone(), self.github_state(&key)) else {
            return self.github_wrote();
        };
        let hash = file.base_hash.clone();
        state.files.insert(repo_path.clone(), file);
        self.github.wrote(&key, &repo_path, Some(hash));
        let path = workcopy::working_path(&root, &key, &repo_path);
        let tab = (self.strip()).find(|tab| tab.path.as_ref() == Some(&path));
        if let Some(id) = tab.map(|tab| tab.id) {
            self.in_tab(id, |app| {
                let unchanged = app.tab.revision == updating.revision;
                app.replace_with_stored(updating.doc);
                // An edit made while it was written stays unsaved, one Undo away.
                if unchanged {
                    app.tab.mark_saved(app.tab.doc.clone());
                }
            });
        }
        // `Updated · @alice: Add an ethyl group`
        self.status = match updating.found.by {
            Some(by) => {
                let who = versions::who(by.login.as_deref(), &by.author, self.github.login());
                format!("Updated · {who}: {}", by.message)
            }
            None => "Updated".into(),
        };
        self.error = false;
        // A failed send with nothing left to send is over.
        if self.github.unsent(&key).is_empty() {
            self.github.failed.remove(&key);
        }
        // History lists GitHub's new commit.
        if (self.github.versions.target.as_ref()).is_some_and(|(at, _)| *at == key) {
            self.github.versions.target = None;
        }
        Task::batch([self.github_keep_state(&key, true), self.github_follow()])
    }
}

/// What Update shows once its write lands: GitHub's version, as found.
#[derive(Debug, Clone)]
pub struct Updating {
    doc: Document,
    found: Found,
    /// The tab's revision when Update was chosen.
    revision: u64,
}
