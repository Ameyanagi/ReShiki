//! Focused deterministic checks: these never start a worker process.

use super::*;
use reshiki_optimize::ts::{IrcBranch, Minimum, Reached};

fn launch() -> Launch {
    Launch {
        frame: Arc::new(Frame {
            // Frame order deliberately differs from the drawing IDs.
            numbers: vec![8, 1, 7, 1, 1],
            positions_angstrom: vec![
                [2.8, 0., 0.],
                [-0.8, 0., 0.],
                [0., 0., 0.],
                [0.8, 0., 0.],
                [0., 0.8, 0.],
            ],
            isotopes: Vec::new(),
            masses_amu: Vec::new(),
            charge: 0,
            multiplicity: 1,
            drawing_atoms: Vec::new(),
        }),
        atoms: vec![Some(90), None, Some(20), None, Some(80)],
        baseline: "test".into(),
    }
}

fn plan(start: Side) -> TsPlan {
    TsPlan {
        start,
        start_atoms: vec![20, 90, 80],
        mapped: vec![1, 2],
        mapped_hydrogens: vec![],
        form: vec![(90, 80)],
        break_: vec![(20, 90)],
        hydrogen_moves: vec![(20, 90)],
        shape: (1, 1),
        drawn_charges: (0, 0),
        atoms: 5,
    }
}

#[test]
fn drawing_ids_map_to_frame_order_and_hydrogen_uses_least_motion() {
    let launch = launch();
    let changes = changes(&launch, &plan(Side::Reactant)).unwrap();
    assert_eq!(changes.form, [(0, 3), (0, 4)]);
    assert_eq!(changes.break_, [(0, 2), (2, 3)]);
    // H at frame index 3 is closest to the acceptor. The drawn H at 4
    // and earlier implicit H at 1 are both farther away.
    assert!(!changes.break_.contains(&(1, 2)));
    assert!(!changes.break_.contains(&(2, 4)));
    let reversed = changes.in_reaction_direction(Side::Product);
    assert_eq!(reversed.form, [(0, 2), (2, 3)]);
    assert_eq!(reversed.break_, [(0, 3), (0, 4)]);
}

#[test]
fn repeated_hydrogen_moves_use_distinct_donor_bonded_hydrogens() {
    let mut plan = plan(Side::Reactant);
    plan.hydrogen_moves.push((20, 90));
    let changes = changes(&launch(), &plan).unwrap();
    assert!(changes.break_.contains(&(2, 3)));
    assert!(changes.break_.contains(&(1, 2)));
    assert!(!changes.break_.contains(&(2, 4)));
}

#[test]
fn explicit_mapped_hydrogen_is_not_reassigned_to_an_implicit_move() {
    let mut launch = launch();
    launch.atoms[3] = Some(70);
    let mut plan = plan(Side::Reactant);
    // The nearest H (3) already has an explicit mapped move. The inferred
    // move must choose the next-nearest donor H (4), preserving both moves.
    plan.form = vec![(90, 70)];
    plan.break_ = vec![(20, 70)];
    let changes = changes(&launch, &plan).unwrap();
    assert_eq!(changes.break_, [(2, 3), (2, 4)]);
    assert_eq!(changes.form, [(0, 3), (0, 4)]);
}

