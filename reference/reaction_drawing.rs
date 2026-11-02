use anyhow::Context;
use reshiki::{
    chemistry::{RDKIT_VERSION, document::Labels, reaction},
    document::Document,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

#[path = "support/reaction_presentation.rs"]
mod reaction_presentation;
#[path = "support/molfile_restrictions.rs"]
mod restrictions;

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
    generic_restriction: Option<restrictions::GenericRestriction>,
}

fn differences(a: &Value, e: &Value, path: &str) -> Vec<String> {
    if a == e {
        return Vec::new();
    }
    let mut failures = Vec::new();
    match (a, e) {
        (Value::Object(a), Value::Object(e)) => {
            for key in a.keys().chain(e.keys()).collect::<BTreeSet<_>>() {
                let path = format!("{path}.{key}");
                match (a.get(key), e.get(key)) {
                    (Some(a), Some(e)) => failures.extend(differences(a, e, &path)),
                    (Some(a), None) => failures.push(format!("{path}: unexpected field {a}")),
                    (None, Some(e)) => {
                        failures.push(format!("{path}: missing field, expected {e}"))
                    }
                    (None, None) => unreachable!(),
                }
            }
        }
        (Value::Array(a), Value::Array(e)) => {
            for i in 0..a.len().max(e.len()) {
                let path = format!("{path}[{i}]");
                match (a.get(i), e.get(i)) {
                    (Some(a), Some(e)) => failures.extend(differences(a, e, &path)),
                    (Some(a), None) => failures.push(format!("{path}: unexpected entry {a}")),
                    (None, Some(e)) => {
                        failures.push(format!("{path}: missing entry, expected {e}"))
                    }
                    (None, None) => unreachable!(),
                }
            }
        }
        _ => failures.push(format!("{path}: {a} != {e}")),
    }
    failures
}

#[test]
fn drawing_diagnostics_report_every_field_difference() {
    let actual = serde_json::json!({
        "a": null,
        "b": [1, 2, 3],
        "c": [],
        "nested": {"one": 1, "two": 2},
        "unexpected": null,
    });
    let expected = serde_json::json!({
        "b": [4, 5],
        "c": [null],
        "missing": null,
        "nested": {"one": 3, "two": 4},
    });
    assert_eq!(
        differences(&actual, &expected, "drawing"),
        [
            "drawing.a: unexpected field null",
            "drawing.b[0]: 1 != 4",
            "drawing.b[1]: 2 != 5",
            "drawing.b[2]: unexpected entry 3",
            "drawing.c[0]: missing entry, expected null",
            "drawing.missing: missing field, expected null",
            "drawing.nested.one: 1 != 3",
            "drawing.nested.two: 2 != 4",
            "drawing.unexpected: unexpected field null",
        ]
    );
    assert!(differences(&actual, &actual, "drawing").is_empty());
}

