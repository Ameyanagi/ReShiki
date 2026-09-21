//! Native reaction parsing is separate from default molecular SMILES import.
use anyhow::Context;
use reshiki::chemistry::{RDKIT_VERSION, reaction};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

#[derive(Deserialize, Serialize)]
struct Case {
    name: String,
    text: String,
    expected: Option<Value>,
    failure: Option<String>,
}

fn difference(actual: &Value, expected: &Value, path: &str) -> Option<String> {
    if actual == expected {
        return None;
    }
    match (actual, expected) {
        (Value::Object(a), Value::Object(e)) if a.len() == e.len() => {
            for (key, value) in e {
                if let Some(d) = difference(
                    a.get(key).unwrap_or(&Value::Null),
                    value,
                    &format!("{path}.{key}"),
                ) {
                    return Some(d);
                }
            }
        }
        (Value::Array(a), Value::Array(e)) if a.len() == e.len() => {
            for (i, (a, e)) in a.iter().zip(e).enumerate() {
                if let Some(d) = difference(a, e, &format!("{path}[{i}]")) {
                    return Some(d);
                }
            }
        }
        _ => (),
    }
    Some(format!("{path}: {actual} != {expected}"))
}

#[test]
fn reaction_smiles_preserves_native_participant_states() -> anyhow::Result<()> {
    compare_reference(false)
}

#[test]
fn mutated_reaction_smiles_preserves_native_framing() -> anyhow::Result<()> {
    compare_reference(true)
}

fn compare_reference(mutations: bool) -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut command = Command::new(python);
    command.arg(root.join("tests/reaction_smiles_reference.py"));
    if mutations {
        command.arg("--mutations");
    }
    let mut child = command
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(
        child
            .stdout
            .take()
            .context("Missing reaction SMILES oracle")?,
    )
    .lines();
    let version: Value =
        serde_json::from_str(&lines.next().context("Missing reference version")??)?;
    assert_eq!(version["rdkit_version"], RDKIT_VERSION);
    let (mut accepted, mut rejected) = (0, 0);
    let mut failures = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        let actual = match reaction::read_smiles(&case.text) {
            Ok(reaction) => Ok(serde_json::to_value(reaction)?),
            Err(error) => Err(error.to_string()),
        };
        let failure = match (&actual, &case.expected) {
            (Ok(a), Some(e)) => {
                accepted += 1;
                difference(a, e, "reaction")
            }
            (Err(_), None) => {
                rejected += 1;
                None
            }
            (Err(e), Some(_)) => Some(format!("Rejected supported input: {e}")),
            (Ok(_), None) => Some(format!("Accepted rejected input: {:?}", case.failure)),
        };
        if let Some(error) = failure {
            if failures.is_empty() {
                std::fs::create_dir_all(root.join("artifacts"))?;
                std::fs::write(
                    root.join(format!(
                        "artifacts/reaction-smiles-{mutations}-mismatch.json"
                    )),
                    serde_json::to_vec_pretty(&case)?,
                )?;
                if let Ok(actual) = actual {
                    std::fs::write(
                        root.join(format!("artifacts/reaction-smiles-{mutations}-actual.json")),
                        serde_json::to_vec_pretty(&actual)?,
                    )?;
                }
            }
            failures.push(format!("{}: {error}", case.name));
        }
    }
    assert!(child.wait()?.success(), "Reaction SMILES reference failed");
    eprintln!(
        "Reaction SMILES (mutations={mutations}): {accepted} accepted, {rejected} rejected, {} mismatches",
        failures.len()
    );
    if !failures.is_empty() {
        std::fs::write(
            root.join(format!(
                "artifacts/reaction-smiles-{mutations}-failures.txt"
            )),
            failures.join("\n"),
        )?;
    }
    assert!(
        failures.is_empty(),
        "{}",
        failures
            .iter()
            .take(25)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(accepted > if mutations { 50 } else { 1000 } && rejected > 1000);
    Ok(())
}
