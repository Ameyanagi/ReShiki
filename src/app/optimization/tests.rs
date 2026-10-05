use super::*;
use reshiki::document::Point;

fn drawing() -> (App, u64, u64, Document) {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let a = app.tab.doc.add_atom("C", Point::default());
    let b = app.tab.doc.add_atom("C", Point::new(42., 0.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    app.tab.doc.add_atom("O", Point::new(180., 80.));
    app.tab.selected = vec![a];
    app.tab.saved = app.tab.doc.clone();
    let original = app.tab.doc.clone();
    (app, a, b, original)
}

fn result(session: &Session) -> Optimized {
    let conformer = session.effective_conformer().unwrap_or(Conformer {
        positions: vec![
            Point3::default(),
            Point3 {
                x: 1.5,
                y: 0.,
                z: 0.4,
            },
        ],
        original_atom_count: 2,
        hydrogen_parents: vec![],
    });
    Optimized {
        conformer,
        initial_energy: 2.,
        energy: 1.,
        gradient: None,
        converged: true,
        iterations: 20,
        force_field: session.field,
        diagnostics: vec![],
    }
}

fn finish(app: &mut App) {
    let session = app.tab.optimization.as_ref().unwrap();
    let key = session.flight.as_ref().unwrap().key;
    let result = result(session);
    let _ = app.optimization_action(Action::WorkerDone(key, Ok(Arc::new(result))));
}

fn prepared_preview() -> (App, u64, u64, Document) {
    let (mut app, a, b, original) = drawing();
    let _ = app.optimization_action(Action::Begin);
    finish(&mut app);
    (app, a, b, original)
}

#[test]
fn initialization_context_survives_relaxation_without_an_error_or_source_edit() {
    for (diagnostic, label) in [
        ("initialization=existing-3d", "Existing 3D geometry"),
        ("initialization=cage-ETDG", "Cage starting geometry"),
        (
            "initialization=ETDG-fallback",
            "Alternative starting geometry",
        ),
        ("initialization=single-conformer", "Single conformer"),
    ] {
        let (mut app, _, _, original) = drawing();
        let _ = app.optimization_action(Action::Begin);
        let session = app.tab.optimization.as_ref().unwrap();
        let key = session.flight.as_ref().unwrap().key;
        let mut computed = result(session);
        // Only the final initialization is shown after a recovered retry.
        computed.diagnostics = vec![
            "initialization=single-conformer; earlier retry".into(),
            diagnostic.into(),
        ];
        let _ = app.optimization_action(Action::WorkerDone(key, Ok(Arc::new(computed))));
        let session = app.tab.optimization.as_ref().unwrap();
        assert_eq!(session.initialization, Some(label));
        assert!(session.notice.is_none());
        assert!(!app.error);
        assert_eq!(app.tab.doc, original);

        let _ = app.optimization_action(Action::Start);
        finish(&mut app);
        assert_eq!(
            app.tab.optimization.as_ref().unwrap().initialization,
            Some(label)
        );
        let _ = app.optimization_action(Action::Cancel);
        assert_eq!(app.tab.doc, original);
    }
}

fn complete_live(app: &mut App, energy: f64, converged: bool) -> Task<Message> {
    let session = app.tab.optimization.as_ref().unwrap();
    let flight = session.flight.as_ref().unwrap();
    assert_eq!(flight.work, Work::Relax);
    let key = flight.key;
    let mut computed = result(session);
    computed.initial_energy = energy + 1.;
    computed.energy = energy;
    computed.converged = converged;
    app.optimization_action(Action::WorkerDone(key, Ok(Arc::new(computed))))
}

fn stall_live(app: &mut App) {
    for _ in 0..=MAX_STAGNANT_BATCHES {
        let _ = complete_live(app, 1., false);
    }
    let session = app.tab.optimization.as_ref().unwrap();
    assert_eq!(session.relaxation.paused, Some(RelaxationPause::Stalled));
    assert!(session.flight.is_none());
}

fn preview_controls(running: bool) -> [(&'static str, Action); 17] {
    [
        (
            "optimization.field.mmff94",
            Action::Field(ForceField::Mmff94),
        ),
        (
            "optimization.field.mmff94s",
            Action::Field(ForceField::Mmff94s),
        ),
        ("optimization.field.uff", Action::Field(ForceField::Uff)),
        (
            "optimization.run",
            if running { Action::Stop } else { Action::Start },
        ),
        ("optimization.pin-selected", Action::PinSelected),
        ("optimization.unpin-selected", Action::UnpinSelected),
        ("optimization.clear-pins", Action::ClearPins),
        ("optimization.rotate-left", Action::Rotate(0., -15.)),
        ("optimization.rotate-right", Action::Rotate(0., 15.)),
        ("optimization.tilt-up", Action::Rotate(15., 0.)),
        ("optimization.tilt-down", Action::Rotate(-15., 0.)),
        ("optimization.roll", Action::Roll(15.)),
        ("optimization.show-original", Action::ShowOriginal(true)),
        (
            "optimization.automatic-depth",
            Action::DepthEnhancement(false),
        ),
        ("optimization.clear-depth", Action::ClearDepthAppearance),
        ("optimization.cancel", Action::Cancel),
        ("optimization.apply", Action::Apply),
    ]
}

#[test]
fn preview_and_cancellation_leave_document_revision_history_and_redo_untouched() {
    let (mut app, _, _, original) = drawing();
    let mut later = original.clone();
    later.add_atom("F", Point::new(240., 100.));
    app.tab.history.commit(original.clone(), &later);
    app.tab.doc = later;
    assert!(app.tab.history.undo(&mut app.tab.doc));
    let revision = app.tab.revision;
    let _ = app.optimization_action(Action::Begin);
    finish(&mut app);
    assert_eq!(app.tab.doc, original);
    assert_eq!(app.tab.revision, revision);
    assert!(app.tab.history.can_redo());
    let _ = app.optimization_action(Action::Rotate(25., 40.));
    let _ = app.optimization_action(Action::Cancel);
    assert_eq!(app.tab.doc, original);
    assert_eq!(app.tab.revision, revision);
    assert!(app.tab.history.can_redo());
    assert!(!app.tab.history.can_undo());
}

#[test]
fn apply_validates_then_commits_once_with_exact_undo_and_redo() {
    let (mut app, _, _, original) = prepared_preview();
    let unrelated = original.atoms.last().unwrap().clone();
    let _ = app.optimization_action(Action::Rotate(20., 35.));
    let _ = app.optimization_action(Action::Apply);
    assert_eq!(app.tab.doc, original, "Application waits for validation");
    finish(&mut app);
    assert!(app.tab.optimization.is_none());
    let applied = app.tab.doc.clone();
    assert_ne!(applied, original);
    assert_eq!(applied.atom(unrelated.id), Some(&unrelated));
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    assert!(!app.tab.history.can_undo());
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, applied);
}

#[test]
fn applying_after_live_drag_stop_and_depth_freeze_retains_pins_and_is_one_edit() {
    let (mut app, pinned, dragged, original) = prepared_preview();
    let _ = app.optimization_action(Action::Rotate(25., -35.));
    let _ = app.optimization_action(Action::PinSelected);
    let session = app.tab.optimization.as_ref().unwrap();
    let pin = session.pins[&pinned];
    let id = session.id;
    let target = session
        .preview
        .atom(dragged)
        .unwrap()
        .position
        .offset(2., 1.);
    let _ = app.optimization_action(Action::Start);
    let _ = app.optimization_edit(Edit::RelaxDragStart {
        session: id,
        atom: dragged,
    });
    let _ = app.optimization_edit(Edit::RelaxDragTarget {
        session: id,
        atom: dragged,
        target,
    });
    let _ = app.optimization_edit(Edit::RelaxDragEnd {
        session: id,
        atom: dragged,
        target: Some(target),
    });
    let _ = app.optimization_action(Action::Stop);
    let _ = app.optimization_action(Action::DepthEnhancement(false));
    let session = app.tab.optimization.as_ref().unwrap();
    assert_eq!(session.pins.get(&pinned), Some(&pin));
    assert!(session.flight.is_none());
    let coordinates = session.effective_conformer().unwrap().positions;
    let frozen = session.preview.depth_appearance.clone();
    assert!(!frozen.is_empty());
    assert!(frozen.iter().all(|scope| !scope.automatic));
    let projected_pin = session.frame.as_ref().unwrap().project(pin);
    let _ = app.optimization_action(Action::Apply);
    let session = app.tab.optimization.as_ref().unwrap();
    assert_eq!(session.phase, Phase::Applying);
    assert_eq!(
        session.effective_conformer().unwrap().positions,
        coordinates
    );
    assert_eq!(session.pins.get(&pinned), Some(&pin));
    let flight = session.flight.as_ref().unwrap();
    assert_eq!(flight.work, Work::Apply);
    assert_eq!(flight.pins.len(), 1);
    assert_eq!(flight.pins[0].atom, session.prepared.index(pinned).unwrap());
    assert_eq!(flight.pins[0].position, pin);
    assert_eq!(app.tab.doc, original, "Apply waits for its energy check");
    assert!(!app.tab.history.can_undo());
    finish(&mut app);
    assert!(app.tab.optimization.is_none());
    assert!(!app.error);
    let applied = app.tab.doc.clone();
    assert_eq!(applied.depth_appearance, frozen);
    assert_eq!(applied.atom(pinned).unwrap().position, projected_pin.0);
    assert_eq!(applied.atom(pinned).unwrap().depth, projected_pin.1);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    assert!(!app.tab.history.can_undo());
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, applied);
}

#[test]
fn stale_completions_never_clear_a_new_sessions_inflight_task_or_status() {
    let (mut app, _, _, _) = drawing();
    let _ = app.optimization_action(Action::Begin);
    let old = app
        .tab
        .optimization
        .as_ref()
        .unwrap()
        .flight
        .as_ref()
        .unwrap()
        .key;
    let _ = app.optimization_action(Action::Cancel);
    let _ = app.optimization_action(Action::Begin);
    let new = app
        .tab
        .optimization
        .as_ref()
        .unwrap()
        .flight
        .as_ref()
        .unwrap()
        .key;
    let status = app.status.clone();
    let _ = app.optimization_action(Action::WorkerDone(old, Err("Old failed request".into())));
    assert_eq!(app.status, status);
    assert!(!app.error);
    assert_eq!(
        app.tab
            .optimization
            .as_ref()
            .unwrap()
            .flight
            .as_ref()
            .unwrap()
            .key,
        new
    );
    let mut bad = new;
    bad.constraints = bad.constraints.wrapping_add(1);
    let _ = app.optimization_action(Action::WorkerDone(bad, Err("Old constraints".into())));
    assert_eq!(app.status, status);
    assert!(app.tab.optimization.as_ref().unwrap().flight.is_some());
    app.pause_optimization();
    let _ = app.optimization_action(Action::WorkerDone(new, Err("Cancelled request".into())));
    assert!(app.tab.optimization.as_ref().unwrap().flight.is_none());
    assert!(!app.error);
}

#[test]
fn view_rotation_preserves_physical_pins_coordinates_and_energy_and_depth_can_freeze() {
    let (mut app, a, _, _) = prepared_preview();
    let session = app.tab.optimization.as_ref().unwrap();
    let pins = session.pins.clone();
    let coordinates = session.conformer.as_ref().unwrap().positions.clone();
    let energy = session.energy;
    assert!(pins.contains_key(&a));
    let _ = app.optimization_action(Action::Rotate(40., -30.));
    let session = app.tab.optimization.as_ref().unwrap();
    assert_eq!(session.pins, pins);
    assert_eq!(session.conformer.as_ref().unwrap().positions, coordinates);
    assert_eq!(session.energy, energy);
    let _ = app.optimization_action(Action::DepthEnhancement(false));
    let weights = app
        .tab
        .optimization
        .as_ref()
        .unwrap()
        .preview
        .depth_appearance
        .clone();
    assert!(weights.iter().all(|scope| !scope.automatic));
    let _ = app.optimization_action(Action::Rotate(35., 15.));
    assert_eq!(
        app.tab
            .optimization
            .as_ref()
            .unwrap()
            .preview
            .depth_appearance,
        weights
    );
    let _ = app.optimization_action(Action::ClearDepthAppearance);
    assert!(
        app.tab
            .optimization
            .as_ref()
            .unwrap()
            .preview
            .depth_appearance
            .is_empty()
    );
}

#[test]
fn exact_pin_violation_is_rejected_without_replacing_the_preview() {
    for action in [Action::Start, Action::Apply] {
        let (mut app, a, _, original) = prepared_preview();
        let preview = app.tab.optimization.as_ref().unwrap().preview.clone();
        let _ = app.optimization_action(action);
        let session = app.tab.optimization.as_ref().unwrap();
        let key = session.flight.as_ref().unwrap().key;
        let mut invalid = result(session);
        invalid.conformer.positions[session.prepared.index(a).unwrap()].x += 0.01;
        let _ = app.optimization_action(Action::WorkerDone(key, Ok(Arc::new(invalid))));
        assert_eq!(app.tab.doc, original);
        assert_eq!(app.tab.optimization.as_ref().unwrap().preview, preview);
        assert_eq!(app.tab.optimization.as_ref().unwrap().phase, Phase::Failed);
        assert!(!app.tab.history.can_undo());
        assert!(app.error);
    }
}

#[test]
fn pointer_targets_are_coalesced_behind_one_flight_and_use_inverse_view_projection() {
    let (mut app, a, _, _) = prepared_preview();
    let _ = app.optimization_action(Action::ClearPins);
    let _ = app.optimization_action(Action::Rotate(25., 40.));
    let id = app.tab.optimization.as_ref().unwrap().id;
    let _ = app.optimization_edit(Edit::RelaxDragStart {
        session: id,
        atom: a,
    });
    let _ = app.optimization_edit(Edit::RelaxDragTarget {
        session: id,
        atom: a,
        target: Point::new(10., 20.),
    });
    let _ = app.optimization_action(Action::Start);
    let first = app
        .tab
        .optimization
        .as_ref()
        .unwrap()
        .flight
        .as_ref()
        .unwrap()
        .key;
    for i in 0..100 {
        let _ = app.optimization_edit(Edit::RelaxDragTarget {
            session: id,
            atom: a,
            target: Point::new(i as f32, 35.),
        });
    }
    let session = app.tab.optimization.as_ref().unwrap();
    assert_eq!(session.flight.as_ref().unwrap().key, first);
    assert_eq!(session.sequence, first.sequence);
    assert_eq!(session.constraints, first.constraints);
    assert_eq!(session.constraints().len(), 1);
    let drag = session.drag.unwrap();
    let (point, depth) = session.frame.as_ref().unwrap().project(drag.target);
    assert!(point.distance(Point::new(99., 35.)) < 0.001);
    assert!((depth - drag.depth).abs() < 0.001);
}

#[test]
fn cancelling_a_drag_restores_its_geometry_and_discards_a_late_fixed_target() {
    let (mut app, a, _, original_document) = prepared_preview();
    let _ = app.optimization_action(Action::ClearPins);
    let session = app.tab.optimization.as_ref().unwrap();
    let before = session.conformer.as_ref().unwrap().positions.clone();
    let energy = session.energy;
    let id = session.id;
    let _ = app.optimization_edit(Edit::RelaxDragStart {
        session: id,
        atom: a,
    });
    let _ = app.optimization_edit(Edit::RelaxDragTarget {
        session: id,
        atom: a,
        target: Point::new(20., 35.),
    });
    let _ = app.optimization_action(Action::Start);
    let key = app
        .tab
        .optimization
        .as_ref()
        .unwrap()
        .flight
        .as_ref()
        .unwrap()
        .key;
    let _ = app.optimization_edit(Edit::RelaxDragCancel { session: id });
    let session = app.tab.optimization.as_ref().unwrap();
    assert_eq!(session.conformer.as_ref().unwrap().positions, before);
    assert_eq!(session.energy, energy);
    assert!(session.drag.is_none());
    assert!(session.flight.is_none());
    let _ = app.optimization_action(Action::WorkerDone(key, Err("Cancelled drag".into())));
    assert_eq!(app.tab.doc, original_document);
    assert!(!app.error);
}

#[test]
fn whole_molecule_generation_starts_unpinned_and_partial_selection_is_fixed() {
    let (mut whole, a, b, _) = drawing();
    whole.tab.selected = vec![a, b];
    let _ = whole.optimization_action(Action::Begin);
    finish(&mut whole);
    assert!(whole.tab.optimization.as_ref().unwrap().pins.is_empty());
    let (partial, a, _, _) = prepared_preview();
    let session = partial.tab.optimization.as_ref().unwrap();
    assert_eq!(session.pin_ids, vec![a]);
    let index = session.prepared.index(a).unwrap();
    assert_eq!(
        session.pins.get(&a),
        session.conformer.as_ref().unwrap().positions.get(index)
    );
}

#[test]
fn topology_edits_are_blocked_before_they_can_modify_the_committed_drawing() {
    let (mut app, a, _, original) = prepared_preview();
    app.tab.selected = vec![a];
    let _ = app.update(Message::Charge(1));
    assert_eq!(app.tab.doc, original);
    assert!(app.tab.optimization.is_some());
    assert!(!app.tab.history.can_undo());
}

#[test]
fn switching_and_closing_tabs_pause_or_discard_their_tagged_results() {
    let (mut app, _, _, original) = prepared_preview();
    let id = app.tab.id;
    let _ = app.optimization_action(Action::Start);
    let session = app.tab.optimization.as_ref().unwrap();
    let key = session.flight.as_ref().unwrap().key;
    let computed = Arc::new(result(session));
    app.add_tab();
    let front = app.tab.doc.clone();
    let _ = app.update(Message::Tab(
        id,
        Box::new(Message::Optimization(Action::WorkerDone(
            key,
            Ok(computed.clone()),
        ))),
    ));
    assert_eq!(app.tab.doc, front);
    let parked = app.tabs.background.iter().find(|tab| tab.id == id).unwrap();
    assert_eq!(parked.doc, original);
    assert!(parked.optimization.as_ref().unwrap().flight.is_none());
    let _ = app.update(Message::Tabs(super::super::tabs::Action::Select(id)));
    assert!(app.tab.optimization.is_some());
    let _ = app.close_active_tab();
    assert!(app.tab_index(id).is_none());
    let front = app.tab.doc.clone();
    let _ = app.update(Message::Tab(
        id,
        Box::new(Message::Optimization(Action::WorkerDone(key, Ok(computed)))),
    ));
    assert_eq!(app.tab.doc, front);
    assert!(app.strip().all(|tab| tab.optimization.is_none()));
}

#[test]
fn improving_nonconverged_batches_stop_at_the_budget_and_apply_is_one_undoable_edit() {
    let (mut app, _, _, original) = prepared_preview();
    let _ = app.optimization_action(Action::ClearPins);
    let _ = app.optimization_action(Action::Start);
    let running_context = app
        .tab
        .optimization
        .as_ref()
        .unwrap()
        .accessibility_context();
    for batch in 1..=MAX_RELAXATION_BATCHES {
        let task = complete_live(&mut app, 1000. - f64::from(batch), false);
        let session = app.tab.optimization.as_ref().unwrap();
        assert_eq!(app.tab.doc, original);
        assert!(!app.tab.history.can_undo());
        if batch < MAX_RELAXATION_BATCHES {
            assert!(task.units() > 0);
            assert!(session.running);
            assert!(session.flight.is_some());
        } else {
            assert_eq!(task.units(), 0);
            assert!(!session.running);
            assert!(session.flight.is_none());
            assert_eq!(session.phase, Phase::Paused);
            assert_eq!(session.relaxation.paused, Some(RelaxationPause::Limit));
            assert_ne!(running_context, session.accessibility_context());
        }
    }
    assert!(app.status.contains("paused before convergence"));
    assert!(!app.error);
    let session = app.tab.optimization.as_mut().unwrap();
    assert_eq!(session.next().units(), 0);
    let preview = session.preview.clone();
    let _ = app.optimization_action(Action::Apply);
    assert_eq!(
        app.tab.optimization.as_ref().unwrap().phase,
        Phase::Applying
    );
    assert_eq!(app.tab.doc, original);
    finish(&mut app);
    assert!(app.tab.optimization.is_none());
    assert_eq!(app.tab.doc, preview);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    assert!(!app.tab.history.can_undo());
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, preview);
}

