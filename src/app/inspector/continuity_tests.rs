//! Revision-keyed whole/selection receipts and explicit Molecular header preferences.
use super::*;
use crate::app::{atom_text, document_tab::TabId};

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
        assert!(app.property_summary().contains("Whole drawing · C₈H₁₀"));
        let _ = app.context_key_at("2", reshiki::keyboard_drawing::Target::Bond(first, second));
        assert_eq!(app.tab.selected, [first, second]);
        assert!(app.property_analysis().is_none());
        assert!(
            !app.property_summary().contains("C₈H₁₀"),
            "Previous revision must disappear immediately"
        );
        assert_eq!(app.next_property_request(), Some(app.whole_property_key()));
        settle(&mut app, true).await;
        assert!(
            app.property_summary()
                .contains("Whole drawing · C₈H₈ · 104.152\u{a0}g/\u{2060}mol")
        );
        assert_eq!(
            app.next_property_request(),
            Some(app.property_request_key())
        );
        settle(&mut app, false).await;
        assert!(
            app.property_summary()
                .contains("Selection · C₂H₄ · 28.054\u{a0}g/\u{2060}mol")
        );
        assert_eq!(app.whole_property_analysis().unwrap().formula, "C8H8");
        assert_eq!(app.tab.inspector_ui.expanded(Section::Molecule), explicit);
        assert!(app.next_property_request().is_none());
        let before = app.tab.doc.clone();
        app.tab.doc.atom_mut(first).unwrap().element = "O".into();
        app.changed(before);
        assert!(!app.property_summary().contains("C₈H₈"));
        assert!(!app.property_summary().contains("C₂H₄"));
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
            .contains("Whole drawing · H₂O · 18.015\u{a0}g/\u{2060}mol")
    );
    assert!(app.next_property_request().is_none());
}

#[tokio::test]
async fn atom_editor_admits_automatic_property_refresh_and_completion() {
    use iced::futures::StreamExt;
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let o = app.tab.doc.add_atom("O", Default::default());
    app.tab.selected = vec![o];
    let _ = app.update(Message::AtomText(atom_text::Action::Begin(Some(o))));
    assert!(app.tab.atom_text.is_some());
    let _ = app.update(Message::InspectorAction(Action::RefreshProperties));
    assert!(app.tab.inspector_ui.pending.is_none());
    let key = app.property_request_key();
    let task = app.update(Message::InspectorAction(Action::RefreshPropertiesFor(
        key.clone(),
    )));
    assert_eq!(app.tab.inspector_ui.pending.as_ref(), Some(&key));
    let mut outputs = iced_runtime::task::into_stream(task).expect("The property worker starts");
    while let Some(action) = outputs.next().await {
        if let iced_runtime::Action::Output(message) = action {
            let _ = app.update(message);
        }
    }
    assert!(app.tab.atom_text.is_some());
    assert!(app.tab.inspector_ui.pending.is_none());
    assert_eq!(
        app.property_summary(),
        "Whole drawing · H₂O · 18.015\u{a0}g/\u{2060}mol\nSelection · H₂O · 18.015\u{a0}g/\u{2060}mol"
    );
}

#[tokio::test]
async fn atom_editor_settles_a_property_request_started_before_editing() {
    use iced::futures::StreamExt;
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let o = app.tab.doc.add_atom("O", Default::default());
    app.tab.selected = vec![o];
    let key = app.property_request_key();
    let task = app.update(Message::InspectorAction(Action::RefreshPropertiesFor(
        key.clone(),
    )));
    assert_eq!(app.tab.inspector_ui.pending.as_ref(), Some(&key));
    let _ = app.update(Message::AtomText(atom_text::Action::Begin(Some(o))));
    assert!(app.tab.atom_text.is_some());
    let mut outputs = iced_runtime::task::into_stream(task).expect("The property worker starts");
    while let Some(action) = outputs.next().await {
        if let iced_runtime::Action::Output(message) = action {
            let _ = app.update(message);
        }
    }
    assert!(app.tab.atom_text.is_some());
    assert!(app.tab.inspector_ui.pending.is_none());
    assert!(app.property_analysis().is_some());
}

