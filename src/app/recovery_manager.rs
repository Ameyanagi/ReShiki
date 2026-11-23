//! Review abandoned drawings before restoring or discarding them.
use super::{App, Message};
use iced::{
    Task,
    keyboard::{Key, Modifiers, key::Named},
};
use reshiki::recovery::{Candidate, Discovery, Draft, Recovery};
use std::{collections::BTreeSet, path::PathBuf};

mod view;

const BIN_NAME: &str = if cfg!(windows) {
    "Recycle Bin"
} else {
    "Trash"
};

#[derive(Debug, Clone)]
pub enum Action {
    Show,
    Toggle(PathBuf),
    RestoreSelected,
    RestoreAll,
    Discard,
    ConfirmDiscard,
    Close,
    /// Show the folder that holds the drafts a newer ReShiki made.
    #[cfg(not(target_arch = "wasm32"))]
    OpenFolder,
    Discovered(u64, Result<Discovery, String>),
    Loaded(u64, Loaded),
    Finished(u64, Outcome),
    #[cfg(target_arch = "wasm32")]
    OwnersReady(u64, Result<(), String>, Box<Action>),
}

pub(super) fn completion(action: &Action) -> bool {
    match action {
        Action::Discovered(..) | Action::Loaded(..) | Action::Finished(..) => true,
        #[cfg(target_arch = "wasm32")]
        Action::OwnersReady(..) => true,
        _ => false,
    }
}

