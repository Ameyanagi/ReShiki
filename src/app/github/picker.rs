//! The repository picker: repositories on the left, the folders, drawings
//! and other chemistry files of the chosen one on the right, as tiles newest
//! first or as a tree. It opens a drawing from GitHub, inserts a file's
//! structures into the drawing in front, or chooses where a drawing not on
//! GitHub yet is sent. Files other than drawings open read only. Tiles list
//! one folder at a time as it opens, at most [`SHOWN`]; the tree reads the
//! whole repository once. Who changed a file and its preview are asked for
//! only once it is on screen, so the size of a repository does not matter. A
//! github.com link in the search field shows one row for what it names;
//! repositories opened by link stay listed, tagged "shared". Signed out,
//! public repositories list and open (names only); the others known here are
//! greyed.
use super::{Action as GitHubAction, Dialog, identity, perform, versions::who};
use crate::app::{
    App, Message,
    document_tab::TabId,
    import::{self, Batch, Ticket},
    workspace::muted_text,
};
use chrono::{DateTime, Local};
use iced::widget::{Space, button, column, container, row, scrollable, text};
use iced::{Alignment, Border, Color, Element, Length, Task, Theme};
use reshiki::document::Document;
use reshiki::github::{
    self as gh, Entry, Repo, RepoKey, api,
    link::{self, Link},
    workcopy::{self, FileState, RepoState},
};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

mod tree;

const SEARCH: &str = "github-search";
const NEW_NAME: &str = "github-new-repo";
const ITEM_NAME: &str = "github-new-item";
const SAVE_NAME: &str = "github-save-name";
const ADD_NAME: &str = "github-add-repo";
pub(super) const LIMITED: &str =
    "GitHub's limit for signed-out use is reached. Sign in to continue.";
const SIGNED_OUT_HINT: &str = "Sign in to see who changed each file and a preview.";
/// A tile's width and spacing, and about its height with its two lines:
/// enough to tell which tiles are on screen.
const TILE: f32 = 136.;
const TILE_ROW: f32 = 150.;
/// Every tile is this tall, so rows are [`TILE_ROW`] apart.
const TILE_HEIGHT: f32 = TILE_ROW - 8.;
const NAME_TAKEN: &str = "Name taken";
const CANT_OPEN: &str = "Can't open this name here";
/// Two clicks on one tile this close together open it.
const DOUBLE_CLICK: Duration = Duration::from_millis(450);
/// The most tiles a folder shows; the search finds the others.
const SHOWN: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Open,
    /// Choose where a drawing that is not on GitHub yet goes, then send it.
    SendTo,
}

#[derive(Debug)]
pub(in crate::app) struct Picker {
    pub(super) mode: Mode,
    /// The last request's number, from one count for every picker, so a
    /// late reply drops, even in a picker opened since.
    pub(super) generation: u64,
    /// The last download or new repository's number, apart from listings,
    /// so neither drops a listing's late replies.
    request: u64,
    pub(super) search: String,
    /// The last used first, then by name; `None` until GitHub lists them.
    /// Kept in this order while the picker is open.
    pub(super) repos: Option<Vec<Repo>>,
    /// Signed out: the repositories known here, greyed, and Sign in.
    pub(super) signed_out: bool,
    /// The only or last used repository was chosen once on opening.
    chosen: bool,
    /// Choose repositories was opened in the browser from here: the
    /// repositories are listed again once the window is back in front.
    pub(super) browsing: bool,
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
    pub(super) waiting: usize,
    /// Every name in the folder, previews and other files too.
    names: Vec<String>,
    /// The folder could not be listed, so its names are not known.
    unlisted: bool,
    pub(super) selected: Option<usize>,
    /// The last click, by repository path, for a double click.
    clicked: Option<(String, Instant)>,
    pub(super) thumbs: HashMap<String, Option<Arc<Document>>>,
    /// Who last changed each drawing of this repository, and when, by
    /// repository path.
    changed: HashMap<String, (String, DateTime<Local>)>,
    new_repo: Option<String>,
    /// A new drawing (`false`) or folder (`true`) being named here.
    pub(super) new_item: Option<(bool, String)>,
    /// The name a drawing is sent under, without `.rsk`.
    pub(super) save_name: String,
    busy: bool,
    pub(super) error: Option<String>,
    /// A link in the search field: one row for what it names.
    pub(super) link: Option<Link>,
    /// The branch or commit the files are listed at; `None`: the default
    /// branch. Drawings open from it as old versions.
    pub(super) reference: Option<String>,
    /// The repository a link named that GitHub would not show.
    pub(super) cant_open: Option<RepoKey>,
    /// Signed out, GitHub's hourly limit is spent.
    pub(super) limited: bool,
    /// `+ Add public repository…` being typed.
    adding: Option<String>,
    /// The shared row whose Remove from list is open, by repository id.
    pub(super) menu: Option<u64>,
    /// The files pane's scroll offset, height and width, for the tiles on
    /// screen; `None` before the first scroll.
    view: Option<(f32, f32, f32)>,
    /// Drawings whose author, time and preview were asked for.
    pub(super) asked: HashSet<String>,
    /// The repository paths of the first tiles on screen, whose dates the
    /// one sort waits for.
    first: HashSet<String>,
    /// Save a copy: This computer is offered too.
    pub(super) copy: bool,
    /// The tree, not tiles; Open only.
    pub(super) tree: bool,
    /// Every folder and file of the repository by repository path, read once
    /// when it opens, and whether GitHub listed only part of it; `None` while
    /// it is read.
    pub(super) whole: Option<(Vec<Entry>, bool)>,
    /// The request that opened the repository, for its whole listing.
    opening: u64,
    /// The folders open in the tree, by repository path.
    pub(super) opened: HashSet<String>,
    /// The tree's selected row, by repository path.
    pub(super) row: Option<String>,
    /// The tree's scroll offset and height; `None` before the first scroll.
    tree_view: Option<(f32, f32)>,
    /// The file whose menu is open, by repository path.
    pub(super) file_menu: Option<String>,
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
            browsing: false,
            target: None,
            goto: Vec::new(),
            highlighted: 0,
            repo: None,
            root: None,
            trail: Vec::new(),
            entries: None,
            waiting: 0,
            names: Vec::new(),
            unlisted: false,
            selected: None,
            clicked: None,
            thumbs: HashMap::new(),
            changed: HashMap::new(),
            new_repo: None,
            new_item: None,
            save_name,
            busy: false,
            error: None,
            link: None,
            reference: None,
            cant_open: None,
            limited: false,
            adding: None,
            menu: None,
            view: None,
            asked: HashSet::new(),
            first: HashSet::new(),
            copy: false,
            tree: false,
            whole: None,
            opening: 0,
            opened: HashSet::new(),
            row: None,
            tree_view: None,
            file_menu: None,
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
        // The repository open here follows the new listing, as its access may.
        if let Some(open) = &mut self.repo
            && let Some(now) = repos.iter().find(|repo| repo.id == open.id)
        {
            *open = now.clone();
        }
        self.repos = Some(repos);
    }

    /// A drawing or folder may be made here: the folder (in the tree, the
    /// whole repository) was listed, so its names are known, the user may
    /// send to the repository, and it is the default branch, not an old
    /// version.
    pub(super) fn can_create(&self) -> bool {
        let listed = match &self.whole {
            _ if !self.tree => !self.unlisted,
            Some((_, truncated)) => !truncated,
            None => false,
        };
        listed && self.reference.is_none() && self.repo.as_ref().is_some_and(|repo| repo.push)
    }

    /// → and ← go to the tree: no text in the search field, and no other
    /// field open.
    pub(super) fn tree_keys(&self) -> bool {
        self.tree
            && self.repo.is_some()
            && self.search.is_empty()
            && self.new_item.is_none()
            && self.new_repo.is_none()
            && self.adding.is_none()
    }

    /// The search, lowercase; a link is no search.
    fn query(&self) -> String {
        match self.link {
            Some(_) => String::new(),
            None => self.search.trim().to_lowercase(),
        }
    }

    /// With a repository open, the search filters its files instead.
    fn shown_repos(&self) -> Vec<&Repo> {
        let search = match self.repo {
            Some(_) => String::new(),
            None => self.search.to_lowercase(),
        };
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
        join(&self.folder(), name)
    }

    /// How many `+ New` tiles come before the entries: none in a folder
    /// that could not be listed, a read-only repository or an old version.
    fn creating(&self) -> usize {
        match self.mode {
            _ if !self.can_create() => 0,
            Mode::Open => 2,
            Mode::SendTo => 1,
        }
    }

    /// A drawing's tile shows once every drawing's last change is known.
    fn visible(&self, entry: &Entry) -> bool {
        entry.folder || self.waiting == 0
    }

    /// What is selected, if its tile shows.
    fn selected_entry(&self) -> Option<&Entry> {
        let entry = self.entries.as_ref()?.get(self.selected?)?;
        (self.visible(entry) && entry.name.to_lowercase().contains(&self.query())).then_some(entry)
    }

    /// The tiles that show, by entry: those the search matches, at most
    /// [`SHOWN`]; and how many it matches.
    pub(super) fn tiles(&self) -> (Vec<usize>, usize) {
        let query = self.query();
        let matching: Vec<usize> = (self.entries.iter().flatten().enumerate())
            .filter(|(_, entry)| self.visible(entry) && entry.name.to_lowercase().contains(&query))
            .map(|(at, _)| at)
            .collect();
        let count = matching.len();
        (matching.into_iter().take(SHOWN).collect(), count)
    }

    /// The file selected in either view: its repository path and blob.
    fn selected_file(&self) -> Option<(String, String)> {
        if self.tree {
            let path = self.row.as_ref()?;
            let (entries, _) = self.whole.as_ref()?;
            let entry = entries.iter().find(|e| !e.folder && e.name == *path)?;
            return Some((path.clone(), entry.sha.clone()));
        }
        let entry = self.selected_entry().filter(|entry| !entry.folder)?;
        Some((self.repo_path(&entry.name), entry.sha.clone()))
    }

    /// Where `+ New` makes a drawing or folder, and the names there: the
    /// open folder, or in the tree the selected folder or the selected file's.
    fn place(&self) -> (String, Vec<String>) {
        let Some((entries, _)) = self.whole.as_ref().filter(|_| self.tree) else {
            return (self.folder(), self.names.clone());
        };
        let folder = match &self.row {
            Some(path) if entries.iter().any(|e| e.folder && e.name == *path) => path.clone(),
            Some(path) => parent(path).to_owned(),
            None => String::new(),
        };
        let names = (entries.iter())
            .filter(|entry| parent(&entry.name) == folder)
            .map(|entry| last(&entry.name).to_owned())
            .collect();
        (folder, names)
    }

    /// Folders by name, then drawings newest first. The selection stays on
    /// the same tile.
    fn sort(&mut self) {
        let selected = self.selected_entry().map(|entry| entry.name.clone());
        let Some(mut entries) = self.entries.take() else {
            return;
        };
        entries.sort_by_cached_key(|entry| {
            let newest = (!entry.folder)
                .then(|| self.changed.get(&self.repo_path(&entry.name)))
                .flatten()
                .map(|(_, at)| -at.timestamp());
            (!entry.folder, newest.is_none(), newest, entry.name.clone())
        });
        self.selected =
            selected.and_then(|name| entries.iter().position(|entry| entry.name == name));
        self.clicked = None;
        self.entries = Some(entries);
    }
}

