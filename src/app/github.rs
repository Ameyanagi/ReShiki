//! GitHub in the editor: sign-in, the repository picker, Send, the status
//! chip, the question on closing and Versions. Other people's repositories
//! open by link, read only (signed out too); a branch or a commit opens as
//! an old version, kept apart and never sent. The work itself is in
//! `reshiki::github`; this keeps what the window shows. Its messages are
//! handled before the modal gates, like the star toast's: none of them edits
//! a drawing except through the normal save and edit paths.
use super::{
    App, InspectorTab, Message,
    document_tab::{DocumentTab, TabId},
    focus_drawing,
};
use iced::widget::{Space, button, column, container, row, text};
use iced::{Alignment, Border, Color, Element, Length, Task, Theme};
use reshiki::github::{
    self as gh, Repo, RepoKey, api, auth,
    client::Client,
    config,
    credentials::{self, Tokens},
    send,
    workcopy::{self, RepoState},
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

mod account;
mod compare;
mod conflict;
mod identity;
pub(super) mod picker;
#[cfg(test)]
mod tests;
mod versions;

pub(super) use picker::Mode;

const OPEN_FROM: &str = "Open from GitHub…";
/// The palette's rows for a repository or drawing: `Open from GitHub: lab-notes/ring`.
pub(super) const OPEN_FROM_ROW: &str = "Open from GitHub: ";
const SEND_TO: &str = "Send to GitHub…";
const NOT_SET_UP: &str = "Not set up in this build";
const SAVE_COPY: &str = "Save a copy…";
/// Read-only repositories are asked again on window focus, at most this often.
const RECHECK: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, Clone)]
pub(super) enum Account {
    /// This build has no GitHub App client id.
    NotConfigured,
    /// With a note on why, such as `Code expired.`
    SignedOut(Option<String>),
    Requesting,
    Waiting(auth::DeviceCode),
    SignedIn {
        login: String,
        /// `false`: the sign-in lasts until ReShiki quits.
        remembered: bool,
    },
    /// Signed in, but ReShiki may use no repository yet.
    NeedsInstall {
        login: String,
    },
}

#[derive(Debug)]
pub(super) enum Dialog {
    /// The sign-in code, or why the last sign-in ended.
    SignIn,
    Picker(Box<picker::Picker>),
    /// Drawn as a popover above the status chip.
    Send(SendDraft),
    Conflict(Box<conflict::Review>),
    /// A version, under its label, and the drawing now.
    Compare(Box<compare::Compare>, String),
}

#[derive(Debug)]
pub(super) struct SendDraft {
    key: RepoKey,
    files: Vec<String>,
    message: String,
    /// The paths of a send of several drawings, shown on request.
    listed: bool,
}

/// A close waiting for the Send question, or for the Send it started.
#[derive(Debug, Clone)]
enum Closing {
    /// The tab, and its revision when asked: an edit made since cancels.
    Tab(TabId, u64),
    /// The window, the tabs whose changes were discarded, and the
    /// repositories still sending.
    Window(iced::window::Id, Vec<TabId>, HashSet<RepoKey>),
}

pub(super) struct State {
    client_id: Option<String>,
    ends: config::Endpoints,
    /// `<data>/github`; `None` in tests until one sets a folder.
    pub(super) root: Option<PathBuf>,
    /// Rises with every sign-in, sign-out and picker, so late results drop.
    generation: u64,
    pub(super) account: Account,
    client: Option<Client>,
    /// The repositories ReShiki may use, as last listed.
    repos: Vec<Repo>,
    pub(super) dialog: Option<Dialog>,
    pub(super) menu: bool,
    /// What runs once a sign-in started for it succeeds.
    after_sign_in: Option<Action>,
    /// Keyed by the repository's folder here.
    states: HashMap<RepoKey, RepoState>,
    /// Rises with every state write and picker request, so late ones drop.
    seq: u64,
    unsent: HashMap<RepoKey, Vec<String>>,
    sending: HashSet<RepoKey>,
    failed: HashMap<RepoKey, String>,
    /// What the last send of each repository found changed on GitHub.
    conflicts: HashMap<RepoKey, Vec<send::Conflict>>,
    /// A choice waiting for its drawing to be saved.
    resolving: Option<(RepoKey, send::Choice)>,
    /// What the last choice kept, until the send it started ends.
    kept: HashMap<RepoKey, String>,
    /// A Send waiting for the open drawings of its repository to be saved:
    /// its message, and the tabs still to save.
    after_save: Option<(RepoKey, String, Vec<TabId>)>,
    closing: Option<Closing>,
    /// Tabs answered Later, and a window close answered once.
    later: HashSet<TabId>,
    window_answered: bool,
    /// Starred through the API in this run.
    pub(super) starred: bool,
    versions: versions::Versions,
    /// Repository colors and the repository last used.
    prefs: identity::Prefs,
    /// A status that is a question, not an error, shown in amber.
    pub(super) amber: Option<String>,
    /// When read-only repositories were last asked whether the user may
    /// send to them now.
    checked: Option<Instant>,
    /// The read-only chip's menu is open.
    pub(super) chip_menu: bool,
}

impl State {
    pub(super) fn new() -> Self {
        let client_id = config::client_id();
        let root = if cfg!(test) {
            None
        } else {
            workcopy::root().ok()
        };
        Self {
            account: if client_id.is_some() {
                Account::SignedOut(None)
            } else {
                Account::NotConfigured
            },
            client_id,
            ends: config::endpoints(),
            prefs: root.as_deref().map(identity::load).unwrap_or_default(),
            root,
            generation: 0,
            client: None,
            repos: Vec::new(),
            dialog: None,
            menu: false,
            after_sign_in: None,
            states: HashMap::new(),
            seq: 0,
            unsent: HashMap::new(),
            sending: HashSet::new(),
            failed: HashMap::new(),
            conflicts: HashMap::new(),
            resolving: None,
            kept: HashMap::new(),
            after_save: None,
            closing: None,
            later: HashSet::new(),
            window_answered: false,
            starred: false,
            versions: versions::Versions::default(),
            amber: None,
            checked: None,
            chip_menu: false,
        }
    }

    pub(super) fn configured(&self) -> bool {
        !matches!(self.account, Account::NotConfigured)
    }

    /// A client while signed in.
    pub(super) fn client(&self) -> Option<Client> {
        match self.account {
            Account::SignedIn { .. } | Account::NeedsInstall { .. } => self.client.clone(),
            _ => None,
        }
    }

    /// A client for reading: signed in, or else signed out, for public
    /// repositories.
    pub(super) fn reader(&self) -> Option<Client> {
        self.client().or_else(|| {
            self.configured()
                .then(|| Client::public(self.ends.clone()).ok())
                .flatten()
        })
    }

    pub(super) fn login(&self) -> Option<&str> {
        match &self.account {
            Account::SignedIn { login, .. } | Account::NeedsInstall { login } => Some(login),
            _ => None,
        }
    }

    /// The dialog owns the keyboard: see [`modal_key`].
    pub(super) fn dialog_open(&self) -> bool {
        self.dialog.is_some()
    }

    /// The repository and repository path of a drawing kept for GitHub.
    pub(super) fn locate(&self, path: Option<&Path>) -> Option<(RepoKey, String)> {
        workcopy::locate(self.root.as_deref()?, path?)
    }

    /// The repository and repository path of an old version.
    pub(super) fn old(&self, path: Option<&Path>) -> Option<(RepoKey, String)> {
        workcopy::locate_old(self.root.as_deref()?, path?)
    }

    /// Whether the user may not send to the repository kept in `key`.
    fn read_only(&self, key: &RepoKey) -> bool {
        self.states.get(key).is_some_and(|state| state.read_only)
    }

    /// `Please add ReShiki to owner/repo …` with the page that does it.
    fn request(&self, key: &RepoKey) -> String {
        let slug = config::app_slug().unwrap_or_default();
        format!(
            "Please add ReShiki to {}/{} so I can send drawings: {}",
            key.owner,
            key.repo,
            self.ends.web(&format!("/apps/{slug}/installations/new"))
        )
    }

    /// The repository kept in folder `key`, as GitHub names it now.
    fn named(&self, key: &RepoKey) -> RepoKey {
        self.states
            .get(key)
            .map_or_else(|| key.clone(), RepoState::key)
    }

    /// The repository's id, when known.
    fn repo_id(&self, key: &RepoKey) -> Option<u64> {
        self.states.get(key).map(|state| state.repo_id).or_else(|| {
            self.repos
                .iter()
                .find(|repo| repo.key == *key)
                .map(|repo| repo.id)
        })
    }

    fn hue(&self, key: &RepoKey) -> usize {
        identity::hue(
            &self.prefs,
            self.repo_id(key).unwrap_or(u64::MAX),
            &self.named(key),
        )
    }
}

#[derive(Debug, Clone)]
pub enum Action {
    Menu(bool),
    /// Escape, Cancel or a click outside the dialog.
    Close,
    Loaded(u64, Result<Option<Tokens>, String>),
    /// Asks GitHub for a code and shows it.
    SignIn,
    CodeReady(u64, Result<auth::DeviceCode, gh::Error>),
    Poll(u64),
    Polled(u64, Result<auth::Poll, gh::Error>),
    CopyCode,
    OpenVerification,
    SignedIn(u64, Result<gh::Account, gh::Error>),
    /// `false`: the sign-in lasts until ReShiki quits.
    Remembered(u64, bool),
    SignOut,
    SignedOut(Result<(), String>),
    ChooseRepositories,
    Listed(u64, Result<gh::Account, gh::Error>),
    Browser(Result<(), String>),
    Picker(picker::Action),
    /// Send for the drawing in front: the Send popover, or the picker for a
    /// drawing not on GitHub.
    ShowSend,
    SendMessage(String),
    /// Shows or hides the paths of a send of several drawings.
    SendList,
    Send,
    /// Saves the next drawing a Send waits for, or sends.
    SaveNext,
    Sent(RepoKey, String, Result<send::SendOutcome, gh::Error>),
    Recorded(RepoKey, Result<Option<workcopy::SaveMeta>, String>),
    Persisted(RepoKey, Result<(), String>),
    /// Send (`true`) or Later, asked on closing.
    CloseAnswer(bool),
    /// The conflict dialog for the drawing in front's repository.
    Review,
    /// Who changed the drawing in conflict on GitHub, and when: by
    /// repository, repository path and the conflict's commit.
    ReviewMeta(RepoKey, String, String, Option<api::RemoteVersion>),
    Resolve(send::Choice),
    /// The choice, once the save it waited for has landed.
    Settle(RepoKey, send::Choice),
    ShowVersions,
    Versions(versions::Action),
    /// A hue chosen for the repository in a tab's menu.
    Color(RepoKey, usize),
    /// The repository colors and the last repository, written.
    Kept(Result<(), String>),
    CopyLink,
    OpenOnGitHub,
    /// Copies the link of a tab's drawing, from its menu.
    CopyTabLink(TabId),
    /// Copies the request that the owner add ReShiki to a repository.
    CopyRequest(RepoKey),
    /// The repository's page on GitHub.
    OpenRepo(RepoKey),
    /// The drawing in front, read only or an old version, saved to a
    /// repository the user may send to, or to this computer.
    SaveCopy,
    /// The read-only chip's menu.
    ChipMenu(bool),
    /// From an old version: the drawing as it is now.
    OpenLatest,
    /// The window came to the front: after Choose repositories, the picker
    /// lists them again; read-only repositories are asked about again.
    Focused,
    /// Whether a read-only repository may be sent to now, asked in the
    /// sign-in of that generation.
    Access(u64, RepoKey, Result<Repo, gh::Error>),
    /// The clipboard's text on Paste in a tab: a link opens, anything else
    /// pastes into that tab.
    Pasted(TabId, Option<String>),
    /// A drawing known here, from the command palette.
    OpenKnown(PathBuf),
}

