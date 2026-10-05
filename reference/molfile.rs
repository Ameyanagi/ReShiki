use anyhow::Context;
use reshiki::chemistry::{
    RDKIT_VERSION,
    document::Molecule,
    molfile,
    stereo::{Point3, perception::State},
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

#[derive(Deserialize)]
struct Input {
    rdkit_version: String,
    ids: Vec<u64>,
    positions: Vec<Point3>,
    state: State,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    molecule: Input,
    force_v3000: bool,
    expected: Option<String>,
    failure: Option<String>,
}

#[test]
fn molecular_file_output_matches_native_writers() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/molfile_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines =
        BufReader::new(child.stdout.take().context("Missing MOL oracle output")?).lines();
    let version: Value = serde_json::from_str(&lines.next().context("Missing oracle version")??)?;
    assert_eq!(version["rdkit_version"], RDKIT_VERSION);
    let (mut total, mut accepted, mut rejected, mut mismatches) = (0, 0, 0, 0);
    let mut failures = Vec::new();
    for line in lines {
        let line = line?;
        let case: Case = serde_json::from_str(&line)?;
        total += 1;
        assert_eq!(case.molecule.rdkit_version, RDKIT_VERSION);
        let input = Molecule {
            rdkit_version: RDKIT_VERSION,
            ids: case.molecule.ids,
            positions: case.molecule.positions,
            state: case.molecule.state,
        };
        let before = serde_json::to_value(&input)?;
        let result = molfile::write(
            &input,
            molfile::Options {
                force_v3000: case.force_v3000,
            },
        );
        let failure = match (&result, &case.expected) {
            (Ok(actual), Some(expected)) if actual == expected => {
                accepted += 1;
                None
            }
            (Err(_), None) => {
                rejected += 1;
                None
            }
            (Err(error), Some(_)) => Some(format!("Unexpected error: {error}")),
            (Ok(_), None) => Some(format!("Accepted reference rejection: {:?}", case.failure)),
            (Ok(actual), Some(expected)) => {
                let first = actual
                    .lines()
                    .zip(expected.lines())
                    .position(|(a, e)| a != e);
                Some(format!("MOL text differs at line {first:?}"))
            }
        };
        assert_eq!(serde_json::to_value(&input)?, before);
        if let Some(error) = failure {
            mismatches += 1;
            if failures.len() < 16 {
                failures.push(format!("{} V3000={}: {error}", case.name, case.force_v3000));
            }
            if mismatches == 1 {
                std::fs::create_dir_all(root.join("artifacts"))?;
                std::fs::write(root.join("artifacts/molfile-first-mismatch.json"), line)?;
                if let Ok(actual) = result {
                    std::fs::write(root.join("artifacts/molfile-actual.mol"), actual)?;
                }
            }
        }
    }
    assert!(child.wait()?.success(), "MOL oracle failed");
    eprintln!(
        "Verified {total} MOL outputs: {accepted} accepted, {rejected} rejected, {mismatches} mismatches"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(
        accepted > 20000 && rejected > 10,
        "Insufficient MOL coverage"
    );
    Ok(())
}

#[test]
fn malformed_molecular_state_is_rejected_before_export() -> anyhow::Result<()> {
    use reshiki::{
        chemistry::document,
        document::{Document, Point},
    };
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::default());
    let b = doc.add_atom("O", Point::new(42., 0.));
    doc.add_bond(a, b, 1, "plain");
    let original = document::prepare(&doc)?;
    for case in 0..7 {
        let mut molecule = original.clone();
        match case {
            0 => molecule.positions.first_mut().context("Missing point")?.x = f64::NAN,
            1 => molecule.state.metadata.atoms.clear(),
            2 => molecule.state.valences.clear(),
            3 => molecule.state.properties.atoms.clear(),
            4 => {
                molecule
                    .state
                    .graph
                    .bonds
                    .first_mut()
                    .context("Missing bond")?
                    .b = usize::MAX
            }
            5 => molecule.state.directions.clear(),
            _ => molecule.rdkit_version = "wrong",
        }
        assert!(molfile::write(&molecule, Default::default()).is_err());
    }
    let mut unsupported = original.clone();
    unsupported
        .state
        .graph
        .bonds
        .first_mut()
        .context("Missing bond")?
        .order = 7;
    assert!(matches!(
        molfile::write(&unsupported, Default::default()),
        Err(molfile::Error::UnsupportedBond)
    ));
    assert!(molfile::write(&original, Default::default())?.contains("V2000"));
    assert!(molfile::write(&original, molfile::Options { force_v3000: true })?.contains("V3000"));
    Ok(())
}
