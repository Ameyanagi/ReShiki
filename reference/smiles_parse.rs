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
fn smiles_graph_matches_native_reader() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/smiles_parse_reference.py"))
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
        let result = smiles::parse(&case.text);
        let failure = match (&result, &case.expected) {
            (Ok(actual), Some(expected)) => {
                accepted += 1;
                let mut observed = serde_json::json!({"graph": actual.graph, "metadata": actual.metadata,
                    "directions": actual.directions, "dummy_labels": actual.dummy_labels});
                if expected.get("bond_indices").is_some() {
                    observed
                        .as_object_mut()
                        .context("Expected graph snapshot")?
                        .insert(
                            "bond_indices".into(),
                            serde_json::to_value(&actual.bond_indices)?,
                        );
                }
                difference(&observed, expected, "molecule")
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
                    root.join("artifacts/smiles-parse-mismatch.json"),
                    serde_json::to_vec_pretty(&case)?,
                )?;
                if let Ok(actual) = result {
                    std::fs::write(
                        root.join("artifacts/smiles-parse-actual.json"),
                        serde_json::to_vec_pretty(&actual)?,
                    )?;
                }
            }
        }
    }
    assert!(child.wait()?.success(), "SMILES reader reference failed");
    if !all_failures.is_empty() {
        std::fs::write(
            root.join("artifacts/smiles-parse-failures.txt"),
            all_failures.join("\n"),
        )?;
    }
    eprintln!("SMILES import: {accepted} accepted, {rejected} rejected, {mismatches} mismatches");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(
        accepted > 5000 && rejected > 1000,
        "Insufficient SMILES import coverage"
    );
    Ok(())
}

#[test]
fn malformed_and_excessive_smiles_are_bounded() {
    for text in [
        "C".repeat(1_048_577),
        format!("C{}C{}", "(C".repeat(4097), ")".repeat(4097)),
        "C".repeat(100_001),
    ] {
        assert!(matches!(smiles::parse(&text), Err(smiles::Error::Limit)));
    }
    for text in [
        "[",
        "[C",
        "C%",
        "C%(",
        "C%(123456)",
        "C->",
        "C<-",
        "C11",
        "C12.C12",
        "C(=)",
        "[C:999999999999999999999]",
    ] {
        assert!(smiles::parse(text).is_err(), "Accepted {text:?}");
    }
    for prefix in ["", "C", "[", "[C@", "C%(", "C(C"] {
        for code in [0, 1, 31, 127, 128, 255, 0x3000, 0x5316, 0x1f9ea, 0x10ffff] {
            if let Some(ch) = char::from_u32(code) {
                let _ = smiles::parse(&format!("{prefix}{ch}C"));
            }
        }
    }
}
