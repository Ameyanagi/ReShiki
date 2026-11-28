use super::super::tests::completion;
use super::*;

#[test]
fn local_changes_follow_observed_endpoints_instead_of_global_edits() {
    let (_, mut done) = completion();
    done.report.reactant.positions = vec![0., 0., 0., 0., 0., 1.4];
    done.report.product.positions = vec![0., 0., 0., 0., 0., 5.];
    let changes = local_changes(&[1, 1], &done.report).unwrap();
    assert!(changes.form.is_empty());
    assert_eq!(changes.break_, [(0, 1)]);
}

#[test]
fn rebound_local_edge_retains_irc_without_relabeling_the_global_neb() {
    let (launch, mut done) = completion();
    done.report.reactant.positions[5] = 2.25;
    let reactant = done.report.reactant.clone();
    done.report.irc.as_mut().unwrap()[0].end = reactant;
    let report = complete_with_thermo(&launch, done, false).unwrap();
    assert_eq!(report.result.status, qcdata::Status::Converged);
    assert!(report.result.trajectory.is_none());
    assert!(report.extras.image_positions.is_empty());
    assert!(
        report
            .extras
            .irc
            .as_ref()
            .unwrap()
            .neb_arc_lengths_sqrt_amu_bohr
            .is_empty()
    );
    assert_eq!(
        report.frames[1].positions_angstrom[1][2],
        2.25 * ANGSTROM_PER_BOHR
    );
}

#[test]
fn minimum_correction_reuses_the_checked_spectrum_and_minimum_frequency_rule() {
    let (launch, mut done) = completion();
    let computed = done.checked.as_mut().unwrap();
    computed.vibrations.as_mut().unwrap().frequencies_cm1 = vec![-25.];
    assert!(
        minimum_thermochemistry(
            &launch.frame,
            &done.report.reactant.positions,
            &done.masses,
            computed,
        )
        .is_none()
    );
    computed.vibrations.as_mut().unwrap().frequencies_cm1 = vec![150.];
    assert!(
        minimum_thermochemistry(
            &launch.frame,
            &done.report.reactant.positions,
            &done.masses,
            computed,
        )
        .is_some_and(f64::is_finite)
    );
}

fn discovery_report(report: &TsReport) -> discovery::Report<()> {
    discovery::Report {
        cancelled: false,
        error: None,
        minima: vec![report.reactant.clone(), report.product.clone()],
        order: vec![0, 1],
        edges: Vec::new(),
        gaps: vec![discovery::Gap {
            reactant: 0,
            product: 1,
            reason: "both IRC branches reach the same minimum; no progress".into(),
            attempt: Some(0),
        }],
        attempts: Vec::new(),
        diagnostics: Vec::new(),
    }
}

#[test]
fn unresolved_single_step_cannot_keep_the_loose_original_connection_or_barrier() {
    let (launch, done) = completion();
    let found = discovery_report(&done.report);
    let mut report = complete_with_thermo(&launch, done, false).unwrap();
    assert_eq!(report.result.status, qcdata::Status::Converged);
    assert!(report.extras.connects);
    apply_discovery_verification(&mut report, &found);
    assert!(
        matches!(&report.result.status, qcdata::Status::NotConverged { reason }
        if reason.contains("same minimum"))
    );
    assert!(!report.extras.connects);
    assert!(report.discovered.is_none());
}

#[test]
fn fatal_discovery_error_is_retained_without_additional_minima() {
    let (launch, done) = completion();
    let mut found = discovery_report(&done.report);
    found.error = Some(ts::TsError::Failed("native session disconnected".into()));
    let mut report = complete_with_thermo(&launch, done, false).unwrap();
    apply_discovery_verification(&mut report, &found);
    assert!(
        matches!(&report.result.status, qcdata::Status::NotConverged { reason }
        if reason.contains("native session disconnected"))
    );
    assert!(
        report
            .extras
            .diagnostics
            .iter()
            .any(|(_, text)| text.contains("native session disconnected"))
    );
    assert!(!report.extras.connects);
    assert!(report.discovered.is_none());
}

fn request() -> crate::app::xtb::ts::Request {
    crate::app::xtb::ts::Request {
        method: qcdata::XtbMethod::Gfn2Xtb,
        arrow: 1,
        free_energy: false,
        bonds: crate::app::xtb::ts::card::BondChanges(qcdata::TransitionStateMapping {
            formed: Vec::new(),
            broken: Vec::new(),
            hydrogen_moves: Vec::new(),
            automatic: true,
        }),
        context: qcdata::TransitionStateContext {
            reactant_chemistry: "reactant".into(),
            product_chemistry: "product".into(),
            title: "Single step".into(),
            reactant: "Reactant".into(),
            endpoint_complex: Some(false),
        },
        plan: TsPlan {
            start: Side::Reactant,
            start_atoms: vec![20, 90],
            mapped: Vec::new(),
            mapped_hydrogens: Vec::new(),
            form: Vec::new(),
            break_: Vec::new(),
            hydrogen_moves: Vec::new(),
            shape: (1, 1),
            drawn_charges: (0, 0),
            atoms: 2,
        },
        images: 5,
        retry: None,
    }
}

#[test]
fn exact_single_step_uses_the_planners_corrected_irc_and_worker_acceptance() {
    let (launch, mut done) = completion();
    done.report.connects = false;
    for branch in done.report.irc.as_mut().unwrap() {
        branch.reached = ts::Reached::Other;
        branch.path = vec![ts::Minimum {
            energy: -1.5,
            positions: branch.end.positions.clone(),
        }];
    }
    let (tx, _rx) = mpsc::unbounded();
    let report =
        complete_scheme(&launch, &request(), done, &mut [], &Cancel::default(), &tx).unwrap();
    assert_eq!(report.result.status, qcdata::Status::Converged);
    assert!(report.extras.connects);
    assert!(report.discovered.is_none());
    assert!(report.result.trajectory.is_some());
    assert_eq!(
        report.extras.irc.as_ref().unwrap().reached,
        [qcdata::IrcEndpoint::Reactant, qcdata::IrcEndpoint::Product]
    );
}

#[test]
fn canonicalization_checks_cancellation_before_any_worker_evaluation() {
    let (launch, done) = completion();
    let cancel = Cancel::default();
    cancel.cancel();
    let (tx, _rx) = mpsc::unbounded();
    let mut backend = Native {
        workers: &mut [],
        frame: &launch.frame,
        numbers: &launch.frame.numbers,
        masses: &done.masses,
        method: done.method,
        images: 5,
        cancel: &cancel,
        tx: &tx,
        free: false,
        minimum_thermo: Vec::new(),
    };
    assert_eq!(
        discovery::Backend::canonicalize(&mut backend, &done.report.reactant),
        Err(ts::TsError::Cancelled)
    );
}
