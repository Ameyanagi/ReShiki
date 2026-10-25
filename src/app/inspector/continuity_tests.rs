//! Revision-keyed whole/selection receipts and explicit Molecular header preferences.
use super::*;
use crate::app::document_tab::TabId;

async fn analyze(app: &App, key: &PropertyKey) -> Analysis {
    app.engine
        .execute(Request::molecule("analyze", app.property_document(key)))
        .await
        .unwrap()
        .analysis
        .unwrap()
}

async fn settle(app: &mut App, whole: bool) {
    let key = if whole {
        app.whole_property_key()
    } else {
        app.property_request_key()
    };
    let _ = app.inspector_action(if whole {
        Action::RefreshPropertiesFor(app.whole_property_key())
    } else {
        Action::RefreshProperties
    });
    assert_eq!(app.tab.inspector_ui.pending.as_ref(), Some(&key));
    let result = analyze(app, &key).await;
    let _ = app.inspector_action(Action::PropertiesCalculated(key, Box::new(Ok(result))));
}

#[test]
fn atom_and_bond_counts_use_singular_only_for_one() {
    for (atoms, bonds, expected) in [
        (0, 0, "0 atoms · 0 bonds"),
        (1, 0, "1 atom · 0 bonds"),
        (2, 1, "2 atoms · 1 bond"),
        (1, 2, "1 atom · 2 bonds"),
    ] {
        assert_eq!(
            super::super::workspace::atom_bond_counts(atoms, bonds),
            expected
        );
    }
}

#[test]
fn drawing_tools_keep_nonempty_molecular_default_and_explicit_choices() {
    let (mut app, _) = App::new();
    for tool in [
        Tool::Select,
        Tool::Atom,
        Tool::RingPreset(reshiki::rings::Preset::Benzene),
    ] {
        app.tool = tool;
        app.element = "H".into();
        assert_eq!(app.opened_section(), None, "An empty drawing stays folded");
        let id = app.tab.doc.add_atom("O", Default::default());
        assert_eq!(app.opened_section(), Some(Section::Molecule));
        for expanded in [false, true] {
            let _ = app.inspector_action(Action::Section(Section::Molecule, expanded));
            assert_eq!(
                app.tab.inspector_ui.expanded(Section::Molecule),
                Some(expanded)
            );
        }
        app.tab.doc.atoms.retain(|a| a.id != id);
    }
}

fn choose(app: &mut App, expanded: bool) {
    let _ = app.update(Message::InspectorAction(Action::Section(
        Section::Molecule,
        expanded,
    )));
}
fn select(app: &mut App, id: TabId) {
    let _ = app.update(Message::Tabs(crate::app::tabs::Action::Select(id)));
}

#[test]
fn only_new_or_reset_tabs_inherit_the_last_explicit_molecular_choice() {
    for expanded in [false, true] {
        let (mut app, _) = App::new();
        assert_eq!(app.tabs.molecular_expanded, None);
        assert_eq!(app.tab.inspector_ui.expanded(Section::Molecule), None);
        app.tab.doc.add_atom("O", Default::default());
        choose(&mut app, expanded);
        let old = app.tab.id;
        let _ = app.update(Message::New);
        let fresh = app.tab.id;
        assert_eq!(
            app.tab.inspector_ui.expanded(Section::Molecule),
            Some(expanded)
        );
        choose(&mut app, !expanded);
        select(&mut app, old);
        assert_eq!(
            app.tab.inspector_ui.expanded(Section::Molecule),
            Some(expanded),
            "Existing tabs keep their own choice"
        );
        select(&mut app, fresh);
        assert_eq!(
            app.tab.inspector_ui.expanded(Section::Molecule),
            Some(!expanded)
        );
        app.reset_tab();
        assert_eq!(
            app.tab.inspector_ui.expanded(Section::Molecule),
            Some(!expanded)
        );
        assert_eq!(app.tab.inspector_ui.expanded(Section::Atoms), None);
    }
}

#[test]
fn discarding_the_last_dirty_tab_seeds_the_replacement_molecular_choice() {
    for expanded in [false, true] {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        choose(&mut app, expanded);
        let before = app.tab.doc.clone();
        app.tab.doc.add_atom("O", Default::default());
        app.changed(before);
        let old = app.tab.id;
        let _ = app.update(Message::Tabs(crate::app::tabs::Action::Close(None)));
        assert!(matches!(app.pending, Some(crate::app::Pending::CloseTab(id)) if id == old));
        let _ = app.update(Message::Discard);
        assert_ne!(app.tab.id, old);
        assert!(app.tab.doc.atoms.is_empty());
        assert_eq!(
            app.tab.inspector_ui.expanded(Section::Molecule),
            Some(expanded)
        );
    }
}

