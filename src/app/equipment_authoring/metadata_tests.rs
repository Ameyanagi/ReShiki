use super::super::Action as DraftAction;
use super::*;

fn send(app: &mut App, action: Action) {
    let draft = app
        .tab
        .equipment_authoring
        .as_ref()
        .unwrap()
        .identity
        .clone();
    let _ = app.update(message(&draft, action));
}

fn ready() -> App {
    let (mut app, edges) = super::super::tests::joint_drawing();
    let _ = app.update(Message::EquipmentAuthoring(DraftAction::Begin));
    super::super::tests::mark_joint(&mut app, edges);
    let _ = app.update(Message::EquipmentAuthoring(DraftAction::AddJoint));
    assert!(app.tab.equipment_authoring.as_ref().unwrap().ready());
    app
}

fn keyword(app: &mut App, value: &str) -> u64 {
    send(app, Action::AddKeyword);
    let id = app
        .tab
        .equipment_authoring
        .as_ref()
        .unwrap()
        .current
        .metadata
        .keywords
        .last()
        .unwrap()
        .id;
    send(app, Action::Text(Field::Keyword(id), value.into()));
    id
}

fn variant(app: &mut App, name: &str, value: &str) -> u64 {
    send(app, Action::AddVariant);
    let id = app
        .tab
        .equipment_authoring
        .as_ref()
        .unwrap()
        .current
        .metadata
        .variants
        .last()
        .unwrap()
        .id;
    send(app, Action::Text(Field::VariantName(id), name.into()));
    send(app, Action::Text(Field::VariantValue(id), value.into()));
    id
}

#[test]
fn equipment_metadata_undo_redo_and_discard_leave_the_drawing_untouched() {
    let mut app = ready();
    let before = app.tab.doc.clone();
    let revision = app.tab.revision;
    let dirty = app.dirty();
    let snapshot = app
        .tab
        .equipment_authoring
        .as_ref()
        .unwrap()
        .current
        .clone();
    send(&mut app, Action::Text(Field::Japanese, "フラスコ".into()));
    let shown = app.display_document().clone();
    assert_eq!(shown.piece_definitions[0].names["ja"], "フラスコ");
    let _ = app.update(Message::Undo);
    assert!(app.tab.equipment_authoring.as_ref().unwrap().current == snapshot);
    let _ = app.update(Message::Redo);
    assert_eq!(app.display_document(), &shown);
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.revision, revision);
    assert_eq!(app.dirty(), dirty);
    assert!(!app.tab.history.can_undo());
    let _ = app.update(Message::EquipmentAuthoring(DraftAction::Discard));
    assert!(app.tab.equipment_authoring.is_none());
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.revision, revision);
    assert_eq!(app.dirty(), dirty);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn equipment_metadata_accept_and_reopen_keep_metadata_geometry_and_one_undo() {
    let mut app = ready();
    let before = app.tab.doc.clone();
    let geometry = app
        .tab
        .equipment_authoring
        .as_ref()
        .unwrap()
        .current
        .draft
        .definition
        .clone();
    let _ = app.update(Message::EquipmentAuthoring(DraftAction::Name(
        "Custom flask".into(),
    )));
    send(
        &mut app,
        Action::Text(Field::Japanese, "自作フラスコ".into()),
    );
    send(&mut app, Action::Category(Category::Flask));
    send(&mut app, Action::Text(Field::Author, "Laboratory A".into()));
    send(&mut app, Action::Text(Field::License, "CC-BY-4.0".into()));
    keyword(&mut app, "air-free, glass");
    variant(&mut app, "capacity_ml", "100");
    variant(&mut app, "joint", "15/25");
    variant(&mut app, "note", "");
    let shown = app.display_document().clone();
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::EquipmentAuthoring(DraftAction::Accept));
    assert!(app.tab.equipment_authoring.is_none());
    assert_eq!(app.tab.doc, shown);
    assert_eq!(app.tab.history.undo_frames().len(), 1);
    let accepted = app.tab.doc.clone();
    let d = &accepted.piece_definitions[0];
    assert_eq!(d.names["en"], "Custom flask");
    assert_eq!(d.names["ja"], "自作フラスコ");
    assert_eq!(d.category, Category::Flask);
    assert_eq!(d.keywords, ["air-free, glass"]);
    assert_eq!(d.variant["capacity_ml"], "100");
    assert_eq!(d.variant["joint"], "15/25");
    assert_eq!(d.variant["note"], "");
    assert_eq!(d.credits.author.as_deref(), Some("Laboratory A"));
    assert_eq!(d.credits.license.as_deref(), Some("CC-BY-4.0"));
    assert_eq!(d.shapes, geometry.shapes);
    assert_eq!(d.joints, geometry.joints);
    assert_eq!(d.holds, geometry.holds);
    assert_eq!(d.params, geometry.params);
    assert_eq!(d.parts, geometry.parts);
    assert_eq!(d.liquids, geometry.liquids);
    assert_eq!(
        d.format_version,
        reshiki::equipment::format::PREVIEW_VERSION
    );
    let reopened = Document::from_json(&accepted.file_json().unwrap()).unwrap();
    assert_eq!(reopened.piece_definitions, accepted.piece_definitions);
    assert_eq!(reopened.graphics, accepted.graphics);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, accepted);
}

