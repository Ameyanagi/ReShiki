//! A run from Run to its finish, with the worker's events written by hand.
use super::run::{self, Event, Report};
use super::*;
use reshiki::document::Point;
use reshiki::geometry::Point3;
use reshiki::models3d::Display;
use std::sync::Arc;

pub(super) const START: [[f64; 3]; 3] = [[0., 0., 0.], [0.96, 0., 0.], [-0.24, 0.93, 0.]];
pub(super) const MOVED: [[f64; 3]; 3] = [[0., 0., 0.01], [0.9572, 0., 0.], [-0.2400, 0.9266, 0.]];

pub(super) fn send(app: &mut App, action: Action) {
    let _ = app.update(Message::Xtb(action));
}

pub(super) fn event(app: &mut App, event: Event) {
    app.xtb_answer(event);
}

/// A selected oxygen; Run asks for its 3D model, answered here as water.
pub(super) fn water(task: CalcTask) -> (App, u64) {
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

pub(super) fn model(doc: &Document) -> Vec<[f32; 3]> {
    let model = &doc.models[0];
    model
        .atoms
        .iter()
        .map(|a| a.xyz)
        .chain(model.hydrogens.iter().map(|h| h.xyz))
        .collect()
}

pub(super) fn f32s(positions: &[[f64; 3]]) -> Vec<[f32; 3]> {
    positions.iter().map(|p| p.map(|x| x as f32)).collect()
}

pub(super) fn optimized(start: &[[f64; 3]], end: &[[f64; 3]]) -> Event {
    Event::Done(Ok(Arc::new(optimization(start, end))))
}

fn optimization(start: &[[f64; 3]], end: &[[f64; 3]]) -> Report {
    // An optimization's last compute carries the gradient.
    let mut computed = run::example_computed(-5.070176, 14.01);
    computed.gradient = Some(vec![[0.; 3]; end.len()]);
    Report {
        engine: run::example_engine(),
        computed,
        energy: -5.070176,
        converged: true,
        positions: end.to_vec(),
        steps: 1,
        energies: vec![-5.06, -5.070176],
        path: [start.as_flattened(), end.as_flattened()].concat(),
        evaluations: vec![],
    }
}

pub(super) fn optfreq(start: &[[f64; 3]], end: &[[f64; 3]]) -> Report {
    let mut report = optimization(start, end);
    report.computed.vibrations = Some(
        serde_json::from_value(serde_json::json!({
            "conventions": "axtb",
            "frequencies_cm1": [1590.13, 3468.58, 3480.11],
            "ir_intensities_km_mol": [127.68, 14.05, 5.08],
            "reduced_masses_amu": [1.08, 1.04, 1.07],
            "force_constants_mdyn_per_angstrom": [1.6, 7.4, 7.6],
            "masses_amu": [15.999, 1.008, 1.008],
            "linear": false,
            "point_group": "c2v"
        }))
        .unwrap(),
    );
    report.computed.thermo = Some(qcdata::Thermo {
        temperature_k: 298.15,
        pressure_pa: 101325.,
        zero_point_energy_hartree: 0.019453,
        enthalpy_correction_hartree: 0.023230,
        entropy_hartree_per_k: 0.00007198,
        gibbs_correction_hartree: 0.001769,
        heat_capacity_hartree_per_k: Some(0.0000095),
        frequency_scale: 1.,
        rotor_cutoff_cm1: 50.,
        imaginary_cutoff_cm1: 20.,
        symmetry_number: 2,
        point_group: None,
    });
    report.computed.normal_modes = (0..3)
        .flat_map(|i| (0..9).map(move |j| if j == i { 1. } else { 0. }))
        .collect();
    report
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
        Some(Outcome::Done { line, notes, .. }) if line == "E −5.070176 Eh · gap 14.01 eV · converged in 1 step" && notes.is_empty()
    ));

    let _ = app.update(Message::Undo);
    assert_eq!(model(&app.tab.doc), before);
    assert!(app.tab.doc.science.results.is_empty());
}

