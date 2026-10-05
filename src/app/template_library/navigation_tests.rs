use super::*;

fn persisted_app() -> (App, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = App::new();
    app.templates.path = Some(dir.path().join("templates.json"));
    let mut fragment = Document::default();
    fragment.add_atom("O", Default::default());
    app.templates.draft = Some(fragment);
    app.templates.editing = true;
    app.templates.name = "Hydroxyl".into();
    (app, dir)
}

fn save_with_injected_release_warning(
    library: &Library,
    path: &Path,
    expected: &Library,
) -> Result<SaveOutcome, String> {
    let mut outcome = library.save_checked(path, expected)?;
    assert!(outcome.release_warning.is_none());
    outcome.release_warning = Some("Templates saved; injected lock cleanup warning".into());
    Ok(outcome)
}

#[test]
fn committed_library_warning_waits_for_ack_before_close_or_restart_without_retrying() {
    for restart in [false, true] {
        let (mut app, _dir) = persisted_app();
        app.inspector_open = false;
        let operation = Operation::capture(&app, Action::SaveDetails)
            .unwrap()
            .unwrap();
        let _ = app.update(Message::Templates(Action::SaveDetails));
        let serial = app.templates.pending.unwrap();
        let transaction = operation
            .execute_with_save(save_with_injected_release_warning)
            .unwrap();
        let committed = transaction.library.clone();
        let warning = transaction.release_warning.clone();
        let path = app.templates.path.clone().unwrap();
        let committed_bytes = std::fs::read(&path).unwrap();
        if restart {
            let _ = app.restart_after_recovery();
        } else {
            let _ = app.update(Message::Close(iced::window::Id::unique()));
        }
        assert!(app.exit.closing());

        // Use the full update path, which concurrently starts autosave.
        let task = app.update(Message::Templates(Action::Finished(
            serial,
            Ok(Box::new(transaction)),
        )));
        assert!(task.units() > 0, "The native warning is scheduled");
        assert!(app.exit.closing() && !app.exit.committed());
        assert_eq!(app.templates.pending_warning, Some(serial));
        assert_eq!(app.templates.close_warning(), warning);
        assert!(app.templates.pending());
        assert!(app.templates.pending.is_none(), "The write is finished");
        assert_eq!(app.templates.library, committed);
        assert_eq!(Library::load(&path).unwrap(), committed);
        assert_eq!(app.templates.notice, warning);
        assert!(app.templates.draft.is_none() && !app.templates.editing);
        assert!(!app.error);
        assert_eq!(app.status, "Template saved to your library");
        assert_eq!(app.start_autosave().units(), 0);

        let _ = app.update(Message::Templates(Action::WarningAcknowledged(
            serial.wrapping_sub(1),
        )));
        let _ = app.update(Message::Templates(Action::Finished(
            serial,
            Err("duplicate late result".into()),
        )));
        assert_eq!(app.templates.pending_warning, Some(serial));
        assert!(app.exit.closing() && !app.exit.committed());
        assert!(!app.error);

        let task = app.update(Message::Templates(Action::WarningAcknowledged(serial)));
        assert!(task.units() > 0, "Acknowledgment continues the exit");
        assert!(!app.templates.pending());
        assert!(!app.exit.closing() && app.exit.frozen());
        assert_eq!(app.exit.committed(), !restart);

        let _ = app.update(Message::Templates(Action::WarningAcknowledged(serial)));
        assert!(!app.templates.pending());
        assert!(!app.exit.closing() && app.exit.frozen());
        assert_eq!(app.exit.committed(), !restart);
        assert_eq!(app.templates.library, committed);
        assert_eq!(app.templates.notice, warning);
        assert!(app.templates.close_warnings.is_empty());
        assert_eq!(app.templates.serial, serial, "No save retry is started");
        assert!(app.templates.pending.is_none());
        assert_eq!(std::fs::read(&path).unwrap(), committed_bytes);
    }
}

