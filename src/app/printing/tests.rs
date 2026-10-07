use super::*;
use reshiki::document::{Document, Point};
fn ready() -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = Document::default();
    app.tab.doc.add_atom("O", Point::default());
    app.tab.saved = app.tab.doc.clone();
    app
}
fn ticket(app: &App, serial: u64) -> Ticket {
    Ticket {
        serial,
        epoch: app.tab.file_epoch,
        revision: app.tab.revision,
    }
}

#[test]
fn background_and_closed_print_results_release_only_their_job() {
    for closed in [false, true] {
        for prepared in [false, true] {
            let mut app = ready();
            let id = app.tab.id;
            let ticket = ticket(&app, 1);
            app.printing.active = Some(1);
            if closed {
                let _ = app.close_active_tab();
            }
            let front = super::super::tabs::tests::Front::new(&mut app);
            let action = if prepared {
                Action::Prepared(ticket, Err("Printer unavailable".into()))
            } else {
                Action::Finished(ticket, Err("Printer unavailable".into()))
            };
            let _ = app.update(Message::Tab(id, Box::new(Message::Printing(action))));
            assert!(app.printing.active.is_none());
            front.assert_unchanged(&app);
            if !closed {
                assert!(
                    app.tabs.background[0]
                        .status
                        .contains("Printer unavailable")
                );
                assert!(app.tabs.background[0].error);
            }
        }
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
        let before = app.tab.doc.clone();
        app.tab.selected = app.tab.doc.all_ids();
        app.printing.active = Some(1);
        let ticket = ticket(&app, 1);
        let _ = app.update(Message::Printing(Action::Finished(ticket, outcome)));
        assert!(app.printing.active.is_none());
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.selected, before.all_ids());
        assert!(!app.dirty());
        assert!(!app.tab.history.can_undo());
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
            app.tab.file_epoch += 1;
        } else {
            app.tab.revision += 1;
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
    assert!(!app.tab.history.can_undo());
}
#[test]
fn printing_uses_valid_page_drafts_without_applying_them() {
    use crate::app::pages::{Action as Pages, Field};
    let mut app = ready();
    let before = app.tab.doc.clone();
    let _ = app.page_action(Pages::Open);
    let _ = app.page_action(Pages::Input(Field::Columns, "2".into()));
    let snapshot = app.print_document().unwrap();
    assert_eq!(snapshot.page_layout.as_ref().unwrap().count(), 2);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    let _ = app.page_action(Pages::Input(Field::Width, "invalid".into()));
    assert!(app.print_document().is_err());
    let _ = app.page_action(Pages::Cancel);
    assert_eq!(app.print_document().unwrap(), before);
    let _ = app.page_action(Pages::Open);
    app.tab.file_epoch += 1;
    assert!(app.print_document().is_err());
}
