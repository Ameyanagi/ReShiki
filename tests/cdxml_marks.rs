use anyhow::Context;
use reshiki::chemistry::{
    RDKIT_VERSION,
    cdxml::{self, PreparedAtoms},
    graph::Atom,
    stereo::Point3,
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

#[derive(Deserialize)]
struct Fact {
    atomic_number: u8,
    charge: i8,
    radical_electrons: u8,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    text: String,
    atoms: Vec<Fact>,
    positions: Vec<Point3>,
    scale: f64,
    expected: Option<Value>,
    failure: Option<String>,
    failure_type: Option<String>,
    restriction: Option<String>,
}
fn compare(
    field: &str,
    actual: &Value,
    expected: &Value,
    differences: &mut usize,
    max_difference: &mut f64,
) -> bool {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => {
            let Some((a, b)) = a.as_f64().zip(b.as_f64()) else {
                return a == b;
            };
            if a != b {
                *differences += 1;
                eprintln!(
                    "Mark numeric difference {field}: Rust={a:?}, Python={b:?}, f32 Rust={:?}, Python={:?}",
                    a as f32, b as f32
                );
                assert_eq!(a as f32, b as f32, "Changed document geometry: {field}");
                *max_difference = max_difference.max((a - b).abs());
            }
            a == b
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len()
                && a.iter()
                    .zip(b)
                    .all(|(a, b)| compare(field, a, b, differences, max_difference))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter().all(|(key, a)| {
                    b.get(key)
                        .is_some_and(|b| compare(key, a, b, differences, max_difference))
                })
        }
        _ => actual == expected,
    }
}
#[test]
fn mark_patches_match_original_reader() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut child = Command::new(root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    }))
    .arg(root.join("tests/cdxml_marks_reference.py"))
    .stdout(Stdio::piped())
    .stderr(Stdio::inherit())
    .spawn()?;
    let mut lines =
        BufReader::new(child.stdout.take().context("Missing reference output")?).lines();
    let version: Value = serde_json::from_str(&lines.next().context("Missing version")??)?;
    assert_eq!(version["rdkit_version"], RDKIT_VERSION);
    let (mut accepted, mut rejected, mut restrictions, mut differences, mut max_difference) =
        (0, 0, 0, 0, 0.0f64);
    let mut failures = Vec::new();
    let (mut documents, mut document_failures) = (0, 0);
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        let atoms = case
            .atoms
            .iter()
            .map(|a| Atom {
                atomic_number: a.atomic_number,
                charge: a.charge,
                radical_electrons: a.radical_electrons,
                ..Atom::default()
            })
            .collect::<Vec<_>>();
        let before = serde_json::to_value((&atoms, &case.positions, &case.text))?;
        let prepared = PreparedAtoms::new(&atoms, &case.positions)?;
        let result = cdxml::read_marks(&case.text, &prepared, case.scale);
        assert_eq!(
            before,
            serde_json::to_value((&atoms, &case.positions, &case.text))?
        );
        let failure = if case.restriction.is_some() {
            restrictions += 1;
            (result.is_ok() || case.expected.is_none())
                .then(|| "Incorrect safety restriction".to_owned())
        } else {
            match (&result, &case.expected) {
                (Ok(actual), Some(expected)) => {
                    accepted += 1;
                    let actual = serde_json::to_value(actual)?;
                    match (document(&actual), document(expected)) {
                        (Ok(actual), Ok(expected)) => {
                            documents += 1;
                            assert_eq!(actual, expected, "Final Document mismatch: {}", case.name);
                        }
                        (Err(actual), Err(expected)) => {
                            document_failures += 1;
                            assert_eq!(
                                actual.to_string(),
                                expected.to_string(),
                                "Document conversion error: {}",
                                case.name
                            );
                        }
                        _ => anyhow::bail!("Final Document acceptance changed: {}", case.name),
                    }
                    (!compare("", &actual, expected, &mut differences, &mut max_difference))
                        .then(|| format!("{actual} != {expected}"))
                }
                (Err(error), None) => {
                    rejected += 1;
                    let source = case.failure.as_deref().unwrap_or("");
                    (case.failure_type.as_deref() == Some("ValueError")
                        && !source.starts_with("could not convert")
                        && !source.starts_with("invalid literal")
                        && error.to_string() != format!("Invalid molecular CDXML: {source}"))
                    .then(|| format!("Wrong error: {error}; expected {source}"))
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
        "CDXML marks: {accepted} accepted, {rejected} original failures, {restrictions} separate restrictions; {differences} unequal floats, max absolute delta {max_difference}; {documents} exact Documents, {document_failures} matched conversion errors"
    );
    anyhow::ensure!(
        failures.is_empty(),
        "{} mismatches: {}",
        failures.len(),
        failures
            .iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(accepted == 3669 && rejected == 494 && restrictions == 3);
    assert_eq!(differences, 0);
    assert_eq!((documents, document_failures), (3668, 1));
    Ok(())
}
#[test]
fn large_mark_reader_preserves_ids_and_order() -> anyhow::Result<()> {
    use std::fmt::Write;
    let mut xml = String::from("<CDXML>");
    let mut atoms = Vec::new();
    let mut positions = Vec::new();
    for i in 0..20_000u32 {
        let x = f64::from(i) * 2.;
        write!(
            xml,
            "<n id='{i}' p='{x} 0'/><graphic SymbolType='Plus' BoundingBox='4 3 0 0'><represent object='{i}' attribute='Charge'/></graphic>"
        )?;
        atoms.push(Atom {
            atomic_number: 6,
            charge: 1,
            ..Atom::default()
        });
        positions.push(Point3 {
            x: x / 28.,
            y: 0.,
            z: 0.,
        });
    }
    xml.push_str("</CDXML>");
    let prepared = PreparedAtoms::new(&atoms, &positions)?;
    let marks = cdxml::read_marks(&xml, &prepared, 1.)?;
    assert_eq!(marks.atoms.len(), 20_000);
    assert_eq!(marks.objects.len(), 20_000);
    for (i, (atom, object)) in marks.atoms.iter().zip(&marks.objects).enumerate() {
        assert_eq!(atom.id, u64::try_from(i)? + 1);
        assert_eq!(object.source, i * 3 + 2);
        assert_eq!(object.atoms, vec![atom.id]);
    }
    Ok(())
}
#[test]
fn association_checks_dimensions_and_limits() -> anyhow::Result<()> {
    assert!(PreparedAtoms::new(&[Atom::default()], &[]).is_err());
    assert!(PreparedAtoms::new(&[], &[Point3::default()]).is_err());
    assert!(
        PreparedAtoms::new(
            &[Atom::default()],
            &[Point3 {
                x: f64::INFINITY,
                y: 0.,
                z: 0.
            }]
        )
        .is_err()
    );
    assert!(
        PreparedAtoms::new(
            &vec![Atom::default(); 100_001],
            &vec![Point3::default(); 100_001]
        )
        .is_err()
    );
    let prepared = PreparedAtoms::new(&[], &[])?;
    assert!(cdxml::read_marks("<CDXML/>", &prepared, f64::NAN).is_err());
    assert!(cdxml::read_marks("<CDXML>", &prepared, 1.).is_err());
    assert!(
        cdxml::read_marks(
            &format!("<CDXML>{}</CDXML>", "<x/>".repeat(100_000)),
            &prepared,
            1.
        )
        .is_err()
    );
    Ok(())
}

fn document(patches: &Value) -> anyhow::Result<reshiki::document::Document> {
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
    // PythonEngine first parses the response as Value, then decodes Response.
    let document: reshiki::document::Document =
        serde_json::from_value(serde_json::json!({"version":15,"atoms":atoms,"bonds":[]}))?;
    document.validate().map_err(anyhow::Error::msg)?;
    Ok(document)
}

#[test]
fn marks_follow_value_transport_at_f32_midpoints() -> anyhow::Result<()> {
    let wire = r#"{"version":15,"atoms":[{"id":1,"element":"C","position":{"x":0,"y":0},"marks":[{"kind":"charge","offset":{"x":249.96781158447266,"y":0},"angle":0,"size_pt":3}]}],"bonds":[]}"#;
    let value: Value = serde_json::from_str(wire)?;
    let correct: reshiki::document::Document = serde_json::from_value(value.clone())?;
    let direct: reshiki::document::Document = serde_json::from_str(wire)?;
    let patches = serde_json::json!({"atoms":value.get("atoms").context("Missing atoms")?});
    assert_eq!(document(&patches)?, correct);
    assert_ne!(
        correct, direct,
        "The fixture must expose direct-f32 JSON rounding"
    );
    Ok(())
}
