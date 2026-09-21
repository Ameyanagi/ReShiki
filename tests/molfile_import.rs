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
fn molecular_file_import_matches_native_reader() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/molfile_import_reference.py"))
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
    let mut pending = BTreeMap::<String, usize>::new();
    let mut failures = Vec::new();
    let mut all_failures = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        let result = molfile::read(&case.text);
        let failure = match (&result, &case.expected) {
            (Ok(actual), Some(expected)) => {
                accepted += 1;
                difference(
                    &serde_json::json!({"state":actual.state,"positions":actual.positions}),
                    expected,
                    "molecule",
                )
            }
            (Err(ReadError::Pending(reason)), _) => {
                *pending.entry((*reason).to_owned()).or_default() += 1;
                None
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
    if !all_failures.is_empty() {
        std::fs::write(
            root.join("artifacts/molfile-import-failures.txt"),
            all_failures.join("\n"),
        )?;
    }
    eprintln!(
        "MOL import: {accepted} accepted, {rejected} rejected, {mismatches} mismatches, pending {pending:?}"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(
        accepted > 1000 && rejected > 100,
        "Insufficient MOL import coverage"
    );
    Ok(())
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
