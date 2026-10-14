use anyhow::Context;
use reshiki::chemistry::{RDKIT_VERSION, reaction};
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
fn reaction_participants_match_native_reader() -> anyhow::Result<()> {
    compare_reference(false)
}

#[test]
fn reaction_query_unwrapping_matches_native_reader() -> anyhow::Result<()> {
    compare_reference(true)
}

fn compare_reference(queries: bool) -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut command = Command::new(python);
    command.arg(root.join("reference/reaction_import_reference.py"));
    if queries {
        command.arg("--queries");
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
            .context("Missing RXN reader reference output")?,
    )
    .lines();
    let version: Value =
        serde_json::from_str(&lines.next().context("Missing reference version")??)?;
    assert_eq!(version["rdkit_version"], RDKIT_VERSION);
    let (mut accepted, mut rejected, mut mismatches) = (0, 0, 0);

    let mut failures = Vec::new();
    let mut categories = BTreeMap::<String, usize>::new();
    let mut restrictions = BTreeMap::<String, usize>::new();
    let mut fields = BTreeMap::<String, usize>::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        let result = reaction::read_rxn(&case.text);

        let failure = if let Some(restriction) = &case.restriction {
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
            match (&result, &case.expected) {
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
            }
        };
        if let Some(error) = failure {
            let category = if let Some(restriction) = &case.restriction {
                format!("Restriction contract: {}", restriction.category)
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
                    root.join(format!("artifacts/reaction-import-{queries}-mismatch.json")),
                    serde_json::to_vec_pretty(&case)?,
                )?;
                if let Ok(actual) = result {
                    std::fs::write(
                        root.join(format!("artifacts/reaction-import-{queries}-actual.json")),
                        serde_json::to_vec_pretty(&actual)?,
                    )?;
                }
            }
        }
    }
    assert!(child.wait()?.success(), "RXN reader reference failed");
    if !failures.is_empty() {
        std::fs::write(
            root.join(format!("artifacts/reaction-import-{queries}-failures.txt")),
            failures.join("\n"),
        )?;
    }
    let restricted = restrictions.values().sum::<usize>();
    eprintln!(
        "RXN participants (queries={queries}): {accepted} accepted, {rejected} rejected, {restricted} restricted, {mismatches} mismatches"
    );
    for (category, count) in categories {
        eprintln!("RXN import mismatch category: {count} {category}");
    }
    for (category, count) in restrictions {
        eprintln!("RXN import restriction category: {count} {category}");
    }
    eprintln!("RXN import: {} mismatch diagnostics", failures.len());
    for (field, count) in fields {
        eprintln!("RXN import mismatch field: {count} {field}");
    }
    assert!(mismatches == 0, "{}", failures.join("\n"));
    assert!(
        accepted > if queries { 50 } else { 1000 } && rejected > 100,
        "Insufficient RXN coverage"
    );
    Ok(())
}

#[test]
fn malformed_reaction_input_is_bounded() {
    use reshiki::chemistry::molfile::ReadError;
    for counts in [
        "10001 1",
        "1 10001",
        "1 1 10001",
        "-1 1",
        "1 -1",
        "1 1 -1",
        "18446744073709551616 1",
    ] {
        let input = format!("$RXN V3000\n\n\n\nM  V30 COUNTS {counts}\n");
        assert!(reaction::read_rxn(&input).is_err());
    }
    assert!(matches!(
        reaction::read_rxn(&" ".repeat(16 * 1024 * 1024 + 1)),
        Err(ReadError::Limit)
    ));
    for column in 0..12 {
        for character in ['\0', '酸', '🧪'] {
            let row = format!(
                "{}{}{}",
                " ".repeat(column),
                character,
                " ".repeat(12 - column)
            );
            assert!(reaction::read_rxn(&format!("$RXN\n\n\n\n{row}\n")).is_err());
        }
    }
    let ctab = "M  V30 BEGIN CTAB\nM  V30 COUNTS 1 0 0 0 0\nM  V30 BEGIN ATOM\nM  V30 1 O 0 0 0 0\nM  V30 END ATOM\nM  V30 END CTAB\n";
    let input = format!(
        "$RXN V3000\n\n\n\nM  V30 COUNTS 1 1 9999\nM  V30 BEGIN REACTANT\n{ctab}M  V30 END REACTANT\nM  V30 BEGIN PRODUCT\n{ctab}M  V30 END PRODUCT\nM  V30 BEGIN AGENT\n{}M  V30 END AGENT\n",
        ctab.repeat(9999)
    );
    assert!(matches!(reaction::read_rxn(&input), Err(ReadError::Limit)));
}