#[test]
fn optfreq_stores_the_vibrations_and_applies_geometry_as_one_undo_step() {
    let (mut app, _) = water(CalcTask::OptFreq);
    event(
        &mut app,
        Event::Steps {
            first: 0,
            energies: vec![-5.06, -5.07],
            positions: MOVED.to_vec(),
        },
    );
    assert!(app.status.starts_with("xTB · H2O · Optimizing · step 1"));
    event(&mut app, Event::Hessian);
    assert!(app.status.starts_with("xTB · H2O · Computing frequencies"));

    event(&mut app, Event::Done(Ok(Arc::new(optfreq(&START, &MOVED)))));
    assert_eq!(model(&app.tab.doc), f32s(&MOVED));
    assert!(matches!(
        &app.tab.xtb.outcome,
        Some(Outcome::Done { line, .. }) if line.ends_with("converged in 1 step · lowest 1590 cm⁻¹")
    ));
    let shown = app.xtb_shown().expect("the new result");
    let result = &shown.stored.result;
    assert_eq!(result.task, qcdata::Task::Optimize);
    let qcdata::Settings::Xtb(settings) = &result.settings else {
        panic!("expected xTB settings");
    };
    assert_eq!(settings.convergence, Some(qcdata::Convergence::Tight));
    assert_eq!(result.frame, shown.frame.hash());
    assert_eq!(shown.frame.positions_angstrom, MOVED);
    assert_eq!(result.vibrations.as_ref().unwrap().modes.len(), 27);
    assert!(result.thermo.is_some());
    assert_eq!(
        results::View::available(result),
        [View::Summary, View::Vibrations, View::Steps, View::Log]
    );
    let rows = shown.rows();
    let row = |name: &str| {
        rows.iter()
            .find(|(label, _)| label == name)
            .map(|(_, value)| value.as_str())
    };
    assert_eq!(row("Frequencies"), Some("lowest 1590 cm⁻¹"));
    assert_eq!(row("ZPVE"), Some("0.019453 Eh (12.21 kcal/mol)"));
    assert_eq!(row("G (298.15 K)"), Some("−5.068407 Eh"));
    assert_eq!(row("Point group"), Some("C2v"));

    let _ = app.update(Message::Undo);
    assert_eq!(model(&app.tab.doc), f32s(&START));
    assert!(app.tab.doc.science.results.is_empty());
}

#[test]
fn frequencies_store_a_hessian_on_the_starting_geometry() {
    let (mut app, _) = water(CalcTask::Frequencies);
    let report = Report {
        steps: 0,
        energies: vec![-5.070176],
        path: vec![],
        ..optfreq(&START, &START)
    };
    event(&mut app, Event::Done(Ok(Arc::new(report))));
    assert_eq!(model(&app.tab.doc), f32s(&START));
    let shown = app.xtb_shown().expect("the new result");
    assert_eq!(shown.stored.result.task, qcdata::Task::Hessian);
    let qcdata::Settings::Xtb(settings) = &shown.stored.result.settings else {
        panic!("expected xTB settings");
    };
    assert_eq!(settings.convergence, None);
    assert_eq!(shown.stored.result.frame, shown.input.hash());
    assert_eq!(
        shown.frame.positions_angstrom,
        shown.input.positions_angstrom
    );
    assert!(shown.stored.result.vibrations.is_some());
    assert!(shown.stored.result.trajectory.is_none());
}

#[test]
fn frequencies_reject_a_target_above_the_hessian_limit() {
    let (mut app, _) = App::new();
    let settings = Settings {
        task: CalcTask::Frequencies,
        ..Settings::default()
    };
    let _ = app.xtb_begin(Request {
        atoms: (1..=201).collect(),
        config: settings.config((0, 0), false, vec![]).unwrap(),
        keep_full: false,
        start: None,
        search: None,
    });
    assert!(matches!(
        &app.tab.xtb.outcome,
        Some(Outcome::Failed(failed)) if matches!(failed.failure, state::Failure::TooMany { count: 201, limit: 200 })
    ));
    assert!(!app.xtb_running());
}

#[test]
fn an_energy_keeps_the_result_and_the_model() {
    let (mut app, _) = water(CalcTask::Energy);
    event(
        &mut app,
        Event::Done(Ok(Arc::new(Report {
            engine: run::example_engine(),
            computed: run::example_computed(-5.070176, 0.05),
            energy: -5.070176,
            converged: true,
            positions: START.to_vec(),
            steps: 0,
            energies: vec![-5.070176],
            path: vec![],
            evaluations: vec![],
        }))),
    );
    assert_eq!(model(&app.tab.doc), f32s(&START));
    let stored = &app.tab.doc.science.results[0].result;
    assert_eq!(stored.task, qcdata::Task::Energy);
    assert_eq!(stored.energy_hartree, Some(-5.070176));
    assert_eq!(app.status, "xTB · E −5.070176 Eh · gap 0.05 eV");
    assert!(matches!(
        &app.tab.xtb.outcome,
        Some(Outcome::Done { notes, .. }) if notes == &[state::NEAR_DEGENERATE.to_owned()]
    ));
}

