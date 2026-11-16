//! Newer on GitHub: on window focus, with the access re-check and its
//! throttle, the open drawings are compared with GitHub. One someone changed
//! there waits for Update while it has no changes here; one with changes
//! here becomes the conflict Review, as a send would have found it.
use super::{Action, Found, perform, versions};
use crate::app::{App, Message};
use iced::Task;
use reshiki::document::Document;
use reshiki::github::{
    RepoKey, api,
    client::Client,
    send,
    workcopy::{self, ConflictMark},
};
use std::path::Path;

impl App {
    /// Asks GitHub whether someone changed an open drawing of `key` since
    /// it was opened or sent here.
    pub(super) fn github_check_newer(&self, client: Client, key: RepoKey) -> Task<Message> {
        let Some(state) = self.github.states.get(&key) else {
            return Task::none();
        };
        let files: Vec<(String, String)> = self
            .strip()
            .filter_map(|tab| {
                let (other, repo_path) = self.github.locate(tab.path.as_deref())?;
                let file = (state.files.get(&repo_path))
                    .filter(|file| other == key && file.conflict.is_none())?;
                Some((repo_path, file.base_blob.clone()?))
            })
            .collect();
        if files.is_empty() {
            return Task::none();
        }
        let (named, branch) = (state.key(), state.branch.clone());
        let generation = self.github.generation;
        perform(
            async move { send::newer(&client, &named, &branch, &files).await },
            move |result| Action::Newer(generation, key, result),
        )
    }

    /// What a check of `key` found.
    pub(super) fn github_newer_found(&mut self, key: RepoKey, found: Vec<Found>) -> Task<Message> {
        let Some(root) = self.github.root.clone() else {
            return Task::none();
        };
        self.github_refresh(&key);
        self.github.newer.retain(|(other, _), _| *other != key);
        let mut marked = false;
        for (mut conflict, by) in found {
            let path = workcopy::working_path(&root, &key, &conflict.repo_path);
            let changed = (self.github.unsent.get(&key))
                .is_some_and(|unsent| unsent.contains(&conflict.repo_path))
                || (self.strip()).any(|tab| tab.path.as_ref() == Some(&path) && self.edited(tab));
            let Some(file) = (self.github.states.get_mut(&key))
                .and_then(|state| state.files.get_mut(&conflict.repo_path))
            else {
                continue;
            };
            // Taken, or found by a send, while GitHub was asked.
            if file.base_blob == conflict.blob || file.conflict.is_some() {
                continue;
            }
            if !changed {
                let at = (key.clone(), conflict.repo_path.clone());
                self.github.newer.insert(at, (conflict, by));
                continue;
            }
            file.conflict = Some(ConflictMark {
                commit: conflict.commit.clone(),
                blob: conflict.blob.clone(),
            });
            conflict.mine = std::fs::read(&path).unwrap_or_default();
            self.github
                .conflicts
                .entry(key.clone())
                .or_default()
                .push(conflict);
            marked = true;
        }
        if marked {
            self.github_persist(&key)
        } else {
            Task::none()
        }
    }

    /// GitHub's newer version of the drawing at `path`, while it has no
    /// changes here.
    pub(super) fn github_newer(&self, path: Option<&Path>) -> Option<&Found> {
        let (key, repo_path) = self.github.locate(path)?;
        let unsent =
            (self.github.unsent.get(&key)).is_some_and(|unsent| unsent.contains(&repo_path));
        let edited =
            (self.strip()).any(|tab| tab.path.as_deref() == path && self.shown_edited(tab));
        let found = self.github.newer.get(&(key, repo_path))?;
        (!unsent && !edited).then_some(found)
    }

    /// Update: GitHub's newer version replaces the drawing in front as one
    /// undoable edit and becomes what it is based on.
    pub(super) fn github_update(&mut self) -> Task<Message> {
        let front = self.tab.path.clone();
        let (Some(root), Some((key, repo_path)), Some((found, by))) = (
            self.github.root.clone(),
            self.github.locate(front.as_deref()),
            self.github_newer(front.as_deref()).cloned(),
        ) else {
            return Task::none();
        };
        let (Some(blob), Some(bytes)) = (found.blob, found.theirs) else {
            return Task::none();
        };
        let theirs = match Document::from_native_file(&bytes) {
            Ok(theirs) => theirs,
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
            commit: found.commit,
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
        self.replace_with_stored(theirs);
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
        // History lists GitHub's new commit.
        if (self.github.versions.target.as_ref()).is_some_and(|(at, _)| *at == key) {
            self.github.versions.target = None;
        }
        Task::batch([self.github_persist(&key), self.github_follow()])
    }
}
