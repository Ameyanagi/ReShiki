//! GitHub in the editor: sign-in, the repository picker, Send, the status
//! chip, the question on closing and Versions. The work itself is in
//! `reshiki::github`; this keeps what the window shows. Its messages are
//! handled before the modal gates, like the star toast's: none of them edits
//! a drawing except through the normal save and edit paths.
use super::{App, InspectorTab, Message, document_tab::TabId, focus_drawing};
use iced::widget::{column, row, text};
use iced::{Alignment, Element, Length, Task};
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
pub(super) mod picker;
#[cfg(test)]
mod tests;
mod versions;

pub(super) use picker::Mode;

const OPEN_FROM: &str = "Open from GitHub…";
const SEND: &str = "Send to GitHub";
const VERSIONS: &str = "Versions";
const ACCOUNT: &str = "GitHub account…";
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
    Account,
    Picker(Box<picker::Picker>),
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
}

impl State {
    pub(super) fn new() -> Self {
        let client_id = config::client_id();
        Self {
            account: if client_id.is_some() {
                Account::SignedOut(None)
            } else {
                Account::NotConfigured
            },
            client_id,
            ends: config::endpoints(),
            root: if cfg!(test) {
                None
            } else {
                workcopy::root().ok()
            },
            generation: 0,
            client: None,
            repos: Vec::new(),
            dialog: None,
            menu: false,
            states: HashMap::new(),
            seq: 0,
            unsent: HashMap::new(),
            sending: HashSet::new(),
            failed: HashMap::new(),
            conflicts: HashMap::new(),
            resolving: None,
            after_save: None,
            closing: None,
            later: HashSet::new(),
            window_answered: false,
            starred: false,
            versions: versions::Versions::default(),
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

    /// The dialog owns the keyboard: see [`modal_key`].
    pub(super) fn dialog_open(&self) -> bool {
        self.dialog.is_some()
    }

    /// The repository and repository path of a drawing kept for GitHub.
    pub(super) fn locate(&self, path: Option<&Path>) -> Option<(RepoKey, String)> {
        workcopy::locate(self.root.as_deref()?, path?)
    }
}

#[derive(Debug, Clone)]
pub enum Action {
    Menu(bool),
    ShowAccount,
    /// Escape, Cancel or a click outside the dialog.
    Close,
    Loaded(u64, Result<Option<Tokens>, String>),
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
    CheckAgain,
    Listed(u64, Result<gh::Account, gh::Error>),
    Browser(Result<(), String>),
    Picker(picker::Action),
    /// Send for the drawing in front: the Send box, or the picker's "Save
    /// here" for a drawing not on GitHub.
    ShowSend,
    SendMessage(String),
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
    Resolve(send::Choice),
    /// The choice, once the save it waited for has landed.
    Settle(RepoKey, send::Choice),
    ShowVersions,
    Versions(versions::Action),
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

fn perform<T: Send + 'static>(
    future: impl Future<Output = T> + Send + 'static,
    then: impl FnOnce(T) -> Action + Send + 'static,
) -> Task<Message> {
    Task::perform(future, move |result| Message::GitHub(then(result)))
}

fn open_url(url: String) -> Task<Message> {
    perform(reshiki::updates::open_url(url), Action::Browser)
}

/// `Send “a.rsk” to GitHub?` with Send to GitHub and Later.
fn ask_send(title: String) -> Task<Message> {
    iced::window::latest().then(move |window| {
        let dialog = rfd::AsyncMessageDialog::new()
            .set_level(rfd::MessageLevel::Info)
            .set_title(title.clone())
            .set_description("Saved on this computer only.")
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

    pub(super) fn github_action(&mut self, action: Action) -> Task<Message> {
        let id = self.github.client_id.clone().unwrap_or_default();
        let ends = self.github.ends.clone();
        match action {
            Action::Menu(open) => self.github.menu = open,
            Action::ShowAccount => {
                self.github.menu = false;
                self.github.dialog = Some(Dialog::Account);
                let list = if self.github.repos.is_empty() {
                    self.github_list()
                } else {
                    Task::none()
                };
                return Task::batch([list, self.account_focus()]);
            }
            Action::Close => {
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
            Action::SignIn => {
                if !self.github.configured() {
                    return Task::none();
                }
                self.github.generation += 1;
                let generation = self.github.generation;
                self.github.account = Account::Requesting;
                return perform(
                    async move { auth::request_code(&id, &ends).await },
                    move |result| Action::CodeReady(generation, result),
                );
            }
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
                        self.github_listed(account, true);
                        self.github.versions.target = None;
                        self.star_github_signed_in();
                        self.status = "Signed in to GitHub".into();
                        self.error = false;
                        if let Some(client) = client {
                            return perform(
                                async move { client.remember(&login).await },
                                move |kept| Action::Remembered(generation, kept),
                            );
                        }
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
                }
            }
            Action::SignOut => {
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
                let slug = config::app_slug().unwrap_or_default();
                return open_url(ends.web(&format!("/apps/{slug}/installations/new")));
            }
            Action::CheckAgain => return self.github_list(),
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
                        self.github_listed(account, remembered);
                        if let Some(Dialog::Picker(picker)) = &mut self.github.dialog {
                            picker.listed(&self.github.repos);
                        }
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
                    return self
                        .github_action(Action::Picker(picker::Action::Show(Mode::SaveHere)));
                };
                if self.github.client().is_none() {
                    return self.github_action(Action::ShowAccount);
                }
                self.github_refresh(&key);
                let mut files = self.github.unsent.get(&key).cloned().unwrap_or_default();
                for (_, path) in self.github_edited(&key) {
                    if !files.contains(&path) {
                        files.push(path);
                    }
                }
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
                }));
                return iced::widget::operation::focus(SEND_MESSAGE);
            }
            Action::SendMessage(message) => {
                if let Some(Dialog::Send(draft)) = &mut self.github.dialog {
                    draft.message = message;
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
            Action::Resolve(choice) => return self.github_resolve(choice),
            Action::Settle(key, choice) => return self.github_settle(key, choice),
            Action::ShowVersions => {
                self.github.menu = false;
                if self.github.locate(self.tab.path.as_deref()).is_none() {
                    return Task::none();
                }
                let task = self.show_inspector_tab(InspectorTab::Versions);
                return Task::batch([task.unwrap_or_else(Task::none), self.github_follow()]);
            }
            Action::Versions(action) => return self.versions_action(action),
        }
        Task::none()
    }

