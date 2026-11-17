//! A drawing changed on GitHub since it was opened here: GitHub's version and
//! yours side by side, each with its Keep button under it, then Later or
//! Keep both. Nothing is the default, so a bare Enter decides nothing; Later
//! (Escape) decides later and the status bar keeps Review. Nothing is
//! overwritten without a choice.
use super::{
    Action, Dialog,
    compare::{Compare, Side, reading},
    file_name, perform,
};
use crate::app::{App, Message, workspace::muted_text};
use iced::widget::{Space, column, container, row, text, tooltip};
use iced::{Alignment, Element, Length, Task};
use reshiki::document::Document;
use reshiki::github::{
    RepoKey, api,
    send::{self, Choice},
    workcopy,
};
use std::path::PathBuf;

/// A conflict choice, written off the UI thread.
#[derive(Debug, Clone)]
pub struct Settled {
    /// The repository's state as the choice left it.
    state: workcopy::RepoState,
    conflict: send::Conflict,
    /// The file Keep both put yours in.
    copy: Option<String>,
    /// What the files the choice touched hold now.
    read: Vec<(String, Option<String>)>,
}

#[derive(Debug)]
pub(in crate::app) struct Review {
    pub(super) key: RepoKey,
    /// The drawing here.
    path: PathBuf,
    pub(super) repo_path: String,
    pub(super) name: String,
    /// The commit that last changed GitHub's version, once known.
    pub(super) theirs: Option<api::RemoteVersion>,
    /// When yours was saved, as `14:10`.
    saved: Option<String>,
    /// The name Keep both gives yours.
    pub(super) copy: String,
    /// Settled in this dialog so far, and in all, for `2 of 3`.
    pub(super) done: usize,
    pub(super) total: usize,
    pub(super) compare: Compare,
}

impl App {
    /// The first conflict of `key` that this run knows. With none known
    /// (ReShiki restarted since), the drawings in conflict are sent again:
    /// GitHub's versions come back as conflicts. A drawing saved back to
    /// what it was based on is in conflict no more.
    pub(super) fn github_review(&mut self, key: RepoKey, done: usize) -> Task<Message> {
        if let Some(review) = self.conflict_review(&key, done) {
            let meta = self.conflict_meta(&key, &review.repo_path);
            let start = review.compare.start();
            self.github.dialog = Some(Dialog::Conflict(Box::new(review)));
            return Task::batch([meta, start]);
        }
        let unsent = self.github.unsent(&key);
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
        let saved = std::fs::metadata(&path)
            .and_then(|meta| meta.modified())
            .ok()
            .map(|time| {
                chrono::DateTime::<chrono::Local>::from(time)
                    .format("%H:%M")
                    .to_string()
            });
        Some(Review {
            key: key.clone(),
            repo_path: conflict.repo_path.clone(),
            name: file_name(&conflict.repo_path).to_owned(),
            copy: file_name(&send::copy_path(root, key, state, conflict)).to_owned(),
            theirs: None,
            saved,
            path,
            done,
            total: done + conflicts.len(),
            compare: Compare::new(theirs, mine),
        })
    }

    /// Who last changed the drawing on GitHub, for the dialog's header.
    fn conflict_meta(&mut self, key: &RepoKey, repo_path: &str) -> Task<Message> {
        let (Some(client), Some(state), Some(commit)) = (
            self.github.client(),
            self.github.states.get(key),
            self.github
                .conflicts
                .get(key)
                .and_then(|found| found.first())
                .map(|conflict| conflict.commit.clone()),
        ) else {
            return Task::none();
        };
        let (named, repo_path, key) = (state.key(), repo_path.to_owned(), key.clone());
        perform(
            {
                let (repo_path, commit) = (repo_path.clone(), commit.clone());
                async move {
                    api::file_commits(&client, &named, &repo_path, &commit, 1)
                        .await
                        .ok()
                        .and_then(|found| found.into_iter().next())
                }
            },
            move |found| Action::ReviewMeta(key, repo_path, commit, found),
        )
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
                let save = self.save_drawing(None);
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

    /// Writes the choice here off the UI thread; [`Self::github_settled`]
    /// goes on once it is written.
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
        if self.github.settling {
            return Task::none();
        }
        let Some(mut state) = self.github_state(&key).cloned() else {
            return Task::none();
        };
        self.github.settling = true;
        let write = {
            let key = key.clone();
            move || {
                let copy = send::resolve(&root, &key, &mut state, &conflict, choice)?;
                let touched = [Some(conflict.repo_path.clone()), copy.clone()];
                let read = workcopy::hashes(&root, &key, touched.into_iter().flatten().collect());
                Ok(Settled {
                    state,
                    conflict,
                    copy,
                    read,
                })
            }
        };
        self.github_write(write, move |result| {
            Action::Settled(key, choice, Box::new(result))
        })
    }

