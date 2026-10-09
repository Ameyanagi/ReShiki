use super::*;
use reshiki::naming::Provenance;

fn record(smiles: &str) -> Record {
    Record {
        title: "Ethanol".into(),
        systematic_name: Some("ethanol".into()),
        canonical_smiles: naming::canonical_smiles(smiles).unwrap(),
        smiles: smiles.into(),
        synonyms: vec!["ethyl alcohol".into()],
        warnings: vec![],
        provenance: Provenance::PubChem(702),
    }
}

#[tokio::test]
async fn preview_insertion_is_native_editable_and_one_history_step() -> Result<(), String> {
    let (mut app, _) = App::new();
    let preview = prepare_preview(app.engine.clone(), record("CCO"), "CCO".into()).await?;
    app.tab.naming.set_preview(preview);
    let preview = app.tab.naming.preview.as_ref().unwrap();
    let atom = preview.document.atoms[0].id;
    let point = preview.document.atoms[0].position;
    let _ = app.naming_action(Action::PreviewEdit(Edit::Move(vec![atom], 3., 0.)));
    assert_eq!(
        app.tab
            .naming
            .preview
            .as_ref()
            .unwrap()
            .document
            .atom(atom)
            .unwrap()
            .position,
        point.offset(3., 0.)
    );
    let original = app.tab.doc.clone();
    let _ = app.naming_action(Action::Insert);
    assert_eq!(app.tab.doc.atoms.len(), 3);
    assert_eq!(app.tab.doc.bonds.len(), 2);
    assert_eq!(naming::document_identity(&app.tab.doc)?.smiles, "CCO");
    let inserted = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, inserted);
    let json = serde_json::to_string(&app.tab.doc).map_err(|e| e.to_string())?;
    let restored: Document = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    assert_eq!(naming::document_identity(&restored)?.smiles, "CCO");
    Ok(())
}

#[tokio::test]
async fn native_preview_and_exchange_retain_specified_and_unspecified_stereo() -> Result<(), String>
{
    let engine = LocalEngine::default();
    for smiles in [
        "C[C@@H](O)C(=O)O",
        "C[C@H](O)C(=O)O",
        "CC(O)C(=O)O",
        "C/C=C/C",
        "C/C=C\\C",
        "[13CH3]CO",
        "CC(=O)[O-]",
    ] {
        let preview = prepare_preview(engine.clone(), record(smiles), smiles.into()).await?;
        naming::verify_identity(
            smiles,
            &naming::document_identity(&preview.document)?.smiles,
        )?;
        let mut request = Request::molecule("export", preview.document);
        request.format = Some("mol".into());
        let mol = engine
            .request(request)
            .await?
            .output
            .ok_or("Missing MOL export")?;
        let restored = engine
            .request(Request::import("mol", &mol))
            .await?
            .document
            .ok_or("Missing restored drawing")?;
        naming::verify_identity(smiles, &naming::document_identity(&restored)?.smiles)?;
    }
    Ok(())
}

#[tokio::test]
async fn warnings_and_unapplied_smiles_edits_block_insertion() -> Result<(), String> {
    let (mut app, _) = App::new();
    let mut source = record("CCO");
    source.warnings.push("Ambiguous interpretation".into());
    let preview = prepare_preview(app.engine.clone(), source, "CCO".into()).await?;
    app.tab.naming.set_preview(preview);
    let _ = app.naming_action(Action::Insert);
    assert!(app.tab.doc.atoms.is_empty());
    let _ = app.naming_action(Action::Acknowledge(true));
    let _ = app.naming_action(Action::Smiles("CCN".into()));
    let _ = app.naming_action(Action::Insert);
    assert!(app.tab.doc.atoms.is_empty());
    let _ = app.naming_action(Action::Smiles("CCO".into()));
    let _ = app.naming_action(Action::Insert);
    assert_eq!(app.tab.doc.atoms.len(), 3);
    Ok(())
}

