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
fn aromatic_heterocycles_belong_to_both_collections_without_duplicate_results() {
    use std::collections::{BTreeMap, BTreeSet};

    let mut state = State::default();
    for (name, expected) in [
        ("Indole", &["Aromatics", "Heterocycles"][..]),
        ("Pyridine", &["Aromatics", "Heterocycles"][..]),
        ("Benzene", &["Aromatics"][..]),
        ("Cyclohexane", &["Cycloalkanes"][..]),
        ("Pyrrolidine", &["Heterocycles"][..]),
        ("Piperidine", &["Heterocycles"][..]),
        ("Piperazine", &["Heterocycles"][..]),
        ("Morpholine", &["Heterocycles"][..]),
        ("Tetrahydrofuran", &["Heterocycles"][..]),
        ("Tetrahydropyran", &["Heterocycles"][..]),
        ("1,4-Dioxane", &["Heterocycles"][..]),
    ] {
        let (index, template) = state
            .library
            .iter()
            .enumerate()
            .find(|(_, t)| t.name == name)
            .unwrap();
        assert_eq!(categories(template).collect::<Vec<_>>(), expected, "{name}");
        for collection in ["Aromatics", "Heterocycles", "Cycloalkanes"] {
            state.collection = collection.into();
            assert_eq!(
                state.matches(index, template),
                expected.contains(&collection),
                "{name} in {collection}"
            );
        }
    }

    state.collection = "All collections".into();
    let results: Vec<_> = state
        .library
        .iter()
        .enumerate()
        .filter(|(i, t)| state.matches(*i, t))
        .map(|(_, t)| &t.id)
        .collect();
    assert_eq!(results.len(), LIBRARY.len());
    assert_eq!(results.iter().collect::<BTreeSet<_>>().len(), results.len());

    let mut counts = BTreeMap::new();
    for template in state.library.iter() {
        for collection in categories(template) {
            *counts.entry(collection.to_owned()).or_insert(0) += 1;
        }
    }
    for (collection, count) in counts {
        state.collection = collection;
        assert_eq!(
            state
                .library
                .iter()
                .enumerate()
                .filter(|(i, t)| state.matches(*i, t))
                .count(),
            count,
            "{} count agrees with collection membership",
            state.collection
        );
    }

    state.collection = "All collections".into();
    let index = state
        .library
        .iter()
        .position(|t| t.name == "Pyridine")
        .unwrap();
    for query in ["aromatics pyridine", "heterocycles pyridine"] {
        state.query = query.into();
        assert!(state.matches(index, state.library.get(index).unwrap()));
    }
}

#[test]
fn aromatic_membership_preserves_halogens_and_user_collections() {
    let mut template = LIBRARY
        .iter()
        .find(|t| t.name == "Benzene")
        .unwrap()
        .clone();
    for smiles in ["Clc1ccccc1", "Brc1ccccc1"] {
        template.smiles = smiles.into();
        assert_eq!(categories(&template).collect::<Vec<_>>(), ["Aromatics"]);
    }
    for smiles in ["c1ccncc1", "c1ccoc1", "c1ccsc1", "c1ccpc1", "[b-]1ccccc1"] {
        template.smiles = smiles.into();
        assert_eq!(
            categories(&template).collect::<Vec<_>>(),
            ["Aromatics", "Heterocycles"]
        );
    }
    template.id = "user:aromatic-ring".into();
    assert_eq!(categories(&template).collect::<Vec<_>>(), ["Rings"]);
}

