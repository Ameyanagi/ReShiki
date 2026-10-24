//! UI-side ownership acknowledgement; the transport cannot accept a request
//! merely because it queued bytes. A committed open is retained until processed.
use super::{App, Message, document_tab::TabId, event_slot, files};
use crate::desktop::{Event, protocol::Status};
use crate::session::{Binding, Phase};
use iced::{Subscription, Task};
use std::time::Duration;
use tokio::sync::mpsc::Receiver;

static EVENTS: event_slot::Slot<Receiver<Event>> = event_slot::Slot::new();
const EVENT_CAPACITY: usize = 16;
const LEASE_POLL: Duration = Duration::from_secs(1);
const LOST_PROXY_STATUS: &str =
    "The Office launch session ended. Save this retained drawing to another file.";
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
    EVENTS.install(events);
}
pub(super) fn subscription(app: &App) -> Subscription<Message> {
    let requests = event_slot::subscription(&EVENTS, EVENT_CAPACITY, |event| {
        Message::Desktop(Action::Request(event))
    });
    Subscription::batch([
        requests,
        if app.strip().any(|tab| {
            tab.office
                .as_ref()
                .is_some_and(|binding| binding.lease.is_some())
        }) {
            iced::time::every(LEASE_POLL).map(|_| Message::Desktop(Action::Poll))
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
    pub(super) fn desktop_update(&mut self, action: Action) -> Task<Message> {
        Task::batch([
            self.desktop_action(action),
            self.start_label_refresh(),
            self.start_autosave(),
            self.accessibility_refresh(),
        ])
    }

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
                let conflict = self.office_path_conflict(&binding);
                // Desktop prepare requires a launcher lease. OfficePrepared
                // also accepts standalone bindings, for which preparable is true.
                if conflict || binding.lease.is_none() || !binding.preparable() {
                    reply.set(Status::Rejected);
                } else if self.desktop_busy() {
                    reply.set(Status::Busy);
                } else {
                    reply.set(Status::Prepared);
                }
            }
            Action::Request(Event::Commit(binding)) => {
                if self.desktop_busy() {
                    binding.reject();
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
                    self.retain_lost_office_tab(id);
                }
            }
        }
        Task::none()
    }

    fn retain_lost_office_tab(&mut self, id: TabId) {
        self.office.closing.retain(|queued| *queued != id);
        self.in_tab(id, |app| {
            app.tab.office = None;
            app.tab.path = None;
            app.tab.mark_saved(Default::default());
            app.begin_file_epoch();
            app.request_autosave();
            app.status = LOST_PROXY_STATUS.into();
            app.error = true;
        });
    }
}

#[cfg(test)]
mod tests;
