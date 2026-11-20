use super::*;
use iced::futures::StreamExt;

async fn ethanol() -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.inspector_open = true;
    app.inspector_tab = InspectorTab::Properties;
    app.engine = reshiki::engine::LocalEngine::in_process();
    app.tab.doc = Document::from_json(include_bytes!(
        "../../../../tests/fixtures/chemical-naming-rust/ethanol-rust-native.rsk"
    ))
    .unwrap();
    app.tab.analysis = app
        .engine
        .execute(Request::molecule("analyze", app.tab.doc.clone()))
        .await
        .unwrap()
        .analysis;
    app
}

fn generated(app: &mut App) {
    let mut record = super::super::super::naming::test_support::record("CCO");
    record.synonyms = reshiki::naming::aliases_for_smiles("CCO").unwrap();
    super::super::super::naming::test_support::generated(app, record);
}

#[tokio::test]
async fn identity_copy_uses_displayed_analysis_and_full_identifiers_without_editing() {
    let mut app = ethanol().await;
    let drawing = app.tab.doc.clone();
    let selection = app.tab.selected.clone();
    let expected = [
        (IdentityField::Formula, "C₂H₆O".to_owned()),
        (IdentityField::Weight, "46.069 g/mol".to_owned()),
        (
            IdentityField::Smiles,
            app.tab.analysis.as_ref().unwrap().smiles.clone(),
        ),
        (
            IdentityField::Inchi,
            app.tab.analysis.as_ref().unwrap().inchi.clone(),
        ),
        (
            IdentityField::InchiKey,
            app.tab.analysis.as_ref().unwrap().inchikey.clone(),
        ),
        (
            IdentityField::Analysis,
            "Anal. Calcd for C2H6O: C, 52.14; H, 13.13.".to_owned(),
        ),
    ];
    for (field, expected) in expected {
        assert!(!expected.is_empty());
        let expected_write = expected.clone();
        let mut effects = iced_runtime::task::into_stream(app.copy_identity_with(
            field,
            move |contents| async move {
                assert_eq!(contents, expected_write);
                Ok(())
            },
        ))
        .unwrap();
        assert_eq!(app.status, format!("Copying {}…", field.label()));
        let Some(iced_runtime::Action::Output(message)) = effects.next().await else {
            panic!("Missing copy acknowledgement");
        };
        let _ = app.update(message);
        assert!(effects.next().await.is_none());
        assert_eq!(app.status, format!("{} copied", field.label()));
        assert_eq!(app.tab.doc, drawing);
        assert_eq!(app.tab.selected, selection);
        assert!(!app.tab.history.can_undo());
    }
    app.appearance.two_decimal_weight = true;
    assert_eq!(
        app.identity_text(IdentityField::Weight).as_deref(),
        Some("46.07 g/mol")
    );
    app.copy_as_busy = true;
    assert_eq!(app.copy_identity(IdentityField::InchiKey).units(), 0);
    assert_eq!(app.status, "A clipboard operation is already in progress");
}

#[tokio::test]
async fn naming_is_on_demand_and_record_is_limited_to_its_current_complete_scope() {
    let mut app = ethanol().await;
    let before = app.tab.doc.clone();
    for _ in 0..5 {
        let _ = app.molecular_section(Some(Section::Molecule));
        assert!(app.identity_target().is_some());
    }
    assert_eq!(app.tab.inspector_ui.identity_scope.borrow().calculations, 1);
    assert!(app.identity_naming_idle());
    assert!(app.identity_names().is_none());
    assert!(app.identity_caption_action().is_none());
    assert!(app.inspector_action(Action::NameIdentity).units() > 0);
    assert!(!app.identity_naming_idle());
    generated(&mut app);
    let names = app.identity_names().unwrap();
    assert_eq!(names.primary, "ethanol");
    assert_eq!(names.systematic.as_deref(), Some("ethan-1-ol"));
    assert!(app.identity_caption_action().is_some());
    assert_eq!(
        app.tab.doc, before,
        "Naming alone does not place text or create Undo"
    );
    assert!(!app.tab.history.can_undo());
    app.tab.revision += 1;
    assert!(
        app.identity_names().is_some(),
        "An unchanged chemical identity survives layout/caption revisions"
    );
    let before = app.tab.doc.clone();
    app.tab
        .doc
        .atom_mut(app.tab.doc.atoms[0].id)
        .unwrap()
        .element = "N".into();
    app.changed(before);
    assert!(app.identity_names().is_none());
    assert!(app.identity_caption_action().is_none());
    assert_eq!(app.copy_identity(IdentityField::Name).units(), 0);
}

