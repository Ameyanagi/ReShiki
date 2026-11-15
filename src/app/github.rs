//! GitHub in the editor: sign-in, the repository picker, Send, the status
//! chip, the question on closing and Versions. The work itself is in
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
const SEND_TO: &str = "Send to GitHub…";
const NOT_SET_UP: &str = "Not set up in this build";

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

    fn login(&self) -> Option<&str> {
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

    /// The repository kept in folder `key`, as GitHub names it now.
    fn named(&self, key: &RepoKey) -> RepoKey {
        self.states.get(key).map_or_else(|| key.clone(), RepoState::key)
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
    /// Who changed the drawing in conflict on GitHub, and when.
    ReviewMeta(String, Option<api::RemoteVersion>),
    Resolve(send::Choice),
    /// The choice, once the save it waited for has landed.
    Settle(RepoKey, send::Choice),
    ShowVersions,
    /// Back to the inspector tab Versions replaced.
    CloseVersions,
    Versions(versions::Action),
    /// A hue chosen for the repository in a tab's menu.
    Color(RepoKey, usize),
    /// The repository colors and the last repository, written.
    Kept(Result<(), String>),
    CopyLink,
    OpenOnGitHub,
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
    fn github_keep_prefs(&self) -> Task<Message> {
        let (Some(root), prefs) = (self.github.root.clone(), self.github.prefs.clone()) else {
            return Task::none();
        };
        perform(
            async move {
                reshiki::task::spawn_blocking(move || identity::save(&root, &prefs))
                    .await
                    .map_err(|error| error.to_string())?
            },
            Action::Kept,
        )
    }

    /// Repositories seen for the first time keep a color.
    fn github_seen<'a>(&mut self, repos: impl IntoIterator<Item = (u64, &'a RepoKey)>) -> Task<Message> {
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
                if matches!(self.github.dialog, Some(Dialog::SignIn)) {
                    self.github.after_sign_in = None;
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
            Action::ReviewMeta(repo_path, found) => {
                if let Some(Dialog::Conflict(review)) = &mut self.github.dialog
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
                if self.inspector_tab != InspectorTab::Versions {
                    self.github.versions.back = self.inspector_open.then_some(self.inspector_tab);
                }
                let task = self.show_inspector_tab(InspectorTab::Versions);
                return Task::batch([task.unwrap_or_else(Task::none), self.github_follow()]);
            }
            Action::CloseVersions => {
                match self.github.versions.back.take() {
                    Some(tab) => {
                        let task = self.show_inspector_tab(tab);
                        return Task::batch([task.unwrap_or_else(Task::none), focus_drawing()]);
                    }
                    None => {
                        self.inspector_tab = InspectorTab::Properties;
                        self.inspector_open = false;
                    }
                }
                return focus_drawing();
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
                if let Some(url) = self.github_link() {
                    return open_url(url);
                }
            }
        }
        Task::none()
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
        let repos: Vec<(u64, RepoKey)> = self
            .github
            .repos
            .iter()
            .map(|repo| (repo.id, repo.key.clone()))
            .collect();
        self.github_seen(repos.iter().map(|(id, key)| (*id, key)))
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
        let Some((key, _)) = self.github.locate(Some(path)) else {
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
        self.github.unsent.values().any(|unsent| !unsent.is_empty())
            || self.strip().any(|tab| {
                self.shown_edited(tab) && self.github.locate(tab.path.as_deref()).is_some()
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
        let (key, repo_path) = self.github.locate(self.tab.path.as_deref())?;
        let state = self.github.states.get(&key)?;
        Some(self.github.ends.web(&format!(
            "/{}/{}/blob/{}/{repo_path}",
            state.owner, state.repo, state.branch
        )))
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
                    app.save_drawing(false)
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
                    (Some(changed), sent, _) => format!("Sent {sent} · {changed} changed on GitHub"),
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
        self.github_refresh(&key);
        self.github
            .unsent
            .get(&key)
            .is_some_and(|unsent| unsent.contains(&repo_path))
            .then_some(key)
    }

    /// Before a tab closes: asks whether to send its saved, unsent drawing.
    /// `None` closes it now.
    pub(super) fn github_before_close(
        &mut self,
        id: TabId,
    ) -> Option<Task<Message>> {
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
            count if count > 1 => format!("Sends {count} drawings in {}.", self.github.named(&key).repo),
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
                let left = self.github.states.keys().any(|key| {
                    self.github.unsent.get(key).is_some_and(|u| !u.is_empty())
                        || self.github_conflicts(key) > 0
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
        if !self.inspector_open || self.inspector_tab != InspectorTab::Versions {
            return Task::none();
        }
        let front = self.github.locate(self.tab.path.as_deref());
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
        let Some((key, _)) = self.github.locate(self.tab.path.as_deref()) else {
            return (
                SEND_TO.into(),
                "Choose a repository".into(),
                Some(Action::ShowSend),
            );
        };
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
            rows.push(("Versions".into(), name.clone(), Some(Action::ShowVersions)));
            rows.push(("Copy GitHub link".into(), name.clone(), Some(Action::CopyLink)));
            rows.push(("Open on GitHub ↗".into(), name, Some(Action::OpenOnGitHub)));
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
        rows
    }

    /// The GitHub button in the command bar, with its menu: what applies to
    /// the drawing in front first, the account last. A dot while anything
    /// is not on GitHub yet.
    pub(super) fn github_button(&self) -> Element<'_, Message> {
        let state = &self.github;
        let unsent = self.github_anything_unsent();
        let anchor = super::workspace::icon_button(
            super::icons::Icon::Cloud,
            if unsent {
                "GitHub · Not everything is sent"
            } else {
                "GitHub"
            },
            Some(Message::GitHub(Action::Menu(!state.menu))),
            state.menu,
        );
        let anchor: Element<'_, Message> = if unsent {
            iced::widget::stack![
                anchor,
                container(identity::dot(|theme: &Theme| theme.palette().primary))
                    .padding([6, 5])
                    .align_right(36)
                    .align_top(36),
            ]
            .into()
        } else {
            anchor
        };
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
                    "Versions".into(),
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
        let (key, _) = self.github.locate(self.tab.path.as_deref())?;
        let state = &self.github;
        let name = self.github_label(&key);
        let hue = state.hue(&key);
        let muted = |line: String| text(line).size(12).style(super::workspace::muted_text);
        let action = |id: &'static str, label: &'static str, action: Action| {
            super::workspace::accessible_command(id, label, label)
                .on_press(Message::GitHub(action))
                .padding([4, 9])
        };
        let mut content = row![
            identity::dot(move |theme: &Theme| identity::color(theme, hue)),
            text(name.clone()).size(12),
        ]
        .spacing(6)
        .align_y(Alignment::Center);
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
                Some(action("github-retry", "Retry", Action::ShowSend).style(super::workspace::control(true))),
                "not sent".to_owned(),
            ),
            Chip::New | Chip::Unsent(_) => {
                let said = match state_now {
                    Chip::Unsent(count) => format!("{count} unsent"),
                    _ => "not sent yet".to_owned(),
                };
                (
                    Some(format!("· {said}")),
                    Some(action("github-send-chip", "Send", Action::ShowSend).style(super::workspace::control(true))),
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
        let spoken = format!("{name}, {said}");
        let chip = reshiki::accessibility::button("github-chip", spoken, content)
            .padding([4, 8])
            .style(super::workspace::control(false))
            .on_press(Message::GitHub(Action::ShowVersions));
        let chip = row![chip].push(after).spacing(4).align_y(Alignment::Center);
        let popup = match &state.dialog {
            Some(Dialog::Send(draft)) => Some(self.send_view(draft)),
            _ => None,
        };
        Some(Element::new(
            super::popover::popover(chip, popup, Message::GitHub(Action::Close)).align_end(),
        ))
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
        let (key, repo_path) = self.github.locate(tab.path.as_deref())?;
        let named = self.github.named(&key);
        let hue = self.github.hue(&key);
        let letters = identity::monograms(&self.github_open_repos())
            .remove(&key)
            .unwrap_or_else(|| identity::monogram(&named.repo));
        let unsent = self.github_waiting(&key, |tab| self.shown_edited(tab)).len();
        let spoken = match unsent {
            0 => self.github_label(&key),
            n => format!("{}, {n} unsent", self.github_label(&key)),
        };
        let menu = menu.then(|| {
            let mut items = column![
                container(text("Color").size(11).style(super::workspace::muted_text))
                    .padding([4, 10])
            ]
            .spacing(1);
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
            badge: identity::badge(hue, letters),
            underline: container(Space::new().width(Length::Fill).height(2))
                .style(move |theme: &Theme| container::Style {
                    background: Some(identity::color(theme, hue).into()),
                    ..container::Style::default()
                })
                .into(),
            hint: format!("{}/{} · {repo_path}", named.owner, named.repo),
            spoken,
            menu,
        })
    }

    /// The repository of the drawing in front, for the window title.
    pub(super) fn github_title(&self) -> Option<String> {
        let (key, _) = self.github.locate(self.tab.path.as_deref())?;
        Some(self.github.named(&key).repo)
    }

    /// The open GitHub dialog over everything else. Send is a popover of the
    /// status chip instead.
    pub(super) fn with_github<'a>(&'a self, content: Element<'a, Message>) -> Element<'a, Message> {
        let Some(dialog) = &self.github.dialog else {
            return content;
        };
        let close = Message::GitHub(Action::Close);
        let (body, width, height, placement): (_, f32, f32, _) = match dialog {
            Dialog::Send(_) => return content,
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
                conflict::view(review),
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
        let line = row![identity::badge(hue, letters), text(what.clone()).size(12)]
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
