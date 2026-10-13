//! Finder and Dock requests to open drawings, which open as tabs.
use super::{App, Message, event_slot};
use iced::{Subscription, Task};
use tokio::sync::mpsc::UnboundedReceiver;

static EVENTS: event_slot::Slot<UnboundedReceiver<reshiki_macos::OpenRequest>> =
    event_slot::Slot::new();
const EVENT_CAPACITY: usize = 16;

#[derive(Debug, Clone)]
pub enum Action {
    Open(reshiki_macos::OpenRequest),
}

pub(crate) fn install_document_events(events: UnboundedReceiver<reshiki_macos::OpenRequest>) {
    EVENTS.install(events);
}

pub(super) fn subscription() -> Subscription<Message> {
    event_slot::subscription(&EVENTS, EVENT_CAPACITY, |request| {
        Message::MacFiles(Action::Open(request))
    })
}

impl App {
    /// Finder and Dock requests open as tabs, in the order given.
    pub(super) fn mac_file_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Open(Ok(paths)) => super::files::open_paths(paths),
            Action::Open(Err(error)) => {
                self.error = true;
                self.status = format!("Could not open document: {error}");
                Task::none()
            }
        }
    }
}

#[cfg(test)]
mod tests;
