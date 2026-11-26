//! Deterministic TS CLI report checks; these never start an xTB process.

use super::{finish, output::Summary, worker};
use crate::cli::args::Ts;
use reshiki_optimize::ts::{IrcBranch, Minimum, Reached, TsReport};
use reshiki_xtb_worker::{
    client::Error,
    protocol::{Engine, Units},
};

#[test]
fn trace_requires_an_explicit_value() {
    for value in [
        None,
        Some(""),
        Some("0"),
        Some("false"),
        Some("TRUE"),
        Some(" 1"),
    ] {
        assert!(!super::trace_enabled(value), "{value:?}");
    }
    for value in ["1", "true"] {
        assert!(super::trace_enabled(Some(value)), "{value}");
    }
}

#[test]
fn trace_distinguishes_climbing_image_from_a_higher_endpoint() {
    use reshiki_optimize::ts::Event;

    let mut progress = super::Progress {
        trace: true,
        ..Default::default()
    };
    let mut output = Vec::new();
    for climbing in [None, Some(2)] {
        let _ = progress.show(
            Event::Neb {
                iteration: 0,
                energies: &[5., 1., 3., 2., 0.],
                climbing,
                max_force: 0.,
            },
            &mut output,
        );
    }
    let output = String::from_utf8(output).unwrap();
    let mut lines = output.lines();
    let pending = lines.next().unwrap();
    assert!(
        pending.contains("band_peak_energy=5.000000000000 climbing_image_energy=pending"),
        "{pending}"
    );
    let climbing = lines.next().unwrap();
    assert!(
        climbing.contains("band_peak_energy=5.000000000000 climbing_image_energy=3.000000000000"),
        "{climbing}"
    );
}

#[test]
fn force_exception_trace_preserves_both_ratios() {
    use reshiki_optimize::ts::Event;

    let mut progress = super::Progress {
        trace: true,
        ..Default::default()
    };
    let mut output = Vec::new();
    let _ = progress.show(
        Event::SaddleTrial {
            energy: 0.0,
            max_gradient: 0.0,
            rms_gradient: 0.0,
            length: 0.0028,
            ratio: 0.988862,
            energy_ratio: -3.975,
            force_ratio: 0.988862,
            decision: "accept: projected force",
        },
        &mut output,
    );
    let output = String::from_utf8(output).unwrap();
    assert!(
        output.contains("energy_ratio=-3.975000 force_ratio=0.988862"),
        "{output}"
    );
}

fn point(energy: f64, distance: f64) -> Minimum {
    Minimum {
        energy,
        positions: vec![0., 0., 0., 0., 0., distance],
    }
}