/// Keys of an open GitHub dialog, before its fields: Escape closes, and the
/// arrows move in the picker's lists.
pub(super) fn modal_key(
    key: &iced::keyboard::Key,
    modifiers: iced::keyboard::Modifiers,
) -> Option<Message> {
    use iced::keyboard::{Key, key::Named};
    match key {
        Key::Named(Named::Escape) => Some(Message::GitHub(Action::Close)),
        Key::Named(Named::ArrowUp) if modifiers.is_empty() => {
            Some(Message::GitHub(Action::Picker(picker::Action::Move(-1))))
        }
        Key::Named(Named::ArrowDown) if modifiers.is_empty() => {
            Some(Message::GitHub(Action::Picker(picker::Action::Move(1))))
        }
        _ => None,
    }
}

/// Keys of an open GitHub dialog that no field took: in the picker, Enter
/// opens what is selected and Backspace goes up a folder. Elsewhere they do
/// nothing, so the conflict dialog has no default button.
pub(super) fn leftover_key(
    key: &iced::keyboard::Key,
    modifiers: iced::keyboard::Modifiers,
) -> Option<Message> {
    use iced::keyboard::{Key, key::Named};
    let action = match key {
        Key::Named(Named::Enter) if modifiers.is_empty() => picker::Action::Enter,
        Key::Named(Named::Backspace) if modifiers.is_empty() => picker::Action::Up,
        _ => return None,
    };
    Some(Message::GitHub(Action::Picker(action)))
}

fn perform<T: Send + 'static>(
    future: impl Future<Output = T> + Send + 'static,
    then: impl FnOnce(T) -> Action + Send + 'static,
) -> Task<Message> {
    Task::perform(future, move |result| Message::GitHub(then(result)))
}

fn open_url(url: String) -> Task<Message> {
    perform(reshiki::updates::open_url(url), Action::Browser)
}

/// `Send “a.rsk” to GitHub?` with Send and Later.
fn ask_send(title: String, body: String) -> Task<Message> {
    const SEND: &str = "Send";
    iced::window::latest().then(move |window| {
        let dialog = rfd::AsyncMessageDialog::new()
            .set_level(rfd::MessageLevel::Info)
            .set_title(title.clone())
            .set_description(body.clone())
            .set_buttons(rfd::MessageButtons::OkCancelCustom(
                SEND.into(),
                "Later".into(),
            ));
        let answer = |result: rfd::MessageDialogResult| {
            use rfd::MessageDialogResult as Answer;
            let send = match result {
                Answer::Ok | Answer::Yes => true,
                Answer::Custom(label) => label == SEND,
                _ => false,
            };
            Message::GitHub(Action::CloseAnswer(send))
        };
        match window {
            Some(id) => iced::window::run(id, move |window| dialog.set_parent(&window).show())
                .then(move |shown| Task::perform(shown, answer)),
            None => Task::perform(dialog.show(), answer),
        }
    })
}

/// The address in a link file: a Windows `.url` (`URL=…`), or a macOS
/// `.webloc`, whose plist is XML or binary (where the address is the one
/// run of text starting with `http`).
fn shortcut_url(text: &str) -> Option<String> {
    if let Some(url) = text
        .lines()
        .find_map(|line| line.trim().strip_prefix("URL="))
    {
        return Some(url.trim().to_owned());
    }
    if let Some((_, rest)) = text.split_once("<key>URL</key>") {
        let (_, value) = rest.split_once("<string>")?;
        let (value, _) = value.split_once("</string>")?;
        return Some(value.trim().replace("&amp;", "&"));
    }
    let start = text.find("http")?;
    let url: String = text[start..]
        .chars()
        .take_while(|c| c.is_ascii_graphic() && !"<>\"'".contains(*c))
        .collect();
    Some(url)
}

/// `1 drawing` or `3 drawings`.
fn drawings(count: usize) -> String {
    match count {
        1 => "1 drawing".into(),
        n => format!("{n} drawings"),
    }
}

/// `scheme/ring.rsk` → `ring.rsk`.
pub(super) fn file_name(repo_path: &str) -> &str {
    repo_path.rsplit('/').next().unwrap_or(repo_path)
}

impl App {
    /// Reads the sign-in kept on this computer, without asking GitHub.
    pub(super) fn github_start(&mut self) -> Task<Message> {
        if !self.github.configured() || cfg!(test) {
            return Task::none();
        }
        let generation = self.github.generation;
        perform(credentials::load(), move |result| {
            Action::Loaded(generation, result)
        })
    }

    fn github_signed_out(&mut self, note: Option<String>) {
        self.github.generation += 1;
        self.github.client = None;
        self.github.account = Account::SignedOut(note);
    }

    /// An error from GitHub. A sign-in GitHub no longer takes signs out.
    fn github_failed(&mut self, error: &gh::Error) -> String {
        if *error == gh::Error::SignInExpired {
            self.github_signed_out(Some(error.to_string()));
        }
        error.to_string()
    }

    /// Closes the dialog; the picker's late results drop with it.
    fn github_close_dialog(&mut self) -> Task<Message> {
        self.github.dialog = None;
        self.github.resolving = None;
        focus_drawing()
    }

    /// Shows a code at once; `then` runs once GitHub approves.
    fn github_sign_in(&mut self, then: Option<Action>) -> Task<Message> {
        self.github.menu = false;
        if !self.github.configured() {
            return Task::none();
        }
        if self.github.client().is_some() {
            return then.map_or_else(Task::none, |then| self.github_action(then));
        }
        let (id, ends) = (
            self.github.client_id.clone().unwrap_or_default(),
            self.github.ends.clone(),
        );
        // Trying again in the dialog keeps what the first try was for.
        if then.is_some() || !matches!(self.github.dialog, Some(Dialog::SignIn)) {
            self.github.after_sign_in = then;
        }
        self.github.dialog = Some(Dialog::SignIn);
        self.github.generation += 1;
        let generation = self.github.generation;
        self.github.account = Account::Requesting;
        perform(
            async move { auth::request_code(&id, &ends).await },
            move |result| Action::CodeReady(generation, result),
        )
    }

    /// Writes the repository colors and the last repository.
    fn github_keep_prefs(&mut self) -> Task<Message> {
        let (Some(root), prefs) = (self.github.root.clone(), self.github.prefs.clone()) else {
            return Task::none();
        };
        self.github.seq += 1;
        let seq = self.github.seq;
        perform(
            async move {
                reshiki::task::spawn_blocking(move || identity::save(&root, &prefs, seq))
                    .await
                    .map_err(|error| error.to_string())?
            },
            Action::Kept,
        )
    }

    /// Repositories seen for the first time keep a color.
    fn github_seen<'a>(
        &mut self,
        repos: impl IntoIterator<Item = (u64, &'a RepoKey)>,
    ) -> Task<Message> {
        let mut new = false;
        for (id, key) in repos {
            new |= identity::assign(&mut self.github.prefs, id, key);
        }
        if new {
            self.github_keep_prefs()
        } else {
            Task::none()
        }
    }