#[test]
fn committed_library_warning_waits_until_exit_after_an_optional_drawing_question() {
    for drawing_question in [false, true] {
        let (mut app, _dir) = persisted_app();
        let operation = Operation::capture(&app, Action::SaveDetails)
            .unwrap()
            .unwrap();
        let _ = app.update(Message::Templates(Action::SaveDetails));
        let serial = app.templates.pending.unwrap();
        if drawing_question {
            app.tab.doc.add_atom("N", Default::default());
            app.tab.revision += 1;
            let _ = app.update(Message::Close(iced::window::Id::unique()));
            assert!(matches!(
                &app.pending,
                Some(super::super::Pending::CloseWindow(..))
            ));
        }
        let transaction = operation
            .execute_with_save(save_with_injected_release_warning)
            .unwrap();
        let warning = transaction.release_warning.clone();
        let _ = app.update(Message::Templates(Action::Finished(
            serial,
            Ok(Box::new(transaction)),
        )));
        assert_eq!(app.templates.notice, warning);
        assert_eq!(app.templates.close_warning(), warning);
        assert!(app.templates.pending_warning.is_none());
        assert!(!app.templates.pending() && !app.exit.frozen());
        assert!(!app.error);

        // Do not show a second native dialog over the drawing's question.
        // Its answer, or a later close, reaches the single warning gate.
        if drawing_question {
            let _ = app.update(Message::Discard);
        } else {
            let _ = app.update(Message::Close(iced::window::Id::unique()));
        }
        assert!(app.exit.closing() && !app.exit.committed());
        assert_eq!(app.templates.pending_warning, Some(serial));
        let _ = app.update(Message::Templates(Action::WarningAcknowledged(serial)));
        assert!(app.exit.committed());
        assert!(!app.templates.pending());
    }
}

#[test]
fn committed_library_warning_preserves_undo_for_applied_changes() {
    let (mut app, _dir) = persisted_app();
    let saved = Operation::capture(&app, Action::SaveDetails)
        .unwrap()
        .unwrap()
        .execute()
        .unwrap();
    app.apply_library_transaction(saved);
    let previous = app.templates.library.clone();
    let removed = Operation::capture(&app, Action::Remove)
        .unwrap()
        .unwrap()
        .execute_with_save(save_with_injected_release_warning)
        .unwrap();
    app.apply_library_transaction(removed);
    assert!(app.templates.library.templates.is_empty());
    assert_eq!(app.templates.undo.as_ref(), Some(&previous));
    assert_eq!(
        app.templates.notice.as_deref(),
        Some("Templates saved; injected lock cleanup warning")
    );
    let restored = Operation::capture(&app, Action::Restore)
        .unwrap()
        .unwrap()
        .execute()
        .unwrap();
    app.apply_library_transaction(restored);
    assert_eq!(app.templates.library, previous);
    assert_eq!(
        Library::load(app.templates.path.as_ref().unwrap()).unwrap(),
        previous
    );
    assert!(app.templates.undo.is_none());
    assert!(app.templates.notice.is_none());
    assert_eq!(
        app.templates.close_warning().as_deref(),
        Some("Templates saved; injected lock cleanup warning")
    );
}

