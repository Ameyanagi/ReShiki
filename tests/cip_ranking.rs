use reshiki::chemistry::{RDKIT_VERSION, graph::Graph, ranking::Metadata, stereo};
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
    metadata: Metadata,
    expected: Option<Vec<u32>>,
}

#[test]
fn legacy_atom_priorities_match_independent_rdkit() -> Result<(), Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/cip_ranking_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().ok_or("Missing oracle output")?).lines();
    let version: serde_json::Value =
        serde_json::from_str(&lines.next().ok_or("Missing oracle version")??)?;
    assert_eq!(version["rdkit_version"], RDKIT_VERSION);
    let (mut count, mut mapped, mut isotopic, mut rejected, mut tied) = (0, 0, 0, 0, 0);
    let mut failures = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        count += 1;
        mapped += usize::from(case.metadata.atoms.iter().any(|a| a.map_present));
        isotopic += usize::from(case.graph.atoms.iter().any(|a| a.isotope != 0));
        let before = serde_json::to_value((&case.graph, &case.metadata))?;
        let result = stereo::atom_priorities(&case.graph, &case.metadata);
        let matches = match (&result, &case.expected) {
            (Ok(actual), Some(expected)) => {
                tied += usize::from(
                    expected
                        .iter()
                        .max()
                        .is_some_and(|v| *v as usize + 1 < expected.len()),
                );
                actual == expected
            }
            (Err(_), None) => {
                rejected += 1;
                true
            }
            _ => false,
        };
        if !matches && failures.len() < 10 {
            failures.push(format!("{}: {result:?} != {:?}", case.name, case.expected));
        }
        assert_eq!(
            serde_json::to_value((&case.graph, &case.metadata))?,
            before,
            "Input changed"
        );
    }
    assert!(child.wait()?.success(), "Oracle failed");
    assert!(
        count > 35000 && mapped > 10000 && isotopic > 15000 && tied > 10000,
        "Insufficient coverage {count}/{mapped}/{isotopic}/{tied}"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    eprintln!(
        "Verified {count} CIP rank cases: {mapped} mapped, {isotopic} isotopic, {tied} with ties, {rejected} rejected"
    );
    Ok(())
}
