use anyhow::Context;
use reshiki::chemistry::{RDKIT_VERSION, hydrogens};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

#[derive(Deserialize, Serialize)]
struct Case {
    name: String,
    before: hydrogens::Input,
    expected: Option<Value>,
    failure: Option<String>,
}
fn difference(a: &Value, e: &Value, path: &str) -> Option<String> {
    if a == e {
        return None;
    }
    match (a, e) {
        (Value::Object(a), Value::Object(e)) if a.len() == e.len() => {
            for (key, value) in e {
                if let Some(d) = difference(
                    a.get(key).unwrap_or(&Value::Null),
                    value,
                    &format!("{path}.{key}"),
                ) {
                    return Some(d);
                }
            }
        }
        (Value::Array(a), Value::Array(e)) if a.len() == e.len() => {
            for (i, (a, e)) in a.iter().zip(e).enumerate() {
                if let Some(d) = difference(a, e, &format!("{path}[{i}]")) {
                    return Some(d);
                }
            }
        }
        _ => (),
    }
    Some(format!("{path}: {a} != {e}"))
}

#[test]
fn hydrogen_removal_matches_native_operation() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/hydrogen_removal_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(
        child
            .stdout
            .take()
            .context("Missing Hydrogen removal reader reference output")?,
    )
    .lines();
    let version: Value =
        serde_json::from_str(&lines.next().context("Missing reference version")??)?;
    assert_eq!(version["rdkit_version"], RDKIT_VERSION);
    let (mut accepted, mut rejected, mut mismatches) = (0, 0, 0);
    let mut failures = Vec::new();
    let mut all_failures = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        let before = serde_json::to_value(&case.before)?;
        let result = hydrogens::remove(&case.before);
        assert_eq!(serde_json::to_value(&case.before)?, before, "Input changed");
        let failure = match (&result, &case.expected) {
            (Ok(actual), Some(expected)) => {
                accepted += 1;
                difference(&serde_json::to_value(actual)?, expected, "molecule")
            }
            (Err(_), None) => {
                rejected += 1;
                None
            }
            (Err(error), Some(_)) => Some(format!("Rejected supported input: {error}")),
            (Ok(_), None) => Some(format!("Accepted invalid input: {:?}", case.failure)),
        };
        if let Some(error) = failure {
            all_failures.push(format!("{}: {error}", case.name));
            mismatches += 1;
            if failures.len() < 24 {
                failures.push(format!("{}: {error}", case.name));
            }
            if mismatches == 1 {
                std::fs::create_dir_all(root.join("artifacts"))?;
                std::fs::write(
                    root.join("artifacts/hydrogen-removal-mismatch.json"),
                    serde_json::to_vec_pretty(&case)?,
                )?;
                if let Ok(actual) = result {
                    std::fs::write(
                        root.join("artifacts/hydrogen-removal-actual.json"),
                        serde_json::to_vec_pretty(&actual)?,
                    )?;
                }
            }
        }
    }
    assert!(
        child.wait()?.success(),
        "Hydrogen removal reader reference failed"
    );
    if !all_failures.is_empty() {
        std::fs::write(
            root.join("artifacts/hydrogen-removal-failures.txt"),
            all_failures.join("\n"),
        )?;
    }
    eprintln!(
        "Hydrogen removal import: {accepted} accepted, {rejected} rejected, {mismatches} mismatches"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(
        accepted > 3000,
        "Insufficient Hydrogen removal import coverage"
    );
    Ok(())
}

fn input(text: &str) -> anyhow::Result<hydrogens::Input> {
    let parsed = reshiki::chemistry::smiles::parse(text)?;
    let count = parsed.graph.atoms.len();
    Ok(hydrogens::Input {
        graph: parsed.graph,
        metadata: parsed.metadata,
        directions: parsed.directions,
        unknown_atoms: vec![false; count],
        annotations: hydrogens::Annotations::default(),
    })
}

#[test]
fn malformed_annotations_fail_without_partial_changes() -> anyhow::Result<()> {
    let base = input("[H][C@](F)(Cl)Br")?;
    let mut inputs = Vec::new();
    let mut bad = base.clone();
    bad.directions.clear();
    inputs.push(bad);
    let mut bad = base.clone();
    bad.unknown_atoms.clear();
    inputs.push(bad);
    let mut bad = base.clone();
    bad.metadata.atoms.clear();
    inputs.push(bad);
    let mut bad = base.clone();
    bad.annotations.protected_atoms.push(usize::MAX);
    inputs.push(bad);
    let mut bad = base.clone();
    bad.annotations.substance_groups.push(vec![usize::MAX]);
    inputs.push(bad);
    let mut bad = base;
    bad.graph.bonds.get_mut(0).context("Missing test bond")?.a = usize::MAX;
    inputs.push(bad);
    for bad in inputs {
        let before = serde_json::to_value(&bad)?;
        assert!(hydrogens::remove(&bad).is_err());
        assert_eq!(serde_json::to_value(&bad)?, before);
    }
    Ok(())
}

fn hub(count: usize, chiral: bool) -> anyhow::Result<hydrogens::Input> {
    use reshiki::chemistry::{
        graph::{Atom, Bond, Graph},
        kekulize::Direction,
        ranking::Metadata,
    };
    let graph = Graph {
        atoms: std::iter::once(Atom {
            atomic_number: 6,
            ..Atom::default()
        })
        .chain(std::iter::repeat_n(
            Atom {
                atomic_number: 1,
                no_implicit: true,
                ..Atom::default()
            },
            count,
        ))
        .collect(),
        bonds: (1..=count)
            .map(|b| Bond {
                a: 0,
                b,
                order: 1,
                aromatic: false,
            })
            .collect(),
    };
    let mut metadata = Metadata::unspecified(&graph);
    metadata
        .atoms
        .get_mut(0)
        .context("Missing hub atom")?
        .chiral_tag = u8::from(chiral);
    Ok(hydrogens::Input {
        graph,
        metadata,
        directions: vec![Direction::None; count],
        unknown_atoms: vec![false; count + 1],
        annotations: hydrogens::Annotations::default(),
    })
}

#[test]
fn high_degree_hydrogen_removal_is_bounded() -> anyhow::Result<()> {
    let plain = hub(30_000, false)?;
    let result = hydrogens::remove(&plain)?;
    assert_eq!(result.kept_atoms, [0]);
    assert!(result.graph.bonds.is_empty());
    assert_eq!(plain.graph.atoms.len(), 30_001);
    // Deliberately impossible stereo still cannot cause unlimited scans.
    assert!(matches!(
        hydrogens::remove(&hub(10_000, true)?),
        Err(hydrogens::Error::Limit)
    ));
    Ok(())
}
