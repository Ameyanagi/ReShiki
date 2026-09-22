//! Compare all elements and edge cases directly with the pinned RDKit source.
use anyhow::Context;
use reshiki::chemistry::{
    RDKIT_VERSION,
    graph::{Atom, Graph, Valence},
};
use serde::Deserialize;
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};
type TestResult = anyhow::Result<()>;

#[derive(Deserialize)]
struct Case {
    name: String,
    operation: String,
    graph: Graph,
    expected: Option<serde_json::Value>,
}

#[test]
fn valence_hydrogens_and_radicals_match_rdkit() -> TestResult {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/valence_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().context("Missing oracle output")?).lines();
    let header: serde_json::Value =
        serde_json::from_str(&lines.next().context("Missing version")??)?;
    assert_eq!(header["rdkit_version"], RDKIT_VERSION);
    let (mut count, mut rejected, mut radical_cases, mut provisional_cases) = (0, 0, 0, 0);
    let mut failures = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        count += 1;
        let actual = if case.operation == "radicals" {
            radical_cases += 1;
            case.graph
                .assign_radicals()
                .and_then(|v| serde_json::to_value(v).map_err(|e| e.to_string()))
        } else if case.operation == "provisional" {
            provisional_cases += 1;
            case.graph
                .provisional_valences()
                .and_then(|v| serde_json::to_value(v).map_err(|e| e.to_string()))
        } else {
            case.graph
                .valences()
                .and_then(|v| serde_json::to_value(v).map_err(|e| e.to_string()))
        };
        let matches = match (&actual, &case.expected) {
            (Ok(actual), Some(expected)) => actual == expected,
            (Err(_), None) => {
                rejected += 1;
                true
            }
            _ => false,
        };
        if !matches && failures.len() < 30 {
            failures.push(format!(
                "{} {}: {:?} != {:?}",
                case.operation, case.name, actual, case.expected
            ));
        }
    }
    assert!(child.wait()?.success(), "Reference process failed");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(count > 150_000, "Corpus unexpectedly shrank: {count}");
    assert!(
        rejected > 10_000,
        "Missing invalid valence cases: {rejected}"
    );
    assert!(
        radical_cases > 40_000,
        "Missing radical cases: {radical_cases}"
    );
    assert!(
        provisional_cases > 100_000,
        "Missing intermediate cache cases: {provisional_cases}"
    );
    eprintln!(
        "Verified {count} RDKit cases, including {rejected} valence rejections and {radical_cases} radical assignments"
    );
    Ok(())
}

#[test]
fn rejects_malformed_graphs_without_partial_results() -> TestResult {
    let carbon = serde_json::to_value(Atom {
        atomic_number: 6,
        ..Default::default()
    })?;
    let unknown = serde_json::to_value(Atom {
        atomic_number: 119,
        ..Default::default()
    })?;
    let bond = |a: usize, b: usize, order: u8| serde_json::json!({"a":a,"b":b,"order":order,"aromatic":false});
    for value in [
        serde_json::json!({"atoms":[unknown],"bonds":[]}),
        serde_json::json!({"atoms":[carbon],"bonds":[bond(0,0,1)]}),
        serde_json::json!({"atoms":[carbon],"bonds":[bond(0,1,1)]}),
        serde_json::json!({"atoms":[carbon],"bonds":[bond(0,usize::MAX,1)]}),
        serde_json::json!({"atoms":[carbon,carbon],"bonds":[bond(0,1,8)]}),
        serde_json::json!({"atoms":[carbon,carbon],"bonds":[bond(0,1,1),bond(1,0,2)]}),
    ] {
        let graph: Graph = serde_json::from_value(value)?;
        assert!(graph.valences().is_err());
        assert!(graph.provisional_valences().is_err());
        assert!(graph.assign_radicals().is_err());
        assert!(reshiki::chemistry::normalize::functional_groups(&graph).is_err());
        assert!(reshiki::chemistry::aromaticity::perceive(&graph, &[]).is_err());
    }
    let graph: Graph = serde_json::from_value(serde_json::json!({"atoms":[carbon],"bonds":[]}))?;
    assert_eq!(
        graph.valences().map_err(anyhow::Error::msg)?,
        vec![Valence {
            explicit_valence: 0,
            implicit_hydrogens: 4
        }]
    );
    let mut too_large = graph.clone();
    too_large.atoms.resize(100_001, graph.atoms[0].clone());
    assert!(too_large.valences().is_err());
    assert!(too_large.provisional_valences().is_err());
    assert!(too_large.assign_radicals().is_err());
    let valid = serde_json::json!({"atoms":[carbon,carbon],"bonds":[bond(0,1,1)]});
    for section in ["atoms", "bonds"] {
        let fields = valid[section][0]
            .as_object()
            .context("Missing test fields")?;
        for key in fields.keys() {
            let mut missing = valid.clone();
            missing[section][0]
                .as_object_mut()
                .context("Missing test fields")?
                .remove(key);
            assert!(
                serde_json::from_value::<Graph>(missing).is_err(),
                "missing {section}.{key}"
            );
        }
    }
    for value in [-1, 256] {
        let mut invalid = valid.clone();
        invalid["atoms"][0]["atomic_number"] = value.into();
        assert!(serde_json::from_value::<Graph>(invalid).is_err());
    }
    Ok(())
}
