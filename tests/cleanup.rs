//! Full original cleanup responses, using only independently captured layouts.
use anyhow::Context;
use reshiki::{
    chemistry::{cleanup, stereo::Point3},
    cleanup::Options,
    document::{Document, History},
    engine::{Response, native_response},
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Command, Stdio},
    sync::Arc,
};

#[derive(Deserialize)]
struct Case {
    name: String,
    document: Document,
    options: Options,
    selected: Vec<u64>,
    layouts: Vec<Layout>,
    expected: Option<Value>,
    error: Option<String>,
    analysis_positions: Vec<Point3>,
}
#[derive(Deserialize)]
struct Layout {
    ids: Vec<u64>,
    old: Vec<Point3>,
    fixed: Value,
    bond_length: f64,
    canonical_orientation: bool,
    use_ring_templates: bool,
    force_rdkit: bool,
    positions: Vec<Point3>,
}

fn compare(actual: Response, expected: Response) -> anyhow::Result<()> {
    if let (Some(actual), Some(expected)) = (&actual.document, &expected.document) {
        for (a, e) in actual.atoms.iter().zip(&expected.atoms) {
            anyhow::ensure!(
                a.position.x.to_bits() == e.position.x.to_bits()
                    && a.position.y.to_bits() == e.position.y.to_bits(),
                "Drawing coordinate bits changed for atom {}",
                a.id
            );
        }
    }
    let mut actual = serde_json::to_value(actual)?;
    let mut expected = serde_json::to_value(expected)?;
    for field in ["mass", "exact_mass", "logp", "tpsa"] {
        if let (Some(a), Some(e)) = (
            actual["analysis"][field].as_f64(),
            expected["analysis"][field].as_f64(),
        ) {
            anyhow::ensure!(
                (a - e).abs() <= e.abs().max(1.) * 1e-12,
                "{field}: {a} != {e}"
            );
            actual["analysis"][field] = Value::Null;
            expected["analysis"][field] = Value::Null;
        }
    }
    anyhow::ensure!(actual == expected, "Actual: {actual}\nExpected: {expected}");
    Ok(())
}

#[tokio::test]
async fn complete_cleanup_matches_original_worker_and_captured_layouts() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let helper = root.join("artifacts/inchi-helper").join(if cfg!(windows) {
        "reshiki-inchi-helper.exe"
    } else {
        "reshiki-inchi-helper"
    });
    if !helper.is_file() {
        anyhow::ensure!(
            std::env::var_os("RESHIKI_REQUIRE_INCHI_HELPER").is_none(),
            "Build the native helper first"
        );
        return Ok(());
    }
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/cleanup_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let output = child.stdout.take().context("Missing oracle output")?;
    let mut count = 0;
    let mut rejected = 0;
    let mut failures = Vec::new();
    for line in BufReader::new(output).lines() {
        // Actual worker response transport parses JSON as f64 Value first.
        let case: Case = serde_json::from_value(serde_json::from_str(&line?)?)?;
        let source = case.document.clone();
        let prepared = cleanup::prepare(&case.document, case.options, &case.selected);
        let result = match prepared {
            Ok(prepared) => {
                for (request, reference) in prepared.requests().zip(&case.layouts) {
                    anyhow::ensure!(request.molecule.ids == reference.ids, "{} IDs", case.name);
                    anyhow::ensure!(
                        serde_json::to_value(&request.molecule.positions)?
                            == serde_json::to_value(&reference.old)?,
                        "{} old positions",
                        case.name
                    );
                    anyhow::ensure!(
                        serde_json::to_value(request.fixed)? == reference.fixed,
                        "{} fixed",
                        case.name
                    );
                    anyhow::ensure!(
                        request.bond_length == reference.bond_length
                            && request.canonical_orientation == reference.canonical_orientation
                            && request.use_ring_templates == reference.use_ring_templates
                            && request.force_rdkit == reference.force_rdkit,
                        "{} options",
                        case.name
                    );
                }
                prepared.finish(case.layouts.into_iter().map(|r| r.positions).collect())
            }
            Err(error) => Err(error),
        };
        match (result, case.expected) {
            (Ok(cleaned), Some(expected)) => {
                anyhow::ensure!(
                    cleaned.molecule.is_none() == cleaned.analysis_failure.is_some(),
                    "{} lost the typed partial-analysis cause",
                    case.name
                );
                if let Some(molecule) = &cleaned.molecule {
                    anyhow::ensure!(molecule.positions.len() == case.analysis_positions.len());
                    for (index, (a, e)) in molecule
                        .positions
                        .iter()
                        .zip(&case.analysis_positions)
                        .enumerate()
                    {
                        anyhow::ensure!(
                            a.x.to_bits() == e.x.to_bits()
                                && a.y.to_bits() == e.y.to_bits()
                                && a.z.to_bits() == e.z.to_bits(),
                            "{} full-precision analysis position {index}: {a:?} != {e:?}",
                            case.name
                        );
                    }
                }
                let analysis = if let Some(molecule) = cleaned.molecule {
                    Some(
                        native_response::analyze_prepared(
                            Arc::new(molecule),
                            Some(native_response::Config::new(helper.clone())),
                        )
                        .await?,
                    )
                } else {
                    None
                };
                let actual = Response {
                    document: Some(cleaned.document.clone()),
                    analysis,
                    output: None,
                    engine_version: reshiki::chemistry::RDKIT_VERSION.into(),
                    warnings: cleaned.warnings,
                };
                let mut expected: Response = serde_json::from_value(expected)?;
                if case.name.starts_with("partial-warning/element/") {
                    // Deliberate diagnostic difference: Rust must not fabricate
                    // RDKit's C++ assertion header, source line or Boost version.
                    // Every other response field remains an exact comparison.
                    anyhow::ensure!(
                        expected.analysis.is_none()
                            && expected.warnings.len() == 1
                            && expected.warnings.first().is_some_and(|w| w
                                .contains("Post-condition Violation\n\tElement 'Xx' not found"))
                    );
                    let warning = "Selected geometry cleaned. Another part of the drawing needs checking: Unknown element Xx";
                    anyhow::ensure!(actual.warnings == [warning]);
                    expected.warnings = vec![warning.into()];
                }
                if case.name.starts_with("partial-warning/stereo-references/") {
                    // The second precise assertion-only diagnostic change:
                    // retain the checked Rust cause instead of Bond.cpp details.
                    anyhow::ensure!(expected.analysis.is_none() && expected.warnings.len()==1 && expected.warnings.first().is_some_and(|w|w.contains("Pre-condition Violation\n\tbgnIdx not connected to begin atom of bond")));
                    let warning = "Selected geometry cleaned. Another part of the drawing needs checking: Bond stereo references must be attached to the corresponding endpoints";
                    anyhow::ensure!(actual.warnings == [warning]);
                    expected.warnings = vec![warning.into()];
                }
                if let Err(error) = compare(actual, expected) {
                    failures.push(format!("{}: {error:#}", case.name));
                }
                let mut history = History::default();
                let mut restored = cleaned.document;
                history.commit(source.clone(), &restored);
                history.undo(&mut restored);
                anyhow::ensure!(restored == source, "{} undo changed", case.name);
                count += 1;
            }
            (Err(error), None) => {
                if let Some(expected) = case
                    .error
                    .as_deref()
                    .filter(|e| e.starts_with("Cleanup ") || e.starts_with("Select atoms "))
                {
                    anyhow::ensure!(
                        error.to_string() == expected,
                        "{}: {error} != {expected}",
                        case.name
                    );
                }
                rejected += 1;
            }
            (actual, expected) => failures.push(format!(
                "{} outcome: {actual:?}, expected {expected:?}, error {:?}",
                case.name, case.error
            )),
        }
        anyhow::ensure!(case.document == source, "Source changed");
    }
    let status = child.wait()?;
    if !failures.is_empty() {
        std::fs::write(
            root.join("artifacts/cleanup-failures.log"),
            failures.join("\n"),
        )?;
    }
    println!("Cleanup: {count} complete responses, {rejected} matching rejections");
    anyhow::ensure!(status.success(), "Original cleanup oracle failed");
    anyhow::ensure!(
        failures.is_empty(),
        "{} cleanup differences; see artifacts/cleanup-failures.log",
        failures.len()
    );
    Ok(())
}