#[test]
fn stalled_nonconverged_batches_pause_after_multiple_batches_and_cancel_keeps_the_drawing() {
    let (mut app, _, _, original) = prepared_preview();
    let _ = app.optimization_action(Action::Start);
    assert!(complete_live(&mut app, 1., false).units() > 0);
    for _ in 1..MAX_STAGNANT_BATCHES {
        assert!(complete_live(&mut app, 1., false).units() > 0);
    }
    assert_eq!(complete_live(&mut app, 1., false).units(), 0);
    let session = app.tab.optimization.as_ref().unwrap();
    assert_eq!(session.phase, Phase::Paused);
    assert_eq!(session.relaxation.paused, Some(RelaxationPause::Stalled));
    assert_eq!(session.relaxation.batches, MAX_STAGNANT_BATCHES + 1);
    assert!(app.status.contains("no further energy improvement"));
    assert!(session.notice.is_none());
    assert!(!app.error);
    assert_eq!(app.tab.doc, original);
    let _ = app.optimization_action(Action::Cancel);
    assert!(app.tab.optimization.is_none());
    assert_eq!(app.tab.doc, original);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn small_cumulative_energy_improvements_keep_live_relaxation_active() {
    let (mut app, _, _, original) = prepared_preview();
    let _ = app.optimization_action(Action::Start);
    let _ = complete_live(&mut app, 1., false);
    for batch in 1..=2 * MAX_STAGNANT_BATCHES {
        // Each step is below the progress threshold, but two successive
        // steps make meaningful progress against the retained baseline.
        assert!(complete_live(&mut app, 1. - f64::from(batch) * 6e-7, false).units() > 0);
    }
    let session = app.tab.optimization.as_ref().unwrap();
    assert!(session.running);
    assert!(session.relaxation.paused.is_none());
    assert!(session.flight.is_some());
    let _ = app.optimization_action(Action::Stop);
    assert_eq!(app.tab.doc, original);
}

#[test]
fn real_pin_and_field_changes_resume_a_paused_budget_while_noop_edits_and_rotation_do_not() {
    for action in [
        Action::PinSelected,
        Action::UnpinSelected,
        Action::ClearPins,
        Action::Field(ForceField::Uff),
    ] {
        let (mut app, _, b, original) = prepared_preview();
        let _ = app.optimization_action(Action::Start);
        stall_live(&mut app);
        assert_eq!(app.optimization_action(Action::Rotate(15., 25.)).units(), 0);
        assert_eq!(
            app.optimization_action(Action::Field(ForceField::Mmff94s))
                .units(),
            0
        );
        assert_eq!(app.optimization_action(Action::PinSelected).units(), 0);
        let session = app.tab.optimization.as_ref().unwrap();
        assert!(!session.running);
        assert_eq!(session.relaxation.batches, MAX_STAGNANT_BATCHES + 1);
        if matches!(action, Action::PinSelected) {
            app.tab.selected = vec![b];
        }
        assert!(app.optimization_action(action).units() > 0);
        let session = app.tab.optimization.as_ref().unwrap();
        assert!(session.running);
        assert_eq!(session.phase, Phase::Running);
        assert_eq!(session.relaxation.batches, 1);
        assert_eq!(session.relaxation.stagnant_batches, 0);
        assert!(session.relaxation.best_energy.is_none());
        assert!(session.relaxation.paused.is_none());
        assert!(session.flight.is_some());
        assert_eq!(app.tab.doc, original);
    }
}

#[test]
fn changed_drag_targets_resume_and_an_older_flight_does_not_consume_the_new_budget() {
    let (mut app, a, _, original) = prepared_preview();
    let _ = app.optimization_action(Action::ClearPins);
    let _ = app.optimization_action(Action::Start);
    stall_live(&mut app);
    let id = app.tab.optimization.as_ref().unwrap().id;
    let _ = app.optimization_edit(Edit::RelaxDragStart {
        session: id,
        atom: a,
    });
    let session = app.tab.optimization.as_ref().unwrap();
    let key = session.flight.as_ref().unwrap().key;
    let mut older = result(session);
    older.converged = false;
    let _ = app.optimization_edit(Edit::RelaxDragTarget {
        session: id,
        atom: a,
        target: Point::new(10., 15.),
    });
    let session = app.tab.optimization.as_ref().unwrap();
    assert_eq!(session.relaxation.batches, 0);
    assert_eq!(session.flight.as_ref().unwrap().key, key);
    let task = app.optimization_action(Action::WorkerDone(key, Ok(Arc::new(older))));
    assert!(task.units() > 0);
    let session = app.tab.optimization.as_ref().unwrap();
    assert_eq!(session.relaxation.batches, 1);
    assert_eq!(session.relaxation.stagnant_batches, 0);
    assert!(session.relaxation.best_energy.is_none());
    stall_live(&mut app);
    let task = app
        .optimization_edit(Edit::RelaxDragTarget {
            session: id,
            atom: a,
            target: Point::new(20., 15.),
        })
        .unwrap();
    assert!(task.units() > 0);
    assert_eq!(app.tab.optimization.as_ref().unwrap().relaxation.batches, 1);
    let _ = app.optimization_action(Action::Stop);
    let _ = app.optimization_edit(Edit::RelaxDragStart {
        session: id,
        atom: a,
    });
    let task = app
        .optimization_edit(Edit::RelaxDragTarget {
            session: id,
            atom: a,
            target: Point::new(25., 15.),
        })
        .unwrap();
    assert_eq!(task.units(), 0);
    let session = app.tab.optimization.as_ref().unwrap();
    assert!(!session.running);
    assert!(session.flight.is_none());
    assert_eq!(app.tab.doc, original);
}

#[test]
fn stop_retains_visual_drag_targets_without_dispatch_and_start_uses_the_latest_target() {
    let (mut app, a, _, original) = prepared_preview();
    let _ = app.optimization_action(Action::ClearPins);
    let _ = app.optimization_action(Action::Start);
    assert!(app.tab.optimization.as_ref().unwrap().flight.is_some());
    let _ = app.optimization_action(Action::Stop);
    let id = app.tab.optimization.as_ref().unwrap().id;
    let before = app.tab.optimization.as_ref().unwrap().preview.clone();
    let _ = app.optimization_edit(Edit::RelaxDragStart {
        session: id,
        atom: a,
    });
    for x in [5., 15., 25.] {
        let task = app
            .optimization_edit(Edit::RelaxDragTarget {
                session: id,
                atom: a,
                target: Point::new(x, 15.),
            })
            .unwrap();
        assert_eq!(task.units(), 0);
    }
    let task = app
        .optimization_edit(Edit::RelaxDragEnd {
            session: id,
            atom: a,
            target: Some(Point::new(25., 15.)),
        })
        .unwrap();
    assert_eq!(task.units(), 0);
    let session = app.tab.optimization.as_ref().unwrap();
    assert!(!session.running);
    assert!(session.flight.is_none());
    assert!(session.pending);
    assert_ne!(session.preview, before);
    assert_eq!(app.tab.doc, original);
    let target = session.drag.unwrap().target;
    let task = app.optimization_action(Action::Start);
    assert!(task.units() > 0);
    let session = app.tab.optimization.as_ref().unwrap();
    let flight = session.flight.as_ref().unwrap();
    assert_eq!(flight.drag, Some((a, target)));
    assert_eq!(flight.pins.len(), 1);
    assert_eq!(flight.pins[0].position, target);
}

#[test]
fn a_converged_running_session_idles_without_polling_and_relaxes_the_next_drag() {
    let (mut app, a, _, _) = prepared_preview();
    let _ = app.optimization_action(Action::ClearPins);
    let _ = app.optimization_action(Action::Start);
    finish(&mut app);
    let session = app.tab.optimization.as_mut().unwrap();
    assert!(session.running);
    assert!(session.flight.is_none());
    assert_eq!(session.next().units(), 0);
    let id = session.id;
    let _ = app.optimization_edit(Edit::RelaxDragStart {
        session: id,
        atom: a,
    });
    let task = app
        .optimization_edit(Edit::RelaxDragTarget {
            session: id,
            atom: a,
            target: Point::new(10., 15.),
        })
        .unwrap();
    assert!(task.units() > 0);
    let session = app.tab.optimization.as_ref().unwrap();
    assert!(session.flight.is_some());
    assert_eq!(session.relaxation.batches, 1);
    assert_eq!(session.relaxation.stagnant_batches, 0);
}

#[test]
fn switching_between_tilt_and_select_preserves_the_preview_and_session() {
    let (mut app, _, _, original) = prepared_preview();
    let id = app.tab.optimization.as_ref().unwrap().id;
    let _ = app.update(Message::Tool(Tool::Tilt));
    assert_eq!(app.tool, Tool::Tilt);
    let _ = app.update(Message::Canvas(Edit::RelaxRotate {
        session: id,
        x: 25.,
        y: 35.,
    }));
    let rotated = app.tab.optimization.as_ref().unwrap().preview.clone();
    let _ = app.update(Message::Tool(Tool::Select));
    assert_eq!(app.tool, Tool::Select);
    let session = app.tab.optimization.as_ref().unwrap();
    assert_eq!(session.id, id);
    assert_eq!(session.preview, rotated);
    assert_eq!(app.tab.doc, original);
    assert!(!app.tab.history.can_undo());
    let _ = app.update(Message::Escape);
    assert!(app.tab.optimization.is_none());
    assert_eq!(app.tab.doc, original);
}

#[test]
fn accessibility_context_changes_for_controls_but_not_projection_or_energy_frames() {
    let (mut app, _, _, _) = prepared_preview();
    let before = app
        .tab
        .optimization
        .as_ref()
        .unwrap()
        .accessibility_context();
    let _ = app.optimization_action(Action::Rotate(25., 35.));
    let session = app.tab.optimization.as_mut().unwrap();
    session.energy = Some(99.);
    session.iterations += 20;
    assert_eq!(before, session.accessibility_context());
    for action in [
        Action::ShowOriginal(true),
        Action::DepthEnhancement(false),
        Action::Field(ForceField::Uff),
        Action::ClearPins,
        Action::Start,
        Action::Stop,
    ] {
        let before = app
            .tab
            .optimization
            .as_ref()
            .unwrap()
            .accessibility_context();
        let _ = app.optimization_action(action);
        assert_ne!(
            before,
            app.tab
                .optimization
                .as_ref()
                .unwrap()
                .accessibility_context()
        );
    }
    let before = app
        .tab
        .optimization
        .as_ref()
        .unwrap()
        .accessibility_context();
    let _ = app.optimization_action(Action::Cancel);
    let _ = app.optimization_action(Action::Begin);
    assert_ne!(
        before,
        app.tab
            .optimization
            .as_ref()
            .unwrap()
            .accessibility_context()
    );
}

struct PreviewUi {
    renderer: iced::Renderer,
    cache: iced_runtime::user_interface::Cache,
    size: iced::Size,
}

impl PreviewUi {
    async fn new(size: iced::Size) -> Self {
        use iced::advanced::renderer::Headless;
        Self {
            renderer: <iced::Renderer as Headless>::new(
                iced::Font::with_name(reshiki::style::ui_font_family()),
                iced::Pixels(16.),
                None,
            )
            .await
            .unwrap(),
            cache: iced_runtime::user_interface::Cache::new(),
            size,
        }
    }

    fn operate<T: 'static>(
        &mut self,
        app: &App,
        operation: &mut dyn iced::advanced::widget::Operation<T>,
    ) {
        let mut ui = iced_runtime::UserInterface::build(
            app.view(),
            self.size,
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );
        ui.operate(
            &self.renderer,
            &mut iced::advanced::widget::operation::black_box(operation),
        );
        self.cache = ui.into_cache();
    }

    fn snapshot(&mut self, app: &App) -> reshiki::accessibility::Snapshot {
        let mut collect =
            reshiki::accessibility::Collect::new(iced::Rectangle::with_size(self.size));
        self.operate(app, &mut collect);
        let snapshot = collect.snapshot().clone();
        assert!(snapshot.duplicate_ids.is_empty());
        snapshot
    }

    fn focus(&mut self, app: &App, id: &str) {
        use iced::advanced::widget::{Operation, operation};
        let mut operation: Box<dyn Operation> =
            Box::new(reshiki::accessibility::FocusControl::new(id));
        loop {
            self.operate(app, operation.as_mut());
            match operation.finish() {
                operation::Outcome::Chain(next) => operation = next,
                _ => break,
            }
        }
    }

    fn event(
        &mut self,
        app: &App,
        event: iced::Event,
        cursor: iced::mouse::Cursor,
    ) -> (iced::event::Status, Vec<Message>) {
        let mut ui = iced_runtime::UserInterface::build(
            app.view(),
            self.size,
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );
        let mut messages = Vec::new();
        let (_, statuses) = ui.update(
            &[event],
            cursor,
            &mut self.renderer,
            &mut iced::advanced::clipboard::Null,
            &mut messages,
        );
        self.cache = ui.into_cache();
        (statuses[0], messages)
    }

    fn settle(&mut self, app: &mut App) {
        for _ in 0..3 {
            let (_, messages) = self.event(
                app,
                iced::Event::Window(iced::window::Event::RedrawRequested(
                    std::time::Instant::now(),
                )),
                iced::mouse::Cursor::Unavailable,
            );
            if messages.is_empty() {
                break;
            }
            for message in messages {
                let _ = app.update(message);
            }
        }
    }

    fn canvas_bounds(&self, app: &App) -> iced::Rectangle {
        use iced::advanced::{Layout, layout, widget::Tree};
        fn canvas(layout: Layout<'_>, size: iced::Size) -> Option<iced::Rectangle> {
            if layout.bounds().size() == size && layout.children().next().is_none() {
                return Some(layout.bounds());
            }
            layout.children().find_map(|child| canvas(child, size))
        }
        let mut view = app.view();
        let mut tree = Tree::new(view.as_widget());
        let node = view.as_widget_mut().layout(
            &mut tree,
            &self.renderer,
            &layout::Limits::new(self.size, self.size),
        );
        canvas(Layout::new(&node), app.viewport).expect("actual canvas layout")
    }
}

