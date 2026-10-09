//! UI-side ownership acknowledgement; the transport cannot accept a request
//! merely because it queued bytes. A committed open is retained until processed.
use super::{
    App, Message, files,
    office::{Binding, Phase},
};
use crate::desktop::{Event, protocol::Status};
use iced::{Subscription, Task, futures::SinkExt};
use std::sync::Mutex;
use tokio::sync::mpsc::Receiver;

static EVENTS: Mutex<Option<Receiver<Event>>> = Mutex::new(None);
#[derive(Debug, Clone)]
pub enum Action {
    Request(Event),
    Ready,
    Finished,
    Office(Binding, files::Opened),
    Poll,
}
#[derive(Default)]
pub(super) struct State {
    pub(super) opening: bool,
    reading: bool,
    imports: Vec<super::document_tab::TabId>,
    ready: bool,
}

pub(crate) fn install_document_events(events: Receiver<Event>) {
    if let Ok(mut slot) = EVENTS.lock() {
        *slot = Some(events);
    }
}
pub(super) fn subscription(app: &App) -> Subscription<Message> {
    let requests = Subscription::run(|| {
        iced::stream::channel(16, async |mut output| {
            let events = EVENTS.lock().ok().and_then(|mut slot| slot.take());
            if let Some(mut events) = events {
                while let Some(event) = events.recv().await {
                    if output
                        .send(Message::Desktop(Action::Request(event)))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            }
        })
    });
    Subscription::batch([
        requests,
        if app.strip().any(|tab| {
            tab.office
                .as_ref()
                .is_some_and(|binding| binding.lease.is_some())
        }) {
            iced::time::every(std::time::Duration::from_secs(1))
                .map(|_| Message::Desktop(Action::Poll))
        } else {
            Subscription::none()
        },
    ])
}
fn activate() -> Task<Message> {
    iced::window::latest().then(|window| {
        window.map_or_else(Task::none, |id| {
            iced::window::minimize(id, false)
                .chain(iced::window::gain_focus(id))
                .chain(iced::window::request_user_attention(
                    id,
                    Some(iced::window::UserAttention::Informational),
                ))
        })
    })
}
impl App {
    pub(super) fn desktop_import_started(&mut self) {
        if self.desktop.opening && self.tab.busy && !self.desktop.imports.contains(&self.tab.id) {
            self.desktop.imports.push(self.tab.id);
        }
    }
    pub(super) fn desktop_import_finished(&mut self) {
        self.desktop.imports.retain(|id| *id != self.tab.id);
        self.desktop.opening = self.desktop.reading || !self.desktop.imports.is_empty();
    }
    fn desktop_read_finished(&mut self) {
        self.desktop.reading = false;
        self.desktop.opening = !self.desktop.imports.is_empty();
    }
    fn desktop_busy(&self) -> bool {
        !self.desktop.ready
            || self.desktop.opening
            || self.exit.frozen()
            || self.pending.is_some()
            || self.updates.open
            || self.updates.restarting
    }
    pub(super) fn desktop_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Ready => self.desktop.ready = true,
            Action::Finished => self.desktop_read_finished(),
            Action::Office(binding, opened) => {
                self.desktop_read_finished();
                return self.office_prepared(binding, opened);
            }
            Action::Request(Event::Open(paths, reply)) => {
                if self.desktop_busy() {
                    reply.set(Status::Busy);
                    return Task::none();
                }
                self.desktop.opening = !paths.is_empty();
                self.desktop.reading = self.desktop.opening;
                reply.set(Status::Accepted);
                return Task::batch([
                    activate(),
                    files::open_paths(paths).chain(Task::done(Message::Desktop(Action::Finished))),
                ]);
            }
            Action::Request(Event::Prepare(binding, reply)) => {
                let conflict = self.strip().any(|tab| {
                    tab.path
                        .as_ref()
                        .is_some_and(|path| binding.same_path(path))
                        && (tab.office.is_some()
                            || self.edited(tab)
                            || self.file_io.saving && self.file_io.saving_tab == Some(tab.id))
                });
                if conflict
                    || binding
                        .lease
                        .as_ref()
                        .is_none_or(|lease| !lease.alive() || lease.phase() != Phase::Prepared)
                {
                    reply.set(Status::Rejected);
                } else if self.desktop_busy() {
                    reply.set(Status::Busy);
                } else {
                    reply.set(Status::Prepared);
                }
            }
            Action::Request(Event::Commit(binding)) => {
                if self.desktop_busy() {
                    if let Some(lease) = &binding.lease {
                        lease.transition(Phase::Prepared, Phase::Rejected);
                    }
                    return Task::none();
                }
                self.desktop.opening = true;
                self.desktop.reading = true;
                let source = binding.path.clone();
                return Task::batch([
                    activate(),
                    Task::perform(files::read(source), move |opened| {
                        Message::Desktop(Action::Office(binding.clone(), opened))
                    }),
                ]);
            }
            Action::Poll => {
                let lost = self
                    .strip()
                    .filter(|tab| {
                        tab.office
                            .as_ref()
                            .and_then(|binding| binding.lease.as_ref())
                            .is_some_and(|lease| lease.phase() == Phase::Lost || !lease.alive())
                    })
                    .map(|tab| tab.id)
                    .collect::<Vec<_>>();
                for id in lost {
                    self.office.closing.retain(|queued| *queued != id);
                    self.in_tab(id,|app| {
                        app.tab.office = None;
                        app.tab.path = None;
                        app.tab.saved = reshiki::document::Document::default();
                        app.tab.file_epoch = app.next_epoch();
                        app.request_autosave();
                        app.status = "The Office launch session ended. Save this retained drawing to another file.".into();
                        app.error = true;
                    });
                }
            }
        }
        Task::none()
    }
}

#[cfg(test)]
mod tests;
