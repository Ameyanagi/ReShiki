use super::*;
use reshiki::equipment::{
    builtin, placed,
    solve::{self, Offer},
};

fn begin(app: &mut App, edges: [Point; 2]) {
    send(app, Action::Begin);
    mark_joint(app, edges);
    send(app, Action::AddJoint);
    assert!(app.tab.equipment_authoring.as_ref().unwrap().ready());
    send(app, Action::TestSnapping);
    assert!(
        app.tab
            .equipment_authoring
            .as_ref()
            .unwrap()
            .trial
            .is_some()
    );
    choose(app, "reshiki/stopper-15");
}

fn choose(app: &mut App, id: &str) {
    let choice = app
        .tab
        .equipment_authoring
        .as_ref()
        .unwrap()
        .trial
        .as_ref()
        .unwrap()
        .choices
        .iter()
        .find(|c| c.definition.id == id)
        .unwrap()
        .clone();
    send(app, Action::TestPiece(choice));
}

fn toward(app: &mut App, target: u64, finish: bool) -> Point {
    let (at, epoch, reach) = {
        let state = app.tab.equipment_authoring.as_ref().unwrap();
        let trial = state.trial.as_ref().unwrap();
        let doc = app.display_document();
        let moving = doc
            .graphics
            .iter()
            .find(|g| Some(g.id) == trial.id)
            .unwrap();
        let target = doc.graphics.iter().find(|g| g.id == target).unwrap();
        let moving_port = solve::ports(moving).remove(0);
        let target_port = solve::ports(target).remove(0);
        let center = placed::middle(moving);
        (
            center.offset(
                (target_port.point[0] - moving_port.point[0]) as f32,
                (target_port.point[1] - moving_port.point[1]) as f32,
            ),
            trial.epoch,
            solve::REACH_PX / app.tab.camera.zoom,
        )
    };
    send(app, Action::TestStart(epoch));
    send(
        app,
        Action::TestAt {
            at,
            epoch,
            reach,
            finish,
        },
    );
    at
}

fn target(app: &App) -> u64 {
    app.tab
        .equipment_authoring
        .as_ref()
        .unwrap()
        .preview_piece
        .unwrap()
}

#[test]
fn equipment_trial_matching_scale_and_repeated_updates_use_a_clean_baseline() {
    let (mut app, edges) = joint_drawing();
    let before = app.tab.doc.clone();
    let revision = app.tab.revision;
    begin(&mut app, edges);
    let id = target(&app);
    let at = toward(&mut app, id, false);
    let shown = app.display_document().clone();
    let state = app.tab.equipment_authoring.as_ref().unwrap();
    let trial = state.trial.as_ref().unwrap();
    let snap = trial.snap.as_ref().unwrap();
    assert!(matches!(snap.offer, Offer::Seat));
    assert_eq!(shown.seats.len(), 1);
    let moving = shown
        .graphics
        .iter()
        .find(|g| Some(g.id) == trial.id)
        .unwrap();
    let fixed = shown.graphics.iter().find(|g| g.id == id).unwrap();
    assert!(
        (moving.axis_x.x.hypot(moving.axis_x.y) - fixed.axis_x.x.hypot(fixed.axis_x.y)).abs()
            < 1e-5
    );
    let epoch = trial.epoch;
    let reach = trial.reach;
    for _ in 0..4 {
        send(
            &mut app,
            Action::TestAt {
                at,
                epoch,
                reach,
                finish: false,
            },
        );
        assert_eq!(app.display_document(), &shown);
    }
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.revision, revision);
    assert!(!app.tab.history.can_undo());
    assert_eq!(
        app.tab.doc.file_json().unwrap(),
        before.file_json().unwrap()
    );
}

#[test]
fn equipment_trial_misalignment_mismatch_and_adapters_use_normal_feedback() {
    let (mut app, edges) = joint_drawing();
    let before = app.tab.doc.clone();
    begin(&mut app, edges);
    let id = target(&app);
    send(&mut app, Action::TestTurn(90.));
    toward(&mut app, id, false);
    assert!(
        app.tab
            .equipment_authoring
            .as_ref()
            .unwrap()
            .trial
            .as_ref()
            .unwrap()
            .snap
            .is_none()
    );
    choose(&mut app, "reshiki/rbf-50-15");
    send(&mut app, Action::TestTurn(180.));
    toward(&mut app, id, false);
    let state = app.tab.equipment_authoring.as_ref().unwrap();
    let snap = state.trial.as_ref().unwrap().snap.as_ref().unwrap();
    assert!(matches!(snap.offer, Offer::Mismatch));
    assert!(
        super::super::super::equipment::snap_text(app.display_document(), snap)
            .contains("Two sockets")
    );
    assert!(app.display_document().seats.is_empty());
    assert!(
        state.ready(),
        "a wrong test piece does not invalidate the authored piece"
    );
    choose(&mut app, "reshiki/stopper-29");
    send(&mut app, Action::TestTurn(0.));
    let at = toward(&mut app, id, false);
    let state = app.tab.equipment_authoring.as_ref().unwrap();
    let trial = state.trial.as_ref().unwrap();
    assert!(matches!(
        trial.snap.as_ref().unwrap().offer,
        Offer::Adapter(_)
    ));
    assert_eq!(app.display_document().seats.len(), 2);
    let added = trial.snap.as_ref().unwrap().added.unwrap();
    assert_eq!(
        app.display_document()
            .graphics
            .iter()
            .find(|g| g.id == added)
            .unwrap()
            .style
            .effects
            .opacity,
        Some(0.5)
    );
    let shown = app.display_document().clone();
    let (epoch, reach) = (trial.epoch, trial.reach);
    for _ in 0..3 {
        send(
            &mut app,
            Action::TestAt {
                at,
                epoch,
                reach,
                finish: false,
            },
        );
        assert_eq!(app.display_document(), &shown);
    }
    assert_eq!(app.tab.doc, before);
    send(&mut app, Action::FinishTesting);
    assert_eq!(app.display_document().graphics.len(), 1);
    assert!(app.display_document().seats.is_empty());
    assert_eq!(app.display_document().piece_definitions.len(), 1);
}

