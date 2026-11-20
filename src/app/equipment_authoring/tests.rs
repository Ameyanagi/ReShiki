use super::*;
use reshiki::graphics::{Graphic, GraphicKind, GraphicStyle};

fn drawing() -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    for (lo, hi) in [
        (Point::new(-20., 0.), Point::new(20., 50.)),
        (Point::new(-8., -20.), Point::new(8., 0.)),
    ] {
        let id = app.tab.doc.next_id();
        app.tab.doc.graphics.push(Graphic::dragged(
            id,
            GraphicKind::Rectangle,
            lo,
            hi,
            GraphicStyle::default(),
            Default::default(),
            false,
        ));
        app.tab.selected.push(id);
    }
    app.tab.doc.group_selection(&app.tab.selected).unwrap();
    app
}

fn send(app: &mut App, action: Action) {
    let _ = app.update(Message::EquipmentAuthoring(action));
}
fn calibrate(app: &mut App) {
    let _ = app.update(Message::Canvas(Edit::EquipmentAuthoringPoint(Point::new(
        -20., 0.,
    ))));
    let _ = app.update(Message::Canvas(Edit::EquipmentAuthoringPoint(Point::new(
        20., 0.,
    ))));
    send(app, Action::Length("20".into()));
    send(app, Action::Calibrate);
    assert!(
        app.tab.equipment_authoring.as_ref().unwrap().ready(),
        "{}",
        app.status
    );
}

#[test]
fn equipment_authoring_normal_app_preview_accept_and_one_document_undo() {
    let mut app = drawing();
    let before = app.tab.doc.clone();
    let selected = app.tab.selected.clone();
    assert!(app.can_define_equipment());
    send(&mut app, Action::Begin);
    assert!(app.inspector_open);
    assert_eq!(app.inspector_tab, InspectorTab::Properties);
    assert!(app.tab.drawing_edit_active());
    calibrate(&mut app);
    send(&mut app, Action::Name("Glass adapter".into()));
    assert_eq!(
        app.tab.doc, before,
        "calibration and naming leave source untouched"
    );
    assert!(!app.tab.history.can_undo());
    let shown = app.display_document().clone();
    assert!(shown.graphics.iter().any(|g| g.kind == GraphicKind::Piece));
    for blocked in [
        Message::Delete,
        Message::New,
        Message::ToggleInspector,
        Message::Inspector(InspectorTab::Equipment),
    ] {
        let _ = app.update(blocked);
        assert_eq!(app.tab.doc, before);
        assert!(app.inspector_open);
        assert_eq!(app.inspector_tab, InspectorTab::Properties);
    }
    let _ = app.update(Message::ContextKey("Enter".into()));
    assert!(!app.error, "{}", app.status);
    assert!(app.tab.equipment_authoring.is_none());
    assert_eq!(app.tab.doc, shown);
    assert_eq!(app.tab.doc.graphics.len(), 1);
    assert_eq!(app.tab.history.undo_frames().len(), 1);
    let piece = app.tab.selected[0];
    assert!(
        app.tab.doc.groups.is_empty(),
        "the piece replaces a now redundant single-member group"
    );
    assert!(
        app.tab
            .doc
            .graphics
            .iter()
            .any(|graphic| graphic.id == piece)
    );
    assert_eq!(
        app.tab.doc.piece_definitions[0].names["en"],
        "Glass adapter"
    );
    let accepted = app.tab.doc.clone();
    let json = serde_json::to_string(&accepted).unwrap();
    let mut reopened: Document = serde_json::from_str(&json).unwrap();
    reshiki::equipment::placed::link(&mut reopened).unwrap();
    assert_eq!(reopened, accepted);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    assert!(
        selected
            .iter()
            .all(|id| app.tab.doc.graphics.iter().any(|g| g.id == *id))
    );
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, accepted);
}

#[test]
fn equipment_authoring_discard_keeps_document_history_and_redo() {
    let mut app = drawing();
    let before = app.tab.doc.clone();
    app.tab.doc.add_atom("C", Point::new(300., 300.));
    app.changed(before.clone());
    let _ = app.update(Message::Undo);
    let revision = app.tab.revision;
    let selected = app.tab.selected.clone();
    send(&mut app, Action::Begin);
    calibrate(&mut app);
    let _ = app.update(Message::Escape);
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.selected, selected);
    assert_eq!(app.tab.revision, revision);
    assert!(app.tab.history.can_redo());
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc.atoms.len(), 1);
}

