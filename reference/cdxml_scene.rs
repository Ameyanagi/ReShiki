#[path = "support/empty_heads.rs"]
mod empty_heads;
#[path = "support/reference_dative.rs"]
mod reference_dative;
#[path = "support/reference_diagnostics.rs"]
mod reference_diagnostics;

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

#[path = "support/reference_annotations.rs"]
mod reference_annotations;

#[derive(Deserialize)]
struct Case {
    name: String,
    text: String,
    scene: Option<Value>,
    scene_error: Option<String>,
    document: Option<Value>,
    document_error: Option<String>,
    restriction: Option<String>,
    molecular_extension: Option<Value>,
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
    .arg(root.join("reference/cdxml_scene_reference.py"))
    .env("PYTHONUTF8", "1")
    .stdout(Stdio::piped())
    .stderr(Stdio::inherit())
    .spawn()?;
    let lines = BufReader::new(child.stdout.take().context("oracle output")?).lines();
    let (mut accepted, mut rejected, mut converted, mut bounds, mut restrictions) = (0, 0, 0, 0, 0);
    let mut failures = Vec::new();
    let mut extensions = 0;
    let mut dative_extensions = 0;
    let (mut omitted_default, mut explicit_default) = (None, None);
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
        if case.name == "preparation/single-scale/None" {
            // The historical importer rejected this omitted default during
            // bond association. Preserve that recorded outcome, then compare
            // the extension with its independently captured explicit control.
            assert!(case.scene.is_none() && case.document.is_none());
            let error = "Could not safely associate CDXML bond appearance with its atoms";
            assert_eq!(case.scene_error.as_deref(), Some(error));
            assert_eq!(case.document_error.as_deref(), Some(error));
            omitted_default = Some((case.text, actual?));
            continue;
        }
        if case.name == "preparation/single-scale/30" {
            explicit_default = Some((
                case.text.clone(),
                case.scene
                    .clone()
                    .context("Missing explicit-default scene")?,
                case.document
                    .clone()
                    .context("Missing explicit-default document")?,
            ));
        }
        if let Some(expected) = &case.molecular_extension {
            // The old importer rejects annotations and multiple abbreviation
            // attachments. This newly accepted fixture uses direct RDKit
            // chemistry instead; it is not a complete legacy scene comparison.
            assert!(case.scene.is_none() && case.document.is_none());
            let imported = actual?.into_document()?;
            reference_annotations::check(&imported.document, expected)?;
            extensions += 1;
            continue;
        }
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
                let expected = reference_dative::scene(&case.text, expected.clone())
                    .unwrap_or_else(|error| {
                        failures.push(format!("{}: dative expectation: {error}", case.name));
                        expected.clone()
                    });
                for d in reference_diagnostics::numeric_differences(
                    &empty_heads::without(serde_json::to_value(actual)?),
                    &expected,
                    "scene",
                ) {
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
            .map(|legacy| {
                let original = wire_document(legacy.clone());
                let adapted = reference_dative::document(&case.text, legacy.clone())
                    .unwrap_or_else(|error| {
                        failures.push(format!("{}: dative expectation: {error}", case.name));
                        legacy
                    });
                let intended = wire_document(adapted);
                if let Err(original_error) = original
                    && intended.is_ok()
                {
                    if !reference_dative::has_projection(&case.text).unwrap_or(false)
                        || original_error.to_string()
                            != "Unsupported bond appearance for this order"
                    {
                        failures.push(format!(
                            "{}: unexpected original dative document failure",
                            case.name
                        ));
                    }
                    dative_extensions += 1;
                }
                intended
            })
            .unwrap_or_else(|| Err(anyhow::anyhow!("{:?}", case.document_error)));
        match (actual, expected) {
            (Ok(actual), Ok(expected)) => {
                converted += 1;
                for d in reference_diagnostics::numeric_differences(
                    &empty_heads::without(serde_json::to_value(actual.document)?),
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
    let (omitted_text, actual) = omitted_default.context("Missing omitted-default extension")?;
    let (explicit_text, expected_scene, expected_document) =
        explicit_default.context("Missing explicit-default oracle")?;
    // This is an exact source identity check: only the root's explicit 30pt
    // attribute differs. Compare every scene and document field with the
    // unchanged Python worker's successful explicit-default result.
    assert_eq!(explicit_text.matches(" BondLength=\"30\"").count(), 1);
    assert_eq!(
        omitted_text,
        explicit_text.replacen(" BondLength=\"30\"", "", 1)
    );
    for difference in reference_diagnostics::numeric_differences(
        &empty_heads::without(serde_json::to_value(&actual)?),
        &expected_scene,
        "scene",
    ) {
        failures.push(format!("omitted-BondLength extension {difference}"));
    }
    let expected = wire_document(expected_document)?;
    for difference in reference_diagnostics::numeric_differences(
        &empty_heads::without(serde_json::to_value(actual.into_document()?.document)?),
        &serde_json::to_value(expected)?,
        "document",
    ) {
        failures.push(format!("omitted-BondLength extension {difference}"));
    }
    eprintln!("Omitted-BondLength extension: 1 exact explicit-30pt scene and document comparison");
    eprintln!(
        "scene: {accepted} native accepts, {rejected} native rejects, {converted} document accepts, {bounds} document rejects, {restrictions} explicit restrictions; {} mismatches",
        failures.len()
    );
    eprintln!(
        "Projected-dative document extensions: {dative_extensions}; final-only rejections: {final_only_rejected}"
    );
    reference_diagnostics::report("CDXML scene", &failures)?;
    // #792 (9e5ef83b) added tests/fixtures/chemdraw-acs1996-chain-alkenes.cdxml;
    // every top-level fixture runs twice, adding two native and two document accepts.
    // tests/fixtures/aa-nicknames.cdxml (A and a generic nicknames) and #371
    // (519e91c6) tests/fixtures/cobalt-hexammine.cdxml add two more each.
    // #1168 keeps three or four radical electrons, so 46 drawings whose atoms
    // have them now become documents instead of being rejected.
    assert_eq!(
        (accepted, rejected, converted, bounds),
        (2651, 285, 2405, 531)
    );
    // Nine original raw Documents failed the projection requirement. The
    // independently adapted paint now passes the same validation boundary.
    assert_eq!(dative_extensions, 9);
    // The 46 drawings above were final-only rejections before #1168.
    assert_eq!(final_only_rejected, 246);
    assert_eq!(extensions, 1);
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
