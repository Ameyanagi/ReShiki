use anyhow::Context;
use reshiki::chemistry::{RDKIT_VERSION, cdxml};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

#[path = "support/reference_dative.rs"]
mod reference_dative;

#[derive(Deserialize)]
struct Case {
    name: String,
    text: String,
    expected: Option<Value>,
    failure: Option<String>,
    native_accepted: bool,
    restriction: Option<String>,
    application_accepted: Option<bool>,
}
fn differences(a: &Value, b: &Value, path: &str, output: &mut Vec<(String, String)>) {
    if a == b {
        return;
    }
    match (a, b) {
        (Value::Object(a), Value::Object(b)) => {
            for (key, b) in b {
                let field = format!("{path}.{key}");
                if let Some(a) = a.get(key) {
                    differences(a, b, &field, output);
                } else {
                    output.push((field.clone(), format!("{field}: missing != {b}")));
                }
            }
            for (key, a) in a {
                if !b.contains_key(key) {
                    let field = format!("{path}.{key}");
                    output.push((field.clone(), format!("{field}: {a} != missing")));
                }
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                output.push((
                    format!("{path}.length"),
                    format!("{path}.length: {} != {}", a.len(), b.len()),
                ));
            }
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                differences(a, b, &format!("{path}[{i}]"), output);
            }
            for (i, a) in a.iter().enumerate().skip(b.len()) {
                let field = format!("{path}[{i}]");
                output.push((field.clone(), format!("{field}: {a} != missing")));
            }
            for (i, b) in b.iter().enumerate().skip(a.len()) {
                let field = format!("{path}[{i}]");
                output.push((field.clone(), format!("{field}: missing != {b}")));
            }
        }
        _ => output.push((path.to_owned(), format!("{path}: {a} != {b}"))),
    }
}

fn category(path: &str) -> String {
    let mut inside_index = false;
    path.chars()
        .filter(|&c| {
            if c == '[' {
                inside_index = true;
                true
            } else if c == ']' {
                inside_index = false;
                true
            } else {
                !inside_index
            }
        })
        .collect()
}

