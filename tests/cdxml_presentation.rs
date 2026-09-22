use anyhow::Context;
use reshiki::{
    chemistry::{RDKIT_VERSION, cdxml::presentation},
    style::DrawingStyle,
    typography::TextFormat,
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
    xml: String,
    mode: String,
    defaults: Option<presentation::Attributes>,
    atom: bool,
    element_xml: Option<String>,
    palette: Option<Value>,
    expected: Option<Value>,
    error: Option<String>,
    error_type: Option<String>,
}
#[derive(Deserialize)]
struct DocumentText {
    text: String,
    format: TextFormat,
}

fn difference(actual: &Value, expected: &Value, path: &str) -> Option<String> {
    if actual == expected {
        return None;
    }
    match (actual, expected) {
        (Value::Number(a), Value::Number(e)) if a.as_f64() == e.as_f64() => return None,
        (Value::Object(a), Value::Object(e)) if a.len() == e.len() => {
            for (key, value) in e {
                if let Some(error) = difference(
                    a.get(key).unwrap_or(&Value::Null),
                    value,
                    &format!("{path}.{key}"),
                ) {
                    return Some(error);
                }
            }
            return None;
        }
        (Value::Array(a), Value::Array(e)) if a.len() == e.len() => {
            for (i, (a, e)) in a.iter().zip(e).enumerate() {
                if let Some(error) = difference(a, e, &format!("{path}[{i}]")) {
                    return Some(error);
                }
            }
            return None;
        }
        _ => (),
    }
    Some(format!("{path}: {actual} != {expected}"))
}

#[test]
fn native_precision_styles_text_and_document_conversion_match_original_functions()
-> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut child = Command::new(root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    }))
    .arg(root.join("tests/cdxml_presentation_reference.py"))
    .env("PYTHONUTF8", "1")
    .stdout(Stdio::piped())
    .stderr(Stdio::inherit())
    .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().context("Missing oracle output")?).lines();
    let header: Value = serde_json::from_str(&lines.next().context("Missing native header")??)?;
    assert_eq!(header["rdkit_version"], RDKIT_VERSION);
    assert!(
        difference(
            &serde_json::to_value(presentation::NativeDrawingStyle::defaults()?)?,
            &header["defaults"],
            "defaults"
        )
        .is_none()
    );
    let (mut accepted, mut rejected, mut converted, mut document_boundaries) = (0, 0, 0, 0);
    let mut failures = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_value(serde_json::from_str(&line?)?)?;
        let xml = presentation::parse(&case.xml)?;
        let external = case
            .element_xml
            .as_deref()
            .map(presentation::parse)
            .transpose()?;
        let defaults_before = case.defaults.clone();
        let mut actual_document = None;
        let mut expected_document = None;
        let actual = if case.mode == "style" {
            presentation::drawing_style(xml.root_element()).map(|style| {
                let value = serde_json::to_value(&style);
                actual_document = Some(style.into_document().and_then(|doc| {
                    serde_json::to_value(doc)
                        .map_err(|e| presentation::Error::Document(e.to_string()))
                }));
                value
            })
        } else {
            presentation::TextReader::new(xml.root_element()).and_then(|reader| {
                if let Some(palette) = &case.palette {
                    let actual = serde_json::to_value(&reader.palette().colors)
                        .map_err(|error| presentation::Error::Document(error.to_string()))?;
                    if let Some(error) = difference(&actual, palette, "palette") {
                        failures.push(format!("{}: {error}", case.name));
                    }
                }
                if case.mode == "palette" {
                    return Ok(serde_json::to_value(&reader.palette().colors));
                }
                let element = if let Some(external) = &external {
                    external.root_element()
                } else {
                    xml.descendants()
                        .find(|n| n.has_tag_name("t"))
                        .ok_or(presentation::Error::Runs)?
                };
                reader
                    .read(element, case.defaults.as_ref(), case.atom)
                    .map(|text| {
                        let value = serde_json::to_value(&text);
                        actual_document = Some(text.into_document().and_then(|(text, format)| {
                            serde_json::to_value(serde_json::json!({"text":text,"format":format}))
                                .map_err(|e| presentation::Error::Document(e.to_string()))
                        }));
                        value
                    })
            })
        };
        assert_eq!(case.defaults, defaults_before, "Defaults mutated");
        if let Some(expected) = &case.expected {
            expected_document = match case.mode.as_str() {
                "style" => Some(
                    serde_json::from_value::<DrawingStyle>(expected.clone())
                        .map_err(|e| e.to_string())
                        .and_then(|style| {
                            style.validate()?;
                            serde_json::to_value(style).map_err(|e| e.to_string())
                        }),
                ),
                "text" => Some(
                    serde_json::from_value::<DocumentText>(expected.clone())
                        .map_err(|e| e.to_string())
                        .and_then(|text| {
                            text.format.validate(&text.text)?;
                            Ok(serde_json::json!({"text":text.text,"format":text.format}))
                        }),
                ),
                _ => None,
            };
        }
        let error = match (actual, &case.expected) {
            (Ok(actual), Some(expected)) => {
                accepted += 1;
                difference(&actual?, expected, "native")
            }
            (Err(actual), None) if Some(actual.to_string()) == case.error => {
                rejected += 1;
                let overflow = matches!(actual, presentation::Error::ColorInfinite);
                (overflow != (case.error_type.as_deref() == Some("OverflowError")))
                    .then(|| "Error class differs".into())
            }
            (Err(actual), expected) => Some(format!(
                "Unexpected error {actual}; expected {expected:?}, {:?}",
                case.error
            )),
            (Ok(actual), None) => Some(format!(
                "Unexpected success {actual:?}; expected {:?}",
                case.error
            )),
        };
        if let Some(error) = error
            && failures.len() < 30
        {
            failures.push(format!("{}: {error}", case.name));
        }
        if let Some((actual, expected)) = actual_document.zip(expected_document) {
            match (actual, expected) {
                (Ok(actual), Ok(expected)) => {
                    converted += 1;
                    if let Some(error) = difference(&actual, &expected, "document")
                        && failures.len() < 30
                    {
                        failures.push(format!("{}: {error}", case.name));
                    }
                }
                (Err(_), Err(_)) => document_boundaries += 1,
                (actual, expected) => {
                    if failures.len() < 30 {
                        failures.push(format!(
                            "{}: Document boundary {actual:?} != {expected:?}",
                            case.name
                        ));
                    }
                }
            }
        }
    }
    assert!(
        child.wait()?.success(),
        "Oracle failed: {}",
        failures.join("\n")
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(
        accepted > 2000 && rejected > 300 && converted > 1500 && document_boundaries > 10,
        "Incomplete corpus {accepted}/{rejected}/{converted}/{document_boundaries}"
    );
    eprintln!(
        "Verified {accepted} native presentation successes, {rejected} exact errors, {converted} document conversions and {document_boundaries} separate document rejections"
    );
    Ok(())
}

