//! Newer on GitHub: on window focus, with the access re-check and its
//! throttle, the open drawings are compared with GitHub. One someone changed
//! there waits for Update while it has no changes here; with changes here it
//! is the conflict Review, as a send would have found it.
use super::{Action, perform, versions};
use crate::app::{App, Message, document_tab::DocumentTab};
use iced::Task;
use reshiki::document::Document;
use reshiki::github::{
    RepoKey, api,
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
                let found = send::newer(&client, id, &files).await?;
                let base = |path: &str| {
                    let (_, base) = files.iter().zip(&bases).find(|((at, _), _)| at == path)?;
                    Some(base.clone())
                };
                Ok(found
                    .into_iter()
                    .filter_map(|(theirs, by)| {
                        Some(Found {
                            base: base(&theirs.repo_path)?,
                            theirs,
                            by,
                        })
                    })
                    .collect())
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
    /// sends, or after a drawing's base moved, is dropped.
    pub(super) fn github_newer_found(&mut self, key: RepoKey, found: Vec<Found>) -> Task<Message> {
        if self.github_busy(&key) {
            return Task::none();
        }
        self.github_refresh(&key);
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
        if marked {
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
        (self.github.unsent.get(key)).is_some_and(|unsent| unsent.contains(repo_path))
            || (self.strip()).any(|tab| tab.path.is_some() && tab.path == path && edited(tab))
    }

    /// GitHub's newer version of the drawing at `path`, while the drawing is
    /// still based on what GitHub was asked against and no send found it in
    /// conflict since, and whether it has changes here.
    fn github_found(
        &self,
        path: Option<&Path>,
        edited: impl Fn(&DocumentTab) -> bool,
    ) -> Option<(&Found, bool)> {
        let at = self.github.locate(path)?;
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
    /// undoable edit and becomes what it is based on.
    pub(super) fn github_update(&mut self) -> Task<Message> {
        let front = self.tab.path.clone();
        // A save made elsewhere since counts as a change here.
        if let Some((key, _)) = self.github.locate(front.as_deref()) {
            self.github_refresh(&key);
        }
        let found = self.github_found(front.as_deref(), |tab| self.edited(tab));
        let (Some(root), Some((key, repo_path)), Some((found, false))) = (
            self.github.root.clone(),
            self.github.locate(front.as_deref()),
            found.map(|(found, changed)| (found.clone(), changed)),
        ) else {
            return Task::none();
        };
        let Found { theirs, by, .. } = found;
        let (Some(blob), Some(bytes)) = (theirs.blob, theirs.theirs) else {
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
            commit: theirs.commit,
            blob,
            bytes,
        };
        let Some(state) = self.github_state(&key) else {
            return Task::none();
        };
        if let Err(error) = workcopy::checkout(&root, &key, state, &repo_path, &download) {
            self.status = format!("Couldn't update: {error}");
            self.error = true;
            return Task::none();
        }
        self.replace_with_stored(doc);
        self.tab.mark_saved(self.tab.doc.clone());
        self.github.newer.remove(&(key.clone(), repo_path));
        // `Updated · @alice: Add an ethyl group`
        self.status = match by {
            Some(by) => {
                let who = versions::who(by.login.as_deref(), &by.author, self.github.login());
                format!("Updated · {who}: {}", by.message)
            }
            None => "Updated".into(),
        };
        self.error = false;
        self.github_refresh(&key);
        // A failed send with nothing left to send is over.
        if (self.github.unsent.get(&key)).is_none_or(Vec::is_empty) {
            self.github.failed.remove(&key);
        }
        // History lists GitHub's new commit.
        if (self.github.versions.target.as_ref()).is_some_and(|(at, _)| *at == key) {
            self.github.versions.target = None;
        }
        Task::batch([self.github_persist(&key), self.github_follow()])
    }
}