    pub(super) fn github_action(&mut self, action: Action) -> Task<Message> {
        let id = self.github.client_id.clone().unwrap_or_default();
        let ends = self.github.ends.clone();
        match action {
            Action::Menu(open) => self.github.menu = open,
            Action::Close => {
                if matches!(self.github.dialog, Some(Dialog::SignIn))
                    && let Some(then) = self.github.after_sign_in.take()
                    && matches!(then, Action::CloseAnswer(_))
                {
                    // A close that waited for this sign-in waits no more.
                    if let Some(Closing::Window(..)) = self.github.closing.take() {
                        self.cancel_close();
                    }
                }
                // Closing the sign-in dialog while waiting cancels the sign-in.
                if matches!(
                    self.github.account,
                    Account::Requesting | Account::Waiting(_)
                ) {
                    self.github_signed_out(None);
                }
                return self.github_close_dialog();
            }
            Action::Loaded(generation, result) => {
                if generation != self.github.generation
                    || !matches!(self.github.account, Account::SignedOut(None))
                {
                    return Task::none();
                }
                match result.map(|tokens| tokens.filter(|tokens| !tokens.login.is_empty())) {
                    Ok(Some(tokens)) => {
                        let login = tokens.login.clone();
                        if let Ok(client) = Client::new(id, ends, Some(tokens)) {
                            self.github.client = Some(client);
                            self.github.account = Account::SignedIn {
                                login,
                                remembered: true,
                            };
                            // Versions lists GitHub's versions again.
                            self.github.versions.target = None;
                            self.star_github_ready();
                            return self.picker_signed_in();
                        }
                    }
                    Ok(None) => {}
                    Err(error) => {
                        self.github.account = Account::SignedOut(Some(error));
                    }
                }
            }
            Action::SignIn => return self.github_sign_in(None),
            Action::CodeReady(generation, result) => {
                if generation != self.github.generation {
                    return Task::none();
                }
                match result {
                    Ok(code) => {
                        let (user_code, url, wait) = (
                            code.user_code.clone(),
                            code.verification_uri.clone(),
                            code.interval,
                        );
                        self.github.account = Account::Waiting(code);
                        return Task::batch([
                            self.account_focus(),
                            iced::clipboard::write(user_code),
                            open_url(url),
                            poll_after(generation, wait),
                        ]);
                    }
                    Err(error) => self.github_signed_out(Some(error.to_string())),
                }
            }
            Action::Poll(generation) => {
                if let (true, Account::Waiting(code)) =
                    (generation == self.github.generation, &self.github.account)
                {
                    let code = code.clone();
                    return perform(
                        async move { auth::poll(&id, &ends, &code).await },
                        move |result| Action::Polled(generation, result),
                    );
                }
            }
            Action::Polled(generation, result) => {
                let Account::Waiting(code) = &mut self.github.account else {
                    return Task::none();
                };
                if generation != self.github.generation {
                    return Task::none();
                }
                match result {
                    Ok(auth::Poll::Pending) | Err(gh::Error::Offline) => {
                        return poll_after(generation, code.interval);
                    }
                    Ok(auth::Poll::SlowDown(interval)) => {
                        code.interval =
                            interval.max(code.interval + std::time::Duration::from_secs(5));
                        return poll_after(generation, code.interval);
                    }
                    Ok(auth::Poll::Expired) => {
                        self.github_signed_out(Some("Code expired.".into()));
                    }
                    Ok(auth::Poll::Denied) => {
                        self.github_signed_out(Some("Sign-in canceled.".into()));
                    }
                    Ok(auth::Poll::Done(tokens)) => {
                        let Ok(client) = Client::new(id, ends, Some(tokens)) else {
                            self.github_signed_out(Some(gh::Error::Offline.to_string()));
                            return Task::none();
                        };
                        self.github.client = Some(client.clone());
                        return perform(
                            async move { api::account(&client).await },
                            move |result| Action::SignedIn(generation, result),
                        );
                    }
                    Err(error) => self.github_signed_out(Some(error.to_string())),
                }
            }
            Action::CopyCode => {
                if let Account::Waiting(code) = &self.github.account {
                    self.status = "Code copied".into();
                    self.error = false;
                    return iced::clipboard::write(code.user_code.clone());
                }
            }
            Action::OpenVerification => {
                if let Account::Waiting(code) = &self.github.account {
                    return open_url(code.verification_uri.clone());
                }
            }
            Action::SignedIn(generation, result) => {
                if generation != self.github.generation {
                    return Task::none();
                }
                match result {
                    Ok(account) => {
                        // Kept for the next launch only now, so a canceled
                        // sign-in is never kept.
                        let (client, login) = (self.github.client.clone(), account.login.clone());
                        let colors = self.github_listed(account, true);
                        self.github.versions.target = None;
                        self.star_github_signed_in();
                        self.status = format!("Signed in as @{}", login);
                        self.error = false;
                        let mut tasks = vec![colors];
                        if let Some(client) = client {
                            tasks.push(perform(
                                async move { client.remember(&login).await },
                                move |kept| Action::Remembered(generation, kept),
                            ));
                        }
                        // The dialog closes, and what started the sign-in goes on.
                        if matches!(self.github.dialog, Some(Dialog::SignIn)) {
                            tasks.push(self.github_close_dialog());
                        }
                        if let Some(then) = self.github.after_sign_in.take() {
                            tasks.push(self.github_action(then));
                        }
                        return Task::batch(tasks);
                    }
                    Err(error) => self.github_signed_out(Some(error.to_string())),
                }
            }
            Action::Remembered(generation, kept) => {
                if let (true, Account::SignedIn { remembered, .. }) = (
                    generation == self.github.generation,
                    &mut self.github.account,
                ) {
                    *remembered = kept;
                    if !kept {
                        self.status = "Sign-in won't be remembered on this computer.".into();
                        self.error = false;
                    }
                }
            }
            Action::SignOut => {
                self.github.menu = false;
                let client = self.github.client.take();
                self.github_signed_out(None);
                self.github.repos.clear();
                return perform(
                    async move {
                        match client {
                            Some(client) => client.sign_out().await,
                            None => credentials::delete().await,
                        }
                    },
                    Action::SignedOut,
                );
            }
            Action::SignedOut(result) => {
                if result.is_err() {
                    self.status = "Couldn't forget the sign-in on this computer.".into();
                    self.error = true;
                }
            }
            Action::ChooseRepositories => {
                self.github.menu = false;
                if let Some(Dialog::Picker(picker)) = &mut self.github.dialog {
                    picker.browsing = true;
                }
                let slug = config::app_slug().unwrap_or_default();
                return open_url(ends.web(&format!("/apps/{slug}/installations/new")));
            }
            Action::Listed(generation, result) => {
                if generation != self.github.generation {
                    return Task::none();
                }
                match result {
                    Ok(account) => {
                        let remembered = !matches!(
                            self.github.account,
                            Account::SignedIn {
                                remembered: false,
                                ..
                            }
                        );
                        let colors = self.github_listed(account, remembered);
                        return Task::batch([colors, self.picker_listed()]);
                    }
                    Err(error) => {
                        let error = self.github_failed(&error);
                        if let Some(Dialog::Picker(picker)) = &mut self.github.dialog {
                            picker.error = Some(error);
                        }
                    }
                }
            }
            Action::Browser(result) => {
                if let Err(error) = result {
                    self.status = error;
                    self.error = true;
                }
            }
            Action::Picker(action) => return self.picker_action(action),
            Action::ShowSend => {
                self.github.menu = false;
                let Some((key, _)) = self.github.locate(self.tab.path.as_deref()) else {
                    return self.picker_action(picker::Action::Show(Mode::SendTo));
                };
                if self.github.read_only(&key) {
                    return self.github_action(Action::SaveCopy);
                }
                if self.github.client().is_none() {
                    return self.github_sign_in(Some(Action::ShowSend));
                }
                self.github_refresh(&key);
                let files = self.github_waiting(&key, |tab| self.edited(tab));
                if files.is_empty() {
                    self.status = "Nothing to send".into();
                    self.error = false;
                    return Task::none();
                }
                let message = send::default_message(&files);
                self.github.dialog = Some(Dialog::Send(SendDraft {
                    key,
                    files,
                    message,
                    listed: false,
                }));
                return iced::widget::operation::focus(SEND_MESSAGE)
                    .chain(iced::widget::operation::select_all(SEND_MESSAGE));
            }
            Action::SendMessage(message) => {
                if let Some(Dialog::Send(draft)) = &mut self.github.dialog {
                    draft.message = message;
                }
            }
            Action::SendList => {
                if let Some(Dialog::Send(draft)) = &mut self.github.dialog {
                    draft.listed = !draft.listed;
                }
            }
            Action::Send => {
                let Some(Dialog::Send(draft)) = self.github.dialog.take() else {
                    return Task::none();
                };
                let message = match draft.message.trim() {
                    "" => send::default_message(&draft.files),
                    message => message.to_owned(),
                };
                let close = self.github_close_dialog();
                if !self.finish_inline(true) {
                    return close;
                }
                // Send what the drawings show: each is saved first, as Cmd+S does.
                let tabs = self.github_edited(&draft.key);
                let tabs = tabs.into_iter().map(|(id, _)| id).collect();
                self.github.after_save = Some((draft.key, message, tabs));
                return Task::batch([close, self.github_save_next()]);
            }
            Action::SaveNext => return self.github_save_next(),
            Action::Sent(key, sent_at, result) => return self.github_sent(key, sent_at, result),
            Action::Recorded(key, result) => {
                if let Err(error) = result {
                    self.status = format!("Couldn't keep this version: {error}");
                    self.error = true;
                }
                self.github_refresh(&key);
                return self.versions_reload_local();
            }
            Action::Persisted(key, result) => {
                if let Err(error) = result {
                    self.status = error;
                    self.error = true;
                }
                self.github_refresh(&key);
                return self.versions_reload_local();
            }
            Action::CloseAnswer(send) => return self.github_close_answer(send),
            Action::Review => {
                if let Some((key, _)) = self.github.locate(self.tab.path.as_deref()) {
                    return self.github_review(key, 0);
                }
            }
            Action::ReviewMeta(key, repo_path, commit, found) => {
                let current = self
                    .github
                    .conflicts
                    .get(&key)
                    .and_then(|found| found.first())
                    .is_some_and(|conflict| conflict.commit == commit);
                // A late failure never erases details already shown.
                if let Some(Dialog::Conflict(review)) = &mut self.github.dialog
                    && current
                    && found.is_some()
                    && review.key == key
                    && review.repo_path == repo_path
                {
                    review.theirs = found;
                }
            }
            Action::Resolve(choice) => return self.github_resolve(choice),
            Action::Settle(key, choice) => return self.github_settle(key, choice),
            Action::ShowVersions => {
                self.github.menu = false;
                if self.github.locate(self.tab.path.as_deref()).is_none() {
                    return Task::none();
                }
                let top = (self.inspector_tab != InspectorTab::Versions).then(|| {
                    iced::widget::operation::snap_to(
                        "inspector-content",
                        iced::widget::operation::RelativeOffset { x: 0., y: 0. },
                    )
                });
                let task = self.show_inspector_tab(InspectorTab::Versions);
                return Task::batch([
                    task.unwrap_or_else(Task::none),
                    top.unwrap_or_else(Task::none),
                    self.github_follow(),
                ]);
            }
            Action::Versions(action) => return self.versions_action(action),
            Action::Color(key, hue) => {
                self.tabs.color_menu = None;
                if let Some(id) = self.github.repo_id(&key) {
                    self.github.prefs.colors.insert(id, hue % identity::HUES);
                    return self.github_keep_prefs();
                }
            }
            Action::Kept(result) => {
                if let Err(error) = result {
                    self.status = format!("Couldn't keep the repository colors: {error}");
                    self.error = true;
                }
            }
            Action::CopyLink => {
                if let Some(url) = self.github_link() {
                    self.status = "Link copied".into();
                    self.error = false;
                    return iced::clipboard::write(url);
                }
            }
            Action::OpenOnGitHub => {
                self.github.chip_menu = false;
                if let Some(url) = self.github_link() {
                    return open_url(url);
                }
            }
            Action::CopyTabLink(id) => {
                self.tabs.color_menu = None;
                let path = self
                    .strip()
                    .find(|tab| tab.id == id)
                    .and_then(|tab| tab.path.clone());
                if let Some(url) = self.github_link_for(path.as_deref()) {
                    self.status = "Link copied".into();
                    self.error = false;
                    return iced::clipboard::write(url);
                }
            }
            Action::CopyRequest(key) => {
                self.github.chip_menu = false;
                self.status = "Request copied · Send it to the owner".into();
                self.error = false;
                return iced::clipboard::write(self.github.request(&key));
            }
            Action::OpenRepo(key) => {
                self.github.chip_menu = false;
                let path = format!("/{}/{}", key.owner, key.repo);
                return open_url(ends.web(&api::encoded(&path)));
            }
            Action::SaveCopy => {
                self.github.chip_menu = false;
                let show = self.picker_action(picker::Action::Show(Mode::SendTo));
                if let Some(Dialog::Picker(picker)) = &mut self.github.dialog {
                    picker.copy = true;
                }
                return show;
            }
            Action::ChipMenu(open) => self.github.chip_menu = open,
            Action::OpenLatest => {
                let Some((key, repo_path)) = self.github.old(self.tab.path.as_deref()) else {
                    return Task::none();
                };
                let branch = self
                    .github_state(&key)
                    .map(|state| state.branch.clone())
                    .unwrap_or_default();
                let named = self.github.named(&key);
                let path = format!("/{}/{}/blob/{branch}/{repo_path}", named.owner, named.repo);
                let url = ends.web(&api::encoded(&path));
                return self.picker_action(picker::Action::Link(url));
            }
            Action::Focused => {
                let listed = self.picker_returned();
                return Task::batch([listed, self.github_recheck()]);
            }
            Action::Access(generation, key, result) => {
                // Access given since: local changes count as unsent now.
                if let Ok(repo) = result
                    && generation == self.github.generation
                    && repo.push
                    && let Some(state) = self.github.states.get_mut(&key)
                    && state.read_only
                    && state.repo_id == repo.id
                {
                    state.read_only = false;
                    let shared = self.github.prefs.is_shared(repo.id);
                    let keep = if shared && self.github.prefs.share(&repo) {
                        self.github_keep_prefs()
                    } else {
                        Task::none()
                    };
                    self.github_refresh(&key);
                    return Task::batch([keep, self.github_persist(&key)]);
                }
            }
            Action::Pasted(id, text) => {
                let web = &self.github.ends.web;
                if let Some(link) = text.filter(|text| gh::link::parse(text, web).is_some()) {
                    return self.picker_action(picker::Action::Link(link));
                }
                // The tab Paste was pressed in, even if another is in front now.
                let paste = self.in_tab(id, |app| app.paste_clipboard());
                return super::tagged(paste.unwrap_or_else(Task::none), id);
            }
            Action::OpenKnown(path) => return super::files::open_paths(vec![path]),
        }
        Task::none()
    }