#[test]
fn molecular_cdxml_matches_direct_native_reader() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("reference/cdxml_molecular_reference.py"))
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines =
        BufReader::new(child.stdout.take().context("Missing reference output")?).lines();
    let version: Value =
        serde_json::from_str(&lines.next().context("Missing reference version")??)?;
    assert_eq!(version["rdkit_version"], RDKIT_VERSION);
    assert_eq!(version["chemdraw"], true);
    let (mut accepted, mut rejected, mut restricted_native, mut restricted_invalid) = (0, 0, 0, 0);
    let mut failures = Vec::new();
    let mut category_counts = BTreeMap::<String, usize>::new();
    let mut mismatch_cases = Vec::new();
    let mut intended_changes = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        let result = cdxml::read(&case.text);
        let actual = result.as_ref().ok().map(serde_json::to_value).transpose()?;
        let mut case_failures = Vec::new();
        // Retain the unmodified direct-native observation. The shared contract
        // validates its exact old dative values from source XML before deriving
        // the documented new values, without consulting the Rust result.
        let mut intended_expected = case.expected.clone();
        if let Some(expected) = &case.expected {
            match reference_dative::normalized_fragments(&case.text, expected.clone()) {
                Ok(intended) => {
                    let mut changes = Vec::new();
                    differences(&intended, expected, "parsed", &mut changes);
                    if !changes.is_empty() {
                        intended_changes.push(serde_json::json!({
                            "name": case.name,
                            "text": case.text,
                            "original_expected": expected,
                            "intended_expected": intended,
                            "changes": changes,
                        }));
                    }
                    intended_expected = Some(intended);
                }
                Err(error) => {
                    case_failures.push((
                        "original_dative_contract".to_owned(),
                        format!("Original dative contract changed: {error:#}"),
                    ));
                    intended_expected = None;
                }
            }
        }
        if let Some(restriction) = &case.restriction {
            // Raw hydrogen order belongs to the pre-normalization stage. Its
            // normalized single-bond case is compared with the native parser.
            if case.application_accepted != Some(case.name == "restriction/order/hydrogen") {
                case_failures.push((
                    "application_boundary".to_owned(),
                    format!("Application boundary changed: {restriction}"),
                ));
            }
            if case.native_accepted {
                restricted_native += 1;
            } else {
                restricted_invalid += 1;
            }
            if result.is_ok() {
                case_failures.push((
                    "accepted_restriction".to_owned(),
                    format!("Accepted intentional restriction: {restriction}"),
                ));
            }
        } else {
            match (&result, &case.expected) {
                (Ok(_), Some(_)) => {
                    accepted += 1;
                    if let Some(expected) = &intended_expected {
                        differences(
                            actual.as_ref().unwrap(),
                            expected,
                            "parsed",
                            &mut case_failures,
                        );
                    }
                }
                (Err(_), None) => {
                    rejected += 1;
                }
                (Err(error), Some(_)) => case_failures.push((
                    "rejected_native_input".to_owned(),
                    format!("Rejected native input: {error}"),
                )),
                (Ok(_), None) => case_failures.push((
                    "accepted_native_failure".to_owned(),
                    format!("Accepted native failure: {:?}", case.failure),
                )),
            }
        }
        if !case_failures.is_empty() {
            if failures.is_empty() {
                std::fs::create_dir_all(root.join("artifacts"))?;
                std::fs::write(
                    root.join("artifacts/cdxml-molecular-mismatch.cdxml"),
                    &case.text,
                )?;
                std::fs::write(
                    root.join("artifacts/cdxml-molecular-expected.json"),
                    serde_json::to_vec_pretty(&case.expected)?,
                )?;
                std::fs::write(
                    root.join("artifacts/cdxml-molecular-intended.json"),
                    serde_json::to_vec_pretty(&intended_expected)?,
                )?;
                if let Some(actual) = &actual {
                    std::fs::write(
                        root.join("artifacts/cdxml-molecular-actual.json"),
                        serde_json::to_vec_pretty(&actual)?,
                    )?;
                }
            }
            for (path, failure) in &case_failures {
                *category_counts.entry(category(path)).or_default() += 1;
                failures.push(format!("{}: {failure}", case.name));
            }
            mismatch_cases.push(serde_json::json!({
                "name": case.name,
                "text": case.text,
                "expected": case.expected,
                "intended_expected": intended_expected,
                "actual": actual,
                "native_failure": case.failure,
                "actual_failure": result.as_ref().err().map(ToString::to_string),
                "restriction": case.restriction,
                "application_accepted": case.application_accepted,
                "differences": case_failures,
            }));
        }
    }
    assert!(child.wait()?.success());
    eprintln!(
        "CDXML: {accepted} native accepted, {rejected} native rejected; {restricted_native} deliberate restrictions of native accepted inputs, {restricted_invalid} restricted native failures"
    );
    let categories = category_counts
        .iter()
        .map(|(name, count)| format!("{name}: {count}"))
        .collect::<Vec<_>>()
        .join("\n");
    eprintln!(
        "{} mismatching cases, {} diagnostics; categories:\n{categories}",
        mismatch_cases.len(),
        failures.len()
    );
    std::fs::create_dir_all(root.join("artifacts"))?;
    std::fs::write(
        root.join("artifacts/cdxml-molecular-failures.txt"),
        format!("{categories}\n\n{}\n", failures.join("\n")),
    )?;
    std::fs::write(
        root.join("artifacts/cdxml-molecular-mismatches.json"),
        serde_json::to_vec_pretty(&mismatch_cases)?,
    )?;
    std::fs::write(
        root.join("artifacts/cdxml-molecular-intended-changes.json"),
        serde_json::to_vec_pretty(&intended_changes)?,
    )?;
    eprintln!(
        "{} independently validated intended dative changes",
        intended_changes.len()
    );
    assert!(accepted > 1500);
    assert!(rejected > 0);
    anyhow::ensure!(
        failures.is_empty(),
        "{} mismatching cases, {} diagnostics:\n{}",
        mismatch_cases.len(),
        failures.len(),
        failures.join("\n")
    );
    Ok(())
}

#[test]
fn malformed_and_excessive_inputs_fail_atomically() {
    for text in [
        "<CDXML><page><fragment id='1'><n id='2' p='nan 0'/></fragment></page></CDXML>",
        "<CDXML><page><fragment id='1'><n id='2' p='32768 0'/></fragment></page></CDXML>",
        "<CDXML><page><fragment id='1'><n id='2'/><b id='3' B='2' E='2'/></fragment></page></CDXML>",
        "<!DOCTYPE CDXML [<!ENTITY molecule 'C'>]><CDXML/>",
    ] {
        assert!(cdxml::read(text).is_err(), "{text}");
    }
    assert!(matches!(
        cdxml::read(&" ".repeat(16 * 1024 * 1024 + 1)),
        Err(cdxml::Error::Limit)
    ));
    let deep = format!(
        "<CDXML><page>{}<fragment id='1'><n id='2'/></fragment>{}</page></CDXML>",
        "<group>".repeat(70),
        "</group>".repeat(70)
    );
    assert!(matches!(cdxml::read(&deep), Err(cdxml::Error::Limit)));
}

#[test]
fn large_disconnected_fragment_keeps_every_atom() -> anyhow::Result<()> {
    use std::fmt::Write;
    let mut text = String::from("<CDXML BondLength='14.4'><page><fragment id='50000'>");
    for id in 1..=20_000 {
        write!(text, "<n id='{id}' p='0 0'/>")?;
    }
    text.push_str("</fragment></page></CDXML>");
    let parsed = cdxml::read(&text)?;
    let part = parsed.fragments.first().context("Missing large fragment")?;
    assert_eq!(part.graph.atoms.len(), 20_000);
    assert_eq!(part.atom_ids.first(), Some(&1));
    assert_eq!(part.atom_ids.last(), Some(&20_000));
    assert_eq!(part.positions.len(), 20_000);
    assert!(part.graph.bonds.is_empty());
    Ok(())
}
