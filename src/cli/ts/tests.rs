//! Deterministic TS CLI report checks; these never start an xTB process.

use super::{finish, output::Summary, worker};
use crate::cli::args::Ts;
use reshiki_optimize::ts::{IrcBranch, IrcReason, Minimum, Reached, TsReport};
use reshiki_xtb_worker::{
    client::Error,
    protocol::{Engine, Method, Units},
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
                finished: false,
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
                energies: vec![-1., -1.5, -3.],
                path: vec![point(-1.5, 2.5)],
                reason: IrcReason::Converged,
                path_length: 0.13,
                gradient_calls: 5,
                end: reactant,
                reached: Reached::Reactant,
            },
            IrcBranch {
                energies: vec![-1., -1.5, -2.],
                path: vec![point(-1.5, 3.5)],
                reason: IrcReason::Converged,
                path_length: 0.15,
                gradient_calls: 7,
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
        method: Method::Gfn2,
        refine: None,
        ts: None,
        from: None,
        reactant: Some("r.xyz".into()),
        product: Some("p.xyz".into()),
        form: Vec::new(),
        break_: Vec::new(),
        charge: 0,
        charge_explicit: false,
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
        reactive_start: None,
        source_report: None,
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

#[test]
fn reactive_start_json_preserves_units_and_one_based_pairs() {
    let directory = tempfile::tempdir().unwrap();
    let command = command(directory.path());
    let mut summary = summary(&command);
    summary.reactive_start = Some(reshiki_optimize::ts::ReactiveStart {
        pairs: vec![(0, 1)],
        before_angstrom: vec![4.72],
        after_angstrom: vec![3.05],
        energy_change_kcal: 2.1,
    });
    let (_, _, _, json) = finish_report(&summary);
    assert_eq!(
        json["reactive_start"],
        serde_json::json!({
            "pairs": [[1, 2]],
            "before_angstrom": [4.72],
            "after_angstrom": [3.05],
            "energy_change_kcal": 2.1,
        })
    );
}

#[test]
fn gxtb_report_labels_every_calculation_and_exports_a_verified_saddle() {
    let directory = tempfile::tempdir().unwrap();
    let mut command = command(directory.path());
    command.method = Method::Gxtb;
    let summary = summary(&command);
    let (success, table, _, json) = finish_report(&summary);
    assert!(success);
    assert!(table.starts_with("Transition state · g-xTB"), "{table}");
    assert_eq!(json["verified"], true);
    assert_eq!(json["settings"]["method"], "gxtb");
    assert_eq!(json["energies"]["method"], "gxtb");
    assert_eq!(json["barriers"]["method"], "gxtb");
    assert!(json["barriers"]["forward"].is_number());
    for method in json["methods"].as_object().unwrap().values() {
        assert_eq!(method, "gxtb");
    }
    for branch in ["plus", "minus"] {
        assert_eq!(json["irc"][branch]["method"], "gxtb");
    }
    let dir = command.out_dir.as_ref().unwrap();
    let stored: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("report.json")).unwrap()).unwrap();
    assert_eq!(stored, json);
    for file in [
        "reactant.xyz",
        "product.xyz",
        "ts.xyz",
        "path.xyz",
        "irc.xyz",
    ] {
        let text = std::fs::read_to_string(dir.join(file)).unwrap();
        assert!(
            text.lines()
                .skip(1)
                .step_by(4)
                .all(|line| line.contains("method=gxtb verification=verified")),
            "{file}: {text}"
        );
        assert!(!text.contains("Barrier"));
    }
}

