#[path = "support/ring_basis.rs"]
mod ring_basis;
use anyhow::Context;
use reshiki::chemistry::{
    RDKIT_VERSION, graph::Graph, kekulize::Direction, ranking::Metadata, sanitize,
};
use serde::Deserialize;
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

#[derive(Deserialize)]
struct Case {
    name: String,
    graph: Graph,
    metadata: Metadata,
    directions: Vec<Direction>,
    expected: Option<serde_json::Value>,
    failure: Option<String>,
}

#[test]
fn complete_pipeline_matches_independent_rdkit() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("reference/sanitize_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().context("Missing oracle output")?).lines();
    let header: serde_json::Value =
        serde_json::from_str(&lines.next().context("Missing oracle version")??)?;
    assert_eq!(header["rdkit_version"], RDKIT_VERSION);
    let (mut count, mut rejected, mut changed, mut retried, mut other_basis) = (0, 0, 0, 0, 0);
    let mut failures = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        let before = serde_json::to_value((&case.graph, &case.metadata, &case.directions))?;
        count += 1;
        rejected += usize::from(case.expected.is_none());
        let result = sanitize::sanitize(&case.graph, &case.metadata, &case.directions);
        let matches = match (&result, &case.expected) {
            (Ok(actual), Some(expected)) => {
                retried += usize::from(actual.canonical_retry);
                changed += usize::from(serde_json::to_value(&actual.graph)? != before[0]);
                // The oracle clears tag 5 on non-allene atoms first (#892).
                let mut actual = serde_json::to_value(actual)?;
                other_basis += ring_basis::adopt_rdkit_basis(&mut actual, expected);
                actual == *expected
            }
            (Err(_), None) => true,
            _ => false,
        };
        if !matches && failures.len() < 12 {
            failures.push(format!(
                "{}: {result:?} != {:?}, failure {:?}",
                case.name, case.expected, case.failure
            ));
        }
        assert_eq!(
            serde_json::to_value((&case.graph, &case.metadata, &case.directions))?,
            before,
            "Mutated pipeline input"
        );
    }
    assert!(child.wait()?.success(), "Oracle failed");
    assert!(
        count > 30_000 && rejected > 1_000 && changed > 1_000 && retried > 0,
        "Insufficient coverage: {count}/{rejected}/{changed}/{retried}"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    eprintln!(
        "Verified {count} complete sanitization cases: {rejected} rejected, {changed} changed, {retried} canonical retries, {other_basis} RDKit SSSR basis differences"
    );
    Ok(())
}