#[test]
fn an_energy_stores_the_worker_charges_bond_orders_and_orbital_levels() {
    let (mut app, _) = water(CalcTask::Energy);
    let mut computed = run::example_computed(-5.070176, 14.01);
    computed.conventions = qcdata::XtbConventions::Xtb68;
    computed.engine_conventions = "xtb".into();
    computed.settings.scf.accuracy = Some(0.5);
    computed.settings.scf.electronic_temperature_k = Some(500.);
    computed.settings.scf.max_iterations = Some(400);
    computed.charges = Some(qcdata::AtomicCharges {
        scheme: qcdata::ChargeScheme::Mulliken,
        values: vec![-0.56, 0.28, 0.28],
    });
    computed.dipole_e_bohr = Some([0., 0., 0.85]);
    computed.bond_orders = Some(vec![
        qcdata::BondOrder {
            a: 0,
            b: 1,
            order: 0.92,
        },
        qcdata::BondOrder {
            a: 0,
            b: 2,
            order: 0.92,
        },
    ]);
    computed.orbitals = Some(
        serde_json::from_value(serde_json::json!({
            "core_electrons": [2, 0, 0],
            "alpha": {
                "energies_hartree": [-0.7, -0.5, -0.4, -0.3, 0.2, 0.4],
                "occupations": [2., 2., 2., 2., 0., 0.]
            },
            "beta": "restricted"
        }))
        .expect("orbital levels"),
    );
    let report = Arc::new(Report {
        engine: run::example_engine(),
        energy: computed.energy,
        computed,
        converged: true,
        positions: START.to_vec(),
        steps: 0,
        energies: vec![-5.070176],
        path: vec![],
        evaluations: vec![],
    });
    event(&mut app, Event::Done(Ok(report.clone())));

    let stored = &app.tab.doc.science.results[0].result;
    assert_eq!(stored.charges, report.computed.charges.as_slice());
    assert_eq!(stored.dipole_e_bohr, report.computed.dipole_e_bohr);
    assert_eq!(
        Some(&stored.bond_orders),
        report.computed.bond_orders.as_ref()
    );
    assert_eq!(stored.orbitals, report.computed.orbitals);
    let qcdata::Settings::Xtb(settings) = &stored.settings else {
        panic!("expected xTB settings");
    };
    assert_eq!(settings.conventions, qcdata::XtbConventions::Xtb68);
    assert_eq!(settings.accuracy, Some(0.5));
    assert_eq!(settings.electronic_temperature_k, Some(500.));
    assert_eq!(settings.max_iterations, Some(400));
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
        Event::Done(Err(state::Failure::Scf("250 iterations".into()))),
    );
    assert!(app.error);
    assert_eq!(
        app.status,
        "xTB calculation failed: The SCF did not converge."
    );
    assert!(app.tab.doc.science.results.is_empty());
    assert_eq!(model(&app.tab.doc), f32s(&START));
}

#[test]
fn an_scf_failure_retains_the_request_for_retry() {
    let (mut app, o) = water(CalcTask::Optimize);
    send(&mut app, Action::Cancel);
    app.xtb.settings.temperature = "450".into();
    app.xtb.settings.threads = 2;
    send(&mut app, Action::Run);
    let config = app
        .tab
        .xtb
        .run
        .as_ref()
        .expect("run started")
        .config
        .clone();
    app.xtb.settings.task = CalcTask::Energy;

    event(
        &mut app,
        Event::Done(Err(state::Failure::Scf("250 iterations".into()))),
    );
    assert!(!app.xtb_running());
    let Some(Outcome::Failed(failed)) = &app.tab.xtb.outcome else {
        panic!("expected an SCF failure");
    };
    let Some(request) = &failed.request else {
        panic!("the failure keeps its request");
    };
    assert!(
        matches!(failed.failure.reason(), state::Failure::Scf(message) if message == "250 iterations")
    );
    assert_eq!(request.atoms, [o]);
    assert_eq!(request.config, config);
    assert!(!request.keep_full);
}

