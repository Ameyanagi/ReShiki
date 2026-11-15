//! Versions: the saves on this computer since the last send, and the commits
//! on GitHub that touched the drawing. Open shows one in a new tab; Restore
//! makes it the drawing again, as one undoable edit and a new save.
use super::{Action as GitHubAction, perform};
use crate::app::{App, Message, workspace::muted_text};
use chrono::{DateTime, Local};
use iced::widget::{column, container, row, text};
use iced::{Alignment, Element, Length, Task};
use reshiki::document::Document;
use reshiki::github::{self as gh, RepoKey, api, workcopy};
use std::sync::Arc;

#[derive(Debug, Default)]
pub(super) struct Versions {
    /// The drawing listed, by repository folder and repository path.
    pub(super) target: Option<(RepoKey, String)>,
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

/// `Today 14:02`, or `Nov 3 14:02` on another day.
fn when(time: DateTime<Local>) -> String {
    if time.date_naive() == Local::now().date_naive() {
        time.format("Today %H:%M").to_string()
    } else {
        time.format("%b %-d %H:%M").to_string()
    }
}

fn local_time(id: &str) -> String {
    id.parse::<i64>()
        .ok()
        .and_then(DateTime::from_timestamp_millis)
        .map_or_else(String::new, |time| when(time.with_timezone(&Local)))
}

fn remote_time(date: &str) -> String {
    DateTime::parse_from_rfc3339(date)
        .map_or_else(|_| date.to_owned(), |time| when(time.with_timezone(&Local)))
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
        let Some(branch) = self.github_state(&key).map(|state| state.branch.clone()) else {
            self.github.versions.remote = Some(Vec::new());
            return local;
        };
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
        let (Some(client), Some((key, repo_path))) =
            (self.github.client(), self.github.versions.target.clone())
        else {
            return Task::none();
        };
        let generation = self.github.versions.generation;
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
                let (Some(client), Some((key, repo_path)), Some(head)) = (
                    self.github.client(),
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
                return self.accessibility_refresh();
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
                self.tab.untitled_name = Some(format!("{stem} · {label}"));
                return crate::app::focus_drawing();
            }
            Action::Restore => {
                let Some((label, doc)) = self.versions_picked() else {
                    return Task::none();
                };
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
                if let Err(error) = self.try_replace_document((*doc).clone(), Vec::new()) {
                    self.status = error;
                    self.error = true;
                    return Task::none();
                }
                let save = self.save_drawing(false);
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
            Pick::Local(id) => versions
                .local
                .iter()
                .flatten()
                .find(|(save, _)| save == id)
                .map(|(save, doc)| (local_time(save), doc.clone())),
            Pick::Remote(commit) => versions
                .remote
                .iter()
                .flatten()
                .find(|remote| &remote.version.commit == commit)
                .and_then(|remote| Some((remote_time(&remote.version.date), remote.doc.clone()?))),
        }
    }

    pub(in crate::app) fn versions_panel(&self) -> Element<'_, Message> {
        let versions = &self.github.versions;
        let front = self.github.locate(self.tab.path.as_deref());
        if front.is_none() {
            return container(text("Not a GitHub drawing.").size(12).style(muted_text))
                .padding(16)
                .into();
        }
        let selected = versions.selected.as_ref();
        let mut body = column![crate::app::workspace::section(
            if cfg!(target_os = "macos") {
                "ON THIS MAC"
            } else {
                "ON THIS COMPUTER"
            }
        )]
        .spacing(8);
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
                for (id, doc) in saves {
                    body = body.push(version_row(
                        Pick::Local(id.clone()),
                        selected,
                        Some(doc),
                        vec![local_time(id)],
                        false,
                    ));
                }
            }
        }
        body = body.push(crate::app::workspace::section("ON GITHUB"));
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
                for found in remote {
                    let version = &found.version;
                    let who = version
                        .login
                        .as_ref()
                        .map_or_else(|| version.author.clone(), |login| format!("@{login}"));
                    body = body.push(version_row(
                        Pick::Remote(version.commit.clone()),
                        selected,
                        found.doc.as_ref(),
                        vec![
                            version.message.clone(),
                            format!("{who} · {}", remote_time(&version.date)),
                        ],
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
        iced::widget::scrollable(container(body).padding([12, 14]))
            .height(Length::Fill)
            .into()
    }
}

/// One version: its drawing, its time and, for a commit, the message and author.
fn version_row<'a>(
    pick: Pick,
    selected: Option<&Pick>,
    doc: Option<&'a Arc<Document>>,
    lines: Vec<String>,
    deleted: bool,
) -> Element<'a, Message> {
    let selected = selected == Some(&pick);
    let thumb: Element<'a, Message> = match doc {
        Some(doc) => crate::canvas::layered::canvas(crate::canvas::DrawingPreview(doc))
            .width(120)
            .height(80)
            .into(),
        None => container(
            text(if deleted { "Deleted" } else { "" })
                .size(11)
                .style(muted_text),
        )
        .center_x(120)
        .center_y(80)
        .into(),
    };
    let label = lines.first().cloned().unwrap_or_default();
    let mut details = column![].spacing(2);
    for (at, line) in lines.into_iter().enumerate() {
        details = details.push(if at == 0 {
            text(line).size(12)
        } else {
            text(line).size(11).style(muted_text)
        });
    }
    let id = match &pick {
        Pick::Local(id) => format!("github-version-local-{id}"),
        Pick::Remote(commit) => format!("github-version-remote-{commit}"),
    };
    let mut item = column![
        reshiki::accessibility::button(
            id,
            label,
            row![thumb, details].spacing(8).align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding(4)
        .style(crate::app::workspace::control(selected))
        .on_press(msg(Action::Select(pick))),
    ]
    .spacing(4);
    if selected && !deleted {
        item = item.push(
            row![
                reshiki::accessibility::button(
                    "github-version-compare",
                    "Compare with current",
                    text("Compare with current").size(12)
                )
                .padding([5, 10])
                .style(crate::appearance::secondary)
                .on_press(msg(Action::Compare)),
                reshiki::accessibility::button(
                    "github-version-open",
                    "Open",
                    text("Open").size(12)
                )
                .padding([5, 10])
                .style(crate::appearance::secondary)
                .on_press(msg(Action::Open)),
                reshiki::accessibility::button(
                    "github-version-restore",
                    "Restore",
                    text("Restore").size(12)
                )
                .padding([5, 10])
                .style(crate::appearance::primary)
                .on_press(msg(Action::Restore)),
            ]
            .spacing(6)
            .wrap(),
        );
    }
    item.into()
}
