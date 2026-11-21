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
            GraphicStyle {
                width_pt: 0.4,
                ..Default::default()
            },
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

fn joint_drawing() -> (App, [Point; 2]) {
    use reshiki::{graphics::PathCommand, palette::Color};
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let id = app.tab.doc.next_id();
    let mut body = Graphic::pen_curve(
        id,
        Point::default(),
        Point::new(1., 1.),
        GraphicStyle {
            fill: Some(Color::Paper),
            ..Default::default()
        },
    );
    body.origin = Point::default();
    body.axis_x = Point::new(1., 0.);
    body.axis_y = Point::new(0., 1.);
    body.node_types.clear();
    // The socket's generated base meets this neck at y=29, x=±6.5.
    body.path = vec![
        PathCommand::Move(Point::new(-9.6, 0.)),
        PathCommand::Line(Point::new(9.6, 0.)),
        PathCommand::Line(Point::new(6.5, 29.)),
        PathCommand::Line(Point::new(25., 45.)),
        PathCommand::Line(Point::new(-25., 45.)),
        PathCommand::Line(Point::new(-6.5, 29.)),
        PathCommand::Close,
    ];
    app.tab.doc.graphics.push(body);
    app.tab.selected.push(id);
    (app, [Point::new(-9.6, 0.), Point::new(9.6, 0.)])
}

fn mark_joint(app: &mut App, edges: [Point; 2]) {
    send(app, Action::BeginJoint);
    for point in edges {
        let _ = app.update(Message::Canvas(Edit::EquipmentAuthoringPoint(point)));
    }
}

#[test]
fn equipment_authoring_first_joint_previews_a_true_port_and_accepts_with_one_undo() {
    let (mut app, edges) = joint_drawing();
    let before = app.tab.doc.clone();
    send(&mut app, Action::Begin);
    mark_joint(&mut app, edges);
    let state = app.tab.equipment_authoring.as_ref().unwrap();
    assert_eq!(state.notice, None);
    assert!(!state.ready(), "a candidate must be added before accepting");
    assert!(state.current.draft.definition.joints.is_empty());
    assert!(!state.current.draft.calibrated());
    let shown = app.display_document().clone();
    assert_eq!(shown.piece_definitions[0].joints.len(), 1);
    let ports = reshiki::equipment::solve::ports(&shown.graphics[0]);
    assert_eq!(
        ports.len(),
        1,
        "the preview uses a typed joint, not artwork alone"
    );
    assert!(ports[0].dir[1] < -0.99);
    let [at, toward] = state.measurement().direction.unwrap();
    assert!(toward.y < at.y, "the arrow points out of the mouth");
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    // Enter adds to the detached draft, then a second Enter accepts the piece.
    let _ = app.update(Message::ContextKey("Enter".into()));
    assert!(app.tab.equipment_authoring.as_ref().unwrap().ready());
    let _ = app.update(Message::Undo);
    let state = app.tab.equipment_authoring.as_ref().unwrap();
    assert!(state.current.joint.is_some());
    assert!(state.current.draft.definition.joints.is_empty());
    assert!(!app.tab.history.can_undo());
    let _ = app.update(Message::Redo);
    assert!(app.tab.equipment_authoring.as_ref().unwrap().ready());
    let _ = app.update(Message::ContextKey("Enter".into()));
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc, shown);
    assert_eq!(app.tab.history.undo_frames().len(), 1);
    let accepted = app.tab.doc.clone();
    let bytes = accepted.file_json().unwrap();
    let reopened = Document::from_json(&bytes).unwrap();
    assert_eq!(
        reshiki::equipment::solve::ports(&reopened.graphics[0]).len(),
        1
    );
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, accepted);
}

