use anyhow::Context;
use reshiki::chemistry::{
    RDKIT_VERSION, graph::Graph, kekulize::Direction, normalize, ranking::Metadata, rings,
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
    cached_rings: Option<Vec<Vec<usize>>>,
    fast_atoms: Vec<Vec<usize>>,
    fast_bonds: Vec<Vec<usize>>,
    expected: Option<serde_json::Value>,
}

#[test]
fn metal_cleanup_and_fast_cycles_match_rdkit() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/organometallic_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().context("Missing oracle stdout")?).lines();
    let version: serde_json::Value =
        serde_json::from_str(&lines.next().context("Missing oracle version")??)?;
    assert_eq!(version["rdkit_version"], RDKIT_VERSION);
    let (mut count, mut changed, mut rejected) = (0, 0, 0);
    let mut failures = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        count += 1;
        let before = serde_json::to_value(&case.graph)?;
        let fast = rings::fast(&case.graph).map_err(anyhow::Error::msg)?;
        if (fast.atoms, fast.bonds) != (case.fast_atoms, case.fast_bonds) && failures.len() < 20 {
            failures.push(format!("{}: fast traversal mismatch", case.name));
        }
        changed += usize::from(case.expected.as_ref().is_some_and(|e| e["graph"] != before));
        rejected += usize::from(case.expected.is_none());
        let result =
            normalize::organometallics(&case.graph, &case.metadata, case.cached_rings.as_deref());
        let matches = match (&result, &case.expected) {
            (Ok(graph), Some(expected)) => {
                serde_json::json!({"graph":graph,"metadata":case.metadata,"directions":case.directions})
                    == *expected
            }
            (Err(_), None) => true,
            _ => false,
        };
        if !matches && failures.len() < 20 {
            failures.push(format!("{}: {result:?} != {:?}", case.name, case.expected));
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
        count > 20_000 && changed > 1_000 && rejected > 100,
        "Missing coverage {count}/{changed}/{rejected}"
    );
    eprintln!(
        "Verified {count} metal-cleanup/fast-ring cases: {changed} changed, {rejected} rejected"
    );
    Ok(())
}
