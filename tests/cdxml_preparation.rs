use anyhow::Context;
use reshiki::chemistry::cdxml::{self, PreparationStage};
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
    failure_stage: Option<String>,
    restriction: Option<String>,
    application_checked: bool,
    application_document: Option<Value>,
    application_failure: Option<String>,
}
fn tree(node: roxmltree::Node<'_, '_>) -> Value {
    let attributes = node
        .attributes()
        .map(|a| (a.name().to_owned(), Value::String(a.value().to_owned())))
        .collect::<serde_json::Map<_, _>>();
    let text = node
        .children()
        .take_while(|n| !n.is_element())
        .filter(|n| n.is_text())
        .filter_map(|n| n.text())
        .collect::<String>();
    let tail = node
        .next_siblings()
        .skip(1)
        .take_while(|n| !n.is_element())
        .filter(|n| n.is_text())
        .filter_map(|n| n.text())
        .collect::<String>();
    json!({"tag":node.tag_name().name(),"attributes":attributes,"text":text,"tail":tail,"children":node.children().filter(|n| n.is_element()).map(tree).collect::<Vec<_>>()})
}
fn observed(value: &cdxml::PreparedCdxml) -> anyhow::Result<Value> {
    let mut result = serde_json::to_value(value)?;
    let object = result.as_object_mut().context("Missing prepared object")?;
    object.remove("expanded_xml");
    object.insert(
        "expanded".into(),
        tree(roxmltree::Document::parse(&value.expanded_xml)?.root_element()),
    );
    for part in object
        .get_mut("fragments")
        .and_then(Value::as_array_mut)
        .context("Missing fragments")?
    {
        let part = part.as_object_mut().context("Missing fragment")?;
        for field in [
            "atom_ids",
            "bond_ids",
            "fuse_labels",
            "bond_cfg",
            "non_explicit_3d_chirality",
            "bond_cip",
        ] {
            part.remove(field);
        }
    }
    Ok(result)
}
fn mismatch(actual: &Value, expected: &Value, path: &str) -> Option<String> {
    match (actual, expected) {
        (Value::Object(a), Value::Object(b)) if a.len() == b.len() => {
            for (k, v) in b {
                if let Some(x) = a.get(k) {
                    if let Some(message) = mismatch(x, v, &format!("{path}.{k}")) {
                        return Some(message);
                    }
                } else {
                    return Some(format!("{path}.{k} missing"));
                }
            }
            return None;
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            for (i, (x, y)) in a.iter().zip(b).enumerate() {
                if let Some(message) = mismatch(x, y, &format!("{path}[{i}]")) {
                    return Some(message);
                }
            }
            return None;
        }
        (Value::Number(a), Value::Number(b)) if a.is_f64() || b.is_f64() => {
            return (a.as_f64().map(f64::to_bits) != b.as_f64().map(f64::to_bits))
                .then(|| format!("{path}: {actual} != {expected}"));
        }
        _ if actual == expected => return None,
        _ => (),
    }
    Some(format!("{path}: {actual} != {expected}"))
}
#[test]
fn chemical_prefix_matches_original_worker_capture() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut child = Command::new(root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    }))
    .arg(root.join("tests/cdxml_preparation_reference.py"))
    .env("PYTHONUTF8", "1")
    .stdout(Stdio::piped())
    .stderr(Stdio::inherit())
    .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().context("Missing oracle stdout")?).lines();
    let header: Value = serde_json::from_str(&lines.next().context("Missing oracle header")??)?;
    assert_eq!(header["rdkit_version"], reshiki::chemistry::RDKIT_VERSION);
    let (mut accepted, mut errors, mut restricted, mut stages) = (0, 0, 0, 0);
    let mut failures = Vec::new();
    let (mut application_accepted, mut application_rejected) = (0, 0);
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        if case.application_checked {
            if let Some(document) = case.application_document.clone() {
                // Actual engine transport parses response JSON into Value first.
                let document: reshiki::document::Document = serde_json::from_value(document)?;
                document.validate().map_err(anyhow::Error::msg)?;
                application_accepted += 1;
            } else {
                anyhow::ensure!(
                    case.application_failure.is_some(),
                    "Missing application failure"
                );
                application_rejected += 1;
            }
        }
        let before = case.text.clone();
        let actual = cdxml::prepare_cdxml(&case.text);
        assert_eq!(before, case.text);
        if case.restriction.is_some() {
            restricted += 1;
            if !(case.expected.is_some() && actual.is_err()) {
                failures.push(format!("{}: invalid restriction {actual:?}", case.name));
            }
            continue;
        }
        match (actual, case.expected) {
            (Ok(actual), Some(expected)) => {
                accepted += 1;
                if let Some(error) = mismatch(&observed(&actual)?, &expected, "") {
                    failures.push(format!("{}: {error}", case.name));
                }
            }
            (Err(actual), None) => {
                errors += 1;
                if serde_json::to_value(actual.stage)?.as_str() != case.failure_stage.as_deref() {
                    stages += 1;
                    anyhow::ensure!(
                        actual.stage == PreparationStage::Parser
                            && case.failure_stage.as_deref() == Some("style")
                            && matches!(
                                case.name.as_str(),
                                "single-scale/nan"
                                    | "multi-scale/nan"
                                    | "single-scale/inf"
                                    | "multi-scale/inf"
                                    | "single-scale/bad"
                                    | "multi-scale/bad"
                            ),
                        "Unexpected preparation error stage for {}: {:?} / {:?}",
                        case.name,
                        actual.stage,
                        case.failure_stage
                    );
                    eprintln!(
                        "Earlier bounded rejection {}: {:?} / {:?}",
                        case.name, actual.stage, case.failure_stage
                    );
                }
                if actual.stage == PreparationStage::Validation
                    && matches!(
                        actual.cause,
                        cdxml::PreparationCause::Predicate(_) | cdxml::PreparationCause::Objects
                    )
                {
                    assert_eq!(
                        Some(actual.cause.to_string()),
                        case.failure,
                        "{}",
                        case.name
                    );
                }
            }
            (actual, expected) => failures.push(format!(
                "{}: {actual:?} / {expected:?}; {:?}",
                case.name, case.failure
            )),
        }
    }
    anyhow::ensure!(child.wait()?.success(), "Oracle failed");
    eprintln!(
        "CDXML preparation: {accepted} accepted, {errors} original errors ({stages} earlier bounded stages), {restricted} separately classified restrictions; {} mismatches",
        failures.len()
    );
    eprintln!(
        "Original complete application: {application_accepted} valid Documents, {application_rejected} rejected imports"
    );
    anyhow::ensure!(
        failures.is_empty(),
        "{}",
        failures
            .iter()
            .take(35)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(accepted > 2000 && errors > 160 && restricted == 4 && stages == 6);
    assert_eq!((application_accepted, application_rejected), (6, 5));
    Ok(())
}
#[test]
fn detached_preparation_has_bounded_work_and_stable_binding_ids() -> anyhow::Result<()> {
    let mut xml = String::from("<CDXML BondLength=\"14.4\"><page><fragment id=\"17\">");
    for i in 0..20_000usize {
        xml.push_str(&format!(
            "<n id=\"{}\" p=\"{} {}\"/>",
            i + 1,
            i % 200,
            i / 200
        ));
    }
    xml.push_str("</fragment></page></CDXML>");
    let prepared = cdxml::prepare_cdxml(&xml)?;
    assert_eq!(prepared.molecule.ids.len(), 20_000);
    assert_eq!(prepared.molecule.ids.last(), Some(&20_000));
    assert_eq!(prepared.fragment_bindings.len(), 1);
    let binding = prepared
        .fragment_bindings
        .first()
        .context("Missing binding")?;
    assert_eq!(binding.source, 2);
    assert_eq!(binding.atoms, prepared.molecule.ids);
    let deep = format!(
        "<CDXML><page>{}<n/>{}</page></CDXML>",
        "<group>".repeat(65),
        "</group>".repeat(65)
    );
    assert!(cdxml::prepare_cdxml(&deep).is_err());
    Ok(())
}