#[test]
fn equipment_metadata_invalid_fields_block_accept_without_losing_draft_history() {
    let mut app = ready();
    let before = app.tab.doc.clone();
    let revision = app.tab.revision;
    let shown = app.display_document().clone();
    for action in [
        Action::Text(Field::Japanese, "x".repeat(201)),
        Action::Text(Field::Author, "x".repeat(201)),
        Action::Text(Field::License, "bad\0licence".into()),
    ] {
        send(&mut app, action);
        let state = app.tab.equipment_authoring.as_ref().unwrap();
        assert!(!state.ready());
        assert!(state.notice.as_ref().unwrap().contains("printable"));
        assert!(state.preview.is_none());
        let _ = app.update(Message::EquipmentAuthoring(DraftAction::Accept));
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.revision, revision);
        assert!(!app.tab.history.can_undo());
        let _ = app.update(Message::Undo);
        assert_eq!(app.display_document(), &shown);
        let history = app.tab.equipment_authoring.as_ref().unwrap().past.len();
        send(&mut app, Action::Text(Field::Author, String::new()));
        let state = app.tab.equipment_authoring.as_ref().unwrap();
        assert_eq!(state.past.len(), history);
        assert!(
            !state.future.is_empty(),
            "an unchanged input keeps draft Redo"
        );
    }
    for name in [String::new(), "x".repeat(201)] {
        let _ = app.update(Message::EquipmentAuthoring(DraftAction::Name(name)));
        assert!(!app.tab.equipment_authoring.as_ref().unwrap().ready());
        let _ = app.update(Message::EquipmentAuthoring(DraftAction::Accept));
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.revision, revision);
        let _ = app.update(Message::Undo);
        assert_eq!(app.display_document(), &shown);
    }
    for i in 0..=reshiki::equipment::validate::MAX_KEYWORDS {
        keyword(&mut app, &format!("keyword {i}"));
    }
    let state = app.tab.equipment_authoring.as_ref().unwrap();
    assert!(!state.ready());
    assert!(state.notice.as_ref().unwrap().contains("limit is 64"));
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.revision, revision);
}

#[test]
fn equipment_metadata_rows_preserve_punctuation_and_ignore_removed_targets() {
    let mut app = ready();
    let first = keyword(&mut app, "remove me");
    let second = keyword(&mut app, "comma, equals=kept");
    send(&mut app, Action::RemoveKeyword(first));
    let shown = app.display_document().clone();
    let history = app.tab.equipment_authoring.as_ref().unwrap().past.len();
    send(
        &mut app,
        Action::Text(Field::Keyword(first), "stale".into()),
    );
    assert_eq!(app.display_document(), &shown);
    let state = app.tab.equipment_authoring.as_ref().unwrap();
    assert_eq!(state.past.len(), history);
    assert_eq!(state.current.metadata.keywords[0].id, second);
    variant(&mut app, "capacity", "100");
    variant(&mut app, "capacity", "200");
    let state = app.tab.equipment_authoring.as_ref().unwrap();
    assert!(!state.ready());
    assert!(state.notice.as_ref().unwrap().contains("listed twice"));
    let id = state.current.metadata.variants.last().unwrap().id;
    send(&mut app, Action::RemoveVariant(id));
    assert!(app.tab.equipment_authoring.as_ref().unwrap().ready());
}

