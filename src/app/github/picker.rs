//! The repository picker: open a drawing from GitHub, or choose where a new
//! one goes ("Save here"). Folders are listed one at a time as they open, so
//! the size of a repository does not matter.
use super::{Action as GitHubAction, Dialog, perform};
use crate::app::{App, Message, workspace::muted_text};
use iced::widget::{Space, column, container, row, scrollable, text};
use iced::{Alignment, Element, Length, Task};
use reshiki::document::Document;
use reshiki::github::{
    self as gh, Entry, Repo, RepoKey, api,
    workcopy::{self, FileState, RepoState},
};
use std::collections::HashMap;
use std::sync::Arc;

const SEARCH: &str = "github-search";
const NEW_NAME: &str = "github-new-repo";
const SAVE_NAME: &str = "github-save-name";
const NAME_TAKEN: &str = "Name taken";
const CANT_OPEN: &str = "Can't open this name here";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Open,
    /// Choose where a drawing that is not on GitHub yet goes, then send it.
    SaveHere,
}

#[derive(Debug)]
pub(in crate::app) struct Picker {
    mode: Mode,
    /// Rises with every listing, so a late one drops.
    generation: u64,
    search: String,
    /// `None` until GitHub lists them.
    repos: Option<Vec<Repo>>,
    /// The highlighted repository in the filtered list.
    highlighted: usize,
    repo: Option<Repo>,
    /// The top folder's tree, then each folder opened below it.
    root: Option<String>,
    trail: Vec<(String, String)>,
    /// Folders first, then drawings; `None` while listing.
    entries: Option<Vec<Entry>>,
    selected: Option<usize>,
    thumbs: HashMap<String, Option<Arc<Document>>>,
    new_repo: Option<String>,
    save_name: String,
    busy: bool,
    pub(super) error: Option<String>,
}

impl Picker {
    fn new(mode: Mode, save_name: String) -> Self {
        Self {
            mode,
            generation: 0,
            search: String::new(),
            repos: None,
            highlighted: 0,
            repo: None,
            root: None,
            trail: Vec::new(),
            entries: None,
            selected: None,
            thumbs: HashMap::new(),
            new_repo: None,
            save_name,
            busy: false,
            error: None,
        }
    }

    /// The repositories GitHub listed; "Save here" offers only those the
    /// user may send to.
    pub(super) fn listed(&mut self, repos: &[Repo]) {
        let mode = self.mode;
        self.repos = Some(
            repos
                .iter()
                .filter(|repo| mode == Mode::Open || repo.push)
                .cloned()
                .collect(),
        );
    }

    fn shown_repos(&self) -> Vec<&Repo> {
        let search = self.search.to_lowercase();
        self.repos
            .iter()
            .flatten()
            .filter(|repo| {
                format!("{}/{}", repo.key.owner, repo.key.repo)
                    .to_lowercase()
                    .contains(&search)
            })
            .collect()
    }

    /// The folder open now, as a repository path (`""` is the top).
    fn folder(&self) -> String {
        self.trail
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>()
            .join("/")
    }

    fn repo_path(&self, name: &str) -> String {
        match self.folder().as_str() {
            "" => name.to_owned(),
            folder => format!("{folder}/{name}"),
        }
    }

    fn selected_entry(&self) -> Option<&Entry> {
        self.entries.as_ref()?.get(self.selected?)
    }
}

/// Folders, then drawings, each by name; previews and other files are left out.
fn shown(entries: Vec<Entry>) -> Vec<Entry> {
    let mut entries: Vec<Entry> = entries
        .into_iter()
        .filter(|entry| entry.folder || entry.name.ends_with(".rsk"))
        .collect();
    entries.sort_by(|a, b| (!a.folder, &a.name).cmp(&(!b.folder, &b.name)));
    entries
}

#[derive(Debug, Clone)]
pub enum Action {
    Show(Mode),
    Search(String),
    Move(isize),
    PickRepo(RepoKey),
    Listing(u64, Result<(String, Vec<Entry>), gh::Error>),
    Folder(String),
    Up,
    Select(usize),
    /// Enter: open what is highlighted.
    Enter,
    Thumb(String, Option<Arc<Document>>),
    Open,
    Downloaded(u64, Repo, String, Result<api::Download, gh::Error>),
    NewRepo,
    NewRepoName(String),
    CreateRepo,
    RepoCreated(u64, Result<Repo, gh::Error>),
    SaveName(String),
    SaveHere,
}

