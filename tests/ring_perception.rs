use anyhow::Context;
use reshiki::chemistry::{
    RDKIT_VERSION,
    graph::Graph,
    rings::{self, Options, RingError},
};
use serde::Deserialize;
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};
type TestResult = anyhow::Result<()>;

#[derive(Deserialize)]
struct Expected {
    basis_count: usize,
    atoms: Vec<Vec<usize>>,
    bonds: Vec<Vec<usize>>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    graph: Graph,
    dative: bool,
    hydrogen: bool,
    expected: Option<Expected>,
}

#[test]
fn ring_sets_match_rdkit_examples_permutations_and_nci() -> TestResult {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/ring_perception_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().context("Missing oracle output")?).lines();
    let header: serde_json::Value =
        serde_json::from_str(&lines.next().context("Missing version")??)?;
    assert_eq!(header["rdkit_version"], RDKIT_VERSION);
    let mut count = 0;
    let mut failures = Vec::new();
    let mut unresolved = 0;
    let mut verified = 0;
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        count += 1;
        let actual = rings::perceive(
            &case.graph,
            Options {
                include_dative: case.dative,
                include_hydrogen: case.hydrogen,
            },
        );
        match (actual, case.expected) {
            (Ok(mut actual), Some(expected)) => {
                verified += 1;
                for ring in &mut actual.atoms {
                    ring.sort_unstable();
                }
                actual.atoms.sort();
                for ring in &mut actual.bonds {
                    ring.sort_unstable();
                }
                actual.bonds.sort();
                if (actual.basis_count, &actual.atoms, &actual.bonds)
                    != (expected.basis_count, &expected.atoms, &expected.bonds)
                    && failures.len() < 30
                {
                    failures.push(format!(
                        "{}: {:?}/{:?}/{:?} != {:?}/{:?}/{:?}",
                        case.name,
                        actual.basis_count,
                        actual.atoms,
                        actual.bonds,
                        expected.basis_count,
                        expected.atoms,
                        expected.bonds
                    ));
                }
            }
            (Err(RingError::UnresolvedOrdering), Some(_)) => {
                // These graphs retain their backend result. Do not silently
                // accept a different ring count just to pass the comparison.
                unresolved += 1;
                assert!(
                    case.name.starts_with("topology "),
                    "Molecular corpus unexpectedly needs reference rings: {}",
                    case.name
                );
            }
            (Err(_), None) => {}
            (actual, expected) => {
                if failures.len() < 30 {
                    failures.push(format!(
                        "{}: success {} != {} ({:?})",
                        case.name,
                        actual.is_ok(),
                        expected.is_some(),
                        actual.err()
                    ));
                }
            }
        }
    }
    assert!(child.wait()?.success());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(count > 10_000, "Missing corpus: {count}");
    assert!(
        verified > 10_000,
        "Insufficient proven ring coverage: {verified}"
    );
    assert!(unresolved > 0, "Missing platform-ordering regression cases");
    eprintln!(
        "Verified {verified} exact ring cases; {unresolved} explicitly retained reference results ({count} total)"
    );
    Ok(())
}
