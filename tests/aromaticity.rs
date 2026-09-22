use anyhow::Context;
use reshiki::chemistry::{RDKIT_VERSION, aromaticity, graph::Graph};
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
    rings: Vec<Vec<usize>>,
    adjust: bool,
    expected: Option<serde_json::Value>,
}

#[test]
fn aromaticity_and_hydrogen_adjustment_match_rdkit() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/aromaticity_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().context("Missing oracle output")?).lines();
    let header: serde_json::Value =
        serde_json::from_str(&lines.next().context("Missing oracle version")??)?;
    assert_eq!(header["rdkit_version"], RDKIT_VERSION);
    let (mut count, mut changed) = (0, 0);
    let mut failures = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        let before = serde_json::to_value(&case.graph)?;
        changed += usize::from(case.expected.as_ref().is_some_and(|e| e["graph"] != before));
        count += 1;
        let result = aromaticity::perceive(&case.graph, &case.rings).and_then(|mut a| {
            if case.adjust {
                a.graph =
                    aromaticity::adjust_hydrogens(&a.graph, &case.graph.provisional_valences()?)?;
            }
            Ok(serde_json::json!({"graph": a.graph, "aromatic_rings": a.aromatic_rings}))
        });
        let matches = match (&result, &case.expected) {
            (Ok(a), Some(e)) => a == e,
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
        count > 30_000 && changed > 5_000,
        "Missing coverage: {count} / {changed}"
    );
    eprintln!("Verified {count} aromaticity cases, including {changed} transformed graphs");
    Ok(())
}