    /// The choice is written: it shows in the drawing's tab, then the next
    /// conflict shows or, after the last, the repository sends.
    pub(super) fn github_settled(
        &mut self,
        key: RepoKey,
        choice: Choice,
        written: Result<Settled, String>,
    ) -> Task<Message> {
        let Settled {
            mut state,
            conflict,
            copy,
            read,
        } = match written {
            Ok(settled) => settled,
            Err(error) => {
                self.status = format!("Couldn't keep it: {error}");
                self.error = true;
                return self.github_wrote();
            }
        };
        let (Some(root), Some(live)) = (self.github.root.clone(), self.github_state(&key)) else {
            return self.github_wrote();
        };
        // Only the files the choice touched: the rest may have moved on.
        for path in [Some(&conflict.repo_path), copy.as_ref()]
            .into_iter()
            .flatten()
        {
            match state.files.remove(path) {
                Some(file) => live.files.insert(path.clone(), file),
                None => live.files.remove(path),
            };
        }
        self.github.seq += 1;
        let seq = self.github.seq;
        self.github.hashed(&key, seq, read);
        if let Some(left) = self.github.conflicts.get_mut(&key) {
            left.retain(|other| other.repo_path != conflict.repo_path);
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
        // Keep both: GitHub's opens beside yours.
        let theirs = match (choice, &copy) {
            (Choice::KeepBoth, Some(_)) if path.exists() => {
                crate::app::files::open_paths(vec![path.clone()])
            }
            _ => Task::none(),
        };
        let undo = crate::app::shortcuts::label(&Message::Undo).unwrap_or_default();
        self.status = match (choice, &copy) {
            (Choice::KeepMine, _) => "Kept yours".into(),
            (Choice::KeepBoth, Some(copy)) => format!("Kept both · yours is {}", file_name(copy)),
            _ if tab.is_some() && conflict.theirs.is_some() => {
                format!("Kept GitHub’s · {undo} brings yours back")
            }
            _ => "Kept GitHub’s".into(),
        };
        self.error = false;
        let persist = self.github_keep_state(&key, true);
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
            return Task::batch([persist, theirs, self.github_review(key, done)]);
        }
        let close = self.github_close_dialog();
        let unsent = self.github.unsent(&key);
        let send = if unsent.is_empty() {
            Task::none()
        } else {
            let send = self.github_send(key.clone(), send::default_message(&unsent), None);
            if self.github.sending.contains(&key) {
                self.github.kept.insert(key, self.status.clone());
            }
            send
        };
        Task::batch([persist, close, theirs, send])
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
            (_, _, Some(theirs)) => {
                self.replace_with_stored(theirs);
                self.tab.mark_saved(self.tab.doc.clone());
            }
            // Nothing here to show: yours stays open as an unsaved drawing.
            (_, _, None) => {
                self.tab.untitled_name = Some(self.tab.name());
                self.tab.path = None;
                self.tab.mark_saved(Document::default());
            }
        }
    }
}

pub(super) fn view<'a>(review: &'a Review, me: Option<&str>) -> Element<'a, Message> {
    let mut title = row![text(format!("“{}” changed on GitHub", review.name)).size(18)]
        .push(review.compare.verdict_badge())
        .push(Space::new().width(Length::Fill))
        .spacing(10)
        .align_y(Alignment::Center);
    if review.total > 1 {
        let at = format!("{} of {}", review.done + 1, review.total);
        title = title.push(text(at).size(13).style(muted_text));
    }
    let button = |id: &'static str, label: &'static str, action: Action| {
        reshiki::accessibility::button(id, label, text(label).size(13))
            .padding([7, 14])
            .style(crate::appearance::secondary)
            .on_press(Message::GitHub(action))
    };
    let keep = |id, label, choice| {
        container(button(id, label, Action::Resolve(choice))).center_x(Length::Fill)
    };
    // `On GitHub · @alice · 14:02 · message` beside `Your drawing · not sent`.
    let theirs = review.theirs.as_ref().map_or_else(String::new, |found| {
        let who = super::versions::who(found.login.as_deref(), &found.author, me);
        format!(
            "{who} · {} · {}",
            super::versions::clock(&found.date),
            found.message
        )
    });
    let yours = review.saved.as_ref().map_or_else(
        || "not sent".to_owned(),
        |at| format!("not sent · saved {at}"),
    );
    let sides = [
        Side::new("On GitHub", theirs),
        Side::new("Your drawing", yours),
    ];
    let keep_both = tooltip(
        button(
            "github-keep-both",
            "Keep both",
            Action::Resolve(Choice::KeepBoth),
        ),
        container(text(format!("Yours becomes {}", review.copy)).size(12))
            .padding([4, 8])
            .style(container::rounded_box),
        tooltip::Position::Top,
    );
    column![
        title,
        review.compare.view(
            sides.clone(),
            [
                Some(keep("github-keep-theirs", "Keep GitHub’s", Choice::KeepTheirs).into()),
                Some(keep("github-keep-mine", "Keep mine", Choice::KeepMine).into()),
            ],
        ),
        row![
            button("github-later", "Later", Action::Close),
            text(reading(&sides)).size(12).style(muted_text),
            Space::new().width(Length::Fill),
            keep_both,
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    ]
    .spacing(12)
    .into()
}