#[test]
fn equipment_authoring_has_its_own_undo_and_rejects_invalid_calibration() {
    let mut app = drawing();
    let before = app.tab.doc.clone();
    send(&mut app, Action::Begin);
    calibrate(&mut app);
    let _ = app.update(Message::Undo);
    assert!(
        app.tab
            .equipment_authoring
            .as_ref()
            .unwrap()
            .current
            .measuring
    );
    assert!(!app.tab.history.can_undo());
    let _ = app.update(Message::Redo);
    assert!(app.tab.equipment_authoring.as_ref().unwrap().ready());
    send(&mut app, Action::MeasureAgain);
    send(&mut app, Action::Point(Point::new(0., 0.)));
    send(&mut app, Action::Point(Point::new(0., 0.)));
    for invalid in ["0", "NaN", "inf", "-20", "20"] {
        send(&mut app, Action::Length(invalid.into()));
        send(&mut app, Action::Calibrate);
        assert!(!app.tab.equipment_authoring.as_ref().unwrap().ready());
        assert!(
            app.tab
                .equipment_authoring
                .as_ref()
                .unwrap()
                .notice
                .is_some()
        );
    }
    send(&mut app, Action::Accept);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn equipment_authoring_accept_rechecks_sources_and_does_not_overwrite_them() {
    let mut app = drawing();
    send(&mut app, Action::Begin);
    calibrate(&mut app);
    app.tab.doc.graphics[0].style.effects.opacity = Some(0.4);
    let changed = app.tab.doc.clone();
    send(&mut app, Action::Accept);
    assert!(app.error);
    assert!(app.tab.equipment_authoring.is_some());
    assert_eq!(app.tab.doc, changed);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn equipment_authoring_reuses_model_selection_policy_and_new_file_clears_draft() {
    let mut app = drawing();
    let id = app.tab.doc.add_atom("C", Point::new(200., 0.));
    app.tab.selected.push(id);
    assert!(!app.can_define_equipment());
    send(&mut app, Action::Begin);
    assert!(app.tab.equipment_authoring.is_none());
    app.tab.selected.retain(|selected| *selected != id);
    send(&mut app, Action::Begin);
    assert!(app.tab.equipment_authoring.is_some());
    app.begin_file_epoch();
    assert!(app.tab.equipment_authoring.is_none());
}

#[test]
fn equipment_authoring_draft_keys_own_history_cancel_and_confirmation() {
    use keyboard::{Key, Modifiers, key::Named};
    assert!(matches!(
        modal_key(&Key::Named(Named::Escape), Modifiers::empty()),
        Some(Message::Escape)
    ));
    assert!(
        matches!(leftover_key(&Key::Named(Named::Enter), Modifiers::empty()), Some(Message::ContextKey(key)) if key == "Enter")
    );
    assert!(
        modal_key(&Key::Named(Named::Enter), Modifiers::empty()).is_none(),
        "focused controls receive Enter before leftover confirmation"
    );
    let command = if cfg!(target_os = "macos") {
        Modifiers::LOGO
    } else {
        Modifiers::CTRL
    };
    assert!(matches!(
        modal_key(&Key::Character("z".into()), command),
        Some(Message::Undo)
    ));
    assert!(matches!(
        modal_key(&Key::Character("z".into()), command | Modifiers::SHIFT),
        Some(Message::Redo)
    ));
    assert!(modal_key(&Key::Character("m".into()), Modifiers::empty()).is_none());
}

#[test]
fn equipment_authoring_screenshot_fixture_uses_the_normal_model_and_app_route() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = serde_json::from_str(include_str!(
        "../../../tests/fixtures/equipment-authoring/measured-flask.rsk"
    ))
    .unwrap();
    app.tab.doc.validate().unwrap();
    app.tab.selected = vec![1];
    assert!(app.can_define_equipment());
    let before = app.tab.doc.clone();
    send(&mut app, Action::Begin);
    send(&mut app, Action::Point(Point::new(-24., -100.)));
    send(&mut app, Action::Point(Point::new(24., -100.)));
    send(&mut app, Action::Length("16".into()));
    send(&mut app, Action::Name("Measured flask".into()));
    send(&mut app, Action::Calibrate);
    assert!(app.tab.equipment_authoring.as_ref().unwrap().ready());
    assert_eq!(app.tab.doc, before);
    send(&mut app, Action::Accept);
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc.graphics.len(), 1);
    assert_eq!(app.tab.doc.graphics[0].kind, GraphicKind::Piece);
}