/// CHFClBr with F wedged, its model matching the wedge (as in the science tests).
pub(super) fn halocarbon() -> (App, u64, Vec<[f64; 3]>) {
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

#[test]
fn optimize_with_gfn2_shows_the_card_and_runs() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let o = app.tab.doc.add_atom("O", Point::default());
    app.tab.selected = vec![o];
    app.xtb.settings.method = qcdata::XtbMethod::GfnFf;
    app.xtb.settings.task = CalcTask::Energy;
    send(&mut app, Action::Entry(Entry::OptimizeGfn2));
    if !available() {
        // Builds without xTB hide the entry; a stray message changes nothing.
        assert_eq!(app.xtb.settings.method, qcdata::XtbMethod::GfnFf);
        assert!(!app.xtb_running());
        return;
    }
    assert_eq!(app.xtb.settings.method, qcdata::XtbMethod::Gfn2Xtb);
    assert_eq!(app.xtb.settings.task, CalcTask::Optimize);
    assert_eq!(app.xtb.page, Page::Xtb);
    assert!(app.inspector_open);
    assert_eq!(app.inspector_tab, crate::app::InspectorTab::Predict);
    let run = app.tab.xtb.run.as_ref().expect("the entry starts a run");
    assert_eq!(
        (run.config.task, run.config.method),
        (CalcTask::Optimize, qcdata::XtbMethod::Gfn2Xtb)
    );
}

#[test]
fn rerun_repeats_the_cards_last_calculation() {
    let (mut app, _) = water(CalcTask::Energy);
    send(&mut app, Action::Cancel);
    app.xtb.page = Page::Nmr;
    send(&mut app, Action::Entry(Entry::Rerun));
    if !available() {
        assert_eq!(app.xtb.page, Page::Nmr);
        assert!(!app.xtb_running());
        return;
    }
    assert_eq!(app.xtb.page, Page::Xtb);
    let run = app.tab.xtb.run.as_ref().expect("⌘R starts a run");
    assert_eq!(run.config.task, CalcTask::Energy);
}

#[test]
fn retry_runs_the_failed_request_at_1000_k() {
    let (mut app, _) = water(CalcTask::Optimize);
    event(
        &mut app,
        Event::Done(Err(state::Failure::Scf(
            "SCF did not converge in 250 iterations".into(),
        ))),
    );
    assert!(
        app.tab.xtb.palette.open,
        "the error card shows in the palette"
    );
    // The card changing meanwhile does not change what is retried.
    app.xtb.settings.task = CalcTask::Energy;
    send(&mut app, Action::Retry);
    let run = app.tab.xtb.run.as_ref().expect("Retry starts a run");
    assert_eq!(run.config.task, CalcTask::Optimize);
    assert_eq!(run.config.temperature_k, Some(state::RETRY_TEMPERATURE_K));
    assert!(app.tab.xtb.outcome.is_none());
}

#[test]
fn an_edit_during_the_run_retires_retry() {
    let (mut app, _) = water(CalcTask::Energy);
    app.tab.doc.add_atom("C", Point::new(40., 0.));
    app.tab.revision += 1;
    event(
        &mut app,
        Event::Done(Err(state::Failure::Scf("not converged".into()))),
    );
    let Some(Outcome::Failed(failed)) = &app.tab.xtb.outcome else {
        panic!("a failure");
    };
    assert!(
        !app.xtb_can_retry(failed),
        "the run calculated the drawing before the edit"
    );
}

#[test]
fn an_edit_after_a_failure_retires_retry() {
    let (mut app, _) = water(CalcTask::Energy);
    event(
        &mut app,
        Event::Done(Err(state::Failure::Scf("not converged".into()))),
    );
    let failed = |app: &App| match &app.tab.xtb.outcome {
        Some(Outcome::Failed(failed)) => app.xtb_can_retry(failed),
        _ => false,
    };
    assert!(failed(&app));
    let c = app.tab.doc.add_atom("C", Point::new(40., 0.));
    app.tab.revision += 1;
    assert!(!failed(&app), "Retry would calculate another drawing");
    send(&mut app, Action::Retry);
    assert!(!app.xtb_running());
    assert!(app.tab.doc.atom(c).is_some());
}