#[test]
fn equipment_authoring_joint_errors_flip_and_cancel_are_draft_edits() {
    let (mut app, edges) = joint_drawing();
    let before = app.tab.doc.clone();
    send(&mut app, Action::Begin);
    mark_joint(&mut app, edges);
    let candidate = app.display_document().clone();
    send(&mut app, Action::FlipJoint);
    let state = app.tab.equipment_authoring.as_ref().unwrap();
    assert!(state.notice.is_some());
    assert!(state.joint_candidate.is_none());
    let [at, toward] = state.measurement().direction.unwrap();
    assert!(toward.y > at.y);
    send(&mut app, Action::AddJoint);
    assert!(
        app.tab
            .equipment_authoring
            .as_ref()
            .unwrap()
            .current
            .draft
            .definition
            .joints
            .is_empty()
    );
    let _ = app.update(Message::Undo);
    assert_eq!(app.display_document(), &candidate);
    send(&mut app, Action::JointSize("24/29".into()));
    let state = app.tab.equipment_authoring.as_ref().unwrap();
    assert_eq!(
        state.notice, None,
        "a first size may recalibrate the candidate"
    );
    assert!(!state.current.draft.calibrated());
    assert!(
        (state
            .joint_candidate
            .as_ref()
            .unwrap()
            .rim(edges)
            .unwrap()
            .gap_mm
            - 28.8)
            .abs()
            < 1e-5
    );
    send(&mut app, Action::MarkJointAgain);
    for point in [Point::new(-5., 35.), Point::new(5., 35.)] {
        send(&mut app, Action::Point(point));
    }
    assert!(
        app.tab
            .equipment_authoring
            .as_ref()
            .unwrap()
            .joint_candidate
            .is_none()
    );
    send(&mut app, Action::CancelJoint);
    let state = app.tab.equipment_authoring.as_ref().unwrap();
    assert!(state.current.measuring);
    assert!(state.current.points.is_empty());
    assert!(!state.current.draft.calibrated());
    send(&mut app, Action::Discard);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn equipment_authoring_joint_cone_and_unfilled_art_use_the_same_validation() {
    let (mut app, edges) = joint_drawing();
    send(&mut app, Action::Begin);
    mark_joint(&mut app, edges);
    send(&mut app, Action::JointEnd(JointEnd::Cone));
    let state = app.tab.equipment_authoring.as_ref().unwrap();
    assert_eq!(state.notice, None);
    let joint = &state.joint_candidate.as_ref().unwrap().definition.joints[0];
    assert!(matches!(
        joint,
        reshiki::equipment::format::Joint::Taper {
            gender: Gender::M,
            draw: JointDraw::Standard,
            part: None,
            ..
        }
    ));
    send(&mut app, Action::CancelJoint);
    assert!(
        !app.tab
            .equipment_authoring
            .as_ref()
            .unwrap()
            .current
            .draft
            .calibrated()
    );
    let mut app = drawing(); // ordinary unfilled outlines remain Detail, not invented glass
    send(&mut app, Action::Begin);
    mark_joint(&mut app, edges);
    let state = app.tab.equipment_authoring.as_ref().unwrap();
    assert!(
        state
            .notice
            .as_ref()
            .unwrap()
            .contains("filled glass outline")
    );
    assert!(state.joint_candidate.is_none());
}

#[test]
fn equipment_authoring_joint_fixture_calibration_and_later_size_errors() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = serde_json::from_str(include_str!(
        "../../../tests/fixtures/equipment-authoring/standard-joint-neck.rsk"
    ))
    .unwrap();
    app.tab.doc.validate().unwrap();
    app.tab.selected = vec![1];
    let before = app.tab.doc.clone();
    let edges = [Point::new(-28.8, 0.), Point::new(28.8, 0.)];
    send(&mut app, Action::Begin);
    for point in edges {
        send(&mut app, Action::Point(point));
    }
    send(&mut app, Action::Length("19.2".into()));
    send(&mut app, Action::Calibrate);
    assert!(app.tab.equipment_authoring.as_ref().unwrap().ready());
    let calibrated = app
        .tab
        .equipment_authoring
        .as_ref()
        .unwrap()
        .current
        .clone();
    mark_joint(&mut app, edges);
    send(&mut app, Action::CancelJoint);
    assert!(app.tab.equipment_authoring.as_ref().unwrap().current == calibrated);
    mark_joint(&mut app, edges);
    send(&mut app, Action::AddJoint);
    let draft = app
        .tab
        .equipment_authoring
        .as_ref()
        .unwrap()
        .current
        .draft
        .clone();
    assert_eq!(draft.definition.joints.len(), 1);
    mark_joint(&mut app, edges);
    send(&mut app, Action::JointSize("24/29".into()));
    let state = app.tab.equipment_authoring.as_ref().unwrap();
    assert!(state.notice.as_ref().unwrap().contains("opening is"));
    assert!(state.joint_candidate.is_none());
    assert_eq!(state.current.draft, draft);
    send(&mut app, Action::Accept);
    assert_eq!(
        app.tab.doc, before,
        "a pending invalid joint cannot be accepted"
    );
    send(&mut app, Action::CancelJoint);
    send(&mut app, Action::Accept);
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc.piece_definitions[0].joints.len(), 1);
    assert_eq!(app.tab.history.undo_frames().len(), 1);
}

