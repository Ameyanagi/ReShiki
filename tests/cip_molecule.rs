use anyhow::Context;
use reshiki::chemistry::{
    RDKIT_VERSION,
    stereo::{cip::Molecule, perception::State},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

#[derive(Deserialize)]
struct Case {
    name: String,
    before: State,
    order: String,
    expected: Option<Value>,
}

fn prepare(state: &State, order: &str) -> anyhow::Result<Value> {
    let mut mol = Molecule::new(state)?;
    match order {
        "rings" => {
            for i in 0..state.graph.bonds.len() {
                mol.is_in_ring(i)?;
            }
        }
        "fractions" => {
            for i in 0..state.graph.atoms.len() {
                mol.fraction(i)?;
            }
        }
        "bonds" => {}
        _ => anyhow::bail!("Unknown access order: {order}"),
    }
    let orders = (0..state.graph.bonds.len())
        .map(|i| mol.bond_order(i))
        .collect::<Result<Vec<_>, _>>()?;
    let fractions = (0..state.graph.atoms.len())
        .map(|i| mol.fraction(i))
        .collect::<Result<Vec<_>, _>>()?;
    let ring_bonds = (0..state.graph.bonds.len())
        .map(|i| mol.is_in_ring(i))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({"orders": orders, "fractions": fractions, "ring_bonds": ring_bonds}))
}

#[test]
fn molecule_preparation_matches_native_cipmol() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/cip_molecule_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().context("Missing oracle output")?).lines();
    let header: Value = serde_json::from_str(&lines.next().context("Missing version")??)?;
    assert_eq!(header["rdkit_version"], RDKIT_VERSION);
    let (mut accepted, mut rejected, mut mismatches) = (0, 0, 0);
    let mut failures = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        let before = serde_json::to_value(&case.before)?;
        let actual = prepare(&case.before, &case.order);
        assert_eq!(
            before,
            serde_json::to_value(&case.before)?,
            "{} mutated",
            case.name
        );
        match (&actual, &case.expected) {
            (Ok(value), Some(expected)) if value == expected => accepted += 1,
            (Err(_), None) => rejected += 1,
            _ => {
                mismatches += 1;
                if failures.len() < 20 {
                    failures.push(format!("{}: {actual:?} != {:?}", case.name, case.expected));
                }
            }
        }
    }
    assert!(child.wait()?.success());
    eprintln!("CIP molecule: {accepted} accepted, {rejected} rejected, {mismatches} mismatches");
    assert_eq!(mismatches, 0, "{}", failures.join("\n"));
    assert!(accepted > 21_000);
    assert!(rejected > 0);
    Ok(())
}
