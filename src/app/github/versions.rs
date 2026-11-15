//! Versions: the saves on this computer since the last send, and the commits
//! on GitHub that touched the drawing, by day. Open shows one in a new tab;
//! Restore makes it the drawing again, as one undoable edit and a new save.
use super::{Action as GitHubAction, perform};
use crate::app::{App, Message, workspace::muted_text};
use chrono::{DateTime, Local, NaiveDate};
use iced::widget::{Space, column, container, row, text};
use iced::{Alignment, Element, Length, Task};
use reshiki::document::Document;
use reshiki::github::{self as gh, RepoKey, api, workcopy};
use std::sync::Arc;

#[derive(Debug, Default)]
pub(super) struct Versions {
    /// The drawing listed, by repository folder and repository path.
    pub(super) target: Option<(RepoKey, String)>,
    /// The repository as GitHub last named it, for asking GitHub.
    pub(super) named: Option<RepoKey>,
    pub(super) generation: u64,
    /// Newest first; `None` while reading.
    local: Option<Vec<(String, Arc<Document>)>>,
    remote: Option<Vec<Remote>>,
    /// The head the pages count from.
    head: Option<String>,
    page: u32,
    more: bool,
    selected: Option<Pick>,
    error: Option<String>,
}

#[derive(Debug, Clone)]
struct Remote {
    version: api::RemoteVersion,
    doc: Option<Arc<Document>>,
    /// That commit deleted the drawing.
    deleted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pick {
    /// A save, by its id.
    Local(String),
    /// A commit.
    Remote(String),
}

const PAGE: usize = 20;

#[derive(Debug, Clone)]
pub enum Action {
    LocalLoaded(u64, Result<Vec<(String, Arc<Document>)>, String>),
    RemoteLoaded(u64, Result<(String, Vec<api::RemoteVersion>), gh::Error>),
    /// A commit's drawing; `Err(true)` when that commit deleted it.
    Doc(u64, String, Result<Arc<Document>, bool>),
    LoadMore,
    Select(Pick),
    /// The version beside the drawing now, with what differs marked.
    Compare,
    Open,
    Restore,
}

fn msg(action: Action) -> Message {
    Message::GitHub(GitHubAction::Versions(action))
}

/// `Today`, `Yesterday` or `12 Nov`.
pub(super) fn day(date: NaiveDate) -> String {
    let today = Local::now().date_naive();
    if date == today {
        "Today".into()
    } else if today.pred_opt() == Some(date) {
        "Yesterday".into()
    } else {
        date.format("%-d %b").to_string()
    }
}

fn remote_date(date: &str) -> Option<DateTime<Local>> {
    DateTime::parse_from_rfc3339(date)
        .ok()
        .map(|time| time.with_timezone(&Local))
}

fn local(id: &str) -> Option<DateTime<Local>> {
    id.parse::<i64>()
        .ok()
        .and_then(DateTime::from_timestamp_millis)
        .map(|time| time.with_timezone(&Local))
}

/// A commit's time: `14:02` today, else `12 Nov 14:02`.
pub(super) fn clock(date: &str) -> String {
    remote_date(date).map_or_else(
        || date.to_owned(),
        |time| match day(time.date_naive()).as_str() {
            "Today" => time.format("%H:%M").to_string(),
            other => format!("{other} {}", time.format("%H:%M")),
        },
    )
}

/// `Today 23:02`, or `12 Nov` on an older day, for a picker tile.
pub(super) fn tile_time(time: DateTime<Local>) -> String {
    match day(time.date_naive()).as_str() {
        older @ ("Today" | "Yesterday") => format!("{older} {}", time.format("%H:%M")),
        older => older.to_owned(),
    }
}

/// `Saved 18:33`; seconds only when another save shares the minute.
pub(super) fn saved(id: &str, ids: &[&str]) -> String {
    let Some(time) = local(id) else {
        return "Saved".into();
    };
    let minute = |time: DateTime<Local>| time.format("%Y%m%d%H%M").to_string();
    let shared = ids
        .iter()
        .filter(|other| local(other).is_some_and(|other| minute(other) == minute(time)))
        .count()
        > 1;
    let at = time.format(if shared { "%H:%M:%S" } else { "%H:%M" });
    match day(time.date_naive()).as_str() {
        "Today" => format!("Saved {at}"),
        other => format!("Saved {other} {at}"),
    }
}

/// A message ReShiki wrote, such as `Update ring.rsk`: shown grey.
pub(super) fn default_message(message: &str) -> bool {
    message
        .strip_prefix("Update ")
        .is_some_and(|rest| rest.ends_with(".rsk") || rest.ends_with(" more"))
}

/// `Add an ethyl group · @alice · 23:02`.
fn commit_label(version: &api::RemoteVersion) -> String {
    let who = version
        .login
        .as_ref()
        .map_or_else(|| version.author.clone(), |login| format!("@{login}"));
    format!("{} · {who} · {}", version.message, clock(&version.date))
}

fn parse(bytes: Vec<u8>) -> Option<Arc<Document>> {
    Document::from_native_file(&bytes).ok().map(Arc::new)
}

impl App {
    pub(super) fn versions_load(&mut self, target: Option<(RepoKey, String)>) -> Task<Message> {
        self.github.versions = Versions {
            target: target.clone(),
            generation: self.github.versions.generation + 1,
            ..Versions::default()
        };
        let Some((key, repo_path)) = target else {
            return Task::none();
        };
        let local = self.versions_reload_local();
        let Some(client) = self.github.client() else {
            self.github.versions.remote = Some(Vec::new());
            return local;
        };
        let Some((key, branch)) = self
            .github_state(&key)
            .map(|state| (state.key(), state.branch.clone()))
        else {
            self.github.versions.remote = Some(Vec::new());
            return local;
        };
        self.github.versions.named = Some(key.clone());
        let generation = self.github.versions.generation;
        let remote = perform(
            async move {
                let head = api::head(&client, &key, &branch).await?.commit;
                let versions = api::file_commits(&client, &key, &repo_path, &head, 1).await?;
                Ok((head, versions))
            },
            move |result| GitHubAction::Versions(Action::RemoteLoaded(generation, result)),
        );
        Task::batch([local, remote])
    }