#[test]
fn keto_enol_inferred_transfer_skips_the_closest_stationary_mapped_hydrogen() {
    let (doc, arrow) = super::super::card::tests::hydrogen_transfer();
    let plan = reshiki::reactions::ts_plan::plan(&doc, arrow).unwrap();
    let id = |map| doc.atoms.iter().find(|a| a.map_num == map).unwrap().id;
    assert_eq!(plan.mapped_hydrogens, [id(5)]);
    assert_eq!(plan.hydrogen_moves, [(id(2), id(3))]);
    assert!(plan.form.is_empty() && plan.break_.is_empty());

    let launch = Launch {
        frame: Arc::new(Frame {
            numbers: vec![6, 6, 8, 6, 1, 1, 1, 1, 1, 1],
            positions_angstrom: vec![
                [2.3, -1.27, 0.],
                [0., 0., 0.],
                [1.9, 1.14, 0.],
                [1.5, 0., 0.],
                [0.3, 1., 0.], // Stationary mapped H, closest to O.
                [-0.4, -0.9, 0.4],
                [-0.45, 0.03, -0.98], // The closest eligible H.
                [1.82, -2.22, 0.],
                [2.94, -1.19, 0.88],
                [2.94, -1.19, -0.88],
            ],
            isotopes: vec![],
            masses_amu: vec![],
            charge: 0,
            multiplicity: 1,
            drawing_atoms: vec![],
        }),
        atoms: (1..=5).map(|map| Some(id(map))).chain([None; 5]).collect(),
        baseline: "test".into(),
    };
    let mut unreserved = plan.clone();
    unreserved.mapped_hydrogens.clear();
    assert_eq!(changes(&launch, &unreserved).unwrap().form, [(2, 4)]);
    let changes = changes(&launch, &plan).unwrap();
    assert_eq!(changes.form, [(2, 6)]);
    assert_eq!(changes.break_, [(1, 6)]);
}

#[test]
fn missing_atom_or_bonded_hydrogen_is_rejected_before_worker_start() {
    let mut plan = plan(Side::Reactant);
    plan.form.push((999, 20));
    assert!(
        matches!(changes(&launch(), &plan), Err(Failure::Other(message)) if message.contains("999"))
    );
    plan.form.pop();
    plan.hydrogen_moves = vec![(90, 20)];
    assert!(
        matches!(changes(&launch(), &plan), Err(Failure::Other(message)) if message.contains("no available bonded hydrogen"))
    );
}

#[test]
fn worker_pool_respects_cores_and_inner_images() {
    assert_eq!(worker_count(5, 64), 3);
    assert_eq!(worker_count(15, 64), 4);
    assert_eq!(worker_count(15, 2), 2);
    assert_eq!(worker_count(24, 1), 1);
}

fn point(energy: f64, distance: f64) -> Minimum {
    Minimum {
        energy,
        positions: vec![0., 0., 0., 0., 0., distance],
    }
}

fn ts_report() -> TsReport {
    let reactant = point(-3., 2.);
    let product = point(-2., 4.);
    TsReport {
        reactant: reactant.clone(),
        product: product.clone(),
        path: vec![reactant.clone(), point(-1.1, 3.), product.clone()],
        neb_converged: true,
        climbing_image: 1,
        ts: point(-1., 3.),
        saddle_converged: true,
        hessian: vec![0.25; 36],
        frequencies_cm1: vec![-60.],
        imaginary: 1,
        transition_mode: vec![0., 0., -1., 0., 0., 1.],
        irc: Some([
            IrcBranch {
                energies: vec![-1., -3.],
                path: Vec::new(),
                completed: true,
                end: reactant,
                reached: Reached::Reactant,
            },
            IrcBranch {
                energies: vec![-1., -2.],
                path: Vec::new(),
                completed: true,
                end: product,
                reached: Reached::Product,
            },
        ]),
        irc_error: None,
        connects: true,
        evaluations: 12,
        hessians: 2,
    }
}

