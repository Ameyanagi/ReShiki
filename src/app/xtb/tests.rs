//! A run from Run to its finish, with the worker's events written by hand.
use super::run::{Event, Report};
use super::*;
use reshiki::document::Point;
use reshiki::geometry::Point3;
use reshiki::models3d::Display;
use std::sync::Arc;

const START: [[f64; 3]; 3] = [[0., 0., 0.], [0.96, 0., 0.], [-0.24, 0.93, 0.]];
const MOVED: [[f64; 3]; 3] = [[0., 0., 0.01], [0.9572, 0., 0.], [-0.2400, 0.9266, 0.]];

fn send(app: &mut App, action: Action) {
    let _ = app.update(Message::Xtb(action));
}

fn event(app: &mut App, event: Event) {
    app.xtb_answer(event);
}

/// A selected oxygen; Run asks for its 3D model, answered here as water.
fn water(task: CalcTask) -> (App, u64) {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let o = app.tab.doc.add_atom("O", Point::default());
    app.tab.selected = vec![o];
    app.xtb.settings.task = task;
    send(&mut app, Action::Run);
    assert!(app.tab.model_job.is_some(), "Run makes the 3D model first");
    assert!(app.xtb_running());
    let positions = START.map(|[x, y, z]| Point3 { x, y, z }).to_vec();
    let _ = app.finish_model(positions, vec![0, 0]);
    assert_eq!(app.tab.doc.models[0].display, Display::Both);
    assert!(
        app.tab
            .xtb
            .run
            .as_ref()
            .is_some_and(|run| run.launch.is_some()),
        "the worker starts once the model exists: {}",
        app.status
    );
    (app, o)
}

fn model(doc: &Document) -> Vec<[f32; 3]> {
    let model = &doc.models[0];
    model
        .atoms
        .iter()
        .map(|a| a.xyz)
        .chain(model.hydrogens.iter().map(|h| h.xyz))
        .collect()
}

fn f32s(positions: &[[f64; 3]]) -> Vec<[f32; 3]> {
    positions.iter().map(|p| p.map(|x| x as f32)).collect()
}

fn optimized(start: &[[f64; 3]], end: &[[f64; 3]]) -> Event {
    Event::Done(Ok(Arc::new(Report {
        engine_version: "0.2.0".into(),
        engine_rev: "9d0388c".into(),
        energy: -5.070176,
        gap_ev: Some(14.01),
        near_degenerate: false,
        converged: true,
        positions: end.to_vec(),
        gradient: vec![0.; end.len() * 3],
        steps: 1,
        energies: vec![-5.06, -5.070176],
        path: [start.as_flattened(), end.as_flattened()].concat(),
    })))
}

#[test]
fn an_optimization_moves_a_preview_then_applies_as_one_undo_step() {
    let (mut app, _) = water(CalcTask::Optimize);
    let before = model(&app.tab.doc);
    assert_eq!(before, f32s(&START));
    event(
        &mut app,
        Event::Steps {
            first: 0,
            energies: vec![-5.06, -5.07],
            positions: MOVED.to_vec(),
        },
    );
    // The canvas shows the step; the document does not change.
    assert_eq!(model(app.display_document()), f32s(&MOVED));
    assert_eq!(model(&app.tab.doc), before);
    assert!(
        app.status
            .starts_with("xTB · H2O · Optimizing · step 1 · ΔE"),
        "{}",
        app.status
    );

    event(&mut app, optimized(&START, &MOVED));
    assert!(!app.xtb_running());
    assert_eq!(model(&app.tab.doc), f32s(&MOVED));
    assert_eq!(app.tab.doc.science.results.len(), 1);
    assert!(
        app.status
            .starts_with("Applied xTB geometry · E −5.070176 Eh"),
        "{}",
        app.status
    );
    assert!(matches!(
        &app.tab.xtb.outcome,
        Some(Outcome::Done { line, notes }) if line == "E −5.070176 Eh · gap 14.01 eV · converged in 1 step" && notes.is_empty()
    ));

    let _ = app.update(Message::Undo);
    assert_eq!(model(&app.tab.doc), before);
    assert!(app.tab.doc.science.results.is_empty());
}

