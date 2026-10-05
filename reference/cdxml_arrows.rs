use anyhow::Context;
use reshiki::{
    chemistry::cdxml::{self, NativeArrow, presentation::NativeColor},
    document::{Arrow, Document},
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
    source: usize,
    colors: Vec<[f64; 3]>,
    scale: f64,
    identifier: u64,
    expected: Option<Value>,
    failure: Option<String>,
    failure_type: Option<String>,
    restriction: Option<String>,
}
fn difference(a: &Value, b: &Value, path: &str) -> Option<String> {
    if a == b {
        return None;
    }
    match (a, b) {
        (Value::Number(a), Value::Number(b)) if a.as_f64() == b.as_f64() => None,
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => a
            .iter()
            .zip(b)
            .enumerate()
            .find_map(|(i, (a, b))| difference(a, b, &format!("{path}[{i}]"))),
        (Value::Object(a), Value::Object(b)) if a.len() == b.len() => {
            a.iter().find_map(|(key, a)| {
                difference(
                    a,
                    b.get(key).unwrap_or(&Value::Null),
                    &format!("{path}.{key}"),
                )
            })
        }
        _ => Some(format!("{path}: {a} != {b}")),
    }
}
fn number(v: f64) -> Value {
    if v.is_finite() {
        Value::from(v)
    } else {
        Value::String(
            if v.is_nan() {
                "NaN"
            } else if v.is_sign_positive() {
                "Infinity"
            } else {
                "-Infinity"
            }
            .into(),
        )
    }
}
fn native(arrow: &NativeArrow) -> anyhow::Result<Value> {
    let mut value = serde_json::to_value(arrow)?;
    let object = value.as_object_mut().context("Invalid arrow object")?;
    for (key, point) in [
        ("start", Some(arrow.start)),
        ("end", Some(arrow.end)),
        ("control", arrow.control),
    ] {
        if let Some(point) = point {
            object.insert(
                key.into(),
                serde_json::json!({"x":number(point.x),"y":number(point.y)}),
            );
        }
    }
    Ok(value)
}
fn expected_document(value: &Value) -> anyhow::Result<Document> {
    let arrow: Arrow = serde_json::from_value(value.clone())?;
    let document = Document {
        arrows: vec![arrow],
        ..Document::default()
    };
    document.validate().map_err(anyhow::Error::msg)?;
    Ok(document)
}
#[test]
fn arrows_match_original_helpers_and_document_boundaries() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut child = Command::new(root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    }))
    .arg(root.join("tests/cdxml_arrows_reference.py"))
    .stdout(Stdio::piped())
    .stderr(Stdio::inherit())
    .spawn()?;
    let lines = BufReader::new(child.stdout.take().context("Missing oracle output")?).lines();
    let (mut accepted, mut rejected, mut restricted, mut documents, mut conversion_errors) =
        (0, 0, 0, 0, 0);
    let mut failures = Vec::new();
    let mut transport_traps = 0;
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        let before = case.text.clone();
        let colors = case
            .colors
            .iter()
            .copied()
            .map(NativeColor)
            .collect::<Vec<_>>();
        let colors_before = colors.clone();
        let result = cdxml::ArrowReader::new(&case.text)
            .and_then(|reader| reader.read(case.source, case.scale, &colors, case.identifier));
        assert_eq!(before, case.text);
        assert_eq!(colors, colors_before);
        if case.restriction.is_some() {
            restricted += 1;
            anyhow::ensure!(
                result.is_err() && case.expected.is_some(),
                "{}: invalid restriction",
                case.name
            );
            continue;
        }
        let failure = match (result, case.expected) {
            (Ok(actual), Some(expected)) => {
                accepted += 1;
                let diff = difference(&native(&actual)?, &expected, "arrow");
                let actual_document = actual
                    .into_document()
                    .map_err(anyhow::Error::from)
                    .and_then(|arrow| {
                        let doc = Document {
                            arrows: vec![arrow],
                            ..Document::default()
                        };
                        doc.validate().map_err(anyhow::Error::msg)?;
                        Ok(doc)
                    });
                if let (Ok(correct), Ok(direct)) = (
                    expected_document(&expected),
                    serde_json::from_str::<Arrow>(&serde_json::to_string(&expected)?),
                ) && correct.arrows != vec![direct]
                {
                    transport_traps += 1;
                }
                match (actual_document, expected_document(&expected)) {
                    (Ok(actual), Ok(expected)) => {
                        documents += 1;
                        assert_eq!(actual, expected, "Final Document: {}", case.name);
                    }
                    (Err(_), Err(_)) => conversion_errors += 1,
                    (a, b) => anyhow::bail!("{}: conversion changed {a:?} != {b:?}", case.name),
                }
                diff
            }
            (Err(error), None) => {
                rejected += 1;
                let expected = case.failure.as_deref().unwrap_or("");
                (error.to_string() != expected)
                    .then(|| format!("Error {} != {:?} {expected}", error, case.failure_type))
            }
            (Err(error), Some(_)) => Some(format!("Rejected original: {error}")),
            (Ok(_), None) => Some(format!("Accepted original {:?}", case.failure)),
        };
        if let Some(failure) = failure {
            failures.push(format!("{}: {failure}", case.name));
        }
    }
    assert!(child.wait()?.success());
    eprintln!(
        "CDXML arrows: {accepted} exact helpers, {rejected} original failures, {restricted} separate restrictions; {documents} exact Documents, {conversion_errors} matching conversion rejections; {transport_traps} direct-f32 transport traps"
    );
    anyhow::ensure!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures
            .iter()
            .take(25)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(accepted > 1000 && rejected > 500 && restricted == 2);
    assert!(transport_traps > 0);
    Ok(())
}
#[test]
fn reader_reuses_source_ordinals_for_twenty_thousand_arrows() -> anyhow::Result<()> {
    let mut text = String::from("<CDXML>");
    for _ in 0..20_000 {
        text.push_str("<arrow id='same' Tail3D='0 0 0' Head3D='10 0 0'/>");
    }
    text.push_str("</CDXML>");
    let reader = cdxml::ArrowReader::new(&text)?;
    let colors = vec![NativeColor([0.; 3]); 4];
    for i in 0..20_000usize {
        let arrow = reader.read(i + 1, 1., &colors, u64::try_from(i)? + 100)?;
        assert_eq!(arrow.id, u64::try_from(i)? + 100);
        assert_eq!(arrow.end.x, 10.);
    }
    Ok(())
}
#[test]
fn arrow_reader_bounds_are_atomic() -> anyhow::Result<()> {
    let reader = cdxml::ArrowReader::new("<CDXML><arrow Tail3D='0 0' Head3D='1 2'/></CDXML>")?;
    let colors = vec![NativeColor([0.; 3]); 4];
    assert!(reader.read(usize::MAX, 1., &colors, 1).is_err());
    assert!(reader.read(1, f64::NAN, &colors, 1).is_err());
    assert!(
        reader
            .read(1, 1., &vec![NativeColor([0.; 3]); 100_001], 1)
            .is_err()
    );
    assert!(
        cdxml::ArrowReader::new(&format!("<CDXML>{}</CDXML>", "<x/>".repeat(100_000))).is_err()
    );
    Ok(())
}