    /// Reads the saves again, after a save or a send.
    pub(super) fn versions_reload_local(&mut self) -> Task<Message> {
        let (Some(root), Some((key, repo_path))) = (
            self.github.root.clone(),
            self.github.versions.target.clone(),
        ) else {
            return Task::none();
        };
        let generation = self.github.versions.generation;
        let read = move || {
            let mut saves = Vec::new();
            for save in workcopy::snapshots(&root, &key, &repo_path)?
                .into_iter()
                .rev()
            {
                if let Some(doc) = std::fs::read(&save.path).ok().and_then(parse) {
                    saves.push((save.id, doc));
                }
            }
            Ok(saves)
        };
        perform(
            async move {
                reshiki::task::spawn_blocking(read)
                    .await
                    .map_err(|error| error.to_string())?
            },
            move |result| GitHubAction::Versions(Action::LocalLoaded(generation, result)),
        )
    }

    fn versions_docs(&mut self, commits: Vec<String>) -> Task<Message> {
        let versions = &self.github.versions;
        let (Some(client), Some(key), Some((_, repo_path))) = (
            self.github.client(),
            versions.named.clone(),
            versions.target.clone(),
        ) else {
            return Task::none();
        };
        let generation = versions.generation;
        Task::batch(commits.into_iter().map(|commit| {
            let (client, key, repo_path) = (client.clone(), key.clone(), repo_path.clone());
            let at = commit.clone();
            perform(
                async move {
                    match api::raw_at(&client, &key, &repo_path, &at).await {
                        Ok(bytes) => reshiki::task::spawn_blocking(move || parse(bytes))
                            .await
                            .ok()
                            .flatten()
                            .ok_or(false),
                        Err(gh::Error::NotFound) => Err(true),
                        Err(_) => Err(false),
                    }
                },
                move |result| GitHubAction::Versions(Action::Doc(generation, commit, result)),
            )
        }))
    }