#[tokio::test]
async fn malformed_layouts_fixed_atoms_and_helper_failures_are_atomic() -> anyhow::Result<()> {
    use reshiki::{cleanup::Scope, document::Point};
    let mut document = Document::default();
    let a = document.add_atom("C", Point::new(0., 0.));
    let b = document.add_atom("C", Point::new(42., 0.));
    let c = document.add_atom("O", Point::new(84., 0.));
    document.add_bond(a, b, 1, "plain");
    document.add_bond(b, c, 1, "plain");
    let before = document.clone();
    let options = Options {
        scope: Scope::SelectedAtoms,
        keep_orientation: true,
    };
    for variant in 0..6 {
        let prepared = cleanup::prepare(&document, options, &[b])?;
        let mut points = prepared
            .requests()
            .next()
            .context("Missing cleanup request")?
            .molecule
            .positions
            .clone();
        match variant {
            0 => {
                points.pop();
            }
            1 => points.first_mut().context("Missing first point")?.x += 1e-4,
            2 => points.first_mut().context("Missing first point")?.x = f64::NAN,
            3 => points.first_mut().context("Missing first point")?.z = 1.,
            _ => {}
        }
        let layouts = match variant {
            4 => vec![],
            5 => vec![points.clone(), points],
            _ => vec![points],
        };
        let error = prepared
            .finish(layouts)
            .err()
            .context("Invalid layout was accepted")?;
        anyhow::ensure!(matches!(
            error,
            cleanup::Error::Layout | cleanup::Error::Fixed
        ));
        anyhow::ensure!(document == before);
    }
    let prepared = cleanup::prepare(&document, options, &[b])?;
    let mut points = prepared
        .requests()
        .next()
        .context("Missing cleanup request")?
        .molecule
        .positions
        .clone();
    points.first_mut().context("Missing first point")?.x += 1e-8;
    let cleaned = prepared.finish(vec![points])?;
    anyhow::ensure!(
        cleaned.document.atoms.first() == document.atoms.first()
            && cleaned.document.atoms.last() == document.atoms.last()
    );
    anyhow::ensure!(
        cleaned.analysis_policy == cleanup::AnalysisPolicy::SelectedChemistry
            && cleaned.analysis_failure.is_none()
    );
    let temporary = tempfile::tempdir()?;
    let error = native_response::analyze_prepared(
        Arc::new(cleaned.molecule.context("Missing molecule")?),
        Some(native_response::Config::new(
            temporary.path().join("missing-helper"),
        )),
    )
    .await
    .err()
    .context("Missing helper was swallowed")?;
    anyhow::ensure!(matches!(error, native_response::Error::Helper(_)));
    anyhow::ensure!(document == before);
    let excessive = vec![a; 1_000_001];
    anyhow::ensure!(matches!(
        cleanup::prepare(&document, options, &excessive),
        Err(cleanup::Error::Limit)
    ));
    Ok(())
}
