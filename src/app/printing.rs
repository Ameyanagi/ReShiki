use super::{App, Message};
use iced::Task;
use moruno::printing::{Outcome, Prepared, Scope};

#[derive(Debug, Clone, Copy)]
pub struct Ticket {
    serial: u64,
    epoch: u64,
    revision: u64,
}
#[derive(Debug, Clone)]
pub enum Action {
    Start(Scope),
    Prepared(Ticket, Result<Prepared, String>),
    Finished(Ticket, Result<Outcome, String>),
}
#[derive(Default)]
pub struct State {
    next: u64,
    pub active: Option<u64>,
}
impl App {
    fn finish_print(&mut self, ticket: Ticket, result: Result<Outcome, String>) {
        if self.printing.active != Some(ticket.serial) {
            return;
        }
        self.printing.active = None;
        if ticket.epoch != self.file_epoch || ticket.revision != self.revision {
            return;
        }
        match result {
            Ok(outcome) => {
                self.error = false;
                self.status = if outcome.completed {
                    "Print operation completed"
                } else {
                    "Print cancelled or not completed"
                }
                .into();
            }
            Err(error) => {
                self.error = true;
                self.status = format!("Could not print: {error}");
            }
        }
    }
    pub(super) fn print_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Start(scope) => {
                if self.printing.active.is_some() {
                    self.status = "A print dialog is already open or being prepared".into();
                    return Task::none();
                }
                if !moruno::printing::available() {
                    self.status = "Export a page PDF to print on this system".into();
                    return Task::none();
                }
                let source = match self.print_document() {
                    Ok(doc) => doc,
                    Err(error) => {
                        self.error = true;
                        self.status = error;
                        return Task::none();
                    }
                };
                let snapshot = match moruno::printing::snapshot(&source, &self.selected, scope) {
                    Ok(doc) => doc,
                    Err(error) => {
                        if source.page_layout.is_none() && !source.all_ids().is_empty() {
                            let _ = self.page_action(super::pages::Action::Open);
                        }
                        self.error = true;
                        self.status = error;
                        return Task::none();
                    }
                };
                self.printing.next = self.printing.next.wrapping_add(1);
                let ticket = Ticket {
                    serial: self.printing.next,
                    epoch: self.file_epoch,
                    revision: self.revision,
                };
                self.printing.active = Some(ticket.serial);
                self.error = false;
                self.status = "Preparing print preview…".into();
                let mut title = self
                    .path
                    .as_ref()
                    .and_then(|p| p.file_stem())
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Untitled drawing".into());
                if scope == Scope::Selection {
                    title.push_str(" — Selection");
                }
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            moruno::printing::prepare(snapshot, title)
                        })
                        .await
                        .map_err(|e| e.to_string())?
                    },
                    move |result| Message::Printing(Action::Prepared(ticket, result)),
                );
            }
            Action::Prepared(ticket, result) => {
                if self.printing.active != Some(ticket.serial) {
                    return Task::none();
                }
                match result {
                    Ok(job) => {
                        if ticket.epoch == self.file_epoch && ticket.revision == self.revision {
                            self.status = "Print dialog open".into();
                        }
                        return Task::perform(moruno::printing::show_dialog(job), move |result| {
                            Message::Printing(Action::Finished(ticket, result))
                        });
                    }
                    Err(error) => self.finish_print(ticket, Err(error)),
                }
            }
            Action::Finished(ticket, result) => self.finish_print(ticket, result),
        }
        Task::none()
    }
}
