use anyhow::Context;
use reshiki::{
    chemistry::{RDKIT_VERSION, document::Labels, molfile},
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
    text: String,
    expected: Option<Value>,
    labels: Option<Labels>,
    document: Option<Document>,
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
fn imported_drawings_match_native_coordinates_wedges_and_labels() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/molfile_drawing_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(
        child
            .stdout
            .take()
            .context("Missing drawing reference output")?,
    )
    .lines();
    let version: Value =
        serde_json::from_str(&lines.next().context("Missing reference version")??)?;
    assert_eq!(version["rdkit_version"], RDKIT_VERSION);
    let (mut accepted, mut rejected, mut spatial, mut attachments, mut wedges) = (0, 0, 0, 0, 0);
    let mut failures = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        let imported = molfile::read(&case.text);
        let result = match &imported {
            Ok(imported) => {
                spatial += usize::from(imported.annotations.is_3d);
                attachments += imported
                    .annotations
                    .attachment_points
                    .iter()
                    .filter(|p| p.is_some())
                    .count();
                let before = serde_json::to_value(imported)?;
                let result = imported.drawing().map_err(|e| e.to_string());
                assert_eq!(
                    serde_json::to_value(imported)?,
                    before,
                    "Import changed during drawing"
                );
                result
            }
            Err(error) => Err(error.to_string()),
        };
        // PythonEngine also validates the returned editable Document in Rust.
        // Native output can contain labels/appearances outside that contract.
        let expected = case
            .expected
            .as_ref()
            .filter(|_| case.document.as_ref().is_some_and(|d| d.validate().is_ok()));
        let failure = match (result, expected) {
            (Ok(drawing), Some(expected)) => {
                accepted += 1;
                let state = difference(
                    &serde_json::to_value(&drawing.molecule().state)?,
                    expected,
                    "state",
                );
                let labels = drawing.labels()?;
                let labeling = difference(
                    &serde_json::to_value(&labels)?,
                    &serde_json::to_value(case.labels.as_ref().context("Missing native labels")?)?,
                    "labels",
                );
                let actual = drawing.finish(labels)?;
                wedges += actual
                    .bonds
                    .iter()
                    .filter(|b| matches!(b.display.as_str(), "wedge" | "hash"))
                    .count();
                state.or(labeling).or(difference(
                    &serde_json::to_value(&actual)?,
                    &serde_json::to_value(&case.document)?,
                    "drawing",
                ))
            }
            (Err(_), None) => {
                rejected += 1;
                None
            }
            (Err(error), Some(_)) => Some(format!("Rejected supported drawing: {error}")),
            (Ok(_), None) => Some(format!("Accepted invalid drawing: {:?}", case.failure)),
        };
        if let Some(error) = failure {
            if failures.is_empty() {
                std::fs::create_dir_all(root.join("artifacts"))?;
                std::fs::write(
                    root.join("artifacts/molfile-drawing-mismatch.json"),
                    serde_json::to_vec_pretty(&case)?,
                )?;
                if let Ok(imported) = &imported
                    && let Ok(drawing) = imported.drawing()
                {
                    std::fs::write(
                        root.join("artifacts/molfile-drawing-actual.json"),
                        serde_json::to_vec_pretty(drawing.molecule())?,
                    )?;
                }
            }
            failures.push(format!("{}: {error}", case.name));
        }
    }
    assert!(child.wait()?.success(), "Drawing reference failed");
    eprintln!(
        "MOL drawings: {accepted} accepted, {rejected} rejected, {spatial} spatial, {attachments} attachments, {wedges} wedges, {} mismatches",
        failures.len()
    );
    if !failures.is_empty() {
        std::fs::write(
            root.join("artifacts/molfile-drawing-failures.txt"),
            failures.join("\n"),
        )?;
    }
    assert!(
        failures.is_empty(),
        "{}",
        failures
            .iter()
            .take(24)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        accepted > 1000 && rejected > 100 && spatial > 100 && wedges > 100,
        "Insufficient import drawing coverage"
    );
    Ok(())
}
