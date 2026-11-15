//! The repository picker: repositories on the left, the folders and drawings
//! of the chosen one on the right, newest first. It opens a drawing from
//! GitHub, or chooses where a drawing not on GitHub yet is sent. Folders are
//! listed one at a time as they open, so the size of a repository does not
//! matter. Signed out, it shows the repositories known here, greyed.
use super::{Action as GitHubAction, Dialog, identity, perform};
use crate::app::{App, Message, workspace::muted_text};
use chrono::{DateTime, Local};
use iced::widget::{Space, button, column, container, row, scrollable, text};
use iced::{Alignment, Border, Color, Element, Length, Task, Theme};
use reshiki::document::Document;
use reshiki::github::{
    self as gh, Entry, Repo, RepoKey, api,
    workcopy::{self, FileState, RepoState},
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

const SEARCH: &str = "github-search";
const NEW_NAME: &str = "github-new-repo";
const ITEM_NAME: &str = "github-new-item";
const SAVE_NAME: &str = "github-save-name";
const NAME_TAKEN: &str = "Name taken";
const CANT_OPEN: &str = "Can't open this name here";
/// Two clicks on one tile this close together open it.
const DOUBLE_CLICK: Duration = Duration::from_millis(450);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Open,
    /// Choose where a drawing that is not on GitHub yet goes, then send it.
    SendTo,
}

#[derive(Debug)]
pub(in crate::app) struct Picker {
    mode: Mode,
    /// The last request's number, from one count for every picker, so a
    /// late reply drops, even in a picker opened since.
    pub(super) generation: u64,
    /// The last download or new repository's number, apart from listings,
    /// so neither drops a listing's late replies.
    request: u64,
    search: String,
    /// The last used first, then by name; `None` until GitHub lists them.
    /// Kept in this order while the picker is open.
    pub(super) repos: Option<Vec<Repo>>,
    /// Signed out: the repositories known here, greyed, and Sign in.
    pub(super) signed_out: bool,
    /// The only or last used repository was chosen once on opening.
    chosen: bool,
    /// Where the picker was asked to show (see `ShowIn`): the repository,
    /// once listed, and the folders still to open, outermost first.
    target: Option<RepoKey>,
    goto: Vec<String>,
    /// The highlighted repository in the filtered list.
    highlighted: usize,
    pub(super) repo: Option<Repo>,
    /// The top folder's tree, then each folder opened below it.
    root: Option<String>,
    pub(super) trail: Vec<(String, String)>,
    /// Folders first, then drawings, newest first; `None` while listing.
    pub(super) entries: Option<Vec<Entry>>,
    /// Drawings whose last change is still being asked for; they show
    /// once all are known, so nothing moves under the pointer.
    waiting: usize,
    /// Every name in the folder, previews and other files too.
    names: Vec<String>,
    selected: Option<usize>,
    /// The last click, for a double click.
    clicked: Option<(usize, Instant)>,
    thumbs: HashMap<String, Option<Arc<Document>>>,
    /// Who last changed each drawing, and when, by name.
    changed: HashMap<String, (String, DateTime<Local>)>,
    new_repo: Option<String>,
    /// A new drawing (`false`) or folder (`true`) being named here.
    new_item: Option<(bool, String)>,
    /// The name a drawing is sent under, without `.rsk`.
    save_name: String,
    busy: bool,
    pub(super) error: Option<String>,
}

impl Picker {
    fn new(mode: Mode, save_name: String) -> Self {
        Self {
            mode,
            generation: 0,
            request: 0,
            search: String::new(),
            repos: None,
            signed_out: false,
            chosen: false,
            target: None,
            goto: Vec::new(),
            highlighted: 0,
            repo: None,
            root: None,
            trail: Vec::new(),
            entries: None,
            waiting: 0,
            names: Vec::new(),
            selected: None,
            clicked: None,
            thumbs: HashMap::new(),
            changed: HashMap::new(),
            new_repo: None,
            new_item: None,
            save_name,
            busy: false,
            error: None,
        }
    }

    /// The repositories GitHub listed, the last used first, then by name;
    /// Send offers only those the user may send to.
    pub(super) fn listed(&mut self, repos: &[Repo], last: Option<u64>) {
        let mode = self.mode;
        let mut repos: Vec<Repo> = repos
            .iter()
            .filter(|repo| mode == Mode::Open || repo.push)
            .cloned()
            .collect();
        repos.sort_by_cached_key(|repo| {
            (
                Some(repo.id) != last,
                repo.key.repo.to_lowercase(),
                repo.key.owner.to_lowercase(),
            )
        });
        self.repos = Some(repos);
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

    /// Folders by name, then drawings newest first.
    fn sort(&mut self) {
        let changed = &self.changed;
        if let Some(entries) = &mut self.entries {
            entries.sort_by_cached_key(|entry| {
                let newest = (!entry.folder)
                    .then(|| changed.get(&entry.name).map(|(_, at)| -at.timestamp()))
                    .flatten();
                (!entry.folder, newest.is_none(), newest, entry.name.clone())
            });
        }
    }
}

/// Folders and drawings; previews and other files are left out.
fn shown(entries: Vec<Entry>) -> Vec<Entry> {
    entries
        .into_iter()
        .filter(|entry| entry.folder || entry.name.ends_with(".rsk"))
        .collect()
}

#[derive(Debug, Clone)]
pub enum Action {
    Show(Mode),
    /// Open from GitHub, in this repository's folder (`""` is the top).
    ShowIn(RepoKey, String),
    Search(String),
    Move(isize),
    PickRepo(RepoKey),
    Listing(u64, Result<(String, Vec<Entry>), gh::Error>),
    /// Who last changed a drawing, and when (`None`: unknown).
    Changed(u64, String, Option<(String, String)>),
    Folder(String),
    /// Backspace: up one folder.
    Up,
    /// A folder of the breadcrumb, by depth (0 is the top).
    Crumb(usize),
    Select(usize),
    /// Enter: open what is selected.
    Enter,
    Thumb(String, Option<Arc<Document>>),
    /// Open the selected drawing, or go into the selected folder.
    Open,
    Downloaded(u64, Repo, String, Result<api::Download, gh::Error>),
    NewRepo,
    NewRepoName(String),
    CreateRepo,
    RepoCreated(u64, Result<Repo, gh::Error>),
    SaveName(String),
    SendHere,
    /// `+ New drawing` (`false`) or `+ New folder` (`true`): a name field.
    NewItem(bool),
    NewItemName(String),
    /// Creates the drawing or folder named.
    CreateItem,
    /// Signed out: sign in, then show the picker again.
    SignIn,
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