#[test]
fn automatic_output_report_can_also_be_the_json_destination() {
    let directory = tempfile::tempdir().unwrap();
    let mut command = command(directory.path());
    command.json = Some(command.out_dir.as_ref().unwrap().join("report.json"));
    super::output::check_destinations(&command).unwrap();
    assert!(finish_report(&summary(&command)).0);
    let files: Vec<_> = std::fs::read_dir(command.out_dir.as_ref().unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(files.len(), 7);
}

#[test]
fn unverified_gxtb_candidates_never_report_or_export_a_barrier() {
    for failure in ["saddle", "imaginary", "worker", "irc"] {
        let directory = tempfile::tempdir().unwrap();
        let mut command = command(directory.path());
        command.refine = Some(Method::Gxtb);
        command.ts = Some("ts.xyz".into());
        let mut summary = summary(&command);
        match failure {
            "saddle" => summary.report.saddle_converged = false,
            "imaginary" => summary.report.imaginary = 2,
            "worker" => summary.worker_check = Err("SCF failed".into()),
            "irc" => summary.report.connects = false,
            _ => unreachable!(),
        }
        let (success, table, _, json) = finish_report(&summary);
        assert!(!success, "{failure}");
        let barrier = table
            .lines()
            .find(|line| line.starts_with("Barrier"))
            .unwrap();
        assert_eq!(
            barrier,
            "Barrier       unavailable · g-xTB candidate is unverified"
        );
        assert_eq!(json["barriers"]["method"], "gxtb");
        assert_eq!(json["barriers"]["verified"], false);
        assert!(json["barriers"]["forward"].is_null());
        assert!(json["barriers"]["reverse"].is_null());
        assert_eq!(json["operation"], "refine");
        assert_eq!(json["neb"]["performed"], false);
        for file in [
            "reactant.xyz",
            "product.xyz",
            "ts.xyz",
            "path.xyz",
            "irc.xyz",
        ] {
            let text =
                std::fs::read_to_string(command.out_dir.as_ref().unwrap().join(file)).unwrap();
            assert!(
                text.contains("method=gxtb verification=unverified-candidate"),
                "{failure}/{file}"
            );
            assert!(!text.contains("Barrier"));
        }
    }
}

fn from_command(dir: &std::path::Path) -> Ts {
    let mut command = command(dir);
    command.reactant = None;
    command.product = None;
    command.refine = Some(Method::Gxtb);
    command.from = Some(dir.join("structures"));
    command.out_dir = Some(dir.join("refined"));
    command.json = None;
    command
}

#[test]
fn from_requires_verified_gfn2_provenance_and_preserves_the_source() {
    let directory = tempfile::tempdir().unwrap();
    let original = command(directory.path());
    let mut original_summary = summary(&original);
    // A short trace can still connect after relaxation: completed is diagnostic.
    original_summary.report.irc.as_mut().unwrap()[0].reason = IrcReason::LengthCap;
    let (_, table, _, original_json) = finish_report(&original_summary);
    assert_eq!(
        table.lines().find(|line| line.starts_with("IRC ")),
        Some("IRC           connects reactant and product")
    );
    for (mode, reason, calls, length, converged) in [
        ("plus", "length cap", 5, 0.13, false),
        ("minus", "converged", 7, 0.15, true),
    ] {
        let branch = &original_json["irc"][mode];
        assert_eq!(branch["completed"], converged);
        assert_eq!(branch["stop_reason"], reason);
        assert_eq!(branch["gradient_calls"], calls);
        assert_eq!(branch["path_length_sqrt_amu_bohr"], length);
        assert_eq!(branch["steps"], 1);
    }
    let dir = original.out_dir.as_ref().unwrap();
    let source_ts = std::fs::read(dir.join("ts.xyz")).unwrap();
    let mut refine = from_command(directory.path());
    let source = super::source::load(&mut refine)
        .unwrap()
        .unwrap()
        .report
        .unwrap();
    assert_eq!(source, original_json);
    assert_eq!(refine.charge, original.charge);
    assert_eq!(refine.uhf, Some(1));
    assert!(refine.ts.is_none());
    super::output::check_destinations(&refine).unwrap();

    let mut refined = summary(&refine);
    refined.source_report = Some(&source);
    refined.report.saddle_converged = false;
    let (mut out, mut err) = (Vec::new(), Vec::new());
    assert!(!finish(&refined, &mut out, &mut err).unwrap());
    let table = String::from_utf8(out).unwrap();
    assert!(table.starts_with("Retained GFN2-xTB · forward"), "{table}");
    assert!(table.contains("Barrier       unavailable"), "{table}");
    assert_eq!(std::fs::read(dir.join("ts.xyz")).unwrap(), source_ts);
    let retained: serde_json::Value = serde_json::from_slice(
        &std::fs::read(refine.out_dir.as_ref().unwrap().join("report.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(retained["source_result"], original_json);

    let mut clobber = from_command(directory.path());
    clobber.out_dir = Some(dir.clone());
    assert!(
        super::output::check_destinations(&clobber)
            .unwrap_err()
            .contains("already exists")
    );
}

#[test]
fn from_rejects_legacy_unverified_mixed_method_and_modified_geometry_sources() {
    for failure in [
        "legacy",
        "gxtb",
        "worker",
        "irc",
        "mixed-method",
        "energy-method",
        "barrier-method",
        "structure-method",
        "branch-method",
        "barrier-energy",
        "missing-null-proof",
        "scalar-report",
        "geometry",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let command = command(directory.path());
        let (_, _, _, mut json) = finish_report(&summary(&command));
        let dir = command.out_dir.as_ref().unwrap();
        match failure {
            "legacy" => {
                json.as_object_mut().unwrap().remove("schema_version");
            }
            "gxtb" => json["settings"]["method"] = "gxtb".into(),
            "worker" => json["worker_imaginary_count"] = 0.into(),
            "irc" => json["irc"]["minus"]["reached"] = "Reactant".into(),
            "mixed-method" => json["methods"]["irc"] = "gxtb".into(),
            "energy-method" => json["energies"]["method"] = "gxtb".into(),
            "barrier-method" => json["barriers"]["method"] = "gxtb".into(),
            "structure-method" => json["structures"]["method"] = "gxtb".into(),
            "branch-method" => json["irc"]["plus"]["method"] = "gxtb".into(),
            "barrier-energy" => json["barriers"]["forward"] = 1234.into(),
            "missing-null-proof" => {
                json.as_object_mut().unwrap().remove("failure_reason");
            }
            "scalar-report" => json = "not a report".into(),
            "geometry" => {
                std::fs::write(dir.join("ts.xyz"), "2\nmodified\nC 0 0 0\nH 0 0 9\n").unwrap()
            }
            _ => unreachable!(),
        }
        std::fs::write(dir.join("report.json"), json.to_string()).unwrap();
        let error = super::source::load(&mut from_command(directory.path()))
            .err()
            .unwrap();
        assert!(error.contains("--ts TS.xyz"), "{failure}: {error}");
    }
    let directory = tempfile::tempdir().unwrap();
    let error = super::source::load(&mut from_command(directory.path()))
        .err()
        .unwrap();
    assert!(error.contains("reactant.xyz"), "{error}");
}

#[test]
fn legacy_directory_without_a_report_is_only_a_geometry_seed() {
    let directory = tempfile::tempdir().unwrap();
    let original = command(directory.path());
    finish_report(&summary(&original));
    let dir = original.out_dir.as_ref().unwrap();
    std::fs::remove_file(dir.join("report.json")).unwrap();
    let mut refine = from_command(directory.path());
    refine.charge = -1;
    refine.charge_explicit = true;
    refine.uhf = Some(2);
    assert!(
        super::source::load(&mut refine)
            .unwrap()
            .unwrap()
            .report
            .is_none()
    );
    assert_eq!(refine.charge, -1);
    assert_eq!(refine.uhf, Some(2));
    assert!(refine.ts.is_none());
    let mut refined = summary(&refine);
    refined.report.saddle_converged = false;
    refined.write_files().unwrap();
    let json: serde_json::Value = serde_json::from_slice(
        &std::fs::read(refine.out_dir.as_ref().unwrap().join("report.json")).unwrap(),
    )
    .unwrap();
    assert!(json["source_result"].is_null());
    assert_eq!(json["source_seed"]["geometry_only"], true);
    assert!(json["source_seed"]["method"].is_null());
    assert!(json["barriers"]["forward"].is_null());
}

#[test]
fn verified_source_charge_and_spin_cannot_be_changed() {
    let directory = tempfile::tempdir().unwrap();
    let original = command(directory.path());
    finish_report(&summary(&original));
    for (charge, uhf) in [(1, None), (0, Some(2))] {
        let mut refine = from_command(directory.path());
        refine.charge = charge;
        refine.charge_explicit = true;
        refine.uhf = uhf;
        let error = super::source::load(&mut refine).err().unwrap();
        assert!(error.contains("disagrees"), "{error}");
    }
}

#[test]
fn from_keeps_admitted_geometries_when_the_source_directory_is_replaced() {
    for verified in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let original = command(directory.path());
        let (_, _, _, report) = finish_report(&summary(&original));
        let dir = original.out_dir.as_ref().unwrap();
        if !verified {
            std::fs::remove_file(dir.join("report.json")).unwrap();
        }
        let loaded = super::source::load(&mut from_command(directory.path()))
            .unwrap()
            .unwrap();
        std::fs::rename(dir, directory.path().join("admitted")).unwrap();
        let mut replacement = command(directory.path());
        replacement.json = None;
        let mut replacement = summary(&replacement);
        for geometry in [
            &mut replacement.report.reactant,
            &mut replacement.report.product,
            &mut replacement.report.ts,
        ] {
            geometry.positions[5] += 10.;
        }
        replacement.write_files().unwrap();

        assert_eq!(loaded.report, verified.then_some(report.clone()));
        for (name, geometry) in ["reactant", "product", "ts"]
            .into_iter()
            .zip(&loaded.geometries)
        {
            assert_eq!(geometry.numbers, [6, 1]);
            let expected: Vec<f64> =
                serde_json::from_value(report["structures"][name].clone()).unwrap();
            assert!(
                super::bohr(geometry)
                    .iter()
                    .zip(expected)
                    .all(|(a, b)| (a - b).abs() < 1e-8)
            );
            let replacement = super::geometry::endpoint(&dir.join(format!("{name}.xyz"))).unwrap();
            assert_ne!(geometry.positions, replacement.positions);
        }
    }
}

#[test]
fn from_rejects_oversized_and_non_regular_reports() {
    let directory = tempfile::tempdir().unwrap();
    let dir = directory.path().join("structures");
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("report.json");
    let limit = reshiki_agent::ops::budget::Budgets::default().max_text_bytes;
    std::fs::File::create(&path)
        .unwrap()
        .set_len(limit as u64 + 1)
        .unwrap();
    let error = super::source::load(&mut from_command(directory.path()))
        .err()
        .unwrap();
    assert!(
        error.contains(&format!("input exceeds {limit} bytes")),
        "{error}"
    );
    assert!(error.contains("--ts TS.xyz"), "{error}");

    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    let error = super::source::load(&mut from_command(directory.path()))
        .err()
        .unwrap();
    assert!(error.contains("not a regular file"), "{error}");
    assert!(error.contains("--ts TS.xyz"), "{error}");
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
    assert_eq!(json.get("reactive_start"), Some(&serde_json::Value::Null));
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
    assert_eq!(
        irc.lines().nth(1),
        Some("TS E=-1.00000000 Eh method=gfn2 verification=unverified-candidate")
    );
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
