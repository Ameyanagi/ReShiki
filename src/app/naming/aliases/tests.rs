use super::super::test_support::ethanol;
use super::*;

fn record() -> Record {
    Record {
        title: "ethanol".into(),
        synonyms: naming::aliases_for_smiles("CCO").unwrap(),
        ..super::super::test_support::record("CCO")
    }
}
fn ready() -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = ethanol();
    app.tab.selected = app.tab.doc.all_ids();
    let ticket = Ticket {
        epoch: app.tab.file_epoch,
        revision: app.tab.revision,
        serial: 0,
    };
    app.tab.naming.structure = Some((ticket, record()));
    app
}

#[test]
fn import_details_resolve_aliases_without_opening_preview_or_inserting() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let _ = app.naming_action(Action::Name("ethanol".into()));
    let before = app.tab.doc.clone();
    let _ = app.naming_action(Action::DetailsOpen(true));
    let pending = app.tab.naming.pending.unwrap();
    assert!(matches!(pending, Pending::Request(_)));
    let _ = app.naming_action(Action::Finished(
        pending.ticket(),
        Box::new(Ok(Outcome::Names(
            vec![record()],
            Some(Box::new(Preview::new(
                record(),
                ethanol(),
                vec![],
                "CCO".into(),
            ))),
        ))),
    ));
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    assert!(!app.tab.naming.preview_open);
    assert!(app.tab.naming.details_open);
    assert_eq!(
        app.alias_record(AliasTarget::Import).unwrap().synonyms[1].name,
        "ethyl alcohol"
    );
}

#[test]
fn edited_name_can_resolve_current_aliases_without_reopening_details() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let _ = app.naming_action(Action::Name("ethanol".into()));
    let _ = app.naming_action(Action::DetailsOpen(true));
    let original_ticket = app.tab.naming.pending.unwrap().ticket();
    let original = Preview::new(record(), ethanol(), vec![], "CCO".into());
    let _ = app.naming_action(Action::Finished(
        original_ticket,
        Box::new(Ok(Outcome::Names(
            vec![record()],
            Some(Box::new(original.clone())),
        ))),
    ));
    let before = app.tab.doc.clone();
    let _ = app.naming_action(Action::Name("(R)-lactic acid".into()));
    assert!(app.tab.naming.details_open);
    assert!(!app.tab.naming.preview_open);
    assert!(app.alias_record(AliasTarget::Import).is_none());
    let _ = app.naming_action(Action::Resolve);
    let pending = app.tab.naming.pending.unwrap();
    assert!(matches!(pending, Pending::Request(_)));
    let _ = app.naming_action(Action::Resolve);
    assert_eq!(app.tab.naming.pending, Some(pending));
    let _ = app.naming_action(Action::Finished(
        original_ticket,
        Box::new(Ok(Outcome::Names(vec![record()], Some(Box::new(original))))),
    ));
    assert_eq!(app.tab.naming.pending, Some(pending));
    assert!(app.alias_record(AliasTarget::Import).is_none());
    let identity = "C[C@@H](O)C(=O)O";
    let source = Record {
        title: "(R)-lactic acid".into(),
        systematic_name: Some("(2R)-2-hydroxypropanoic acid".into()),
        smiles: identity.into(),
        canonical_smiles: identity.into(),
        synonyms: naming::aliases_for_smiles(identity).unwrap(),
        warnings: vec![],
        provenance: naming::Provenance::LocalRules,
    };
    let document = Document::from_json(include_bytes!(
        "../../../../tests/fixtures/chemical-naming-rust/r-lactic-rust-native.rsk"
    ))
    .unwrap();
    naming::verify_identity(
        identity,
        &naming::document_identity(&document).unwrap().smiles,
    )
    .unwrap();
    let _ = app.naming_action(Action::Finished(
        pending.ticket(),
        Box::new(Ok(Outcome::Names(
            vec![source.clone()],
            Some(Box::new(Preview::new(
                source,
                document,
                vec![],
                identity.into(),
            ))),
        ))),
    ));
    let current = app.alias_record(AliasTarget::Import).unwrap();
    assert_eq!(current.canonical_smiles, identity);
    assert_eq!(
        current
            .synonyms
            .iter()
            .map(|alias| alias.name.as_str())
            .collect::<Vec<_>>(),
        ["D-lactic acid"]
    );
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    assert!(app.tab.naming.details_open);
    assert!(!app.tab.naming.preview_open);
}

