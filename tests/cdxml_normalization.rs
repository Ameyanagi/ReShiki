use anyhow::Context;
use reshiki::chemistry::{RDKIT_VERSION, cdxml};
use serde::Deserialize;
use serde_json::{Value, json};
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
    restriction: Option<String>,
    native: bool,
    native_expected: Option<Value>,
    native_failure: Option<String>,
    application_order: Option<u8>,
}

fn canonical(node: roxmltree::Node<'_, '_>) -> Value {
    let mut text = String::new();
    let mut children = Vec::new();
    for child in node.children() {
        if child.is_element() {
            children.push(canonical(child));
        } else if child.is_text() {
            if let Some(last) = children.last_mut() {
                let tail =
                    last["tail"].as_str().unwrap_or("").to_owned() + child.text().unwrap_or("");
                last["tail"] = json!(tail);
            } else {
                text.push_str(child.text().unwrap_or(""));
            }
        }
    }
    json!({"tag":node.tag_name().name(), "attributes":node.attributes().map(|a| (a.name(), a.value())).collect::<Vec<_>>(), "text":text, "tail":"", "children":children})
}

#[test]
fn normalization_matches_original_worker_and_direct_native_parser() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/cdxml_normalization_reference.py"))
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines =
        BufReader::new(child.stdout.take().context("Missing reference output")?).lines();
    let version: Value =
        serde_json::from_str(&lines.next().context("Missing reference version")??)?;
    assert_eq!(version["rdkit_version"], RDKIT_VERSION);
    assert_eq!(version["chemdraw"], true);
    let (mut accepted, mut rejected, mut restricted, mut native, mut application) = (0, 0, 0, 0, 0);
    let mut failures = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        let before = case.text.clone();
        let result = cdxml::chemistry_xml(&case.text);
        assert_eq!(case.text, before);
        let failure = if case.restriction.is_some() {
            restricted += 1;
            if case.expected.is_none() {
                Some("Restriction was not accepted by original normalization".into())
            } else if result.is_ok() {
                Some("Accepted deliberate XML contract restriction".into())
            } else {
                None
            }
        } else {
            match (&result, &case.expected) {
                (Ok(xml), Some(expected)) => {
                    accepted += 1;
                    let tree = roxmltree::Document::parse(xml)?;
                    let actual = canonical(tree.root_element());
                    if actual != *expected {
                        Some(format!("Tree mismatch: {actual} != {expected}"))
                    } else if case.native {
                        native += 1;
                        match (cdxml::read(xml), &case.native_expected) {
                            (Ok(parsed), Some(expected)) => {
                                let actual = serde_json::to_value(parsed)?;
                                (actual != *expected)
                                    .then(|| format!("Native mismatch: {actual} != {expected}"))
                            }
                            (Err(_), None) => None,
                            (Err(error), Some(_)) => {
                                Some(format!("Rejected native normalized input: {error}"))
                            }
                            (Ok(_), None) => Some(format!(
                                "Accepted native failure: {:?}",
                                case.native_failure
                            )),
                        }
                    } else {
                        None
                    }
                }
                (Err(error), None) => {
                    rejected += 1;
                    (Some(error.to_string())
                        != case
                            .failure
                            .as_ref()
                            .map(|e| format!("Invalid molecular CDXML: {e}")))
                    .then(|| format!("Different failure: {error}; expected {:?}", case.failure))
                }
                (Err(error), Some(_)) => Some(format!("Rejected original success: {error}")),
                (Ok(_), None) => Some(format!("Accepted original failure: {:?}", case.failure)),
            }
        };
        if let Some(order) = case.application_order {
            application += 1;
            assert_eq!(
                order, 0,
                "Original application must preserve hydrogen bond order"
            );
            assert!(result.is_ok());
        }
        if let Some(failure) = failure {
            if failures.is_empty() {
                std::fs::create_dir_all(root.join("artifacts"))?;
                std::fs::write(
                    root.join("artifacts/cdxml-normalization-mismatch.cdxml"),
                    &case.text,
                )?;
                std::fs::write(
                    root.join("artifacts/cdxml-normalization-expected.json"),
                    serde_json::to_vec_pretty(&case.expected)?,
                )?;
                if let Ok(xml) = result {
                    std::fs::write(root.join("artifacts/cdxml-normalization-actual.cdxml"), xml)?;
                }
            }
            failures.push(format!("{}: {failure}", case.name));
        }
    }
    assert!(child.wait()?.success());
    eprintln!(
        "CDXML normalization: {accepted} original successes, {rejected} original failures, {restricted} separate XML restrictions; {native} direct-native pairs, {application} application hydrogen cases"
    );
    anyhow::ensure!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures
            .iter()
            .take(12)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(accepted > 1000 && rejected > 2000 && native > 500);
    assert_eq!(application, 2);
    Ok(())
}

#[test]
fn pictures_are_removed_without_losing_surviving_siblings() -> anyhow::Result<()> {
    use std::fmt::Write;
    let mut text = String::from("<CDXML><page><fragment id='1'><n id='2' p='0 0'/>");
    for i in 0..20_000 {
        write!(
            text,
            "<embeddedobject id='{}'><b Display='invalid'/></embeddedobject>",
            100 + i
        )?;
    }
    text.push_str(
        "<n id='3' p='14.4 0'/><b id='4' B='2' E='3' Order='hydrogen'/></fragment></page></CDXML>",
    );
    let before = text.clone();
    let normalized = cdxml::chemistry_xml(&text)?;
    assert_eq!(text, before);
    let tree = roxmltree::Document::parse(&normalized)?;
    assert_eq!(tree.descendants().filter(|n| n.is_element()).count(), 6);
    let fragment = tree
        .descendants()
        .find(|n| n.has_tag_name("fragment"))
        .context("Missing surviving fragment")?;
    assert_eq!(
        fragment
            .children()
            .filter(|n| n.is_element())
            .map(|n| n.attribute("id"))
            .collect::<Vec<_>>(),
        [Some("2"), Some("3"), Some("4")]
    );
    Ok(())
}

#[test]
fn excessive_and_invalid_xml_fail_without_changing_source() {
    for text in [
        " ".repeat(16 * 1024 * 1024 + 1),
        format!(
            "<CDXML>{}{}</CDXML>",
            "<group>".repeat(70),
            "</group>".repeat(70)
        ),
        "<CDXML><page></CDXML>".into(),
    ] {
        let before = text.clone();
        assert!(cdxml::chemistry_xml(&text).is_err());
        assert_eq!(text, before);
    }
}