#[tokio::test]
async fn changed_preview_no_longer_claims_original_chemical_name() -> Result<(), String> {
    let engine = LocalEngine::default();
    let original = prepare_preview(engine.clone(), record("CCO"), "CCO".into()).await?;
    assert!(!original.modified);
    let edited = prepare_preview(engine, record("CCO"), "CCN".into()).await?;
    assert!(edited.modified);
    assert_eq!(edited.identity, "CCN");
    Ok(())
}

#[test]
fn stale_results_and_name_consent_cannot_reach_another_document_or_request() {
    let (mut app, _) = App::new();
    app.tab.naming.consent = true;
    let before = app.tab.naming.serial;
    let _ = app.naming_action(Action::Lookup);
    assert_eq!(
        app.tab.naming.serial, before,
        "Name consent does not authorize sending a structure"
    );
    let ticket = app.naming_ticket();
    let _ = app.naming_action(Action::Name("aspirin".into()));
    let _ = app.naming_action(Action::Finished(
        ticket,
        Box::new(Ok(Outcome::Names(vec![record("CCO")], None))),
    ));
    assert!(app.tab.naming.candidates.is_empty());
    let ticket = app.naming_ticket();
    app.tab.revision += 1;
    let _ = app.naming_action(Action::Finished(
        ticket,
        Box::new(Ok(Outcome::Structure(record("CCO")))),
    ));
    assert!(app.tab.naming.structure.is_none());
}

#[tokio::test]
async fn caption_requires_current_graph_and_is_undoable() -> Result<(), String> {
    let (mut app, _) = App::new();
    let preview = prepare_preview(app.engine.clone(), record("CCO"), "CCO".into()).await?;
    app.tab.doc = preview.document;
    app.tab.selected = app.tab.doc.all_ids();
    let ticket = Ticket {
        epoch: app.tab.file_epoch,
        revision: app.tab.revision,
        serial: 0,
    };
    app.tab.naming.structure = Some((ticket, record("CCO")));
    let _ = app.naming_action(Action::Caption);
    assert_eq!(app.tab.doc.annotations[0].text, "ethanol");
    let _ = app.update(Message::Undo);
    assert!(app.tab.doc.annotations.is_empty());
    app.tab.selected.clear();
    let _ = app.naming_action(Action::Caption);
    assert!(app.tab.doc.annotations.is_empty());
    Ok(())
}