fn completion() -> (Launch, Completion) {
    let launch = Launch {
        frame: Arc::new(Frame {
            numbers: vec![3, 17],
            positions_angstrom: vec![[0., 0., 0.], [0., 0., 2.]],
            isotopes: Vec::new(),
            masses_amu: Vec::new(),
            charge: 0,
            multiplicity: 1,
            drawing_atoms: Vec::new(),
        }),
        atoms: vec![Some(20), Some(90)],
        baseline: "test".into(),
    };
    let masses = engine_masses(&launch.frame).unwrap();
    let mut checked = crate::app::xtb::run::example_computed(-1., 1.);
    checked.hessian = vec![0.5; 36];
    checked.vibrations = Some(
        serde_json::from_value(serde_json::json!({
            "conventions": "test",
            "frequencies_cm1": [-55.],
            "ir_intensities_km_mol": [12.],
            "reduced_masses_amu": [1.],
            "force_constants_mdyn_per_angstrom": [1.],
            "masses_amu": masses,
            "linear": true,
        }))
        .unwrap(),
    );
    checked.normal_modes = vec![0.5; 6];
    let done = Completion {
        report: ts_report(),
        checked: Ok(checked),
        engine: crate::app::xtb::run::example_engine(),
        changes: Changes {
            form: vec![(0, 1)],
            break_: Vec::new(),
        },
        evaluations: vec![Evaluation {
            energy: -1.,
            scf_iterations: 7,
            seconds: 0.1,
        }],
        masses,
        seconds: 0.25,
    };
    (launch, done)
}

#[test]
fn result_retains_worker_hessian_endpoint_hashes_and_energy_only_band() {
    let (launch, done) = completion();
    let report = complete(&launch, done).unwrap();
    let result = &report.result;
    assert_eq!(result.task, qcdata::Task::TransitionState);
    assert_eq!(result.input_frame, Some(launch.frame.hash()));
    assert_eq!(result.frame, report.frames[0].hash());
    assert_eq!(
        result.endpoints,
        Some(qcdata::Endpoints {
            reactant: report.frames[1].hash(),
            product: report.frames[2].hash(),
        })
    );
    assert_eq!(result.hessian_hartree_per_bohr2, [0.5; 36]);
    let vibrations = result.vibrations.as_ref().unwrap();
    assert_eq!(vibrations.frequencies_cm1, [-55.]);
    assert_eq!(vibrations.ir_intensities_km_mol, [12.]);
    assert_eq!(vibrations.modes, [0.5; 6]);
    assert_eq!(report.extras.worker_frequencies, [-55.]);
    assert_eq!(report.extras.engine_imaginary, Some(-60.));
    assert_eq!(report.extras.image_positions.len(), 3);
    assert_eq!(report.extras.irc_energies, [vec![-1., -3.], vec![-1., -2.]]);
    assert!(report.extras.connects);
    let trajectory = result.trajectory.as_ref().unwrap();
    assert_eq!(trajectory.energies_hartree, [-3., -1.1, -2.]);
    assert!(trajectory.positions_angstrom.is_empty());
    assert_eq!(result.provenance.method.as_deref(), Some("GFN2-xTB"));
    assert_eq!(result.status, qcdata::Status::Converged);
    assert!(!result.keep_full);
    assert_eq!(result.run.as_ref().unwrap().scf_iterations, [7]);
    result.validate(&report.frames[0]).unwrap();
}

#[test]
fn worker_disagreement_or_failed_irc_keeps_candidate_with_cli_reason() {
    let (launch, mut done) = completion();
    done.checked
        .as_mut()
        .unwrap()
        .vibrations
        .as_mut()
        .unwrap()
        .frequencies_cm1 = vec![-49.9];
    let report = complete(&launch, done).unwrap();
    assert_eq!(
        report.result.status,
        qcdata::Status::NotConverged {
            reason: "engine/worker imaginary frequency counts disagree: engine 1, worker 0".into(),
        }
    );
    assert_eq!(report.result.hessian_hartree_per_bohr2.len(), 36);
    let (launch, mut done) = completion();
    done.report.connects = false;
    done.report.irc_error = Some("-mode relaxation reached its step limit".into());
    let report = complete(&launch, done).unwrap();
    assert_eq!(
        report.result.status,
        qcdata::Status::NotConverged {
            reason: "IRC failed: -mode relaxation reached its step limit".into(),
        }
    );
    assert!(!report.extras.connects);
}

