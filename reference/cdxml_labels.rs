use anyhow::Context;
use reshiki::{
    chemistry::{
        RDKIT_VERSION,
        cdxml::{self, presentation},
        graph::Atom,
        stereo::Point3,
    },
    document::Document,
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
    atoms: Vec<u8>,
    positions: Vec<Point3>,
    bonds: Vec<[u64; 2]>,
    scale: f64,
    expected: Option<Value>,
    failure: Option<String>,
    failure_type: Option<String>,
    calls: Value,
    restriction: Option<String>,
}
fn difference(actual: &Value, expected: &Value, path: &str) -> Option<String> {
    if actual == expected {
        return None;
    }
    match (actual, expected) {
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
        _ => Some(format!("{path}: {actual} != {expected}")),
    }
}
fn document(patches: &Value, bonds: &[[u64; 2]]) -> anyhow::Result<Document> {
    let mut converted = patches.clone();
    convert_colors(&mut converted)?;
    let patches = &converted;
    let mut atoms = patches
        .get("atoms")
        .and_then(Value::as_array)
        .context("Missing atom patches")?
        .clone();
    for atom in &mut atoms {
        let atom = atom.as_object_mut().context("Invalid atom patch")?;
        atom.insert("element".into(), Value::String("C".into()));
        atom.insert("position".into(), serde_json::json!({"x":0.,"y":0.}));
    }
    let mut bonds = bonds
        .iter()
        .map(|[a, b]| serde_json::json!({"a":a,"b":b,"order":1}))
        .collect::<Vec<_>>();
    for patch in patches
        .get("bonds")
        .and_then(Value::as_array)
        .context("Missing bond patches")?
    {
        let index = usize::try_from(
            patch
                .get("index")
                .and_then(Value::as_u64)
                .context("Missing bond index")?,
        )?;
        bonds
            .get_mut(index)
            .and_then(Value::as_object_mut)
            .context("Invalid bond patch index")?
            .insert(
                "indicator".into(),
                patch.get("indicator").context("Missing indicator")?.clone(),
            );
    }
    // PythonEngine first parses the response as Value, then decodes Response.
    let document: Document = serde_json::from_value(
        serde_json::json!({"version":15,"atoms":atoms,"bonds":bonds,"atom_labels":patches.get("atom_labels")}),
    )?;
    document.validate().map_err(anyhow::Error::msg)?;
    Ok(document)
}
#[test]
fn label_patches_and_callback_order_match_original_reader() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut child = Command::new(root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    }))
    .arg(root.join("tests/cdxml_labels_reference.py"))
    .stdout(Stdio::piped())
    .stderr(Stdio::inherit())
    .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().context("Missing oracle output")?).lines();
    let version: Value = serde_json::from_str(&lines.next().context("Missing version")??)?;
    assert_eq!(version["rdkit_version"], RDKIT_VERSION);
    let (mut accepted, mut rejected, mut restrictions, mut documents, mut document_errors) =
        (0, 0, 0, 0, 0);
    let mut failures = Vec::new();
    let mut boundaries = std::collections::BTreeMap::<String, usize>::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        let atoms = case
            .atoms
            .iter()
            .map(|&atomic_number| Atom {
                atomic_number,
                ..Atom::default()
            })
            .collect::<Vec<_>>();
        let before = serde_json::to_value((&atoms, &case.positions, &case.bonds, &case.text))?;
        let prepared = cdxml::PreparedAtoms::new(&atoms, &case.positions)?;
        let xml = presentation::parse(&case.text)?;
        let nodes = xml
            .descendants()
            .filter(|n| n.is_element())
            .collect::<Vec<_>>();
        let reader = presentation::TextReader::new(xml.root_element())?;
        let mut calls = Vec::new();
        let result = cdxml::read_labels(
            &case.text,
            &prepared,
            &case.bonds,
            case.scale,
            |source, attrs| {
                calls.push(serde_json::json!({"source":source,"attributes":attrs}));
                let node = nodes
                    .get(source)
                    .copied()
                    .ok_or(presentation::Error::Runs)?;
                reader.read(node, Some(attrs), false)
            },
        );
        assert_eq!(
            before,
            serde_json::to_value((&atoms, &case.positions, &case.bonds, &case.text))?
        );
        let failure = if case.restriction.is_some() {
            restrictions += 1;
            (result.is_ok() || case.expected.is_none())
                .then(|| "Incorrect explicit restriction".to_owned())
        } else {
            if let Some(error) = difference(&serde_json::to_value(calls)?, &case.calls, "callback")
            {
                failures.push(format!("{}: {error}", case.name));
            }
            match (&result, &case.expected) {
                (Ok(actual), Some(expected)) => {
                    accepted += 1;
                    let actual = serde_json::to_value(actual)?;
                    match (
                        document(&actual, &case.bonds),
                        document(expected, &case.bonds),
                    ) {
                        (Ok(a), Ok(b)) => {
                            documents += 1;
                            assert_eq!(a, b, "Final Document: {}", case.name);
                        }
                        (Err(a), Err(b)) => {
                            document_errors += 1;
                            *boundaries.entry(a.to_string()).or_default() += 1;
                            assert_eq!(
                                a.to_string(),
                                b.to_string(),
                                "Document conversion: {}",
                                case.name
                            );
                        }
                        _ => anyhow::bail!("Document conversion acceptance changed: {}", case.name),
                    }
                    difference(&actual, expected, "labels")
                }
                (Err(error), None) => {
                    rejected += 1;
                    let expected = case.failure.as_deref().unwrap_or("");
                    let actual = error.to_string();
                    let actual = actual
                        .strip_prefix("Invalid molecular CDXML: ")
                        .unwrap_or(&actual);
                    (case.failure_type.as_deref() == Some("ValueError")
                        && !expected.starts_with("could not convert")
                        && !expected.starts_with("invalid literal")
                        && actual != expected)
                        .then(|| format!("Error {actual} != {expected}"))
                }
                (Err(error), Some(_)) => Some(format!("Rejected original: {error}")),
                (Ok(_), None) => Some(format!("Accepted original error {:?}", case.failure)),
            }
        };
        if let Some(failure) = failure {
            failures.push(format!("{}: {failure}", case.name));
        }
    }
    assert!(child.wait()?.success());
    eprintln!(
        "CDXML labels: {accepted} exact helpers, {rejected} original failures, {restrictions} separate restrictions; {documents} exact Documents, {document_errors} matched conversion errors"
    );
    eprintln!("Label document boundaries: {boundaries:?}");
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
    assert_eq!((accepted, rejected, restrictions), (1289, 392, 2));
    assert_eq!((documents, document_errors), (1278, 11));
    Ok(())
}
#[test]
fn large_label_reader_shares_prepared_association_and_uses_bond_index() -> anyhow::Result<()> {
    use std::fmt::Write;
    let mut text = String::from("<CDXML ShowAtomStereo='yes' ShowBondStereo='yes'>");
    let mut atoms = Vec::new();
    let mut positions = Vec::new();
    let mut bonds = Vec::new();
    for i in 0..20_000u32 {
        let x = f64::from(i) * 2.;
        write!(text, "<n id='{i}' p='{x} 0'>")?;
        if i % 10 == 0 {
            text.push_str("<objecttag Name='stereo'><t><s>R</s></t></objecttag>");
        }
        text.push_str("</n>");
        atoms.push(Atom {
            atomic_number: 6,
            ..Atom::default()
        });
        positions.push(Point3 {
            x: x / 28.,
            y: 0.,
            z: 0.,
        });
        if i > 0 {
            write!(text, "<b B='{}' E='{i}'>", i - 1)?;
            if i % 10 == 0 {
                text.push_str("<objecttag Name='stereo'><t><s>E</s></t></objecttag>");
            }
            text.push_str("</b>");
            bonds.push([u64::from(i), u64::from(i) + 1]);
        }
    }
    text.push_str("</CDXML>");
    bonds.reverse();
    let prepared = cdxml::PreparedAtoms::new(&atoms, &positions)?;
    let xml = presentation::parse(&text)?;
    let reader = presentation::TextReader::new(xml.root_element())?;
    let elements = xml
        .descendants()
        .filter(|n| n.is_element())
        .collect::<Vec<_>>();
    let mut calls = 0;
    let output = cdxml::read_labels(&text, &prepared, &bonds, 1., |source, attrs| {
        calls += 1;
        reader.read(
            *elements.get(source).ok_or(presentation::Error::Runs)?,
            Some(attrs),
            false,
        )
    })?;
    assert_eq!(output.atoms.len(), 20_000);
    assert_eq!(output.bonds.len(), 19_999);
    assert_eq!(calls, 3999);
    for (i, bond) in output.bonds.iter().enumerate() {
        assert_eq!(bond.index, 19_998 - i);
    }
    assert!(cdxml::read_marks(&text, &prepared, 1.)?.atoms.is_empty());
    Ok(())
}
#[test]
fn callback_failures_remain_typed_and_hidden_text_is_checked() -> anyhow::Result<()> {
    #[derive(Debug, thiserror::Error)]
    #[error("sentinel")]
    struct CallbackError;
    let atoms = [Atom {
        atomic_number: 6,
        ..Atom::default()
    }];
    let prepared = cdxml::PreparedAtoms::new(&atoms, &[Point3::default()])?;
    let xml = "<CDXML><n p='0 0'><objecttag Name='stereo'><t><s>R</s></t></objecttag></n></CDXML>";
    let mut called = false;
    let error = cdxml::read_labels(xml, &prepared, &[], 1., |_, _| {
        called = true;
        Err(CallbackError)
    })
    .err()
    .context("Expected callback error")?;
    assert!(called);
    assert!(matches!(error, cdxml::LabelsError::Text(CallbackError)));
    assert!(
        cdxml::read_labels("<CDXML/>", &prepared, &vec![[1, 1]; 200_001], 1., |_, _| {
            Err(CallbackError)
        })
        .is_err()
    );
    assert!(
        cdxml::read_labels("<CDXML/>", &prepared, &[], f64::INFINITY, |_, _| Err(
            CallbackError
        ))
        .is_err()
    );
    let repeated = format!(
        "<CDXML unused='{}'>{}</CDXML>",
        "x".repeat(25_000),
        "<n p='0 0'/>".repeat(1000)
    );
    assert!(matches!(
        cdxml::read_labels(&repeated, &prepared, &[], 1., |_, _| Err(CallbackError)),
        Err(cdxml::LabelsError::Xml(cdxml::Error::Limit))
    ));
    Ok(())
}