#[tokio::test]
#[ignore = "Opt-in equipment metadata control tree through the real renderer"]
async fn equipment_metadata_controls_have_live_names_and_values() {
    use iced::advanced::{
        renderer::Headless,
        widget::{Operation, operation},
    };
    use iced_runtime::{UserInterface, user_interface::Cache};
    let mut app = ready();
    send(&mut app, Action::Category(Category::Flask));
    let keyword = keyword(&mut app, "glass");
    let variant = variant(&mut app, "capacity_ml", "100");
    let draft = app
        .tab
        .equipment_authoring
        .as_ref()
        .unwrap()
        .identity
        .clone();
    let mut renderer =
        <iced::Renderer as Headless>::new(iced::Font::default(), iced::Pixels(16.), None)
            .await
            .unwrap();
    let size = iced::Size::new(500., 2400.);
    let mut ui = UserInterface::build(
        app.equipment_authoring_panel(),
        size,
        Cache::new(),
        &mut renderer,
    );
    let mut collect = reshiki::accessibility::Collect::new(iced::Rectangle::with_size(size));
    ui.operate(&renderer, &mut operation::black_box(&mut collect));
    let tree = collect.snapshot();
    assert!(tree.duplicate_ids.is_empty());
    for (id, name, value) in [
        (
            "equipment-piece-name".into(),
            "English equipment name",
            "New equipment piece",
        ),
        (
            "equipment-metadata-japanese".into(),
            "Japanese name (optional)",
            "",
        ),
        (
            "equipment-metadata-category".into(),
            "Equipment category",
            "Flask",
        ),
        ("equipment-metadata-author".into(), "Author (optional)", ""),
        (
            "equipment-metadata-license".into(),
            "Licence (optional)",
            "",
        ),
        (format!("equipment-keyword-{keyword}"), "Keyword 1", "glass"),
        (
            format!("equipment-variant-name-{variant}"),
            "Variant 1 name",
            "capacity_ml",
        ),
        (
            format!("equipment-variant-value-{variant}"),
            "Variant 1 value",
            "100",
        ),
    ] {
        let id = control_id(&draft, &id);
        let node = tree.nodes.iter().find(|n| n.id == id).unwrap();
        assert_eq!(node.name, name);
        assert_eq!(node.value.as_deref(), Some(value));
    }
    let old_author = control_id(&draft, "equipment-metadata-author");
    let mut captured =
        reshiki::accessibility::SetValue::<Message>::new(old_author.clone(), "Old draft".into());
    ui.operate(&renderer, &mut operation::black_box(&mut captured));
    let operation::Outcome::Some(captured) = captured.finish() else {
        panic!("The original live control produces a value action");
    };
    let cache = ui.into_cache();
    let _ = app.update(Message::EquipmentAuthoring(DraftAction::Discard));
    let _ = app.update(Message::EquipmentAuthoring(DraftAction::Begin));
    send(&mut app, Action::Text(Field::Author, "New draft".into()));
    let before = app
        .tab
        .equipment_authoring
        .as_ref()
        .unwrap()
        .current
        .clone();
    let _ = app.update(captured);
    assert!(app.tab.equipment_authoring.as_ref().unwrap().current == before);
    let live_author = app
        .tab
        .equipment_authoring
        .as_ref()
        .unwrap()
        .metadata_control_id("equipment-metadata-author");
    let mut ui = UserInterface::build(app.equipment_authoring_panel(), size, cache, &mut renderer);
    let mut stale =
        reshiki::accessibility::SetValue::<Message>::new(old_author, "Stale native request".into());
    ui.operate(&renderer, &mut operation::black_box(&mut stale));
    assert!(matches!(stale.finish(), operation::Outcome::None));
    let mut live =
        reshiki::accessibility::SetValue::<Message>::new(live_author, "Live native edit".into());
    ui.operate(&renderer, &mut operation::black_box(&mut live));
    let operation::Outcome::Some(live) = live.finish() else {
        panic!("The replacement draft's live control remains editable");
    };
    let _ = ui.into_cache();
    let _ = app.update(live);
    assert_eq!(
        app.tab
            .equipment_authoring
            .as_ref()
            .unwrap()
            .current
            .metadata
            .author,
        "Live native edit"
    );
}

#[test]
fn equipment_metadata_discarded_draft_controls_cannot_edit_a_new_draft() {
    let mut app = ready();
    let drawing = app.tab.doc.clone();
    let selection = app.tab.selected.clone();
    let revision = app.tab.revision;
    let captured = {
        let state = app.tab.equipment_authoring.as_ref().unwrap();
        [
            DraftAction::Name("Old English name".into()),
            DraftAction::Metadata(Action::Text(Field::Author, "Old author".into())),
            DraftAction::Metadata(Action::Text(Field::Japanese, "古い下書き".into())),
            DraftAction::Metadata(Action::Category(Category::Heating)),
            DraftAction::Metadata(Action::AddKeyword),
        ]
        .map(|action| state.metadata_control_message(action))
    };
    let _ = app.update(Message::EquipmentAuthoring(DraftAction::Discard));
    let _ = app.update(Message::EquipmentAuthoring(DraftAction::Begin));
    let name = app
        .tab
        .equipment_authoring
        .as_ref()
        .unwrap()
        .metadata_control_message(DraftAction::Name("New English name".into()));
    let _ = app.update(name);
    send(&mut app, Action::Text(Field::Author, "New author".into()));
    send(
        &mut app,
        Action::Text(Field::Japanese, "新しい下書き".into()),
    );
    send(&mut app, Action::Category(Category::Flask));
    keyword(&mut app, "current, literal keyword");
    let state = app.tab.equipment_authoring.as_ref().unwrap();
    let before = state.current.clone();
    let history = state.past.len();
    for message in captured {
        let _ = app.update(message);
    }
    let state = app.tab.equipment_authoring.as_ref().unwrap();
    assert!(state.current == before);
    assert_eq!(state.past.len(), history);
    assert_eq!(app.tab.doc, drawing);
    assert_eq!(app.tab.selected, selection);
    assert_eq!(app.tab.revision, revision);
    let live = state.metadata_control_message(DraftAction::Metadata(Action::Text(
        Field::Author,
        "Live author".into(),
    )));
    let _ = app.update(live);
    let state = app.tab.equipment_authoring.as_ref().unwrap();
    assert_eq!(state.current.metadata.author, "Live author");
    assert_eq!(state.current.name, before.name);
    assert_eq!(state.current.metadata.japanese, before.metadata.japanese);
    assert!(state.current.metadata.keywords == before.metadata.keywords);
    assert_eq!(state.past.len(), history + 1);
}
