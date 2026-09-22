use anyhow::Context;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use reshiki::{
    chemistry::{
        RDKIT_VERSION,
        cdxml::{assemble_cdxml, import_cdxml, prepare_cdxml},
    },
    document::Document,
    pictures,
};
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
    scene: Option<Value>,
    scene_error: Option<String>,
    document: Option<Value>,
    document_error: Option<String>,
    restriction: Option<String>,
}
fn difference(actual: &Value, expected: &Value, path: &str) -> Option<String> {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b))
            if a.as_f64().map(f64::to_bits) == b.as_f64().map(f64::to_bits) =>
        {
            return None;
        }
        (Value::Object(a), Value::Object(b)) if a.len() == b.len() => {
            for (key, value) in b {
                if let Some(d) = difference(
                    a.get(key).unwrap_or(&Value::Null),
                    value,
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
        _ if !actual.is_number()
            && !actual.is_array()
            && !actual.is_object()
            && actual == expected =>
        {
            return None;
        }
        _ => (),
    }
    Some(format!("{path}: {actual} != {expected}"))
}
// Exact production boundary: the response line was parsed into Value first.
fn wire_document(mut value: Value) -> anyhow::Result<Document> {
    let mut budget = pictures::exchange::Budget::default();
    for graphic in value["graphics"].as_array_mut().context("graphics")? {
        if let Some(source) = graphic.get("picture_source") {
            let bytes = STANDARD.decode(source["data"].as_str().context("picture bytes")?)?;
            let picture = pictures::exchange::import(
                &bytes,
                source["format"].as_str().context("picture format")?,
                source["opacity"].as_f64().context("opacity")?,
                &mut budget,
            )
            .map_err(anyhow::Error::msg)?;
            let object = graphic.as_object_mut().context("graphic")?;
            object.remove("picture_source");
            object.insert("picture".into(), serde_json::to_value(picture)?);
        }
    }
    let document: Document = serde_json::from_value(value)?;
    document.validate().map_err(anyhow::Error::msg)?;
    Ok(document)
}
#[test]
fn complete_original_scene_and_actual_document_transport() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut child = Command::new(root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    }))
    .arg(root.join("tests/cdxml_scene_reference.py"))
    .env("PYTHONUTF8", "1")
    .stdout(Stdio::piped())
    .stderr(Stdio::inherit())
    .spawn()?;
    let lines = BufReader::new(child.stdout.take().context("oracle output")?).lines();
    let (mut accepted, mut rejected, mut converted, mut bounds, mut restrictions) = (0, 0, 0, 0, 0);
    let mut failures = Vec::new();
    let (mut restricted_native, mut restricted_documents, mut final_only_rejected) = (0, 0, 0);
    for line in lines {
        let value: Value = serde_json::from_str(&line?)?;
        if value.get("name").is_none() {
            assert_eq!(value["rdkit_version"], RDKIT_VERSION);
            continue;
        }
        let case: Case = serde_json::from_value(value)?;
        let prepared = prepare_cdxml(&case.text);
        let actual = match prepared {
            Ok(prepared) => {
                let before = serde_json::to_value(&prepared)?;
                let result = assemble_cdxml(&prepared);
                assert_eq!(
                    serde_json::to_value(&prepared)?,
                    before,
                    "{} immutable",
                    case.name
                );
                result
            }
            Err(error) => Err(error.into()),
        };
        if case.restriction.is_some() {
            restrictions += 1;
            assert!(actual.is_err(), "{} restriction", case.name);
            restricted_native += usize::from(case.scene.is_some());
            restricted_documents +=
                usize::from(case.document.is_some_and(|v| wire_document(v).is_ok()));
            continue;
        }
        match (&actual, &case.scene) {
            (Ok(actual), Some(expected)) => {
                accepted += 1;
                if let Some(d) = difference(&serde_json::to_value(actual)?, expected, "scene") {
                    failures.push(format!("{} {d}", case.name));
                }
            }
            (Err(_), None) => rejected += 1,
            (Err(error), Some(_)) => {
                failures.push(format!("{} native accepted, Rust {error}", case.name))
            }
            (Ok(_), None) => failures.push(format!(
                "{} Rust accepted, native {:?}",
                case.name, case.scene_error
            )),
        }
        let actual = actual.and_then(|s| s.into_document());
        let expected = case
            .document
            .map(wire_document)
            .unwrap_or_else(|| Err(anyhow::anyhow!("{:?}", case.document_error)));
        match (actual, expected) {
            (Ok(actual), Ok(expected)) => {
                converted += 1;
                if let Some(d) = difference(
                    &serde_json::to_value(actual.document)?,
                    &serde_json::to_value(expected)?,
                    "document",
                ) {
                    failures.push(format!("{} {d}", case.name));
                }
            }
            (Err(_), Err(_)) => {
                bounds += 1;
                final_only_rejected += usize::from(case.scene.is_some());
            }
            (Err(error), Ok(_)) => {
                failures.push(format!("{} document accepted, Rust {error}", case.name))
            }
            (Ok(_), Err(error)) => failures.push(format!(
                "{} document rejected: {error}; Rust accepted",
                case.name
            )),
        }
    }
    assert!(child.wait()?.success());
    eprintln!(
        "scene: {accepted} native accepts, {rejected} native rejects, {converted} document accepts, {bounds} document rejects, {restrictions} explicit restrictions; {} mismatches",
        failures.len()
    );
    for failure in failures.iter() {
        eprintln!("{failure}");
    }
    assert!(failures.is_empty());
    assert_eq!(
        (accepted, rejected, converted, bounds),
        (2645, 287, 2344, 588)
    );
    assert_eq!(final_only_rejected, 301);
    assert_eq!(
        (restrictions, restricted_native, restricted_documents),
        (4, 2, 2)
    );
    eprintln!(
        "Final-only rejections: {final_only_rejected}; safety restrictions accepted by original: {restricted_native} native, {restricted_documents} Documents"
    );
    Ok(())
}
#[test]
fn bounded_large_scene_and_non_cdxml_roots() -> anyhow::Result<()> {
    for xml in [
        "<page><t p='0 0'><s>caption</s></t></page>",
        "<group><page><t p='0 0'><s>caption</s></t></page></group>",
    ] {
        let imported = import_cdxml(xml)?;
        assert_eq!(imported.document.annotations.len(), 1);
    }
    let mut xml = String::from("<CDXML><page>");
    for i in 0..10_000 {
        xml.push_str(&format!("<t id='same' p='{i} 0'><s>x</s></t>"));
    }
    xml.push_str("</page></CDXML>");
    let imported = import_cdxml(&xml)?;
    assert_eq!(imported.document.annotations.len(), 10_000);
    assert!(import_cdxml("<CDXML><page><scheme/></page></CDXML>").is_err());
    Ok(())
}

