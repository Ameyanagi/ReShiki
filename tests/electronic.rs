use anyhow::Context;
use reshiki::chemistry::{RDKIT_VERSION, electronic, graph::Graph};
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
    tags: Vec<u8>,
    flags: Vec<bool>,
    assign_conjugation: bool,
    expected: Option<serde_json::Value>,
}

#[test]
fn electronic_state_matches_independent_rdkit() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/electronic_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().context("Missing oracle stdout")?).lines();
    let version: serde_json::Value =
        serde_json::from_str(&lines.next().context("Missing version")??)?;
    assert_eq!(version["rdkit_version"], RDKIT_VERSION);
    let (mut count, mut conjugated_cases, mut stereo_cases, mut external_cases) = (0, 0, 0, 0);
    let mut hybridizations = std::collections::BTreeSet::new();
    let mut failures = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        count += 1;
        stereo_cases += usize::from(case.tags.iter().any(|&tag| tag != 0));
        external_cases += usize::from(!case.assign_conjugation);
        let before = serde_json::to_value(&case.graph)?;
        let result = (|| -> Result<_, String> {
            let electrons = electronic::pi_electrons(&case.graph)?;
            let conjugated = if case.assign_conjugation {
                electronic::conjugation(&case.graph)?
            } else {
                case.flags.clone()
            };
            let hybridization = electronic::hybridization(&case.graph, &case.tags, &conjugated)?;
            Ok(
                serde_json::json!({"electrons":electrons, "conjugated":conjugated,
                "hybridization":hybridization,"graph":case.graph,"tags":case.tags}),
            )
        })();
        if let Some(expected) = &case.expected {
            conjugated_cases += usize::from(
                expected["conjugated"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(|v| v == true)),
            );
            if let Some(values) = expected["hybridization"].as_array() {
                for value in values {
                    hybridizations.insert(value.to_string());
                }
            }
        }
        let matches = match (&result, &case.expected) {
            (Ok(actual), Some(expected)) => actual == expected,
            (Err(_), None) => true,
            _ => false,
        };
        if !matches && failures.len() < 12 {
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
        count > 50_000
            && conjugated_cases > 5_000
            && stereo_cases > 4_000
            && external_cases > 3_000
    );
    assert_eq!(hybridizations.len(), 8);
    eprintln!(
        "Verified {count} electronic cases: {conjugated_cases} conjugated, {stereo_cases} stereo, {external_cases} external flags"
    );
    Ok(())
}
