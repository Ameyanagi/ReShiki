//! Refinement policy and the two-stage generation lifecycle, with worker replies injected.
use super::super::xtb;
use super::refine::{nudge, refine_method, unpaired_electrons};
use super::*;
use reshiki::document::Point;
use reshiki::geometry::Point3;
use reshiki::qcdata::XtbMethod;
use reshiki_xtb_jobs::{Failure as XtbFailure, run};

const CEF3: [u8; 4] = [58, 9, 9, 9];
const PROPYL: [u8; 10] = [6, 6, 6, 1, 1, 1, 1, 1, 1, 1];
const START: [[f64; 3]; 4] = [
    [0., 0., 0.],
    [2.3, 0., 0.],
    [-1.15, 1.99, 0.],
    [-1.15, -1.99, 0.],
];
const REFINED: [[f64; 3]; 4] = [
    [0.01, 0.02, 0.03],
    [2.15, 0.04, 0.02],
    [-1.10, 1.84, -0.08],
    [-1.05, -1.88, 0.11],
];

#[test]
fn refinement_selects_gxtb_for_f_block_elements_and_gfn2_for_other_examples() {
    for (name, numbers) in [
        ("CeF3", CEF3.as_slice()),
        ("UF6", &[92, 9, 9, 9, 9, 9, 9][..]),
        ("ThCl4", &[90, 17, 17, 17, 17][..]),
    ] {
        assert_eq!(refine_method(numbers), XtbMethod::GXtb, "{name}");
    }
    for (name, numbers) in [
        ("XeF4", &[54, 9, 9, 9, 9][..]),
        ("O2", &[8, 8][..]),
        ("propyl radical", PROPYL.as_slice()),
    ] {
        assert_eq!(refine_method(numbers), XtbMethod::Gfn2Xtb, "{name}");
    }
}

#[test]
fn refinement_spin_respects_electron_parity_and_drawn_radicals() {
    assert_eq!(unpaired_electrons(&CEF3, 0, 0), 1, "neutral CeF3");
    assert_eq!(unpaired_electrons(&[8, 8], 0, 0), 0, "drawn O=O");
    assert_eq!(unpaired_electrons(&PROPYL, 0, 1), 1, "drawn propyl radical");
    assert_eq!(
        unpaired_electrons(&CEF3, 1, 0),
        0,
        "the charge participates in electron parity"
    );
}

#[test]
fn refinement_nudges_are_deterministic_small_and_break_coordinate_symmetry() {
    let offsets: Vec<_> = (0..256)
        .map(|index| {
            std::array::from_fn::<_, 3, _>(|axis| {
                let offset = nudge(index, axis);
                assert_eq!(offset, nudge(index, axis));
                assert!(offset.is_finite());
                assert!(offset.abs() <= 0.05, "atom {index}, axis {axis}: {offset}");
                offset
            })
        })
        .collect();
    assert!(offsets.iter().flatten().any(|offset| *offset != 0.));
    assert!(
        offsets.windows(2).any(|pair| pair[0] != pair[1]),
        "the displacement must change relative positions, rather than only translate the molecule"
    );
}

/// A selected CeF3 whose unsupported force-field shape needs a rough start.
fn cerium() -> (App, Vec<u64>) {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let ce = app.tab.doc.add_atom("Ce", Point::default());
    for i in 0..3 {
        let angle = std::f32::consts::TAU * i as f32 / 3.;
        let f = app
            .tab
            .doc
            .add_atom("F", Point::new(42. * angle.cos(), 42. * angle.sin()));
        app.tab.doc.add_bond(ce, f, 1, "plain");
    }
    let atoms: Vec<_> = app.tab.doc.atoms.iter().map(|atom| atom.id).collect();
    app.tab.selected = atoms.clone();
    app.tab.saved = app.tab.doc.clone();
    (app, atoms)
}

fn show(app: &mut App) {
    let _ = app.update(Message::Models3d(Action::Show(Display::Both)));
}

/// Complete only the geometry worker; refinement remains in flight.
fn start_refinement(app: &mut App) -> Key {
    show(app);
    let job = app.tab.model_job.as_ref().expect("a rough generation");
    let key = job.key;
    assert!(job.rough.is_some());
    assert!(job.refining.is_none());
    let _ = app.finish_model(START.map(|[x, y, z]| Point3 { x, y, z }).to_vec(), vec![]);
    let job = app.tab.model_job.as_ref().expect("refinement in flight");
    assert_eq!(job.key, key, "both stages answer the same generation key");
    assert!(job.refining.is_some());
    assert!(
        app.tab.doc.models.is_empty(),
        "the rough start is not applied"
    );
    assert!(!app.error, "{}", app.status);
    key
}

fn report() -> Arc<run::Report> {
    let mut computed = run::example_computed(-25., 2.);
    computed.method = reshiki_xtb_worker::protocol::Method::Gxtb;
    computed.settings.uhf = 1;
    computed.gradient = Some(vec![[0.; 3]; REFINED.len()]);
    Arc::new(run::Report {
        engine: run::example_engine(),
        computed,
        energy: -25.,
        converged: true,
        positions: REFINED.to_vec(),
        steps: 1,
        optimizers: vec![reshiki_optimize::Method::Internal],
        energies: vec![-24., -25.],
        path: [START.as_flattened(), REFINED.as_flattened()].concat(),
        evaluations: vec![],
    })
}

fn refined(app: &mut App, key: Key, result: Result<Arc<run::Report>, XtbFailure>) {
    let _ = app.update(Message::Models3d(Action::Refined(
        key,
        xtb::RunEvent::Done(result),
    )));
}

