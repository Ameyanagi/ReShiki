use anyhow::Context;
use reshiki::chemistry::{
    RDKIT_VERSION,
    graph::Graph,
    rings::{self, Options},
};
use serde::Deserialize;
use std::{
    collections::BTreeSet,
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

/// The rings of the SSSR basis, each as a sorted atom list.
fn basis(atoms: &[Vec<usize>], count: usize) -> BTreeSet<Vec<usize>> {
    atoms[..count]
        .iter()
        .map(|ring| {
            let mut ring = ring.clone();
            ring.sort_unstable();
            ring
        })
        .collect()
}

#[test]
fn ring_order_matches_rdkit_examples_permutations_dense_graphs_and_nci() -> TestResult {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("reference/ring_perception_reference.py"))
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
    let mut rejected = 0;
    let mut verified = 0;
    let mut other_basis = 0;
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
            (Ok(actual), Some(expected)) => {
                if (actual.basis_count, &actual.atoms, &actual.bonds)
                    == (expected.basis_count, &expected.atoms, &expected.bonds)
                {
                    verified += 1;
                    continue;
                }
                // RDKit 2026.09 takes its SSSR basis from RingDecomposerLib
                // even in legacy mode; ReShiki keeps the Figueras basis. A
                // case is that difference only when the basis rings differ.
                if basis(&actual.atoms, actual.basis_count)
                    != basis(&expected.atoms, expected.basis_count)
                {
                    other_basis += 1;
                } else if failures.len() < 30 {
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
            (Err(_), None) => {
                rejected += 1;
            }
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
    eprintln!(
        "Verified {verified} ordered ring cases; {other_basis} expected differences from RDKit's RingDecomposerLib SSSR basis; {rejected} rejected ({count} total)"
    );
    Ok(())
}