#[test]
fn an_energy_keeps_the_result_and_the_model() {
    let (mut app, _) = water(CalcTask::Energy);
    event(
        &mut app,
        Event::Done(Ok(Arc::new(Report {
            engine_version: "0.2.0".into(),
            engine_rev: "9d0388c".into(),
            energy: -5.070176,
            gap_ev: Some(14.01),
            near_degenerate: true,
            converged: true,
            positions: START.to_vec(),
            gradient: vec![],
            steps: 0,
            energies: vec![-5.070176],
            path: vec![],
        }))),
    );
    assert_eq!(model(&app.tab.doc), f32s(&START));
    let stored = &app.tab.doc.science.results[0].result;
    assert_eq!(stored.task, qcdata::Task::Energy);
    assert_eq!(stored.energy_hartree, Some(-5.070176));
    assert_eq!(app.status, "xTB · E −5.070176 Eh · gap 14.01 eV");
    assert!(matches!(
        &app.tab.xtb.outcome,
        Some(Outcome::Done { notes, .. }) if notes == &[state::NEAR_DEGENERATE.to_owned()]
    ));
}

#[test]
fn cancel_stops_the_preview_and_ignores_late_replies() {
    let (mut app, _) = water(CalcTask::Optimize);
    let serial = app.tab.xtb.run.as_ref().map(|run| run.serial).unwrap();
    event(
        &mut app,
        Event::Steps {
            first: 0,
            energies: vec![-5.06],
            positions: MOVED.to_vec(),
        },
    );
    let _ = app.update(Message::Escape);
    assert!(!app.xtb_running());
    assert_eq!(app.status, "Calculation canceled");
    assert_eq!(model(app.display_document()), f32s(&START));
    send(&mut app, Action::Event(serial, optimized(&START, &MOVED)));
    assert!(app.tab.xtb.outcome.is_none());
    assert!(app.tab.doc.science.results.is_empty());
    assert_eq!(model(&app.tab.doc), f32s(&START));
}

#[test]
fn a_molecule_edited_during_the_run_keeps_the_result_stale_and_unapplied() {
    let (mut app, o) = water(CalcTask::Optimize);
    let before = app.tab.doc.clone();
    let carbon = app.tab.doc.add_atom("C", Point::new(40., 0.));
    app.tab.doc.add_bond(o, carbon, 1, "plain");
    app.changed(before);
    event(
        &mut app,
        Event::Steps {
            first: 0,
            energies: vec![-5.06],
            positions: MOVED.to_vec(),
        },
    );
    assert!(app.xtb_preview().is_none(), "an edit ends the preview");
    event(&mut app, optimized(&START, &MOVED));
    assert_eq!(app.tab.doc.science.results.len(), 1);
    assert_eq!(model(&app.tab.doc), f32s(&START));
    assert!(matches!(
        &app.tab.xtb.outcome,
        Some(Outcome::Done { notes, .. }) if notes[0].starts_with("The molecule changed during the calculation")
    ));
}

#[test]
fn a_worker_failure_applies_nothing() {
    let (mut app, _) = water(CalcTask::Optimize);
    event(
        &mut app,
        Event::Done(Err("SCF did not converge: 250 iterations".into())),
    );
    assert!(app.error);
    assert_eq!(
        app.status,
        "xTB calculation failed: SCF did not converge: 250 iterations"
    );
    assert!(app.tab.doc.science.results.is_empty());
    assert_eq!(model(&app.tab.doc), f32s(&START));
}