    fn github_listed(&mut self, account: gh::Account, remembered: bool) {
        let login = account.login.clone();
        self.github.account = if account.needs_install() {
            Account::NeedsInstall { login }
        } else {
            Account::SignedIn { login, remembered }
        };
        self.github.repos = account.repos;
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

    /// A drawing just opened: count what is unsent in its repository.
    pub(super) fn github_opened(&mut self, path: &Path) {
        if let Some((key, _)) = self.github.locate(Some(path)) {
            self.github_refresh(&key);
        }
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
            self.github.dialog = Some(Dialog::Account);
            return Task::none();
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
        let mut tasks = Vec::new();
        match result {
            Ok(outcome) => {
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
                let conflicts = self.github_conflicts(&key);
                self.error = conflicts > 0;
                self.status = match conflicts {
                    0 => "Sent to GitHub".into(),
                    1 => "Sent · 1 drawing changed on GitHub and was left out".into(),
                    n => format!("Sent · {n} drawings changed on GitHub and were left out"),
                };
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
    pub(super) fn github_before_close(&mut self, id: TabId) -> Option<Task<Message>> {
        if self.github.later.remove(&id) || self.github.closing.is_some() {
            return None;
        }
        let path = self.strip().find(|tab| tab.id == id)?.path.clone();
        self.github_unsent(path.as_deref())?;
        if let Some(index) = self.tab_index(id) {
            self.select_tab(index);
        }
        self.github.closing = Some(Closing::Tab(id, self.tab.revision));
        Some(ask_send(format!("Send “{}” to GitHub?", self.tab.name())))
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
        Some(ask_send(match count {
            1 => "Send 1 drawing to GitHub?".into(),
            n => format!("Send {n} drawings to GitHub?"),
        }))
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

    /// The GitHub button in the command bar, with its menu.
    pub(super) fn github_button(&self) -> Element<'_, Message> {
        let state = &self.github;
        let anchor = super::workspace::icon_button(
            super::icons::Icon::Cloud,
            "GitHub",
            Some(Message::GitHub(Action::Menu(!state.menu))),
            state.menu,
        );
        let popup = state.menu.then(|| {
            let configured = state.configured();
            let on_github = state.locate(self.tab.path.as_deref()).is_some();
            let item = |id: &'static str, label: &'static str, action: Option<Action>| {
                reshiki::accessibility::button(id, label, text(label).size(12))
                    .width(Length::Fill)
                    .padding([6, 10])
                    .style(super::workspace::control(false))
                    .on_press_maybe(action.map(Message::GitHub))
            };
            let mut items = column![
                item(
                    "github-open",
                    OPEN_FROM,
                    configured.then_some(Action::Picker(picker::Action::Show(Mode::Open)))
                ),
                item("github-send", SEND, configured.then_some(Action::ShowSend)),
                item(
                    "github-versions",
                    VERSIONS,
                    (configured && on_github).then_some(Action::ShowVersions)
                ),
                item("github-account", ACCOUNT, Some(Action::ShowAccount)),
            ]
            .spacing(1);
            if !configured {
                items = items.push(
                    iced::widget::container(
                        text(NOT_SET_UP)
                            .size(11)
                            .style(super::workspace::muted_text),
                    )
                    .padding([6, 10]),
                );
            }
            iced::widget::container(items)
                .width(200)
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

    /// The status bar chip for the drawing in front, if it is on GitHub.
    pub(super) fn github_chip(&self) -> Option<Element<'_, Message>> {
        let (key, repo_path) = self.github.locate(self.tab.path.as_deref())?;
        let state = &self.github;
        let unsent = state.unsent.get(&key);
        let mut count = unsent.map_or(0, Vec::len);
        if self.tab.shown_dirty() && !unsent.is_some_and(|unsent| unsent.contains(&repo_path)) {
            count += 1;
        }
        // Other drawings of the repository with unsaved edits.
        count += self
            .tabs
            .background
            .iter()
            .filter(|tab| tab.edited)
            .filter_map(|tab| state.locate(tab.path.as_deref()))
            .filter(|(other, path)| *other == key && !unsent.is_some_and(|u| u.contains(path)))
            .count();
        let conflicts = self.github_conflicts(&key);
        let chip = |id: &'static str, label: String, action: Option<Action>, strong: bool| {
            super::workspace::accessible_command(id, label.clone(), label)
                .on_press_maybe(action.map(Message::GitHub))
                .style(super::workspace::control(strong))
        };
        let signed_in = state.client().is_some();
        let mut items = row![].spacing(4).align_y(Alignment::Center);
        if state.sending.contains(&key) {
            items = items.push(chip("github-chip", "Sending…".into(), None, false));
        } else if conflicts > 0 {
            items = items
                .push(chip(
                    "github-chip",
                    match conflicts {
                        1 => "1 conflict".into(),
                        n => format!("{n} conflicts"),
                    },
                    Some(Action::ShowVersions),
                    false,
                ))
                .push(chip(
                    "github-review",
                    "Review".into(),
                    Some(Action::Review),
                    true,
                ));
        } else if state.failed.contains_key(&key) {
            items = items
                .push(chip(
                    "github-chip",
                    "Not sent".into(),
                    Some(Action::ShowVersions),
                    false,
                ))
                .push(chip(
                    "github-retry",
                    "Retry".into(),
                    Some(Action::ShowSend),
                    true,
                ));
        } else if count > 0 {
            items = items
                .push(chip(
                    "github-chip",
                    format!("{count} not on GitHub"),
                    Some(Action::ShowVersions),
                    false,
                ))
                .push(if signed_in {
                    chip(
                        "github-send-chip",
                        "Send".into(),
                        Some(Action::ShowSend),
                        true,
                    )
                } else {
                    chip(
                        "github-sign-in-chip",
                        "Sign in".into(),
                        Some(Action::ShowAccount),
                        true,
                    )
                });
        } else {
            items = items.push(chip(
                "github-chip",
                "On GitHub".into(),
                Some(Action::ShowVersions),
                false,
            ));
        }
        Some(items.into())
    }

    /// The open GitHub dialog over everything else.
    pub(super) fn with_github<'a>(&'a self, content: Element<'a, Message>) -> Element<'a, Message> {
        let Some(dialog) = &self.github.dialog else {
            return content;
        };
        let close = Message::GitHub(Action::Close);
        let (body, width, height, placement): (_, f32, f32, _) = match dialog {
            Dialog::Account => (
                self.account_view(),
                420.,
                340.,
                super::help::Placement::Center,
            ),
            Dialog::Picker(picker) => (
                self.picker_view(picker),
                760.,
                540.,
                super::help::Placement::Center,
            ),
            Dialog::Send(draft) => (send_view(draft), 420., 340., super::help::Placement::Center),
            Dialog::Conflict(review) => (
                conflict::view(review),
                960.,
                600.,
                super::help::Placement::Center,
            ),
            Dialog::Compare(compare, label) => (
                compare.dialog(self.tab.name(), label),
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
}

fn poll_after(generation: u64, wait: std::time::Duration) -> Task<Message> {
    perform(async move { tokio::time::sleep(wait).await }, move |()| {
        Action::Poll(generation)
    })
}

const SEND_MESSAGE: &str = "github-send-message";

/// `3 drawings: a.rsk, b.rsk, c.rsk`, so nothing is sent unseen.
fn send_view(draft: &SendDraft) -> Element<'_, Message> {
    let names = draft.files.join(", ");
    let files = match draft.files.len() {
        1 => format!("1 drawing: {names}"),
        n => format!("{n} drawings: {names}"),
    };
    column![
        text(SEND).size(18),
        text(format!("{}/{}", draft.key.owner, draft.key.repo))
            .size(12)
            .style(super::workspace::muted_text),
        iced::widget::scrollable(text(files).size(13)).height(Length::Shrink),
        reshiki::accessibility::text_input(SEND_MESSAGE, "Message", "Message", &draft.message)
            .style(crate::appearance::input_style)
            .on_input(|message| Message::GitHub(Action::SendMessage(message)))
            .on_submit(Message::GitHub(Action::Send))
            .size(13)
            .padding(8),
        row![
            iced::widget::Space::new().width(Length::Fill),
            reshiki::accessibility::button("github-send-cancel", "Cancel", text("Cancel").size(13))
                .padding([7, 12])
                .style(crate::appearance::secondary)
                .on_press(Message::GitHub(Action::Close)),
            reshiki::accessibility::button("github-send-confirm", "Send", text("Send").size(13))
                .padding([7, 12])
                .style(crate::appearance::primary)
                .on_press(Message::GitHub(Action::Send)),
        ]
        .spacing(8),
    ]
    .spacing(12)
    .into()
}