#[test]
fn undelivered_cleanup_warning_survives_successful_reload_until_exit_acknowledgment() {
    for restart in [false, true] {
        let (mut app, _dir) = persisted_app();
        app.inspector_open = false;
        let operation = Operation::capture(&app, Action::SaveDetails)
            .unwrap()
            .unwrap();
        let _ = app.update(Message::Templates(Action::SaveDetails));
        let serial = app.templates.pending.unwrap();
        let transaction = operation
            .execute_with_save(save_with_injected_release_warning)
            .unwrap();
        let warning = transaction.release_warning.clone();
        let _ = app.update(Message::Templates(Action::Finished(
            serial,
            Ok(Box::new(transaction)),
        )));
        assert_eq!(app.templates.close_warning(), warning);

        let reload = Operation::capture(&app, Action::Reload).unwrap().unwrap();
        let _ = app.update(Message::Templates(Action::Reload));
        let serial = app.templates.pending.unwrap();
        let _ = app.update(Message::Templates(Action::Finished(
            serial,
            Ok(Box::new(reload.execute().unwrap())),
        )));
        assert!(app.templates.notice.is_none());
        assert_eq!(app.templates.close_warning(), warning);
        let path = app.templates.path.clone().unwrap();
        let committed_bytes = std::fs::read(&path).unwrap();

        if restart {
            let _ = app.restart_after_recovery();
        } else {
            let _ = app.update(Message::Close(iced::window::Id::unique()));
        }
        assert!(app.exit.closing() && !app.exit.committed());
        assert_eq!(app.templates.pending_warning, Some(serial));
        let _ = app.update(Message::Templates(Action::WarningAcknowledged(serial)));
        assert!(!app.templates.pending());
        assert!(app.templates.close_warnings.is_empty());
        assert_eq!(app.exit.committed(), !restart);
        assert_eq!(std::fs::read(path).unwrap(), committed_bytes);
        assert_eq!(app.templates.serial, serial, "Exit must not retry a save");
    }
}

#[test]
fn cleanup_warning_messages_remain_unique_and_clear_only_on_acknowledgment() {
    let (mut app, _dir) = persisted_app();
    app.inspector_open = false;
    const FIRST: &str = "Cleanup failed";
    const MULTILINE: &str = "Cleanup failed\n\nRetained lock";
    const OVERLAPPING: &str = "Retained lock";
    for (i, warning) in [FIRST, MULTILINE, FIRST, OVERLAPPING, MULTILINE]
        .into_iter()
        .enumerate()
    {
        let action = if i == 0 {
            Action::SaveDetails
        } else {
            Action::Favorite(app.template_index)
        };
        let operation = Operation::capture(&app, action.clone()).unwrap().unwrap();
        let _ = app.update(Message::Templates(action));
        let serial = app.templates.pending.unwrap();
        let transaction = operation
            .execute_with_save(|library, path, expected| {
                let mut saved = library.save_checked(path, expected)?;
                assert!(saved.release_warning.is_none());
                saved.release_warning = Some(warning.into());
                Ok(saved)
            })
            .unwrap();
        let _ = app.update(Message::Templates(Action::Finished(
            serial,
            Ok(Box::new(transaction)),
        )));
        if i == 2 {
            assert_eq!(app.templates.close_warnings, [FIRST, MULTILINE]);
            assert_eq!(
                app.templates.close_warning(),
                Some(format!("{FIRST}\n\n{MULTILINE}"))
            );
        }
    }

    let reload = Operation::capture(&app, Action::Reload).unwrap().unwrap();
    let _ = app.update(Message::Templates(Action::Reload));
    let serial = app.templates.pending.unwrap();
    let _ = app.update(Message::Templates(Action::Finished(
        serial,
        Ok(Box::new(reload.execute().unwrap())),
    )));
    assert!(app.templates.notice.is_none());
    assert_eq!(
        app.templates.close_warnings,
        [FIRST, MULTILINE, OVERLAPPING]
    );
    let rendered = Some(format!("{FIRST}\n\n{MULTILINE}\n\n{OVERLAPPING}"));
    assert_eq!(app.templates.close_warning(), rendered);
    let path = app.templates.path.clone().unwrap();
    let committed_bytes = std::fs::read(&path).unwrap();
    let committed = app.templates.library.clone();

    assert!(app.template_close_warning().unwrap().units() > 0);
    assert_eq!(app.templates.pending_warning, Some(serial));
    assert_eq!(app.templates.close_warning(), rendered);
    assert_eq!(app.template_close_warning().unwrap().units(), 0);
    let _ = app.update(Message::Templates(Action::WarningAcknowledged(
        serial.wrapping_sub(1),
    )));
    assert_eq!(app.templates.close_warning(), rendered);
    assert_eq!(app.templates.pending_warning, Some(serial));

    let _ = app.update(Message::Templates(Action::WarningAcknowledged(serial)));
    assert!(app.templates.close_warnings.is_empty());
    assert!(!app.templates.pending());
    assert!(app.template_close_warning().is_none());
    assert_eq!(app.templates.library, committed);
    assert_eq!(std::fs::read(path).unwrap(), committed_bytes);
    assert_eq!(
        app.templates.serial, serial,
        "Acknowledgment never retries a save"
    );
}

