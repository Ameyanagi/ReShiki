//! Independent complete-document comparison with the original replacement edit.
use anyhow::Context;
use reshiki::{
    chemistry::{RDKIT_VERSION, abbreviations, document},
    document::{Document, Point},
    engine::{ChemistryEngine, PythonEngine, Request, Response},
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Command, Stdio},
};

#[derive(Deserialize)]
struct Case {
    name: String,
    document: Document,
    selection: Vec<u64>,
    label: String,
    expected: Option<Document>,
    error: Option<String>,
    chemistry_ok: Option<bool>,
}

fn difference(actual: &Value, expected: &Value, path: &str) -> Option<String> {
    if actual == expected {
        return None;
    }
    match (actual, expected) {
        (Value::Object(a), Value::Object(e)) if a.len() == e.len() => {
            for (key, value) in e {
                if let Some(difference) = difference(
                    a.get(key).unwrap_or(&Value::Null),
                    value,
                    &format!("{path}.{key}"),
                ) {
                    return Some(difference);
                }
            }
        }
        (Value::Array(a), Value::Array(e)) if a.len() == e.len() => {
            for (index, (a, e)) in a.iter().zip(e).enumerate() {
                if let Some(difference) = difference(a, e, &format!("{path}[{index}]")) {
                    return Some(difference);
                }
            }
        }
        _ => {}
    }
    Some(format!("{path}: {actual} != {expected}"))
}

#[test]
fn replacement_documents_and_final_chemistry_match_original_worker() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/abbreviation_replacement_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().context("Missing oracle output")?).lines();
    let header: Value = serde_json::from_str(&lines.next().context("Missing geometry header")??)?;
    assert_eq!(header["rdkit_version"], RDKIT_VERSION);
    let data = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => {
            include_str!("../src/chemistry/abbreviations/geometry-macos-aarch64.json")
        }
        ("linux", "x86_64") => {
            include_str!("../src/chemistry/abbreviations/geometry-linux-x86_64.json")
        }
        ("linux", "aarch64") => {
            include_str!("../src/chemistry/abbreviations/geometry-linux-aarch64.json")
        }
        ("windows", "x86_64" | "aarch64") => {
            include_str!("../src/chemistry/abbreviations/geometry-windows-x86_64.json")
        }
        target => anyhow::bail!("Unsupported reference ABI: {target:?}"),
    };
    let geometry: Value = serde_json::from_str(data)?;
    assert!(
        header == geometry,
        "Pinned geometry changed: {:?}",
        difference(&header, &geometry, "geometry")
    );
    let mut count = 0;
    let mut successful = 0;
    let mut rejected_chemistry = 0;
    let mut failures = Vec::new();
    for line in lines {
        // The actual bridge parses the worker line into Value before decoding
        // Response. Direct f32 JSON parsing can round midpoint decimals away
        // from the f64 value that the worker originally emitted.
        let value: Value = serde_json::from_str(&line?)?;
        let case: Case = serde_json::from_value(value)?;
        count += 1;
        let before = case.document.clone();
        let result = abbreviations::replace(&case.document, &case.selection, &case.label);
        assert_eq!(case.document, before, "Input changed: {}", case.name);
        let failure = match (result, &case.expected) {
            (Ok(actual), Some(expected)) => {
                successful += 1;
                let difference = difference(
                    &serde_json::to_value(&actual)?,
                    &serde_json::to_value(expected)?,
                    "document",
                );
                let chemistry = if let Some(allowed) = case.chemistry_ok {
                    rejected_chemistry += usize::from(!allowed);
                    let prepared = document::prepare(&actual);
                    if prepared.is_ok() != allowed {
                        Some(format!(
                            "Final chemistry mismatch: native allowed={allowed}, local={:?}",
                            prepared.err()
                        ))
                    } else {
                        None
                    }
                } else {
                    None
                };
                difference.or(chemistry)
            }
            (Err(error), None) if Some(error.to_string()) == case.error => None,
            (Err(error), None) => Some(format!("{error} != {:?}", case.error)),
            (Err(error), Some(_)) => Some(format!("Unexpected error: {error}")),
            (Ok(_), None) => Some(format!("Accepted native error: {:?}", case.error)),
        };
        if let Some(failure) = failure
            && failures.len() < 40
        {
            failures.push(format!("{}: {failure}", case.name));
        }
    }
    assert!(
        child.wait()?.success(),
        "Oracle failed: {}",
        failures.join("\n")
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(
        count > 5_000 && successful > 4_000 && rejected_chemistry >= 3,
        "Incomplete corpus: {count}/{successful}/{rejected_chemistry}"
    );
    eprintln!(
        "Verified {count} replacement cases, {successful} complete documents, {rejected_chemistry} rejected final chemical states"
    );
    Ok(())
}

