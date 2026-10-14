use anyhow::Context;
use reshiki::chemistry::{
    RDKIT_VERSION,
    molfile::{self, ReadError},
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
    failure: Option<String>,
    restriction: Option<GenericRestriction>,
    extension: Option<MoleculeExtension>,
}

#[derive(Deserialize, Serialize)]
struct MoleculeExtension {
    failure: String,
    expected: Value,
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
fn mismatch_diagnostics_include_all_fields_and_absent_nulls() {
    let actual = serde_json::json!({
        "nested": {"a": 1, "b": 2}, "items": [3, 4], "extra": null,
    });
    let expected = serde_json::json!({
        "nested": {"a": 0, "b": 0}, "items": [0], "missing": null,
    });
    let failures = difference(&actual, &expected, "molecule").unwrap();
    assert_eq!(failures.lines().count(), 6);
    for path in [
        "molecule.nested.a:",
        "molecule.nested.b:",
        "molecule.items[0]:",
        "molecule.items[1]:",
        "molecule.extra:",
        "molecule.missing:",
    ] {
        assert!(
            failures.contains(path),
            "Missing mismatch diagnostic {path}"
        );
    }
}

#[test]
fn molecular_file_import_matches_native_reader() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("reference/molfile_import_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(
        child
            .stdout
            .take()
            .context("Missing MOL reader reference output")?,
    )
    .lines();
    let version: Value =
        serde_json::from_str(&lines.next().context("Missing reference version")??)?;
    assert_eq!(version["rdkit_version"], RDKIT_VERSION);
    let (mut accepted, mut rejected, mut mismatches) = (0, 0, 0);
    let (mut groups_accepted, mut groups_rejected) = (0, 0);
    let mut failures = Vec::new();
    let mut categories = BTreeMap::<String, usize>::new();
    let mut restrictions = BTreeMap::<String, usize>::new();
    let mut fields = BTreeMap::<String, usize>::new();
    let mut extensions = 0;
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        let result = molfile::read(&case.text);
        let group_case = case.name.starts_with("substance groups/");
        let extension_guard = case.extension.as_ref().and_then(|extension| {
            (case.expected.is_some()
                || case.failure.as_deref() != Some(extension.failure.as_str())
                || extension.failure != "Outside the existing editable chemistry contract"
                || case.restriction.is_some())
            .then(|| "Element-list extension lacks exact original rejection".to_owned())
        });
        let expected = case
            .extension
            .as_ref()
            .map(|extension| &extension.expected)
            .or(case.expected.as_ref());
        let failure = if extension_guard.is_some() {
            extension_guard
        } else if let Some(restriction) = &case.restriction {
            let failure = rejection_difference(
                restriction,
                case.expected.is_some() && case.failure.is_none(),
                &result,
            );
            if failure.is_none() {
                *restrictions
                    .entry(restriction.category.clone())
                    .or_default() += 1;
            }
            failure
        } else {
            match (&result, expected) {
                (Ok(actual), Some(expected)) => {
                    accepted += usize::from(case.extension.is_none());
                    groups_accepted += usize::from(group_case);
                    difference(
                        &serde_json::json!({"state":actual.molecule.state,"positions":actual.molecule.positions,
                        "is_3d":actual.annotations.is_3d,"attachment_points":actual.annotations.attachment_points,
                        "dummy_labels":actual.annotations.dummy_labels}),
                        expected,
                        "molecule",
                    )
                }
                (Err(_), None) => {
                    rejected += 1;
                    groups_rejected += usize::from(group_case);
                    None
                }
                (Err(error), Some(_)) => Some(format!("Rejected supported input: {error}")),
                (Ok(_), None) => Some(format!("Accepted invalid input: {:?}", case.failure)),
            }
        };
        if failure.is_none() && case.extension.is_some() {
            extensions += 1;
        }
        if let Some(error) = failure {
            let category = if let Some(restriction) = &case.restriction {
                format!("Restriction contract: {}", restriction.category)
            } else if case.extension.is_some() {
                "Element-list extension".to_owned()
            } else if error.starts_with("Rejected supported") {
                error.clone()
            } else if error.starts_with("Accepted invalid") {
                "Accepted invalid input".to_owned()
            } else {
                "Output difference".to_owned()
            };
            *categories.entry(category).or_default() += 1;
            for detail in error.lines() {
                failures.push(format!("{}: {detail}", case.name));
                *fields
                    .entry(detail.split(": ").next().unwrap_or(detail).to_owned())
                    .or_default() += 1;
            }
            mismatches += 1;
            if mismatches == 1 {
                std::fs::create_dir_all(root.join("artifacts"))?;
                std::fs::write(
                    root.join("artifacts/molfile-import-mismatch.json"),
                    serde_json::to_vec_pretty(&case)?,
                )?;
                if let Ok(actual) = result {
                    std::fs::write(
                        root.join("artifacts/molfile-import-actual.json"),
                        serde_json::to_vec_pretty(&actual)?,
                    )?;
                }
            }
        }
    }
    assert!(child.wait()?.success(), "MOL reader reference failed");
    if !failures.is_empty() {
        std::fs::write(
            root.join("artifacts/molfile-import-failures.txt"),
            failures.join("\n"),
        )?;
    }
    let restricted = restrictions.values().sum::<usize>();
    eprintln!(
        "MOL import: {accepted} accepted, {rejected} rejected, {restricted} restricted, {mismatches} mismatches"
    );
    for (category, count) in categories {
        eprintln!("MOL import mismatch category: {count} {category}");
    }
    for (category, count) in restrictions {
        eprintln!("MOL import restriction category: {count} {category}");
    }
    eprintln!(
        "MOL import: {extensions} element-list acceptance extensions, {} mismatch diagnostics",
        failures.len()
    );
    for (field, count) in fields {
        eprintln!("MOL import mismatch field: {count} {field}");
    }
    eprintln!("Substance groups: {groups_accepted} accepted, {groups_rejected} rejected");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(
        accepted > 1000 && rejected > 100,
        "Insufficient MOL import coverage"
    );
    assert!(
        groups_accepted > 500 && groups_rejected > 500,
        "Insufficient substance-group coverage"
    );
    Ok(())
}