#[test]
fn a_crash_offers_run_again_and_details_without_the_structure() {
    let (mut app, _) = water(CalcTask::Energy);
    event(
        &mut app,
        Event::Done(Err(state::Failure::Crashed {
            summary: "xTB worker exited with signal: 11".into(),
            details: Some("xTB worker exited with signal: 11: worker stderr".into()),
        })),
    );
    assert_eq!(
        app.status,
        "xTB calculation failed: The calculation stopped unexpectedly. Your drawing is safe."
    );
    let Some(Outcome::Failed(failed)) = &app.tab.xtb.outcome else {
        panic!("a failure");
    };
    let details = failed.details();
    assert!(
        details.contains("GFN2-xTB · Energy · charge 0"),
        "{details}"
    );
    assert!(
        details.contains("solvent none · conventions axtb"),
        "{details}"
    );
    assert!(details.contains("H2O, 3 atoms"), "{details}");
    assert!(details.contains("signal: 11"), "{details}");
    assert!(!details.contains("worker stderr"), "{details}");
    assert!(!details.contains("0.96"), "no coordinates: {details}");
    send(&mut app, Action::RunAgain);
    assert!(app.xtb_running());
}

#[test]
fn a_reopened_file_shows_the_properties_and_recalculates_with_the_stored_settings() {
    let (mut app, o) = water(CalcTask::Energy);
    let mut computed: reshiki_xtb_worker::protocol::Computed =
        serde_json::from_value(serde_json::json!({
            "method": "gxtb",
            "conventions": "xtb-6.8",
            "engine_conventions": "gxtb",
            "numerics": "f64",
            "settings": {
                "charge": 0, "uhf": 0, "threads": 1,
                "solvent": { "model": "alpb", "name": "water", "correction": "gfn2" },
                "scf": { "accuracy": 0.5, "electronic_temperature_k": 1000.0, "max_iterations": 400 }
            },
            "energy": -5.2,
            "energy_terms": {},
            "charges": { "scheme": "mulliken", "values": [-0.56, 0.28, 0.28] },
            "dipole_e_bohr": [0.0, 0.0, 0.85],
            "bond_orders": [{ "a": 0, "b": 1, "order": 0.92 }, { "a": 1, "b": 2, "order": 0.2104 }],
            "homo_lumo_gap_ev": 13.6,
            "near_degenerate": false,
            "arrays": [],
            "seconds": 0.003
        }))
        .expect("worker response");
    computed.orbitals = Some(
        serde_json::from_value(serde_json::json!({
            "core_electrons": [2, 0, 0],
            "alpha": {
                "energies_hartree": [-0.7, -0.5, -0.4, -0.3, 0.2, 0.4],
                "occupations": [2., 2., 2., 2., 0., 0.]
            },
            "beta": "restricted"
        }))
        .expect("orbital levels"),
    );
    event(
        &mut app,
        Event::Done(Ok(Arc::new(Report {
            engine: run::example_engine(),
            energy: computed.energy,
            computed,
            converged: true,
            positions: START.to_vec(),
            steps: 0,
            energies: vec![-5.2],
            path: vec![],
            evaluations: vec![],
        }))),
    );
    let bytes = app.tab.doc.file_json().expect("save");

    let mut reopened = reopen_result(&bytes);
    let result = reopened.tab.doc.science.results[0].result.clone();
    let shown = reopened.xtb_shown().expect("the palette shows the result");
    assert!(shown.info.is_none());
    let rows = shown.rows();
    let row = |name: &str| {
        rows.iter()
            .find(|(label, _)| *label == name)
            .map(|(_, value)| value.clone())
    };
    // (0.2 + 0.3) Eh from the stored levels, not the session's 13.6 eV.
    assert_eq!(row("HOMO–LUMO gap").as_deref(), Some("13.61 eV"));
    assert_eq!(row("Dipole").as_deref(), Some("2.16 D"));
    assert!(row("Charges").is_some_and(|c| c.starts_with("O −0.560")));
    // A weak 0.21 order survives saving, as the Summary showed it.
    assert_eq!(
        results::bond_orders(&result, shown.frame)[1],
        ("H2–H3".to_owned(), "0.21".to_owned())
    );
    assert_eq!(results::levels(&result).len(), 5);
    assert!(shown.method().starts_with(
        "g-xTB · axtb 0.2.0 · water (ALPB, GFN2-xTB correction) · xtb 6.8 compatible · 1000 K · charge 0 · multiplicity 1 · "
    ));
    // A saved GBSA result says GBSA, and the log keeps the SCF settings.
    let mut gbsa = (*result).clone();
    if let reshiki::qcdata::Settings::Xtb(settings) = &mut gbsa.settings {
        settings.solvent.as_mut().expect("water").model = reshiki::qcdata::SolventModel::Gbsa;
        settings.accuracy = Some(0.3);
        settings.max_iterations = Some(500);
    }
    let method = results::method_line(&gbsa, shown.frame);
    assert!(
        method.contains("water (GBSA, GFN2-xTB correction)"),
        "{method}"
    );
    let log = results::log(&gbsa, shown.frame, None, &method);
    assert!(
        log.contains("SCF accuracy 0.3 · max 500 SCF iterations"),
        "{log}"
    );

    // Recalculate runs the stored method and settings, not the card's.
    let _ = reopened.update(Message::Xtb(Action::Palette(palette::Action::Recalculate)));
    let run = reopened
        .tab
        .xtb
        .run
        .as_ref()
        .expect("Recalculate starts a run");
    let config = &run.config;
    assert_eq!(
        (config.method, config.solvent, config.xtb68),
        (qcdata::XtbMethod::GXtb, Some("water"), true)
    );
    assert_eq!(
        (config.accuracy, config.temperature_k, config.max_iterations),
        (Some(0.5), Some(1000.), Some(400))
    );
    assert_eq!(run.atoms, [o]);
}