fn convert_colors(value: &mut Value) -> anyhow::Result<()> {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                if key == "color" {
                    let channels = value.as_array_mut().context("Invalid color")?;
                    for channel in channels {
                        let n = channel.as_f64().context("Invalid channel")?;
                        anyhow::ensure!(
                            n.is_finite() && (0.0..=255.0).contains(&n) && n.fract() == 0.,
                            "Invalid document color"
                        );
                        *channel = Value::from(n as u8);
                    }
                } else {
                    convert_colors(value)?;
                }
            }
        }
        Value::Array(array) => {
            for value in array {
                convert_colors(value)?;
            }
        }
        _ => (),
    }
    Ok(())
}

#[test]
fn labels_follow_value_transport_at_f32_midpoints() -> anyhow::Result<()> {
    let wire = r#"{"version":15,"atom_labels":{"carbons":"skeletal","hydrogens":true,"stereo":false},"atoms":[{"id":1,"element":"C","position":{"x":0,"y":0},"display":{"number":{"text":"7","offset":{"x":249.96781158447266,"y":0}}}}],"bonds":[]}"#;
    let value: Value = serde_json::from_str(wire)?;
    let correct: Document = serde_json::from_value(value.clone())?;
    let direct: Document = serde_json::from_str(wire)?;
    let patches = serde_json::json!({"atoms":value.get("atoms").context("Missing atoms")?,"bonds":[],"atom_labels":value.get("atom_labels").context("Missing settings")?});
    assert_eq!(document(&patches, &[])?, correct);
    assert_ne!(
        correct, direct,
        "The fixture must expose direct-f32 JSON rounding"
    );
    Ok(())
}