#[test]
fn failed_replacement_leaves_its_source_unchanged() -> anyhow::Result<()> {
    let doc: Document = serde_json::from_value(serde_json::json!({"version":15,"atoms":[
        {"id":1,"element":"C","position":{"x":0,"y":0}},
        {"id":2,"element":"C","position":{"x":0,"y":0}}
    ],"bonds":[{"a":1,"b":2,"order":1}],"annotations":[],"arrows":[]}))?;
    let before = doc.clone();
    assert!(matches!(
        abbreviations::replace(&doc, &[1], "OMe"),
        Err(abbreviations::ReplacementError::Geometry)
    ));
    assert!(abbreviations::replace(&doc, &vec![1; 100_001], "OMe").is_err());
    assert_eq!(doc, before);
    let mut infinite = doc.clone();
    infinite
        .atoms
        .first_mut()
        .context("Missing atom")?
        .position
        .x = f32::INFINITY;
    assert!(matches!(
        abbreviations::replace(&infinite, &[1], "OMe"),
        Err(abbreviations::ReplacementError::Geometry)
    ));
    Ok(())
}

#[tokio::test]
async fn replacement_uses_the_actual_request_and_response_numeric_contract() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut oracle = Command::new(python)
        .arg(root.join("tests/abbreviation_replacement_reference.py"))
        .arg("--wire")
        .env("PYTHONUTF8", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut input = oracle.stdin.take().context("Missing oracle input")?;
    let mut output = BufReader::new(oracle.stdout.take().context("Missing oracle output")?);
    let reference = PythonEngine::default();
    let mut cases = 0;
    let mut input_regressions = 0;
    let mut output_regressions = 0;
    for preset in abbreviations::presets()? {
        for (origin, outside) in [
            ((0.1, -0.2), (25.3, 33.4)),
            ((21.25, -17.5), (-3.95, 16.1)),
            ((21.25, -17.5), (46.45, -51.1)),
            ((0.0, 0.0), (25.2, 33.6)),
            ((-234_567.13, 456_789.25), (-231_207.13, 454_269.25)),
            ((0.0, 0.0), (0.15, 0.0)),
        ] {
            let doc: Document = serde_json::from_value(serde_json::json!({
                "version":15,"atoms":[
                    {"id":1,"element":"C","position":Point{x:origin.0,y:origin.1}},
                    {"id":2,"element":"C","position":Point{x:outside.0,y:outside.1}}
                ],"bonds":[{"a":1,"b":2,"order":1}],"annotations":[],"arrows":[]
            }))?;
            let mut request = Request::molecule("abbreviate", doc.clone());
            request.selected_ids = Some(vec![1]);
            request.format = Some("replace".into());
            request.text = Some(preset.label.clone());
            // PythonEngine::request uses to_value, and exchange encodes that
            // Value. Sending a Request directly would shorten f32 decimals.
            let wire = serde_json::to_value(&request)?;
            let direct: Value = serde_json::from_str(&serde_json::to_string(&request)?)?;
            input_regressions += usize::from(doc.atoms.iter().enumerate().any(|(index, _)| {
                wire["document"]["atoms"][index]["position"]
                    != direct["document"]["atoms"][index]["position"]
            }));
            serde_json::to_writer(&mut input, &wire)?;
            writeln!(input)?;
            input.flush()?;
            let mut line = String::new();
            anyhow::ensure!(output.read_line(&mut line)? > 0, "Oracle exited");
            let result: Value = serde_json::from_str(&line)?;
            assert_eq!(result["request"], wire, "Request changed in transit");
            let expected: Document = serde_json::from_value(result["edit"].clone())?;
            let direct: Document = serde_json::from_str(&result["edit"].to_string())?;
            output_regressions += usize::from(direct != expected);
            let actual = abbreviations::replace(&doc, &[1], &preset.label)?;
            let mismatch = difference(
                &serde_json::to_value(&actual)?,
                &serde_json::to_value(&expected)?,
                "document",
            );
            anyhow::ensure!(
                mismatch.is_none(),
                "{} {origin:?}/{outside:?}: {mismatch:?}",
                preset.label
            );
            // Independently check that this wire model produces the complete
            // response returned by the real bridge. The standalone Rust edit
            // comparison above remains independent of runtime dispatch.
            let expected: Response = serde_json::from_value(result["response"].clone())?;
            let actual = reference
                .execute(request)
                .await
                .map_err(anyhow::Error::msg)?;
            let mismatch = difference(
                &serde_json::to_value(actual)?,
                &serde_json::to_value(expected)?,
                "response",
            );
            anyhow::ensure!(mismatch.is_none(), "Bridge wire mismatch: {mismatch:?}");
            cases += 1;
        }
    }
    drop(input);
    assert!(oracle.wait()?.success(), "Wire oracle failed");
    assert!(input_regressions > 0, "No f32 promotion regression covered");
    assert!(output_regressions > 0, "No f64 midpoint regression covered");
    eprintln!(
        "Verified {cases} real bridge responses and independent Rust edits; {input_regressions} shortened-input and {output_regressions} direct-f32-decoding regressions"
    );
    Ok(())
}