fn reopen_result(bytes: &[u8]) -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = Document::from_native_file(bytes).expect("reopen");
    app.tab
        .xtb
        .palette
        .show(&app.tab.doc.science.results[0].result);
    app
}

fn saved_optimization() -> (Vec<u8>, [u64; 2]) {
    let (mut app, carbon, start) = halocarbon();
    send(&mut app, Action::Cancel);
    let fixed = [carbon, app.tab.doc.atoms[2].id];
    app.tab.selected = fixed.into_iter().rev().collect();
    send(&mut app, Action::Task(CalcTask::Optimize));
    send(&mut app, Action::Threads(3));
    send(&mut app, Action::Tight(true));
    send(&mut app, Action::HoldFixed(true));
    send(&mut app, Action::Run);
    app.tab.xtb.run.as_mut().expect("started").started -= std::time::Duration::from_secs(1);

    let mut end = start.clone();
    end.last_mut().unwrap()[2] -= 0.01;
    let mut computed = run::example_computed(-5.2, 13.6);
    computed.settings.threads = 3;
    computed.gradient = Some(vec![[0.; 3]; end.len()]);
    event(
        &mut app,
        Event::Done(Ok(Arc::new(Report {
            engine: run::example_engine(),
            computed,
            energy: -5.2,
            converged: true,
            positions: end.clone(),
            steps: 1,
            energies: vec![-5.06, -5.2],
            path: [start.as_flattened(), end.as_flattened()].concat(),
            evaluations: [(-5.06, 9), (-5.1, 5), (-5.2, 6)]
                .map(|(energy, scf_iterations)| Evaluation {
                    energy,
                    scf_iterations,
                    seconds: 0.01,
                })
                .to_vec(),
        }))),
    );
    assert_eq!(app.tab.doc.science.results.len(), 1, "{}", app.status);
    (app.tab.doc.file_json().expect("save"), fixed)
}

