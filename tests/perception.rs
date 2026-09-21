use reshiki::chemistry::{
    RDKIT_VERSION,
    stereo::perception::{self, Options, State},
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    error::Error,
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

#[derive(Deserialize)]
struct Case {
    name: String,
    before: State,
    options: Options,
    expected: Option<State>,
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
fn legacy_stereo_perception_matches_independent_rdkit() -> Result<(), Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/perception_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().ok_or("Missing oracle output")?).lines();
    let version: Value = serde_json::from_str(&lines.next().ok_or("Missing oracle version")??)?;
    assert_eq!(version["rdkit_version"], RDKIT_VERSION);
    let (mut count, mut labelled, mut ring_stereo, mut bond_stereo, mut rejected) = (0, 0, 0, 0, 0);
    let mut failures = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        count += 1;
        let before = serde_json::to_value(&case.before)?;
        let result = perception::perceive(&case.before, case.options);
        let failure = match (&result, &case.expected) {
            (Ok(actual), Some(expected)) => {
                labelled += usize::from(
                    expected
                        .properties
                        .atoms
                        .iter()
                        .any(|a| a.cip_code.is_some()),
                );
                ring_stereo += usize::from(expected.metadata.atoms.iter().any(|a| a.ring_stereo));
                bond_stereo += usize::from(
                    expected
                        .metadata
                        .bonds
                        .iter()
                        .any(|b| matches!(b.stereo, 2..=5)),
                );
                difference(
                    &serde_json::to_value(actual)?,
                    &serde_json::to_value(expected)?,
                    "state",
                )
            }
            (Err(_), None) => {
                rejected += 1;
                None
            }
            (Err(error), Some(_)) => Some(format!("unexpected error: {error}")),
            (Ok(_), None) => Some(format!("accepted invalid reference: {:?}", case.failure)),
        };
        if let Some(error) = failure
            && failures.len() < 12
        {
            failures.push(format!("{}: {error}", case.name));
        }
        assert_eq!(serde_json::to_value(&case.before)?, before, "Input changed");
    }
    assert!(child.wait()?.success(), "Oracle failed");
    eprintln!(
        "Verified {count} stereo cases: {labelled} atom labels, {ring_stereo} ring relationships, {bond_stereo} bond stereo, {rejected} rejected"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(
        count > 30000 && labelled > 3000 && ring_stereo > 10 && bond_stereo > 100,
        "Insufficient coverage"
    );
    Ok(())
}