#[test]
fn search_all_preserves_query_filter_and_browse_history() {
    use std::collections::BTreeSet;

    let matches = |state: &State| {
        state
            .library
            .iter()
            .enumerate()
            .filter(|(i, t)| state.matches(*i, t))
            .map(|(_, t)| t.name.clone())
            .collect::<BTreeSet<_>>()
    };
    for filter in [Filter::All, Filter::Favorites] {
        let (mut app, _) = App::new();
        let index = app
            .templates
            .library
            .iter()
            .position(|t| t.name == "Thiophene")
            .unwrap();
        app.insert_template(index);
        app.templates.library.favorites = app
            .templates
            .library
            .iter()
            .filter(|t| matches!(t.name.as_str(), "Thiophene" | "L-Phenylalanine"))
            .map(|t| t.id.clone())
            .collect();
        app.templates.query = "Phe".into();
        app.templates.collection = "Aromatics".into();
        app.templates.filter = filter;
        app.templates.scroll = 70.;
        let scoped = matches(&app.templates);
        assert!(scoped.contains("Thiophene"));
        assert!(!scoped.contains("L-Phenylalanine"));
        let expected = matches(&State {
            library: app.templates.library.clone(),
            query: "Phe".into(),
            filter,
            ..State::default()
        });
        assert_eq!(expected.len(), if filter == Filter::All { 9 } else { 2 });
        assert!(expected.contains("L-Phenylalanine"));
        assert!(scoped.is_subset(&expected));

        let _ = app.template_action(Action::SearchAll);
        assert_eq!(app.templates.collection, "All collections");
        assert_eq!(app.templates.query, "Phe");
        assert_eq!(app.templates.filter, filter);
        assert_eq!(app.templates.scroll, 0.);
        assert!(!app.templates.active);
        assert_eq!(app.tool, Tool::Select);
        assert_eq!(matches(&app.templates), expected);

        let _ = app.template_action(Action::Browse);
        assert_eq!(app.templates.collection, "Aromatics");
        assert_eq!(app.templates.query, "Phe");
        assert_eq!(app.templates.filter, filter);
        assert_eq!(app.templates.scroll, 70.);
        assert!(app.templates.active);
        assert_eq!(app.template_index, index);
        assert_eq!(app.tool, Tool::Template);
        assert_eq!(matches(&app.templates), scoped);

        let _ = app.template_action(Action::Forward);
        assert_eq!(app.templates.collection, "All collections");
        assert_eq!(matches(&app.templates), expected);
        let _ = app.template_action(Action::Browse);
        let _ = app.template_action(Action::SearchAll);
        assert!(!app.templates.can_forward());
    }
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

#[test]
fn iupac_and_biochemical_aliases_are_exact_search_matches() {
    let mut state = State::default();
    for (name, aliases) in [
        ("Indole", &["1H-indole"][..]),
        ("Pyrrole", &["1H-pyrrole"]),
        ("Imidazole", &["1H-imidazole"]),
        ("Pyrazole", &["1H-pyrazole"]),
        ("Oxazole", &["1,3-oxazole"]),
        ("Isoxazole", &["1,2-oxazole"]),
        ("Thiazole", &["1,3-thiazole"]),
        ("Isothiazole", &["1,2-thiazole"]),
        ("Tetrahydrofuran", &["oxolane"]),
        ("Tetrahydropyran", &["oxane"]),
        ("Norbornane", &["bicyclo[2.2.1]heptane"]),
        (
            "Adamantane",
            &[
                "tricyclo[3.3.1.1^{3,7}]decane",
                "tricyclo[3.3.1.1^3,7]decane",
                "tricyclo[3.3.1.1³,⁷]decane",
                "tricyclo[3.3.1.13,7]decane",
            ],
        ),
        ("Decalin", &["decahydronaphthalene", "bicyclo[4.4.0]decane"]),
        (
            "Cubane",
            &["pentacyclo[4.2.0.0^{2,5}.0^{3,8}.0^{4,7}]octane"],
        ),
        ("9H-Purine", &["9H-purine"]),
        ("7H-Purine", &["7H-purine"]),
        ("Adenine (9H)", &["Ade"]),
        ("Guanine (9H)", &["Gua"]),
        ("Cytosine", &["Cyt"]),
        ("Thymine", &["Thy"]),
        ("Uracil", &["Ura"]),
    ] {
        let (index, template) = LIBRARY
            .iter()
            .enumerate()
            .find(|(_, t)| t.name == name)
            .unwrap();
        for alias in aliases {
            state.query = format!("  {}  ", alias.to_uppercase());
            assert!(state.matches(index, template), "{name}: {alias}");
            assert_eq!(state.search_rank(template), 0, "{name}: {alias}");
        }
    }
}

#[test]
fn superscript_search_folds_candidates_as_well_as_queries() {
    let mut template = LIBRARY[0].clone();
    template.name = "Printed von Baeyer name".into();
    template.keywords = vec!["tricyclo[3.3.1.1³,⁷]decane".into()];
    let mut state = State::default();
    for query in [
        "tricyclo[3.3.1.1^{3,7}]decane",
        "tricyclo[3.3.1.1^3,7]decane",
    ] {
        state.query = query.into();
        assert!(state.matches(0, &template));
        assert_eq!(state.search_rank(&template), 0);
    }
    assert_eq!(search_fold("⁰¹²³⁴⁵⁶⁷⁸⁹"), "0123456789");
}

#[test]
fn haworth_symbols_rank_matching_ring_and_anomer_exactly() {
    let mut state = State::default();
    for (sugar, symbol, ring_symbol) in [
        ("glucopyranose", "Glc", "Glcp"),
        ("galactopyranose", "Gal", "Galp"),
        ("mannopyranose", "Man", "Manp"),
        ("ribofuranose", "Rib", "Ribf"),
        ("fructofuranose", "Fru", "Fruf"),
    ] {
        for (anomer, prefixes) in [("α", ["a", "α", "alpha"]), ("β", ["b", "β", "beta"])] {
            let name = format!("{anomer}-D-{sugar} · Haworth");
            let (index, template) = LIBRARY
                .iter()
                .enumerate()
                .find(|(_, t)| t.name == name)
                .unwrap();
            for alias in [symbol.to_owned(), ring_symbol.to_owned()]
                .into_iter()
                .chain(prefixes.map(|prefix| format!("{prefix}-D-{ring_symbol}")))
            {
                state.query = alias;
                assert!(state.matches(index, template), "{name}: {}", state.query);
                assert_eq!(state.search_rank(template), 0, "{name}: {}", state.query);
            }
            state.query = format!("{}-D-{ring_symbol}", if anomer == "α" { "b" } else { "a" });
            assert_ne!(state.search_rank(template), 0, "{name}: {}", state.query);
            for wrong in [
                format!("a-L-{ring_symbol}"),
                format!(
                    "{symbol}{}",
                    if ring_symbol.ends_with('p') { 'f' } else { 'p' }
                ),
            ] {
                state.query = wrong;
                assert!(!state.matches(index, template), "{name}: {}", state.query);
            }
        }
        state.query = symbol.into();
        for (index, template) in LIBRARY
            .iter()
            .enumerate()
            .filter(|(_, t)| t.name.contains("scaffold"))
        {
            assert!(!state.matches(index, template));
        }
    }
}

#[test]
fn mixed_aromatic_heterocycles_keep_their_heterocycle_collection() {
    for name in ["Chromene (2H)", "Indole", "Carbazole (9H)", "Dibenzofuran"] {
        let template = LIBRARY.iter().find(|t| t.name == name).unwrap();
        assert_eq!(
            categories(template).collect::<Vec<_>>(),
            ["Aromatics", "Heterocycles"],
            "{name}"
        );
    }
}

#[test]
fn source_anchor_labels_use_parent_locants_and_fall_back_without_them() {
    let template = LIBRARY.iter().find(|t| t.name == "Quinoline").unwrap();
    let atom = |locant: &str| {
        *template
            .locants
            .iter()
            .find(|(_, value)| value.as_str() == locant)
            .unwrap()
            .0
    };
    let c3 = atom("3");
    let c4 = atom("4");
    assert_eq!(anchor_label(template, Anchor::Atom(c3)), "Attach at C-3");
    assert_eq!(
        anchor_label(template, Anchor::Bond(c3, c4)),
        "Attach at C-3–C-4"
    );
    assert_eq!(
        anchor_label(template, Anchor::Atom(atom("1"))),
        "Attach at N-1"
    );
    assert_eq!(anchor_label(template, Anchor::Auto), "Automatic attachment");
    let mut unnumbered = template.clone();
    unnumbered.locants.clear();
    assert_eq!(
        anchor_label(&unnumbered, Anchor::Atom(c3)),
        format!("Atom {c3}")
    );
    assert_eq!(
        anchor_label(&unnumbered, Anchor::Bond(c3, c4)),
        format!("Bond {c3}–{c4}")
    );
}

#[test]
fn renamed_tautomers_keep_existing_favorite_ids() {
    let favorites = [
        "builtin:Heterocycles/Purine",
        "builtin:Nucleobases/Adenine",
        "builtin:Nucleobases/Guanine",
    ];
    let library = Library::from_bytes(
        &serde_json::to_vec(&serde_json::json!({
            "version": 1, "templates": [], "favorites": favorites,
        }))
        .unwrap(),
    )
    .unwrap();
    let state = State {
        library,
        filter: Filter::Favorites,
        ..State::default()
    };
    for (name, id) in ["9H-Purine", "Adenine (9H)", "Guanine (9H)"]
        .into_iter()
        .zip(favorites)
    {
        let (index, template) = LIBRARY
            .iter()
            .enumerate()
            .find(|(_, t)| t.name == name)
            .unwrap();
        assert_eq!(template.id, id);
        assert!(state.matches(index, template));
    }
}
