use reshiki::chemistry::{
    RDKIT_VERSION,
    stereo::wedging::{self, Conformer, WedgeProperties, WedgeState},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    error::Error,
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};
#[derive(Deserialize, Serialize)]
struct Case {
    name: String,
    before: WedgeState,
    properties: WedgeProperties,
    conformer: Option<Conformer>,
    two: bool,
    single: Option<(usize, usize)>,
    expected: Option<WedgeState>,
    alternatives: Vec<WedgeState>,
    failure: Option<String>,
}
fn difference(actual: &Value, expected: &Value, location: &str) -> Option<String> {
    if actual == expected {
        return None;
    }
    match (actual, expected) {
        (Value::Object(a), Value::Object(e)) => {
            for (key, value) in e {
                if let Some(message) = difference(&a[key], value, &format!("{location}.{key}")) {
                    return Some(message);
                }
            }
        }
        (Value::Array(a), Value::Array(e)) if a.len() == e.len() => {
            for (index, (left, right)) in a.iter().zip(e).enumerate() {
                if let Some(message) = difference(left, right, &format!("{location}[{index}]")) {
                    return Some(message);
                }
            }
        }
        _ => {}
    }
    Some(format!("{location}: {actual} != {expected}"))
}

#[test]
fn wedge_assignment_matches_independent_rdkit() -> Result<(), Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/wedging_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().ok_or("Missing oracle output")?).lines();
    let version: Value = serde_json::from_str(&lines.next().ok_or("Missing oracle version")??)?;
    assert_eq!(version["rdkit_version"], RDKIT_VERSION);
    let (mut count, mut changed, mut reversed, mut rejected, mut mismatches) = (0, 0, 0, 0, 0);
    let mut failures = Vec::new();
    let mut tied_choices = 0;
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        count += 1;
        let before = serde_json::to_value((&case.before, &case.properties, &case.conformer))?;
        let result = if let Some((bond, atom)) = case.single {
            wedging::wedge_bond(
                &case.before,
                &case.properties,
                case.conformer
                    .as_ref()
                    .ok_or("Missing single bond conformer")?,
                bond,
                atom,
            )
        } else {
            wedging::wedge_molecule(
                &case.before,
                &case.properties,
                case.conformer.as_ref(),
                case.two,
            )
        };
        let failure = match (&result, &case.expected) {
            (Ok(actual), Some(expected)) => {
                changed += usize::from(expected.directions != case.before.directions);
                reversed += usize::from(
                    expected
                        .graph
                        .bonds
                        .iter()
                        .zip(&case.before.graph.bonds)
                        .any(|(a, b)| a.a != b.a),
                );
                let mismatch = difference(
                    &serde_json::to_value(actual)?,
                    &serde_json::to_value(expected)?,
                    "state",
                );
                if mismatch.is_some() {
                    let actual = serde_json::to_value(actual)?;
                    let mut matched = false;
                    for alternative in &case.alternatives {
                        if actual == serde_json::to_value(alternative)? {
                            matched = true;
                            break;
                        }
                    }
                    if matched {
                        tied_choices += 1;
                        None
                    } else {
                        mismatch
                    }
                } else {
                    None
                }
            }
            (Err(_), None) => {
                rejected += 1;
                None
            }
            (Err(error), Some(_)) => Some(format!("unexpected error: {error}")),
            (Ok(_), None) => Some(format!("accepted invalid reference: {:?}", case.failure)),
        };
        if let Some(error) = failure {
            mismatches += 1;
            if failures.len() < 12 {
                failures.push(format!("{}: {error}", case.name));
            }
            if mismatches == 1 {
                std::fs::create_dir_all(root.join("artifacts"))?;
                std::fs::write(
                    root.join("artifacts/wedging-first-mismatch.json"),
                    serde_json::to_vec_pretty(&case)?,
                )?;
            }
        }
        assert_eq!(
            serde_json::to_value((&case.before, &case.properties, &case.conformer))?,
            before,
            "Input changed"
        );
    }
    assert!(
        child.wait()?.success(),
        "Oracle failed: {}",
        failures.join("\n")
    );
    eprintln!(
        "Verified {count} wedging cases: {changed} direction changes, {reversed} endpoint reversals, {rejected} rejected; {tied_choices} native tie variants, {mismatches} mismatches"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(
        count > 20000 && changed > 1000 && reversed > 500 && rejected > 1,
        "Insufficient coverage"
    );
    Ok(())
}
