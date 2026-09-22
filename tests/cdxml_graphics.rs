use anyhow::Context;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use reshiki::{
    chemistry::{
        RDKIT_VERSION,
        cdxml::{graphics, presentation},
    },
    graphics::Graphic,
    pictures,
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::BTreeSet,
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

#[derive(Deserialize)]
struct Pixels {
    width: u32,
    height: u32,
    pixels: String,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    xml: String,
    scale: f64,
    first_id: u64,
    claimed: BTreeSet<usize>,
    expected: Option<Value>,
    document: Option<Value>,
    bindings: Value,
    error: Option<String>,
    error_type: Option<String>,
    layer_boundary: bool,
    id_boundary: bool,
    images: bool,
    image_error: Option<String>,
    image_pixels: Vec<Pixels>,
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
/// Independent actual bridge boundary: original JSON already decoded to Value;
/// normalize its deferred payload, then deserialize Graphic through from_value.
fn wire_document(mut value: Value) -> anyhow::Result<Vec<Graphic>> {
    let mut budget = pictures::exchange::Budget::default();
    for graphic in value.as_array_mut().context("Expected graphic list")? {
        if let Some(source) = graphic.get("picture_source") {
            let bytes = STANDARD.decode(source["data"].as_str().context("Picture bytes")?)?;
            let picture = pictures::exchange::import(
                &bytes,
                source["format"].as_str().context("Picture format")?,
                source["opacity"].as_f64().context("Opacity")?,
                &mut budget,
            )
            .map_err(anyhow::Error::msg)?;
            let object = graphic.as_object_mut().context("Graphic")?;
            object.remove("picture_source");
            object.insert("picture".into(), serde_json::to_value(picture)?);
        }
    }
    let graphics: Vec<Graphic> = serde_json::from_value(value)?;
    for g in &graphics {
        g.validate().map_err(anyhow::Error::msg)?;
    }
    Ok(graphics)
}

#[test]
fn direct_original_graphics_and_actual_deferred_picture_wire_match() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut child = Command::new(root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    }))
    .arg(root.join("tests/cdxml_graphics_reference.py"))
    .env("PYTHONUTF8", "1")
    .stdout(Stdio::piped())
    .stderr(Stdio::inherit())
    .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().context("Oracle output")?).lines();
    let header: Value = serde_json::from_str(&lines.next().context("Oracle header")??)?;
    assert_eq!(header["rdkit_version"], RDKIT_VERSION);
    let (mut accepted, mut rejected, mut converted, mut bounded, mut decoded) = (0, 0, 0, 0, 0);
    let mut failures = Vec::new();
    for line in lines {
        // Preserve exact worker transport: JSON -> Value(f64) -> typed Case.
        let case: Case = serde_json::from_value(serde_json::from_str(&line?)?)?;
        let document = presentation::parse(&case.xml)?;
        let before = case.claimed.clone();
        let actual = graphics::read(
            document.root_element(),
            case.scale,
            case.first_id,
            &case.claimed,
        );
        assert_eq!(before, case.claimed);
        assert_eq!(document.input_text(), case.xml);
        if case.id_boundary {
            assert!(
                matches!(actual, Err(graphics::Error::IdBoundary)),
                "{}",
                case.name
            );
            bounded += 1;
            continue;
        }
        let actual = match (actual, &case.expected) {
            (Ok(actual), Some(expected)) => {
                accepted += 1;
                if let Some(error) =
                    difference(&serde_json::to_value(&actual.graphics)?, expected, "native")
                {
                    failures.push(format!("{}: {error}", case.name));
                }
                assert_eq!(
                    serde_json::to_value(&actual.bindings)?,
                    case.bindings,
                    "{} bindings",
                    case.name
                );
                actual
            }
            (Err(actual), None) if Some(actual.to_string()) == case.error => {
                rejected += 1;
                continue;
            }
            (Err(actual), _) => {
                failures.push(format!(
                    "{}: Rust {actual}; native {:?}: {:?}",
                    case.name, case.error_type, case.error
                ));
                continue;
            }
            (Ok(actual), None) => {
                failures.push(format!(
                    "{}: Rust accepted {actual:?}; native {:?}",
                    case.name, case.error
                ));
                continue;
            }
        };
        let actual = actual.into_document();
        if case.layer_boundary {
            assert!(
                matches!(actual, Err(graphics::Error::LayerBoundary)),
                "{}",
                case.name
            );
            bounded += 1;
            continue;
        }
        let expected = wire_document(case.document.context("Missing document")?);
        match (actual, expected) {
            (Ok(actual), Ok(expected)) => {
                converted += 1;
                if let Some(error) = difference(
                    &serde_json::to_value(&actual)?,
                    &serde_json::to_value(expected)?,
                    "document",
                ) {
                    failures.push(format!("{}: {error}", case.name));
                }
                if case.images {
                    assert!(
                        case.image_error.is_none(),
                        "{}: {:?}",
                        case.name,
                        case.image_error
                    );
                    let pictures: Vec<_> =
                        actual.iter().filter_map(|g| g.picture.as_ref()).collect();
                    assert_eq!(pictures.len(), case.image_pixels.len());
                    for (picture, expected) in pictures.iter().zip(&case.image_pixels) {
                        assert_eq!(
                            (picture.width(), picture.height()),
                            (expected.width, expected.height),
                            "{}",
                            case.name
                        );
                        let pixels = image::load_from_memory(picture.png())?.to_rgba8();
                        let expected = STANDARD.decode(&expected.pixels)?;
                        let tolerance = if case.name.contains("/JPEG/") { 2 } else { 0 };
                        for (i, (a, e)) in pixels.as_raw().iter().zip(&expected).enumerate() {
                            assert!(
                                a.abs_diff(*e) <= if i % 4 == 3 { 0 } else { tolerance },
                                "{} pixel channel {i}: {a} != {e}",
                                case.name
                            );
                        }
                        decoded += 1;
                    }
                }
            }
            (Err(_), Err(_)) => {
                bounded += 1;
            }
            (actual, expected) => failures.push(format!(
                "{}: document status mismatch: {actual:?} / {expected:?}",
                case.name
            )),
        }
    }
    assert!(child.wait()?.success());
    assert!(
        failures.is_empty(),
        "{} differences:\n{}",
        failures.len(),
        failures
            .iter()
            .take(25)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        accepted > 2000 && rejected > 800 && converted > 1500 && bounded > 100 && decoded >= 90
    );
    eprintln!(
        "{accepted} native successes, {rejected} exact errors, {converted} wire/document conversions, {bounded} separate document boundaries, {decoded} independent Pillow images"
    );
    Ok(())
}

