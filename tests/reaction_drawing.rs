use anyhow::Context;
use reshiki::{
    chemistry::{RDKIT_VERSION, document::Labels, reaction},
    document::Document,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

#[derive(Deserialize, Serialize)]
struct Expected {
    document: Value,
    participants: Value,
    labels: Vec<Labels>,
}
#[derive(Deserialize, Serialize)]
struct Case {
    name: String,
    text: String,
    expected: Option<Expected>,
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
fn reaction_drawings_match_native_scene_assembly() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/reaction_drawing_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(
        child
            .stdout
            .take()
            .context("Missing reaction drawing reference")?,
    )
    .lines();
    let version: Value =
        serde_json::from_str(&lines.next().context("Missing reference version")??)?;
    assert_eq!(version["rdkit_version"], RDKIT_VERSION);
    let (mut accepted, mut rejected, mut separators, mut agents) = (0, 0, 0, 0);
    let mut failures = Vec::new();
    for line in lines {
        let mut case: Case = serde_json::from_str(&line?)?;
        let result = reaction::read_rxn(&case.text)
            .map_err(|e| e.to_string())
            .and_then(|parsed| {
                let before = serde_json::to_value(&parsed).map_err(|e| e.to_string())?;
                let result = parsed.drawing().map_err(|e| e.to_string());
                assert_eq!(
                    serde_json::to_value(&parsed).map_err(|e| e.to_string())?,
                    before,
                    "Drawing preparation changed its input"
                );
                result
            });
        let expected = case.expected.as_mut().and_then(|e| {
            serde_json::from_value::<Document>(e.document.clone())
                .ok()
                .filter(|d| d.validate().is_ok())
                .map(|doc| (e, doc))
        });
        let failure = match (result, expected) {
            (Ok(draft), Some((expected, document))) => {
                accepted += 1;
                let participants: Vec<_> = draft
                    .participants()
                    .map(|(molecule, file)| serde_json::json!({"molecule":molecule,"file":file}))
                    .collect();
                let state = difference(
                    &serde_json::to_value(participants)?,
                    &expected.participants,
                    "participants",
                );
                let labels = draft.labels()?;
                let labeling = difference(
                    &serde_json::to_value(&labels)?,
                    &serde_json::to_value(&expected.labels)?,
                    "labels",
                );
                match draft.finish(labels) {
                    Ok(doc) => {
                        separators += doc.annotations.len();
                        agents += doc.reactions.iter().map(|r| r.agents.len()).sum::<usize>();
                        state.or(labeling).or(difference(
                            &serde_json::to_value(doc)?,
                            &serde_json::to_value(document)?,
                            "drawing",
                        ))
                    }
                    Err(error) => Some(format!("Could not finish drawing: {error}")),
                }
            }
            (Err(_), None) => {
                rejected += 1;
                None
            }
            (Err(e), Some(_)) => Some(format!("Rejected supported drawing: {e}")),
            (Ok(draft), None) => {
                // Drawing bounds are checked after placement and full CIP labels.
                if case.expected.is_some() {
                    match draft.labels().and_then(|labels| draft.finish(labels)) {
                        Err(_) => {
                            rejected += 1;
                            None
                        }
                        Ok(_) => Some("Accepted invalid drawing".into()),
                    }
                } else {
                    Some(format!("Accepted rejected drawing: {:?}", case.failure))
                }
            }
        };
        if let Some(error) = failure {
            if failures.is_empty() {
                std::fs::create_dir_all(root.join("artifacts"))?;
                std::fs::write(
                    root.join("artifacts/reaction-drawing-mismatch.json"),
                    serde_json::to_vec_pretty(&case)?,
                )?;
            }
            failures.push(format!("{}: {error}", case.name));
        }
    }
    assert!(child.wait()?.success(), "Reaction drawing reference failed");
    eprintln!(
        "RXN drawings: {accepted} accepted, {rejected} rejected, {separators} separators, {agents} agents, {} mismatches",
        failures.len()
    );
    if !failures.is_empty() {
        std::fs::write(
            root.join("artifacts/reaction-drawing-failures.txt"),
            failures.join("\n"),
        )?;
    }
    assert!(
        failures.is_empty(),
        "{}",
        failures
            .iter()
            .take(24)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(accepted > 1000 && rejected > 100 && separators > 100 && agents > 100);
    Ok(())
}

#[test]
fn reaction_completion_is_atomic_and_rejects_incomplete_labels() -> anyhow::Result<()> {
    use reshiki::{
        atom_labels::HydrogenPosition,
        chemistry::{document, molfile},
        document::{History, Point},
    };
    let mut water = Document::default();
    water.add_atom("O", Point::default());
    let part = molfile::Imported {
        molecule: document::prepare(&water)?,
        annotations: molfile::FileAnnotations {
            is_3d: false,
            attachment_points: vec![None],
            dummy_labels: vec![None],
        },
    };
    let source = reaction::Imported {
        reactants: vec![part.clone(), part.clone()],
        products: vec![part.clone()],
        agents: vec![part.clone(), part],
    };
    let before = serde_json::to_value(&source)?;
    let labels = || {
        (0..5)
            .map(|_| Labels {
                rdkit_version: RDKIT_VERSION.into(),
                atoms: vec![None],
                bonds: vec![],
            })
            .collect::<Vec<_>>()
    };
    for kind in 0..5 {
        let mut data = labels();
        match kind {
            0 => {
                data.pop();
            }
            1 => data.push(Labels {
                rdkit_version: RDKIT_VERSION.into(),
                atoms: vec![],
                bonds: vec![],
            }),
            2 => {
                data.last_mut()
                    .context("Missing participant")?
                    .rdkit_version = "wrong".into()
            }
            3 => data
                .last_mut()
                .context("Missing participant")?
                .atoms
                .clear(),
            _ => {
                *data
                    .last_mut()
                    .context("Missing participant")?
                    .atoms
                    .first_mut()
                    .context("Missing atom")? = Some("invalid".into())
            }
        }
        assert!(source.drawing()?.finish(data).is_err());
    }
    let doc = source.drawing()?.finish(labels())?;
    assert_eq!(serde_json::to_value(&source)?, before);
    assert!(
        doc.atoms
            .iter()
            .all(|a| a.display.hydrogen_position == HydrogenPosition::Left)
    );
    let mut history = History::default();
    assert!(history.commit(water.clone(), &doc));
    let mut canvas = doc.clone();
    assert!(history.undo(&mut canvas));
    assert_eq!(canvas, water);
    assert!(history.redo(&mut canvas));
    assert_eq!(canvas, doc);
    let mut invalid = source.clone();
    invalid.products.clear();
    assert!(invalid.drawing().is_err());
    let mut invalid = source;
    invalid
        .reactants
        .first_mut()
        .context("Missing reactant")?
        .molecule
        .positions
        .clear();
    assert!(invalid.drawing().is_err());
    Ok(())
}