#[tokio::test]
async fn selected_fragment_keeps_its_analysis_but_cannot_name_or_caption_the_whole_molecule() {
    let mut app = ethanol().await;
    generated(&mut app);
    app.tab.selected = vec![app.tab.doc.atoms[0].id];
    let key = app.property_request_key();
    let result = app
        .engine
        .execute(Request::molecule("analyze", app.property_document(&key)))
        .await
        .unwrap()
        .analysis
        .unwrap();
    let formula = app.shown_formula(&result.display_formula);
    let inchi = result.inchi.clone();
    app.tab.inspector_ui.properties = Some((key, Ok(result)));
    assert_ne!(formula, "C₂H₆O");
    assert_eq!(app.identity_text(IdentityField::Formula), Some(formula));
    assert_eq!(app.identity_text(IdentityField::Inchi), Some(inchi));
    assert!(app.identity_target().is_none());
    assert!(app.identity_names().is_none());
    assert_eq!(app.name_identity().units(), 0);
    assert_eq!(app.add_identity_name().units(), 0);
    assert!(app.identity_naming_idle());
    assert!(app.tab.doc.annotations.is_empty());
    assert!(!app.tab.history.can_undo());
}

#[tokio::test]
async fn common_name_caption_uses_the_existing_link_and_one_undo() {
    let mut app = ethanol().await;
    generated(&mut app);
    let before = app.tab.doc.clone();
    let _ = app.inspector_action(Action::AddIdentityName);
    assert!(
        app.tab
            .doc
            .annotations
            .iter()
            .any(|a| a.text.contains("ethanol"))
    );
    assert_eq!(app.tab.doc.property_captions.len(), 1);
    assert_eq!(
        app.identity_text(IdentityField::Name).as_deref(),
        Some("ethanol")
    );
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
}

#[tokio::test]
async fn custom_name_and_property_pins_keep_the_existing_caption_model() {
    let mut app = ethanol().await;
    let atoms = app.identity_target().unwrap();
    app.rename_molecule(&atoms, "Sample α");
    assert_eq!(
        app.identity_text(IdentityField::Name).as_deref(),
        Some("Sample α")
    );
    assert!(reshiki::linked_properties::shown(
        &app.tab.doc,
        &atoms,
        Property::Name
    ));
    assert!(
        app.identity_caption_action().is_none(),
        "Renaming already shows the linked name"
    );
    app.toggle_property(&atoms, Property::Name);
    assert!(matches!(
        app.identity_caption_action(),
        Some(Message::ToggleProperty(_, Property::Name))
    ));
    let _ = app.add_identity_name();
    assert!(reshiki::linked_properties::shown(
        &app.tab.doc,
        &atoms,
        Property::Name
    ));
    assert!(
        app.identity_caption_action().is_none(),
        "Add never hides a shown name"
    );
    for property in [
        Property::Formula,
        Property::Weight,
        Property::ExactMass,
        Property::LogP,
        Property::Tpsa,
        Property::Analysis,
    ] {
        app.toggle_property(&atoms, property);
        assert!(reshiki::linked_properties::shown(
            &app.tab.doc,
            &atoms,
            property
        ));
    }
    let reopened = Document::from_json(&app.tab.doc.file_json().unwrap()).unwrap();
    let caption = reshiki::linked_properties::caption(&reopened, &atoms).unwrap();
    assert_eq!(caption.custom_name, "Sample α");
    assert_eq!(caption.properties.len(), Property::ALL.len());
}

#[tokio::test]
async fn attachment_scope_does_not_offer_cached_ordinary_identifiers_or_names() {
    let mut app = ethanol().await;
    generated(&mut app);
    app.tab
        .doc
        .atom_mut(app.tab.doc.atoms[0].id)
        .unwrap()
        .attachment = Some(reshiki::attachments::Kind::Variable);
    app.tab.revision += 1;
    assert!(app.special_molecular_scope());
    assert!(app.identity_target().is_none());
    assert!(app.identity_text(IdentityField::Inchi).is_none());
    assert!(app.identity_text(IdentityField::Name).is_none());
    assert_eq!(app.name_identity().units(), 0);
    assert_eq!(app.add_identity_name().units(), 0);
    let _ = app.molecular_section(Some(Section::Molecule));
}

#[test]
fn identifier_elision_keeps_full_unicode_graphemes() {
    assert_eq!(one_line("short", 1000., 12.), "short");
    let result = one_line(
        "A🇯🇵BCDEFGH",
        super::super::super::workspace::text_width("A🇯🇵…", 12.),
        12.,
    );
    assert!(result == "A🇯🇵…" || result == "A…");
    assert!(!result.ends_with("🇯…"));
}