#[test]
fn launch_masses_use_isotope_labels_unless_explicit_masses_are_present() {
    let mut frame = (*launch().frame).clone();
    let natural = engine_masses(&frame).unwrap();
    frame.isotopes = vec![18, 2, 0, 0, 0];
    let labelled = engine_masses(&frame).unwrap();
    assert!((labelled[0] - 17.999_16).abs() < 1e-5);
    assert!((labelled[1] - 2.014_102).abs() < 1e-6);
    assert_eq!(labelled[2..], natural[2..]);
    frame.masses_amu = vec![18., 2., 15., 1., 1.];
    assert_eq!(engine_masses(&frame).unwrap(), frame.masses_amu);
}

#[test]
fn failed_worker_check_keeps_engine_candidate_and_cancellation_discards_it() {
    let (launch, mut done) = completion();
    done.checked = Err(ts::TsError::Failed("worker timed out".into()));
    let report = complete(&launch, done).unwrap();
    let result = &report.result;
    assert_eq!(
        result.status,
        qcdata::Status::NotConverged {
            reason: "worker check failed: worker timed out".into()
        }
    );
    assert_eq!(result.energy_hartree, Some(-1.));
    assert_eq!(result.hessian_hartree_per_bohr2, [0.25; 36]);
    assert_eq!(result.vibrations.as_ref().unwrap().frequencies_cm1, [-60.]);
    assert_eq!(
        result.trajectory.as_ref().unwrap().energies_hartree,
        [-3., -1.1, -2.]
    );
    assert_eq!(
        result.endpoints,
        Some(qcdata::Endpoints {
            reactant: report.frames[1].hash(),
            product: report.frames[2].hash(),
        })
    );
    assert_eq!(report.extras.irc_energies, [vec![-1., -3.], vec![-1., -2.]]);
    assert!(report.extras.worker_frequencies.is_empty());
    result.validate(&report.frames[0]).unwrap();

    let (launch, mut done) = completion();
    done.checked = Err(ts::TsError::Cancelled);
    assert!(
        matches!(complete(&launch, done), Err(Failure::Other(message)) if message == "Calculation canceled")
    );
}

#[test]
fn imaginary_cutoff_and_nonconverged_candidate_match_cli_acceptance() {
    let mut report = ts_report();
    report.imaginary = 0;
    assert_eq!(
        failure_reason(&report, &[-50.]),
        Some("expected one imaginary frequency below -50 cm-1, found 0".into())
    );
    report.saddle_converged = false;
    assert_eq!(
        failure_reason(&report, &[-50.]),
        Some("saddle refinement did not converge".into())
    );
    report = ts_report();
    report.connects = false;
    assert_eq!(
        failure_reason(&report, &[-55.]),
        Some("IRC did not connect the reactant and product".into())
    );
}

#[test]
fn irc_failure_names_the_reached_endpoints() {
    let mut report = ts_report();
    report.connects = false;
    for (reached, reason) in [
        (
            [Reached::Reactant; 2],
            "IRC reaches the reactant on both sides",
        ),
        (
            [Reached::Product; 2],
            "IRC reaches the product on both sides",
        ),
        (
            [Reached::Other, Reached::Product],
            "IRC reaches the product and a different minimum",
        ),
        (
            [Reached::Other; 2],
            "IRC reaches neither the reactant nor the product",
        ),
    ] {
        for (branch, reached) in report.irc.as_mut().unwrap().iter_mut().zip(reached) {
            branch.reached = reached;
        }
        assert_eq!(failure_reason(&report, &[-60.]), Some(reason.into()));
    }
}

#[test]
fn cancellation_and_dropped_receiver_stop_progress() {
    let cancel = Cancel::default();
    let (tx, rx) = mpsc::unbounded();
    drop(rx);
    assert!(matches!(control(&cancel, &tx), ControlFlow::Break(())));
    assert!(cancel.is_cancelled());
    assert!(send(&tx, &cancel, Event::Stage(Stage::Endpoints)).is_err());
}
