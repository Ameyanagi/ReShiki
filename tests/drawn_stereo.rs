use anyhow::Context;
use reshiki::chemistry::{
    RDKIT_VERSION,
    graph::{Graph, Valence},
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
    graph: Graph,
    metadata: Metadata,
    valences: Vec<Valence>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    before: State,
    positions: Option<Vec<Point3>>,
    directions: Vec<Direction>,
    replace: bool,
    operation: String,
    expected: Option<serde_json::Value>,
}

#[test]
fn drawn_stereo_matches_independent_rdkit() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/drawn_stereo_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().context("Missing oracle output")?).lines();
    let version: serde_json::Value =
        serde_json::from_str(&lines.next().context("Missing oracle version")??)?;
    assert_eq!(version["rdkit_version"], RDKIT_VERSION);
    let (mut count, mut rejected, mut changed, mut promoted, mut cleared, mut bond_cases) =
        (0, 0, 0, 0, 0, 0);
    let mut failures = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        count += 1;
        let before = serde_json::to_value(&case.before)?;
        rejected += usize::from(case.expected.is_none());
        bond_cases += usize::from(case.operation == "bond");
        let result = if case.operation == "bond" {
            stereo::bond_stereo_from_directions(
                &case.before.graph,
                &case.before.metadata,
                &case.directions,
            )
            .map(|metadata| stereo::DrawnStereo {
                graph: case.before.graph.clone(),
                metadata,
                valences: case.before.valences.clone(),
            })
        } else {
            stereo::from_directions(
                &case.before.graph,
                &case.before.metadata,
                &case.directions,
                case.positions.as_deref(),
                case.replace,
            )
        };
        let matches = match (&result, &case.expected) {
            (Ok(actual), Some(expected)) => {
                changed += usize::from(*expected != before);
                promoted += usize::from(expected["graph"] != before["graph"]);
                cleared += usize::from(
                    actual
                        .metadata
                        .atoms
                        .iter()
                        .zip(&case.before.metadata.atoms)
                        .any(|(a, b)| a.chiral_tag == 0 && b.chiral_tag != 0),
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
            "Mutated caller data"
        );
    }
    assert!(child.wait()?.success(), "Oracle failed");
    assert!(
        count > 60_000
            && changed > 1000
            && promoted > 100
            && cleared > 100
            && rejected > 0
            && bond_cases > 10_000,
        "Coverage {count}/{changed}/{promoted}/{cleared}/{rejected}"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    eprintln!(
        "Verified {count} drawn-stereo cases: {changed} changed, {promoted} H promotions, {cleared} tag removals, {rejected} rejected, {bond_cases} bond-direction cases"
    );
    Ok(())
}
