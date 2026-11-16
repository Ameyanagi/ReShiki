#[path = "support/query_nicknames.rs"]
mod query_nicknames;
#[path = "support/reference_dative.rs"]
mod reference_dative;
#[path = "support/reference_diagnostics.rs"]
mod reference_diagnostics;

use anyhow::Context;
use reshiki::chemistry::cdxml::{self, PreparationStage};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

#[path = "support/reference_annotations.rs"]
mod reference_annotations;

#[derive(Deserialize)]
struct Case {
    name: String,
    text: String,
    expected: Option<Value>,
    failure: Option<String>,
    failure_stage: Option<String>,
    restriction: Option<String>,
    molecular_extension: Option<Value>,
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
#[test]
fn chemical_prefix_matches_original_worker_capture() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut child = Command::new(root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    }))
    .arg(root.join("reference/cdxml_preparation_reference.py"))
    .env("PYTHONUTF8", "1")
    .stdout(Stdio::piped())
    .stderr(Stdio::inherit())
    .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().context("Missing oracle stdout")?).lines();
    let header: Value = serde_json::from_str(&lines.next().context("Missing oracle header")??)?;
    assert_eq!(header["rdkit_version"], reshiki::chemistry::RDKIT_VERSION);
    let (mut accepted, mut errors, mut restricted, mut stages, mut query_atoms) = (0, 0, 0, 0, 0);
    let mut failures = Vec::new();
    let mut extensions = 0;
    let (mut application_accepted, mut application_rejected) = (0, 0);
    let mut empty_preparation = None;
    let mut multipage_extensions = 0;
    let mut scale_oracles = std::collections::HashMap::new();
    let mut multi_scale_oracle = None;
    let mut scaled_extensions = Vec::new();
    let mut default_extension = None;
    let mut reframed_fragments = 0;
    let mut canonicalized_curves = 0;
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
        if case.name == "empty/<page/>" {
            empty_preparation = case.expected.clone();
        }
        if let Some(length) = case.name.strip_prefix("single-scale/")
            && matches!(length, "5" | "30" | "100")
        {
            scale_oracles.insert(length.to_owned(), case.expected.clone().unwrap());
        }
        if case.name == "multi-scale/14.4" {
            multi_scale_oracle = case.expected.clone();
        }
        if matches!(
            case.name.as_str(),
            "multi-scale/5"
                | "multi-scale/30"
                | "multi-scale/100"
                | "single-scale/None"
                | "multi-scale/None"
        ) {
            assert!(case.expected.is_none());
            assert_eq!(case.failure_stage.as_deref(), Some("bonds"));
            assert_eq!(
                case.failure.as_deref(),
                Some("Could not safely associate CDXML bond appearance with its atoms")
            );
            let extension = (case.name, case.text, observed(&actual?)?);
            if extension.0 == "single-scale/None" {
                default_extension = Some(extension);
            } else {
                scaled_extensions.push(extension);
            }
            continue;
        }
        if case.text == "<CDXML><page/><page/></CDXML>" {
            // Two independent empty pages are now supported by preparation.
            // Reuse the unchanged one-page oracle state, adding only the
            // second source page; all numeric/chemical fields stay exact.
            assert_eq!(case.failure_stage.as_deref(), Some("validation"));
            assert_eq!(
                case.failure.as_deref(),
                Some("CDXML contains unsupported drawing objects or multiple pages.")
            );
            let mut expected: Value = empty_preparation
                .clone()
                .context("Missing empty-page oracle")?;
            let children = expected["expanded"]["children"]
                .as_array_mut()
                .context("Missing empty-page children")?;
            assert_eq!(children.len(), 1);
            children.push(children[0].clone());
            let actual = observed(&actual?)?;
            failures.extend(
                reference_diagnostics::numeric_differences(&actual, &expected, "prepared")
                    .into_iter()
                    .map(|error| format!("{}: {error}", case.name)),
            );
            multipage_extensions += 1;
            continue;
        }
        if let Some(expected) = &case.molecular_extension {
            assert!(case.expected.is_none());
            let imported = cdxml::assemble_cdxml(&actual?)?.into_document()?;
            reference_annotations::check(&imported.document, expected)?;
            extensions += 1;
            continue;
        }
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
                let mut expected = reference_dative::preparation(&case.text, expected.clone())
                    .unwrap_or_else(|error| {
                        failures.push(format!("{}: dative expectation: {error}", case.name));
                        expected
                    });
                // Every fragment now uses the same scaled frame as the
                // combined molecule. Slice the *frozen oracle's* combined
                // positions, including CombineMols' exact signed zeros.
                let positions = expected["molecule"]["positions"]
                    .as_array()
                    .unwrap()
                    .clone();
                let fragments = expected["fragments"].as_array_mut().unwrap();
                if fragments.len() > 1 {
                    let mut offset = 0;
                    for fragment in fragments {
                        let end = offset + fragment["positions"].as_array().unwrap().len();
                        fragment["positions"] = json!(&positions[offset..end]);
                        offset = end;
                        reframed_fragments += 1;
                    }
                    assert_eq!(offset, positions.len());
                }
                if case.name == "fixture/arrows-chemdraw.cdxml" {
                    // Keep the old oracle unchanged. Preparation explicitly
                    // canonicalizes these two legacy flags while preserving
                    // their existing modern head attributes.
                    for (index, flags, head) in [(5, "8", "Full"), (6, "32", "HalfLeft")] {
                        let curve = &mut expected["expanded"]["children"][2]["children"][index];
                        assert_eq!(curve["tag"], "curve");
                        let attributes = curve["attributes"].as_object_mut().unwrap();
                        assert_eq!(attributes["CurveType"], flags);
                        assert_eq!(attributes["ArrowheadHead"], head);
                        assert!(!attributes.contains_key("ArrowheadTail"));
                        attributes.insert("CurveType".into(), json!("0"));
                        attributes.insert("ArrowheadTail".into(), json!("None"));
                        canonicalized_curves += 1;
                    }
                }
                for error in reference_diagnostics::numeric_differences(
                    &observed(&actual)?,
                    &expected,
                    "prepared",
                ) {
                    failures.push(format!("{}: {error}", case.name));
                }
            }
            (Err(actual), None) => {
                errors += 1;
                if actual.stage == PreparationStage::Parser
                    && actual.cause.to_string().ends_with("query atom")
                    && query_nicknames::present(&case.text)
                {
                    query_atoms += 1;
                } else if serde_json::to_value(actual.stage)?.as_str()
                    != case.failure_stage.as_deref()
                {
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
                    if matches!(actual.cause, cdxml::PreparationCause::Objects) {
                        // The object guard still rejects unsupported/nested
                        // objects; independent top-level pages are supported.
                        assert_eq!(
                            case.failure.as_deref(),
                            Some("CDXML contains unsupported drawing objects or multiple pages."),
                            "{}",
                            case.name
                        );
                        assert_eq!(
                            actual.cause.to_string(),
                            "CDXML contains unsupported drawing objects or nested pages.",
                            "{}",
                            case.name
                        );
                    } else {
                        assert_eq!(
                            Some(actual.cause.to_string()),
                            case.failure,
                            "{}",
                            case.name
                        );
                    }
                }
            }
            (actual, expected) => failures.push(format!(
                "{}: {actual:?} / {expected:?}; {:?}",
                case.name, case.failure
            )),
        }
    }
    anyhow::ensure!(child.wait()?.success(), "Oracle failed");
    let (name, text, actual) = default_extension.context("Missing omitted-default extension")?;
    // Omission now resolves to the drawing style's existing 30pt default.
    // Compare every field with the unchanged explicit-30pt single oracle;
    // only the retained source tree differs, by its missing attribute.
    let mut expected = scale_oracles.get("30").unwrap().clone();
    expected["expanded"] = tree(roxmltree::Document::parse(&text)?.root_element());
    failures.extend(
        reference_diagnostics::numeric_differences(&actual, &expected, "prepared")
            .into_iter()
            .map(|error| format!("{name}: {error}")),
    );
    for (name, text, actual) in &scaled_extensions {
        // The unchanged default-scale two-fragment oracle supplies all
        // chemistry, bindings, and bond appearance. The corresponding
        // unchanged single-fragment oracle independently supplies the exact
        // scaled coordinates and drawing style for each new accepted input.
        let mut expected = multi_scale_oracle
            .clone()
            .context("Missing two-fragment oracle")?;
        let length = name.strip_prefix("multi-scale/").unwrap();
        let single = scale_oracles
            .get(if length == "None" { "30" } else { length })
            .unwrap();
        assert_eq!(expected["fragments"].as_array().unwrap().len(), 2);
        assert_eq!(single["fragments"].as_array().unwrap().len(), 1);
        let first = single["molecule"]["positions"].as_array().unwrap();
        assert_eq!(first.len(), 2);
        let mut second = first.clone();
        // RDKit combines the later conformer with a zero offset before
        // scaling; adding +0 canonicalizes -0 exactly as that oracle does.
        for point in &mut second {
            for axis in ["x", "y", "z"] {
                point[axis] = json!(point[axis].as_f64().unwrap() + 0.0);
            }
        }
        let positions = first.iter().chain(&second).cloned().collect::<Vec<_>>();
        expected["molecule"]["positions"] = json!(positions);
        expected["molecule"]["state"]["conformer"]["positions"] = json!(positions);
        expected["fragments"][0]["positions"] = json!(first);
        expected["fragments"][1]["positions"] = json!(second);
        expected["drawing_style"] = single["drawing_style"].clone();
        expected["conformer_scale"] = single["conformer_scale"].clone();
        expected["expanded"] = tree(roxmltree::Document::parse(text)?.root_element());
        failures.extend(
            reference_diagnostics::numeric_differences(actual, &expected, "prepared")
                .into_iter()
                .map(|error| format!("{name}: {error}")),
        );
    }
    eprintln!(
        "CDXML preparation: {accepted} accepted, {errors} original errors ({stages} earlier bounded stages), {restricted} separately classified restrictions, {query_atoms} refused A/a/Q query nicknames; {} mismatches",
        failures.len()
    );
    eprintln!(
        "Original complete application: {application_accepted} valid Documents, {application_rejected} rejected imports"
    );
    eprintln!(
        "Explicit preparation contracts: {multipage_extensions} independent-page extension, {} explicit scaled-fragment extensions, 2 omitted-BondLength extensions, {reframed_fragments} fragments compared in the frozen combined frame, {canonicalized_curves} canonical legacy curves",
        scaled_extensions.len() - 1
    );
    reference_diagnostics::report("CDXML preparation", &failures)?;
    assert_eq!((accepted, errors, restricted, stages), (2044, 158, 4, 6));
    assert_eq!((application_accepted, application_rejected), (6, 5));
    assert_eq!(extensions, 1);
    assert_eq!(multipage_extensions, 1);
    assert_eq!(scaled_extensions.len(), 4);
    assert_eq!(canonicalized_curves, 2);
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