#[test]
fn group_defaults_and_fixed_fields_are_bounded() {
    let header = "\n\n\n  0  0  0  0  0  0  0  0  0  0999 V3000\nM  V30 BEGIN CTAB\nM  V30 COUNTS 0 0 132 0 0\nM  V30 BEGIN SGROUP\n";
    let records = (1..=132)
        .map(|id| format!("M  V30 {id} DAT 0\n"))
        .collect::<String>();
    let input = format!(
        "{header}M  V30 DEFAULT FIELDNAME={}\n{records}M  V30 END SGROUP\nM  V30 END CTAB\nM  END\n",
        "x".repeat(131_072)
    );
    assert!(matches!(molfile::read(&input), Err(ReadError::Limit)));
    let v2_header = "\n\n\n  0  0  0  0  0  0  0  0  0  0999 V2000\nM  STY  1   1 DAT\n";
    assert!(molfile::read(&format!("{v2_header}M  END\n")).is_ok());
    for prop in ["STY", "SAL", "SDT", "SCD", "SAP", "SDI"] {
        for column in 0..30 {
            let row = format!(
                "M  {prop}{}酸{}",
                " ".repeat(column),
                " ".repeat(50 - column)
            );
            let input = format!("{v2_header}{row}\nM  END\n");
            // Ignored unknown-group records may be accepted; neither result
            // may use unchecked UTF-8 slicing or panic on short fields.
            let _ = molfile::read(&input);
        }
    }
}

#[test]
fn malformed_mol_input_is_bounded_and_never_panics() {
    let header =
        "\n                    2D\n\n  0  0  0  0  0  0  0  0  0  0999 V3000\nM  V30 BEGIN CTAB\n";
    for content in [
        "COUNTS 100001 0 0 0 0\n",
        "COUNTS 0 300001 0 0 0\n",
        "COUNTS -1 0 0 0 0\n",
        "COUNTS 1 0 0 0 0\nM  V30 BEGIN ATOM\nM  V30 1 C NaN 0 0 0\n",
        "COUNTS 1 0 0 0 0\nM  V30 BEGIN ATOM\nM  V30 1 C inf 0 0 0\n",
        "COUNTS 1 0 0 0 0\nM  V30 BEGIN ATOM\nM  V30 1 C 0 0 0 0 LABEL=\"unclosed\n",
        "COUNTS 1 0 0 0 0\nM  V30 BEGIN ATOM\nM  V30 1 C 0 0 0 0 VALUE=(1 3\n",
        "COUNTS 1 0 0 0 0\nM  V30 BEGIN ATOM\nM  V30 1 C 0 0 0 0 -\n",
    ] {
        assert!(molfile::read(&format!("{header}M  V30 {content}")).is_err());
    }
    assert!(matches!(
        molfile::read(&" ".repeat(16 * 1024 * 1024 + 1)),
        Err(ReadError::Limit)
    ));
    let nested = "(".repeat(65);
    let input = format!(
        "{header}M  V30 COUNTS 1 0 0 0 0\nM  V30 BEGIN ATOM\nM  V30 1 C 0 0 0 0 X={nested}\n"
    );
    assert!(matches!(molfile::read(&input), Err(ReadError::Limit)));

    // Fixed columns must reject a UTF-8 boundary without unchecked slicing.
    for column in 0..80 {
        for character in ['\0', '酸', '🧪'] {
            let atom = format!(
                "{}{}{}",
                " ".repeat(column),
                character,
                " ".repeat(80 - column)
            );
            let input = format!("\n\n\n  1  0  0  0  0  0  0  0  0  0999 V2000\n{atom}\nM  END\n");
            assert!(molfile::read(&input).is_err());
        }
    }
}