#[test]
fn a_reopened_optimization_keeps_its_run_summary_and_recalculate_settings() {
    let (bytes, fixed) = saved_optimization();
    let mut reopened = reopen_result(&bytes);
    let shown = reopened.xtb_shown().expect("the palette shows the result");
    assert!(shown.info.is_none());
    let result = &shown.stored.result;
    let run = result.run.as_ref().expect("saved run summary");
    assert!(run.seconds >= 1.);
    assert!(shown.rows().contains(&(
        "Run time".into(),
        format!("{:.1} s · 3 SCF calculations", run.seconds),
    )));
    let finished = run.finished.as_ref().expect("finish time");
    let local = chrono::DateTime::parse_from_rfc3339(finished)
        .expect("RFC 3339 finish time")
        .with_timezone(&chrono::Local);
    let method = shown.method();
    assert!(method.ends_with(&format!(" · {}", local.format("%-d %b %Y %H:%M"))));
    let log = results::log(result, shown.frame, shown.info, &method);
    assert!(log.contains("task optimize · 3 threads · tight convergence · 2 atoms held fixed"));
    let mut table = log
        .lines()
        .skip_while(|line| *line != " calc    SCF iterations");
    assert_eq!(table.next(), Some(" calc    SCF iterations"));
    for row in ["1 9", "2 5", "3 6"] {
        assert!(
            table
                .next()
                .unwrap()
                .split_whitespace()
                .eq(row.split_whitespace()),
            "{log}"
        );
    }
    assert_eq!(
        table.next().unwrap(),
        format!("run time {:.1} s", run.seconds)
    );

    reopened.xtb.settings.threads = 7;
    send(&mut reopened, Action::Palette(palette::Action::Recalculate));
    let config = &reopened
        .tab
        .xtb
        .run
        .as_ref()
        .expect("Recalculate starts a run")
        .config;
    assert_eq!(config.threads, 3);
    assert!(config.tight);
    assert_eq!(config.fixed.len(), fixed.len());
    assert!(fixed.iter().all(|id| config.fixed.contains(id)));
}

#[test]
fn a_reopened_legacy_optimization_recalculates_with_defaults() {
    let (bytes, _) = saved_optimization();
    let mut json: serde_json::Value = serde_json::from_slice(&bytes).expect("saved document");
    let result = json["science"]["results"][0]["result"]
        .as_object_mut()
        .unwrap();
    assert!(result.remove("run").is_some());
    let settings = result["settings"]["xtb"].as_object_mut().unwrap();
    for key in ["threads", "convergence", "fixed_atoms"] {
        assert!(settings.remove(key).is_some());
    }
    let mut reopened = reopen_result(&serde_json::to_vec(&json).unwrap());
    let shown = reopened.xtb_shown().expect("the palette shows the result");
    let log = results::log(
        &shown.stored.result,
        shown.frame,
        shown.info,
        &shown.method(),
    );
    assert!(log.lines().any(|line| line == "task optimize"));
    assert!(log.contains("SCF details were not saved with this result; recalculate to see them."));

    reopened.xtb.settings.threads = 7;
    send(&mut reopened, Action::Palette(palette::Action::Recalculate));
    let config = &reopened
        .tab
        .xtb
        .run
        .as_ref()
        .expect("Recalculate starts a run")
        .config;
    assert_eq!(config.threads, 7);
    assert!(!config.tight);
    assert!(config.fixed.is_empty());
}

/// A drawing whose 3D model has `positions` (heavy atoms in drawing order,
/// then hydrogens bound to `parents`), with `element` written over the
/// drawn atom `swap` afterwards: the geometry worker types only organic
/// elements, so uranium and cerium models are made this way.
fn evidence_file(
    heavy: &[(&str, i32)],
    bonds: &[(usize, usize, u8)],
    positions: &[[f64; 3]],
    hydrogens: &[(usize, [f64; 3])],
    swap: Option<(usize, &str)>,
) -> Vec<u8> {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let ids: Vec<u64> = heavy
        .iter()
        .enumerate()
        .map(|(i, (element, charge))| {
            let id = app
                .tab
                .doc
                .add_atom(element, Point::new(i as f32 * 25., (i % 2) as f32 * 14.));
            if let Some(atom) = app.tab.doc.atoms.iter_mut().find(|a| a.id == id) {
                atom.charge = *charge;
            }
            id
        })
        .collect();
    for &(a, b, order) in bonds {
        app.tab.doc.add_bond(ids[a], ids[b], order, "plain");
    }
    app.tab.selected = vec![ids[0]];
    send(&mut app, Action::Run);
    let order = app.model_atoms();
    let mut points: Vec<[f64; 3]> = order
        .iter()
        .map(|id| positions[ids.iter().position(|i| i == id).expect("heavy atom")])
        .collect();
    let parents = hydrogens
        .iter()
        .map(|&(parent, xyz)| {
            points.push(xyz);
            order
                .iter()
                .position(|id| *id == ids[parent])
                .expect("parent")
        })
        .collect();
    let points = points.iter().map(|&[x, y, z]| Point3 { x, y, z }).collect();
    let _ = app.finish_model(points, parents);
    send(&mut app, Action::Cancel);
    assert_eq!(app.tab.doc.models.len(), 1, "{}", app.status);
    if let Some((index, element)) = swap {
        let id = ids[index];
        if let Some(atom) = app.tab.doc.atoms.iter_mut().find(|a| a.id == id) {
            atom.element = element.into();
        }
        let model = &mut app.tab.doc.models[0];
        if let Some(generated) = &mut model.generated {
            let slot = generated.sources.iter().position(|s| *s == Some(id));
            generated.elements[slot.expect("swapped atom")] = element.into();
        }
        // Without a fingerprint the model is not stale after the swap.
        model.fingerprint = None;
    }
    app.tab.doc.file_json().expect("file")
}