/// CHFClBr with F wedged, its model matching the wedge (as in the science tests).
fn halocarbon() -> (App, u64, Vec<[f64; 3]>) {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let doc = &mut app.tab.doc;
    let c = doc.add_atom("C", Point::default());
    let f = doc.add_atom("F", Point::new(0., -28.));
    let cl = doc.add_atom("Cl", Point::new(-28., 14.));
    let br = doc.add_atom("Br", Point::new(28., 14.));
    doc.add_bond(c, f, 1, "wedge");
    doc.add_bond(c, cl, 1, "plain");
    doc.add_bond(c, br, 1, "plain");
    app.tab.selected = vec![c];
    send(&mut app, Action::Run);
    let order = app.model_atoms();
    let at = |id: u64| match id {
        id if id == f => [0., 1., 0.8],
        id if id == cl => [-1., -0.5, 0.],
        id if id == br => [1., -0.5, 0.],
        _ => [0., 0., 0.],
    };
    let mut start: Vec<[f64; 3]> = order.iter().map(|id| at(*id)).collect();
    start.push([0., 0., -1.]);
    let parent = order.iter().position(|id| *id == c).unwrap();
    let points = start.iter().map(|&[x, y, z]| Point3 { x, y, z }).collect();
    let _ = app.finish_model(points, vec![parent]);
    (app, c, start)
}

#[test]
fn a_stereo_change_waits_for_apply_anyway_or_discard() {
    let (mut app, _, start) = halocarbon();
    let mirrored: Vec<[f64; 3]> = start.iter().map(|&[x, y, z]| [-x, y, z]).collect();
    event(&mut app, optimized(&start, &mirrored));
    let Some(Outcome::Review(review)) = &app.tab.xtb.outcome else {
        panic!("expected the stereo warning: {}", app.status);
    };
    assert!(
        review.reasons[0].starts_with("Optimization changed the configuration at C"),
        "{:?}",
        review.reasons
    );
    assert!(app.status.ends_with("· nothing applied"), "{}", app.status);
    assert!(app.tab.doc.science.results.is_empty());
    assert_eq!(model(&app.tab.doc), f32s(&start));

    send(&mut app, Action::Discard);
    assert!(app.tab.xtb.outcome.is_none());
    assert_eq!(model(&app.tab.doc), f32s(&start));

    // Again, applied this time: one undo step.
    send(&mut app, Action::Run);
    event(&mut app, optimized(&start, &mirrored));
    send(&mut app, Action::ApplyAnyway);
    assert_eq!(model(&app.tab.doc), f32s(&mirrored));
    assert_eq!(app.tab.doc.science.results.len(), 1);
    let _ = app.update(Message::Undo);
    assert_eq!(model(&app.tab.doc), f32s(&start));
    assert!(app.tab.doc.science.results.is_empty());
}

#[test]
fn a_model_moved_during_the_run_is_not_overwritten() {
    let (mut app, _) = water(CalcTask::Optimize);
    // Another conformer (no chemical change), as Undo of a regeneration would bring.
    let before = app.tab.doc.clone();
    app.tab.doc.models[0].hydrogens[0].xyz = [0.5, 0.5, 0.5];
    app.changed(before);
    let moved = model(&app.tab.doc);
    event(&mut app, optimized(&START, &MOVED));
    assert_eq!(model(&app.tab.doc), moved);
    assert_eq!(app.tab.doc.science.results.len(), 1);
    assert!(matches!(
        &app.tab.xtb.outcome,
        Some(Outcome::Done { notes, .. }) if notes[0].starts_with("The 3D model changed during the calculation")
    ));
}

#[test]
fn choosing_2d_while_the_model_is_made_stops_the_run() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let o = app.tab.doc.add_atom("O", Point::default());
    app.tab.selected = vec![o];
    send(&mut app, Action::Run);
    assert!(app.tab.model_job.is_some());
    let _ = app.update(Message::Models3d(crate::app::models3d::Action::Show(
        Display::TwoD,
    )));
    assert!(app.tab.model_job.is_none());
    assert!(!app.xtb_running());
    assert!(matches!(app.tab.xtb.outcome, Some(Outcome::Failed(_))));
}

#[test]
fn progress_keeps_menus_open() {
    let (mut app, _) = water(CalcTask::Optimize);
    app.tabs.menu = true;
    event(&mut app, Event::Tick);
    assert!(app.tabs.menu);
    assert!(
        app.status.starts_with("xTB · H2O · Starting xTB"),
        "{}",
        app.status
    );
}