pub(super) fn fail_optional_refinement(app: &mut App, key: Key) {
    if xtb::available() {
        let job = app.tab.model_job.as_ref().expect("refinement in flight");
        assert!(job.refining.is_some());
        assert!(app.tab.doc.models.is_empty());
        refined(
            app,
            key,
            Err(XtbFailure::Other("injected refinement failure".into())),
        );
        assert!(
            app.status
                .contains("xTB refinement failed: injected refinement failure")
        );
    }
}

#[test]
fn cerium_generation_admits_a_rough_source_without_recording_a_failure() {
    if !xtb::available() {
        return;
    }
    let (mut app, atoms) = cerium();
    let before = app.tab.doc.clone();
    show(&mut app);
    assert!(!app.error, "{}", app.status);
    assert!(app.tab.model_failure.is_none());
    assert_eq!(app.model_failure_atom(), None);
    let job = app.tab.model_job.as_ref().expect("a rough generation");
    assert!(job.rough.is_some());
    assert!(job.refining.is_none());
    assert!(job.prepared.check_generation().is_ok());
    assert_eq!(
        subtitle(Some(job), None),
        "Building a rough start · then g-xTB"
    );
    assert_eq!(
        app.status,
        "Ce (cerium) needs xTB: building a rough start, then refining with g-xTB…"
    );
    assert_eq!(job.prepared.ids(), atoms);
    assert_eq!(app.tab.selected, atoms);
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.history.undo_frames().count(), 0);
}

#[test]
fn cerium_generated_reply_starts_refinement_with_the_same_key_and_no_model() {
    if !xtb::available() {
        return;
    }
    let (mut app, atoms) = cerium();
    let before = app.tab.doc.clone();
    let key = start_refinement(&mut app);
    assert!(app.status.starts_with("Refining the 3D start with g-xTB"));
    let _ = app.update(Message::Models3d(Action::Refined(
        key,
        xtb::RunEvent::Steps {
            first: 11,
            energies: vec![-24., -25.],
            positions: START.to_vec(),
        },
    )));
    let note = subtitle(app.tab.model_job.as_ref(), None);
    assert!(note.starts_with("Refining the 3D start with g-xTB · step 12 · "));
    assert!(
        app.status
            .starts_with("Refining the 3D start with g-xTB · step 12 · ")
    );
    assert!(app.status.ends_with(" · Esc cancels"));
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.selected, atoms);
    assert!(app.tab.model_failure.is_none());
    assert_eq!(app.tab.history.undo_frames().count(), 0);
}

#[test]
fn refined_cerium_positions_are_applied_in_one_undo_step_with_the_method_and_start() {
    if !xtb::available() {
        return;
    }
    let (mut app, atoms) = cerium();
    let before = app.tab.doc.clone();
    let key = start_refinement(&mut app);
    refined(&mut app, key, Ok(report()));
    assert!(app.tab.model_job.is_none());
    assert!(app.tab.model_failure.is_none());
    assert!(!app.error, "{}", app.status);
    assert!(app.status.contains("Refined with g-xTB"), "{}", app.status);
    assert!(
        app.status.contains("start: Ce ligands spread out"),
        "{}",
        app.status
    );
    let [model] = app.tab.doc.models.as_slice() else {
        panic!("one refined model");
    };
    assert_eq!(model.display, Display::Both);
    assert_eq!(
        model.atoms.iter().map(|atom| atom.xyz).collect::<Vec<_>>(),
        REFINED.map(|xyz| xyz.map(|value| value as f32))
    );
    assert_eq!(app.tab.selected, atoms);
    assert_eq!(app.tab.doc.atoms, before.atoms);
    assert_eq!(app.tab.doc.bonds, before.bonds);
    assert_eq!(app.tab.history.undo_frames().count(), 1);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn failed_cerium_refinement_records_the_named_atom_without_applying_the_rough_start() {
    if !xtb::available() {
        return;
    }
    let (mut app, atoms) = cerium();
    let before = app.tab.doc.clone();
    let key = start_refinement(&mut app);
    refined(
        &mut app,
        key,
        Err(XtbFailure::Other("injected refinement failure".into())),
    );
    assert!(app.tab.model_job.is_none());
    assert!(app.error);
    assert_eq!(app.model_failure_atom(), Some(atoms[0]));
    let failure = app.tab.model_failure.as_ref().expect("a current failure");
    assert_eq!(failure.molecule, atoms);
    assert_eq!(failure.message, app.status);
    assert_eq!(
        app.status,
        "Can't build a starting 3D structure for Ce (cerium): g-xTB stopped: injected refinement failure."
    );
    assert_eq!(app.tab.selected, atoms);
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.history.undo_frames().count(), 0);
}

#[test]
fn canceling_refinement_discards_the_job_and_ignores_a_late_result() {
    if !xtb::available() {
        return;
    }
    let (mut app, atoms) = cerium();
    let before = app.tab.doc.clone();
    let key = start_refinement(&mut app);
    let _ = app.update(Message::Escape);
    assert!(app.tab.model_job.is_none());
    assert!(!app.error, "{}", app.status);
    assert!(app.tab.model_failure.is_none());
    assert_eq!(app.status, "3D model canceled · Drawing unchanged");
    assert_eq!(app.tab.selected, atoms);
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.history.undo_frames().count(), 0);
    refined(&mut app, key, Ok(report()));
    assert_eq!(
        app.tab.doc, before,
        "a late result cannot apply after cancellation"
    );
    assert_eq!(app.tab.history.undo_frames().count(), 0);
}
