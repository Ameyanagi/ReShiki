#[path = "support/reference_dative.rs"]
mod reference_dative;
#[path = "support/reference_diagnostics.rs"]
mod reference_diagnostics;

use anyhow::Context;
use reshiki::{
    chemistry::{
        cdxml::{self, presentation::NativeColor},
        graph::{Atom, Graph},
        ranking::Metadata,
        stereo::Point3,
    },
    document::Bond,
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

#[derive(Deserialize)]
struct Part {
    id: i32,
    numbers: Vec<u8>,
    positions: Vec<Point3>,
}
impl Part {
    fn fragment(self) -> cdxml::Fragment {
        let graph = Graph {
            atoms: self
                .numbers
                .into_iter()
                .map(|atomic_number| Atom {
                    atomic_number,
                    ..Default::default()
                })
                .collect(),
            bonds: Vec::new(),
        };
        cdxml::Fragment {
            id: self.id,
            atom_ids: Vec::new(),
            bond_ids: Vec::new(),
            fuse_labels: vec![None; graph.atoms.len()],
            singlets: vec![false; graph.atoms.len()],
            metadata: Metadata::unspecified(&graph),
            directions: Vec::new(),
            positions: self.positions,
            is_3d: false,
            bond_cfg: Vec::new(),
            non_explicit_3d_chirality: vec![None; graph.atoms.len()],
            bond_cip: Vec::new(),
            atom_cip_ranks: vec![None; graph.atoms.len()],
            graph,
        }
    }
}
#[derive(Deserialize)]
struct Case {
    name: String,
    text: String,
    parts: Vec<Part>,
    scale: f64,
    colors: Vec<[f64; 3]>,
    expected: Option<Value>,
    document: Option<Value>,
    failure: Option<String>,
}
#[test]
fn original_native_bonds_keep_wide_values_until_document_conversion() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut child = Command::new(root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    }))
    .arg(root.join("reference/cdxml_native_bonds_reference.py"))
    .env("PYTHONUTF8", "1")
    .stdout(Stdio::piped())
    .stderr(Stdio::inherit())
    .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().context("Missing oracle stdout")?).lines();
    let header: Value = serde_json::from_str(&lines.next().context("Missing oracle header")??)?;
    assert_eq!(header["rdkit_version"], reshiki::chemistry::RDKIT_VERSION);
    let (mut accepted, mut rejected, mut narrowed, mut boundary) = (0, 0, 0, 0);
    let mut failures = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        let parts = case
            .parts
            .into_iter()
            .map(Part::fragment)
            .collect::<Vec<_>>();
        let colors = case.colors.into_iter().map(NativeColor).collect::<Vec<_>>();
        let before = serde_json::to_value((&parts, &colors, &case.text))?;
        let actual = cdxml::bonds::read_native(&case.text, &parts, case.scale, &colors);
        assert_eq!(before, serde_json::to_value((&parts, &colors, &case.text))?);
        match (actual, case.expected) {
            (Ok(bonds), Some(expected)) => {
                accepted += 1;
                let expected = reference_dative::native_bonds(
                    &case.text,
                    expected.clone(),
                    &parts,
                    case.scale,
                )
                .unwrap_or_else(|error| {
                    failures.push(format!("{}: dative expectation: {error}", case.name));
                    expected
                });
                for difference in reference_diagnostics::differences(
                    &serde_json::to_value(&bonds)?,
                    &expected,
                    "native",
                ) {
                    failures.push(format!("{}: {difference}", case.name));
                }
                let converted = bonds
                    .into_iter()
                    .map(cdxml::bonds::NativeBond::into_document)
                    .collect::<Result<Vec<_>, _>>();
                match (converted, case.document) {
                    (Ok(bonds), Some(expected)) => {
                        narrowed += 1;
                        let expected = reference_dative::document_bonds(
                            &case.text,
                            expected.clone(),
                            &parts,
                            case.scale,
                        )
                        .unwrap_or_else(|error| {
                            failures.push(format!("{}: dative expectation: {error}", case.name));
                            expected
                        });
                        let expected: Vec<Bond> = serde_json::from_value(expected)?;
                        for difference in reference_diagnostics::differences(
                            &serde_json::to_value(&bonds)?,
                            &serde_json::to_value(&expected)?,
                            "document.bonds",
                        ) {
                            failures.push(format!("{}: {difference}", case.name));
                        }
                    }
                    (Err(_), None) => boundary += 1,
                    (actual, expected) => {
                        failures.push(format!(
                            "{}: boundary mismatch {actual:?} / {expected:?}",
                            case.name
                        ));
                    }
                }
            }
            (Err(error), None) => {
                rejected += 1;
                if Some(error.to_string()) != case.failure {
                    failures.push(format!(
                        "{}: error mismatch {error} / {:?}",
                        case.name, case.failure
                    ));
                }
            }
            (actual, expected) => failures.push(format!(
                "{}: {actual:?} / {expected:?}; {:?}",
                case.name, case.failure
            )),
        }
    }
    anyhow::ensure!(child.wait()?.success(), "Oracle failed");
    eprintln!(
        "Native bonds: {accepted} exact helpers, {rejected} exact errors; {narrowed} checked bonds, {boundary} final boundary rejections"
    );
    assert!(accepted > 5000 && rejected > 80 && boundary > 40);
    reference_diagnostics::report("CDXML native bonds", &failures)?;
    Ok(())
}