    /// On window focus, at most every five minutes: the read-only
    /// repositories of the open drawings are asked whether the user may send
    /// to them now.
    pub(super) fn github_recheck(&mut self) -> Task<Message> {
        let Some(client) = self.github.client() else {
            return Task::none();
        };
        if self.github.checked.is_some_and(|at| at.elapsed() < RECHECK) {
            return Task::none();
        }
        self.github.checked = Some(Instant::now());
        let generation = self.github.generation;
        let read_only: Vec<(RepoKey, u64)> = self
            .github_open_repos()
            .into_iter()
            .filter_map(|key| {
                let state = self.github.states.get(&key)?;
                state.read_only.then_some((key, state.repo_id))
            })
            .collect();
        Task::batch(read_only.into_iter().map(|(key, id)| {
            let client = client.clone();
            perform(async move { api::repo(&client, id).await }, move |result| {
                Action::Access(generation, key, result)
            })
        }))
    }

    /// Paste with no text field focused: a github.com link on the clipboard
    /// opens; anything else pastes into the drawing.
    pub(super) fn github_paste(&self) -> Option<Task<Message>> {
        let id = self.tab.id;
        (self.github.configured() && !cfg!(test)).then(|| {
            iced::clipboard::read().map(move |text| Message::GitHub(Action::Pasted(id, text)))
        })
    }

    /// A link dropped on the window, as a `.webloc` or `.url` file: it opens.
    pub(super) fn github_dropped(&mut self, paths: &[PathBuf]) -> Option<Task<Message>> {
        let [path] = paths else {
            return None;
        };
        let shortcut = path.extension().is_some_and(|ext| {
            ext.eq_ignore_ascii_case("webloc") || ext.eq_ignore_ascii_case("url")
        });
        if !shortcut || !self.github.configured() {
            return None;
        }
        let bytes = std::fs::read(path).ok()?;
        let link = shortcut_url(&String::from_utf8_lossy(&bytes))?;
        gh::link::parse(&link, &self.github.ends.web)?;
        Some(self.picker_action(picker::Action::Link(link)))
    }

    /// Signed in: the account and the repositories ReShiki may use, each
    /// keeping a color from now on.
    fn github_listed(&mut self, account: gh::Account, remembered: bool) -> Task<Message> {
        let login = account.login.clone();
        self.github.account = if account.needs_install() {
            Account::NeedsInstall { login }
        } else {
            Account::SignedIn { login, remembered }
        };
        self.github.repos = account.repos;
        // A repository the user may send to now is read only no more.
        let pushable: HashSet<u64> = (self.github.repos.iter())
            .filter(|repo| repo.push)
            .map(|repo| repo.id)
            .collect();
        let mut cleared = Vec::new();
        for (key, state) in &mut self.github.states {
            if state.read_only && pushable.contains(&state.repo_id) {
                state.read_only = false;
                cleared.push(key.clone());
            }
        }
        let mut tasks = Vec::new();
        for key in cleared {
            self.github_refresh(&key);
            tasks.push(self.github_persist(&key));
        }
        let repos: Vec<(u64, RepoKey)> = self
            .github
            .repos
            .iter()
            .map(|repo| (repo.id, repo.key.clone()))
            .collect();
        tasks.push(self.github_seen(repos.iter().map(|(id, key)| (*id, key))));
        Task::batch(tasks)
    }

    /// Lists the repositories ReShiki may use.
    fn github_list(&mut self) -> Task<Message> {
        let Some(client) = self.github.client() else {
            return Task::none();
        };
        let generation = self.github.generation;
        perform(async move { api::account(&client).await }, move |result| {
            Action::Listed(generation, result)
        })
    }

    /// The state of the repository whose folder is `key`, read once.
    fn github_state(&mut self, key: &RepoKey) -> Option<&mut RepoState> {
        let root = self.github.root.as_deref()?;
        if !self.github.states.contains_key(key) {
            let state = workcopy::load(root, key).ok()??;
            self.github.states.insert(key.clone(), state);
        }
        self.github.states.get_mut(key)
    }

    /// Recounts the drawings of `key` that differ from what GitHub has.
    fn github_refresh(&mut self, key: &RepoKey) {
        let Some(root) = self.github.root.clone() else {
            return;
        };
        let unsent = self
            .github_state(key)
            .map(|state| workcopy::unsent(&root, key, state).unwrap_or_default());
        self.github
            .unsent
            .insert(key.clone(), unsent.unwrap_or_default());
    }

    fn github_persist(&mut self, key: &RepoKey) -> Task<Message> {
        let (Some(root), Some(state)) = (
            self.github.root.clone(),
            self.github.states.get(key).cloned(),
        ) else {
            return Task::none();
        };
        self.github.seq += 1;
        let (seq, key) = (self.github.seq, key.clone());
        perform(
            {
                let key = key.clone();
                async move { workcopy::persist(&root, &key, state, seq).await }
            },
            move |result| Action::Persisted(key, result),
        )
    }

    /// A drawing just opened: count what is unsent in its repository, and
    /// remember the repository as the one last used.
    pub(super) fn github_opened(&mut self, path: &Path) -> Task<Message> {
        let found = (self.github.locate(Some(path))).or_else(|| self.github.old(Some(path)));
        let Some((key, _)) = found else {
            return Task::none();
        };
        self.github_refresh(&key);
        let Some(id) = self.github.repo_id(&key) else {
            return Task::none();
        };
        let named = self.github.named(&key);
        let colors = self.github_seen([(id, &named)]);
        if self.github.prefs.last == Some(id) {
            return colors;
        }
        self.github.prefs.last = Some(id);
        self.github_keep_prefs()
    }

    /// The open drawings of `key` with unsaved edits, by tab and repository path.
    fn github_edited(&self, key: &RepoKey) -> Vec<(TabId, String)> {
        self.strip()
            .filter(|tab| self.edited(tab))
            .filter_map(|tab| {
                let (other, path) = self.github.locate(tab.path.as_deref())?;
                (other == *key).then_some((tab.id, path))
            })
            .collect()
    }

    /// The drawings of `key` a send would take: saved but unsent, or with
    /// unsaved edits (`edited` tells which tabs have them).
    fn github_waiting(&self, key: &RepoKey, edited: impl Fn(&DocumentTab) -> bool) -> Vec<String> {
        let mut files = self.github.unsent.get(key).cloned().unwrap_or_default();
        for tab in self.strip().filter(|tab| edited(tab)) {
            if let Some((other, path)) = self.github.locate(tab.path.as_deref())
                && other == *key
                && !files.contains(&path)
            {
                files.push(path);
            }
        }
        files
    }

    /// Whether anything of any repository is not on GitHub yet.
    fn github_anything_unsent(&self) -> bool {
        let sendable = |key: &RepoKey| !self.github.read_only(key);
        (self.github.unsent.iter()).any(|(key, unsent)| !unsent.is_empty() && sendable(key))
            || self.strip().any(|tab| {
                self.shown_edited(tab)
                    && (self.github.locate(tab.path.as_deref()))
                        .is_some_and(|(key, _)| sendable(&key))
            })
    }

    /// The repository folders of the open tabs, in strip order.
    fn github_open_repos(&self) -> Vec<RepoKey> {
        let mut open = Vec::new();
        for tab in self.strip() {
            if let Some((key, _)) = self.github.locate(tab.path.as_deref())
                && !open.contains(&key)
            {
                open.push(key);
            }
        }
        open
    }