fn preview_key(named: iced::keyboard::key::Named, repeat: bool) -> iced::Event {
    use iced::keyboard::{
        self, Key, Modifiers,
        key::{Code, Named, Physical},
    };
    iced::Event::Keyboard(keyboard::Event::KeyPressed {
        key: Key::Named(named),
        modified_key: Key::Named(named),
        physical_key: Physical::Code(if named == Named::Tab {
            Code::Tab
        } else {
            Code::Enter
        }),
        location: keyboard::Location::Standard,
        modifiers: Modifiers::empty(),
        text: None,
        repeat,
    })
}

#[test]
fn beginning_or_reopening_preview_reveals_properties_without_moving_the_molecule() {
    for (open, tab) in [
        (false, InspectorTab::Properties),
        (true, InspectorTab::Assistant),
        (true, InspectorTab::Templates),
    ] {
        let (mut app, a, _, original) = drawing();
        app.inspector_open = open;
        app.inspector_tab = tab;
        app.tab.camera.zoom = 1.5;
        app.viewport = iced::Size::new(1000. - app.inspector_width(), 600.);
        let at = app.tab.doc.atom(a).unwrap().position;
        let screen = app
            .tab
            .camera
            .screen(at, iced::Rectangle::with_size(app.viewport));
        let _ = app.optimization_action(Action::Begin);
        assert!(app.inspector_open);
        assert_eq!(app.inspector_tab, InspectorTab::Properties);
        assert_eq!(
            app.tab
                .camera
                .screen(at, iced::Rectangle::with_size(app.viewport)),
            screen
        );
        assert_eq!(app.tab.doc, original);
        let _ = app.update(Message::Inspector(InspectorTab::Templates));
        assert_eq!(app.inspector_tab, InspectorTab::Templates);
        let _ = app.update(Message::Inspector(InspectorTab::Properties));
        assert_eq!(app.inspector_tab, InspectorTab::Properties);
        assert!(app.tab.optimization.is_some());
    }
}