fn msg(action: Action) -> Message {
    Message::GitHub(GitHubAction::Picker(action))
}

impl App {
    fn picker(&mut self) -> Option<&mut Picker> {
        match &mut self.github.dialog {
            Some(Dialog::Picker(picker)) => Some(picker.as_mut()),
            _ => None,
        }
    }

    pub(super) fn picker_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Show(mode) => {
                self.github.menu = false;
                if self.github.client().is_none() {
                    return self.github_action(GitHubAction::ShowAccount);
                }
                let stem = std::path::Path::new(&self.tab.name())
                    .file_stem()
                    .map_or("Drawing".into(), |stem| stem.to_string_lossy().into_owned());
                let mut picker = Picker::new(mode, format!("{stem}.rsk"));
                if !self.github.repos.is_empty() {
                    picker.listed(&self.github.repos);
                }
                self.github.dialog = Some(Dialog::Picker(Box::new(picker)));
                return Task::batch([
                    self.github_list(),
                    iced::widget::operation::focus(SEARCH),
                    self.accessibility_refresh(),
                ]);
            }
            Action::Search(search) => {
                if let Some(picker) = self.picker() {
                    picker.search = search;
                    picker.highlighted = 0;
                }
            }
            Action::Move(step) => {
                let Some(picker) = self.picker() else {
                    return Task::none();
                };
                let step_in = |at: usize, len: usize| {
                    (at as isize + step).clamp(0, len.saturating_sub(1) as isize) as usize
                };
                if picker.repo.is_some() {
                    let len = picker.entries.as_ref().map_or(0, Vec::len);
                    picker.selected =
                        (len > 0).then(|| picker.selected.map_or(0, |at| step_in(at, len)));
                } else {
                    let len = picker.shown_repos().len();
                    picker.highlighted = step_in(picker.highlighted, len);
                }
            }
            Action::PickRepo(key) => {
                let Some(repo) = self
                    .github
                    .repos
                    .iter()
                    .find(|repo| repo.key == key)
                    .cloned()
                else {
                    return Task::none();
                };
                let Some(client) = self.github.client() else {
                    return Task::none();
                };
                let Some(picker) = self.picker() else {
                    return Task::none();
                };
                picker.generation += 1;
                let generation = picker.generation;
                picker.repo = Some(repo.clone());
                picker.root = None;
                picker.trail.clear();
                picker.entries = None;
                picker.selected = None;
                picker.error = None;
                return perform(
                    async move {
                        match api::head(&client, &repo.key, &repo.default_branch).await {
                            Ok(head) => {
                                let entries = api::tree(&client, &repo.key, &head.tree).await?;
                                Ok((head.tree, entries))
                            }
                            // An empty repository has nothing to list yet.
                            Err(gh::Error::Conflict) => Ok((String::new(), Vec::new())),
                            Err(error) => Err(error),
                        }
                    },
                    move |result| GitHubAction::Picker(Action::Listing(generation, result)),
                );
            }
            Action::Listing(generation, result) => {
                let failed = match &result {
                    Err(error) => Some(self.github_failed(error)),
                    Ok(_) => None,
                };
                let Some(picker) = self.picker().filter(|p| p.generation == generation) else {
                    return Task::none();
                };
                match result {
                    Ok((tree, entries)) => {
                        if picker.root.is_none() {
                            picker.root = Some(tree);
                        }
                        let entries = shown(entries);
                        picker.selected = (!entries.is_empty()).then_some(0);
                        let wanted: Vec<String> = entries
                            .iter()
                            .filter(|entry| {
                                !entry.folder && !picker.thumbs.contains_key(&entry.sha)
                            })
                            .map(|entry| entry.sha.clone())
                            .collect();
                        picker.entries = Some(entries);
                        return self.picker_thumbs(wanted);
                    }
                    Err(_) => {
                        picker.entries = Some(Vec::new());
                        picker.error = failed;
                    }
                }
            }
            Action::Folder(name) => {
                let Some(client) = self.github.client() else {
                    return Task::none();
                };
                let Some(picker) = self.picker() else {
                    return Task::none();
                };
                let Some((repo, entry)) = picker.repo.clone().zip(
                    picker
                        .entries
                        .iter()
                        .flatten()
                        .find(|entry| entry.folder && entry.name == name)
                        .cloned(),
                ) else {
                    return Task::none();
                };
                picker.trail.push((name, entry.sha.clone()));
                return self.picker_list(client, repo, entry.sha);
            }
            Action::Up => {
                let Some(client) = self.github.client() else {
                    return Task::none();
                };
                let Some(picker) = self.picker() else {
                    return Task::none();
                };
                if picker.trail.pop().is_none() {
                    // Back to the repositories.
                    picker.repo = None;
                    picker.entries = None;
                    return Task::none();
                }
                let tree = picker
                    .trail
                    .last()
                    .map(|(_, sha)| sha.clone())
                    .or_else(|| picker.root.clone());
                if let (Some(repo), Some(tree)) = (picker.repo.clone(), tree) {
                    return self.picker_list(client, repo, tree);
                }
            }
            Action::Select(at) => {
                if let Some(picker) = self.picker() {
                    picker.selected = Some(at);
                }
            }
            Action::Enter => {
                let Some(picker) = self.picker() else {
                    return Task::none();
                };
                if picker.repo.is_none() {
                    let key = picker
                        .shown_repos()
                        .get(picker.highlighted)
                        .map(|repo| repo.key.clone());
                    return key
                        .map_or_else(Task::none, |key| self.picker_action(Action::PickRepo(key)));
                }
                match picker.selected_entry().cloned() {
                    Some(entry) if entry.folder => {
                        return self.picker_action(Action::Folder(entry.name));
                    }
                    Some(_) if picker.mode == Mode::Open => {
                        return self.picker_action(Action::Open);
                    }
                    _ if picker.mode == Mode::SaveHere => {
                        return self.picker_action(Action::SaveHere);
                    }
                    _ => {}
                }
            }
            Action::Thumb(sha, doc) => {
                if let Some(picker) = self.picker() {
                    picker.thumbs.insert(sha, doc);
                }
            }
            Action::Open => return self.picker_open(),
            Action::Downloaded(generation, repo, repo_path, result) => {
                if !self
                    .picker()
                    .is_some_and(|picker| picker.generation == generation)
                {
                    return Task::none();
                }
                let download = match result {
                    Ok(download) => download,
                    Err(error) => {
                        let error = self.github_failed(&error);
                        if let Some(picker) = self.picker() {
                            picker.busy = false;
                            picker.error = Some(error);
                        }
                        return Task::none();
                    }
                };
                let root = self.github.root.clone().unwrap_or_default();
                let state = self
                    .github
                    .states
                    .entry(repo.key.clone())
                    .or_insert_with(|| RepoState::new(&repo));
                return match workcopy::checkout(&root, &repo.key, state, &repo_path, &download) {
                    Ok(path) => {
                        let persist = self.github_persist(&repo.key);
                        let close = self.github_close_dialog();
                        Task::batch([persist, close, crate::app::files::open_paths(vec![path])])
                    }
                    Err(error) => {
                        if let Some(picker) = self.picker() {
                            picker.busy = false;
                            picker.error = Some(error);
                        }
                        Task::none()
                    }
                };
            }
            Action::NewRepo => {
                if let Some(picker) = self.picker() {
                    picker.new_repo = Some(String::new());
                    return iced::widget::operation::focus(NEW_NAME);
                }
            }
            Action::NewRepoName(name) => {
                if let Some(picker) = self.picker() {
                    picker.new_repo = Some(name);
                }
            }
            Action::CreateRepo => {
                let Some(client) = self.github.client() else {
                    return Task::none();
                };
                let Some(picker) = self.picker() else {
                    return Task::none();
                };
                let Some(name) = picker
                    .new_repo
                    .as_deref()
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map(str::to_owned)
                else {
                    return Task::none();
                };
                picker.busy = true;
                picker.generation += 1;
                let generation = picker.generation;
                return perform(
                    async move { api::create_repo(&client, &name).await },
                    move |result| GitHubAction::Picker(Action::RepoCreated(generation, result)),
                );
            }
            Action::RepoCreated(generation, result) => {
                if !self
                    .picker()
                    .is_some_and(|picker| picker.generation == generation)
                {
                    return Task::none();
                }
                match result {
                    Ok(repo) => {
                        let key = repo.key.clone();
                        self.github.repos.push(repo);
                        let repos = self.github.repos.clone();
                        if let Some(picker) = self.picker() {
                            picker.busy = false;
                            picker.new_repo = None;
                            picker.listed(&repos);
                        }
                        return self.picker_action(Action::PickRepo(key));
                    }
                    Err(error) => {
                        let error = self.github_failed(&error);
                        if let Some(picker) = self.picker() {
                            picker.busy = false;
                            picker.error = Some(error);
                        }
                    }
                }
            }
            Action::SaveName(name) => {
                if let Some(picker) = self.picker() {
                    picker.save_name = name;
                }
            }
            Action::SaveHere => return self.picker_save_here(),
        }
        Task::none()
    }

    fn picker_list(
        &mut self,
        client: gh::client::Client,
        repo: Repo,
        tree: String,
    ) -> Task<Message> {
        let Some(picker) = self.picker() else {
            return Task::none();
        };
        picker.generation += 1;
        picker.entries = None;
        picker.selected = None;
        let generation = picker.generation;
        perform(
            async move {
                let entries = api::tree(&client, &repo.key, &tree).await?;
                Ok((tree, entries))
            },
            move |result| GitHubAction::Picker(Action::Listing(generation, result)),
        )
    }

    /// Each drawing's tile shows the drawing itself, read like any opened
    /// file; no preview file from the repository is parsed.
    fn picker_thumbs(&mut self, shas: Vec<String>) -> Task<Message> {
        let (Some(client), Some(repo)) = (
            self.github.client(),
            self.picker().and_then(|picker| picker.repo.clone()),
        ) else {
            return Task::none();
        };
        Task::batch(shas.into_iter().map(|sha| {
            let (client, key) = (client.clone(), repo.key.clone());
            let blob = sha.clone();
            perform(
                async move {
                    let bytes = api::blob(&client, &key, &blob).await.ok()?;
                    reshiki::task::spawn_blocking(move || Document::from_native_file(&bytes).ok())
                        .await
                        .ok()
                        .flatten()
                        .map(Arc::new)
                },
                move |doc| GitHubAction::Picker(Action::Thumb(sha, doc)),
            )
        }))
    }

    fn picker_open(&mut self) -> Task<Message> {
        let Some(client) = self.github.client() else {
            return Task::none();
        };
        let Some(root) = self.github.root.clone() else {
            return Task::none();
        };
        let Some(picker) = self.picker() else {
            return Task::none();
        };
        let (Some(repo), Some(entry)) = (picker.repo.clone(), picker.selected_entry().cloned())
        else {
            return Task::none();
        };
        if entry.folder {
            return Task::none();
        }
        let repo_path = picker.repo_path(&entry.name);
        let key = repo.key.clone();
        let collides = self
            .github_state(&key)
            .is_some_and(|state| state.collides(&repo_path));
        if !workcopy::valid_repo_path(&repo_path) || collides {
            if let Some(picker) = self.picker() {
                picker.error = Some(CANT_OPEN.into());
            }
            return Task::none();
        }
        let path = workcopy::working_path(&root, &key, &repo_path);
        // An open tab, or a working copy with saves not on GitHub yet, opens
        // as it is: never downloaded over.
        let open = self.strip().any(|tab| tab.path.as_ref() == Some(&path));
        let tracked = self
            .github_state(&key)
            .is_some_and(|state| state.files.contains_key(&repo_path));
        let unsent = tracked && path.exists() && self.github_unsent(Some(&path)).is_some();
        if open || unsent {
            let close = self.github_close_dialog();
            if unsent && !open {
                self.status = "Opened your unsent version".into();
            }
            return Task::batch([close, crate::app::files::open_paths(vec![path])]);
        }
        let Some(picker) = self.picker() else {
            return Task::none();
        };
        picker.busy = true;
        picker.error = None;
        picker.generation += 1;
        let generation = picker.generation;
        perform(
            {
                let (repo, repo_path) = (repo.clone(), repo_path.clone());
                async move { api::download(&client, &repo.key, &repo.default_branch, &repo_path).await }
            },
            move |result| {
                GitHubAction::Picker(Action::Downloaded(generation, repo, repo_path, result))
            },
        )
    }

    /// Puts the drawing in front into the chosen folder, saves it there and
    /// sends it.
    fn picker_save_here(&mut self) -> Task<Message> {
        let Some(root) = self.github.root.clone() else {
            return Task::none();
        };
        let Some(picker) = self.picker() else {
            return Task::none();
        };
        let Some(repo) = picker.repo.clone() else {
            return Task::none();
        };
        let name = picker.save_name.trim().to_owned();
        let name = if name.ends_with(".rsk") {
            name
        } else {
            format!("{name}.rsk")
        };
        let stem = name.trim_end_matches(".rsk");
        let taken = picker
            .entries
            .iter()
            .flatten()
            .any(|entry| entry.name == name || entry.name == format!("{stem}.svg"));
        let repo_path = picker.repo_path(&name);
        let collides = self.github_state(&repo.key).is_some_and(|state| {
            state.collides(&repo_path) || state.files.contains_key(&repo_path)
        });
        let refusal = if !workcopy::valid_repo_path(&repo_path) || stem.is_empty() {
            Some(CANT_OPEN)
        } else if taken || collides {
            Some(NAME_TAKEN)
        } else {
            None
        };
        if let Some(refusal) = refusal {
            if let Some(picker) = self.picker() {
                picker.error = Some(refusal.into());
            }
            return Task::none();
        }
        let key = repo.key.clone();
        let path = workcopy::working_path(&root, &key, &repo_path);
        if let Some(folder) = path.parent()
            && let Err(error) = std::fs::create_dir_all(folder)
        {
            if let Some(picker) = self.picker() {
                picker.error = Some(error.to_string());
            }
            return Task::none();
        }
        self.github
            .states
            .entry(key.clone())
            .or_insert_with(|| RepoState::new(&repo))
            .files
            .insert(repo_path.clone(), FileState::default());
        let persist = self.github_persist(&key);
        let close = self.github_close_dialog();
        self.tab.path = Some(path);
        self.tab.untitled_name = None;
        self.github.after_save = Some((key, reshiki::github::send::default_message(&[repo_path])));
        Task::batch([persist, close, self.save_drawing(false)])
    }

    pub(super) fn picker_view<'a>(&'a self, picker: &'a Picker) -> Element<'a, Message> {
        let search = reshiki::accessibility::text_input(
            SEARCH,
            "Search repositories",
            "Search repositories",
            &picker.search,
        )
        .style(crate::appearance::input_style)
        .on_input(|search| msg(Action::Search(search)))
        .on_submit(msg(Action::Enter))
        .size(13)
        .padding(7);
        let mut repos = column![].spacing(1);
        match &picker.repos {
            None => repos = repos.push(text("Listing…").size(12).style(muted_text)),
            Some(_) => {
                for (at, repo) in picker.shown_repos().into_iter().enumerate() {
                    let current = picker.repo.as_ref().map(|r| &r.key) == Some(&repo.key);
                    let label = format!("{}/{}", repo.key.owner, repo.key.repo);
                    let content = row![text(label.clone()).size(12).width(Length::Fill)]
                        .push(repo.private.then(|| {
                            iced::widget::canvas(crate::app::icons::Glyph(
                                crate::app::icons::Icon::Lock(true),
                                true,
                            ))
                            .width(24)
                            .height(24)
                        }))
                        .align_y(Alignment::Center);
                    repos = repos.push(
                        reshiki::accessibility::button(format!("github-repo-{at}"), label, content)
                            .width(Length::Fill)
                            .padding([3, 8])
                            .style(crate::app::workspace::control(
                                current || (picker.repo.is_none() && at == picker.highlighted),
                            ))
                            .on_press(msg(Action::PickRepo(repo.key.clone()))),
                    );
                }
            }
        }
        let new_repo: Element<'_, Message> = match &picker.new_repo {
            None => reshiki::accessibility::button(
                "github-new-repo-open",
                "New private repository",
                text("New private repository…").size(12),
            )
            .width(Length::Fill)
            .padding([6, 8])
            .style(crate::app::workspace::control(false))
            .on_press(msg(Action::NewRepo))
            .into(),
            Some(name) => row![
                reshiki::accessibility::text_input(NEW_NAME, "Name", "Name", name)
                    .style(crate::appearance::input_style)
                    .on_input(|name| msg(Action::NewRepoName(name)))
                    .on_submit(msg(Action::CreateRepo))
                    .size(12)
                    .padding(6),
                reshiki::accessibility::button(
                    "github-create-repo",
                    "Create",
                    text("Create").size(12)
                )
                .padding([6, 10])
                .style(crate::appearance::primary)
                .on_press_maybe((!picker.busy).then(|| msg(Action::CreateRepo))),
            ]
            .spacing(6)
            .into(),
        };
        let left = column![search, scrollable(repos).height(Length::Fill), new_repo,]
            .spacing(8)
            .width(250);

        let right = self.picker_files(picker);
        let mut footer = row![].spacing(8).align_y(Alignment::Center);
        footer = footer.push(
            text(picker.error.clone().unwrap_or_default())
                .size(12)
                .width(Length::Fill)
                .style(|theme: &iced::Theme| text::Style {
                    color: Some(theme.palette().danger),
                }),
        );
        if picker.mode == Mode::SaveHere {
            footer = footer.push(
                reshiki::accessibility::text_input(SAVE_NAME, "Name", "Name", &picker.save_name)
                    .style(crate::appearance::input_style)
                    .on_input(|name| msg(Action::SaveName(name)))
                    .on_submit(msg(Action::SaveHere))
                    .size(13)
                    .padding(7)
                    .width(220),
            );
        }
        footer = footer.push(
            reshiki::accessibility::button(
                "github-picker-cancel",
                "Cancel",
                text("Cancel").size(13),
            )
            .padding([7, 12])
            .style(crate::appearance::secondary)
            .on_press(Message::GitHub(GitHubAction::Close)),
        );
        footer = footer.push(match picker.mode {
            Mode::Open => {
                reshiki::accessibility::button("github-picker-open", "Open", text("Open").size(13))
                    .padding([7, 12])
                    .style(crate::appearance::primary)
                    .on_press_maybe(
                        (!picker.busy
                            && picker.selected_entry().is_some_and(|entry| !entry.folder))
                        .then(|| msg(Action::Open)),
                    )
            }
            Mode::SaveHere => reshiki::accessibility::button(
                "github-save-here",
                "Save here",
                text("Save here").size(13),
            )
            .padding([7, 12])
            .style(crate::appearance::primary)
            .on_press_maybe((!picker.busy && picker.repo.is_some()).then(|| msg(Action::SaveHere))),
        });
        column![
            text(match picker.mode {
                Mode::Open => "Open from GitHub",
                Mode::SaveHere => "Save to GitHub",
            })
            .size(18),
            row![left, right].spacing(16).height(Length::Fill),
            footer,
        ]
        .spacing(12)
        .into()
    }

    fn picker_files<'a>(&'a self, picker: &'a Picker) -> Element<'a, Message> {
        let Some(repo) = &picker.repo else {
            return container(text("Choose a repository.").size(13).style(muted_text))
                .center(Length::Fill)
                .into();
        };
        let mut crumbs = row![
            reshiki::accessibility::button("github-up", "Up one folder", text("‹").size(16))
                .padding([0, 8])
                .style(crate::app::workspace::control(false))
                .on_press(msg(Action::Up)),
            text(repo.key.repo.clone()).size(13),
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        for (name, _) in &picker.trail {
            crumbs = crumbs.push(text("/").size(13).style(muted_text));
            crumbs = crumbs.push(text(name.as_str()).size(13));
        }
        let body: Element<'_, Message> = match &picker.entries {
            None => container(text("Listing…").size(12).style(muted_text))
                .center(Length::Fill)
                .into(),
            Some(entries) if entries.is_empty() => {
                container(text("No drawings here.").size(13).style(muted_text))
                    .center(Length::Fill)
                    .into()
            }
            Some(entries) => {
                let tiles = entries.iter().enumerate().map(|(at, entry)| {
                    let preview: Element<'_, Message> = if entry.folder {
                        container(text("▸").size(28).style(muted_text))
                            .center(Length::Fill)
                            .into()
                    } else {
                        match picker.thumbs.get(&entry.sha) {
                            Some(Some(doc)) => {
                                crate::canvas::layered::canvas(crate::canvas::DrawingPreview(doc))
                                    .width(Length::Fill)
                                    .height(Length::Fill)
                                    .into()
                            }
                            _ => Space::new().into(),
                        }
                    };
                    let label = entry.name.clone();
                    let action = if entry.folder {
                        Action::Folder(entry.name.clone())
                    } else {
                        Action::Select(at)
                    };
                    reshiki::accessibility::button(
                        format!("github-file-{at}"),
                        label.clone(),
                        column![
                            container(preview).width(Length::Fill).height(84),
                            text(label).size(11).wrapping(text::Wrapping::WordOrGlyph),
                        ]
                        .spacing(4),
                    )
                    .width(128)
                    .padding(6)
                    .style(crate::app::workspace::control(picker.selected == Some(at)))
                    .on_press(msg(action))
                    .into()
                });
                scrollable(iced::widget::Row::with_children(tiles).spacing(8).wrap())
                    .height(Length::Fill)
                    .into()
            }
        };
        column![crumbs, body].spacing(10).width(Length::Fill).into()
    }
}
