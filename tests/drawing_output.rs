use anyhow::Context;
use reshiki::{
    chemistry::{
        RDKIT_VERSION,
        document::{self, Labels},
    },
    document::Document,
};
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
    before: Document,
    expected: Option<Value>,
    labels: Option<Labels>,
    document: Option<Document>,
    failure: Option<String>,
}

fn difference(actual: &Value, expected: &Value, location: &str) -> Option<String> {
    if actual == expected {
        return None;
    }
    match (actual, expected) {
        (Value::Object(a), Value::Object(e)) => {
            for (key, value) in e {
                if let Some(message) = difference(&a[key], value, &format!("{location}.{key}")) {
                    return Some(message);
                }
            }
        }
        (Value::Array(a), Value::Array(e)) if a.len() == e.len() => {
            for (i, (a, e)) in a.iter().zip(e).enumerate() {
                if let Some(message) = difference(a, e, &format!("{location}[{i}]")) {
                    return Some(message);
                }
            }
        }
        _ => {}
    }
    Some(format!("{location}: {actual} != {expected}"))
}

#[test]
fn drawing_state_and_editable_document_match_native_reference() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/drawing_output_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().context("Missing oracle output")?).lines();
    let version: Value = serde_json::from_str(&lines.next().context("Missing oracle version")??)?;
    assert_eq!(version["rdkit_version"], RDKIT_VERSION);
    let (mut count, mut changed, mut mismatches) = (0, 0, 0);
    let mut failures = Vec::new();
    for line in lines {
        let mut case: Case = serde_json::from_str(&line?)?;
        count += 1;
        let before = case.before.clone();
        let molecule = document::prepare(&case.before)?;
        let result = document::for_drawing(&molecule, &case.before);
        let failure = match (result, &case.expected) {
            (Ok(draft), Some(expected)) => {
                changed += usize::from(
                    serde_json::to_value(&molecule.state)?
                        != serde_json::to_value(&draft.molecule().state)?,
                );
                let state = difference(
                    &serde_json::to_value(&draft.molecule().state)?,
                    expected,
                    "state",
                );
                let labels = draft.labels()?;
                let label_difference = difference(
                    &serde_json::to_value(&labels)?,
                    &serde_json::to_value(
                        case.labels.take().context("Missing native CIP labels")?,
                    )?,
                    "labels",
                );
                let actual = draft.finish(labels)?;
                let doc = difference(
                    &serde_json::to_value(actual)?,
                    &serde_json::to_value(&case.document)?,
                    "document",
                );
                state.or(label_difference).or(doc)
            }
            (Err(_), None) => None,
            (Err(error), Some(_)) => Some(format!("Unexpected error: {error}")),
            (Ok(_), None) => Some(format!("Accepted native failure: {:?}", case.failure)),
        };
        if let Some(error) = failure {
            mismatches += 1;
            if failures.len() < 12 {
                failures.push(format!("{}: {error}", case.name));
            }
            if mismatches == 1 {
                std::fs::create_dir_all(root.join("artifacts"))?;
                std::fs::write(
                    root.join("artifacts/drawing-first-mismatch.json"),
                    serde_json::to_vec_pretty(&case)?,
                )?;
            }
        }
        assert_eq!(case.before, before);
    }
    assert!(
        child.wait()?.success(),
        "Oracle failed: {}",
        failures.join("\n")
    );
    eprintln!(
        "Verified {count} drawing outputs: {changed} changed states, {mismatches} mismatches"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(count > 10000 && changed > 1000, "Insufficient coverage");
    Ok(())
}
