//! Review abandoned drawings before restoring or discarding them.
use super::{App, Message};
use iced::{
    Task,
    keyboard::{Key, Modifiers, key::Named},
};
use reshiki::recovery::Candidate;
use std::{collections::BTreeSet, path::PathBuf};

mod view;

#[derive(Debug, Clone)]
pub enum Action {
    Show,
    Toggle(PathBuf),
    RestoreSelected,
    RestoreAll,
    Discard,
    ConfirmDiscard,
    Close,
    Finished(u64, Outcome),
    #[cfg(target_arch = "wasm32")]
    OwnersReady(u64, Result<(), String>, Box<Action>),
}

pub(super) fn completion(action: &Action) -> bool {
    match action {
        Action::Finished(..) => true,
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Dialog {
    Picker,
    Discard,
}

#[derive(Default)]
pub(super) struct State {
    dialog: Option<Dialog>,
    selected: BTreeSet<PathBuf>,
    serial: u64,
    pending: Option<u64>,
    failure: Option<String>,
}

impl State {
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
    pub(super) fn prune_recovery_owners(&mut self) {
        #[cfg(target_arch = "wasm32")]
        if !reshiki::storage::browser::owners_verified() {
            // Unknown is protected from deletion, not proof an offer is live.
            return;
        }
        self.recovered.retain(Candidate::available);
    }

    pub(super) fn recovery_action(&mut self, action: Action) -> Task<Message> {
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
                        "Moved {count} recovery {} to the Trash",
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
                self.restore_recovered_selection(|candidate| selected.contains(&candidate.path));
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
            Action::Finished(..) => {}
            #[cfg(target_arch = "wasm32")]
            Action::OwnersReady(..) => {}
        }
        Task::none()
    }
}

fn plural<'a>(count: usize, one: &'a str, many: &'a str) -> &'a str {
    if count == 1 { one } else { many }
}

#[cfg(not(target_arch = "wasm32"))]
async fn discard(candidates: Vec<Candidate>) -> Outcome {
    reshiki::task::spawn_blocking(move || {
        let mut outcome = Outcome::default();
        for candidate in candidates {
            if !candidate.available() {
                outcome.protected.push(candidate.path);
                continue;
            }
            match reshiki::recovery::discard(&candidate) {
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
async fn discard(candidates: Vec<Candidate>) -> Outcome {
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

fn title(candidate: &Candidate) -> String {
    candidate
        .snapshot
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
mod tests;