#[tokio::test]
async fn mapped_reaction_imports_keep_visible_ink_spacing() -> anyhow::Result<()> {
    use reshiki::engine::{LocalEngine, Request};

    for input in [
        "[CH3:123456][OH:2].[OH2:3]>>[CH3:123456][OH:2].[OH2:3]",
        "[CH3:123456][C@@H:2]([OH:3])[Cl:4].[OH2:5]>[NH3:6]>[CH3:123456][C@H:2]([OH:3])[Cl:4].[OH2:5]",
    ] {
        // Use the application's real import path, without the reference
        // projection or injected indicator offsets. These are import defaults.
        let doc = LocalEngine::default()
            .request(Request::import("rsmi", input))
            .await
            .map_err(anyhow::Error::msg)
            .with_context(|| format!("RSMI production import: {input}"))?
            .document
            .context("Missing imported reaction document")?;
        assert!(doc.atom_labels.maps);
        assert!(!doc.atom_labels.stereo);
        assert!(
            reshiki::atom_labels::indicators(&doc)
                .iter()
                .any(|indicator| indicator.text == "123456")
        );
        reaction_presentation::check_spacing(&doc)
            .with_context(|| format!("RSMI visible map spacing: {input}"))?;
        if input.contains('@') {
            assert!(doc.atoms.iter().any(|atom| atom.cip_label.is_some()));
        }

        // The production writer emits a real V3000 RXN, whose imported drawing
        // is measured independently of the RSMI drawing's original positions.
        let rxn = reaction::write_rxn(&doc, None)
            .with_context(|| format!("RXN production export: {input}"))?;
        let imported =
            reaction::read_rxn(&rxn).with_context(|| format!("RXN production read: {input}"))?;
        let drawing = imported
            .drawing()
            .with_context(|| format!("RXN production drawing: {input}"))?;
        let labels = drawing
            .labels()
            .with_context(|| format!("RXN production labels: {input}"))?;
        let round_trip = drawing
            .finish(labels)
            .with_context(|| format!("RXN production finish: {input}"))?;
        assert!(round_trip.atom_labels.maps);
        assert!(!round_trip.atom_labels.stereo);
        reaction_presentation::check_spacing(&round_trip)
            .with_context(|| format!("RXN visible map spacing: {input}"))?;
        let maps = |doc: &Document| {
            let mut maps: Vec<_> = doc.atoms.iter().map(|atom| atom.map_num).collect();
            maps.sort_unstable();
            maps
        };
        assert_eq!(maps(&round_trip), maps(&doc));
    }
    Ok(())
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
        .arg(root.join("reference/reaction_drawing_reference.py"))
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
    let mut failed_cases = 0;
    let mut failures = Vec::new();
    let mut categories = BTreeMap::<String, usize>::new();
    let mut restriction_categories = BTreeMap::<String, usize>::new();
    for line in lines {
        let mut case: Case = serde_json::from_str(&line?)?;
        let parsed = reaction::read_rxn(&case.text).map_err(|e| e.to_string());
        let expected = case.expected.as_mut().and_then(|e| {
            serde_json::from_value::<Document>(e.document.clone())
                .ok()
                .filter(|d| d.validate().is_ok())
                .map(|doc| (e, doc))
        });
        let case_failures = if let Some(restriction) = &case.generic_restriction {
            // #300 restricts these explicit generic RXN records. Keep the
            // original drawing capture and require a valid legacy Document
            // plus the exact independently predicted read-stage rejection.
            let failure = if matches!(
                restriction.category.as_str(),
                "polymer_crossing_membership"
                    | "empty_polymer_membership"
                    | "generic_rxn_substance_groups"
                    | "v2000_link_nodes"
                    | "polymer_parent_atoms"
                    | "polymer_crossing_correspondence"
            ) {
                restrictions::rejection_difference(
                    restriction,
                    expected.is_some() && case.failure.is_none(),
                    &parsed,
                )
            } else {
                Some(format!(
                    "Unclassified drawing restriction: {} ({})",
                    restriction.category, restriction.error
                ))
            };
            if failure.is_none() {
                *restriction_categories
                    .entry(restriction.category.clone())
                    .or_default() += 1;
            }
            failure.into_iter().collect::<Vec<_>>()
        } else {
            let result = parsed.and_then(|parsed| {
                let before = serde_json::to_value(&parsed).map_err(|e| e.to_string())?;
                let result = parsed.drawing().map_err(|e| e.to_string());
                assert_eq!(
                    serde_json::to_value(&parsed).map_err(|e| e.to_string())?,
                    before,
                    "Drawing preparation changed its input"
                );
                result
            });
            match (result, expected) {
                (Ok(draft), Some((expected, document))) => {
                    accepted += 1;
                    let participants: Vec<_> = draft
                        .participants()
                        .map(
                            |(molecule, file)| serde_json::json!({"molecule":molecule,"file":file}),
                        )
                        .collect();
                    let mut failures = differences(
                        &serde_json::to_value(participants)?,
                        &expected.participants,
                        "participants",
                    );
                    match draft.labels() {
                        Ok(labels) => {
                            failures.extend(differences(
                                &serde_json::to_value(&labels)?,
                                &serde_json::to_value(&expected.labels)?,
                                "labels",
                            ));
                            match draft.finish(labels) {
                                Ok(doc) => {
                                    separators += doc.annotations.len();
                                    agents +=
                                        doc.reactions.iter().map(|r| r.agents.len()).sum::<usize>();
                                    let mut actual = serde_json::to_value(doc)?;
                                    // RDKit keeps only the radical count; the MOL RAD 1
                                    // singlet spin is native drawing metadata.
                                    for atom in actual["atoms"].as_array_mut().into_iter().flatten()
                                    {
                                        atom.as_object_mut().map(|a| a.remove("singlet"));
                                    }
                                    let mut expected = serde_json::to_value(document)?;
                                    if let Err(error) = reaction_presentation::adapt_document(
                                        &actual,
                                        &mut expected,
                                    ) {
                                        failures.push(format!("[reaction-ink] {error:#}"));
                                    }
                                    failures.extend(differences(&actual, &expected, "drawing"));
                                }
                                Err(error) => {
                                    failures.push(format!("Could not finish drawing: {error}"));
                                }
                            }
                        }
                        Err(error) => {
                            failures.push(format!("Could not label drawing: {error}"));
                        }
                    }
                    failures
                }
                (Err(_), None) => {
                    rejected += 1;
                    Vec::new()
                }
                (Err(e), Some(_)) => vec![format!("Rejected supported drawing: {e}")],
                (Ok(draft), None) => {
                    // Drawing bounds are checked after placement and full CIP labels.
                    if case.expected.is_some() {
                        match draft.labels().and_then(|labels| draft.finish(labels)) {
                            Err(_) => {
                                rejected += 1;
                                Vec::new()
                            }
                            Ok(_) => vec!["Accepted invalid drawing".into()],
                        }
                    } else {
                        vec![format!("Accepted rejected drawing: {:?}", case.failure)]
                    }
                }
            }
        };
        if !case_failures.is_empty() {
            failed_cases += 1;
            if failures.is_empty() {
                std::fs::create_dir_all(root.join("artifacts"))?;
                std::fs::write(
                    root.join("artifacts/reaction-drawing-mismatch.json"),
                    serde_json::to_vec_pretty(&case)?,
                )?;
            }
            for error in case_failures {
                let category = if error.starts_with("Rejected supported drawing:") {
                    error.clone()
                } else if error.starts_with("Accepted rejected drawing:") {
                    "Accepted rejected drawing".to_owned()
                } else if error.starts_with("participants") {
                    "Participant difference".to_owned()
                } else if error.starts_with("labels") {
                    "Label difference".to_owned()
                } else if error.starts_with("drawing") {
                    "Drawing difference".to_owned()
                } else {
                    error.clone()
                };
                *categories.entry(category).or_default() += 1;
                failures.push(format!("{}: {error}", case.name));
            }
        }
    }
    assert!(child.wait()?.success(), "Reaction drawing reference failed");
    eprintln!(
        "RXN drawings: {accepted} accepted, {rejected} rejected, {} intended restrictions, {separators} separators, {agents} agents, {failed_cases} failed cases, {} mismatches",
        restriction_categories.values().sum::<usize>(),
        failures.len()
    );
    for (category, count) in categories {
        eprintln!("RXN drawing mismatch category: {count} {category}");
    }
    for (category, count) in restriction_categories {
        eprintln!("RXN drawing restriction category: {count} {category}");
    }
    if !failures.is_empty() {
        std::fs::write(
            root.join("artifacts/reaction-drawing-failures.txt"),
            failures.join("\n"),
        )?;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
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
            singlets: vec![false],
            chemdraw_directions: Vec::new(),
            attachments: Vec::new(),
            is_3d: false,
            attachment_points: vec![None],
            dummy_labels: vec![None],
            generic: Default::default(),
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