    /// The repository's name, with its owner only when another open
    /// repository has the same name.
    fn github_label(&self, key: &RepoKey) -> String {
        let named = self.github.named(key);
        let shared = self
            .github_open_repos()
            .iter()
            .any(|other| other != key && self.github.named(other).repo == named.repo);
        if shared {
            format!("{}/{}", named.owner, named.repo)
        } else {
            named.repo
        }
    }

    /// The front drawing's file on GitHub, on its default branch.
    fn github_link(&self) -> Option<String> {
        self.github_link_for(self.tab.path.as_deref())
    }

    /// The file on GitHub, on its default branch, of the drawing at `path`:
    /// a working copy GitHub has, or an old version.
    fn github_link_for(&self, path: Option<&Path>) -> Option<String> {
        let (key, repo_path, working) = match self.github.locate(path) {
            Some((key, repo_path)) => (key, repo_path, true),
            None => {
                let (key, repo_path) = self.github.old(path)?;
                (key, repo_path, false)
            }
        };
        let state = self.github.states.get(&key)?;
        if working {
            state.files.get(&repo_path)?.base_blob.as_ref()?;
        }
        let path = format!(
            "/{}/{}/blob/{}/{repo_path}",
            state.owner, state.repo, state.branch
        );
        Some(self.github.ends.web(&api::encoded(&path)))
    }

    /// Saves the next drawing the Send waits for, in its own tab as Cmd+S
    /// does; once all are saved, sends. A save already writing calls back.
    /// Each drawing is saved once, so one that stays edited cannot loop.
    fn github_save_next(&mut self) -> Task<Message> {
        let Some((_, _, tabs)) = &self.github.after_save else {
            return Task::none();
        };
        if self.file_io.saving {
            return Task::none();
        }
        let next = tabs
            .iter()
            .copied()
            .find(|id| self.strip().any(|tab| tab.id == *id && self.edited(tab)));
        if let Some(id) = next {
            if let Some((_, _, tabs)) = &mut self.github.after_save {
                tabs.retain(|tab| *tab != id);
            }
            let save = self.in_tab(id, |app| {
                if app.finish_inline(true) {
                    app.save_drawing(None)
                } else {
                    Task::none()
                }
            });
            if !self.file_io.saving {
                // Not started, as for a text draft that cannot apply: the
                // tab says why, and the Send waits for the user.
                self.github.after_save = None;
            }
            return super::tagged(save.unwrap_or_else(Task::none), id);
        }
        let Some((key, message, _)) = self.github.after_save.take() else {
            return Task::none();
        };
        self.github_send(key, message, None)
    }

    /// After every save of an open tab, with that tab in front: a GitHub
    /// drawing keeps what was written as a version, and a Send or a choice
    /// waiting for this save goes on.
    pub(super) fn github_saved(
        &mut self,
        saved: bool,
        written: Option<(PathBuf, reshiki::document::Document)>,
    ) -> Task<Message> {
        if !saved {
            // A save failed or was canceled: what waited for it waits for the user.
            self.github.after_save = None;
            self.github.resolving = None;
        }
        let next = match self.github.after_save {
            Some(_) => Task::done(Message::GitHub(Action::SaveNext)),
            None => Task::none(),
        };
        let Some((path, doc)) = written else {
            return next;
        };
        let resolve = self.github_resolve_saved(&path);
        let Some((key, repo_path)) = self.github.locate(Some(&path)) else {
            return Task::batch([next, resolve]);
        };
        let root = self.github.root.clone().unwrap_or_default();
        let base = self
            .github_state(&key)
            .and_then(|state| state.files.get(&repo_path))
            .map(|file| file.base_hash.clone());
        let record = perform(
            {
                let key = key.clone();
                async move {
                    let bytes = doc.file_json()?;
                    workcopy::snapshot(&root, &key, &repo_path, bytes, base).await
                }
            },
            {
                let key = key.clone();
                move |result| Action::Recorded(key, result)
            },
        );
        self.github_refresh(&key);
        Task::batch([record, next, resolve])
    }

    /// One commit of every unsent drawing of `key` (or of those in `only`),
    /// read from its saved file.
    fn github_send(
        &mut self,
        key: RepoKey,
        message: String,
        only: Option<Vec<String>>,
    ) -> Task<Message> {
        let Some(client) = self.github.client() else {
            return self.github_sign_in(None);
        };
        let (Some(root), Some(state)) =
            (self.github.root.clone(), self.github_state(&key).cloned())
        else {
            return Task::none();
        };
        if !self.github.sending.insert(key.clone()) {
            return Task::none();
        }
        self.github.failed.remove(&key);
        let sent_at = workcopy::now_id();
        perform(
            {
                let key = key.clone();
                async move {
                    let mut outgoing = {
                        let (root, key, state) = (root.clone(), key.clone(), state.clone());
                        reshiki::task::spawn_blocking(move || send::outgoing(&root, &key, &state))
                            .await
                            .map_err(|error| gh::Error::Rejected(error.to_string()))?
                            .map_err(gh::Error::Rejected)?
                    };
                    if let Some(only) = only {
                        outgoing.retain(|file| only.contains(&file.repo_path));
                    }
                    send::send(&client, state.repo_id, &message, &outgoing).await
                }
            },
            move |result| Action::Sent(key, sent_at, result),
        )
    }

    fn github_sent(
        &mut self,
        key: RepoKey,
        sent_at: String,
        result: Result<send::SendOutcome, gh::Error>,
    ) -> Task<Message> {
        self.github.sending.remove(&key);
        // What a conflict choice kept, said before the send that followed it.
        let kept = self.github.kept.remove(&key);
        let mut tasks = Vec::new();
        match result {
            Ok(outcome) => {
                let changed = match outcome.conflicts.as_slice() {
                    [] => None,
                    [one] => Some(file_name(&one.repo_path).to_owned()),
                    all => Some(drawings(all.len())),
                };
                let status = match (changed, outcome.sent.len(), kept) {
                    (Some(changed), 0, _) => format!("Not sent · {changed} changed on GitHub"),
                    (Some(changed), sent, _) => {
                        format!("Sent {sent} · {changed} changed on GitHub")
                    }
                    (None, _, Some(kept)) => format!("{kept} · sent"),
                    (None, ..) => "Sent to GitHub".into(),
                };
                let (Some(root), Some(state)) = (self.github.root.clone(), self.github_state(&key))
                else {
                    return Task::none();
                };
                send::apply(state, &outcome);
                let state = state.clone();
                self.github
                    .conflicts
                    .insert(key.clone(), outcome.conflicts.clone());
                self.github.seq += 1;
                let seq = self.github.seq;
                let question = !outcome.conflicts.is_empty();
                tasks.push(perform(
                    {
                        let key = key.clone();
                        async move {
                            let mut state = state;
                            send::record(&root, &key, &mut state, seq, &outcome, &sent_at).await
                        }
                    },
                    {
                        let key = key.clone();
                        move |result| Action::Persisted(key, result)
                    },
                ));
                self.github_refresh(&key);
                // Versions lists again, with the new commit.
                if self
                    .github
                    .versions
                    .target
                    .as_ref()
                    .is_some_and(|(at, _)| *at == key)
                {
                    self.github.versions.target = None;
                }
                // A drawing changed on GitHub is a question, not an error.
                self.error = false;
                self.github.amber = question.then(|| status.clone());
                self.status = status;
            }
            Err(error) => {
                let reason = self.github_failed(&error);
                self.status = format!("Not sent: {reason}");
                self.error = true;
                self.github.failed.insert(key.clone(), reason);
            }
        }
        tasks.push(self.github_close_after_send(&key));
        // GitHub's and yours, side by side, at once.
        if self.github.dialog.is_none()
            && self
                .github
                .conflicts
                .get(&key)
                .is_some_and(|found| !found.is_empty())
        {
            tasks.push(self.github_review(key, 0));
        }
        Task::batch(tasks)
    }

    fn github_conflicts(&self, key: &RepoKey) -> usize {
        self.github.states.get(key).map_or(0, |state| {
            state
                .files
                .values()
                .filter(|file| file.conflict.is_some())
                .count()
        })
    }

    /// Whether the saved file of a GitHub drawing differs from GitHub's.
    fn github_unsent(&mut self, path: Option<&Path>) -> Option<RepoKey> {
        let (key, repo_path) = self.github.locate(path)?;
        // Nothing of a read-only repository can be sent.
        if self.github.read_only(&key) {
            return None;
        }
        self.github_refresh(&key);
        self.github
            .unsent
            .get(&key)
            .is_some_and(|unsent| unsent.contains(&repo_path))
            .then_some(key)
    }

    /// Before a tab closes: asks whether to send its saved, unsent drawing.
    /// `None` closes it now.
    pub(super) fn github_before_close(&mut self, id: TabId) -> Option<Task<Message>> {
        if self.github.later.remove(&id) || self.github.closing.is_some() {
            return None;
        }
        let path = self.strip().find(|tab| tab.id == id)?.path.clone();
        let key = self.github_unsent(path.as_deref())?;
        if let Some(index) = self.tab_index(id) {
            self.select_tab(index);
        }
        self.github.closing = Some(Closing::Tab(id, self.tab.revision));
        // Send takes every unsent drawing of the repository: say so.
        let body = match self.github.unsent.get(&key).map_or(0, Vec::len) {
            count if count > 1 => format!(
                "Sends {count} drawings in {}.",
                self.github.named(&key).repo
            ),
            _ => "Saved on this computer only.".into(),
        };
        Some(ask_send(
            format!("Send “{}” to GitHub?", self.tab.name()),
            body,
        ))
    }

    /// Before the window closes, once every Save question is answered.
    pub(super) fn github_before_quit(
        &mut self,
        window: iced::window::Id,
        discarded: &[TabId],
    ) -> Option<Task<Message>> {
        if std::mem::take(&mut self.github.window_answered) {
            return None;
        }
        // A close already waits for an answer or a send: this one waits too.
        if self.github.closing.is_some() {
            return Some(Task::none());
        }
        let paths: Vec<_> = self.strip().filter_map(|tab| tab.path.clone()).collect();
        let keys: HashSet<RepoKey> = paths
            .iter()
            .filter_map(|path| self.github.locate(Some(path)).map(|(key, _)| key))
            .filter(|key| !self.github.read_only(key))
            .collect();
        let mut count = 0;
        for key in &keys {
            self.github_refresh(key);
            count += self.github.unsent.get(key).map_or(0, Vec::len);
        }
        if count == 0 {
            return None;
        }
        self.github.closing = Some(Closing::Window(window, discarded.to_vec(), keys));
        Some(ask_send(
            format!("Send {} to GitHub?", drawings(count)),
            "Saved on this computer only.".into(),
        ))
    }