#[tokio::test]
#[ignore = "Opt-in headless renderer accessibility check"]
async fn identity_controls_are_accessible_and_descriptors_stay_collapsed() -> Result<(), String> {
    use iced::advanced::{renderer::Headless, widget::operation};
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .ok_or("No renderer")?;
    let mut app = ethanol().await;
    let size = iced::Size::new(360., 1800.);
    let snapshot = |app: &App, renderer: &mut iced::Renderer| {
        let mut ui = iced_runtime::UserInterface::build(
            app.molecular_section(Some(Section::Molecule)),
            size,
            iced_runtime::user_interface::Cache::new(),
            renderer,
        );
        let mut collect = reshiki::accessibility::Collect::new(iced::Rectangle::with_size(size));
        ui.operate(renderer, &mut operation::black_box(&mut collect));
        collect.snapshot().clone()
    };
    let initial = snapshot(&app, &mut renderer);
    assert!(initial.duplicate_ids.is_empty());
    for field in [
        IdentityField::Formula,
        IdentityField::Weight,
        IdentityField::Smiles,
        IdentityField::Inchi,
        IdentityField::InchiKey,
    ] {
        let node = initial
            .nodes
            .iter()
            .find(|node| node.id == field.id())
            .unwrap();
        assert!(node.enabled);
        assert_eq!(node.value, app.identity_text(field));
    }
    assert!(
        initial
            .nodes
            .iter()
            .any(|node| node.id == "identity-name-it" && node.enabled)
    );
    assert!(
        initial
            .nodes
            .iter()
            .any(|node| node.id == "identity-add-name" && !node.enabled)
    );
    assert!(
        initial
            .nodes
            .iter()
            .any(|node| node.id == "inspector-section-Descriptors" && node.expanded == Some(false))
    );
    assert!(!initial.nodes.iter().any(|node| node.id == "molecule-name"));
    let _ = app.inspector_action(Action::Section(Section::Descriptors, true));
    let _ = app.inspector_action(Action::Section(Section::NameOptions, true));
    let expanded = snapshot(&app, &mut renderer);
    assert!(expanded.duplicate_ids.is_empty());
    assert!(expanded.nodes.iter().any(|node| node.id == "molecule-name"));
    for property in Property::ALL {
        assert!(
            expanded
                .nodes
                .iter()
                .any(|node| node.id == format!("property-pin-{property:?}"))
        );
    }
    Ok(())
}

#[tokio::test]
async fn identity_copy_waits_for_acknowledgement_and_reports_denial_without_success() {
    use iced::futures::FutureExt;
    let mut app = ethanol().await;
    let expected = app.identity_text(IdentityField::InchiKey).unwrap();
    let observed = std::sync::Arc::new(std::sync::Mutex::new(None));
    let captured = observed.clone();
    let (send, receive) = tokio::sync::oneshot::channel();
    let task = app.copy_identity_with(IdentityField::InchiKey, move |text| async move {
        *captured.lock().unwrap() = Some(text);
        receive.await.unwrap()
    });
    let mut effects = iced_runtime::task::into_stream(task).unwrap();
    assert!(effects.next().now_or_never().is_none());
    assert!(app.copy_as_busy);
    assert_eq!(app.status, "Copying InChIKey…");
    send.send(Err("Clipboard access denied".into())).unwrap();
    let Some(iced_runtime::Action::Output(message)) = effects.next().await else {
        panic!("Missing failure acknowledgement");
    };
    assert_eq!(observed.lock().unwrap().as_deref(), Some(expected.as_str()));
    let _ = app.update(message);
    assert!(!app.copy_as_busy);
    assert!(app.error);
    assert_eq!(
        app.status,
        "Could not copy InChIKey: Clipboard access denied"
    );
}

#[tokio::test]
async fn identity_copy_completion_is_scoped_and_releases_busy_state_after_tab_close() {
    for closed in [false, true] {
        let mut app = ethanol().await;
        let id = app.tab.id;
        let mut effects = iced_runtime::task::into_stream(
            app.copy_identity_with(IdentityField::Formula, |_| async { Ok(()) }),
        )
        .unwrap();
        let Some(iced_runtime::Action::Output(message)) = effects.next().await else {
            panic!("Missing receipt");
        };
        if closed {
            let _ = app.close_active_tab();
        } else {
            app.add_tab();
        }
        let doc = app.tab.doc.clone();
        let status = app.status.clone();
        let _ = app.update(Message::Tab(id, Box::new(message)));
        assert!(!app.copy_as_busy);
        assert_eq!(app.status, status);
        assert_eq!(app.tab.doc, doc);
    }
}

