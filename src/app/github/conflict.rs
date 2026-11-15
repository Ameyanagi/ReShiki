//! A drawing changed on GitHub since it was opened here: GitHub's version and
//! yours side by side, then Keep GitHub's, Keep mine or Keep both. Nothing is
//! the default, so a bare Enter decides nothing; Escape decides later and the
//! status bar keeps Review. Nothing is overwritten without a choice.
use super::{Action, Dialog, compare::Compare};
use crate::app::{App, Message, workspace::muted_text};
use iced::widget::{Space, column, row, text};
use iced::{Alignment, Element, Length, Task};
use reshiki::document::Document;
use reshiki::github::{
    RepoKey,
    send::{self, Choice},
    workcopy,
};
use std::path::PathBuf;

#[derive(Debug)]
pub(in crate::app) struct Review {
    pub(super) key: RepoKey,
    /// The drawing here.
    path: PathBuf,
    pub(super) name: String,
    /// The name Keep both gives yours.
    pub(super) copy: String,
    /// Settled in this dialog so far, and in all, for `2 of 3`.
    pub(super) done: usize,
    pub(super) total: usize,
    pub(super) compare: Compare,
}

pub(super) fn file_name(repo_path: &str) -> String {
    repo_path.rsplit('/').next().unwrap_or(repo_path).to_owned()
}

impl App {
    /// The first conflict of `key` that this run knows. With none known
    /// (ReShiki restarted since), the drawings in conflict are sent again:
    /// GitHub's versions come back as conflicts. A drawing saved back to
    /// what it was based on is in conflict no more.
    pub(super) fn github_review(&mut self, key: RepoKey, done: usize) -> Task<Message> {
        if let Some(review) = self.conflict_review(&key, done) {
            self.github.dialog = Some(Dialog::Conflict(Box::new(review)));
            return Task::none();
        }
        self.github_refresh(&key);
        let unsent = self.github.unsent.get(&key).cloned().unwrap_or_default();
        let Some(state) = self.github_state(&key) else {
            return Task::none();
        };
        let (mut marked, mut cleared) = (Vec::new(), false);
        for (path, file) in &mut state.files {
            if file.conflict.is_none() {
                continue;
            }
            if unsent.contains(path) {
                marked.push(path.clone());
            } else {
                file.conflict = None;
                cleared = true;
            }
        }
        let persist = if cleared {
            self.github_persist(&key)
        } else {
            Task::none()
        };
        if marked.is_empty() {
            return persist;
        }
        let message = send::default_message(&marked);
        Task::batch([persist, self.github_send(key, message, Some(marked))])
    }

    fn conflict_review(&self, key: &RepoKey, done: usize) -> Option<Review> {
        let root = self.github.root.as_deref()?;
        let conflicts = self.github.conflicts.get(key)?;
        let conflict = conflicts.first()?;
        let path = workcopy::working_path(root, key, &conflict.repo_path);
        // Yours: the drawing as it shows, unsaved edits included.
        let mine = match self.strip().find(|tab| tab.path.as_ref() == Some(&path)) {
            Some(tab) => tab.doc.clone(),
            None => {
                let saved = std::fs::read(&path).unwrap_or_else(|_| conflict.mine.clone());
                Document::from_native_file(&saved).ok()?
            }
        };
        let theirs = match &conflict.theirs {
            Some(bytes) => Document::from_native_file(bytes),
            None => Err("Deleted on GitHub".into()),
        };
        let state = self.github.states.get(key)?;
        Some(Review {
            key: key.clone(),
            name: file_name(&conflict.repo_path),
            copy: file_name(&send::copy_path(root, key, state, conflict)),
            path,
            done,
            total: done + conflicts.len(),
            compare: Compare::new(theirs, mine),
        })
    }

    /// Settles the conflict shown. "Mine" is the drawing as it shows, so a
    /// tab with unsaved edits is saved first, as Cmd+S does.
    pub(super) fn github_resolve(&mut self, choice: Choice) -> Task<Message> {
        let Some(Dialog::Conflict(review)) = &self.github.dialog else {
            return Task::none();
        };
        if self.github.resolving.is_some() {
            return Task::none();
        }
        let (key, path) = (review.key.clone(), review.path.clone());
        let tab = self
            .strip()
            .position(|tab| tab.path.as_ref() == Some(&path));
        if let Some(index) = tab {
            self.select_tab(index);
            // A text draft is part of what the drawing shows.
            if !self.finish_inline(true) {
                return Task::none();
            }
            // A save still writing would land over what is kept: wait for it.
            let id = self.tab.id;
            let writing = self.file_io.saving && self.file_io.saving_tab == Some(id);
            if self.dirty() || writing {
                // A tab closing goes first and is the one saved.
                let save = self.save_drawing(false);
                let save = crate::app::tagged(save, self.tab.id);
                // Unless this drawing's save is under way, the choice can be made again.
                if self.file_io.saving && self.file_io.saving_tab == Some(id) {
                    self.github.resolving = Some((key, choice));
                }
                return save;
            }
        }
        self.github_settle(key, choice)
    }