#[tokio::test]
async fn view_edits_preserve_cached_and_in_flight_properties_until_chemistry_changes() {
    use crate::canvas::Edit;
    use iced::futures::StreamExt;
    use reshiki::{
        document::Point,
        models3d::{Model3d, Placed},
    };
    for pending in [false, true] {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        let o = app.tab.doc.add_atom("O", Point::default());
        app.tab.doc.add_atom("N", Point::new(100., 0.));
        let model = Model3d::new(
            &app.tab.doc,
            vec![Placed {
                id: o,
                xyz: [0.; 3],
            }],
            vec![],
            Default::default(),
        );
        app.tab.doc.models.push(model);
        reshiki::models3d::require_version(&mut app.tab.doc);
        app.tab.selected = vec![o];
        settle(&mut app, true).await;
        settle(&mut app, false).await;
        let summary = app.property_summary();
        assert!(summary.contains("Selection · H₂O · 18.015\u{a0}g/\u{2060}mol"));
        let task = pending.then(|| app.update(Message::InspectorAction(Action::RefreshProperties)));
        let before = app.tab.doc.clone();
        let revision = app.tab.revision;
        for rotation in [[0., 1., 0., 0.], [0., 0., 1., 0.]] {
            let _ = app.update(Message::Canvas(Edit::ModelView {
                atoms: vec![o],
                rotation,
            }));
        }
        assert_eq!(app.tab.revision, revision + 2);
        assert!(!reshiki::transaction::chemistry_changed(
            &before,
            &app.tab.doc
        ));
        assert!(app.property_result(&app.whole_property_key()).is_some());
        if let Some(task) = task {
            let mut outputs = iced_runtime::task::into_stream(task).unwrap();
            while let Some(action) = outputs.next().await {
                if let iced_runtime::Action::Output(message) = action {
                    let _ = app.update(message);
                }
            }
        }
        assert!(app.tab.inspector_ui.pending.is_none());
        assert_eq!(app.property_summary(), summary);
        assert!(app.next_property_request().is_none());
        let _ = app.update(Message::Undo);
        assert_eq!(app.property_summary(), summary);
        assert!(app.next_property_request().is_none());

        // An in-flight receipt must stay stale across a chemistry edit followed
        // by a view edit, just like results already in the cache.
        let task = app.update(Message::InspectorAction(Action::RefreshProperties));
        app.tool = Tool::Atom;
        app.element = "C".into();
        let before = app.tab.doc.clone();
        let _ = app.update(Message::Canvas(Edit::Click(Point::default())));
        assert!(reshiki::transaction::chemistry_changed(
            &before,
            &app.tab.doc
        ));
        let revision = app.tab.revision;
        let _ = app.update(Message::Canvas(Edit::ModelView {
            atoms: vec![o],
            rotation: [1., 0., 0., 0.],
        }));
        assert_eq!(app.tab.revision, revision + 1);
        let mut outputs = iced_runtime::task::into_stream(task).unwrap();
        while let Some(action) = outputs.next().await {
            if let iced_runtime::Action::Output(message) = action {
                let _ = app.update(message);
            }
        }
        assert!(app.tab.inspector_ui.pending.is_none());
        assert!(app.whole_property_analysis().is_none());
        assert!(app.property_analysis().is_none());
        assert!(!app.property_summary().contains("H₂O · 18.015"));
    }
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

#[derive(Default)]
struct PanelText(Vec<String>);
impl iced::advanced::widget::Operation for PanelText {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn iced::advanced::widget::Operation)) {
        operate(self);
    }
    fn text(&mut self, _: Option<&iced::advanced::widget::Id>, _: iced::Rectangle, value: &str) {
        self.0.push(value.into());
    }
}

#[tokio::test]
#[ignore = "Opt-in real renderer Molecular Properties text regression"]
async fn rendered_panel_keeps_whole_mass_and_uses_singular_counts() {
    use iced::advanced::{renderer::Headless, widget::operation};
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .unwrap();
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let response = app
        .engine
        .execute(Request::import_smiles("CCc1ccccc1"))
        .await
        .unwrap();
    app.tab.doc = response.document.unwrap();
    app.tab.analysis = response.analysis;
    let first = app.tab.doc.atoms[0].id;
    let second = app.tab.doc.atoms[1].id;
    let _ = app.context_key_at("2", reshiki::keyboard_drawing::Target::Bond(first, second));
    settle(&mut app, true).await;
    settle(&mut app, false).await;
    let panel_text = |app: &App, renderer: &mut iced::Renderer| {
        let mut ui = iced_runtime::UserInterface::build(
            app.molecular_section(),
            iced::Size::new(300., 800.),
            iced_runtime::user_interface::Cache::new(),
            renderer,
        );
        let mut text = PanelText::default();
        ui.operate(renderer, &mut operation::black_box(&mut text));
        text.0.join("\n")
    };
    for expanded in [None, Some(true), Some(false)] {
        app.tab.inspector_ui.expanded.clear();
        if let Some(expanded) = expanded {
            choose(&mut app, expanded);
        }
        let text = panel_text(&app, &mut renderer);
        assert!(text.contains("Whole drawing · C₈H₈ · 104.152\u{a0}g/\u{2060}mol"));
        assert!(text.contains("Selection · C₂H₄ · 28.054\u{a0}g/\u{2060}mol"));
    }
    app.tab.selected = vec![first];
    choose(&mut app, true);
    settle(&mut app, false).await;
    let text = panel_text(&app, &mut renderer);
    assert!(text.contains("1 atom · 0 bonds"));
    assert!(!text.contains("1 atoms"));
}

#[tokio::test]
async fn selected_receipt_cannot_fill_whole_or_another_revision_selection_or_file() {
    for stale in ["selection", "revision", "epoch"] {
        let (mut app, _) = App::new();
        let o = app.tab.doc.add_atom("O", Default::default());
        let n = app
            .tab
            .doc
            .add_atom("N", reshiki::document::Point::new(80., 0.));
        app.tab.selected = vec![o];
        let key = app.property_request_key();
        let result = analyze(&app, &key).await;
        let _ = app.inspector_action(Action::RefreshProperties);
        match stale {
            "revision" => app.tab.revision += 1,
            "epoch" => app.tab.file_epoch += 1,
            _ => app.tab.selected = vec![n],
        }
        let _ = app.inspector_action(Action::PropertiesCalculated(key, Box::new(Ok(result))));
        assert!(app.tab.inspector_ui.pending.is_none());
        assert!(app.whole_property_analysis().is_none());
        assert!(app.property_analysis().is_none());
    }
}
