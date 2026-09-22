use anyhow::Context;
use reshiki::chemistry::{RDKIT_VERSION, smiles};
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
fn difference(a: &Value, e: &Value, path: &str) -> Option<String> {
    if a == e {
        return None;
    }
    match (a, e) {
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
    Some(format!("{path}: {a} != {e}"))
}

#[test]
fn smiles_chemistry_matches_native_import() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/smiles_prepare_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(
        child
            .stdout
            .take()
            .context("Missing SMILES reader reference output")?,
    )
    .lines();
    let version: Value =
        serde_json::from_str(&lines.next().context("Missing reference version")??)?;
    assert_eq!(version["rdkit_version"], RDKIT_VERSION);
    let (mut accepted, mut rejected, mut mismatches) = (0, 0, 0);
    let mut failures = Vec::new();
    let mut all_failures = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        let result = smiles::prepare(&case.text);
        let failure = match (&result, &case.expected) {
            (Ok(actual), Some(expected)) => {
                accepted += 1;
                difference(&serde_json::to_value(actual)?, expected, "molecule")
            }
            (Err(_), None) => {
                rejected += 1;
                None
            }
            (Err(error), Some(_)) => Some(format!("Rejected supported input: {error}")),
            (Ok(_), None) => Some(format!("Accepted invalid input: {:?}", case.failure)),
        };
        if let Some(error) = failure {
            all_failures.push(format!("{}: {error}", case.name));
            mismatches += 1;
            if failures.len() < 24 {
                failures.push(format!("{}: {error}", case.name));
            }
            if mismatches == 1 {
                std::fs::create_dir_all(root.join("artifacts"))?;
                std::fs::write(
                    root.join("artifacts/smiles-prepare-mismatch.json"),
                    serde_json::to_vec_pretty(&case)?,
                )?;
                if let Ok(actual) = result {
                    std::fs::write(
                        root.join("artifacts/smiles-prepare-actual.json"),
                        serde_json::to_vec_pretty(&actual)?,
                    )?;
                }
            }
        }
    }
    assert!(child.wait()?.success(), "SMILES reader reference failed");
    if !all_failures.is_empty() {
        std::fs::write(
            root.join("artifacts/smiles-prepare-failures.txt"),
            all_failures.join("\n"),
        )?;
    }
    eprintln!(
        "SMILES preparation: {accepted} accepted, {rejected} rejected, {mismatches} mismatches"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(
        accepted > 5000 && rejected > 1000,
        "Insufficient SMILES import coverage"
    );
    Ok(())
}
