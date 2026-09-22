use anyhow::Context;
use reshiki::chemistry::cdxml;
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
    error: Option<String>,
    restriction: Option<String>,
    application_accepted: Option<bool>,
    allowed_application_acceptance: bool,
    application_expectation: Option<bool>,
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
    json!({"tag":node.tag_name().name(), "attributes":node.attributes().map(|a| (a.name(), a.value())).collect::<std::collections::BTreeMap<_,_>>(), "text":text, "tail":"", "children":children})
}

#[test]
fn flattening_matches_original_python_tree_and_metadata() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/cdxml_abbreviations_reference.py"))
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let lines = BufReader::new(child.stdout.take().context("Missing reference output")?).lines();
    let (mut accepted, mut rejected, mut restricted) = (0, 0, 0);
    let (mut restricted_original_success, mut restricted_application_success) = (0, 0);
    let mut failures = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        if let Some(expected) = case.application_expectation {
            anyhow::ensure!(
                case.application_accepted == Some(expected),
                "Original application acceptance changed for {}",
                case.name
            );
        }
        let before = case.text.clone();
        let result = cdxml::flatten_abbreviations(&case.text);
        assert_eq!(case.text, before, "Input was modified: {}", case.name);
        let failure = if case.restriction.is_some() {
            restricted += 1;
            restricted_original_success += usize::from(case.expected.is_some());
            restricted_application_success += usize::from(case.application_accepted == Some(true));
            if case.application_accepted == Some(true) && !case.allowed_application_acceptance {
                Some("Restriction rejects an application-accepted input".to_owned())
            } else if result.is_ok() {
                Some("Accepted explicit restriction".to_owned())
            } else {
                None
            }
        } else {
            match (&result, &case.expected) {
                (Ok(actual), Some(expected)) => {
                    accepted += 1;
                    let document = roxmltree::Document::parse(&actual.xml)?;
                    let value = json!({"tree":canonical(document.root_element()), "abbreviations":actual.abbreviations});
                    (value != *expected).then(|| format!("{value} != {expected}"))
                }
                (Err(error), None) => {
                    rejected += 1;
                    // Preserve the explicit application diagnostics. Python's
                    // float conversion and KeyError strings are runtime text.
                    case.error
                        .as_ref()
                        .filter(|expected| {
                            !expected.starts_with("could not convert string to float:")
                                && !expected.starts_with("<Element ")
                        })
                        .and_then(|expected| {
                            (error.to_string() != format!("Invalid molecular CDXML: {expected}"))
                                .then(|| format!("Different failure: {error}; expected {expected}"))
                        })
                }
                (Err(error), Some(_)) => Some(format!("Rejected original success: {error}")),
                (Ok(_), None) => Some(format!("Accepted original failure: {:?}", case.error)),
            }
        };
        if let Some(failure) = failure {
            if failures.is_empty() {
                std::fs::create_dir_all(root.join("artifacts"))?;
                std::fs::write(
                    root.join("artifacts/cdxml-abbreviation-mismatch.cdxml"),
                    &case.text,
                )?;
                std::fs::write(
                    root.join("artifacts/cdxml-abbreviation-expected.json"),
                    serde_json::to_vec_pretty(&case.expected)?,
                )?;
                if let Ok(actual) = result {
                    std::fs::write(
                        root.join("artifacts/cdxml-abbreviation-actual.xml"),
                        actual.xml,
                    )?;
                }
            }
            failures.push(format!("{}: {failure}", case.name));
        }
    }
    assert!(child.wait()?.success());
    eprintln!(
        "CDXML abbreviations: {accepted} original successes, {rejected} original failures; {restricted} explicit restrictions ({restricted_original_success} original successes, {restricted_application_success} application successes)"
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
    assert!(accepted > 4000);
    assert!(rejected > 40);
    Ok(())
}

#[test]
fn limits_and_invalid_xml_leave_source_unchanged() {
    let sources = [
        " ".repeat(16 * 1024 * 1024 + 1),
        format!(
            "<CDXML>{}{}</CDXML>",
            "<group>".repeat(70),
            "</group>".repeat(70)
        ),
        "<CDXML><page></CDXML>".into(),
        "<CDXML xmlns:x='urn:x'><page x:id='2'/></CDXML>".into(),
        "<!DOCTYPE CDXML [<!ENTITY x 'bad'>]><CDXML>&x;</CDXML>".into(),
    ];
    for source in sources {
        let before = source.clone();
        assert!(cdxml::flatten_abbreviations(&source).is_err());
        assert_eq!(source, before);
    }
}

#[test]
fn expanded_ids_and_attachment_feed_the_molecular_reader() -> anyhow::Result<()> {
    let source = "<CDXML BondLength='14.4'><page><fragment id='1'><n id='2' p='0 0'/><n id='3' p='14.4 0' NodeType='Fragment'><fragment id='4'><n id='5' p='0 0' Element='8'/><n id='6' p='14.4 0'/><n id='7' p='-14.4 0' NodeType='ExternalConnectionPoint'/><b id='8' B='7' E='5'/><b id='9' B='5' E='6'/></fragment><t><s>Me</s><s>O</s></t></n><b id='10' B='2' E='3'/></fragment></page></CDXML>";
    let flat = cdxml::flatten_abbreviations(source)?;
    let group = flat.abbreviations.first().context("Missing abbreviation")?;
    assert_eq!(group.label, "OMe");
    assert_eq!(group.reverse_label, "MeO");
    assert_eq!(group.anchor.as_deref(), Some("5"));
    assert_eq!(group.members, [Some("5".into()), Some("6".into())]);
    let parsed = cdxml::read(&flat.xml)?;
    let fragment = parsed.fragments.first().context("Missing fragment")?;
    assert_eq!(fragment.atom_ids, [2, 5, 6]);
    assert_eq!(fragment.bond_ids, [10, 9]);
    assert_eq!(fragment.graph.bonds.len(), 2);
    assert_eq!(fragment.positions.len(), 3);
    Ok(())
}

fn many_abbreviations(count: usize) -> anyhow::Result<String> {
    use std::fmt::Write;
    let mut text = String::from("<CDXML><page><fragment id='1'>");
    for i in 0..count {
        let outer = 100 + 3 * i;
        let inner = outer + 1;
        let atom = outer + 2;
        write!(
            text,
            "<n id='{outer}' p='{i} 0' NodeType='Fragment'><fragment id='{inner}'><n id='{atom}' p='0 0'/></fragment><t><s>Me</s></t></n>"
        )?;
    }
    text.push_str("</fragment></page></CDXML>");
    Ok(text)
}

#[test]
fn many_groups_keep_all_atoms_and_excessive_work_fails_atomically() -> anyhow::Result<()> {
    let text = many_abbreviations(500)?;
    let flat = cdxml::flatten_abbreviations(&text)?;
    assert_eq!(flat.abbreviations.len(), 500);
    let doc = roxmltree::Document::parse(&flat.xml)?;
    let nodes = doc
        .descendants()
        .filter(|n| n.has_tag_name("n"))
        .collect::<Vec<_>>();
    assert_eq!(nodes.len(), 500);
    assert!(nodes.iter().all(|n| n.attribute("NodeType").is_none()));
    for (i, node) in nodes.iter().enumerate() {
        assert_eq!(
            node.attribute("id"),
            Some((102 + 3 * i).to_string().as_str())
        );
    }
    let text = many_abbreviations(4000)?;
    let before = text.clone();
    assert!(matches!(
        cdxml::flatten_abbreviations(&text),
        Err(cdxml::Error::Limit)
    ));
    assert_eq!(text, before);
    Ok(())
}