#[test]
fn equipment_trial_adapter_cancellation_restores_the_visible_fit_without_accumulation() {
    for canvas_cancel in [false, true] {
        let (mut app, edges) = joint_drawing();
        let before = app.tab.doc.clone();
        let revision = app.tab.revision;
        begin(&mut app, edges);
        choose(&mut app, "reshiki/stopper-29");
        let id = target(&app);
        toward(&mut app, id, true);
        let state = app.tab.equipment_authoring.as_ref().unwrap();
        let canonical = state.base_preview.clone().unwrap();
        let shown = app.display_document().clone();
        let trial = state.trial.as_ref().unwrap();
        let pose = (trial.at, trial.turn, trial.scale, trial.reach);
        let snap = trial.snap.clone().unwrap();
        assert!(matches!(snap.offer, Offer::Adapter(_)));
        assert!(snap.added.is_some());
        assert_eq!(shown.seats.len(), 2);
        let feedback = super::super::super::equipment::snap_text(&shown, &snap);
        for attempt in 0..3 {
            let epoch = app
                .tab
                .equipment_authoring
                .as_ref()
                .unwrap()
                .trial
                .as_ref()
                .unwrap()
                .epoch;
            send(&mut app, Action::TestStart(epoch));
            if attempt > 0 {
                send(
                    &mut app,
                    Action::TestAt {
                        at: pose.0.offset(1000., 0.),
                        epoch,
                        reach: pose.3,
                        finish: false,
                    },
                );
                assert!(app.display_document().seats.is_empty());
            }
            if canvas_cancel {
                // Pointer cancellation, lost touch and pinch all use this batch.
                let _ = app.update(Message::Canvas(Edit::Touch(vec![
                    Edit::EquipmentAuthoringTestCancel(epoch),
                    Edit::Hover(None),
                ])));
            } else {
                send(&mut app, Action::TestCancel(epoch));
            }
            assert_eq!(app.display_document(), &shown);
            app.display_document().validate().unwrap();
            let trial = app
                .tab
                .equipment_authoring
                .as_ref()
                .unwrap()
                .trial
                .as_ref()
                .unwrap();
            assert_eq!((trial.at, trial.turn, trial.scale, trial.reach), pose);
            let restored = trial.snap.as_ref().unwrap();
            assert!(matches!(restored.offer, Offer::Adapter(_)));
            assert_eq!(restored.added, snap.added);
            assert_eq!(restored.target.at, snap.target.at);
            assert_eq!(restored.moving.at, snap.moving.at);
            assert_eq!(
                super::super::super::equipment::snap_text(app.display_document(), restored),
                feedback
            );
            assert_eq!(app.tab.doc, before);
            assert_eq!(app.tab.revision, revision);
            assert!(!app.tab.history.can_undo());
        }
        send(&mut app, Action::Accept);
        assert_eq!(app.tab.doc, canonical);
        assert!(app.tab.doc.seats.is_empty());
        assert_eq!(app.tab.history.undo_frames().len(), 1);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
    }
}