fn report() -> TsReport {
    let reactant = point(-3., 2.);
    let product = point(-2., 4.);
    let ts = point(-1., 3.);
    TsReport {
        reactant: reactant.clone(),
        product: product.clone(),
        path: vec![reactant.clone(), ts.clone(), product.clone()],
        neb_converged: true,
        climbing_image: 1,
        ts,
        saddle_converged: true,
        saddle_reason: reshiki_optimize::ts::SaddleReason::Stationary,
        saddle_diagnostics: Vec::new(),
        frequencies_cm1: vec![-50.1],
        imaginary: 1,
        hessian: vec![0.25; 36],
        transition_mode: vec![0., 0., -1., 0., 0., 1.],
        irc: Some([
            IrcBranch {
                energies: vec![-1., -3.],
                path: vec![point(-1.5, 2.5)],
                completed: true,
                end: reactant,
                reached: Reached::Reactant,
            },
            IrcBranch {
                energies: vec![-1., -2.],
                path: vec![point(-1.5, 3.5)],
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

fn command(directory: &std::path::Path) -> Ts {
    Ts {
        reactant: Some("r.xyz".into()),
        product: Some("p.xyz".into()),
        form: Vec::new(),
        break_: Vec::new(),
        charge: 0,
        uhf: None,
        images: 5,
        workers: 1,
        remap_hydrogens: false,
        keep_endpoints: false,
        out_dir: Some(directory.join("structures")),
        json: Some(directory.join("report.json")),
    }
}

fn summary<'a>(command: &'a Ts) -> Summary<'a> {
    Summary {
        command,
        numbers: &[6, 1],
        masses: &[12.011, 1.008],
        report: report(),
        engine: Engine {
            name: "axtb".into(),
            version: "test".into(),
            license: "test".into(),
            source: "test".into(),
            rev: "test".into(),
        },
        uhf: 1,
        worker_check: Ok(worker::HessianAnalysis {
            frequencies_cm1: vec![-50.1],
            hessian: vec![0.5; 36],
        }),
        iterations: 2,
        steps: 3,
        evaluations: 12,
        seconds: 0.,
        moving_hydrogens: Vec::new(),
    }
}

fn finish_report(summary: &Summary<'_>) -> (bool, String, String, serde_json::Value) {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let success = finish(summary, &mut out, &mut err).unwrap();
    let json =
        serde_json::from_slice(&std::fs::read(summary.command.json.as_ref().unwrap()).unwrap())
            .unwrap();
    (
        success,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
        json,
    )
}

pub(in crate::cli) fn worker_failure_keeps_characterized_candidate_and_files() {
    let directory = tempfile::tempdir().unwrap();
    let command = command(directory.path());
    let mut summary = summary(&command);
    summary.worker_check = Err("SCF failed during the final Hessian".into());
    let (success, table, stderr, json) = finish_report(&summary);
    assert!(!success);
    let reason = "worker check failed: SCF failed during the final Hessian";
    assert!(table.starts_with("Transition state · GFN2-xTB"), "{table}");
    assert!(
        table
            .lines()
            .find(|line| line.starts_with("Imaginary"))
            .unwrap()
            .contains(reason),
        "{table}"
    );
    assert_eq!(stderr, format!("reshiki: {reason}\n"));
    assert_eq!(json["energies"]["ts"], -1.);
    assert_eq!(json["frequencies_cm1"], serde_json::json!([-50.1]));
    assert_eq!(json["imaginary_count"], 1);
    assert!(json["worker_frequencies_cm1"].is_null());
    assert!(json["worker_imaginary_count"].is_null());
    assert_eq!(json["worker_check_error"], reason);
    assert_eq!(json["failure_reason"], reason);
    assert_eq!(json["hessian_source"], "engine");
    assert_eq!(json["irc"]["plus"]["completed"], true);
    for name in [
        "reactant.xyz",
        "product.xyz",
        "ts.xyz",
        "path.xyz",
        "irc.xyz",
        "ts.hess",
    ] {
        let path = command.out_dir.as_ref().unwrap().join(name);
        assert!(
            !std::fs::read(&path).unwrap().is_empty(),
            "{}",
            path.display()
        );
    }
    let hessian =
        std::fs::read_to_string(command.out_dir.as_ref().unwrap().join("ts.hess")).unwrap();
    assert!(hessian.contains("$hessian\n6\n"), "{hessian}");
    assert!(hessian.contains("2.5000000000E-01"), "{hessian}");
    assert!(!hessian.contains("5.0000000000E-01"), "{hessian}");
}

#[test]
fn ceiling_reason_and_optional_probe_failure_reach_cli_report() {
    let directory = tempfile::tempdir().unwrap();
    let command = command(directory.path());
    let mut summary = summary(&command);
    summary.report.saddle_converged = false;
    summary.report.saddle_reason = reshiki_optimize::ts::SaddleReason::EnergyCeiling;
    let diagnostic = "left midpoint recovery probe: SCF failure: iteration limit";
    summary.report.saddle_diagnostics.push(diagnostic.into());
    summary.report.irc = None;
    summary.report.connects = false;

    let (success, table, stderr, json) = finish_report(&summary);
    assert!(!success);
    assert!(table.contains("energy ceiling exhausted"));
    assert!(stderr.contains("energy ceiling exhausted") && stderr.contains(diagnostic));
    assert_eq!(json["saddle"]["reason"], "energy ceiling exhausted");
    assert_eq!(
        json["saddle"]["diagnostics"],
        serde_json::json!([diagnostic])
    );
    assert_eq!(
        json["failure_reason"],
        "saddle refinement did not converge (energy ceiling exhausted)"
    );
}

pub(in crate::cli) fn worker_count_disagreement_fails_with_diagnostics() {
    let directory = tempfile::tempdir().unwrap();
    let command = command(directory.path());
    let mut summary = summary(&command);
    summary.worker_check.as_mut().unwrap().frequencies_cm1 = vec![-49.9];
    let (success, table, stderr, json) = finish_report(&summary);
    assert!(!success);
    let reason = "engine/worker imaginary frequency counts disagree: engine 1, worker 0";
    assert!(table.contains(reason), "{table}");
    assert_eq!(stderr, format!("reshiki: {reason}\n"));
    assert_eq!(json["worker_frequencies_cm1"], serde_json::json!([-49.9]));
    assert_eq!(json["worker_imaginary_count"], 0);
    assert_eq!(json["worker_check_error"], reason);
    assert_eq!(json["failure_reason"], reason);
    assert_eq!(json["hessian_source"], "worker");
    assert_eq!(json["connects"], true);

    let directory = tempfile::tempdir().unwrap();
    let command = self::command(directory.path());
    let summary = self::summary(&command);
    let (success, _, stderr, json) = finish_report(&summary);
    assert!(success);
    assert!(stderr.is_empty());
    assert_eq!(json["worker_imaginary_count"], 1);
    assert!(json["worker_check_error"].is_null());
    assert!(json["failure_reason"].is_null());
}

pub(in crate::cli) fn irc_failure_preserves_candidate_and_explains_missing_branches() {
    let directory = tempfile::tempdir().unwrap();
    let command = command(directory.path());
    let mut summary = summary(&command);
    summary.report.connects = false;
    summary.report.irc = None;
    summary.report.irc_error = Some("-mode relaxation reached its step limit".into());
    let (success, table, stderr, json) = finish_report(&summary);
    assert!(!success);
    assert!(
        table.contains("failed: -mode relaxation reached its step limit"),
        "{table}"
    );
    assert!(!table.contains("skipped"), "{table}");
    assert_eq!(
        stderr,
        "reshiki: IRC failed: -mode relaxation reached its step limit\n"
    );
    assert_eq!(json["irc_error"], "-mode relaxation reached its step limit");
    assert_eq!(
        json["failure_reason"],
        "IRC failed: -mode relaxation reached its step limit"
    );
    assert_eq!(json["energies"]["ts"], -1.);
    assert_eq!(json["frequencies_cm1"], serde_json::json!([-50.1]));
    assert!(json["irc"].is_null());
    let irc = std::fs::read_to_string(command.out_dir.as_ref().unwrap().join("irc.xyz")).unwrap();
    assert_eq!(irc.lines().count(), 4);
    assert_eq!(irc.lines().nth(1), Some("TS E=-1.00000000 Eh"));
}

pub(in crate::cli) fn worker_hessian_uses_engine_masses_and_explicit_symmetry() {
    let positions = [0., 0., 0., 0., 0., 3.];
    let masses: Vec<_> = [3_u8, 17]
        .iter()
        .map(|&z| crate::chemistry::ELEMENTS[usize::from(z)].average_mass_amu())
        .collect();
    let (requests, incoming) = std::sync::mpsc::channel();
    let (outgoing, results) = std::sync::mpsc::channel();
    let mut worker = worker::Worker::with_transport(requests, results);
    let request = std::thread::scope(|scope| {
        let capture = scope.spawn(move || {
            let request = incoming.recv().unwrap();
            outgoing
                .send(Err(Error::Unsupported(
                    "captured final Hessian request".into(),
                )))
                .unwrap();
            request
        });
        assert!(worker.check_hessian(&positions, &masses).is_err());
        capture.join().unwrap()
    });
    assert_eq!(request.positions.unwrap(), [[0., 0., 0.], [0., 0., 3.]]);
    assert!(matches!(request.units, Units::Bohr));
    let hessian = request.hessian.unwrap();
    assert_eq!(hessian.masses_amu, Some(masses));
    assert_eq!(hessian.thermo.symmetry_number, Some(1));
}