#[tokio::test]
async fn end_bond_shortcut_keeps_styrene_whole_summary_and_capped_selection() {
    for explicit in [None, Some(false), Some(true)] {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        let response = app
            .engine
            .execute(Request::import_smiles("CCc1ccccc1"))
            .await
            .unwrap();
        app.tab.doc = response.document.unwrap();
        app.tab.analysis = response.analysis;
        if let Some(expanded) = explicit {
            choose(&mut app, expanded);
        }
        let first = app.tab.doc.atoms[0].id;
        let second = app.tab.doc.atoms[1].id;
        assert!(app.property_summary().contains("Whole drawing · C8H10"));
        let _ = app.context_key_at("2", reshiki::keyboard_drawing::Target::Bond(first, second));
        assert_eq!(app.tab.selected, [first, second]);
        assert!(app.property_analysis().is_none());
        assert!(
            !app.property_summary().contains("C8H10"),
            "Previous revision must disappear immediately"
        );
        assert_eq!(app.next_property_request(), Some(app.whole_property_key()));
        settle(&mut app, true).await;
        assert!(
            app.property_summary()
                .contains("Whole drawing · C8H8 · 104.152 g/mol")
        );
        assert_eq!(
            app.next_property_request(),
            Some(app.property_request_key())
        );
        settle(&mut app, false).await;
        assert!(
            app.property_summary()
                .contains("Selection · C2H4 · 28.054 g/mol")
        );
        assert_eq!(app.whole_property_analysis().unwrap().formula, "C8H8");
        assert_eq!(app.tab.inspector_ui.expanded(Section::Molecule), explicit);
        assert!(app.next_property_request().is_none());
        let before = app.tab.doc.clone();
        app.tab.doc.atom_mut(first).unwrap().element = "O".into();
        app.changed(before);
        assert!(!app.property_summary().contains("C8H8"));
        assert!(!app.property_summary().contains("C2H4"));
    }
}

#[tokio::test]
async fn whole_receipt_survives_selection_change_but_not_revision_or_file_epoch() {
    for stale in ["selection", "revision", "epoch"] {
        let (mut app, _) = App::new();
        app.tab.doc.add_atom("O", Default::default());
        let n = app
            .tab
            .doc
            .add_atom("N", reshiki::document::Point::new(80., 0.));
        let key = app.whole_property_key();
        let result = analyze(&app, &key).await;
        let _ = app.inspector_action(Action::RefreshPropertiesFor(app.whole_property_key()));
        match stale {
            "revision" => app.tab.revision += 1,
            "epoch" => app.tab.file_epoch += 1,
            _ => app.tab.selected = vec![n],
        }
        let _ = app.inspector_action(Action::PropertiesCalculated(key, Box::new(Ok(result))));
        assert!(app.tab.inspector_ui.pending.is_none());
        assert_eq!(
            app.whole_property_analysis().is_some(),
            stale == "selection"
        );
        assert!(
            app.property_analysis().is_none(),
            "A whole receipt cannot replace a fragment"
        );
    }
}

#[test]
fn whole_error_or_missing_analysis_settles_until_key_changes() {
    for error in [
        "Cannot analyze this drawing",
        "No molecular properties were returned.",
    ] {
        let (mut app, _) = App::new();
        let o = app.tab.doc.add_atom("O", Default::default());
        let _ = app.inspector_action(Action::RefreshPropertiesFor(app.whole_property_key()));
        let key = app.whole_property_key();
        let _ = app.inspector_action(Action::PropertiesCalculated(
            key,
            Box::new(Err(error.into())),
        ));
        assert!(app.next_property_request().is_none());
        app.tab.selected = vec![o];
        assert!(
            app.next_property_request().is_none(),
            "Selecting every atom reuses the same failure"
        );
        assert!(app.whole_property_analysis().is_none());
        app.tab.revision += 1;
        assert!(app.next_property_request().is_some());
    }
}

#[tokio::test]
async fn tagged_whole_job_finishes_in_its_background_tab_and_is_ready_on_return() {
    use iced::futures::StreamExt;
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc.add_atom("O", Default::default());
    let id = app.tab.id;
    let task = app.update(Message::InspectorAction(Action::RefreshPropertiesFor(
        app.whole_property_key(),
    )));
    let mut outputs = iced_runtime::task::into_stream(task).unwrap();
    app.add_tab();
    let front = app.tab.id;
    let front_doc = app.tab.doc.clone();
    while let Some(action) = outputs.next().await {
        if let iced_runtime::Action::Output(message) = action {
            assert!(matches!(&message, Message::Tab(owner, _) if *owner == id));
            let _ = app.update(message);
        }
    }
    assert_eq!(app.tab.id, front);
    assert_eq!(app.tab.doc, front_doc);
    assert!(app.tabs.background[0].inspector_ui.pending.is_none());
    select(&mut app, id);
    assert!(
        app.property_summary()
            .contains("Whole drawing · H2O · 18.015 g/mol")
    );
    assert!(app.next_property_request().is_none());
}

#[tokio::test]
async fn automatic_queued_request_rechecks_cache_selection_and_revision_before_starting() {
    let (mut app, _) = App::new();
    let o = app.tab.doc.add_atom("O", Default::default());
    app.tab
        .doc
        .add_atom("N", reshiki::document::Point::new(80., 0.));
    let whole = app.whole_property_key();
    settle(&mut app, true).await;
    assert_eq!(
        app.inspector_action(Action::RefreshPropertiesFor(whole))
            .units(),
        0,
        "A queued timer cannot repeat settled work"
    );
    app.tab.selected = vec![o];
    let selected = app.property_request_key();
    app.tab.selected.clear();
    assert_eq!(
        app.inspector_action(Action::RefreshPropertiesFor(selected))
            .units(),
        0,
        "A queued timer cannot start an obsolete fragment"
    );
    let old = app.whole_property_key();
    app.tab.revision += 1;
    assert_eq!(
        app.inspector_action(Action::RefreshPropertiesFor(old))
            .units(),
        0,
        "A queued timer cannot start an obsolete revision"
    );
    assert!(app.tab.inspector_ui.pending.is_none());
}
