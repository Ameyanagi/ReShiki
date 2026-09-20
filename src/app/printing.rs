use super::{App, Message};
use iced::Task;
use reshiki::printing::{Outcome, Prepared, Scope};

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
                if !reshiki::printing::available() {
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
                let snapshot = match reshiki::printing::snapshot(&source, &self.selected, scope) {
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
                            reshiki::printing::prepare(snapshot, title)
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
                        return Task::perform(reshiki::printing::show_dialog(job), move |result| {
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

#[cfg(test)]
mod tests {
    use super::*;
    use reshiki::document::{Document, Point};
    fn ready() -> App {
        let (mut app, _) = App::new();
        app.busy = false;
        app.doc = Document::default();
        app.doc.add_atom("O", Point::default());
        app.saved = app.doc.clone();
        app
    }
    fn ticket(app: &App, serial: u64) -> Ticket {
        Ticket {
            serial,
            epoch: app.file_epoch,
            revision: app.revision,
        }
    }
    #[test]
    fn cancelled_failed_and_completed_prints_leave_drawing_selection_and_history_intact() {
        for outcome in [
            Ok(Outcome { completed: false }),
            Ok(Outcome { completed: true }),
            Err("Unavailable printer".into()),
        ] {
            let mut app = ready();
            let before = app.doc.clone();
            app.selected = app.doc.all_ids();
            app.printing.active = Some(1);
            let ticket = ticket(&app, 1);
            let _ = app.update(Message::Printing(Action::Finished(ticket, outcome)));
            assert!(app.printing.active.is_none());
            assert_eq!(app.doc, before);
            assert_eq!(app.selected, before.all_ids());
            assert!(!app.dirty());
            assert!(!app.history.can_undo());
        }
    }
    #[test]
    fn late_print_results_do_not_overwrite_newer_edits_or_another_job() {
        let mut app = ready();
        let old = ticket(&app, 1);
        app.printing.active = Some(2);
        let _ = app.print_action(Action::Finished(old, Err("Old failure".into())));
        assert_eq!(app.printing.active, Some(2));
        for changed_file in [false, true] {
            let current = ticket(&app, 2);
            app.printing.active = Some(2);
            if changed_file {
                app.file_epoch += 1;
            } else {
                app.revision += 1;
            }
            app.status = "Newer editing status".into();
            let _ = app.print_action(Action::Finished(current, Ok(Outcome { completed: true })));
            assert!(app.printing.active.is_none());
            assert_eq!(app.status, "Newer editing status");
        }
    }
    #[test]
    fn preparation_failures_release_the_job_and_duplicate_starts_do_not_replace_it() {
        let mut app = ready();
        app.printing.active = Some(1);
        let current = ticket(&app, 1);
        let _ = app.print_action(Action::Start(Scope::Document));
        assert_eq!(app.printing.active, Some(1));
        let _ = app.print_action(Action::Prepared(current, Err("Invalid snapshot".into())));
        assert!(app.printing.active.is_none());
        assert!(app.error);
        assert!(app.status.contains("Invalid snapshot"));
        assert!(!app.history.can_undo());
    }
    #[test]
    fn printing_uses_valid_page_drafts_without_applying_them() {
        use crate::app::pages::{Action as Pages, Field};
        let mut app = ready();
        let before = app.doc.clone();
        let _ = app.page_action(Pages::Open);
        let _ = app.page_action(Pages::Input(Field::Columns, "2".into()));
        let snapshot = app.print_document().unwrap();
        assert_eq!(snapshot.page_layout.as_ref().unwrap().count(), 2);
        assert_eq!(app.doc, before);
        assert!(!app.history.can_undo());
        let _ = app.page_action(Pages::Input(Field::Width, "invalid".into()));
        assert!(app.print_document().is_err());
        let _ = app.page_action(Pages::Cancel);
        assert_eq!(app.print_document().unwrap(), before);
        let _ = app.page_action(Pages::Open);
        app.file_epoch += 1;
        assert!(app.print_document().is_err());
    }
}