#[test]
fn equipment_trial_pose_is_retained_but_accept_save_and_undo_never_keep_trial_objects() {
    for discard in [false, true] {
        let (mut app, edges) = joint_drawing();
        let before = app.tab.doc.clone();
        begin(&mut app, edges);
        let canonical = app
            .tab
            .equipment_authoring
            .as_ref()
            .unwrap()
            .base_preview
            .clone()
            .unwrap();
        let id = target(&app);
        toward(&mut app, id, true);
        let state = app.tab.equipment_authoring.as_ref().unwrap();
        let trial = state.trial.as_ref().unwrap();
        let shown = state.preview.as_ref().unwrap();
        let test = shown
            .graphics
            .iter()
            .find(|g| Some(g.id) == trial.id)
            .unwrap();
        assert!(trial.at.distance(placed::middle(test)) < 1e-4);
        assert!((trial.scale - test.axis_x.x.hypot(test.axis_x.y)).abs() < 1e-5);
        send(
            &mut app,
            if discard {
                Action::Discard
            } else {
                Action::Accept
            },
        );
        assert!(app.tab.equipment_authoring.is_none());
        if discard {
            assert_eq!(app.tab.doc, before);
            assert!(!app.tab.history.can_undo());
        } else {
            assert_eq!(app.tab.doc, canonical);
            assert_eq!(app.tab.doc.graphics.len(), 1);
            assert_eq!(app.tab.doc.piece_definitions.len(), 1);
            assert!(app.tab.doc.seats.is_empty());
            let bytes = app.tab.doc.file_json().unwrap();
            assert!(
                !String::from_utf8(bytes.clone())
                    .unwrap()
                    .contains("reshiki/stopper")
            );
            assert_eq!(Document::from_json(&bytes).unwrap(), app.tab.doc.current());
            let accepted = app.tab.doc.clone();
            let _ = app.update(Message::Undo);
            assert_eq!(app.tab.doc, before);
            let _ = app.update(Message::Redo);
            assert_eq!(app.tab.doc, accepted);
        }
    }
}

#[test]
fn adding_a_joint_while_testing_returns_the_canvas_to_rim_measurement() {
    let (mut app, edges) = joint_drawing();
    let original = app.tab.doc.clone();
    begin(&mut app, edges);
    let id = target(&app);
    let _ = toward(&mut app, id, false);

    send(&mut app, Action::BeginJoint);
    send(&mut app, Action::TestSnapping); // Ignore an older queued button press.
    let state = app.tab.equipment_authoring.as_ref().unwrap();
    assert!(state.trial.is_none());
    assert!(state.measurement().measuring);
    assert!(state.measurement().points.is_empty());
    assert_eq!(app.display_document(), state.base_preview.as_ref().unwrap());

    for point in edges {
        let _ = app.update(Message::Canvas(Edit::EquipmentAuthoringPoint(point)));
    }
    let state = app.tab.equipment_authoring.as_ref().unwrap();
    assert_eq!(state.measurement().points, &edges);
    send(&mut app, Action::CancelJoint);
    send(&mut app, Action::TestSnapping);
    assert!(
        app.tab
            .equipment_authoring
            .as_ref()
            .unwrap()
            .trial
            .is_some()
    );
    assert_eq!(app.tab.doc, original);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn equipment_trial_draft_history_and_invalid_joints_remove_stale_paint() {
    let (mut app, edges) = joint_drawing();
    let before = app.tab.doc.clone();
    begin(&mut app, edges);
    let id = target(&app);
    let at = toward(&mut app, id, false);
    let old_epoch = app
        .tab
        .equipment_authoring
        .as_ref()
        .unwrap()
        .trial
        .as_ref()
        .unwrap()
        .epoch;
    let _ = app.update(Message::Undo); // pending joint again
    send(&mut app, Action::FlipJoint);
    let state = app.tab.equipment_authoring.as_ref().unwrap();
    assert!(state.trial.is_none());
    assert!(state.measurement().measuring);
    assert!(state.preview.is_none() && state.base_preview.is_none());
    assert!(!state.ready());
    send(
        &mut app,
        Action::TestAt {
            at,
            epoch: old_epoch,
            reach: 14.,
            finish: true,
        },
    );
    send(&mut app, Action::TestCancel(old_epoch));
    assert_eq!(app.display_document(), &before);
    let _ = app.update(Message::Undo); // valid pending joint
    assert!(
        app.tab
            .equipment_authoring
            .as_ref()
            .unwrap()
            .preview
            .is_some()
    );
    let _ = app.update(Message::Redo); // invalid flipped joint
    assert_eq!(app.display_document(), &before);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn equipment_trial_can_test_a_grounded_piece_already_in_the_drawing() {
    let (mut app, edges) = joint_drawing();
    let mut definition = (*builtin::find("reshiki/rbf-50-15").unwrap()).clone();
    definition.id = "lab/grounded-target".into();
    definition.grounded = true;
    let definition = Arc::new(definition);
    let fixed_id = app.tab.doc.next_id();
    let fixed = placed::graphic(fixed_id, definition.clone(), Point::new(250., 0.), 1.);
    app.tab.doc.graphics.push(fixed.clone());
    app.tab.doc.piece_definitions.push(definition);
    let before = app.tab.doc.clone();
    begin(&mut app, edges);
    toward(&mut app, fixed_id, true);
    let state = app.tab.equipment_authoring.as_ref().unwrap();
    assert_eq!(
        state
            .trial
            .as_ref()
            .unwrap()
            .snap
            .as_ref()
            .unwrap()
            .target
            .at
            .piece,
        fixed_id
    );
    let shown = app
        .display_document()
        .graphics
        .iter()
        .find(|g| g.id == fixed_id)
        .unwrap();
    assert_eq!(
        (shown.origin, shown.axis_x, shown.axis_y),
        (fixed.origin, fixed.axis_x, fixed.axis_y)
    );
    assert_eq!(app.tab.doc, before);
    send(&mut app, Action::Discard);
    assert_eq!(app.tab.doc, before);
}