#[test]
fn equipment_authoring_joint_drops_unconfirmed_measurement_without_mislabeling_scale() {
    let (mut app, edges) = joint_drawing();
    send(&mut app, Action::Begin);
    send(&mut app, Action::Point(edges[0]));
    send(&mut app, Action::Length("100".into()));
    mark_joint(&mut app, edges);
    send(&mut app, Action::AddJoint);
    let state = app.tab.equipment_authoring.as_ref().unwrap();
    assert!(state.ready());
    assert_eq!(state.current.measured_length, None);
    assert!(state.current.length.is_empty());
    assert!(state.current.points.is_empty());
    assert!((state.current.draft.rim(edges).unwrap().gap_mm - 19.2).abs() < 1e-5);

    let (mut app, edges) = joint_drawing();
    send(&mut app, Action::Begin);
    for point in edges {
        send(&mut app, Action::Point(point));
    }
    send(&mut app, Action::Length("19.2".into()));
    send(&mut app, Action::Calibrate);
    send(&mut app, Action::MeasureAgain);
    send(&mut app, Action::Length("100".into()));
    mark_joint(&mut app, edges);
    send(&mut app, Action::AddJoint);
    let state = app.tab.equipment_authoring.as_ref().unwrap();
    assert!(state.ready());
    assert_eq!(state.current.measured_length.as_deref(), Some("19.2"));
    assert!(state.current.length.is_empty());
}

#[test]
fn equipment_authoring_rejected_first_joint_arrow_uses_the_proposed_physical_scale() {
    use reshiki::graphics::PathCommand;
    let (mut app, _) = joint_drawing();
    // A small central notch contains neither default-scale probe. Once the
    // selected 15/25 size calibrates the draft, its +Y probe enters the body.
    app.tab.doc.graphics[0].path = [
        (-28.8, 0.),
        (-3., 0.),
        (-3., 1.8),
        (3., 1.8),
        (3., 0.),
        (28.8, 0.),
        (19.5, 87.),
        (75., 135.),
        (-75., 135.),
        (-19.5, 87.),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (x, y))| {
        let point = Point::new(x, y);
        if i == 0 {
            PathCommand::Move(point)
        } else {
            PathCommand::Line(point)
        }
    })
    .chain([PathCommand::Close])
    .collect();
    let before = app.tab.doc.clone();
    let edges = [Point::new(-28.8, 0.), Point::new(28.8, 0.)];
    send(&mut app, Action::Begin);
    mark_joint(&mut app, edges);
    let state = app.tab.equipment_authoring.as_ref().unwrap();
    assert!(state.joint_candidate.is_some());
    let [at, toward] = state.measurement().direction.unwrap();
    assert!(toward.y < at.y);
    send(&mut app, Action::FlipJoint);
    let state = app.tab.equipment_authoring.as_ref().unwrap();
    assert!(state.notice.is_some());
    assert!(state.joint_candidate.is_none());
    let [at, toward] = state.measurement().direction.unwrap();
    assert!(
        toward.y > at.y,
        "the rejected proposal points into the body"
    );
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Undo);
    let state = app.tab.equipment_authoring.as_ref().unwrap();
    assert!(state.joint_candidate.is_some());
    let [at, toward] = state.measurement().direction.unwrap();
    assert!(toward.y < at.y);
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
    // This preserved historical fixture has a 0.6 pt open rim. Prepare that
    // detail explicitly; conversion must no longer silently thin it.
    assert!(!app.can_define_equipment());
    send(&mut app, Action::Begin);
    assert!(app.status.contains("0.4 pt"));
    assert!(app.tab.equipment_authoring.is_none());
    for graphic in &mut app.tab.doc.graphics {
        if graphic.style.fill.is_none() {
            graphic.style.width_pt = 0.4;
        }
    }
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
fn equipment_authoring_refuses_incompatible_strokes_without_changing_source() {
    for color in [false, true] {
        let mut app = drawing();
        if color {
            app.tab.doc.graphics[0].style.stroke = reshiki::palette::Color::Custom([180, 50, 55]);
        } else {
            app.tab.doc.graphics[0].style.width_pt = 2.;
        }
        let before = app.tab.doc.clone();
        let revision = app.tab.revision;
        assert!(!app.can_define_equipment());
        send(&mut app, Action::Begin);
        assert!(app.tab.equipment_authoring.is_none());
        assert!(app.status.contains(if color { "Ink" } else { "0.4 pt" }));
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.revision, revision);
        assert!(!app.tab.history.can_undo());
    }
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