#[test]
fn library_write_failure_replays_a_background_job_held_during_close() {
    let (mut app, _dir) = persisted_app();
    let id = app.tab.id;
    let revision = app.tab.revision;
    app.tab.busy = true;
    app.add_tab();
    let _ = app.update(Message::Templates(Action::SaveDetails));
    let serial = app.templates.pending.unwrap();
    let _ = app.update(Message::Close(iced::window::Id::unique()));
    assert!(app.exit.closing());
    let _ = app.update(Message::Tab(
        id,
        Box::new(Message::EngineDone {
            revision,
            kind: super::super::Job::Analyze,
            result: Box::new(Err("Chemistry failure".into())),
        }),
    ));
    assert_eq!(app.tabs.deferred_results.len(), 1);
    let _ = app.update(Message::Templates(Action::Finished(
        serial,
        Err("Library write failed".into()),
    )));
    assert!(!app.exit.frozen() && app.tabs.deferred_results.is_empty());
    assert!(app.templates.pending_warning.is_none());
    assert!(!app.tabs.background[0].busy);
    assert_eq!(app.tabs.background[0].status, "Chemistry failure");
    assert_eq!(app.status, "Library write failed");
    app.in_tab(id, |app| assert!(app.update(Message::Analyze).units() > 0))
        .unwrap();
}

#[test]
fn import_read_serializes_library_changes_and_releases_slot_on_cancellation() {
    let (mut app, _dir) = persisted_app();
    let task = app.update(Message::Templates(Action::Import));
    assert!(task.units() > 0 && app.templates.pending());
    assert!(app.templates.importing);
    let _ = app.update(Message::Templates(Action::SaveDetails));
    let _ = app.update(Message::Templates(Action::Reload));
    let _ = app.update(Message::Templates(Action::Import));
    assert!(
        app.templates.pending.is_none(),
        "No library write may overtake an import read"
    );
    assert!(app.templates.library.templates.is_empty());
    let _ = app.update(Message::Templates(Action::Imported(Ok(None))));
    assert!(!app.templates.pending());
    let _ = app.update(Message::Templates(Action::SaveDetails));
    assert!(app.templates.pending.is_some());
}

#[test]
fn checked_worker_save_preserves_draft_until_success_and_rejects_competing_writer() {
    let (mut app, _dir) = persisted_app();
    let operation = Operation::capture(&app, Action::SaveDetails)
        .unwrap()
        .unwrap();
    let task = app.update(Message::Templates(Action::SaveDetails));
    assert!(task.units() > 0 && app.templates.pending());
    assert!(app.templates.library.templates.is_empty());
    assert!(app.templates.draft.is_some());
    assert!(!app.templates.path.as_ref().unwrap().exists());
    let serial = app.templates.pending.unwrap();
    let _ = app.update(Message::Templates(Action::Reload));
    assert_eq!(app.templates.pending, Some(serial));
    let mut competing = Library::default();
    competing
        .add(
            "Other window",
            "Mine",
            app.templates.draft.clone().unwrap(),
            Anchor::Auto,
        )
        .unwrap();
    competing
        .save(app.templates.path.as_ref().unwrap())
        .unwrap();
    let failure = operation.execute().map(Box::new);
    assert!(
        failure
            .as_ref()
            .unwrap_err()
            .contains("changed in another window")
    );
    let _ = app.update(Message::Templates(Action::Finished(serial, failure)));
    assert!(!app.templates.pending());
    assert!(app.templates.draft.is_some());
    assert!(app.templates.library.templates.is_empty());
    assert_eq!(
        Library::load(app.templates.path.as_ref().unwrap()).unwrap(),
        competing
    );
}

