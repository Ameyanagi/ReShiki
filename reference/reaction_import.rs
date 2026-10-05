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
    command.arg(root.join("tests/reaction_import_reference.py"));
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
    let mut all_failures = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        let result = reaction::read_rxn(&case.text);

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
    if !all_failures.is_empty() {
        std::fs::write(
            root.join(format!("artifacts/reaction-import-{queries}-failures.txt")),
            all_failures.join("\n"),
        )?;
    }
    eprintln!(
        "RXN participants (queries={queries}): {accepted} accepted, {rejected} rejected, {mismatches} mismatches"
    );
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