    /// GitHub listed the repositories again: the open picker shows them,
    /// and chooses the only or the last used one once.
    pub(super) fn picker_listed(&mut self) -> Task<Message> {
        let (repos, last) = (self.github.repos.clone(), self.github.prefs.last);
        let Some(picker) = self.picker() else {
            return Task::none();
        };
        picker.listed(&repos, last);
        self.picker_choose()
    }

    fn picker_choose(&mut self) -> Task<Message> {
        let last = self.github.prefs.last;
        let Some(picker) = self.picker() else {
            return Task::none();
        };
        if picker.chosen || picker.repo.is_some() || picker.signed_out {
            return Task::none();
        }
        let repos = picker.repos.as_deref().unwrap_or_default();
        let chosen = match repos {
            _ if picker.target.is_some() => picker
                .target
                .clone()
                .filter(|key| repos.iter().any(|repo| repo.key == *key)),
            [only] => Some(only.key.clone()),
            _ => repos
                .iter()
                .find(|repo| Some(repo.id) == last)
                .map(|repo| repo.key.clone()),
        };
        picker.chosen = picker.repos.is_some();
        chosen.map_or_else(Task::none, |key| self.picker_action(Action::PickRepo(key)))
    }

    pub(super) fn picker_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Show(mode) => {
                let stem = std::path::Path::new(&self.tab.name())
                    .file_stem()
                    .map_or("Drawing".into(), |stem| stem.to_string_lossy().into_owned());
                return self.picker_show(Picker::new(mode, stem));
            }
            Action::SignIn => {
                let again = match self.picker() {
                    Some(Picker {
                        target: Some(key),
                        goto,
                        ..
                    }) => Action::ShowIn(key.clone(), goto.join("/")),
                    picker => Action::Show(picker.map_or(Mode::Open, |picker| picker.mode)),
                };
                return self.github_sign_in(Some(GitHubAction::Picker(again)));
            }
            Action::ShowIn(key, folder) => {
                // Shown first, then chosen once listed (or signed in).
                let mut picker = Picker::new(Mode::Open, String::new());
                picker.target = Some(key);
                picker.goto = folder
                    .split('/')
                    .filter(|name| !name.is_empty())
                    .map(str::to_owned)
                    .collect();
                return self.picker_show(picker);
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
                if picker.repo.is_some() && picker.search.is_empty() {
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
                let generation = self.github_request();
                let Some(picker) = self.picker() else {
                    return Task::none();
                };
                picker.generation = generation;
                // Another repository than the one asked for ends that walk;
                // a download still running is for the repository left.
                if picker.target.take().as_ref() != Some(&key) {
                    picker.goto.clear();
                }
                picker.request = 0;
                picker.busy = false;
                picker.repo = Some(repo.clone());
                picker.search.clear();
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
                        picker.names = entries.iter().map(|entry| entry.name.clone()).collect();
                        let entries = shown(entries);
                        picker.selected = None;
                        let drawings: Vec<Entry> = entries
                            .iter()
                            .filter(|entry| !entry.folder)
                            .cloned()
                            .collect();
                        let wanted: Vec<String> = drawings
                            .iter()
                            .filter(|entry| !picker.thumbs.contains_key(&entry.sha))
                            .map(|entry| entry.sha.clone())
                            .collect();
                        let paths: Vec<(String, String)> = drawings
                            .iter()
                            .map(|entry| (entry.name.clone(), picker.repo_path(&entry.name)))
                            .collect();
                        picker.waiting = paths.len();
                        // On the way to a folder asked for: open the next one.
                        // One GitHub does not have yet is opened as new.
                        if !picker.goto.is_empty() {
                            let next = picker.goto.remove(0);
                            if let Some(entry) = entries
                                .iter()
                                .find(|entry| entry.folder && entry.name == next)
                            {
                                let (repo, sha) = (picker.repo.clone(), entry.sha.clone());
                                picker.trail.push((next, sha.clone()));
                                picker.entries = Some(entries);
                                return match (self.github.client(), repo) {
                                    (Some(client), Some(repo)) => {
                                        self.picker_list(client, repo, sha)
                                    }
                                    _ => Task::none(),
                                };
                            }
                            picker.trail.push((next, String::new()));
                            let rest = std::mem::take(&mut picker.goto);
                            picker
                                .trail
                                .extend(rest.into_iter().map(|name| (name, String::new())));
                            picker.entries = Some(Vec::new());
                            picker.names.clear();
                            picker.waiting = 0;
                            return Task::none();
                        }
                        picker.entries = Some(entries);
                        picker.sort();
                        let changed = self.picker_changed(generation, paths);
                        return Task::batch([self.picker_thumbs(wanted), changed]);
                    }
                    Err(_) => {
                        picker.entries = Some(Vec::new());
                        picker.waiting = 0;
                        picker.names.clear();
                        picker.error = failed;
                    }
                }
            }
            Action::Changed(generation, name, found) => {
                let Some(picker) = self.picker().filter(|p| p.generation == generation) else {
                    return Task::none();
                };
                if let Some((who, date)) = found
                    && let Ok(at) = DateTime::parse_from_rfc3339(&date)
                {
                    picker.changed.insert(name, (who, at.with_timezone(&Local)));
                }
                picker.waiting = picker.waiting.saturating_sub(1);
                if picker.waiting == 0 {
                    picker.sort();
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
                picker.goto.clear();
                picker.trail.push((name, entry.sha.clone()));
                return self.picker_list(client, repo, entry.sha);
            }
            Action::Up => {
                let depth = self.picker().map_or(0, |picker| picker.trail.len());
                if depth > 0 {
                    return self.picker_action(Action::Crumb(depth - 1));
                }
            }
            Action::Crumb(depth) => {
                let Some(client) = self.github.client() else {
                    return Task::none();
                };
                let Some(picker) = self.picker() else {
                    return Task::none();
                };
                if depth >= picker.trail.len() {
                    return Task::none();
                }
                picker.goto.clear();
                picker.trail.truncate(depth);
                let tree = picker
                    .trail
                    .last()
                    .map(|(_, sha)| sha.clone())
                    .or_else(|| picker.root.clone());
                match (picker.repo.clone(), tree) {
                    // A new folder has nothing on GitHub yet.
                    (Some(_), Some(tree)) if tree.is_empty() => {
                        picker.entries = Some(Vec::new());
                        picker.names.clear();
                        picker.selected = None;
                    }
                    (Some(repo), Some(tree)) => return self.picker_list(client, repo, tree),
                    _ => {}
                }
            }
            Action::Select(at) => {
                let Some(picker) = self.picker() else {
                    return Task::none();
                };
                let now = Instant::now();
                let double = picker
                    .clicked
                    .is_some_and(|(was, when)| was == at && now - when < DOUBLE_CLICK);
                picker.selected = Some(at);
                picker.clicked = (!double).then_some((at, now));
                if double {
                    return self.picker_action(Action::Open);
                }
            }
            Action::Enter => {
                let Some(picker) = self.picker() else {
                    return Task::none();
                };
                if picker.repo.is_none() || !picker.search.is_empty() {
                    let key = picker
                        .shown_repos()
                        .get(picker.highlighted)
                        .map(|repo| repo.key.clone());
                    return key
                        .map_or_else(Task::none, |key| self.picker_action(Action::PickRepo(key)));
                }
                let selected = picker.selected_entry().cloned();
                match (selected, picker.mode) {
                    (Some(_), Mode::Open) => return self.picker_action(Action::Open),
                    (Some(entry), Mode::SendTo) if entry.folder => {
                        return self.picker_action(Action::Folder(entry.name));
                    }
                    (_, Mode::SendTo) => return self.picker_action(Action::SendHere),
                    _ => {}
                }
            }
            Action::Thumb(sha, doc) => {
                if let Some(picker) = self.picker() {
                    picker.thumbs.insert(sha, doc);
                }
            }
            Action::Open => {
                let folder = self
                    .picker()
                    .and_then(|picker| picker.selected_entry())
                    .filter(|entry| entry.folder)
                    .map(|entry| entry.name.clone());
                if let Some(folder) = folder {
                    return self.picker_action(Action::Folder(folder));
                }
                return self.picker_open();
            }
            Action::Downloaded(generation, repo, repo_path, result) => {
                if !self
                    .picker()
                    .is_some_and(|picker| picker.request == generation)
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
                let key = self.github_folder(&repo);
                let state = self.github_tracked(&key, &repo);
                return match workcopy::checkout(&root, &key, state, &repo_path, &download) {
                    Ok(path) => {
                        let persist = self.github_persist(&key);
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
                let generation = self.github_request();
                if let Some(picker) = self.picker() {
                    picker.request = generation;
                }
                return perform(
                    async move { api::create_repo(&client, &name).await },
                    move |result| GitHubAction::Picker(Action::RepoCreated(generation, result)),
                );
            }
            Action::RepoCreated(generation, result) => {
                if !self
                    .picker()
                    .is_some_and(|picker| picker.request == generation)
                {
                    return Task::none();
                }
                match result {
                    Ok(repo) => {
                        let (id, key) = (repo.id, repo.key.clone());
                        self.github.repos.push(repo);
                        let colors = self.github_seen([(id, &key)]);
                        let (repos, last) = (self.github.repos.clone(), self.github.prefs.last);
                        if let Some(picker) = self.picker() {
                            picker.busy = false;
                            picker.new_repo = None;
                            picker.listed(&repos, last);
                        }
                        return Task::batch([colors, self.picker_action(Action::PickRepo(key))]);
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
            Action::SendHere => return self.picker_send_here(),
            Action::NewItem(folder) => {
                if let Some(picker) = self.picker() {
                    picker.new_item = Some((folder, String::new()));
                    picker.error = None;
                    return iced::widget::operation::focus(ITEM_NAME);
                }
            }
            Action::NewItemName(name) => {
                if let Some(picker) = self.picker()
                    && let Some((_, typed)) = &mut picker.new_item
                {
                    *typed = name;
                }
            }
            Action::CreateItem => return self.picker_create_item(),
        }
        Task::none()
    }

    /// Shows `picker`: the repositories GitHub lists or, signed out, those
    /// known here, greyed; then lists them again.
    fn picker_show(&mut self, mut picker: Picker) -> Task<Message> {
        self.github.menu = false;
        if !self.github.configured() {
            return Task::none();
        }
        if self.github.client().is_none() {
            // The repositories known here, greyed, until signed in.
            picker.signed_out = true;
            let known = self
                .github
                .root
                .as_deref()
                .map(workcopy::known)
                .unwrap_or_default();
            let repos: Vec<Repo> = known
                .into_iter()
                .map(|state| Repo {
                    id: state.repo_id,
                    key: state.key(),
                    private: state.private,
                    default_branch: state.branch.clone(),
                    push: true,
                })
                .collect();
            picker.listed(&repos, self.github.prefs.last);
        } else if !self.github.repos.is_empty() {
            picker.listed(&self.github.repos, self.github.prefs.last);
        }
        let focus = match picker.mode {
            Mode::Open => iced::widget::operation::focus(SEARCH),
            Mode::SendTo => iced::widget::operation::focus(SAVE_NAME)
                .chain(iced::widget::operation::select_all(SAVE_NAME)),
        };
        self.github.dialog = Some(Dialog::Picker(Box::new(picker)));
        Task::batch([self.github_list(), focus, self.picker_choose()])
    }

    /// A number for a picker request, never given before in this run.
    fn github_request(&mut self) -> u64 {
        self.github.seq += 1;
        self.github.seq
    }

    /// The folder of `repo` here: the one its drawings first went to, even
    /// after GitHub renamed it.
    fn github_folder(&self, repo: &Repo) -> RepoKey {
        let mut known = self.github.states.iter();
        if let Some((key, _)) = known.find(|(_, state)| state.repo_id == repo.id) {
            return key.clone();
        }
        match &self.github.root {
            Some(root) => workcopy::folder(root, repo),
            None => repo.key.clone(),
        }
    }

    /// The state of `repo`, kept in folder `key`, under GitHub's names now.
    fn github_tracked(&mut self, key: &RepoKey, repo: &Repo) -> &mut RepoState {
        // Read from disk first, if kept there.
        self.github_state(key);
        // Versions asks GitHub again, under these names.
        let versions = &mut self.github.versions;
        if versions.target.as_ref().is_some_and(|(at, _)| at == key)
            && versions.named.as_ref() != Some(&repo.key)
        {
            versions.target = None;
        }
        let state = self
            .github
            .states
            .entry(key.clone())
            .or_insert_with(|| RepoState::new(repo));
        state.owner.clone_from(&repo.key.owner);
        state.repo.clone_from(&repo.key.repo);
        state
    }

    fn picker_list(
        &mut self,
        client: gh::client::Client,
        repo: Repo,
        tree: String,
    ) -> Task<Message> {
        let generation = self.github_request();
        let Some(picker) = self.picker() else {
            return Task::none();
        };
        picker.generation = generation;
        picker.entries = None;
        picker.selected = None;
        picker.new_item = None;
        // A download still running is for the folder left.
        picker.request = 0;
        picker.busy = false;
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

    /// The last commit of each drawing, `(name, repository path)`, for its
    /// tile's second line and the order.
    fn picker_changed(&mut self, generation: u64, paths: Vec<(String, String)>) -> Task<Message> {
        let (Some(client), Some(repo)) = (
            self.github.client(),
            self.picker().and_then(|picker| picker.repo.clone()),
        ) else {
            if let Some(picker) = self.picker() {
                picker.waiting = 0;
            }
            return Task::none();
        };
        Task::batch(paths.into_iter().map(|(name, path)| {
            let (client, repo) = (client.clone(), repo.clone());
            perform(
                async move {
                    let found =
                        api::file_commits(&client, &repo.key, &path, &repo.default_branch, 1)
                            .await
                            .ok()?;
                    let last = found.into_iter().next()?;
                    let who = last.login.map_or(last.author, |login| format!("@{login}"));
                    Some((who, last.date))
                },
                move |found| GitHubAction::Picker(Action::Changed(generation, name, found)),
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
        let key = self.github_folder(&repo);
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
        // An open tab, or a working copy that differs from what was last
        // synced (or with no record of that), opens as it is: never
        // downloaded over.
        let open = self.strip().any(|tab| tab.path.as_ref() == Some(&path));
        let base = self
            .github_state(&key)
            .and_then(|state| state.files.get(&repo_path))
            .map(|file| file.base_hash.clone());
        let unsent = std::fs::read(&path).is_ok_and(|bytes| base != Some(workcopy::hash(&bytes)));
        if open || unsent {
            // A renamed repository: Versions asks GitHub under its names now.
            self.github_tracked(&key, &repo);
            let persist = self.github_persist(&key);
            let close = self.github_close_dialog();
            if unsent && !open {
                self.status = "Opened your unsent version".into();
            }
            return Task::batch([persist, close, crate::app::files::open_paths(vec![path])]);
        }
        let generation = self.github_request();
        let Some(picker) = self.picker() else {
            return Task::none();
        };
        picker.busy = true;
        picker.error = None;
        picker.request = generation;
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
    fn picker_send_here(&mut self) -> Task<Message> {
        let Some(root) = self.github.root.clone() else {
            return Task::none();
        };
        let Some(picker) = self.picker() else {
            return Task::none();
        };
        // The folder's names are known once it is listed.
        let (Some(repo), Some(_)) = (picker.repo.clone(), picker.entries.as_ref()) else {
            return Task::none();
        };
        let typed = picker.save_name.trim();
        let stem = typed
            .strip_suffix(".rsk")
            .unwrap_or(typed)
            .trim()
            .to_owned();
        // A name, not a path: the folder is the one open in the picker.
        let stem = if stem.contains(['/', '\\']) {
            String::new()
        } else {
            stem
        };
        let name = format!("{stem}.rsk");
        let preview = format!("{stem}.svg");
        let taken = picker
            .names
            .iter()
            .any(|taken| *taken == name || *taken == preview);
        let repo_path = picker.repo_path(&name);
        let key = self.github_folder(&repo);
        let collides = self.github_state(&key).is_some_and(|state| {
            state.collides(&repo_path) || state.files.contains_key(&repo_path)
        });
        let refusal = if !workcopy::valid_repo_path(&repo_path) || stem.is_empty() {
            Some(CANT_OPEN)
        } else if self.file_io.saving {
            Some("Wait for the save to finish")
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
        let path = workcopy::working_path(&root, &key, &repo_path);
        if let Some(folder) = path.parent()
            && let Err(error) = std::fs::create_dir_all(folder)
        {
            if let Some(picker) = self.picker() {
                picker.error = Some(error.to_string());
            }
            return Task::none();
        }
        self.github_tracked(&key, &repo)
            .files
            .insert(repo_path.clone(), FileState::default());
        let persist = self.github_persist(&key);
        let close = self.github_close_dialog();
        self.github.prefs.last = Some(repo.id);
        let colors = self.github_keep_prefs();
        self.tab.path = Some(path);
        self.tab.untitled_name = None;
        let message = reshiki::github::send::default_message(&[repo_path]);
        self.github.after_save = Some((key, message, vec![self.tab.id]));
        let save = crate::app::tagged(self.save_drawing(false), self.tab.id);
        Task::batch([persist, close, colors, save])
    }

    /// A new folder opens at once and exists only here until a drawing in it
    /// is sent. A new drawing is an empty working copy with nothing on
    /// GitHub yet, opened in a tab: Cmd+S saves it, Send makes it a new file.
    fn picker_create_item(&mut self) -> Task<Message> {
        let Some(root) = self.github.root.clone() else {
            return Task::none();
        };
        let Some(picker) = self.picker() else {
            return Task::none();
        };
        let (Some(repo), Some((folder, typed))) = (picker.repo.clone(), picker.new_item.clone())
        else {
            return Task::none();
        };
        let typed = typed.trim();
        // A name, not a path: the folder is the one open in the picker.
        let typed = if typed.contains(['/', '\\']) {
            ""
        } else {
            typed
        };
        if folder {
            if typed.is_empty() || !workcopy::valid_repo_path(&picker.repo_path(typed)) {
                picker.error = Some(CANT_OPEN.into());
                return Task::none();
            }
            if picker.names.iter().any(|name| name == typed) {
                picker.error = Some(NAME_TAKEN.into());
                return Task::none();
            }
            picker.trail.push((typed.to_owned(), String::new()));
            picker.new_item = None;
            picker.entries = Some(Vec::new());
            picker.names.clear();
            picker.selected = None;
            picker.error = None;
            return Task::none();
        }
        let stem = typed
            .strip_suffix(".rsk")
            .unwrap_or(typed)
            .trim()
            .to_owned();
        let (name, preview) = (format!("{stem}.rsk"), format!("{stem}.svg"));
        let taken = picker
            .names
            .iter()
            .any(|taken| *taken == name || *taken == preview);
        let repo_path = picker.repo_path(&name);
        let key = self.github_folder(&repo);
        let path = workcopy::working_path(&root, &key, &repo_path);
        let collides = path.exists()
            || path.with_extension("svg").exists()
            || self.github_state(&key).is_some_and(|state| {
                state.collides(&repo_path) || state.files.contains_key(&repo_path)
            });
        let refusal = if stem.is_empty() || !workcopy::valid_repo_path(&repo_path) {
            Some(CANT_OPEN)
        } else if taken || collides {
            Some(NAME_TAKEN)
        } else {
            None
        };
        let written = match refusal {
            Some(refusal) => Err(refusal.to_owned()),
            None => Document::default().file_json().and_then(|bytes| {
                let folder = path.parent().unwrap_or(&root);
                std::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
                reshiki::storage::write_atomic(&path, &bytes)
            }),
        };
        if let Err(error) = written {
            if let Some(picker) = self.picker() {
                picker.error = Some(error);
            }
            return Task::none();
        }
        self.github_tracked(&key, &repo)
            .files
            .insert(repo_path, FileState::default());
        let persist = self.github_persist(&key);
        let close = self.github_close_dialog();
        Task::batch([persist, close, crate::app::files::open_paths(vec![path])])
    }

    /// One repository row: its tinted tile, its name, what is unsent or in
    /// conflict as a dot and a count, and a lock when it is private.
    fn picker_repo_row<'a>(
        &'a self,
        picker: &'a Picker,
        at: usize,
        repo: &'a Repo,
        shared_name: bool,
    ) -> Element<'a, Message> {
        let folder = self
            .github
            .states
            .iter()
            .find(|(_, state)| state.repo_id == repo.id)
            .map_or_else(|| repo.key.clone(), |(key, _)| key.clone());
        let unsent = self.github.unsent.get(&folder).map_or(0, Vec::len);
        let conflicts = self.github_conflicts(&folder);
        let hue = identity::hue(&self.github.prefs, repo.id, &repo.key);
        let mut spoken = repo.key.repo.clone();
        let mut name = row![].align_y(Alignment::Center);
        if shared_name {
            spoken = format!("{}/{}", repo.key.owner, repo.key.repo);
            name = name.push(
                text(format!("{}/", repo.key.owner))
                    .size(12)
                    .style(muted_text),
            );
        }
        name = name.push(text(repo.key.repo.clone()).size(12));
        let mut content = row![
            identity::tile(hue, identity::monogram(&repo.key.repo)),
            name
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .push(Space::new().width(Length::Fill));
        if conflicts > 0 {
            spoken.push_str(&format!(", {conflicts} changed on GitHub"));
            content = content.push(state_dot(true, conflicts));
        } else if unsent > 0 {
            spoken.push_str(&format!(", {unsent} unsent"));
            content = content.push(state_dot(false, unsent));
        }
        if repo.private {
            spoken.push_str(", private");
            content = content.push(lock());
        }
        let current = picker.repo.as_ref().map(|r| &r.key) == Some(&repo.key);
        let highlighted = !picker.search.is_empty() && at == picker.highlighted;
        reshiki::accessibility::button(format!("github-repo-{at}"), spoken, content)
            .width(Length::Fill)
            .padding([4, 6])
            .style(crate::app::workspace::control(current || highlighted))
            .on_press_maybe((!picker.signed_out).then(|| msg(Action::PickRepo(repo.key.clone()))))
            .into()
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
                let shown = picker.shown_repos();
                for (at, repo) in shown.iter().enumerate() {
                    let shared = shown
                        .iter()
                        .any(|other| other.id != repo.id && other.key.repo == repo.key.repo);
                    repos = repos.push(self.picker_repo_row(picker, at, repo, shared));
                }
                if !picker.signed_out {
                    repos = repos.push(new_repo_row(picker));
                }
            }
        }
        let left = column![search, scrollable(repos).height(Length::Fill)]
            .spacing(8)
            .width(250);

        let right = self.picker_files(picker);
        let mut footer = row![].spacing(8).align_y(Alignment::Center);
        if picker.mode == Mode::SendTo {
            footer = footer.push(
                row![
                    text("Name").size(13),
                    reshiki::accessibility::text_input(
                        SAVE_NAME,
                        "Name",
                        "Name",
                        &picker.save_name
                    )
                    .style(crate::appearance::input_style)
                    .on_input(|name| msg(Action::SaveName(name)))
                    .on_submit(msg(Action::SendHere))
                    .size(13)
                    .padding(7)
                    .width(180),
                    text(".rsk").size(13).style(muted_text),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            );
        }
        footer = footer.push(
            text(match picker.mode {
                Mode::Open => "Enter Open · Backspace Up · Esc Close",
                Mode::SendTo => "Enter Send · Backspace Up · Esc Close",
            })
            .size(11)
            .style(muted_text),
        );
        footer = footer.push(Space::new().width(Length::Fill));
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
        let (id, label, action, ready) = match picker.mode {
            Mode::Open => (
                "github-picker-open",
                "Open",
                Action::Open,
                picker.selected_entry().is_some(),
            ),
            Mode::SendTo => (
                "github-send-here",
                "Send",
                Action::SendHere,
                picker.repo.is_some() && picker.entries.is_some(),
            ),
        };
        footer = footer.push(
            reshiki::accessibility::button(id, label, text(label).size(13))
                .padding([7, 12])
                .style(crate::appearance::primary)
                .on_press_maybe((!picker.busy && ready).then(|| msg(action))),
        );
        let error = picker.error.clone().map(|error| {
            text(error)
                .size(12)
                .style(|theme: &iced::Theme| text::Style {
                    color: Some(theme.palette().danger),
                })
        });
        column![
            text(match picker.mode {
                Mode::Open => "Open from GitHub",
                Mode::SendTo => "Send to GitHub",
            })
            .size(18),
            row![left, right].spacing(16).height(Length::Fill),
        ]
        .push(error)
        .push(footer)
        .spacing(12)
        .into()
    }

    fn picker_files<'a>(&'a self, picker: &'a Picker) -> Element<'a, Message> {
        let center = |content: Element<'a, Message>| container(content).center(Length::Fill).into();
        if picker.signed_out {
            return center(
                reshiki::accessibility::button(
                    "github-picker-sign-in",
                    "Sign in…",
                    text("Sign in…").size(13),
                )
                .padding([8, 16])
                .style(crate::appearance::primary)
                .on_press(msg(Action::SignIn))
                .into(),
            );
        }
        if picker.repos.as_ref().is_some_and(Vec::is_empty) {
            return center(
                column![
                    text("No repositories yet").size(14),
                    reshiki::accessibility::button(
                        "github-choose-repos",
                        "Choose repositories ↗",
                        text("Choose repositories ↗").size(13),
                    )
                    .padding([7, 12])
                    .style(crate::appearance::secondary)
                    .on_press(Message::GitHub(GitHubAction::ChooseRepositories)),
                ]
                .spacing(12)
                .align_x(Alignment::Center)
                .into(),
            );
        }
        let Some(repo) = &picker.repo else {
            return center(
                text("Choose a repository.")
                    .size(13)
                    .style(muted_text)
                    .into(),
            );
        };
        // `lab-notes / scheme`: the parents are muted and go back there.
        let mut crumbs = row![].spacing(6).align_y(Alignment::Center);
        let names = std::iter::once(repo.key.repo.as_str())
            .chain(picker.trail.iter().map(|(name, _)| name.as_str()));
        let depth = picker.trail.len();
        for (at, name) in names.enumerate() {
            if at > 0 {
                crumbs = crumbs.push(text("/").size(13).style(muted_text));
            }
            crumbs = crumbs.push(if at < depth {
                Element::from(
                    reshiki::accessibility::button(
                        format!("github-crumb-{at}"),
                        format!("Back to {name}"),
                        text(name).size(13).style(muted_text),
                    )
                    .padding(0)
                    .style(button::text)
                    .on_press(msg(Action::Crumb(at))),
                )
            } else {
                text(name).size(13).into()
            });
        }
        // Unsent drawings here, for the dots on folders and drawings.
        let folder_key = self
            .github
            .states
            .iter()
            .find(|(_, state)| state.repo_id == repo.id)
            .map_or_else(|| repo.key.clone(), |(key, _)| key.clone());
        let unsent = self.github.unsent.get(&folder_key);
        let conflicted: Vec<&String> = self
            .github
            .states
            .get(&folder_key)
            .map(|state| {
                state
                    .files
                    .iter()
                    .filter(|(_, file)| file.conflict.is_some())
                    .map(|(path, _)| path)
                    .collect()
            })
            .unwrap_or_default();
        let under = |paths: &mut dyn Iterator<Item = &String>, path: &str, folder: bool| {
            paths
                .filter(|other| {
                    if folder {
                        other.starts_with(&format!("{path}/"))
                    } else {
                        *other == path
                    }
                })
                .count()
        };
        let unsent_under = |path: &str, folder: bool| {
            unsent.map_or(0, |unsent| under(&mut unsent.iter(), path, folder))
        };
        let conflicts_under =
            |path: &str, folder: bool| under(&mut conflicted.iter().copied(), path, folder);
        let body: Element<'_, Message> = match &picker.entries {
            None => center(text("Listing…").size(12).style(muted_text).into()),
            Some(entries) => {
                let tiles = entries
                    .iter()
                    .enumerate()
                    // Drawings show once their last changes are known.
                    .filter(|(_, entry)| entry.folder || picker.waiting == 0)
                    .map(|(at, entry)| {
                        let selected = picker.selected == Some(at);
                        let path = picker.repo_path(&entry.name);
                        let unsent = unsent_under(&path, entry.folder);
                        let conflicts = conflicts_under(&path, entry.folder);
                        let (preview, label, meta): (Element<'_, Message>, &str, Option<String>) =
                            if entry.folder {
                                (
                                    iced::widget::canvas(crate::app::icons::Glyph(
                                        crate::app::icons::Icon::Open,
                                        true,
                                    ))
                                    .width(30)
                                    .height(30)
                                    .into(),
                                    entry.name.as_str(),
                                    None,
                                )
                            } else {
                                let preview = match picker.thumbs.get(&entry.sha) {
                                    Some(Some(doc)) => crate::canvas::layered::canvas(
                                        crate::canvas::DrawingPreview(doc),
                                    )
                                    .width(Length::Fill)
                                    .height(Length::Fill)
                                    .into(),
                                    _ => Space::new().into(),
                                };
                                let meta = picker.changed.get(&entry.name).map(|(who, at)| {
                                    format!("{who} · {}", super::versions::tile_time(*at))
                                });
                                (
                                    preview,
                                    entry.name.strip_suffix(".rsk").unwrap_or(&entry.name),
                                    meta,
                                )
                            };
                        let mut spoken = if entry.folder {
                            format!("{label}, folder")
                        } else {
                            label.to_owned()
                        };
                        if let Some(meta) = &meta {
                            spoken = format!("{spoken}, {meta}");
                        }
                        if conflicts > 0 {
                            spoken = format!("{spoken}, {conflicts} changed on GitHub");
                        } else if unsent > 0 {
                            spoken = format!("{spoken}, {unsent} unsent");
                        }
                        let mut name = row![
                            text(label.to_owned())
                                .size(11)
                                .wrapping(text::Wrapping::WordOrGlyph)
                                .width(Length::Fill)
                        ]
                        .spacing(4)
                        .align_y(Alignment::Center);
                        if conflicts > 0 || unsent > 0 {
                            name = name.push(state_dot(conflicts > 0, 0));
                        }
                        let content = column![
                            container(preview)
                                .center_x(Length::Fill)
                                .center_y(84)
                                .style(thumb_frame),
                            name,
                        ]
                        .push(meta.map(|meta| {
                            text(meta)
                                .size(10)
                                .style(muted_text)
                                .wrapping(text::Wrapping::None)
                        }))
                        .spacing(4);
                        reshiki::accessibility::button(format!("github-file-{at}"), spoken, content)
                            .width(128)
                            .padding(6)
                            .style(crate::app::workspace::control(selected))
                            .on_press(msg(Action::Select(at)))
                            .into()
                    });
                let creating: Vec<Element<'_, Message>> = match picker.mode {
                    Mode::Open => vec![new_item_tile(picker, false), new_item_tile(picker, true)],
                    Mode::SendTo => vec![new_item_tile(picker, true)],
                };
                let tiles = iced::widget::Row::with_children(creating.into_iter().chain(tiles))
                    .spacing(8)
                    .wrap();
                let mut body = column![tiles].spacing(12);
                if entries.is_empty() {
                    body = body.push(text("Empty").size(13).style(muted_text));
                }
                scrollable(body).height(Length::Fill).into()
            }
        };
        column![crumbs, body].spacing(10).width(Length::Fill).into()
    }
}

/// `+ New repository` as the last repository row; clicked, a name field
/// with the lock that says it will be private. Enter creates it.
fn new_repo_row(picker: &Picker) -> Element<'_, Message> {
    match &picker.new_repo {
        None => reshiki::accessibility::button(
            "github-new-repo-open",
            "New repository",
            row![
                container(text("+").size(14)).center_x(22),
                text("New repository").size(12)
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding([4, 6])
        .style(crate::app::workspace::control(false))
        .on_press(msg(Action::NewRepo))
        .into(),
        Some(name) => row![
            reshiki::accessibility::text_input(
                NEW_NAME,
                "New private repository name",
                "Repository name",
                name
            )
            .style(crate::appearance::input_style)
            .on_input(|name| msg(Action::NewRepoName(name)))
            .on_submit_maybe((!picker.busy).then(|| msg(Action::CreateRepo)))
            .size(12)
            .padding(6),
            lock(),
        ]
        .spacing(4)
        .align_y(Alignment::Center)
        .into(),
    }
}

/// `+ New drawing` or `+ New folder` as the first tiles; clicked, a name
/// field in place, `.rsk` fixed after a drawing's name. Enter creates it.
fn new_item_tile(picker: &Picker, folder: bool) -> Element<'_, Message> {
    let label = if folder { "New folder" } else { "New drawing" };
    let content: Element<'_, Message> = match &picker.new_item {
        Some((naming, name)) if *naming == folder => {
            let field = reshiki::accessibility::text_input(
                ITEM_NAME,
                format!("{label} name"),
                "Name",
                name,
            )
            .style(crate::appearance::input_style)
            .on_input(|name| msg(Action::NewItemName(name)))
            .on_submit(msg(Action::CreateItem))
            .size(11)
            .padding(5);
            let field: Element<'_, Message> = if folder {
                field.into()
            } else {
                row![field, text(".rsk").size(11).style(muted_text)]
                    .spacing(3)
                    .align_y(Alignment::Center)
                    .into()
            };
            return container(
                column![
                    container(text("+").size(22).style(muted_text))
                        .center_x(Length::Fill)
                        .center_y(84)
                        .style(thumb_frame),
                    field,
                ]
                .spacing(4),
            )
            .width(128)
            .padding(6)
            .into();
        }
        _ => column![
            container(text("+").size(22).style(muted_text))
                .center_x(Length::Fill)
                .center_y(84)
                .style(thumb_frame),
            text(label).size(11),
        ]
        .spacing(4)
        .into(),
    };
    reshiki::accessibility::button(
        if folder {
            "github-new-folder"
        } else {
            "github-new-drawing"
        },
        label,
        content,
    )
    .width(128)
    .padding(6)
    .style(crate::app::workspace::control(false))
    .on_press(msg(Action::NewItem(folder)))
    .into()
}

/// A small trailing lock: the repository is private.
fn lock<'a>() -> Element<'a, Message> {
    iced::widget::canvas(crate::app::icons::Glyph(
        crate::app::icons::Icon::Lock(true),
        true,
    ))
    .width(16)
    .height(16)
    .into()
}

/// What is left to do, as a dot and a count: amber for drawings changed on
/// GitHub, the accent for unsent ones. A count of 0 shows the dot alone.
fn state_dot<'a>(conflict: bool, count: usize) -> Element<'a, Message> {
    let dot = identity::dot(move |theme: &Theme| {
        if conflict {
            theme.palette().warning
        } else {
            theme.palette().primary
        }
    });
    if count == 0 {
        // A conflict is also a "!", so color is not the only sign.
        return match conflict {
            true => row![
                dot,
                text("!").size(11).style(|theme: &Theme| text::Style {
                    color: Some(theme.palette().warning),
                })
            ]
            .spacing(2)
            .align_y(Alignment::Center)
            .into(),
            false => dot,
        };
    }
    row![dot, text(count.to_string()).size(11).style(muted_text)]
        .spacing(4)
        .align_y(Alignment::Center)
        .into()
}

/// The frame of a tile's drawing or folder icon.
fn thumb_frame(theme: &Theme) -> container::Style {
    crate::appearance::container(
        theme,
        container::Style {
            border: Border {
                color: Color::from_rgb8(226, 230, 234),
                width: 1.,
                radius: 6.0.into(),
            },
            ..container::Style::default()
        },
    )
}