#[tokio::test]
#[ignore = "Opt-in real renderer semantics and keyboard checks"]
async fn preview_controls_publish_native_actions_and_tab_enter_dispatches_the_same_actions() {
    use iced::advanced::widget::{Operation, operation};
    use iced::keyboard::key::Named;
    use iced::{Rectangle, Size, mouse};
    use reshiki::accessibility::{
        Activate, Role,
        tree::{NativeTree, Request},
    };
    for size in [Size::new(1040., 680.), Size::new(1280., 820.)] {
        let (mut app, _, _, _) = prepared_preview();
        let mut ui = PreviewUi::new(size).await;
        ui.settle(&mut app);
        let snapshot = ui.snapshot(&app);
        let controls: Vec<_> = snapshot
            .nodes
            .iter()
            .filter(|node| node.id.starts_with("optimization."))
            .collect();
        assert_eq!(controls.len(), 17);
        for (id, checked) in [
            ("optimization.field.mmff94", false),
            ("optimization.field.mmff94s", true),
            ("optimization.field.uff", false),
            ("optimization.show-original", false),
            ("optimization.automatic-depth", true),
        ] {
            let node = controls.iter().find(|node| node.id == id).unwrap();
            assert_eq!(node.role, Role::ToggleButton);
            assert_eq!(node.checked, Some(checked));
        }
        let mut native = NativeTree::default();
        let tree = native
            .update(&snapshot, "ReShiki", Rectangle::with_size(size), 2.)
            .unwrap();
        ui.focus(&app, "optimization.field.mmff94");
        for (index, (id, expected)) in preview_controls(false).into_iter().enumerate() {
            if index > 0 {
                let (status, messages) = ui.event(
                    &app,
                    preview_key(Named::Tab, false),
                    mouse::Cursor::Unavailable,
                );
                assert_eq!(status, iced::event::Status::Captured);
                assert!(
                    !messages
                        .iter()
                        .any(|message| matches!(message, Message::Optimization(_)))
                );
            }
            let current = ui.snapshot(&app);
            let focused: Vec<_> = current.nodes.iter().filter(|node| node.focused).collect();
            assert_eq!(focused.len(), 1);
            assert_eq!(focused[0].id, id);
            assert!(
                focused[0].visible_bounds.is_some(),
                "Tab reveals the scrolled control {id}"
            );
            let native_id = tree
                .nodes
                .iter()
                .find(|(_, node)| node.author_id() == Some(id))
                .unwrap()
                .0;
            assert_eq!(
                native.resolve(&accesskit::ActionRequest {
                    action: accesskit::Action::Click,
                    target_tree: accesskit::TreeId::ROOT,
                    target_node: native_id,
                    data: None,
                }),
                Some(Request::Activate(id.into()))
            );
            let mut activate = Activate::<Message>::new(id);
            ui.operate(&app, &mut activate);
            assert!(
                matches!(activate.finish(), operation::Outcome::Some(Message::Optimization(actual)) if format!("{actual:?}") == format!("{expected:?}"))
            );
            for repeat in [false, true] {
                let (status, messages) = ui.event(
                    &app,
                    preview_key(Named::Enter, repeat),
                    mouse::Cursor::Unavailable,
                );
                assert_eq!(status, iced::event::Status::Captured);
                let actions: Vec<_> = messages
                    .iter()
                    .filter_map(|message| {
                        if let Message::Optimization(action) = message {
                            Some(format!("{action:?}"))
                        } else {
                            None
                        }
                    })
                    .collect();
                if repeat {
                    assert!(actions.is_empty());
                } else {
                    assert_eq!(actions, vec![format!("{expected:?}")]);
                }
            }
        }
    }
    let (mut app, _, _, _) = drawing();
    let _ = app.optimization_action(Action::Begin);
    let mut ui = PreviewUi::new(Size::new(1040., 680.)).await;
    ui.settle(&mut app);
    let snapshot = ui.snapshot(&app);
    for id in [
        "optimization.run",
        "optimization.apply",
        "optimization.pin-selected",
        "optimization.rotate-left",
    ] {
        assert!(
            !snapshot
                .nodes
                .iter()
                .find(|node| node.id == id)
                .unwrap()
                .enabled,
            "Unavailable until generation finishes: {id}"
        );
    }
    finish(&mut app);
    app.tab.optimization.as_mut().unwrap().phase = Phase::Applying;
    let snapshot = ui.snapshot(&app);
    assert_eq!(
        snapshot
            .nodes
            .iter()
            .filter(|node| node.id.starts_with("optimization.") && node.enabled)
            .map(|node| node.id.as_str())
            .collect::<Vec<_>>(),
        vec!["optimization.cancel"]
    );
    for (id, _) in preview_controls(false) {
        if id == "optimization.cancel" {
            continue;
        }
        let mut activate = Activate::<Message>::new(id);
        ui.operate(&app, &mut activate);
        assert!(matches!(activate.finish(), operation::Outcome::None));
    }
}