#[test]
fn external_nodes_and_defaults_remain_bounded_while_context_is_reused() -> anyhow::Result<()> {
    let entity = "<!DOCTYPE CDXML [<!-- <!ENTITY inert 'x'> --><!ENTITY actual 'x'>]><CDXML><t><s>&actual;</s></t></CDXML>";
    assert!(matches!(
        presentation::parse(entity),
        Err(presentation::Error::Limit)
    ));
    let expanded = roxmltree::Document::parse_with_options(
        entity,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    )?;
    assert!(matches!(
        presentation::TextReader::new(expanded.root_element()),
        Err(presentation::Error::Limit)
    ));
    let context = presentation::parse("<CDXML/>")?;
    let reader = presentation::TextReader::new(context.root_element())?;
    let deep = format!("{}{}", "<t>".repeat(70), "</t>".repeat(70));
    let external = roxmltree::Document::parse(&deep)?;
    assert!(matches!(
        reader.read(external.root_element(), None, true),
        Err(presentation::Error::Limit)
    ));
    assert!(matches!(
        presentation::drawing_style(external.root_element()),
        Err(presentation::Error::Limit)
    ));
    let external = presentation::parse("<t><s/></t>")?;
    let defaults =
        presentation::Attributes::from([("LabelFont".into(), "x".repeat(16 * 1024 * 1024))]);
    assert!(matches!(
        reader.read(external.root_element(), Some(&defaults), true),
        Err(presentation::Error::Limit)
    ));
    let many = format!("<CDXML>{}</CDXML>", "<t><s>A</s></t>".repeat(20_000));
    let document = presentation::parse(&many)?;
    let reader = presentation::TextReader::new(document.root_element())?;
    for node in document.root_element().children() {
        assert_eq!(reader.read(node, None, false)?.text, "A");
    }
    let too_many = format!("<CDXML>{}</CDXML>", "<s/>".repeat(100_001));
    let document = roxmltree::Document::parse(&too_many)?;
    assert!(matches!(
        presentation::TextReader::new(document.root_element()),
        Err(presentation::Error::Limit)
    ));
    let comments = format!("<t><s/>{}</t>", "<!--x-->".repeat(400_000));
    let document = roxmltree::Document::parse(&comments)?;
    assert!(matches!(
        reader.read(document.root_element(), None, true),
        Err(presentation::Error::Limit)
    ));
    Ok(())
}