#[tokio::test]
#[ignore = "Opt-in actual renderer expanded Details resolve action and pending state"]
async fn expanded_details_resolve_action_is_accessible_and_disabled_while_pending()
-> Result<(), String> {
    use iced::advanced::renderer::Headless;
    use reshiki::accessibility::{Activate, Collect};
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.naming.details_open = true;
    app.tab
        .naming
        .set_preview(Preview::new(record(), ethanol(), vec![], "CCO".into()));
    let _ = app.naming_action(Action::Name("(R)-lactic acid".into()));
    let before = app.tab.doc.clone();
    let size = iced::Size::new(340., 2600.);
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .ok_or("No renderer")?;
    for pending in [false, true] {
        let mut ui = iced_runtime::UserInterface::build(
            app.name_import_panel(),
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
        let node = snapshot
            .nodes
            .iter()
            .find(|node| node.id == "import-name-resolve")
            .ok_or("Resolve name control missing")?;
        assert!(node.visible_bounds.is_some());
        assert_eq!(node.enabled, !pending);
        assert!(
            !snapshot
                .nodes
                .iter()
                .any(|node| node.id.starts_with("import-name-alias-"))
        );
        let mut activate = Activate::<Message>::new("import-name-resolve");
        ui.operate(
            &renderer,
            &mut iced::advanced::widget::operation::black_box(&mut activate),
        );
        let message = activate.message().cloned();
        drop(ui);
        if pending {
            assert!(message.is_none());
        } else {
            assert!(matches!(message, Some(Message::Naming(Action::Resolve))));
            let _ = app.update(message.unwrap());
            assert!(matches!(app.tab.naming.pending, Some(Pending::Request(_))));
        }
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
        assert!(!app.tab.naming.preview_open);
    }
    Ok(())
}

#[test]
fn alias_caption_uses_the_verified_molecule_link_and_one_history_step() -> Result<(), String> {
    let mut app = ready();
    let before = app.tab.doc.clone();
    let _ = app.naming_action(Action::Alias(AliasAction::Caption(1)));
    assert_eq!(app.tab.doc.annotations.len(), 1);
    assert_eq!(app.tab.doc.annotations[0].text, "ethyl alcohol");
    assert_eq!(app.tab.doc.atoms, before.atoms);
    assert_eq!(app.tab.doc.bonds, before.bonds);
    assert_eq!(app.tab.doc.molecule_names.len(), 1);
    let captioned = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, captioned);
    let reopened = Document::from_native_file(&captioned.file_json()?)?;
    assert_eq!(reopened.annotations, captioned.annotations);
    assert_eq!(reopened.molecule_names, captioned.molecule_names);
    // Regenerate first: a history/chemistry revision cannot reuse an old ticket.
    let ticket = Ticket {
        epoch: app.tab.file_epoch,
        revision: app.tab.revision,
        serial: 1,
    };
    app.tab.naming.structure = Some((ticket, record()));
    let _ = app.naming_action(Action::Alias(AliasAction::Caption(0)));
    assert_eq!(
        app.tab.doc.annotations.len(),
        1,
        "Reuse the linked caption, not a duplicate"
    );
    assert_eq!(app.tab.doc.annotations[0].text, "ethanol");
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, captioned);
    Ok(())
}

#[test]
fn stale_selection_epoch_and_invalid_alias_never_partially_change_the_drawing() {
    let mut app = ready();
    let before = app.tab.doc.clone();
    let _ = app.naming_action(Action::Alias(AliasAction::Caption(999)));
    assert_eq!(app.tab.doc, before);
    app.tab.selected.clear();
    let _ = app.naming_action(Action::Alias(AliasAction::Caption(1)));
    assert_eq!(app.tab.doc, before);
    assert!(
        app.tab
            .naming
            .notice
            .as_ref()
            .unwrap()
            .detail
            .contains("selection changed")
    );
    app.tab.selected = app.tab.doc.all_ids();
    app.tab.file_epoch += 1;
    let _ = app.naming_action(Action::Alias(AliasAction::Caption(1)));
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn import_alias_is_an_explicit_caption_choice_without_changing_the_entered_name()
-> Result<(), String> {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let _ = app.naming_action(Action::Name("ethanol".into()));
    app.tab
        .naming
        .set_preview(Preview::new(record(), ethanol(), vec![], "CCO".into()));
    let _ = app.naming_action(Action::Alias(AliasAction::UseBelow(Some(1))));
    assert_eq!(app.tab.naming.name, "ethanol");
    assert!(app.tab.naming.add_name_below);
    let before = app.tab.doc.clone();
    let _ = app.naming_action(Action::InsertName);
    assert_eq!(app.tab.doc.annotations[0].text, "ethyl alcohol");
    assert_eq!(naming::document_identity(&app.tab.doc)?.smiles, "CCO");
    let inserted = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, inserted);
    let _ = app.naming_action(Action::Name("acetone".into()));
    assert_eq!(
        app.tab.naming.import_alias, None,
        "New input discards old alias choices"
    );
    Ok(())
}

#[test]
fn modified_or_unapplied_preview_cannot_offer_original_alias_actions() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab
        .naming
        .set_preview(Preview::new(record(), ethanol(), vec![], "CCO".into()));
    let _ = app.naming_action(Action::Smiles("CCN".into()));
    let _ = app.naming_action(Action::Alias(AliasAction::UseBelow(Some(1))));
    assert_eq!(app.tab.naming.import_alias, None);
    app.tab.naming.smiles = "CCO".into();
    app.tab.naming.preview.as_mut().unwrap().modified = true;
    let _ = app.naming_action(Action::Alias(AliasAction::UseBelow(Some(1))));
    assert_eq!(app.tab.naming.import_alias, None);
    assert!(!app.tab.history.can_undo());
}

#[tokio::test]
#[ignore = "Opt-in actual renderer Import alias controls and native action dispatch"]
async fn import_details_publish_distinct_alias_and_systematic_actions() -> Result<(), String> {
    use iced::advanced::renderer::Headless;
    use reshiki::accessibility::{Activate, Collect};
    let mut app = ready();
    app.tab.naming.details_open = true;
    app.tab
        .naming
        .set_preview(Preview::new(record(), ethanol(), vec![], "CCO".into()));
    let size = iced::Size::new(340., 2600.);
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .ok_or("No renderer")?;
    let mut ui = iced_runtime::UserInterface::build(
        app.name_import_panel(),
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
        "import-name-generate",
        "import-name-alias-copy-1",
        "import-name-alias-caption-1",
        "naming-alias-copy-1",
        "naming-alias-caption-1",
    ] {
        assert!(
            snapshot
                .nodes
                .iter()
                .any(|n| n.id == id && n.visible_bounds.is_some()),
            "Missing {id}"
        );
    }
    let mut activate = Activate::<Message>::new("naming-alias-caption-1");
    ui.operate(
        &renderer,
        &mut iced::advanced::widget::operation::black_box(&mut activate),
    );
    assert!(matches!(
        activate.message(),
        Some(Message::Naming(Action::Alias(AliasAction::Caption(1))))
    ));
    Ok(())
}