#[test]
fn stale_library_completion_updates_storage_without_switching_new_drawing_tools() {
    let (mut app, _dir) = persisted_app();
    let operation = Operation::capture(&app, Action::SaveDetails)
        .unwrap()
        .unwrap();
    let _ = app.update(Message::Templates(Action::SaveDetails));
    let serial = app.templates.pending.unwrap();
    let transaction = operation
        .execute_with_save(save_with_injected_release_warning)
        .map(Box::new);
    let _ = app.update(Message::New);
    app.status = "New drawing is active".into();
    let original = app.tab.doc.clone();
    let tool = app.tool;
    let _ = app.update(Message::Templates(Action::Finished(serial, transaction)));
    assert_eq!(app.tab.doc, original);
    assert_eq!(app.tool, tool);
    assert_eq!(app.status, "New drawing is active");
    assert_eq!(app.templates.library.templates.len(), 1);
    assert_eq!(
        app.templates.notice.as_deref(),
        Some("Templates saved; injected lock cleanup warning")
    );
    assert!(!app.templates.pending());
    let _ = app.update(Message::Templates(Action::Finished(
        serial,
        Err("duplicate late result".into()),
    )));
    assert_eq!(app.status, "New drawing is active");
    assert_eq!(
        app.templates.notice.as_deref(),
        Some("Templates saved; injected lock cleanup warning")
    );
}

#[test]
fn library_completion_during_inline_edit_does_not_discard_draft() {
    let (mut app, _dir) = persisted_app();
    let operation = Operation::capture(&app, Action::SaveDetails)
        .unwrap()
        .unwrap();
    let _ = app.update(Message::Templates(Action::SaveDetails));
    let serial = app.templates.pending.unwrap();
    let transaction = operation.execute().map(Box::new);
    let _ = app.inline_action(super::super::inline_text::Action::Begin(
        None,
        Default::default(),
    ));
    assert!(app.tab.inline_text.is_some());
    let _ = app.update(Message::Templates(Action::Finished(serial, transaction)));
    assert!(app.tab.inline_text.is_some());
    assert!(!app.templates.pending());
    assert_eq!(app.templates.library.templates.len(), 1);
}

#[test]
fn browsing_back_and_forward_restores_category_scroll_and_anchor() {
    let (mut app, _) = App::new();
    app.templates.scroll = 25.;
    let _ = app.template_action(Action::Collection("Aromatics".into()));
    app.templates.scroll = 70.;
    let index = app
        .templates
        .library
        .iter()
        .position(|t| t.name == "Furan")
        .unwrap();
    let _ = app.update(Message::InsertTemplate(index));
    let anchor = Anchor::Atom(app.templates.library.get(index).unwrap().document.atoms[0].id);
    let _ = app.template_action(Action::Anchor(anchor));
    let _ = app.template_action(Action::Browse);
    assert!(!app.templates.active);
    assert_eq!(app.templates.collection, "Aromatics");
    assert_eq!(app.templates.scroll, 70.);
    let _ = app.template_action(Action::Browse);
    assert_eq!(app.templates.collection, "All collections");
    assert_eq!(app.templates.scroll, 25.);
    let _ = app.template_action(Action::Forward);
    let _ = app.template_action(Action::Forward);
    assert!(app.templates.active);
    assert_eq!(app.template_index, index);
    assert_eq!(app.templates.anchor, anchor);
    let _ = app.template_action(Action::Browse);
    let _ = app.template_action(Action::Collection("Amino acids".into()));
    assert!(!app.templates.can_forward());
}

#[test]
fn browser_mouse_buttons_have_the_correct_direction() {
    use iced::{
        Event,
        mouse::{Button, Event::ButtonPressed},
    };
    for button in [Button::Back, Button::Other(3)] {
        assert_eq!(
            navigation_event(&Event::Mouse(ButtonPressed(button))),
            Some(false)
        );
    }
    for button in [Button::Forward, Button::Other(4)] {
        assert_eq!(
            navigation_event(&Event::Mouse(ButtonPressed(button))),
            Some(true)
        );
    }
    assert_eq!(
        navigation_event(&Event::Mouse(ButtonPressed(Button::Left))),
        None
    );
}
