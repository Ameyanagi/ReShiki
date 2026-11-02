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
        name_status: None,
        smiles: identity.into(),
        canonical_smiles: identity.into(),
        synonyms: naming::aliases_for_smiles(identity).unwrap(),
        warnings: vec![],
        provenance: naming::Provenance::Openclatura,
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
            .structure_error
            .as_ref()
            .unwrap()
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

#[test]
fn inspector_name_spans_share_italics_small_caps_primes_and_unicode_superscripts() {
    let spans = chemical_name_spans(
        "L-(2R)-N'-5lambda^6-3a^1-tricyclo[3.2.2.0^{1,5}]nonane",
        13.,
        "Arial",
    );
    let text = spans
        .iter()
        .map(|span| span.text.as_ref())
        .collect::<String>()
        .replace('\u{2060}', "");
    assert_eq!(text, "L-(2R)-N′-5λ⁶-3a¹-tricyclo[3.2.2.0¹,⁵]nonane");
    let italic = spans
        .iter()
        .filter(|span| span.font.unwrap().style == iced::font::Style::Italic)
        .map(|span| span.text.as_ref().replace('\u{2060}', ""))
        .collect::<Vec<_>>();
    assert_eq!(italic, ["R", "N"]);
    assert!(
        spans
            .iter()
            .all(|span| span.font.unwrap().family == iced::font::Family::Name("Arial"))
    );
    assert_eq!(spans[0].size, Some(iced::Pixels(13. * 0.8)));
    assert_eq!(spans[0].font.unwrap().style, iced::font::Style::Normal);
    assert!(
        spans
            .iter()
            .skip(1)
            .all(|span| span.size == Some(iced::Pixels(13.)))
    );
}

#[test]
fn inspector_name_spans_use_the_caption_family_for_upright_and_italic_runs() {
    let family = "Caption Test Family";
    let spans = chemical_name_spans("(2E)-N,N-dimethylbut-2-enamide", 13., family);
    for span in &spans {
        assert_eq!(span.font.unwrap().family, iced::font::Family::Name(family));
    }
    let italic = spans
        .iter()
        .filter(|span| span.font.unwrap().style == iced::font::Style::Italic)
        .map(|span| span.text.as_ref().replace('\u{2060}', ""))
        .collect::<Vec<_>>();
    assert_eq!(italic, ["E", "N", "N"]);
    assert_eq!(spans[0].font.unwrap().style, iced::font::Style::Normal);
}

#[tokio::test]
async fn name_copy_preserves_engine_ascii_and_formatted_copy_uses_unicode() {
    use iced::futures::StreamExt;
    let mut app = ready();
    let ascii = "N,N'-5lambda^6-tricyclo[3.2.2.0^{1,5}]nonane";
    app.tab.naming.structure.as_mut().unwrap().1.systematic_name = Some(ascii.into());
    let before = app.tab.doc.clone();
    for (action, expected) in [
        (Action::CopyName, ascii),
        (
            Action::CopyFormattedName,
            "N,N′-5λ⁶-tricyclo[3.2.2.0¹,⁵]nonane",
        ),
    ] {
        let mut stream = iced_runtime::task::into_stream(app.naming_action(action)).unwrap();
        let Some(iced_runtime::Action::Clipboard(iced_runtime::clipboard::Action::Write {
            contents,
            ..
        })) = stream.next().await
        else {
            panic!("Name copy writes text to the clipboard");
        };
        assert_eq!(contents, expected);
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
    }
    assert_eq!(
        app.tab
            .naming
            .structure
            .as_ref()
            .unwrap()
            .1
            .systematic_name
            .as_deref(),
        Some(ascii)
    );
}

#[test]
fn name_typography_preference_controls_caption_placement_without_editing_existing_captions() {
    let mut app = ready();
    let atoms = app.tab.selected.clone();
    let name = "N,N'-dimethylmethanamine";
    let _ = app.update(Message::View(
        super::super::super::view_settings::Action::NameTypography(false),
    ));
    assert!(!app.appearance.name_typography);
    assert!(!app.tab.history.can_undo());
    app.place_molecule_caption(&atoms, name.into(), "CCO".into());
    assert_eq!(app.tab.doc.annotations[0].text, name);
    assert!(app.tab.doc.annotations[0].format.spans.is_empty());
    let plain = app.tab.doc.clone();
    let _ = app.update(Message::View(
        super::super::super::view_settings::Action::NameTypography(true),
    ));
    assert_eq!(app.tab.doc, plain);
    app.place_molecule_caption(&atoms, name.into(), "CCO".into());
    assert_eq!(app.tab.doc.annotations.len(), 1);
    assert_eq!(app.tab.doc.annotations[0].text, "N,N′-dimethylmethanamine");
    assert_eq!(app.tab.doc.annotations[0].format.spans.len(), 2);
    assert_eq!(
        app.tab.doc.molecule_names[0].text,
        app.tab.doc.annotations[0].text
    );
    assert_eq!(app.tab.doc.molecule_names.len(), 1);
    assert!(!app.error, "{}", app.status);
}

#[test]
fn manual_chemical_name_formatting_preserves_an_unedited_caption_link() {
    let mut app = ready();
    app.appearance.name_typography = false;
    let atoms = app.tab.selected.clone();
    app.place_molecule_caption(&atoms, "N,N'-dimethylmethanamine".into(), "CCO".into());
    let plain = app.tab.doc.clone();
    let caption = plain.annotations[0].id;
    app.tab.selected = vec![caption];
    let _ = app.update(Message::FormatChemicalName);
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc.molecule_names.len(), 1);
    let link = &app.tab.doc.molecule_names[0];
    assert_eq!(link.annotation, caption);
    assert_eq!(link.atoms, atoms);
    assert_eq!(link.smiles, "CCO");
    assert_eq!(link.text, "N,N′-dimethylmethanamine");
    assert_eq!(link.text, app.tab.doc.annotations[0].text);
    assert!(app.tab.doc.annotations[0].format.name_breaks);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, plain);
}
