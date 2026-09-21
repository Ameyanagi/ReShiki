//! Compare independently computed totals and atom contributions before routing
//! any production descriptor away from RDKit.
use reshiki::chemistry::{
    RDKIT_VERSION,
    descriptors::{self, Descriptors},
    graph::Graph,
};
use serde::Deserialize;
use std::{
    error::Error,
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};
#[derive(Deserialize)]
struct Case {
    name: String,
    graph: Graph,
    rings: Vec<Vec<usize>>,
    expected: Descriptors,
}
type TestResult = Result<(), Box<dyn Error>>;

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= a.abs().max(b.abs()).max(1.) * 1e-12
}
fn compare(actual: &Descriptors, expected: &Descriptors) -> Result<(), String> {
    if actual.donors != expected.donors || actual.acceptors != expected.acceptors {
        return Err(format!(
            "HBD/HBA {:?} != {:?}",
            (actual.donors, actual.acceptors),
            (expected.donors, expected.acceptors)
        ));
    }
    for (name, a, b) in [
        ("logP", actual.logp, expected.logp),
        ("MR", actual.molar_refractivity, expected.molar_refractivity),
        ("TPSA", actual.tpsa, expected.tpsa),
        ("TPSA + S/P", actual.tpsa_with_s_p, expected.tpsa_with_s_p),
    ] {
        if !close(a, b) {
            return Err(format!("{name} {a} != {b}"));
        }
    }
    for (name, actual, expected) in [
        ("TPSA", &actual.tpsa_atoms, &expected.tpsa_atoms),
        ("TPSA S/P", &actual.tpsa_s_p_atoms, &expected.tpsa_s_p_atoms),
    ] {
        if actual.len() != expected.len()
            || actual.iter().zip(expected).any(|(&a, &b)| !close(a, b))
        {
            return Err(format!(
                "{name} atom contributions {actual:?} != {expected:?}"
            ));
        }
    }
    if actual.crippen_atoms.len() != expected.crippen_atoms.len() {
        return Err("Expanded hydrogen count differs".into());
    }
    for (i, (a, b)) in actual
        .crippen_atoms
        .iter()
        .zip(&expected.crippen_atoms)
        .enumerate()
    {
        if !close(a[0], b[0]) || !close(a[1], b[1]) {
            return Err(format!("Crippen atom {i}: {a:?} != {b:?}"));
        }
    }
    if actual.crippen_types != expected.crippen_types {
        return Err(format!(
            "Crippen type assignment {:?} != {:?}",
            actual.crippen_types, expected.crippen_types
        ));
    }
    Ok(())
}

#[test]
fn descriptors_and_atom_contributions_match_rdkit() -> TestResult {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/descriptors_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().ok_or("Missing oracle output")?).lines();
    let header: serde_json::Value =
        serde_json::from_str(&lines.next().ok_or("Missing oracle version")??)?;
    assert_eq!(header["rdkit_version"], RDKIT_VERSION);
    let mut count = 0;
    let mut errors = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        count += 1;
        let result = descriptors::calculate(&case.graph, &case.rings)
            .and_then(|d| compare(&d, &case.expected));
        if let Err(error) = result
            && errors.len() < 40
        {
            errors.push(format!("{}: {error}", case.name));
        }
    }
    assert!(
        child.wait()?.success(),
        "Oracle failed; preceding differences: {}",
        errors.join("\n")
    );
    assert!(errors.is_empty(), "{}", errors.join("\n"));
    assert!(count > 10_000, "Incomplete descriptor corpus: {count}");
    eprintln!("Verified {count} descriptor cases with atom contributions");
    Ok(())
}
