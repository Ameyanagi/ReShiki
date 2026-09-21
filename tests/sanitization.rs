//! Differential sanitization tests use RDKit directly, never the worker or
//! a translation of the new Rust rules to compute the expected graph.
use reshiki::chemistry::{RDKIT_VERSION, graph::Graph, normalize};
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
    expected: serde_json::Value,
}

#[test]
fn functional_group_cleanup_matches_rdkit() -> Result<(), Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/sanitization_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().ok_or("Missing oracle output")?).lines();
    let header: serde_json::Value =
        serde_json::from_str(&lines.next().ok_or("Missing oracle version")??)?;
    assert_eq!(header["rdkit_version"], RDKIT_VERSION);
    let (mut count, mut changed) = (0, 0);
    let mut failures = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        let before = serde_json::to_value(&case.graph)?;
        changed += usize::from(before != case.expected);
        count += 1;
        match normalize::functional_groups(&case.graph)
            .and_then(|g| serde_json::to_value(g).map_err(|e| e.to_string()))
        {
            Ok(actual) if actual == case.expected => {}
            actual if failures.len() < 20 => {
                failures.push(format!("{}: {actual:?} != {}", case.name, case.expected))
            }
            _ => {}
        }
        assert_eq!(serde_json::to_value(&case.graph)?, before, "Mutated input");
    }
    assert!(
        child.wait()?.success(),
        "Oracle failed: {}",
        failures.join("\n")
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(
        count > 20_000 && changed > 100,
        "Missing coverage: {count} cases / {changed} changes"
    );
    eprintln!("Verified {count} cleanup cases, including {changed} transformed graphs");
    Ok(())
}