#[tokio::test]
#[ignore = "Opt-in native renderer accessibility metadata and dispatch"]
async fn controls_publish_named_native_actions_and_editable_values() -> Result<(), String> {
    use iced::advanced::renderer::Headless;
    use reshiki::accessibility::{Activate, Collect, Role};
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.naming.name = "ethanol".into();
    app.tab.naming.consent = true;
    let mut source = record("CCO");
    source
        .warnings
        .push("Check this source interpretation".into());
    let preview = prepare_preview(app.engine.clone(), source.clone(), "CCO".into()).await?;
    app.tab.naming.set_preview(preview);
    app.tab.naming.structure = Some((
        Ticket {
            epoch: app.tab.file_epoch,
            revision: app.tab.revision,
            serial: 0,
        },
        source,
    ));
    let size = iced::Size::new(340., 2400.);
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .ok_or("No renderer")?;
    let mut ui = iced_runtime::UserInterface::build(
        app.naming_panel(),
        size,
        Default::default(),
        &mut renderer,
    );
    let mut collect = Collect::new(iced::Rectangle::with_size(size));
    ui.operate(
        &renderer,
        &mut iced::advanced::widget::operation::black_box(&mut collect),
    );
    let snapshot = collect.snapshot();
    assert!(snapshot.duplicate_ids.is_empty());
    for id in [
        "naming.name",
        "naming.smiles",
        "naming.source.opsin",
        "naming.source.pubchem",
        "naming.consent.name",
        "naming.consent.structure",
        "naming.resolve",
        "naming.lookup",
        "naming.acknowledge",
        "naming.insert",
        "naming.copy-name",
        "naming.caption",
        "naming.source-record",
    ] {
        let node = snapshot
            .nodes
            .iter()
            .find(|node| node.id == id)
            .unwrap_or_else(|| panic!("Missing {id}"));
        assert!(!node.name.is_empty());
        assert!(node.visible_bounds.is_some(), "Invisible {id}");
    }
    let consent = snapshot
        .nodes
        .iter()
        .find(|node| node.id == "naming.consent.name")
        .unwrap();
    assert_eq!(consent.role, Role::ToggleButton);
    assert_eq!(consent.checked, Some(true));
    let input = snapshot
        .nodes
        .iter()
        .find(|node| node.id == "naming.name")
        .unwrap();
    assert_eq!(input.value.as_deref(), Some("ethanol"));
    let mut activate = Activate::<Message>::new("naming.resolve");
    ui.operate(
        &renderer,
        &mut iced::advanced::widget::operation::black_box(&mut activate),
    );
    assert!(matches!(
        activate.message(),
        Some(Message::Naming(Action::Resolve))
    ));
    let mut activate = Activate::<Message>::new("naming.consent.structure");
    ui.operate(
        &renderer,
        &mut iced::advanced::widget::operation::black_box(&mut activate),
    );
    assert!(matches!(
        activate.message(),
        Some(Message::Naming(Action::StructureConsent(true)))
    ));
    Ok(())
}

#[tokio::test]
#[ignore = "Opt-in real renderer scrolled preview pointer regression"]
async fn scrolled_insert_click_reaches_button_instead_of_preview() -> Result<(), String> {
    use iced::advanced::renderer::Headless;
    use iced::advanced::widget::operation::{self, scrollable::AbsoluteOffset};
    use iced::{Event, mouse};
    use reshiki::accessibility::Collect;
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let preview = prepare_preview(app.engine.clone(), record("CCO"), "CCO".into()).await?;
    app.tab.naming.set_preview(preview);
    let size = iced::Size::new(340., 620.);
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .ok_or("No renderer")?;
    let mut ui = iced_runtime::UserInterface::build(
        iced::widget::scrollable(app.naming_panel())
            .id("naming-test-scroll")
            .height(620),
        size,
        Default::default(),
        &mut renderer,
    );
    let mut scroll = operation::scrollable::scroll_to::<()>(
        iced::advanced::widget::Id::from("naming-test-scroll"),
        AbsoluteOffset {
            x: Some(0.),
            y: Some(300.),
        },
    );
    ui.operate(&renderer, &mut operation::black_box(&mut scroll));
    let mut collect = Collect::new(iced::Rectangle::with_size(size));
    ui.operate(&renderer, &mut operation::black_box(&mut collect));
    let bounds = collect
        .snapshot()
        .nodes
        .iter()
        .find(|n| n.id == "naming.insert")
        .and_then(|n| n.visible_bounds)
        .ok_or("Insert control not visible after scroll")?;
    let point = bounds.center();
    let mut messages = vec![];
    for event in [
        Event::Mouse(mouse::Event::CursorMoved { position: point }),
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
    ] {
        ui.update(
            &[event],
            mouse::Cursor::Available(point),
            &mut renderer,
            &mut iced::advanced::clipboard::Null,
            &mut messages,
        );
    }
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, Message::Naming(Action::Insert))),
        "Scrolled click produced {messages:?}"
    );
    assert!(
        !messages
            .iter()
            .any(|m| matches!(m, Message::Naming(Action::PreviewEdit(Edit::Select(_)))))
    );
    drop(ui);
    for message in messages {
        let _ = app.update(message);
    }
    assert_eq!(app.tab.doc.atoms.len(), 3);
    assert!(app.tab.history.can_undo());
    Ok(())
}