    /// After a save of the drawing [`Self::github_resolve`] waited for, with
    /// its tab in front: settles once nothing is left unsaved.
    pub(super) fn github_resolve_saved(&mut self, saved: &std::path::Path) -> Task<Message> {
        let waited =
            matches!(&self.github.dialog, Some(Dialog::Conflict(review)) if review.path == saved);
        let Some((key, choice)) = self.github.resolving.take_if(|_| waited) else {
            return Task::none();
        };
        if self.dirty() {
            return Task::none();
        }
        Task::done(Message::GitHub(Action::Settle(key, choice)))
    }

    /// Writes the choice here, shows it in the drawing's tab, then shows the
    /// next conflict or, after the last, sends.
    pub(super) fn github_settle(&mut self, key: RepoKey, choice: Choice) -> Task<Message> {
        let Some(root) = self.github.root.clone() else {
            return Task::none();
        };
        let Some(conflict) = self
            .github
            .conflicts
            .get(&key)
            .and_then(|c| c.first())
            .cloned()
        else {
            return Task::none();
        };
        let Some(state) = self.github_state(&key) else {
            return Task::none();
        };
        let copy = match send::resolve(&root, &key, state, &conflict, choice) {
            Ok(copy) => copy,
            Err(error) => {
                self.status = format!("Couldn't keep it: {error}");
                self.error = true;
                return Task::none();
            }
        };
        if let Some(left) = self.github.conflicts.get_mut(&key) {
            left.remove(0);
        }
        let path = workcopy::working_path(&root, &key, &conflict.repo_path);
        let tab = self
            .strip()
            .position(|tab| tab.path.as_ref() == Some(&path));
        if let Some(index) = tab {
            self.select_tab(index);
            self.github_show_choice(
                choice,
                &conflict,
                copy.as_deref()
                    .map(|copy| workcopy::working_path(&root, &key, copy)),
            );
        }
        self.status = match (choice, &copy) {
            (Choice::KeepMine, _) => "Kept yours".into(),
            (Choice::KeepBoth, Some(copy)) => format!("Kept both · yours is “{}”", file_name(copy)),
            _ => "Kept GitHub’s".into(),
        };
        self.error = false;
        self.github_refresh(&key);
        let persist = self.github_persist(&key);
        let done = match &self.github.dialog {
            Some(Dialog::Conflict(review)) => review.done + 1,
            _ => 0,
        };
        if self
            .github
            .conflicts
            .get(&key)
            .is_some_and(|left| !left.is_empty())
        {
            return Task::batch([persist, self.github_review(key, done)]);
        }
        let close = self.github_close_dialog();
        let unsent = self.github.unsent.get(&key).cloned().unwrap_or_default();
        let send = if unsent.is_empty() {
            Task::none()
        } else {
            let send = self.github_send(key.clone(), send::default_message(&unsent), None);
            if self.github.sending.contains(&key) {
                self.github.kept.insert(key, self.status.clone());
            }
            send
        };
        Task::batch([persist, close, send])
    }

    /// The tab in front was the drawing in conflict: it shows what was kept.
    fn github_show_choice(
        &mut self,
        choice: Choice,
        conflict: &send::Conflict,
        copy: Option<PathBuf>,
    ) {
        let theirs = conflict
            .theirs
            .as_deref()
            .and_then(|bytes| Document::from_native_file(bytes).ok());
        match (choice, copy, theirs) {
            (Choice::KeepMine, ..) => {}
            // Yours, saved, under its new name.
            (Choice::KeepBoth, Some(copy), _) => self.tab.path = Some(copy),
            // GitHub's, as one undoable edit, and saved.
            (_, _, Some(theirs)) => match self.try_replace_document(theirs, Vec::new()) {
                Ok(()) => self.tab.mark_saved(self.tab.doc.clone()),
                Err(error) => {
                    self.status = error;
                    self.error = true;
                }
            },
            // Nothing here to show: yours stays open as an unsaved drawing.
            (_, _, None) => {
                self.tab.untitled_name = Some(self.tab.name());
                self.tab.path = None;
                self.tab.mark_saved(Document::default());
            }
        }
    }
}

pub(super) fn view(review: &Review) -> Element<'_, Message> {
    let mut title = row![
        text(format!("“{}” changed on GitHub", review.name)).size(18),
        Space::new().width(Length::Fill),
    ]
    .align_y(Alignment::Center);
    if review.total > 1 {
        let at = format!("{} of {}", review.done + 1, review.total);
        title = title.push(text(at).size(13).style(muted_text));
    }
    let choice = |id: &'static str, label: &'static str, choice: Choice| {
        reshiki::accessibility::button(id, label, text(label).size(13))
            .padding([7, 12])
            .style(crate::appearance::secondary)
            .on_press(Message::GitHub(Action::Resolve(choice)))
    };
    column![
        title,
        review.compare.view(["GitHub’s", "Yours"]),
        row![
            text(format!("Keep both: yours becomes “{}”", review.copy))
                .size(12)
                .style(muted_text),
            Space::new().width(Length::Fill),
            choice("github-keep-theirs", "Keep GitHub’s", Choice::KeepTheirs),
            choice("github-keep-mine", "Keep mine", Choice::KeepMine),
            choice("github-keep-both", "Keep both", Choice::KeepBoth),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    ]
    .spacing(12)
    .into()
}