#[test]
fn untrusted_nodes_work_and_ids_are_bounded_and_inputs_remain_immutable() -> anyhow::Result<()> {
    let claim = BTreeSet::new();
    let xml = format!(
        "<CDXML><page>{}</page></CDXML>",
        "<group>".repeat(65) + &"</group>".repeat(65)
    );
    let external = roxmltree::Document::parse(&xml)?;
    assert!(matches!(
        graphics::read(external.root_element(), 1., 1, &claim),
        Err(graphics::Error::Presentation(presentation::Error::Limit))
    ));
    let entity = "<!DOCTYPE CDXML [<!ENTITY x 'x'>]><CDXML/>";
    assert!(graphics::parse(entity, 1., 1, &claim).is_err());
    let huge = format!(
        "<CDXML><page><curve CurvePoints='{}'/></page></CDXML>",
        "0 0 ".repeat(1_000_002)
    );
    assert!(matches!(
        graphics::parse(&huge, 1., 1, &claim),
        Err(graphics::Error::Limit)
    ));
    // Raw helper retains paths beyond the document's 20,000-command limit.
    let long = format!(
        "<CDXML><page><curve CurvePoints='{}'/></page></CDXML>",
        "0 0 1 2 3 4 ".repeat(20_001)
    );
    let raw = graphics::parse(&long, 1., 1, &claim)?;
    assert!(matches!(
        raw.into_document(),
        Err(graphics::Error::Document(_))
    ));
    // Charge each inherited scalar parse even though its source is stored once.
    let expensive = format!(
        "<CDXML LineWidth='0.{}1'><page>{}</page></CDXML>",
        "0".repeat(1_000_000),
        "<graphic GraphicType='Line' BoundingBox='0 0 2 2'/>".repeat(101)
    );
    assert!(matches!(
        graphics::parse(&expensive, 1., 1, &claim),
        Err(graphics::Error::Limit)
    ));
    let many = format!(
        "<CDXML><page>{}</page></CDXML>",
        "<graphic GraphicType='Line' BoundingBox='0 0 2 2'/>".repeat(100_001)
    );
    assert!(matches!(
        graphics::parse(&many, 1., 1, &claim),
        Err(graphics::Error::Presentation(presentation::Error::Limit))
    ));
    let xml = "<CDXML><page><graphic GraphicType='Line' BoundingBox='0 0 2 2'/><graphic GraphicType='Line' BoundingBox='bad'/></page></CDXML>";
    let original = xml.to_owned();
    assert!(graphics::parse(xml, 1., u64::MAX, &claim).is_err());
    assert_eq!(xml, original);
    let repeated = format!(
        "<CDXML><page>{}</page></CDXML>",
        "<graphic GraphicType='Line' BoundingBox='0 0 2 2' id='same'/>".repeat(20_000)
    );
    let result = graphics::parse(&repeated, 1., u64::MAX - 19_999, &claim)?;
    assert_eq!(result.graphics.len(), 20_000);
    assert_eq!(result.graphics.last().map(|g| g.id), Some(u64::MAX));
    assert_eq!(result.bindings.last(), Some(&(20_001, u64::MAX)));
    Ok(())
}
