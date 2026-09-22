use anyhow::Context;
use reshiki::chemistry::{
    RDKIT_VERSION, electronic::Hybridization, graph::Graph, kekulize::Direction, ranking::Metadata,
    stereo,
};
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

#[derive(Deserialize, Serialize)]
struct State {
    graph: Graph,
    metadata: Metadata,
    directions: Vec<Direction>,
    hybridizations: Vec<Hybridization>,
    conjugated: Vec<bool>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    operation: String,
    before: State,
    rings: Vec<Vec<usize>>,
    expected: Option<serde_json::Value>,
}

#[test]
fn stereo_cleanup_matches_independent_rdkit() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/stereo_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().context("Missing oracle stdout")?).lines();
    let version: serde_json::Value =
        serde_json::from_str(&lines.next().context("Missing version")??)?;
    assert_eq!(version["rdkit_version"], RDKIT_VERSION);
    let (mut count, mut changed, mut group_changes, mut atrop_cases) = (0, 0, 0, 0);
    let mut failures = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        count += 1;
        let original = serde_json::to_value(&case.before)?;
        let input = &case.before;
        atrop_cases += usize::from(
            input
                .metadata
                .bonds
                .iter()
                .any(|b| matches!(b.stereo, 6 | 7)),
        );
        let result = (|| -> Result<_, String> {
            let meta = match case.operation.as_str() {
                "chirality" => {
                    stereo::chirality(&input.graph, &input.metadata, &input.hybridizations)?
                }
                "atrop" => stereo::atropisomers(
                    &input.graph,
                    &input.metadata,
                    &input.hybridizations,
                    &case.rings,
                )?,
                "both" => {
                    let meta = stereo::atropisomers(
                        &input.graph,
                        &input.metadata,
                        &input.hybridizations,
                        &case.rings,
                    )?;
                    stereo::chirality(&input.graph, &meta, &input.hybridizations)?
                }
                _ => return Err("Unknown test operation".into()),
            };
            Ok(
                serde_json::json!({"graph":input.graph,"metadata":meta,"directions":input.directions,
                "hybridizations":input.hybridizations,"conjugated":input.conjugated}),
            )
        })();
        if let Some(expected) = &case.expected {
            changed += usize::from(expected != &original);
            group_changes +=
                usize::from(expected["metadata"]["groups"] != original["metadata"]["groups"]);
        }
        let matches = match (&result, &case.expected) {
            (Ok(actual), Some(expected)) => actual == expected,
            (Err(_), None) => true,
            _ => false,
        };
        if !matches && failures.len() < 12 {
            failures.push(format!("{}: {result:?} != {:?}", case.name, case.expected));
        }
        assert_eq!(
            serde_json::to_value(&case.before)?,
            original,
            "Mutated input"
        );
    }
    assert!(
        child.wait()?.success(),
        "Oracle failed: {}",
        failures.join("\n")
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(
        count > 40_000 && changed > 10_000 && group_changes > 5_000 && atrop_cases > 2_000,
        "Missing coverage {count}/{changed}/{group_changes}/{atrop_cases}"
    );
    eprintln!(
        "Verified {count} stereo-cleanup cases: {changed} changed, {group_changes} group changes, {atrop_cases} atropisomer inputs"
    );
    Ok(())
}
