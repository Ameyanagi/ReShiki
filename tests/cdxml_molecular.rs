use anyhow::Context;
use reshiki::chemistry::{RDKIT_VERSION, cdxml};
use serde::Deserialize;
use serde_json::Value;
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

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
fn difference(a: &Value, b: &Value, path: &str) -> Option<String> {
    if a == b {
        return None;
    }
    match (a, b) {
        (Value::Object(a), Value::Object(b)) if a.len() == b.len() => {
            for (key, b) in b {
                if let Some(d) = difference(
                    a.get(key).unwrap_or(&Value::Null),
                    b,
                    &format!("{path}.{key}"),
                ) {
                    return Some(d);
                }
            }
            return None;
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                if let Some(d) = difference(a, b, &format!("{path}[{i}]")) {
                    return Some(d);
                }
            }
            return None;
        }
        _ => (),
    }
    Some(format!("{path}: {a} != {b}"))
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
        .arg(root.join("tests/cdxml_molecular_reference.py"))
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
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        let result = cdxml::read(&case.text);
        let failure = if let Some(restriction) = &case.restriction {
            // Raw hydrogen order belongs to the pre-normalization stage. Its
            // normalized single-bond case is compared with the native parser.
            anyhow::ensure!(
                case.application_accepted == Some(case.name == "restriction/order/hydrogen"),
                "Application boundary changed for {}: {restriction}",
                case.name
            );
            if case.native_accepted {
                restricted_native += 1;
            } else {
                restricted_invalid += 1;
            }
            if result.is_ok() {
                Some(format!("Accepted intentional restriction: {restriction}"))
            } else {
                None
            }
        } else {
            match (&result, &case.expected) {
                (Ok(actual), Some(expected)) => {
                    accepted += 1;
                    difference(&serde_json::to_value(actual)?, expected, "parsed")
                }
                (Err(_), None) => {
                    rejected += 1;
                    None
                }
                (Err(error), Some(_)) => Some(format!("Rejected native input: {error}")),
                (Ok(_), None) => Some(format!("Accepted native failure: {:?}", case.failure)),
            }
        };
        if let Some(failure) = failure {
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
                if let Ok(actual) = result {
                    std::fs::write(
                        root.join("artifacts/cdxml-molecular-actual.json"),
                        serde_json::to_vec_pretty(&actual)?,
                    )?;
                }
            }
            failures.push(format!("{}: {failure}", case.name));
        }
    }
    assert!(child.wait()?.success());
    eprintln!(
        "CDXML: {accepted} native accepted, {rejected} native rejected; {restricted_native} deliberate restrictions of native accepted inputs, {restricted_invalid} restricted native failures"
    );
    assert!(accepted > 1500);
    assert!(rejected > 0);
    anyhow::ensure!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures
            .iter()
            .take(30)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
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