    pub(super) fn versions_action(&mut self, action: Action) -> Task<Message> {
        let generation = self.github.versions.generation;
        match action {
            Action::LocalLoaded(at, result) if at == generation => match result {
                Ok(saves) => self.github.versions.local = Some(saves),
                Err(error) => {
                    self.github.versions.local = Some(Vec::new());
                    self.github.versions.error = Some(error);
                }
            },
            Action::RemoteLoaded(at, result) if at == generation => match result {
                Ok((head, found)) => {
                    let versions = &mut self.github.versions;
                    versions.head = Some(head);
                    versions.page += 1;
                    versions.more = found.len() == PAGE;
                    let commits: Vec<String> = found.iter().map(|v| v.commit.clone()).collect();
                    versions
                        .remote
                        .get_or_insert_default()
                        .extend(found.into_iter().map(|version| Remote {
                            version,
                            doc: None,
                            deleted: false,
                        }));
                    return self.versions_docs(commits);
                }
                Err(error) => {
                    let error = self.github_failed(&error);
                    let versions = &mut self.github.versions;
                    versions.remote.get_or_insert_default();
                    versions.error = Some(error);
                }
            },
            Action::Doc(at, commit, result) if at == generation => {
                let remote = self.github.versions.remote.iter_mut().flatten();
                if let Some(found) = remote.into_iter().find(|r| r.version.commit == commit) {
                    match result {
                        Ok(doc) => found.doc = Some(doc),
                        Err(deleted) => found.deleted = deleted,
                    }
                }
            }
            Action::LoadMore => {
                let versions = &self.github.versions;
                let (Some(client), Some(key), Some((_, repo_path)), Some(head)) = (
                    self.github.client(),
                    versions.named.clone(),
                    versions.target.clone(),
                    versions.head.clone(),
                ) else {
                    return Task::none();
                };
                let page = versions.page + 1;
                self.github.versions.more = false;
                return perform(
                    async move {
                        let found =
                            api::file_commits(&client, &key, &repo_path, &head, page).await?;
                        Ok((head, found))
                    },
                    move |result| GitHubAction::Versions(Action::RemoteLoaded(generation, result)),
                );
            }
            Action::Select(pick) => self.github.versions.selected = Some(pick),
            Action::Compare => {
                let Some((label, doc)) = self.versions_picked() else {
                    return Task::none();
                };
                let compare =
                    super::compare::Compare::new(Ok((*doc).clone()), self.tab.doc.clone());
                self.github.dialog = Some(super::Dialog::Compare(Box::new(compare), label));
                return Task::none();
            }
            Action::Open => {
                let Some((label, doc)) = self.versions_picked() else {
                    return Task::none();
                };
                let stem = std::path::Path::new(&self.tab.name())
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.open_target(None);
                self.show_opened((*doc).clone(), None);
                // Unsaved, so closing it asks: it may be the last copy of a save.
                self.tab.mark_saved(Document::default());
                self.tab.untitled_name = Some(format!("{stem} · {label}"));
                return crate::app::focus_drawing();
            }
            Action::Restore => {
                let Some((label, doc)) = self.versions_picked() else {
                    return Task::none();
                };
                // From the Compare dialog: it closes.
                if matches!(self.github.dialog, Some(super::Dialog::Compare(..))) {
                    self.github.dialog = None;
                }
                let front = self.github.locate(self.tab.path.as_deref());
                if front != self.github.versions.target {
                    return Task::none();
                }
                if self.tab.atom_text.is_some()
                    || self.tab.inline_text.is_some()
                    || self.tab.cleanup.is_some()
                    || self.tab.optimization.is_some()
                {
                    self.status = "Finish editing first".into();
                    self.error = true;
                    return Task::none();
                }
                if self.file_io.saving {
                    self.status = "Wait for the save to finish".into();
                    self.error = true;
                    return Task::none();
                }
                if let Err(error) = self.try_replace_document((*doc).clone(), Vec::new()) {
                    self.status = error;
                    self.error = true;
                    return Task::none();
                }
                let save = crate::app::tagged(self.save_drawing(false), self.tab.id);
                self.status = format!("Restored {label}");
                self.error = false;
                return save;
            }
            // A result for a list since replaced.
            Action::LocalLoaded(..) | Action::RemoteLoaded(..) | Action::Doc(..) => {}
        }
        Task::none()
    }

    /// The selected version's time and drawing.
    fn versions_picked(&self) -> Option<(String, Arc<Document>)> {
        let versions = &self.github.versions;
        match versions.selected.as_ref()? {
            Pick::Local(id) => {
                let saves = versions.local.as_ref()?;
                let ids: Vec<&str> = saves.iter().map(|(save, _)| save.as_str()).collect();
                saves
                    .iter()
                    .find(|(save, _)| save == id)
                    .map(|(save, doc)| (saved(save, &ids), doc.clone()))
            }
            Pick::Remote(commit) => versions
                .remote
                .iter()
                .flatten()
                .find(|remote| &remote.version.commit == commit)
                .and_then(|remote| Some((commit_label(&remote.version), remote.doc.clone()?))),
        }
    }

