use anyhow::Context;
use reshiki::chemistry::{
    RDKIT_VERSION,
    graph::Graph,
    kekulize::Direction,
    ranking::Metadata,
    stereo::{self, Point3},
};
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

#[derive(Deserialize, Serialize)]
struct State {
    metadata: Metadata,
    directions: Vec<Direction>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    operation: String,
    graph: Graph,
    before: State,
    positions: Option<Vec<Point3>>,
    rings: Vec<Vec<usize>>,
    expected: Option<serde_json::Value>,
}

#[test]
fn double_bond_geometry_matches_independent_rdkit() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/bond_geometry_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().context("Missing oracle output")?).lines();
    let version: serde_json::Value =
        serde_json::from_str(&lines.next().context("Missing oracle version")??)?;
    assert_eq!(version["rdkit_version"], RDKIT_VERSION);
    let (mut count, mut changed, mut unknown, mut no_coordinates, mut complete) = (0, 0, 0, 0, 0);
    let mut failures = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        count += 1;
        no_coordinates += usize::from(case.positions.is_none());
        complete += usize::from(case.operation == "perceive");
        let before = serde_json::to_value(&case.before)?;
        let graph = serde_json::to_value(&case.graph)?;
        let result = (|| -> Result<_, String> {
            let mut result = if case.operation == "set" {
                stereo::double_bond_directions(
                    &case.graph,
                    &case.before.metadata,
                    &case.before.directions,
                    case.positions.as_deref(),
                    &case.rings,
                )?
            } else {
                stereo::detect_bond_stereo(
                    &case.graph,
                    &case.before.metadata,
                    &case.before.directions,
                    case.positions.as_deref(),
                    &case.rings,
                )?
            };
            if case.operation == "perceive" {
                result.metadata = stereo::bond_stereo_from_directions(
                    &case.graph,
                    &result.metadata,
                    &result.directions,
                )?;
            }
            Ok(result)
        })();
        let matches = match (&result, &case.expected) {
            (Ok(actual), Some(expected)) => {
                changed += usize::from(*expected != before);
                unknown += usize::from(
                    actual
                        .metadata
                        .bonds
                        .iter()
                        .zip(&case.before.metadata.bonds)
                        .any(|(a, b)| a.stereo == 1 && b.stereo != 1),
                );
                serde_json::to_value(actual)? == *expected
            }
            (Err(_), None) => true,
            _ => false,
        };
        if !matches && failures.len() < 10 {
            failures.push(format!("{}: {result:?} != {:?}", case.name, case.expected));
        }
        assert_eq!(
            serde_json::to_value(&case.before)?,
            before,
            "Mutated caller metadata"
        );
        assert_eq!(
            serde_json::to_value(&case.graph)?,
            graph,
            "Mutated caller graph"
        );
    }
    assert!(child.wait()?.success(), "Oracle failed");
    assert!(
        count > 35_000
            && changed > 5000
            && unknown > 500
            && no_coordinates > 5000
            && complete > 5000,
        "Insufficient coverage {count}/{changed}/{unknown}/{no_coordinates}/{complete}"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    eprintln!(
        "Verified {count} bond-geometry cases: {changed} changed, {unknown} unknown assignments, {no_coordinates} without coordinates, {complete} composed perception cases"
    );
    Ok(())
}
