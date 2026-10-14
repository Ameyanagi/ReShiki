use anyhow::Context;
use reshiki::{
    chemistry::{RDKIT_VERSION, document::Labels, molfile},
    document::Document,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

#[path = "support/molfile_restrictions.rs"]
mod molfile_restrictions;
use molfile_restrictions::{GenericRestriction, rejection_difference};

#[derive(Deserialize, Serialize)]
struct Case {
    name: String,
    text: String,
    expected: Option<Value>,
    labels: Option<Labels>,
    document: Option<Document>,
    failure: Option<String>,
    restriction: Option<GenericRestriction>,
    extension: Option<DrawingExtension>,
    generic: Option<GenericDrawingAddition>,
}

#[derive(Deserialize, Serialize)]
struct DrawingExtension {
    failure: String,
    expected: Value,
    labels: Labels,
    document: Document,
}

#[derive(Deserialize, Serialize)]
struct GenericDrawingAddition {
    legacy_document: Document,
    document: Document,
}

fn difference(a: &Value, e: &Value, path: &str) -> Option<String> {
    fn collect(a: &Value, e: &Value, path: &str, failures: &mut Vec<String>) {
        if a == e {
            return;
        }
        match (a, e) {
            (Value::Object(a), Value::Object(e)) => {
                for (key, expected) in e {
                    let path = format!("{path}.{key}");
                    if let Some(actual) = a.get(key) {
                        collect(actual, expected, &path, failures);
                    } else {
                        failures.push(format!("{path}: missing expected field {expected}"));
                    }
                }
                for (key, actual) in a {
                    if !e.contains_key(key) {
                        failures.push(format!("{path}.{key}: unexpected field {actual}"));
                    }
                }
            }
            (Value::Array(a), Value::Array(e)) => {
                for i in 0..a.len().max(e.len()) {
                    let path = format!("{path}[{i}]");
                    match (a.get(i), e.get(i)) {
                        (Some(actual), Some(expected)) => {
                            collect(actual, expected, &path, failures)
                        }
                        (None, Some(expected)) => {
                            failures.push(format!("{path}: missing expected item {expected}"));
                        }
                        (Some(actual), None) => {
                            failures.push(format!("{path}: unexpected item {actual}"));
                        }
                        (None, None) => unreachable!(),
                    }
                }
            }
            _ => failures.push(format!("{path}: {a} != {e}")),
        }
    }
    let mut failures = Vec::new();
    collect(a, e, path, &mut failures);
    (!failures.is_empty()).then(|| failures.join("\n"))
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
        .arg(root.join("reference/molfile_drawing_reference.py"))
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
    let mut categories = BTreeMap::<String, usize>::new();
    let mut restrictions = BTreeMap::<String, usize>::new();
    let mut fields = BTreeMap::<String, usize>::new();
    let (mut mismatches, mut extensions, mut generic_additions) = (0, 0, 0);
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
        let original_success = case.expected.is_some()
            && case.failure.is_none()
            && case.document.as_ref().is_some_and(|d| d.validate().is_ok());
        let extension_guard = case.extension.as_ref().and_then(|extension| {
            (case.expected.is_some()
                || case.failure.as_deref() != Some(extension.failure.as_str())
                || extension.failure != "Outside the existing editable chemistry contract"
                || case.restriction.is_some()
                || extension.document.validate().is_err())
            .then(|| "Element-list drawing extension lacks exact original rejection".to_owned())
        });
        let document = case
            .extension
            .as_ref()
            .map(|extension| &extension.document)
            .or(case.document.as_ref());
        let native_labels = case
            .extension
            .as_ref()
            .map(|extension| &extension.labels)
            .or(case.labels.as_ref());
        let expected = case
            .extension
            .as_ref()
            .map(|extension| &extension.expected)
            .or(case.expected.as_ref())
            .filter(|_| document.is_some_and(|d| d.validate().is_ok()));
        let base_document = serde_json::to_value(document)?;
        // Retain the preexisting typed-Document wire boundary for both frozen
        // and independently derived expectations, including f32 positions.
        let generic_legacy = case
            .generic
            .as_ref()
            .map(|generic| serde_json::to_value(&generic.legacy_document))
            .transpose()?;
        let generic_guard = case.generic.as_ref().and_then(|generic| {
            (Some(&base_document) != generic_legacy.as_ref()
                || expected.is_none()
                || case.restriction.is_some()
                || generic.document.validate().is_err())
            .then(|| "Generic drawing addition changed the complete original drawing".to_owned())
        });
        let expected_document = case
            .generic
            .as_ref()
            .map(|generic| serde_json::to_value(&generic.document))
            .transpose()?
            .unwrap_or(base_document);
        let failure = if extension_guard.is_some() {
            extension_guard
        } else if generic_guard.is_some() {
            generic_guard
        } else if let Some(restriction) = &case.restriction {
            let failure = rejection_difference(restriction, original_success, &imported);
            if failure.is_none() {
                *restrictions
                    .entry(restriction.category.clone())
                    .or_default() += 1;
            }
            failure
        } else {
            match (result, expected) {
                (Ok(drawing), Some(expected)) => {
                    accepted += usize::from(case.extension.is_none() && case.generic.is_none());
                    let state = difference(
                        &serde_json::to_value(&drawing.molecule().state)?,
                        expected,
                        "state",
                    );
                    let labels = drawing.labels()?;
                    let labeling = difference(
                        &serde_json::to_value(&labels)?,
                        &serde_json::to_value(native_labels.context("Missing native labels")?)?,
                        "labels",
                    );
                    let actual = drawing.finish(labels)?;
                    wedges += actual
                        .bonds
                        .iter()
                        .filter(|b| matches!(b.display.as_str(), "wedge" | "hash"))
                        .count();
                    let document = difference(
                        &serde_json::to_value(&actual)?,
                        &expected_document,
                        "drawing",
                    );
                    let differences = [state, labeling, document]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>();
                    (!differences.is_empty()).then(|| differences.join("\n"))
                }
                (Err(_), None) => {
                    rejected += 1;
                    None
                }
                (Err(error), Some(_)) => Some(format!("Rejected supported drawing: {error}")),
                (Ok(_), None) => Some(format!("Accepted invalid drawing: {:?}", case.failure)),
            }
        };
        if failure.is_none() {
            extensions += usize::from(case.extension.is_some());
            generic_additions += usize::from(case.generic.is_some() && case.extension.is_none());
        }
        if let Some(error) = failure {
            let category = if let Some(restriction) = &case.restriction {
                format!("Restriction contract: {}", restriction.category)
            } else if case.extension.is_some() {
                "Element-list extension".to_owned()
            } else if case.generic.is_some() {
                "Generic drawing addition".to_owned()
            } else if error.starts_with("Rejected supported") {
                error.clone()
            } else if error.starts_with("Accepted invalid") {
                "Accepted invalid input".to_owned()
            } else {
                "Output difference".to_owned()
            };
            *categories.entry(category).or_default() += 1;
            if mismatches == 0 {
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
            mismatches += 1;
            for detail in error.lines() {
                failures.push(format!("{}: {detail}", case.name));
                *fields
                    .entry(detail.split(": ").next().unwrap_or(detail).to_owned())
                    .or_default() += 1;
            }
        }
    }
    assert!(child.wait()?.success(), "Drawing reference failed");
    let restricted = restrictions.values().sum::<usize>();
    eprintln!(
        "MOL drawings: {accepted} accepted, {rejected} rejected, {restricted} restricted, {spatial} spatial, {attachments} attachments, {wedges} wedges, {mismatches} mismatches"
    );
    if !failures.is_empty() {
        std::fs::write(
            root.join("artifacts/molfile-drawing-failures.txt"),
            failures.join("\n"),
        )?;
    }
    for (category, count) in categories {
        eprintln!("MOL drawing mismatch category: {count} {category}");
    }
    for (category, count) in restrictions {
        eprintln!("MOL drawing restriction category: {count} {category}");
    }
    eprintln!(
        "MOL drawings: {extensions} element-list acceptance extensions, {generic_additions} generic drawing additions, {} mismatch diagnostics",
        failures.len()
    );
    for (field, count) in fields {
        eprintln!("MOL drawing mismatch field: {count} {field}");
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(
        accepted > 1000 && rejected > 100 && spatial > 100 && wedges > 100,
        "Insufficient import drawing coverage"
    );
    Ok(())
}
