use anyhow::Context;
use reshiki::{
    chemistry::{RDKIT_VERSION, document::prepare},
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
    document: Document,
    expected: Option<Value>,
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
            for (i, (left, right)) in a.iter().zip(e).enumerate() {
                if let Some(message) = difference(left, right, &format!("{location}[{i}]")) {
                    return Some(message);
                }
            }
        }
        _ => {}
    }
    Some(format!("{location}: {actual} != {expected}"))
}

#[test]
fn document_preparation_matches_independent_rdkit() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/document_preparation_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().context("Missing oracle output")?).lines();
    let version: Value = serde_json::from_str(&lines.next().context("Missing oracle version")??)?;
    assert_eq!(version["rdkit_version"], RDKIT_VERSION);
    let (mut count, mut rejected, mut labels, mut mismatches) = (0, 0, 0, 0);
    let mut failures = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        count += 1;
        let before = serde_json::to_value(&case.document)?;
        let result = prepare(&case.document);
        let failure = match (&result, &case.expected) {
            (Ok(actual), Some(expected)) => {
                labels += actual
                    .state
                    .properties
                    .atoms
                    .iter()
                    .filter(|a| a.cip_code.is_some())
                    .count();
                difference(&serde_json::to_value(actual)?, expected, "molecule")
            }
            (Err(_), None) => {
                rejected += 1;
                None
            }
            (Err(error), Some(_)) => Some(format!("unexpected error: {error}")),
            (Ok(_), None) => Some(format!("accepted invalid reference: {:?}", case.failure)),
        };
        if let Some(error) = failure {
            mismatches += 1;
            if failures.len() < 12 {
                failures.push(format!("{}: {error}", case.name));
            }
            if mismatches == 1 {
                std::fs::create_dir_all(root.join("artifacts"))?;
                std::fs::write(
                    root.join("artifacts/document-first-mismatch.json"),
                    serde_json::to_vec_pretty(&case)?,
                )?;
                if let Ok(actual) = &result {
                    std::fs::write(
                        root.join("artifacts/document-first-actual.json"),
                        serde_json::to_vec_pretty(actual)?,
                    )?;
                }
            }
        }
        assert_eq!(
            serde_json::to_value(&case.document)?,
            before,
            "Input changed"
        );
    }
    assert!(
        child.wait()?.success(),
        "Oracle failed: {}",
        failures.join("\n")
    );
    eprintln!(
        "Verified {count} drawings: {labels} stereo labels, {rejected} rejected, {mismatches} mismatches"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(
        count > 10000 && labels > 100 && rejected > 100,
        "Insufficient coverage"
    );
    Ok(())
}