#[test]
fn hydrogen_appearance_is_restored_before_final_validation_atomically() -> anyhow::Result<()> {
    let xml = "<CDXML BondLength='14.4'><page><fragment id='2'><n id='1' Element='8' p='0 0'/><n id='2' Element='8' p='14.4 0'/><b B='1' E='2' Order='hydrogen'/></fragment><t p='0 20'><s>caption</s></t></page></CDXML>";
    let prepared = prepare_cdxml(xml)?;
    let original = serde_json::to_value(&prepared)?;
    let scene = assemble_cdxml(&prepared)?;
    let imported = scene.clone().into_document()?;
    let bond = imported.document.bonds.first().context("hydrogen bond")?;
    assert_eq!(bond.order, 0);
    assert_eq!(bond.display, "dotted");
    let mut broken = scene.clone();
    broken
        .base
        .annotations
        .first_mut()
        .context("caption")?
        .position
        .x = f64::NAN;
    assert!(broken.into_document().is_err());
    assert_eq!(serde_json::to_value(&prepared)?, original);
    assert_eq!(scene.into_document()?.document, imported.document);
    let mut too_large = String::from("<CDXML><page>");
    for _ in 0..100_000 {
        too_large.push_str("<group/>");
    }
    too_large.push_str("</page></CDXML>");
    assert!(prepare_cdxml(&too_large).is_err());
    assert_eq!(serde_json::to_value(&prepared)?, original);
    Ok(())
}

#[test]
fn scene_midpoint_uses_response_value_transport() -> anyhow::Result<()> {
    let wire = r#"{"version":15,"atoms":[],"bonds":[],"graphics":[],"annotations":[{"id":1,"position":{"x":249.96781158447266,"y":0},"text":"x"}]}"#;
    let value: Value = serde_json::from_str(wire)?;
    let correct = wire_document(value)?;
    let direct: Document = serde_json::from_str(wire)?;
    assert_ne!(correct, direct, "fixture exposes direct-f32 JSON rounding");
    let x = 249.96781158447266_f64 / (1.0 / (14.4 / 42.0));
    let scene = import_cdxml(&format!(
        "<CDXML><page><t p='{x} 0'><s>x</s></t></page></CDXML>"
    ))?;
    assert_eq!(
        scene
            .document
            .annotations
            .first()
            .context("caption")?
            .position,
        correct.annotations.first().context("reference")?.position
    );
    Ok(())
}
