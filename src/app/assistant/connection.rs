//! Connecting to Codex and loading the account and model catalog, guarded by the request serial.

use super::Action;
use crate::app::{App, Message};
use iced::Task;
use reshiki::assistant::codex;

impl App {
    pub(super) fn assistant_connect(&mut self) -> Task<Message> {
        if self.assistant.busy {
            return Task::none();
        }
        self.assistant.serial = self.assistant.serial.wrapping_add(1);
        self.assistant.cancel = Default::default();
        self.assistant.busy = true;
        self.assistant.error = false;
        self.assistant.status = "Connecting to Codex…".into();
        self.assistant.started = Some(std::time::Instant::now());
        let serial = self.assistant.serial;
        let cancel = self.assistant.cancel.clone();
        Task::perform(codex::connect(cancel), move |result| {
            Message::Assistant(Action::Connected(serial, result))
        })
    }
    pub(super) fn assistant_connected(
        &mut self,
        serial: u64,
        result: Result<codex::Account, String>,
    ) -> Task<Message> {
        if serial != self.assistant.serial {
            return Task::none();
        }
        self.assistant.busy = false;
        self.assistant.started = None;
        match result {
            Ok(account) => {
                self.assistant.error = !account.connected;
                self.assistant.status = if account.connected {
                    "Connected to Codex".into()
                } else {
                    "Run `codex login` to sign in, then reconnect.".into()
                };
                self.assistant.account = Some(account);
            }
            Err(error) => {
                self.assistant.status = error;
                self.assistant.error = true;
            }
        }
        Task::none()
    }
}