#[test]
fn equipment_authoring_preserves_normal_callout_references_in_one_transaction() {
    use reshiki::leaders::{Leader, Target};
    let mut app = drawing();
    let id = app.tab.doc.next_id();
    let mut callout = Graphic::dragged(
        id,
        GraphicKind::Line,
        Point::new(150., 80.),
        Point::new(20., 40.),
        Default::default(),
        Default::default(),
        false,
    );
    callout.leader = Some(Leader {
        start: None,
        end: Some(Target::Object { id: 1 }),
    });
    app.tab.doc.graphics.push(callout);
    app.tab.doc.validate().unwrap();
    let before = app.tab.doc.clone();
    send(&mut app, Action::Begin);
    calibrate(&mut app);
    send(&mut app, Action::Accept);
    assert!(!app.error, "{}", app.status);
    let piece = app.tab.selected[0];
    let callout = app.tab.doc.graphics.iter().find(|g| g.id == id).unwrap();
    assert_eq!(
        callout.leader.as_ref().unwrap().end,
        Some(Target::Object { id: piece })
    );
    assert_eq!(app.tab.history.undo_frames().len(), 1);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn equipment_authoring_suspends_keyboard_drawing_and_admits_update_receipts() {
    use super::super::updates::Action as Update;
    let mut app = drawing();
    app.sync_keyboard_drawing();
    assert!(app.keyboard_drawing_active());
    // Do not execute the returned network task; deliver its receipt ourselves.
    let _ = app.update_action(Update::Check(true));
    send(&mut app, Action::Begin);
    assert!(!app.keyboard_drawing_active());
    let _ = app.update(Message::Updates(Update::Checked(1, Err("offline".into()))));
    assert_eq!(app.updates.error(), Some("offline"));
    let _ = app.update(Message::Updates(Update::Show(true)));
    assert!(
        !app.updates.open,
        "the receipt must not admit interactive update actions"
    );
    send(&mut app, Action::Discard);
    assert!(app.keyboard_drawing_active());
    let _ = app.update_action(Update::Check(true));
    assert!(
        app.updates.error().is_none(),
        "the completed check no longer blocks another check"
    );
    let _ = app.update(Message::Updates(Update::Checked(
        2,
        Err("second receipt".into()),
    )));
    assert_eq!(app.updates.error(), Some("second receipt"));
}

#[test]
fn equipment_authoring_waits_for_update_work_before_opening_a_draft() {
    use super::super::updates::Action as Update;
    let mut app = drawing();
    // A pending preference save uses the same idle contract as an install or
    // restart. Its returned task is not executed by this app-state test.
    let _ = app.update_action(Update::Automatic(true));
    assert!(!app.updates.idle());
    assert!(!app.can_define_equipment());
    send(&mut app, Action::Begin);
    assert!(app.tab.equipment_authoring.is_none());
    let _ = app.update(Message::Updates(Update::Saved(Ok(()))));
    assert!(app.updates.idle());
    send(&mut app, Action::Begin);
    assert!(app.tab.equipment_authoring.is_some());
}

#[test]
fn equipment_authoring_creates_independent_identities_across_drawings() {
    let mut first = drawing();
    let mut second = drawing();
    assert_eq!(first.tab.doc, second.tab.doc);
    for app in [&mut first, &mut second] {
        send(app, Action::Begin);
        calibrate(app);
        send(app, Action::Accept);
        assert!(!app.error, "{}", app.status);
    }
    let a = &first.tab.doc.piece_definitions[0];
    let b = &second.tab.doc.piece_definitions[0];
    assert_ne!(
        a.id, b.id,
        "separately authored pieces must coexist in My pieces"
    );
    assert!(a.id.starts_with("user/"));
    assert_eq!(a.revision, 1);
    assert_eq!(a.shapes, b.shapes);
}