/// The folder a repository path is in (`""` is the top).
fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(folder, _)| folder)
}

/// The last name of a repository path.
fn last(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// `name` in `folder`, as a repository path.
fn join(folder: &str, name: &str) -> String {
    match folder {
        "" => name.to_owned(),
        folder => format!("{folder}/{name}"),
    }
}

/// The folders of `path`, outermost first.
fn folders(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect()
}

fn same(a: &RepoKey, b: &RepoKey) -> bool {
    a.owner.eq_ignore_ascii_case(&b.owner) && a.repo.eq_ignore_ascii_case(&b.repo)
}

/// The entries whose tiles are on screen, by index: those of the rows in
/// view, after `before` tiles that are not entries.
fn on_screen(view: Option<(f32, f32, f32)>, before: usize, len: usize) -> std::ops::Range<usize> {
    let (top, height, width) = view.unwrap_or((0., 420., 480.));
    let columns = (((width + 8.) / TILE) as usize).max(1);
    let first = (top / TILE_ROW) as usize * columns;
    // One row more, so a tile half in view has what it shows.
    let last = ((top + height) / TILE_ROW) as usize * columns + 2 * columns;
    first.saturating_sub(before).min(len)..last.saturating_sub(before).min(len)
}

/// Folders, drawings and other chemistry files; previews are left out.
fn shown(entries: Vec<Entry>) -> Vec<Entry> {
    entries
        .into_iter()
        .filter(|entry| entry.folder || gh::listed(&entry.name))
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
    /// Who last changed the drawing at a repository path, and when
    /// (`None`: unknown).
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
    /// The drawing at a repository path, and whether it is an old version.
    Downloaded(u64, Repo, String, bool, Result<api::Download, gh::Error>),
    /// A github.com link from the window, the palette or Open latest: shown
    /// in the search field and opened at once.
    Link(String),
    /// GitHub's answer for the repository a link names.
    Resolved(u64, Link, Result<Repo, gh::Error>),
    /// `+ Add public repository…`: a field for `owner/repo` or a link.
    Add,
    AddName(String),
    AddRepo,
    /// Right-click on a shared row, by repository id; `None` closes.
    RowMenu(Option<u64>),
    /// Remove from list: a shared repository, by id.
    Forget(u64),
    /// The files pane scrolled: its offset, height and width.
    Scrolled(f32, f32, f32),
    /// Save a copy on this computer instead.
    ThisComputer,
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
    /// Tiles (`false`) or Tree, the view every picker opens in from now on.
    View(bool),
    /// The repository read whole, by the request that opened it.
    Whole(u64, Result<(Vec<Entry>, bool), gh::Error>),
    /// A tree row clicked, by repository path: a folder opens or closes.
    Row(String),
    /// → (`true`) or ← in the tree.
    Expand(bool),
    /// The tree scrolled: its offset and height.
    TreeScrolled(f32, f32),
    /// Right-click on a file, by repository path; `None` closes its menu.
    FileMenu(Option<String>),
    /// Puts the selected file's structures into the drawing in front.
    Insert,
    /// The file to insert, by repository path, for the drawing it was
    /// chosen for.
    Fetched(TabId, Ticket, String, Result<Vec<u8>, gh::Error>),
    /// Another chemistry file than a drawing, read for its read-only tab.
    Read(PathBuf, Box<Result<Batch, String>>),
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
        let (repos, last) = (self.github_picker_repos(), self.github.prefs.last);
        let Some(picker) = self.picker() else {
            return Task::none();
        };
        picker.listed(&repos, last);
        self.picker_choose()
    }

    /// The window is in front again. After Choose repositories in the
    /// browser, the open picker lists them again, once.
    pub(super) fn picker_returned(&mut self) -> Task<Message> {
        match self.picker() {
            Some(picker) if picker.browsing => {
                picker.browsing = false;
                // The only repository chosen there is chosen here too.
                picker.chosen = false;
                self.github_list()
            }
            _ => Task::none(),
        }
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
                let found = link::parse(&search, &self.github.ends.web);
                if let Some(picker) = self.picker() {
                    picker.search = search;
                    picker.link = found;
                    picker.highlighted = 0;
                    picker.cant_open = None;
                    picker.limited = false;
                    picker.error = None;
                    picker.file_menu = None;
                    // A row the search hides is not selected.
                    let hidden = !(picker.rows().iter())
                        .any(|row| Some(&row.entry.name) == picker.row.as_ref());
                    if hidden {
                        picker.row = None;
                    }
                }
                return self.picker_load();
            }
            Action::Move(step) => {
                let Some(picker) = self.picker() else {
                    return Task::none();
                };
                let step_in = |at: Option<usize>, len: usize| {
                    at.map_or(0, |at| {
                        (at as isize + step).clamp(0, len.saturating_sub(1) as isize) as usize
                    })
                };
                if picker.repo.is_some() && picker.tree {
                    let rows = picker.rows();
                    let at = rows
                        .iter()
                        .position(|row| Some(&row.entry.name) == picker.row.as_ref());
                    let row = rows
                        .get(step_in(at, rows.len()))
                        .map(|row| row.entry.name.clone());
                    if row.is_some() {
                        picker.row = row;
                    }
                    let reveal = picker.reveal_row();
                    return Task::batch([reveal, self.picker_load()]);
                } else if picker.repo.is_some() {
                    let (tiles, _) = picker.tiles();
                    let at = picker
                        .selected
                        .and_then(|selected| tiles.iter().position(|at| *at == selected));
                    picker.selected = tiles.get(step_in(at, tiles.len())).copied();
                } else {
                    let len = picker.shown_repos().len();
                    picker.highlighted = step_in(Some(picker.highlighted), len);
                }
            }
            Action::PickRepo(key) => {
                let listed = self.picker().and_then(|picker| {
                    let mut repos = picker.repos.iter().flatten();
                    repos.find(|repo| repo.key == key).cloned()
                });
                let Some(repo) = listed.or_else(|| {
                    let mut repos = self.github.repos.iter();
                    repos.find(|repo| repo.key == key).cloned()
                }) else {
                    return Task::none();
                };
                let Some(client) = self.github.reader() else {
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
                    picker.reference = None;
                }
                picker.menu = None;
                picker.link = None;
                picker.cant_open = None;
                picker.limited = false;
                picker.asked.clear();
                picker.view = None;
                let reference = picker.reference.clone();
                picker.request = 0;
                picker.busy = false;
                picker.changed.clear();
                picker.repo = Some(repo.clone());
                picker.search.clear();
                picker.root = None;
                picker.trail.clear();
                picker.entries = None;
                picker.selected = None;
                picker.error = None;
                picker.whole = None;
                // A whole listing still on its way is for the repository left.
                picker.opening = generation;
                picker.opened.clear();
                picker.row = None;
                picker.tree_view = None;
                picker.file_menu = None;
                return perform(
                    async move {
                        let head = match &reference {
                            Some(reference) => api::commit(&client, &repo.key, reference).await,
                            None => api::head(&client, &repo.key, &repo.default_branch).await,
                        };
                        match head {
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
                // The top folder: the repository is read whole too, once.
                let top = match (&result, self.picker()) {
                    (Ok((tree, _)), Some(picker))
                        if picker.generation == generation && picker.root.is_none() =>
                    {
                        Some(tree.clone())
                    }
                    _ => None,
                };
                let whole = top.map_or_else(Task::none, |tree| self.picker_whole(generation, tree));
                let failed = match &result {
                    Err(error) => Some(self.picker_failed(error)),
                    Ok(_) => None,
                };
                let Some(picker) = self.picker().filter(|p| p.generation == generation) else {
                    return Task::none();
                };
                picker.unlisted = result.is_err();
                match result {
                    Ok((tree, entries)) => {
                        if picker.root.is_none() {
                            picker.root = Some(tree);
                        }
                        picker.names = entries.iter().map(|entry| entry.name.clone()).collect();
                        let entries = shown(entries);
                        picker.selected = None;
                        picker.asked.clear();
                        picker.view = None;
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
                                let next = match (self.github.reader(), repo) {
                                    (Some(client), Some(repo)) => {
                                        self.picker_list(client, repo, sha)
                                    }
                                    _ => Task::none(),
                                };
                                return Task::batch([whole, next]);
                            }
                            picker.trail.push((next, String::new()));
                            let rest = std::mem::take(&mut picker.goto);
                            picker
                                .trail
                                .extend(rest.into_iter().map(|name| (name, String::new())));
                            picker.entries = Some(Vec::new());
                            picker.names.clear();
                            picker.waiting = 0;
                            return whole;
                        }
                        // The tree opens where the tiles are.
                        let mut open = String::new();
                        for name in folders(&picker.folder()) {
                            open = join(&open, &name);
                            picker.opened.insert(open.clone());
                        }
                        picker.entries = Some(entries);
                        picker.sort();
                        // The first tiles on screen show once their last
                        // changes are known, then sort newest first, once.
                        let load = self.picker_load();
                        if let Some(picker) = self.picker() {
                            picker.first = picker.asked.clone();
                            picker.waiting = picker.first.len();
                        }
                        return Task::batch([whole, load]);
                    }
                    Err(_) => {
                        picker.entries = Some(Vec::new());
                        picker.waiting = 0;
                        picker.names.clear();
                        picker.limited = failed == Some(None);
                        picker.error = failed.flatten();
                    }
                }
            }
            Action::Changed(generation, path, found) => {
                let Some(picker) = self.picker().filter(|p| p.generation == generation) else {
                    return Task::none();
                };
                // Tiles scrolled into view later never move the others, nor
                // end the wait for the first ones.
                let first = picker.first.remove(&path);
                if let Some((who, date)) = found
                    && let Ok(at) = DateTime::parse_from_rfc3339(&date)
                {
                    picker.changed.insert(path, (who, at.with_timezone(&Local)));
                }
                if picker.waiting > 0 && first {
                    picker.waiting -= 1;
                    if picker.waiting == 0 {
                        picker.sort();
                    }
                }
            }
            Action::Folder(name) => {
                let Some(client) = self.github.reader() else {
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
                let depth = (self.picker())
                    .filter(|picker| !picker.tree)
                    .map_or(0, |picker| picker.trail.len());
                if depth > 0 {
                    return self.picker_action(Action::Crumb(depth - 1));
                }
            }
            Action::Crumb(depth) => {
                let Some(client) = self.github.reader() else {
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
                let Some(name) = picker.entries.iter().flatten().nth(at) else {
                    return Task::none();
                };
                let path = picker.repo_path(&name.name);
                let now = Instant::now();
                let double = (picker.clicked.as_ref())
                    .is_some_and(|(was, when)| *was == path && now - *when < DOUBLE_CLICK);
                picker.selected = Some(at);
                picker.file_menu = None;
                picker.clicked = (!double).then_some((path, now));
                if double {
                    return self.picker_action(Action::Open);
                }
            }
            Action::Enter => {
                let Some(picker) = self.picker() else {
                    return Task::none();
                };
                if let Some(link) = picker.link.clone() {
                    return self.picker_resolve(link);
                }
                if picker.repo.is_none() {
                    let key = picker
                        .shown_repos()
                        .get(picker.highlighted)
                        .map(|repo| repo.key.clone());
                    return key
                        .map_or_else(Task::none, |key| self.picker_action(Action::PickRepo(key)));
                }
                if picker.tree {
                    let folder = (picker.row.clone()).filter(|_| picker.selected_file().is_none());
                    return match folder {
                        Some(row) => self.picker_action(Action::Row(row)),
                        None => self.picker_open(),
                    };
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
            // In the tree, a folder opens or closes, as Enter does.
            Action::Open if self.picker().is_some_and(|picker| picker.tree) => {
                return self.picker_action(Action::Enter);
            }
            Action::Open => {
                let folder = (self.picker())
                    .and_then(|picker| picker.selected_entry())
                    .filter(|entry| entry.folder)
                    .map(|entry| entry.name.clone());
                if let Some(folder) = folder {
                    return self.picker_action(Action::Folder(folder));
                }
                return self.picker_open();
            }
            Action::Downloaded(generation, repo, repo_path, old, result) => {
                if !self
                    .picker()
                    .is_some_and(|picker| picker.request == generation)
                {
                    return Task::none();
                }
                let download = match result {
                    Ok(download) => download,
                    Err(error) => {
                        let error = self.picker_failed(&error);
                        if let Some(picker) = self.picker() {
                            picker.busy = false;
                            picker.limited = error.is_none();
                            picker.error = error;
                        }
                        return Task::none();
                    }
                };
                let root = self.github.root.clone().unwrap_or_default();
                let key = self.github_folder(&repo);
                let state = self.github_tracked(&key, &repo);
                let written = if old {
                    // Kept apart from the working copy, and never sent.
                    let path = workcopy::old_path(&root, &key, &download.commit, &repo_path);
                    path.parent()
                        .map_or(Ok(()), |folder| {
                            std::fs::create_dir_all(folder).map_err(|error| error.to_string())
                        })
                        .and_then(|()| reshiki::storage::write_saved(&path, &download.bytes))
                        .map(|()| path)
                } else {
                    workcopy::checkout(&root, &key, state, &repo_path, &download)
                };
                return match written {
                    // Another chemistry file is read as Insert reads it.
                    Ok(path) if !repo_path.ends_with(".rsk") => {
                        let persist = self.github_persist(&key);
                        let close = self.github_close_dialog();
                        let file = crate::app::files::Picked {
                            path: path.clone(),
                            contents: download.bytes,
                        };
                        let theme = Document::default().canvas_theme;
                        let read = import::load(
                            self.engine.clone(),
                            vec![file],
                            Default::default(),
                            theme,
                        );
                        let read = Task::perform(read, move |result| {
                            msg(Action::Read(path.clone(), Box::new(result)))
                        });
                        Task::batch([persist, close, read])
                    }
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
                        let (repos, last) = (self.github_picker_repos(), self.github.prefs.last);
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
                if let Some(picker) = self.picker().filter(|picker| picker.can_create()) {
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
            Action::Link(text) => {
                let Some(found) = link::parse(&text, &self.github.ends.web) else {
                    return Task::none();
                };
                let open = self
                    .picker()
                    .is_some_and(|picker| picker.mode == Mode::Open);
                let show = if open {
                    Task::none()
                } else {
                    self.picker_show(Picker::new(Mode::Open, String::new()))
                };
                if let Some(picker) = self.picker() {
                    picker.search = text;
                    picker.link = Some(found.clone());
                    picker.chosen = true;
                }
                return Task::batch([show, self.picker_resolve(found)]);
            }
            Action::Resolved(generation, found, result) => {
                if !self
                    .picker()
                    .is_some_and(|picker| picker.request == generation)
                {
                    return Task::none();
                }
                match result {
                    Ok(repo) => {
                        let mut tasks = vec![self.github_seen([(repo.id, &repo.key)])];
                        // Someone else's repository stays listed, tagged shared.
                        if !self.github.repos.iter().any(|one| one.id == repo.id)
                            && self.github.prefs.share(&repo)
                        {
                            tasks.push(self.github_keep_prefs());
                        }
                        let (repos, last) = (self.github_picker_repos(), self.github.prefs.last);
                        if let Some(picker) = self.picker() {
                            picker.busy = false;
                            picker.listed(&repos, last);
                        }
                        tasks.push(self.picker_follow(repo, found));
                        return Task::batch(tasks);
                    }
                    Err(error) => {
                        let error = match error {
                            // Private, or no such repository: GitHub says the same.
                            gh::Error::NotFound => Err(found.key),
                            error => Ok(self.picker_failed(&error)),
                        };
                        if let Some(picker) = self.picker() {
                            picker.busy = false;
                            picker.link = None;
                            match error {
                                Err(key) => picker.cant_open = Some(key),
                                Ok(None) => picker.limited = true,
                                Ok(error) => picker.error = error,
                            }
                        }
                    }
                }
            }
            Action::Add => {
                if let Some(picker) = self.picker() {
                    picker.adding = Some(String::new());
                    return iced::widget::operation::focus(ADD_NAME);
                }
            }
            Action::AddName(name) => {
                if let Some(picker) = self.picker() {
                    picker.adding = Some(name);
                }
            }
            Action::AddRepo => {
                let web = self.github.ends.web.clone();
                let Some(picker) = self.picker() else {
                    return Task::none();
                };
                let typed = picker.adding.clone().unwrap_or_default();
                let Some(found) = link::parse_repo(&typed, &web) else {
                    picker.error = Some("Type owner/repo or a GitHub link".into());
                    return Task::none();
                };
                picker.adding = None;
                picker.error = None;
                return self.picker_resolve(found);
            }
            Action::RowMenu(id) => {
                if let Some(picker) = self.picker() {
                    picker.menu = id;
                }
            }
            Action::Forget(id) => {
                self.github.prefs.forget(id);
                let (repos, last) = (self.github_picker_repos(), self.github.prefs.last);
                if let Some(picker) = self.picker() {
                    picker.menu = None;
                    if picker.repo.as_ref().is_some_and(|repo| repo.id == id) {
                        picker.repo = None;
                        picker.entries = None;
                    }
                    picker.listed(&repos, last);
                }
                return self.github_keep_prefs();
            }
            Action::Scrolled(top, height, width) => {
                if let Some(picker) = self.picker() {
                    picker.view = Some((top, height, width));
                }
                return self.picker_load();
            }
            Action::ThisComputer => {
                let close = self.github_close_dialog();
                return Task::batch([close, self.save_as()]);
            }
            Action::View(tree) => {
                let Some(picker) = self.picker().filter(|picker| picker.mode == Mode::Open) else {
                    return Task::none();
                };
                picker.tree = tree;
                picker.file_menu = None;
                // The tile selected is the row selected.
                if tree && let Some(entry) = picker.selected_entry() {
                    picker.row = Some(picker.repo_path(&entry.name));
                }
                self.github.prefs.tree = tree;
                return Task::batch([self.github_keep_prefs(), self.picker_load()]);
            }
            Action::Whole(opening, result) => {
                let failed = result.as_ref().err().map(|error| self.picker_failed(error));
                let Some(picker) = self.picker().filter(|picker| picker.opening == opening) else {
                    return Task::none();
                };
                // Not listed is as good as listed in part: nothing new is
                // made where the names are not known.
                let (mut entries, truncated) = result.unwrap_or((Vec::new(), true));
                // A listing in part may leave out a folder its files are in.
                let mut known: HashSet<String> = (entries.iter())
                    .filter(|entry| entry.folder)
                    .map(|entry| entry.name.clone())
                    .collect();
                for path in entries
                    .iter()
                    .map(|entry| entry.name.clone())
                    .collect::<Vec<_>>()
                {
                    let mut folder = parent(&path);
                    while !folder.is_empty() && known.insert(folder.to_owned()) {
                        entries.push(Entry {
                            name: folder.to_owned(),
                            sha: String::new(),
                            folder: true,
                        });
                        folder = parent(folder);
                    }
                }
                picker.whole = Some((entries, truncated));
                if let Some(failed) = failed {
                    picker.limited = failed.is_none();
                    picker.error = failed;
                }
                return self.picker_load();
            }
            Action::Row(path) => {
                let Some(picker) = self.picker() else {
                    return Task::none();
                };
                let now = Instant::now();
                let double = (picker.clicked.as_ref())
                    .is_some_and(|(was, when)| *was == path && now - *when < DOUBLE_CLICK);
                picker.row = Some(path.clone());
                picker.file_menu = None;
                // A name left half typed is let go: the arrows are the tree's.
                picker.new_item = None;
                picker.new_repo = None;
                picker.adding = None;
                picker.clicked = (!double).then(|| (path.clone(), now));
                if picker.selected_file().is_none() {
                    if !picker.opened.remove(&path) {
                        picker.opened.insert(path);
                    }
                } else if double {
                    return self.picker_open();
                }
                return self.picker_load();
            }
            Action::Expand(open) => {
                let Some(picker) = self.picker() else {
                    return Task::none();
                };
                let rows = picker.rows();
                let Some((at, row)) = (rows.iter().enumerate())
                    .find(|(_, row)| Some(&row.entry.name) == picker.row.as_ref())
                else {
                    return Task::none();
                };
                let path = row.entry.name.clone();
                let child = (rows.get(at + 1))
                    .filter(|next| next.depth > row.depth)
                    .map(|next| next.entry.name.clone());
                match (row.entry.folder, row.open, open) {
                    (true, false, true) => {
                        picker.opened.insert(path);
                    }
                    (true, true, true) => picker.row = child.or(Some(path)),
                    (true, true, false) => {
                        picker.opened.remove(&path);
                    }
                    (_, _, false) if !parent(&path).is_empty() => {
                        picker.row = Some(parent(&path).to_owned());
                    }
                    _ => {}
                }
                let reveal = picker.reveal_row();
                return Task::batch([reveal, self.picker_load()]);
            }
            Action::TreeScrolled(top, height) => {
                if let Some(picker) = self.picker() {
                    picker.tree_view = Some((top, height));
                }
                return self.picker_load();
            }
            Action::FileMenu(path) => {
                let Some(picker) = self.picker() else {
                    return Task::none();
                };
                // The file right-clicked is selected too.
                if let Some(path) = &path {
                    if picker.tree {
                        picker.row = Some(path.clone());
                    } else {
                        let folder = picker.folder();
                        picker.selected = (picker.entries.iter().flatten())
                            .position(|entry| join(&folder, &entry.name) == *path);
                    }
                }
                picker.file_menu = path;
            }
            Action::Insert => return self.picker_insert(),
            Action::Fetched(tab, ticket, path, result) => {
                return match result {
                    Ok(contents) => {
                        let path = PathBuf::from(last(&path));
                        let file = crate::app::files::Picked { path, contents };
                        let read = self.in_tab(tab, |app| app.insert_read(ticket, vec![file]));
                        crate::app::tagged(read.unwrap_or_else(Task::none), tab)
                    }
                    Err(error) => {
                        self.status = self.github_failed(&error);
                        self.error = true;
                        Task::none()
                    }
                };
            }
            Action::Read(path, result) => {
                let (mut drawings, warnings) = match *result {
                    Ok(batch) => (batch.drawings.into_iter(), batch.warnings),
                    Err(error) => {
                        self.status = error;
                        self.error = true;
                        return Task::none();
                    }
                };
                let Some(mut doc) = drawings.next() else {
                    return Task::none();
                };
                // An SD file's other records, beside the first.
                let bounds = |doc: &Document| reshiki::scene::selection_bounds(doc, &doc.all_ids());
                for part in drawings {
                    if let (Some((top, right)), Some((lo, hi))) = (bounds(&doc), bounds(&part)) {
                        let gap = doc.drawing_style.bond_length_world;
                        let offset = reshiki::document::Point::new(
                            right.x + gap - lo.x,
                            (top.y + right.y - lo.y - hi.y) / 2.,
                        );
                        reshiki::editing::append(&mut doc, &part, offset);
                    }
                }
                let kept = self.github_opened(&path);
                if self.open_target(Some(&path)) {
                    self.show_opened(doc, Some(path));
                    for warning in warnings {
                        self.status.push_str(" · ");
                        self.status.push_str(&warning);
                    }
                }
                return kept;
            }
        }
        Task::none()
    }

    /// Puts the selected file's structures into the drawing in front, placed
    /// as an insert is, as one Undo step. Only Open inserts.
    fn picker_insert(&mut self) -> Task<Message> {
        let Some(picker) = self.picker().filter(|picker| picker.mode == Mode::Open) else {
            return Task::none();
        };
        let (Some(repo), Some((path, sha))) = (picker.repo.clone(), picker.selected_file()) else {
            return Task::none();
        };
        let Some(client) = self.github.reader() else {
            return Task::none();
        };
        let (tab, ticket) = (self.tab.id, self.import_ticket());
        let close = self.github_close_dialog();
        self.status = "Reading from GitHub…".into();
        self.error = false;
        let read = perform(
            async move { api::blob(&client, &repo.key, &sha).await },
            move |result| GitHubAction::Picker(Action::Fetched(tab, ticket, path, result)),
        );
        Task::batch([close, read])
    }

    /// An error from GitHub for the picker; `None`: signed out, the hourly
    /// limit is spent, which [`LIMITED`] explains.
    fn picker_failed(&mut self, error: &gh::Error) -> Option<String> {
        if matches!(error, gh::Error::RateLimited(_)) && self.github.client().is_none() {
            return None;
        }
        Some(self.github_failed(error))
    }

    /// The repositories the picker lists: GitHub's (or, signed out, those
    /// known here), then those opened by link.
    pub(super) fn github_picker_repos(&self) -> Vec<Repo> {
        let mut repos: Vec<Repo> = if self.github.client().is_some() {
            self.github.repos.clone()
        } else {
            // Someone else's repository is listed only while shared, so
            // Remove from list removes it.
            let known = self.github.root.as_deref().map(workcopy::known);
            known
                .into_iter()
                .flatten()
                .filter(|(_, state)| !state.read_only)
                .map(|(_, state)| Repo {
                    id: state.repo_id,
                    key: state.key(),
                    private: state.private,
                    default_branch: state.branch.clone(),
                    push: !state.read_only,
                })
                .collect()
        };
        for shared in &self.github.prefs.shared {
            let shared = Repo::from(shared);
            if !repos.iter().any(|repo| repo.id == shared.id) {
                repos.push(shared);
            }
        }
        repos
    }

    /// The repository a link names: listed here already, or asked of GitHub.
    fn picker_resolve(&mut self, found: Link) -> Task<Message> {
        let listed = self.picker().and_then(|picker| {
            let mut repos = picker.repos.iter().flatten();
            repos.find(|repo| same(&repo.key, &found.key)).cloned()
        });
        if let Some(repo) = listed {
            return self.picker_follow(repo, found);
        }
        let Some(client) = self.github.reader() else {
            return Task::none();
        };
        let generation = self.github_request();
        let Some(picker) = self.picker() else {
            return Task::none();
        };
        // Listings still on their way, and a walk to a folder, end here,
        // with the repository they were for.
        picker.generation = generation;
        picker.request = generation;
        picker.goto.clear();
        picker.repo = None;
        picker.entries = None;
        picker.waiting = 0;
        picker.busy = true;
        picker.cant_open = None;
        picker.limited = false;
        picker.error = None;
        let key = found.key.clone();
        perform(
            async move { api::repo_named(&client, &key).await },
            move |result| GitHubAction::Picker(Action::Resolved(generation, found, result)),
        )
    }

    /// Opens what a link names in `repo`: a drawing in a tab (an old version
    /// at another branch or a commit), or else the picker in its folder.
    fn picker_follow(&mut self, repo: Repo, found: Link) -> Task<Message> {
        // Send goes to the default branch: a link there is read at it.
        let send = self
            .picker()
            .is_some_and(|picker| picker.mode == Mode::SendTo);
        let reference = found
            .reference
            .filter(|reference| *reference != repo.default_branch && !send);
        if found.file && gh::listed(&found.path) {
            return self.picker_open_path(repo, found.path, reference);
        }
        // A link to another kind of file shows its folder.
        let folder = match found.path.rsplit_once('/') {
            _ if !found.file => found.path.as_str(),
            Some((folder, _)) => folder,
            None => "",
        };
        let goto = folders(folder);
        let Some(picker) = self.picker() else {
            return Task::none();
        };
        picker.target = Some(repo.key.clone());
        picker.goto = goto;
        picker.reference = reference;
        picker.repo = None;
        self.picker_action(Action::PickRepo(repo.key))
    }

    /// Asks who last changed the files whose tiles or rows are on screen,
    /// and for their previews (in the tree, the selected file's), once each.
    /// Signed out, tiles and rows show names only.
    fn picker_load(&mut self) -> Task<Message> {
        if self.github.client().is_none() {
            return Task::none();
        }
        let Some(picker) = self.picker() else {
            return Task::none();
        };
        // By repository path, with their blobs.
        let (shown, previews) = if picker.tree {
            let rows = picker.rows();
            let range = tree::on_screen(picker.tree_view, rows.len());
            let shown: Vec<(String, String)> = (rows.get(range).unwrap_or_default().iter())
                .filter(|row| !row.entry.folder)
                .map(|row| (row.entry.name.clone(), row.entry.sha.clone()))
                .collect();
            (shown, picker.selected_file().into_iter().collect())
        } else {
            let Some(entries) = &picker.entries else {
                return Task::none();
            };
            let (tiles, _) = picker.tiles();
            let range = on_screen(picker.view, picker.creating(), tiles.len());
            let shown: Vec<(String, String)> = (tiles.get(range).unwrap_or_default().iter())
                .filter_map(|at| entries.get(*at))
                .filter(|entry| !entry.folder)
                .map(|entry| (picker.repo_path(&entry.name), entry.sha.clone()))
                .collect();
            (shown.clone(), shown)
        };
        let paths: Vec<String> = (shown.into_iter())
            .map(|(path, _)| path)
            .filter(|path| !picker.asked.contains(path))
            .collect();
        let previews: Vec<(String, String)> = (previews.into_iter())
            .filter(|(_, sha)| !picker.thumbs.contains_key(sha))
            .collect();
        picker.asked.extend(paths.iter().cloned());
        // Asked for once, even before it arrives.
        for (_, sha) in &previews {
            picker.thumbs.insert(sha.clone(), None);
        }
        let generation = picker.generation;
        let changed = match paths.is_empty() {
            true => Task::none(),
            false => self.picker_changed(generation, paths),
        };
        Task::batch([self.picker_thumbs(previews), changed])
    }

    /// Reads the repository whole, for the tree, once it opens at `tree`.
    fn picker_whole(&mut self, opening: u64, tree: String) -> Task<Message> {
        let (Some(client), Some(picker)) = (self.github.reader(), self.picker()) else {
            return Task::none();
        };
        let Some(repo) = picker.repo.clone() else {
            return Task::none();
        };
        picker.opening = opening;
        // An empty repository has nothing to list.
        if tree.is_empty() {
            picker.whole = Some((Vec::new(), false));
            return Task::none();
        }
        perform(
            async move { api::tree_all(&client, &repo.key, &tree).await },
            move |result| GitHubAction::Picker(Action::Whole(opening, result)),
        )
    }

    /// Shows `picker`: the repositories GitHub lists or, signed out, those
    /// known here, greyed; then lists them again.
    fn picker_show(&mut self, mut picker: Picker) -> Task<Message> {
        self.github.menu = false;
        if !self.github.configured() {
            return Task::none();
        }
        if self.github.client().is_none() {
            // The repositories known here; the private ones greyed.
            picker.signed_out = true;
            picker.listed(&self.github_picker_repos(), self.github.prefs.last);
        } else if !self.github.repos.is_empty() || !self.github.prefs.shared.is_empty() {
            picker.listed(&self.github_picker_repos(), self.github.prefs.last);
        }
        // Tiles the very first time, then the view used last.
        picker.tree = picker.mode == Mode::Open && self.github.prefs.tree;
        let focus = match picker.mode {
            Mode::Open => iced::widget::operation::focus(SEARCH),
            Mode::SendTo => iced::widget::operation::focus(SAVE_NAME)
                .chain(iced::widget::operation::select_all(SAVE_NAME)),
        };
        self.github.dialog = Some(Dialog::Picker(Box::new(picker)));
        Task::batch([self.github_list(), focus, self.picker_choose()])
    }

    /// Saved credentials signed in after the picker opened signed out: it
    /// lists the repositories now.
    pub(super) fn picker_signed_in(&mut self) -> Task<Message> {
        let Some(picker) = self.picker().filter(|picker| picker.signed_out) else {
            return Task::none();
        };
        picker.signed_out = false;
        picker.repos = None;
        self.github_list()
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
        // Access only ever widens here: a repository read before access was
        // given (a download on its way, a picker listed earlier) never makes
        // it read only again. A new state takes the repository's own.
        if repo.push {
            state.read_only = false;
        }
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

    /// Each file's tile or preview shows the file itself, read like any
    /// opened file (an SD file's first record); no preview file from the
    /// repository is parsed. By repository path and blob.
    fn picker_thumbs(&mut self, files: Vec<(String, String)>) -> Task<Message> {
        let (Some(client), Some(repo)) = (
            self.github.client(),
            self.picker().and_then(|picker| picker.repo.clone()),
        ) else {
            return Task::none();
        };
        let theme = Document::default().canvas_theme;
        Task::batch(files.into_iter().map(|(path, sha)| {
            let (client, key, engine) = (client.clone(), repo.key.clone(), self.engine.clone());
            let blob = sha.clone();
            perform(
                async move {
                    let contents = api::blob(&client, &key, &blob).await.ok()?;
                    let file = crate::app::files::Picked {
                        path: PathBuf::from(last(&path)),
                        contents,
                    };
                    // An SD file's first record only.
                    let first = import::records(file).into_iter().take(1).collect();
                    let read = import::load(engine, first, Default::default(), theme);
                    let first = read.await.ok()?.drawings.into_iter().next()?;
                    Some(Arc::new(first))
                },
                move |doc| GitHubAction::Picker(Action::Thumb(sha, doc)),
            )
        }))
    }

    /// The last commit of each drawing, by repository path, for its tile's
    /// second line and the order.
    fn picker_changed(&mut self, generation: u64, paths: Vec<String>) -> Task<Message> {
        let (Some(client), Some(repo)) = (
            self.github.client(),
            self.picker().and_then(|picker| picker.repo.clone()),
        ) else {
            if let Some(picker) = self.picker() {
                picker.waiting = 0;
            }
            return Task::none();
        };
        let me = self.github.login().map(str::to_owned);
        // At a branch or commit, its own history.
        let head = self
            .picker()
            .and_then(|picker| picker.reference.clone())
            .unwrap_or_else(|| repo.default_branch.clone());
        Task::batch(paths.into_iter().map(|path| {
            let (client, repo, me, head) = (client.clone(), repo.clone(), me.clone(), head.clone());
            let asked = path.clone();
            perform(
                async move {
                    let found = api::file_commits(&client, &repo.key, &path, &head, 1)
                        .await
                        .ok()?;
                    let last = found.into_iter().next()?;
                    let who = who(last.login.as_deref(), &last.author, me.as_deref());
                    Some((who, last.date))
                },
                move |found| GitHubAction::Picker(Action::Changed(generation, asked, found)),
            )
        }))
    }

    fn picker_open(&mut self) -> Task<Message> {
        let Some(picker) = self.picker() else {
            return Task::none();
        };
        let (Some(repo), Some((repo_path, _))) = (picker.repo.clone(), picker.selected_file())
        else {
            return Task::none();
        };
        let reference = picker.reference.clone();
        self.picker_open_path(repo, repo_path, reference)
    }

    /// Opens the drawing at `repo_path`; at `reference`, an old version.
    /// Another chemistry file opens read only, kept as old versions are.
    fn picker_open_path(
        &mut self,
        repo: Repo,
        repo_path: String,
        reference: Option<String>,
    ) -> Task<Message> {
        let Some(client) = self.github.reader() else {
            return Task::none();
        };
        let Some(root) = self.github.root.clone() else {
            return Task::none();
        };
        let key = self.github_folder(&repo);
        let drawing = repo_path.ends_with(".rsk");
        let collides = self
            .github_state(&key)
            .is_some_and(|state| state.collides(&repo_path));
        if !workcopy::valid_repo_path(&repo_path) || collides && reference.is_none() && drawing {
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
        if reference.is_none() && drawing && (open || unsent) {
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
        picker.goto.clear();
        let old = reference.is_some() || !drawing;
        perform(
            {
                let (repo, repo_path) = (repo.clone(), repo_path.clone());
                async move {
                    match reference {
                        Some(reference) => {
                            api::download_at(&client, &repo.key, &reference, &repo_path).await
                        }
                        None => {
                            let branch = &repo.default_branch;
                            api::download(&client, &repo.key, branch, &repo_path).await
                        }
                    }
                }
            },
            move |result| {
                GitHubAction::Picker(Action::Downloaded(generation, repo, repo_path, old, result))
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
        // The folder's names are known once it is listed; after a listing
        // that failed, a save could replace a file already there. Send goes
        // to the default branch, so names read at another one do not count.
        let (Some(repo), Some(_), false, None) = (
            picker.repo.clone(),
            picker.entries.as_ref(),
            picker.unlisted,
            &picker.reference,
        ) else {
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
        let save = crate::app::tagged(self.save_drawing(None), self.tab.id);
        Task::batch([persist, close, colors, save])
    }

    /// A new folder opens at once and exists only here until a drawing in it
    /// is sent. A new drawing is an empty working copy with nothing on
    /// GitHub yet, opened in a tab: Cmd+S saves it, Send makes it a new file.
    /// In the tree, both go to the selected folder (or the selected file's).
    fn picker_create_item(&mut self) -> Task<Message> {
        let Some(root) = self.github.root.clone() else {
            return Task::none();
        };
        let Some(picker) = self.picker() else {
            return Task::none();
        };
        let (Some(repo), Some((folder, typed)), true) = (
            picker.repo.clone(),
            picker.new_item.clone(),
            picker.can_create(),
        ) else {
            return Task::none();
        };
        let typed = typed.trim();
        // A name, not a path: the folder is the one open in the picker.
        let typed = if typed.contains(['/', '\\']) {
            ""
        } else {
            typed
        };
        let (place, names) = picker.place();
        if folder {
            let path = join(&place, typed);
            if typed.is_empty() || !workcopy::valid_repo_path(&path) {
                picker.error = Some(CANT_OPEN.into());
                return Task::none();
            }
            if names.iter().any(|name| name == typed) {
                picker.error = Some(NAME_TAKEN.into());
                return Task::none();
            }
            if let Some((entries, _)) = picker.whole.as_mut().filter(|_| picker.tree) {
                entries.push(Entry {
                    name: path.clone(),
                    sha: String::new(),
                    folder: true,
                });
                picker.opened.insert(path.clone());
                picker.row = Some(path);
                picker.new_item = None;
                picker.error = None;
                return Task::none();
            }
            picker.trail.push((typed.to_owned(), String::new()));
            picker.new_item = None;
            // A download still running is for the folder left.
            picker.request = 0;
            picker.busy = false;
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
        let taken = names
            .iter()
            .any(|taken| *taken == name || *taken == preview);
        let repo_path = join(&place, &name);
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
    /// conflict as a dot and a count, and a lock when it is private. One
    /// opened by link is tagged "shared" and can be removed from the list;
    /// one the user may not send to says "Read only". Signed out, only
    /// public ones can be opened.
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
        let shared = self.github.prefs.is_shared(repo.id);
        let mut spoken = repo.key.repo.clone();
        let mut name = row![].align_y(Alignment::Center);
        if shared_name || shared {
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
        .align_y(Alignment::Center);
        if shared {
            spoken.push_str(", shared");
            content = content.push(text("shared").size(10).style(muted_text));
        }
        content = content.push(Space::new().width(Length::Fill));
        if !repo.push {
            spoken.push_str(", read only");
            content = content.push(read_only_label());
        } else if conflicts > 0 {
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
        let highlighted =
            picker.repo.is_none() && !picker.search.is_empty() && at == picker.highlighted;
        let greyed = picker.signed_out && (repo.private || picker.mode == Mode::SendTo);
        let row = reshiki::accessibility::button(format!("github-repo-{at}"), spoken, content)
            .width(Length::Fill)
            .padding([4, 6])
            .style(crate::app::workspace::control(current || highlighted))
            .on_press_maybe((!greyed).then(|| msg(Action::PickRepo(repo.key.clone()))));
        if !shared {
            return row.into();
        }
        // Right-click: Remove from list.
        let menu = (picker.menu == Some(repo.id)).then(|| {
            container(
                reshiki::accessibility::button(
                    "github-forget",
                    "Remove from list",
                    text("Remove from list").size(12),
                )
                .width(Length::Fill)
                .padding([6, 10])
                .style(crate::app::workspace::control(false))
                .on_press(msg(Action::Forget(repo.id))),
            )
            .width(170)
            .padding(5)
            .style(crate::app::color_popover::surface)
            .into()
        });
        let row = iced::widget::mouse_area(row).on_right_press(msg(Action::RowMenu(Some(repo.id))));
        Element::new(crate::app::popover::popover(
            row,
            menu,
            msg(Action::RowMenu(None)),
        ))
    }

    /// The one row a link in the search field shows: its repository's tile,
    /// where it leads, and the link. Enter or a click opens it.
    fn picker_link_row<'a>(&'a self, picker: &'a Picker, found: &'a Link) -> Element<'a, Message> {
        let id = (picker.repos.iter().flatten())
            .find(|repo| same(&repo.key, &found.key))
            .map_or(u64::MAX, |repo| repo.id);
        let hue = identity::hue(&self.github.prefs, id, &found.key);
        let place = found.place();
        let shown = picker
            .search
            .trim()
            .trim_start_matches("https://")
            .to_owned();
        reshiki::accessibility::button(
            "github-link-row",
            format!("Open {place}"),
            row![
                identity::tile(hue, identity::monogram(&found.key.repo)),
                column![
                    text(place.clone()).size(12).wrapping(text::Wrapping::None),
                    text(shown)
                        .size(10)
                        .style(muted_text)
                        .wrapping(text::Wrapping::None),
                ]
                .spacing(2)
                .width(Length::Fill)
                .clip(true),
                container(text("Enter").size(10).style(muted_text))
                    .padding([1, 5])
                    .style(thumb_frame),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding([4, 6])
        .style(crate::app::workspace::control(true))
        .on_press(msg(Action::Enter))
        .into()
    }

    pub(super) fn picker_view<'a>(&'a self, picker: &'a Picker) -> Element<'a, Message> {
        let search = reshiki::accessibility::text_input(
            SEARCH,
            "Search or paste a GitHub link",
            "Search or paste a GitHub link",
            &picker.search,
        )
        .style(crate::appearance::input_style)
        .on_input(|search| msg(Action::Search(search)))
        .on_submit(msg(Action::Enter))
        .size(13)
        .padding(7);
        let mut repos = column![].spacing(1);
        match (&picker.link, &picker.repos) {
            (Some(found), _) => repos = repos.push(self.picker_link_row(picker, found)),
            (None, None) => repos = repos.push(text("Listing…").size(12).style(muted_text)),
            (None, Some(_)) => {
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
                if picker.mode == Mode::Open {
                    repos = repos.push(add_repo_row(picker));
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
                Mode::Open if picker.tree => {
                    "↑↓ Move · → Open folder · Enter Open · ⇧Enter Insert · Esc Close"
                }
                Mode::Open => "Enter Open · ⇧Enter Insert · Backspace Up · Esc Close",
                Mode::SendTo => "Enter Send · Backspace Up · Esc Close",
            })
            .size(11)
            .style(muted_text),
        );
        footer = footer.push(Space::new().width(Length::Fill));
        if picker.copy {
            footer = footer.push(
                reshiki::accessibility::button(
                    "github-this-computer",
                    "This computer…",
                    text("This computer…").size(13),
                )
                .padding([7, 12])
                .style(crate::appearance::secondary)
                .on_press(msg(Action::ThisComputer)),
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
        let file = picker.selected_file();
        if picker.mode == Mode::Open {
            footer = footer.push(
                reshiki::accessibility::button(
                    "github-picker-insert",
                    "Insert into drawing",
                    text("Insert").size(13),
                )
                .padding([7, 12])
                .style(crate::appearance::secondary)
                .on_press_maybe((!picker.busy && file.is_some()).then(|| msg(Action::Insert))),
            );
        }
        // Another chemistry file, or any file of a read-only repository.
        let read_only = file.is_some_and(|(path, _)| {
            !path.ends_with(".rsk") || picker.repo.as_ref().is_some_and(|repo| !repo.push)
        });
        let (id, label, action, ready) = match picker.mode {
            Mode::Open => (
                "github-picker-open",
                if read_only { "Open read-only" } else { "Open" },
                Action::Open,
                match picker.tree {
                    true => picker.row.is_some(),
                    false => picker.selected_entry().is_some(),
                },
            ),
            Mode::SendTo => (
                "github-send-here",
                "Send",
                Action::SendHere,
                picker.repo.is_some()
                    && picker.entries.is_some()
                    && !picker.unlisted
                    && picker.reference.is_none(),
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
                Mode::SendTo if picker.copy => "Save a copy",
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
        let button = |id: &'static str, label: &'static str, action: Message| {
            reshiki::accessibility::button(id, label, text(label).size(13))
                .padding([7, 12])
                .style(crate::appearance::secondary)
                .on_press(action)
        };
        if let Some(key) = &picker.cant_open {
            return center(
                column![
                    text(format!("Can't open {}/{}", key.owner, key.repo)).size(14),
                    text("Private, or the link is wrong. The owner can add ReShiki.")
                        .size(12)
                        .style(muted_text),
                    row![
                        button(
                            "github-copy-request",
                            "Copy request for the owner",
                            Message::GitHub(GitHubAction::CopyRequest(key.clone())),
                        ),
                        button(
                            "github-open-repo",
                            "Open on GitHub ↗",
                            Message::GitHub(GitHubAction::OpenRepo(key.clone())),
                        ),
                    ]
                    .spacing(8),
                ]
                .spacing(10)
                .align_x(Alignment::Center)
                .into(),
            );
        }
        if picker.limited {
            return center(
                column![
                    text(LIMITED).size(13),
                    reshiki::accessibility::button(
                        "github-picker-sign-in",
                        "Sign in…",
                        text("Sign in…").size(13),
                    )
                    .padding([8, 16])
                    .style(crate::appearance::primary)
                    .on_press(msg(Action::SignIn)),
                ]
                .spacing(12)
                .align_x(Alignment::Center)
                .into(),
            );
        }
        if picker.signed_out && picker.repo.is_none() {
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
        // `lab-notes / scheme`: the parents are muted and go back there. The
        // tree shows the repository alone.
        let mut crumbs = row![].spacing(6).align_y(Alignment::Center);
        let trail = picker.trail.iter().filter(|_| !picker.tree);
        let names =
            std::iter::once(repo.key.repo.as_str()).chain(trail.map(|(name, _)| name.as_str()));
        let depth = if picker.tree { 0 } else { picker.trail.len() };
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
        if !repo.push {
            crumbs = crumbs.push(container(read_only_label()).padding([0, 6]));
        }
        crumbs = crumbs.push(Space::new().width(Length::Fill));
        if picker.mode == Mode::Open {
            crumbs = crumbs.push(view_switch(picker.tree));
        }
        if picker.tree {
            return column![crumbs, self.picker_tree(picker)]
                .spacing(10)
                .width(Length::Fill)
                .into();
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
                // Drawings show once their last changes are known.
                let (shown, matching) = picker.tiles();
                let tiles = shown
                    .into_iter()
                    .filter_map(|at| Some((at, entries.get(at)?)))
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
                                let preview = thumbnail(picker, &entry.name, &entry.sha);
                                let meta = picker.changed.get(&path).map(|(who, at)| {
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
                                .wrapping(text::Wrapping::WordOrGlyph)
                        }))
                        .spacing(4);
                        let tile = reshiki::accessibility::button(
                            format!("github-file-{at}"),
                            spoken,
                            content,
                        )
                        .width(128)
                        .height(TILE_HEIGHT)
                        .padding(6)
                        .style(crate::app::workspace::control(selected))
                        .on_press(msg(Action::Select(at)));
                        match entry.folder || picker.mode != Mode::Open {
                            true => tile.into(),
                            false => file_menu(picker, path, tile.into()),
                        }
                    });
                let creating: Vec<Element<'_, Message>> = match picker.mode {
                    _ if !picker.can_create() => vec![],
                    Mode::Open => vec![new_item_tile(picker, false), new_item_tile(picker, true)],
                    Mode::SendTo => vec![new_item_tile(picker, true)],
                };
                let tiles = iced::widget::Row::with_children(creating.into_iter().chain(tiles))
                    .spacing(8)
                    .wrap();
                let mut body = column![tiles].spacing(12);
                if entries.is_empty() && !picker.unlisted {
                    body = body.push(text("Empty").size(13).style(muted_text));
                } else if matching == 0 && !picker.query().is_empty() {
                    body = body.push(text("No match in this folder.").size(13).style(muted_text));
                } else if matching > SHOWN {
                    body = body.push(
                        text(format!(
                            "Showing {SHOWN} of {matching}. Search to find the others."
                        ))
                        .size(11)
                        .style(muted_text),
                    );
                }
                if picker.signed_out {
                    body = body.push(text(SIGNED_OUT_HINT).size(11).style(muted_text));
                }
                scrollable(body)
                    .height(Length::Fill)
                    .on_scroll(|view| {
                        let bounds = view.bounds();
                        let top = view.absolute_offset().y;
                        msg(Action::Scrolled(top, bounds.height, bounds.width))
                    })
                    .into()
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

/// `+ Add public repository…` after the repositories; clicked, a field for
/// `owner/repo` or a link. Enter adds it, tagged shared, and opens it.
fn add_repo_row(picker: &Picker) -> Element<'_, Message> {
    match &picker.adding {
        None => reshiki::accessibility::button(
            "github-add-repo-open",
            "Add public repository…",
            row![
                container(text("+").size(14)).center_x(22),
                text("Add public repository…").size(12)
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding([4, 6])
        .style(crate::app::workspace::control(false))
        .on_press(msg(Action::Add))
        .into(),
        Some(name) => reshiki::accessibility::text_input(
            ADD_NAME,
            "Public repository",
            "owner/repo or GitHub link",
            name,
        )
        .style(crate::appearance::input_style)
        .on_input(|name| msg(Action::AddName(name)))
        .on_submit_maybe((!picker.busy).then(|| msg(Action::AddRepo)))
        .size(12)
        .padding(6)
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
            .height(TILE_HEIGHT)
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
    .height(TILE_HEIGHT)
    .padding(6)
    .style(crate::app::workspace::control(false))
    .on_press(msg(Action::NewItem(folder)))
    .into()
}

/// Tiles and Tree, two icon buttons at the end of the folder path.
fn view_switch<'a>(tree: bool) -> Element<'a, Message> {
    let button = |id: &'static str, label: &'static str, icon, on: bool| {
        reshiki::accessibility::button(
            id,
            label,
            iced::widget::canvas(crate::app::icons::Glyph(icon, on))
                .width(16)
                .height(16),
        )
        .checked(on)
        .padding([4, 8])
        .style(crate::app::workspace::control(on))
        .on_press(msg(Action::View(id == "github-view-tree")))
    };
    container(
        row![
            button(
                "github-view-tiles",
                "Tiles",
                crate::app::icons::Icon::Tiles,
                !tree
            ),
            button(
                "github-view-tree",
                "Tree",
                crate::app::icons::Icon::Tree,
                tree
            ),
        ]
        .spacing(2),
    )
    .padding(1)
    .style(thumb_frame)
    .into()
}

/// A file's drawing, once read.
fn drawing<'a>(picker: &'a Picker, sha: &str) -> Element<'a, Message> {
    match picker.thumbs.get(sha) {
        Some(Some(doc)) => crate::canvas::layered::canvas(crate::canvas::DrawingPreview(doc))
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
        _ => Space::new().into(),
    }
}

/// A file's drawing, with its format's label in the corner when it is not
/// a drawing.
fn thumbnail<'a>(picker: &'a Picker, name: &str, sha: &str) -> Element<'a, Message> {
    match gh::format_label(name) {
        Some(label) => iced::widget::stack![
            drawing(picker, sha),
            container(format_label(label))
                .align_right(Length::Fill)
                .align_bottom(Length::Fill)
                .padding(4),
        ]
        .into(),
        None => drawing(picker, sha),
    }
}

/// `MOL`, `SDF`, …: what kind of file a tile or row is.
fn format_label<'a>(label: &'static str) -> Element<'a, Message> {
    container(text(label).size(10))
        .padding([1, 5])
        .style(|theme: &Theme| {
            let (ink, paper) = if crate::appearance::is_dark(theme) {
                (
                    Color::from_rgb8(224, 200, 140),
                    Color::from_rgb8(58, 50, 32),
                )
            } else {
                (
                    Color::from_rgb8(107, 90, 46),
                    Color::from_rgb8(246, 239, 220),
                )
            };
            container::Style {
                text_color: Some(ink),
                background: Some(paper.into()),
                border: Border {
                    radius: 4.0.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            }
        })
        .into()
}

/// Right-click on a file's tile or row: Insert into drawing.
fn file_menu<'a>(
    picker: &'a Picker,
    path: String,
    content: Element<'a, Message>,
) -> Element<'a, Message> {
    let open = picker.file_menu.as_ref() == Some(&path);
    let menu = open.then(|| {
        container(
            reshiki::accessibility::button(
                "github-insert",
                "Insert into drawing",
                text("Insert into drawing").size(12),
            )
            .width(Length::Fill)
            .padding([6, 10])
            .style(crate::app::workspace::control(false))
            .on_press(msg(Action::Insert)),
        )
        .width(180)
        .padding(5)
        .style(crate::app::color_popover::surface)
        .into()
    });
    let area = iced::widget::mouse_area(content).on_right_press(msg(Action::FileMenu(Some(path))));
    Element::new(crate::app::popover::popover(
        area,
        menu,
        msg(Action::FileMenu(None)),
    ))
}

/// An eye and "Read only": the user may not send to the repository.
pub(super) fn read_only_label<'a>() -> Element<'a, Message> {
    row![
        iced::widget::canvas(crate::app::icons::Glyph(crate::app::icons::Icon::Eye, true))
            .width(14)
            .height(14),
        text("Read only").size(11).style(muted_text),
    ]
    .spacing(3)
    .align_y(Alignment::Center)
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