    pub(in crate::app) fn versions_panel(&self) -> Element<'_, Message> {
        let versions = &self.github.versions;
        let front = self.github.locate(self.tab.path.as_deref());
        if front.is_none() {
            return text("Not a GitHub drawing.").size(12).style(muted_text).into();
        }
        let selected = versions.selected.as_ref();
        let group = |label: &'static str| text(label).size(12).style(muted_text);
        let mut body = column![group("This computer")].spacing(6);
        match &versions.local {
            None => body = body.push(text("Reading…").size(12).style(muted_text)),
            Some(saves) if saves.is_empty() => {
                body = body.push(
                    text("No saves since the last send.")
                        .size(12)
                        .style(muted_text),
                );
            }
            Some(saves) => {
                let ids: Vec<&str> = saves.iter().map(|(save, _)| save.as_str()).collect();
                for (id, doc) in saves {
                    body = body.push(version_row(
                        Pick::Local(id.clone()),
                        selected,
                        Some(doc),
                        saved(id, &ids),
                        None,
                        false,
                    ));
                }
            }
        }
        body = body.push(Space::new().height(6)).push(group("GitHub"));
        match &versions.remote {
            None => body = body.push(text("Listing…").size(12).style(muted_text)),
            Some(remote) if remote.is_empty() && self.github.client().is_none() => {
                body = body.push(
                    text("Sign in to see GitHub's versions.")
                        .size(12)
                        .style(muted_text),
                );
            }
            Some(remote) if remote.is_empty() => {
                body = body.push(text("Not on GitHub yet.").size(12).style(muted_text));
            }
            Some(remote) => {
                let mut shown_day = None;
                for found in remote {
                    let version = &found.version;
                    let time = remote_date(&version.date);
                    let today = time.map(|time| day(time.date_naive()));
                    if today != shown_day {
                        if let Some(today) = &today {
                            body = body.push(
                                container(text(today.clone()).size(11).style(muted_text))
                                    .padding([2, 4]),
                            );
                        }
                        shown_day = today;
                    }
                    let who = version
                        .login
                        .as_ref()
                        .map_or_else(|| version.author.clone(), |login| format!("@{login}"));
                    let at = time.map_or_else(String::new, |time| time.format("%H:%M").to_string());
                    body = body.push(version_row(
                        Pick::Remote(version.commit.clone()),
                        selected,
                        found.doc.as_ref(),
                        version.message.clone(),
                        Some((format!("{who} · {at}"), default_message(&version.message))),
                        found.deleted,
                    ));
                }
                if versions.more {
                    body = body.push(
                        reshiki::accessibility::button(
                            "github-versions-more",
                            "Load more",
                            text("Load more").size(12),
                        )
                        .padding([5, 10])
                        .style(crate::app::workspace::control(false))
                        .on_press(msg(Action::LoadMore)),
                    );
                }
            }
        }
        if let Some(error) = &versions.error {
            body = body.push(text(error.as_str()).size(11).style(muted_text));
        }
        body.into()
    }
}

/// One version: its drawing, its message or save time and, for a commit,
/// who made it and when. The selected one has its actions on one line.
fn version_row<'a>(
    pick: Pick,
    selected: Option<&Pick>,
    doc: Option<&'a Arc<Document>>,
    title: String,
    // The author line, and whether the title is a message ReShiki wrote.
    commit: Option<(String, bool)>,
    deleted: bool,
) -> Element<'a, Message> {
    let selected = selected == Some(&pick);
    let thumb: Element<'a, Message> = match doc {
        Some(doc) => crate::canvas::layered::canvas(crate::canvas::DrawingPreview(doc))
            .width(64)
            .height(42)
            .into(),
        None => container(
            text(if deleted { "Deleted" } else { "" })
                .size(10)
                .style(muted_text),
        )
        .center_x(64)
        .center_y(42)
        .into(),
    };
    let grey = commit.as_ref().is_some_and(|(_, grey)| *grey);
    let mut details = column![
        text(title.clone())
            .size(12)
            .wrapping(text::Wrapping::None)
            .style(move |theme: &iced::Theme| text::Style {
                color: grey.then(|| crate::appearance::muted(theme)),
            })
    ]
    .spacing(2);
    let mut spoken = title;
    if let Some((by, _)) = commit {
        spoken = format!("{spoken}, {by}");
        details = details.push(text(by).size(11).style(muted_text));
    }
    let id = match &pick {
        Pick::Local(id) => format!("github-version-local-{id}"),
        Pick::Remote(commit) => format!("github-version-remote-{commit}"),
    };
    let mut item = column![
        reshiki::accessibility::button(
            id,
            spoken,
            row![thumb, container(details).clip(true)]
                .spacing(8)
                .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding(4)
        .style(crate::app::workspace::control(selected))
        .on_press(msg(Action::Select(pick))),
    ]
    .spacing(4);
    if selected && !deleted {
        let action = |id: &'static str, label: &'static str, action: Action| {
            reshiki::accessibility::button(id, label, text(label).size(12))
                .padding([4, 10])
                .style(crate::appearance::outline)
                .on_press(msg(action))
        };
        item = item.push(
            row![
                action("github-version-compare", "Compare", Action::Compare),
                action("github-version-open", "Open", Action::Open),
                action("github-version-restore", "Restore", Action::Restore),
            ]
            .spacing(6),
        );
    }
    item.into()
}
