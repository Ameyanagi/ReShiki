//! Finder and Dock requests to open drawings, which open as tabs.
use super::{App, Message};
use iced::{Subscription, Task, futures::SinkExt};
use std::sync::Mutex;
use tokio::sync::mpsc::UnboundedReceiver;

static EVENTS: Mutex<Option<UnboundedReceiver<reshiki_macos::OpenRequest>>> = Mutex::new(None);

#[derive(Debug, Clone)]
pub enum Action {
    Open(reshiki_macos::OpenRequest),
}

pub(crate) fn install_document_events(events: UnboundedReceiver<reshiki_macos::OpenRequest>) {
    if let Ok(mut slot) = EVENTS.lock() {
        *slot = Some(events);
    }
}

pub(super) fn subscription() -> Subscription<Message> {
    Subscription::run(|| {
        iced::stream::channel(16, async |mut output| {
            let events = EVENTS.lock().ok().and_then(|mut slot| slot.take());
            if let Some(mut events) = events {
                while let Some(request) = events.recv().await {
                    if output
                        .send(Message::MacFiles(Action::Open(request)))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            }
        })
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