/// A zigzag alkane with `carbons` carbons and its hydrogens.
fn alkane_file(carbons: usize) -> Vec<u8> {
    let heavy = vec![("C", 0); carbons];
    let bonds: Vec<_> = (1..carbons).map(|i| (i - 1, i, 1)).collect();
    let positions: Vec<[f64; 3]> = (0..carbons)
        .map(|i| [i as f64 * 1.27, (i % 2) as f64 * 0.87, 0.])
        .collect();
    let mut hydrogens = vec![];
    for (i, c) in positions.iter().enumerate() {
        let ends = i == 0 || i + 1 == carbons;
        let up = if i % 2 == 0 { -1. } else { 1. };
        hydrogens.push((i, [c[0], c[1] + up * 0.5, 0.9]));
        hydrogens.push((i, [c[0], c[1] + up * 0.5, -0.9]));
        if ends {
            let out = if i == 0 { -1. } else { 1. };
            hydrogens.push((i, [c[0] + out * 1.0, c[1] - up * 0.3, 0.]));
        }
    }
    evidence_file(&heavy, &bonds, &positions, &hydrogens, None)
}

#[test]
#[ignore = "writes the U1c real-app evidence files; set RESHIKI_XTB_EVIDENCE_DIR"]
fn manual_write_evidence_files() {
    let dir = std::path::PathBuf::from(std::env::var("RESHIKI_XTB_EVIDENCE_DIR").expect("dir"));
    let linear = [[-1.78, 0., 0.], [0., 0., 0.], [1.78, 0., 0.]];
    let files = [
        (
            "uranyl.rsk",
            evidence_file(
                &[("O", 0), ("S", 2), ("O", 0)],
                &[(0, 1, 2), (1, 2, 2)],
                &linear,
                &[],
                Some((1, "U")),
            ),
        ),
        (
            "ceria.rsk",
            evidence_file(
                &[("O", 0), ("S", 0), ("O", 0)],
                &[(0, 1, 2), (1, 2, 2)],
                &[[-1.85, 0., 0.], [0., 0., 0.], [1.2, 1.4, 0.]],
                &[],
                Some((1, "Ce")),
            ),
        ),
        (
            "dioxygen.rsk",
            evidence_file(
                &[("O", 0), ("O", 0)],
                &[(0, 1, 2)],
                &[[0., 0., 0.], [1.21, 0., 0.]],
                &[],
                None,
            ),
        ),
        ("c170.rsk", alkane_file(170)),
        ("c100.rsk", alkane_file(100)),
    ];
    for (name, bytes) in files {
        std::fs::write(dir.join(name), bytes).expect("written");
    }
}

#[test]
fn isotope_labels_give_explicit_analysis_masses() {
    let mut frame = Frame {
        numbers: vec![8, 1, 1],
        positions_angstrom: START.to_vec(),
        isotopes: vec![0, 2, 0],
        masses_amu: vec![],
        charge: 0,
        multiplicity: 1,
        drawing_atoms: vec![],
    };
    let masses = isotope_masses(&frame).expect("a labelled atom");
    assert!((masses[1] - 2.014101778).abs() < 1e-9, "deuterium");
    assert!((masses[0] - 15.999).abs() < 0.01 && (masses[2] - 1.008).abs() < 0.001);
    frame.masses_amu = masses;
    let settings = Settings {
        task: CalcTask::OptFreq,
        ..Settings::default()
    };
    let config = settings.config((0, 0), false, vec![]).unwrap();
    let job = job(&config, &frame, vec![]);
    assert_eq!(job.isotope_labels, [8000, 1002, 1000]);
    assert_eq!(job.masses.as_deref(), Some(frame.masses_amu.as_slice()));
    assert!(
        isotope_masses(&Frame {
            isotopes: vec![],
            ..frame
        })
        .is_none()
    );
}