    fn github_close_answer(&mut self, send: bool) -> Task<Message> {
        let Some(closing) = self.github.closing.clone() else {
            return Task::none();
        };
        match (closing, send) {
            (Closing::Tab(id, _), false) => {
                self.github.closing = None;
                self.github_close_now(id)
            }
            (Closing::Window(window, discarded, _), false) => {
                self.github.closing = None;
                self.github.window_answered = true;
                self.close_window(window, discarded)
            }
            (Closing::Tab(id, _), true) => {
                let path = self
                    .strip()
                    .find(|tab| tab.id == id)
                    .and_then(|tab| tab.path.clone());
                match self.github.locate(path.as_deref()) {
                    Some((key, _)) => self.github_send_unsent(key),
                    None => {
                        self.github.closing = None;
                        Task::none()
                    }
                }
            }
            (Closing::Window(_, _, keys), true) => {
                Task::batch(keys.into_iter().map(|key| self.github_send_unsent(key)))
            }
        }
    }

    /// Sends from the saved files, never saving a drawing whose changes the
    /// user just discarded.
    fn github_send_unsent(&mut self, key: RepoKey) -> Task<Message> {
        // Signed out: sign in first, then answer Send again.
        if self.github.client().is_none() {
            return self.github_sign_in(Some(Action::CloseAnswer(true)));
        }
        self.github_refresh(&key);
        let files = self.github.unsent.get(&key).cloned().unwrap_or_default();
        let task = self.github_send(key.clone(), send::default_message(&files), None);
        if !self.github.sending.contains(&key) {
            // Not signed in, or nothing to send: the close waits for the user.
            self.github.closing = None;
        }
        task
    }

    /// Finishes a close that waited for `key` to send: only when nothing is
    /// left unsent, in conflict or edited since the question.
    fn github_close_after_send(&mut self, key: &RepoKey) -> Task<Message> {
        match self.github.closing.clone() {
            Some(Closing::Tab(id, revision)) => {
                let tab = self.strip().find(|tab| tab.id == id);
                let (path, edited) = match tab {
                    Some(tab) => (tab.path.clone(), tab.revision != revision),
                    None => (None, true),
                };
                self.github.closing = None;
                if edited
                    || self.github_unsent(path.as_deref()).is_some()
                    || self.github_conflicts(key) > 0
                {
                    return Task::none();
                }
                self.github_close_now(id)
            }
            Some(Closing::Window(window, discarded, mut keys)) => {
                keys.remove(key);
                if !keys.is_empty() {
                    self.github.closing = Some(Closing::Window(window, discarded, keys));
                    return Task::none();
                }
                self.github.closing = None;
                // Saves in a read-only repository never hold the window open.
                let left = self.github.states.keys().any(|key| {
                    !self.github.read_only(key)
                        && (self.github.unsent.get(key).is_some_and(|u| !u.is_empty())
                            || self.github_conflicts(key) > 0)
                });
                if left || !self.github.failed.is_empty() {
                    self.cancel_close();
                    return Task::none();
                }
                self.github.window_answered = true;
                self.close_window(window, discarded)
            }
            None => Task::none(),
        }
    }

    /// Closes tab `id` without asking about GitHub again.
    fn github_close_now(&mut self, id: TabId) -> Task<Message> {
        self.github.later.insert(id);
        match self.tab_index(id) {
            Some(index) => {
                self.select_tab(index);
                self.close_active_tab()
            }
            None => Task::none(),
        }
    }

    /// The panel and the Versions list follow the drawing in front.
    pub(super) fn github_follow(&mut self) -> Task<Message> {
        if self.inspector_tab != InspectorTab::Versions {
            return Task::none();
        }
        let front = self.github.locate(self.tab.path.as_deref());
        // Only a GitHub drawing has History.
        if front.is_none() {
            self.inspector_tab = InspectorTab::Properties;
        } else if !self.inspector_open {
            return Task::none();
        }
        if front == self.github.versions.target {
            return Task::none();
        }
        self.versions_load(front)
    }

    /// A new tab of a repository opens after that repository's last tab.
    pub(super) fn github_place_tab(&mut self, path: &Path) {
        let Some((key, _)) = self.github.locate(Some(path)) else {
            return;
        };
        let last = self
            .strip()
            .enumerate()
            .filter(|(_, tab)| tab.id != self.tab.id)
            .filter(|(_, tab)| {
                self.github
                    .locate(tab.path.as_deref())
                    .is_some_and(|(other, _)| other == key)
            })
            .map(|(index, _)| index)
            .last();
        if let Some(last) = last
            && last < self.tabs.active
        {
            self.tabs.active = last + 1;
        }
    }

    /// The Send row of the menu and the palette: its label, its detail, and
    /// its action, `None` while there is nothing to send.
    fn github_send_row(&self) -> (String, String, Option<Action>) {
        if self.github.old(self.tab.path.as_deref()).is_some() {
            return (
                SAVE_COPY.into(),
                "Old version".into(),
                Some(Action::SaveCopy),
            );
        }
        let Some((key, _)) = self.github.locate(self.tab.path.as_deref()) else {
            return (
                SEND_TO.into(),
                "Choose a repository".into(),
                Some(Action::ShowSend),
            );
        };
        if self.github.read_only(&key) {
            return (SAVE_COPY.into(), "Read only".into(), Some(Action::SaveCopy));
        }
        let files = self.github_waiting(&key, |tab| self.shown_edited(tab));
        let repo = self.github.named(&key).repo;
        match files.as_slice() {
            [] => ("Nothing to send".into(), String::new(), None),
            [one] => (
                "Send 1 drawing".into(),
                format!("{} → {repo}", file_name(one)),
                Some(Action::ShowSend),
            ),
            all => (
                format!("Send {}", drawings(all.len())),
                format!("{} → {repo}", drawings(all.len())),
                Some(Action::ShowSend),
            ),
        }
    }

    /// The GitHub rows of the command palette, only those that apply now:
    /// name, detail and action (`None`: shown but unavailable).
    pub(super) fn github_palette(&self) -> Vec<(String, String, Option<Action>)> {
        if !self.github.configured() {
            return Vec::new();
        }
        let mut rows = vec![(
            OPEN_FROM.into(),
            "Choose a repository and a drawing".into(),
            Some(Action::Picker(picker::Action::Show(Mode::Open))),
        )];
        rows.push(self.github_send_row());
        if let Some((key, repo_path)) = self.github.locate(self.tab.path.as_deref()) {
            let name = file_name(&repo_path).to_owned();
            if self.github_conflicts(&key) > 0 {
                rows.push((
                    "Review changes on GitHub".into(),
                    name.clone(),
                    Some(Action::Review),
                ));
            }
            rows.push(("History".into(), name.clone(), Some(Action::ShowVersions)));
            // A drawing GitHub has never had has no link yet.
            if self.github_link().is_some() {
                rows.push((
                    "Copy GitHub link".into(),
                    name.clone(),
                    Some(Action::CopyLink),
                ));
                rows.push(("Open on GitHub ↗".into(), name, Some(Action::OpenOnGitHub)));
            }
        }
        rows.push(match self.github.login() {
            Some(login) => (
                "Sign out of GitHub".into(),
                format!("@{login}"),
                Some(Action::SignOut),
            ),
            None => (
                "Sign in to GitHub…".into(),
                String::new(),
                Some(Action::SignIn),
            ),
        });
        // Every repository and drawing known here, read from this computer.
        // `lab-notes/ring`, or, where two would read the same (the palette
        // keeps one row per name), `octo-test/lab-notes/scheme/ring`.
        let stem = |path: &str| path.strip_suffix(".rsk").unwrap_or(path).to_owned();
        let mut known = Vec::new();
        let root = self.github.root.as_deref();
        for (folder, state) in root.map(workcopy::known).into_iter().flatten() {
            let (owner, repo) = (&state.owner, &state.repo);
            let at = Action::Picker(picker::Action::ShowIn(state.key(), String::new()));
            let detail = format!("{owner}/{repo}");
            known.push((repo.clone(), detail.clone(), detail, at));
            for repo_path in state.files.keys() {
                let path =
                    workcopy::working_path(root.unwrap_or(Path::new("")), &folder, repo_path);
                known.push((
                    format!("{repo}/{}", stem(file_name(repo_path))),
                    format!("{owner}/{repo}/{}", stem(repo_path)),
                    format!("{owner}/{repo} · {repo_path}"),
                    Action::OpenKnown(path),
                ));
            }
        }
        for (short, long, detail, action) in &known {
            // Long names never meet each other: a repository's has two parts,
            // a drawing's three or more.
            let twice = known.iter().any(|(other, other_long, ..)| {
                (other == short && other_long != long) || other_long == short
            });
            rows.push((
                format!("{OPEN_FROM_ROW}{}", if twice { long } else { short }),
                detail.clone(),
                Some(action.clone()),
            ));
        }
        rows
    }

    /// The palette row for a github.com link typed or pasted there.
    pub(super) fn github_link_row(&self, query: &str) -> Option<(String, String, Action)> {
        let found = gh::link::parse(query, &self.github.ends.web)?;
        let place: Vec<&str> = std::iter::once(found.key.repo.as_str())
            .chain(found.path.split('/').filter(|name| !name.is_empty()))
            .collect();
        let place = place.join("/");
        let mut detail = format!("{}/{}", found.key.owner, found.key.repo);
        if let Some(reference) = &found.reference {
            detail = format!("{detail} · {reference}");
        }
        Some((
            format!(
                "{OPEN_FROM_ROW}{}",
                place.strip_suffix(".rsk").unwrap_or(&place)
            ),
            detail,
            Action::Picker(picker::Action::Link(query.trim().to_owned())),
        ))
    }