#[tokio::test]
async fn custom_name_warning_tracks_a_real_chemical_change() {
    let mut app = ethanol().await;
    let atoms = app.identity_target().unwrap();
    app.rename_molecule(&atoms, "Original sample");
    assert!(!app.identity_name_stale());
    let before = app.tab.doc.clone();
    app.tab.doc.atom_mut(atoms[0]).unwrap().element = "N".into();
    app.changed(before);
    app.tab.analysis = app
        .engine
        .execute(Request::molecule("analyze", app.tab.doc.clone()))
        .await
        .unwrap()
        .analysis;
    assert!(app.identity_name_stale());
    assert_eq!(
        app.identity_text(IdentityField::Name).as_deref(),
        Some("Original sample")
    );
    assert_eq!(app.tab.inspector_ui.expanded(Section::NameOptions), None);
    assert_eq!(app.tab.inspector_ui.expanded(Section::Descriptors), None);
    let _ = app.identity_content(app.property_analysis().unwrap());
}

#[tokio::test]
#[ignore = "Requires this source's built RESHIKI_NAMING_HELPER"]
async fn deselected_identity_names_and_captions_through_the_real_local_worker() {
    let mut app = ethanol().await;
    assert!(app.tab.selected.is_empty());
    let before = app.tab.doc.clone();
    let mut effects =
        iced_runtime::task::into_stream(app.inspector_action(Action::NameIdentity)).unwrap();
    while let Some(effect) = effects.next().await {
        if let iced_runtime::Action::Output(message) = effect {
            let _ = app.update(message);
        }
    }
    assert!(app.identity_naming_idle());
    assert!(app.identity_names().is_some(), "{}", app.status);
    assert!(app.tab.selected.is_empty());
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    let _ = app.inspector_action(Action::AddIdentityName);
    assert_eq!(app.tab.doc.property_captions.len(), 1);
    assert!(app.identity_names().is_some());
    assert!(app.tab.selected.is_empty());
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}

#[tokio::test]
async fn adding_then_customizing_a_name_keeps_one_caption_and_one_undo_step() {
    let mut app = ethanol().await;
    generated(&mut app);
    let _ = app.add_identity_name();
    let atoms = app.identity_target().unwrap();
    let before = app.tab.doc.clone();
    assert_eq!(before.annotations.len(), 1);
    app.rename_molecule(&atoms, "Custom sample");
    assert_eq!(app.tab.doc.annotations.len(), 1);
    assert_eq!(
        app.identity_text(IdentityField::Name).as_deref(),
        Some("Custom sample")
    );
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    assert_eq!(
        app.identity_text(IdentityField::Name).as_deref(),
        Some("ethanol")
    );
}

#[tokio::test]
async fn legacy_name_stays_visible_and_custom_edit_replaces_it_without_a_second_caption() {
    let mut app = ethanol().await;
    let atoms = app.identity_target().unwrap();
    let canonical = reshiki::naming::selected_identity(&app.tab.doc, &atoms)
        .unwrap()
        .smiles;
    app.place_molecule_caption(&atoms, "ethanol".into(), canonical);
    let before = app.tab.doc.clone();
    assert_eq!(
        app.identity_text(IdentityField::Name).as_deref(),
        Some("ethanol")
    );
    assert!(app.identity_caption_action().is_none());
    app.rename_molecule(&atoms, "Legacy sample");
    assert_eq!(app.tab.doc.annotations.len(), 1);
    assert!(app.tab.doc.molecule_names.is_empty());
    assert_eq!(app.tab.doc.property_captions.len(), 1);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}

#[tokio::test]
async fn balanced_salt_keeps_the_existing_standalone_name_caption_path() {
    let mut app = ethanol().await;
    app.tab.doc = Document::default();
    let sodium = app.tab.doc.add_atom("Na", Default::default());
    let chloride = app
        .tab
        .doc
        .add_atom("Cl", reshiki::document::Point::new(80., 0.));
    app.tab.doc.atom_mut(sodium).unwrap().charge = 1;
    app.tab.doc.atom_mut(chloride).unwrap().charge = -1;
    app.tab.revision += 1;
    let mut record = super::super::super::naming::test_support::record("[Na+].[Cl-]");
    record.systematic_name = Some("sodium chloride".into());
    super::super::super::naming::test_support::generated(&mut app, record);
    assert!(app.property_target(&[]).is_none());
    assert!(app.identity_target().is_some());
    let _ = app.add_identity_name();
    assert_eq!(app.tab.doc.molecule_names.len(), 1);
    assert_eq!(
        app.identity_text(IdentityField::Name).as_deref(),
        Some("sodium chloride")
    );
}