#[tokio::test]
#[ignore = "Opt-in renderer right-panel layout, scrolling and pointer checks"]
async fn properties_preview_controls_fit_scroll_and_keep_canvas_toolbar_height() {
    use iced::{Event, Size, mouse};
    for size in [
        Size::new(1040., 480.),
        Size::new(1040., 680.),
        Size::new(1280., 820.),
    ] {
        let (mut app, _, _, original) = drawing();
        app.inspector_open = true;
        app.inspector_tab = InspectorTab::Properties;
        let mut ui = PreviewUi::new(size).await;
        ui.settle(&mut app);
        let before = ui.canvas_bounds(&app);
        let _ = app.optimization_action(Action::Begin);
        finish(&mut app);
        ui.settle(&mut app);
        let after = ui.canvas_bounds(&app);
        assert_eq!(
            before, after,
            "Preview keeps the ordinary 46 px canvas toolbar and viewport"
        );
        assert_eq!(app.inspector_width(), 300.);
        let snapshot = ui.snapshot(&app);
        let controls: Vec<_> = snapshot
            .nodes
            .iter()
            .filter(|node| node.id.starts_with("optimization."))
            .collect();
        assert_eq!(controls.len(), 17);
        for control in controls {
            assert!(
                control.bounds.x >= size.width - 300. - 0.5
                    && control.bounds.x + control.bounds.width <= size.width + 0.5,
                "{control:?} must fit the 300 px Properties panel"
            );
        }
        for running in [false, true] {
            if running {
                let _ = app.optimization_action(Action::Start);
            }
            for (id, expected) in preview_controls(running) {
                ui.focus(&app, id);
                let snapshot = ui.snapshot(&app);
                for footer in ["optimization.cancel", "optimization.apply"] {
                    let node = snapshot
                        .nodes
                        .iter()
                        .find(|node| node.id == footer)
                        .unwrap();
                    let visible = node.visible_bounds.expect("fixed footer is always visible");
                    assert!(
                        (visible.height - node.bounds.height).abs() < 0.5
                            && (visible.width - node.bounds.width).abs() < 0.5
                    );
                }
                let node = snapshot.nodes.iter().find(|node| node.id == id).unwrap();
                let visible = node
                    .visible_bounds
                    .expect("focus reveals the control through the real inspector scroller");
                assert!(
                    (visible.height - node.bounds.height).abs() < 0.5
                        && (visible.width - node.bounds.width).abs() < 0.5,
                    "Fully reveal {id}: {node:?}"
                );
                let cursor = mouse::Cursor::Available(visible.center());
                let mut actions = Vec::new();
                for event in [mouse::Event::ButtonPressed, mouse::Event::ButtonReleased] {
                    let (_, messages) =
                        ui.event(&app, Event::Mouse(event(mouse::Button::Left)), cursor);
                    actions.extend(messages.into_iter().filter_map(|message| {
                        if let Message::Optimization(action) = message {
                            Some(format!("{action:?}"))
                        } else {
                            None
                        }
                    }));
                }
                assert_eq!(
                    actions,
                    vec![format!("{expected:?}")],
                    "Click the real right-panel control {id}"
                );
            }
        }
        assert_eq!(app.tab.doc, original);
        assert!(!app.tab.history.can_undo());
        // The toolbar link restores the card after the inspector is hidden.
        app.inspector_open = false;
        ui.settle(&mut app);
        let snapshot = ui.snapshot(&app);
        let link = snapshot
            .nodes
            .iter()
            .find(|node| node.id == "context-3d-properties")
            .unwrap();
        let cursor = mouse::Cursor::Available(link.visible_bounds.unwrap().center());
        for event in [mouse::Event::ButtonPressed, mouse::Event::ButtonReleased] {
            let (_, messages) = ui.event(&app, Event::Mouse(event(mouse::Button::Left)), cursor);
            for message in messages {
                if matches!(message, Message::Inspector(InspectorTab::Properties)) {
                    let _ = app.update(message);
                }
            }
        }
        assert!(app.inspector_open);
        assert_eq!(app.inspector_tab, InspectorTab::Properties);
        assert!(app.tab.optimization.is_some());
    }
}