    /// The GitHub button in the command bar, with its menu: what applies to
    /// the drawing in front first, the account last. A dot while anything
    /// is not on GitHub yet.
    pub(super) fn github_button(&self) -> Element<'_, Message> {
        let state = &self.github;
        let unsent = self.github_anything_unsent();
        let mut hint = match state.login() {
            Some(login) => format!("GitHub · Signed in as @{login}"),
            None => "GitHub · Not signed in".to_owned(),
        };
        if unsent {
            hint.push_str(" · Not everything is sent");
        }
        let anchor = super::workspace::icon_button(
            super::icons::Icon::Cloud,
            hint,
            Some(Message::GitHub(Action::Menu(!state.menu))),
            state.menu,
        );
        // Signed in: a disc with the account's initial; unsent: a dot.
        let account = state.login().map(|login| {
            let initial = login
                .chars()
                .next()
                .unwrap_or('?')
                .to_uppercase()
                .to_string();
            container(
                container(text(initial).size(7.5).style(|theme: &Theme| text::Style {
                    color: Some(theme.palette().background),
                }))
                .center(11)
                .style(|theme: &Theme| container::Style {
                    background: Some(theme.palette().text.into()),
                    border: Border {
                        radius: 5.5.into(),
                        width: 1.,
                        color: theme.palette().background,
                    },
                    ..container::Style::default()
                }),
            )
            .padding([4, 3])
            .align_right(36)
            .align_bottom(36)
        });
        let dot = unsent.then(|| {
            container(identity::dot(|theme: &Theme| theme.palette().primary))
                .padding([6, 5])
                .align_right(36)
                .align_top(36)
        });
        let anchor: Element<'_, Message> = iced::widget::Stack::with_children([anchor])
            .extend(account.map(Element::from))
            .extend(dot.map(Element::from))
            .into();
        let popup = state.menu.then(|| {
            let configured = state.configured();
            let item = |id: &'static str, label: String, action: Option<Action>| {
                reshiki::accessibility::button(id, label.clone(), text(label).size(12))
                    .width(Length::Fill)
                    .padding([6, 10])
                    .style(super::workspace::control(false))
                    .on_press_maybe(action.map(Message::GitHub))
            };
            let muted = |line: String| {
                container(text(line).size(11).style(super::workspace::muted_text)).padding([6, 10])
            };
            let (send, _, send_action) = self.github_send_row();
            let mut items = column![
                item(
                    "github-open",
                    OPEN_FROM.into(),
                    configured.then_some(Action::Picker(picker::Action::Show(Mode::Open)))
                ),
                item("github-send", send, send_action.filter(|_| configured)),
            ]
            .spacing(1);
            if configured && state.locate(self.tab.path.as_deref()).is_some() {
                items = items.push(item(
                    "github-versions",
                    "History".into(),
                    Some(Action::ShowVersions),
                ));
            }
            items = items.push(super::workspace::horizontal_line());
            items = match (state.login(), configured) {
                (Some(login), _) => items
                    .push(muted(format!("@{login}")))
                    .push(item(
                        "github-repositories",
                        "Add repositories… ↗".into(),
                        Some(Action::ChooseRepositories),
                    ))
                    .push(item(
                        "github-sign-out",
                        "Sign out".into(),
                        Some(Action::SignOut),
                    )),
                (None, true) => items.push(item(
                    "github-sign-in",
                    "Sign in…".into(),
                    Some(Action::SignIn),
                )),
                (None, false) => items.push(muted(NOT_SET_UP.into())),
            };
            container(items)
                .width(220)
                .padding(5)
                .style(super::color_popover::surface)
                .into()
        });
        Element::new(super::popover::popover(
            anchor,
            popup,
            Message::GitHub(Action::Menu(false)),
        ))
    }

    /// The status bar chip for the drawing in front, if it is on GitHub: the
    /// repository's dot and name, then what is left to do. The Send popover
    /// opens above it.
    pub(super) fn github_chip(&self) -> Option<Element<'_, Message>> {
        let front = self.tab.path.as_deref();
        let (key, repo_path, old) = match self.github.locate(front) {
            Some((key, repo_path)) => (key, repo_path, false),
            None => {
                let (key, repo_path) = self.github.old(front)?;
                (key, repo_path, true)
            }
        };
        let state = &self.github;
        let folder = repo_path
            .rsplit_once('/')
            .map(|(folder, _)| folder.to_owned());
        // `lab-notes / scheme`, with a long folder shortened in the middle.
        let name = match &folder {
            Some(folder) => format!("{} / {}", self.github_label(&key), middle(folder)),
            None => self.github_label(&key),
        };
        let hue = state.hue(&key);
        let muted = |line: String| text(line).size(12).style(super::workspace::muted_text);
        let action = |id: &'static str, label: &'static str, action: Action| {
            super::workspace::accessible_command(id, label, label)
                .on_press(Message::GitHub(action))
                .padding([4, 9])
        };
        // The name opens the picker in the drawing's folder.
        let full = match &folder {
            Some(folder) => format!("{} / {folder}", self.github_label(&key)),
            None => self.github_label(&key),
        };
        let place = reshiki::accessibility::button(
            "github-chip-place",
            format!("{full}, open from GitHub here"),
            row![
                identity::dot(move |theme: &Theme| identity::color(theme, hue)),
                text(name.clone()).size(12),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .padding([4, 8])
        .style(super::workspace::control(false))
        .on_press(Message::GitHub(Action::Picker(picker::Action::ShowIn(
            state.named(&key),
            folder.unwrap_or_default(),
        ))));
        if old || state.read_only(&key) {
            return Some(self.github_read_only_chip(place, &key, old));
        }
        let mut content = row![].spacing(6).align_y(Alignment::Center);
        let state_now = self.github_chip_state(&key);
        let (suffix, after, said) = match state_now {
            Chip::Sending => (Some("· Sending…".to_owned()), None, "sending".to_owned()),
            Chip::Review => (
                None,
                Some(action("github-review", "Review", Action::Review).style(amber_button)),
                "changed on GitHub".to_owned(),
            ),
            Chip::NotSent => (
                Some("· Not sent".to_owned()),
                Some(
                    action("github-retry", "Retry", Action::ShowSend)
                        .style(super::workspace::control(true)),
                ),
                "not sent".to_owned(),
            ),
            Chip::New | Chip::Unsent(_) => {
                let said = match state_now {
                    Chip::Unsent(count) => format!("{count} unsent"),
                    _ => "not sent yet".to_owned(),
                };
                (
                    Some(format!("· {said}")),
                    Some(
                        action("github-send-chip", "Send", Action::ShowSend)
                            .style(super::workspace::control(true)),
                    ),
                    said,
                )
            }
            Chip::Sent => (None, None, "everything sent".to_owned()),
        };
        if let Some(suffix) = suffix {
            content = content.push(muted(suffix));
        }
        if state_now == Chip::Sent {
            content = content.push(text("✓").size(12).style(|theme: &Theme| text::Style {
                color: Some(theme.palette().success),
            }));
        }
        // The state opens History; Send and Review have their own buttons.
        let spoken = format!("{said}, history");
        let shown = suffix_shown(state_now);
        let chip = row![place]
            .push(shown.then(|| {
                reshiki::accessibility::button("github-chip", spoken, content)
                    .padding([4, 6])
                    .style(super::workspace::control(false))
                    .on_press(Message::GitHub(Action::ShowVersions))
            }))
            .push(after)
            .spacing(2)
            .align_y(Alignment::Center);
        let popup = match &state.dialog {
            Some(Dialog::Send(draft)) => Some(self.send_view(draft)),
            _ => None,
        };
        Some(Element::new(
            super::popover::popover(chip, popup, Message::GitHub(Action::Close)).align_end(),
        ))
    }

    /// The chip of a drawing nothing is sent from: an old version, with Open
    /// latest; or a read-only repository's, whose menu offers the way on.
    fn github_read_only_chip<'a>(
        &'a self,
        place: impl Into<Element<'a, Message>>,
        key: &RepoKey,
        old: bool,
    ) -> Element<'a, Message> {
        let muted = |line: String| text(line).size(12).style(super::workspace::muted_text);
        if old {
            let latest = super::workspace::accessible_command(
                "github-open-latest",
                "Open latest",
                "Open latest",
            )
            .on_press(Message::GitHub(Action::OpenLatest))
            .padding([4, 9])
            .style(super::workspace::control(true));
            return row![place.into(), muted("· Old version".into()), latest]
                .spacing(6)
                .align_y(Alignment::Center)
                .into();
        }
        let changes = match self.github_waiting(key, |tab| self.shown_edited(tab)).len() {
            0 => String::new(),
            1 => "· 1 change".into(),
            n => format!("· {n} changes"),
        };
        let open = self.github.chip_menu;
        let state = reshiki::accessibility::button(
            "github-chip-read-only",
            format!("read only {}, more", changes.trim_start_matches("· ")),
            row![
                picker::read_only_label(),
                muted(changes),
                super::workspace::caret(10.)
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .expanded(open)
        .padding([4, 6])
        .style(super::workspace::control(open))
        .on_press(Message::GitHub(Action::ChipMenu(!open)));
        let menu = open.then(|| {
            let item = |id: &'static str, label: &'static str, action: Action| {
                reshiki::accessibility::button(id, label, text(label).size(12))
                    .width(Length::Fill)
                    .padding([6, 10])
                    .style(super::workspace::control(false))
                    .on_press(Message::GitHub(action))
            };
            container(
                column![
                    item("github-save-copy", SAVE_COPY, Action::SaveCopy),
                    item(
                        "github-copy-request",
                        "Copy request for the owner",
                        Action::CopyRequest(self.github.named(key)),
                    ),
                    item("github-open-on", "Open on GitHub ↗", Action::OpenOnGitHub),
                ]
                .spacing(1),
            )
            .width(230)
            .padding(5)
            .style(super::color_popover::surface)
            .into()
        });
        let chip = row![place.into(), state]
            .spacing(2)
            .align_y(Alignment::Center);
        Element::new(
            super::popover::popover(chip, menu, Message::GitHub(Action::ChipMenu(false)))
                .align_end(),
        )
    }

    /// What the chip says about repository `key`.
    pub(super) fn github_chip_state(&self, key: &RepoKey) -> Chip {
        let state = &self.github;
        let count = self.github_waiting(key, |tab| self.shown_edited(tab)).len();
        // A drawing made here that GitHub has never had.
        let new = self
            .github
            .locate(self.tab.path.as_deref())
            .and_then(|(_, path)| state.states.get(key)?.files.get(&path).cloned())
            .is_some_and(|file| file.base_blob.is_none());
        if state.sending.contains(key) {
            Chip::Sending
        } else if self.github_conflicts(key) > 0 {
            Chip::Review
        } else if state.failed.contains_key(key) {
            Chip::NotSent
        } else if count == 1 && new {
            Chip::New
        } else if count > 0 {
            Chip::Unsent(count)
        } else {
            Chip::Sent
        }
    }

    /// A tab's repository badge and names; `None` for a drawing on this
    /// computer only.
    pub(super) fn github_tab(
        &self,
        tab: &DocumentTab,
        menu: bool,
    ) -> Option<super::tabs::RepoBadge<'_>> {
        let (key, repo_path, old) = match self.github.locate(tab.path.as_deref()) {
            Some((key, repo_path)) => (key, repo_path, false),
            None => {
                let (key, repo_path) = self.github.old(tab.path.as_deref())?;
                (key, repo_path, true)
            }
        };
        let named = self.github.named(&key);
        let hue = self.github.hue(&key);
        let letters = identity::monograms(&self.github_open_repos())
            .remove(&key)
            .unwrap_or_else(|| identity::monogram(&named.repo));
        let unsent = self
            .github_waiting(&key, |tab| self.shown_edited(tab))
            .len();
        // Nothing is sent from a read-only repository or an old version.
        let read_only = old || self.github.read_only(&key);
        let spoken = match (old, read_only, unsent) {
            (true, ..) => format!("{}, old version", self.github_label(&key)),
            (_, true, _) => format!("{}, read only", self.github_label(&key)),
            (_, _, 0) => self.github_label(&key),
            (_, _, n) => format!("{}, {n} unsent", self.github_label(&key)),
        };
        let link = self.github_link_for(tab.path.as_deref()).is_some();
        let menu = menu.then(|| {
            let mut items = column![].spacing(1);
            if link {
                items = items.push(
                    reshiki::accessibility::button(
                        "github-tab-copy-link",
                        "Copy GitHub link",
                        text("Copy GitHub link").size(12),
                    )
                    .width(Length::Fill)
                    .padding([6, 10])
                    .style(super::workspace::control(false))
                    .on_press(Message::GitHub(Action::CopyTabLink(tab.id))),
                );
                items = items.push(super::workspace::horizontal_line());
            }
            items = items.push(
                container(text("Color").size(11).style(super::workspace::muted_text))
                    .padding([4, 10]),
            );
            for (at, label) in identity::NAMES.iter().enumerate() {
                items = items.push(
                    reshiki::accessibility::button(
                        format!("github-color-{at}"),
                        *label,
                        row![
                            identity::dot(move |theme: &Theme| identity::color(theme, at)),
                            text(*label).size(12)
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center),
                    )
                    .checked(at == hue)
                    .width(Length::Fill)
                    .padding([6, 10])
                    .style(super::workspace::control(at == hue))
                    .on_press(Message::GitHub(Action::Color(key.clone(), at))),
                );
            }
            container(items)
                .width(160)
                .padding(5)
                .style(super::color_popover::surface)
                .into()
        });
        Some(super::tabs::RepoBadge {
            badge: identity::badge(hue, letters, read_only),
            underline: container(Space::new().width(Length::Fill).height(2))
                .style(move |theme: &Theme| container::Style {
                    background: Some(identity::color(theme, hue).into()),
                    ..container::Style::default()
                })
                .into(),
            hint: match old {
                true => format!("{}/{} · {repo_path} · Old version", named.owner, named.repo),
                false => format!("{}/{} · {repo_path}", named.owner, named.repo),
            },
            spoken,
            menu,
        })
    }

    /// The repository of the drawing in front, for the window title.
    pub(super) fn github_title(&self) -> Option<String> {
        let front = self.tab.path.as_deref();
        let (key, _) = (self.github.locate(front)).or_else(|| self.github.old(front))?;
        Some(self.github.named(&key).repo)
    }

    /// Whether the status chip is there to hold the Send popover: the theme
    /// editor replaces the status bar, and a drawing not on GitHub has no chip.
    pub(super) fn send_on_chip(&self) -> bool {
        !(self.inspector_tab == InspectorTab::ThemeGenerator && self.theme_library.editor.is_some())
            && self.github.locate(self.tab.path.as_deref()).is_some()
    }

    /// The open GitHub dialog over everything else. Send is a popover of the
    /// status chip instead.
    pub(super) fn with_github<'a>(&'a self, content: Element<'a, Message>) -> Element<'a, Message> {
        let Some(dialog) = &self.github.dialog else {
            return content;
        };
        let close = Message::GitHub(Action::Close);
        let (body, width, height, placement): (_, f32, f32, _) = match dialog {
            Dialog::Send(_) if self.send_on_chip() => return content,
            Dialog::Send(draft) => (
                self.send_view(draft),
                360.,
                self.viewport.height,
                super::help::Placement::Fit,
            ),
            Dialog::SignIn => (
                self.account_view(),
                360.,
                self.viewport.height,
                super::help::Placement::Fit,
            ),
            Dialog::Picker(picker) => (
                self.picker_view(picker),
                760.,
                540.,
                super::help::Placement::Center,
            ),
            Dialog::Conflict(review) => (
                conflict::view(review, self.github.login()),
                960.,
                600.,
                super::help::Placement::Center,
            ),
            Dialog::Compare(compare, label) => (
                compare.dialog(label),
                960.,
                600.,
                super::help::Placement::Center,
            ),
        };
        let height = match dialog {
            Dialog::Picker(_) => height,
            _ => height.min(self.viewport.height),
        };
        let modal = super::help::modal(content, body, width, height, placement, close);
        reshiki::accessibility::focus_scope(modal)
    }

    /// The Send popover: what goes where, then the message, selected so
    /// typing replaces it. Enter sends; Escape or a click outside closes.
    fn send_view<'a>(&'a self, draft: &'a SendDraft) -> Element<'a, Message> {
        let repo = self.github.named(&draft.key).repo;
        let hue = self.github.hue(&draft.key);
        let letters = identity::monograms(&self.github_open_repos())
            .remove(&draft.key)
            .unwrap_or_else(|| identity::monogram(&repo));
        let what = match draft.files.as_slice() {
            [one] => format!("{} → {repo}", file_name(one)),
            all => format!("{} → {repo}", drawings(all.len())),
        };
        let line = row![
            identity::badge(hue, letters, false),
            text(what.clone()).size(12)
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        let mut body = column![].spacing(10);
        if draft.files.len() > 1 {
            body = body.push(
                reshiki::accessibility::button(
                    "github-send-files",
                    format!("{what}, list them"),
                    row![line, super::workspace::caret(10.)]
                        .spacing(6)
                        .align_y(Alignment::Center),
                )
                .expanded(draft.listed)
                .padding([2, 0])
                .style(button::text)
                .on_press(Message::GitHub(Action::SendList)),
            );
            if draft.listed {
                body = body.push(
                    iced::widget::scrollable(
                        iced::widget::Column::with_children(draft.files.iter().map(|path| {
                            text(path.as_str())
                                .size(11)
                                .style(super::workspace::muted_text)
                                .into()
                        }))
                        .spacing(2),
                    )
                    .height(Length::Shrink),
                );
            }
        } else {
            body = body.push(line);
        }
        body = body
            .push(
                reshiki::accessibility::text_input(
                    SEND_MESSAGE,
                    "Message",
                    "What changed?",
                    &draft.message,
                )
                .style(crate::appearance::input_style)
                .on_input(|message| Message::GitHub(Action::SendMessage(message)))
                .on_submit(Message::GitHub(Action::Send))
                .size(13)
                .padding(8),
            )
            .push(
                container(
                    reshiki::accessibility::button(
                        "github-send-confirm",
                        "Send",
                        text("Send").size(13),
                    )
                    .padding([6, 14])
                    .style(crate::appearance::primary)
                    .on_press(Message::GitHub(Action::Send)),
                )
                .align_right(Length::Fill),
            );
        container(body)
            .width(320)
            .padding(12)
            .style(super::color_popover::surface)
            .into()
    }
}