#[derive(Debug, Clone, Default)]
pub struct Outcome {
    removed: Vec<PathBuf>,
    protected: Vec<PathBuf>,
    errors: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Loaded {
    candidates: Vec<Candidate>,
    protected: Vec<PathBuf>,
    errors: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Dialog {
    Picker,
    Discard,
}

#[derive(PartialEq, Eq)]
pub(super) struct AccessibilityContext {
    dialog: Option<Dialog>,
    pending: bool,
    offered: Vec<PathBuf>,
    selected: BTreeSet<PathBuf>,
}

#[derive(Default)]
pub(super) struct State {
    dialog: Option<Dialog>,
    selected: BTreeSet<PathBuf>,
    serial: u64,
    pending: Option<u64>,
    discovery: Option<u64>,
    failure: Option<String>,
    /// Abandoned drafts made with a newer ReShiki, which cannot be offered.
    pub(super) newer: usize,
}

impl State {
    /// The startup state: `newer` unreadable drafts in the recovery folder.
    #[cfg(test)]
    pub(super) fn found(newer: usize) -> Self {
        Self {
            newer,
            ..Self::default()
        }
    }

    pub(super) fn open(&self) -> bool {
        self.dialog.is_some()
    }

    pub(super) fn close(&mut self) {
        if self.pending.is_none() {
            self.dialog = None;
            self.selected.clear();
            self.failure = None;
        }
    }
}

pub(super) fn modal_key(key: &Key, modifiers: Modifiers) -> Option<Message> {
    (modifiers.is_empty() && matches!(key, Key::Named(Named::Escape)))
        .then_some(Message::Recovery(Action::Close))
}

impl App {
    /// Whether the status bar shows the recovery banner.
    pub(super) fn recovery_banner_shown(&self) -> bool {
        !self.recovered.is_empty() || self.recovery_manager.newer > 0
    }

    pub(super) fn dismiss_recovery(&mut self) {
        self.recovery_manager.discovery = None;
        self.recovery_manager.pending = None;
        self.recovery_manager.close();
        self.recovered.clear();
        self.recovery_manager.newer = 0;
        if self.status.is_empty() {
            self.status = super::READY.into();
        }
    }

    pub(super) fn recovery_accessibility_context(&self) -> AccessibilityContext {
        AccessibilityContext {
            dialog: self.recovery_manager.dialog,
            pending: self.recovery_manager.pending.is_some(),
            offered: self
                .recovered
                .iter()
                .map(|draft| draft.path.clone())
                .collect(),
            selected: self.recovery_manager.selected.clone(),
        }
    }

    pub(super) fn prune_recovery_owners(&mut self) {
        #[cfg(target_arch = "wasm32")]
        if !reshiki::storage::browser::owners_verified() {
            // Unknown is protected from deletion, not proof an offer is live.
            return;
        }
        self.recovered.retain(Draft::available);
    }

    pub(super) fn restore_recovered(&mut self) -> Task<Message> {
        self.restore_recovered_selection(|_| true)
    }

    pub(super) fn restore_recovered_selection(
        &mut self,
        selected: impl Fn(&Draft) -> bool,
    ) -> Task<Message> {
        if self.recovery_manager.pending.is_some() || self.exit.frozen() {
            return Task::none();
        }
        #[cfg(target_arch = "wasm32")]
        if !reshiki::storage::browser::owners_verified() {
            self.status = "Could not check open ReShiki pages · Drafts kept · Try again".into();
            self.error = true;
            return Task::none();
        }
        let drafts: Vec<_> = self
            .recovered
            .iter()
            .filter(|draft| selected(draft))
            .cloned()
            .collect();
        if drafts.is_empty() {
            return Task::none();
        }
        self.recovery_manager.serial = self.recovery_manager.serial.wrapping_add(1);
        let serial = self.recovery_manager.serial;
        self.recovery_manager.pending = Some(serial);
        self.recovery_manager.failure = None;
        self.status = "Loading recovery drafts…".into();
        self.error = false;
        Task::perform(load(drafts), move |loaded| {
            Message::Recovery(Action::Loaded(serial, loaded))
        })
    }

    /// Discovery is app-owned and cannot delay initial window construction.
    pub(super) fn discover_recovery(&mut self, recovery: Option<Recovery>) -> Task<Message> {
        let Some(recovery) = recovery else {
            return Task::none();
        };
        self.recovery_manager.serial = self.recovery_manager.serial.wrapping_add(1);
        let serial = self.recovery_manager.serial;
        self.recovery_manager.discovery = Some(serial);
        Task::perform(
            async move {
                reshiki::task::spawn_blocking(move || recovery.discover())
                    .await
                    .map_err(|error| format!("Recovery worker failed: {error}"))
            },
            move |result| Message::Recovery(Action::Discovered(serial, result)),
        )
    }

    pub(super) fn recovery_action(&mut self, action: Action) -> Task<Message> {
        if let Action::Discovered(serial, result) = action {
            if self.recovery_manager.discovery != Some(serial) {
                return Task::none();
            }
            self.recovery_manager.discovery = None;
            match result {
                Ok(discovery) => {
                    self.recovered = discovery.drafts;
                    self.recovery_manager.newer = discovery.newer;
                    self.prune_recovery_owners();
                }
                Err(error) => {
                    self.status = format!("Could not check recovery drafts · {error}");
                    self.error = true;
                }
            }
            return Task::none();
        }
        if let Action::Loaded(serial, loaded) = action {
            if self.recovery_manager.pending != Some(serial) {
                return Task::none();
            }
            self.recovery_manager.pending = None;
            if self.exit.frozen() || self.pending.is_some() {
                self.recovery_manager.close();
                self.status = "Recovery postponed · Drafts kept · Try again".into();
                return Task::none();
            }
            let errors = loaded.errors;
            self.recovered
                .retain(|draft| !loaded.protected.contains(&draft.path));
            let mut count = 0;
            // Hydration preserves newest-first discovery order. Install oldest
            // first so the newest is the front tab, just like synchronous restore.
            for candidate in loaded.candidates.into_iter().rev() {
                if !candidate.available() {
                    self.recovered.retain(|draft| draft.path != candidate.path);
                    continue;
                }
                self.recovered.retain(|draft| draft.path != candidate.path);
                self.restore_candidate(candidate);
                count += 1;
            }
            if errors.is_empty() {
                self.recovery_manager.close();
                if count > 0 {
                    self.status = match count {
                        1 => "Recovered drawing · Save to keep a new copy".into(),
                        n => format!("Recovered {n} drawings · Save each to keep a copy"),
                    };
                    self.error = false;
                } else {
                    self.status = "No abandoned recovery drafts to restore".into();
                    self.error = false;
                }
            } else {
                self.status = format!(
                    "Could not restore recovery drafts: {} · Drafts kept · Try again",
                    errors.join("; ")
                );
                self.error = true;
                self.recovery_manager.dialog = Some(Dialog::Picker);
                self.recovery_manager.failure = Some(self.status.clone());
            }
            if count > 0 {
                // This app-owned receipt bypasses update_front. Start the same
                // document follow-up work now, with each result tagged to the
                // restored tab rather than the tab that requested recovery.
                self.sync_keyboard_drawing();
                let document_tasks =
                    Task::batch([self.start_label_refresh(), self.start_caption_names()]);
                return Task::batch([
                    super::tagged(document_tasks, self.tab.id),
                    self.start_autosave(),
                ]);
            }
            return Task::none();
        }
        #[cfg(target_arch = "wasm32")]
        if let Action::OwnersReady(serial, result, then) = action {
            if self.recovery_manager.pending != Some(serial) {
                return Task::none();
            }
            self.recovery_manager.pending = None;
            return match result {
                Ok(()) => self.recovery_action(*then),
                Err(error) => {
                    self.status =
                        format!("Could not verify open ReShiki pages · {error} · Try again");
                    self.error = true;
                    self.recovery_manager.failure = Some(self.status.clone());
                    Task::none()
                }
            };
        }
        if let Action::Finished(serial, outcome) = action {
            if self.recovery_manager.pending != Some(serial) {
                return Task::none();
            }
            self.recovery_manager.pending = None;
            self.prune_recovery_owners();
            self.recovered.retain(|candidate| {
                !outcome.removed.contains(&candidate.path)
                    && !outcome.protected.contains(&candidate.path)
            });
            if outcome.errors.is_empty() {
                let count = outcome.removed.len();
                self.status = if count == 0 {
                    "No abandoned recovery drafts to discard".into()
                } else if cfg!(target_arch = "wasm32") {
                    format!(
                        "Deleted {count} recovery {}",
                        plural(count, "draft", "drafts")
                    )
                } else {
                    format!(
                        "Moved {count} recovery {} to the {BIN_NAME}",
                        plural(count, "draft", "drafts")
                    )
                };
                self.error = false;
                self.recovery_manager.close();
            } else {
                self.status = format!(
                    "Could not discard recovery drafts: {} · Try again",
                    outcome.errors.join("; ")
                );
                self.error = true;
                self.recovery_manager.dialog = Some(Dialog::Picker);
                self.recovery_manager.failure = Some(self.status.clone());
            }
            return Task::none();
        }
        if self.recovery_manager.pending.is_some() {
            return Task::none();
        }
        #[cfg(target_arch = "wasm32")]
        if matches!(action, Action::RestoreSelected | Action::RestoreAll)
            && self.recovery_manager.dialog == Some(Dialog::Picker)
            && !reshiki::storage::browser::owners_verified()
        {
            self.recovery_manager.serial = self.recovery_manager.serial.wrapping_add(1);
            let serial = self.recovery_manager.serial;
            self.recovery_manager.pending = Some(serial);
            self.recovery_manager.failure = None;
            self.status = "Checking open ReShiki pages…".into();
            self.error = false;
            return Task::perform(crate::web::refresh_recovery_owners(), move |result| {
                Message::Recovery(Action::OwnersReady(
                    serial,
                    result,
                    Box::new(action.clone()),
                ))
            });
        }
        match action {
            Action::Show | Action::Discard => {
                if self.pending.is_some() || self.exit.frozen() {
                    return Task::none();
                }
                self.prune_recovery_owners();
                if self.recovered.is_empty() {
                    return Task::none();
                }
                self.recovery_manager.dialog = Some(if matches!(action, Action::Discard) {
                    Dialog::Discard
                } else {
                    Dialog::Picker
                });
                self.recovery_manager.selected.clear();
                self.recovery_manager.failure = None;
            }
            Action::Toggle(path) => {
                if self.recovery_manager.dialog == Some(Dialog::Picker)
                    && self
                        .recovered
                        .iter()
                        .any(|candidate| candidate.path == path)
                    && !self.recovery_manager.selected.remove(&path)
                {
                    self.recovery_manager.selected.insert(path);
                }
            }
            Action::RestoreSelected => {
                if self.recovery_manager.dialog != Some(Dialog::Picker) {
                    return Task::none();
                }
                let selected = self.recovery_manager.selected.clone();
                if selected.is_empty() {
                    return Task::none();
                }
                return self
                    .restore_recovered_selection(|candidate| selected.contains(&candidate.path));
            }
            Action::RestoreAll => {
                if self.recovery_manager.dialog == Some(Dialog::Picker) {
                    self.recovery_manager.close();
                    return self.update(Message::Restore);
                }
            }
            Action::ConfirmDiscard => {
                if self.recovery_manager.dialog != Some(Dialog::Discard) {
                    return Task::none();
                }
                self.recovery_manager.serial = self.recovery_manager.serial.wrapping_add(1);
                let serial = self.recovery_manager.serial;
                self.recovery_manager.pending = Some(serial);
                self.status = "Discarding recovery drafts…".into();
                self.error = false;
                return Task::perform(discard(self.recovered.clone()), move |outcome| {
                    Message::Recovery(Action::Finished(serial, outcome))
                });
            }
            Action::Close => self.recovery_manager.close(),
            #[cfg(not(target_arch = "wasm32"))]
            Action::OpenFolder => {
                if let Some(folder) = &self.tabs.recovery_root
                    && let Err(error) = open::that(folder)
                {
                    self.status = format!("Could not open the recovery folder: {error}");
                    self.error = true;
                }
            }
            Action::Discovered(..) | Action::Loaded(..) | Action::Finished(..) => {}
            #[cfg(target_arch = "wasm32")]
            Action::OwnersReady(..) => {}
        }
        Task::none()
    }
}

fn plural<'a>(count: usize, one: &'a str, many: &'a str) -> &'a str {
    if count == 1 { one } else { many }
}

async fn load(drafts: Vec<Draft>) -> Loaded {
    reshiki::task::spawn_blocking(move || {
        let mut loaded = Loaded::default();
        for draft in drafts {
            if !draft.available() {
                loaded.protected.push(draft.path);
                continue;
            }
            match draft.load() {
                Ok(candidate) => loaded.candidates.push(candidate),
                Err(error) => loaded.errors.push(format!("{}: {error}", title(&draft))),
            }
        }
        loaded
    })
    .await
    .unwrap_or_else(|error| Loaded {
        errors: vec![format!("Recovery worker failed: {error}")],
        ..Loaded::default()
    })
}

#[cfg(not(target_arch = "wasm32"))]
async fn discard(candidates: Vec<Draft>) -> Outcome {
    reshiki::task::spawn_blocking(move || {
        let mut outcome = Outcome::default();
        for candidate in candidates {
            if !candidate.available() {
                outcome.protected.push(candidate.path);
                continue;
            }
            if std::fs::symlink_metadata(&candidate.path)
                .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
            {
                outcome.removed.push(candidate.path);
                continue;
            }
            match candidate
                .load()
                .and_then(|loaded| reshiki::recovery::discard(&loaded))
            {
                Ok(()) => outcome.removed.push(candidate.path),
                Err(error) => outcome
                    .errors
                    .push(format!("{}: {error}", title(&candidate))),
            }
        }
        outcome
    })
    .await
    .unwrap_or_else(|error| Outcome {
        errors: vec![format!("Recovery worker failed: {error}")],
        ..Outcome::default()
    })
}

#[cfg(target_arch = "wasm32")]
async fn discard(candidates: Vec<Draft>) -> Outcome {
    // Refresh before hydration too: an unknown owner is protected until the
    // browser lock query completes. The existing transaction refreshes again
    // and compares its durable bytes before deleting anything.
    if let Err(error) = crate::web::refresh_recovery_owners().await {
        return Outcome {
            errors: vec![error],
            ..Outcome::default()
        };
    }
    let candidates = match candidates
        .iter()
        .map(Draft::load)
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(candidates) => candidates,
        Err(error) => {
            return Outcome {
                errors: vec![error],
                ..Outcome::default()
            };
        }
    };
    match crate::web::discard_recovery(&candidates).await {
        Ok(removed) => Outcome {
            removed,
            ..Outcome::default()
        },
        Err(error) => Outcome {
            errors: vec![error],
            ..Outcome::default()
        },
    }
}

fn title(candidate: &Draft) -> String {
    candidate
        .source
        .as_ref()
        .and_then(|path| path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Untitled".into())
}

fn date(saved_at: u64) -> String {
    i64::try_from(saved_at)
        .ok()
        .and_then(|timestamp| time::OffsetDateTime::from_unix_timestamp(timestamp).ok())
        .map(|date| {
            format!(
                "{:04}-{:02}-{:02} {:02}:{:02} UTC",
                date.year(),
                u8::from(date.month()),
                date.day(),
                date.hour(),
                date.minute()
            )
        })
        .unwrap_or_else(|| "Unknown date".into())
}

fn age(saved_at: u64, now: u64) -> String {
    let seconds = now.saturating_sub(saved_at);
    match seconds {
        0..60 => "just now".into(),
        60..3600 => format!("{} min ago", seconds / 60),
        3600..86400 => format!("{} h ago", seconds / 3600),
        _ => format!("{} d ago", seconds / 86400),
    }
}

fn now() -> u64 {
    #[cfg(not(target_arch = "wasm32"))]
    use std::time::{SystemTime, UNIX_EPOCH};
    #[cfg(target_arch = "wasm32")]
    use web_time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
pub(super) async fn finish_test_task(app: &mut App, task: Task<Message>) {
    use iced::futures::StreamExt;
    if let Some(mut stream) = iced_runtime::task::into_stream(task) {
        while let Some(action) = stream.next().await {
            if let iced_runtime::Action::Output(message) = action {
                let _ = app.update(message);
            }
        }
    }
}

#[cfg(test)]
pub(super) fn run_test_task(app: &mut App, task: Task<Message>) {
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(finish_test_task(app, task));
}

#[cfg(test)]
pub(super) fn test_draft(snapshot: reshiki::recovery::Snapshot) -> (tempfile::TempDir, Draft) {
    let directory = tempfile::tempdir().unwrap();
    let store = Recovery::in_directory(directory.path()).unwrap();
    std::fs::write(
        directory.path().join("4294967294-example.json"),
        serde_json::to_vec(&snapshot).unwrap(),
    )
    .unwrap();
    let draft = store.discover().drafts.remove(0);
    (directory, draft)
}

#[cfg(test)]
mod tests;