/// Whether the chip has a state part to click; Review stands alone.
fn suffix_shown(chip: Chip) -> bool {
    chip != Chip::Review
}

/// A folder path of at most 24 characters: `scheme/…/drafts`, or the
/// start and end of one long name.
fn middle(folder: &str) -> String {
    const ROOM: usize = 24;
    let chars: Vec<char> = folder.chars().collect();
    if chars.len() <= ROOM {
        return folder.to_owned();
    }
    let ends = |head: usize, tail: usize| {
        let head: String = chars.iter().take(head).collect();
        let tail: String = chars.iter().skip(chars.len() - tail).collect();
        format!("{head}…{tail}")
    };
    match (folder.split_once('/'), folder.rsplit_once('/')) {
        (Some((first, _)), Some((_, last)))
            if first.chars().count() + last.chars().count() + 3 <= ROOM =>
        {
            format!("{first}/…/{last}")
        }
        _ => ends(ROOM / 2, ROOM / 2 - 1),
    }
}

/// What the status chip says, after the repository's dot and name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Chip {
    /// `· Sending…`
    Sending,
    /// An amber Review: a drawing changed on GitHub.
    Review,
    /// `· Not sent` and Retry.
    NotSent,
    /// `· not sent yet` and Send: a drawing GitHub has never had.
    New,
    /// `· 1 unsent` and Send.
    Unsent(usize),
    /// `✓`
    Sent,
}

/// Review, in amber: a question waiting for the user, not an error.
fn amber_button(theme: &Theme, status: button::Status) -> button::Style {
    let amber = crate::appearance::readable(
        theme.palette().warning,
        &[theme.palette().background],
        reshiki::color_contrast::TEXT_TARGET,
    );
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: Some(
            Color {
                a: if hovered { 0.24 } else { 0.14 },
                ..theme.palette().warning
            }
            .into(),
        ),
        text_color: amber,
        border: Border {
            color: amber,
            width: 1.,
            radius: 5.0.into(),
        },
        ..button::Style::default()
    }
}

fn poll_after(generation: u64, wait: std::time::Duration) -> Task<Message> {
    perform(async move { tokio::time::sleep(wait).await }, move |()| {
        Action::Poll(generation)
    })
}

const SEND_MESSAGE: &str = "github-send-message";
