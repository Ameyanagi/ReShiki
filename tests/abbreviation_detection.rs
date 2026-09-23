//! Exact original-worker detection, validation and native-query differential tests.
use anyhow::Context;
use reshiki::{
    chemistry::{
        RDKIT_VERSION, abbreviations,
        document::{self, Molecule},
        stereo::{Point3, perception::State},
    },
    document::Document,
};
use serde::Deserialize;
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

#[derive(Deserialize)]
struct Case {
    name: String,
    operation: String,
    document: Document,
    state: Option<State>,
    #[serde(default)]
    selection: Vec<u64>,
    label: Option<String>,
    expected: Option<Document>,
    error: Option<String>,
    #[serde(default)]
    prepare: bool,
}

fn check(case: &Case) -> anyhow::Result<()> {
    if case.operation == "validate" {
        let error = abbreviations::validate(&case.document)
            .err()
            .map(|e| e.to_string());
        anyhow::ensure!(
            error == case.error,
            "validation {error:?} != {:?}",
            case.error
        );
        return Ok(());
    }
    let molecule = Molecule {
        rdkit_version: RDKIT_VERSION,
        ids: case.document.atoms.iter().map(|a| a.id).collect(),
        positions: case
            .document
            .atoms
            .iter()
            .map(|a| Point3 {
                x: f64::from(a.position.x) / 28.,
                y: -f64::from(a.position.y) / 28.,
                z: 0.,
            })
            .collect(),
        state: case.state.clone().context("Missing native state")?,
    };
    let before = case.document.clone();
    let result = abbreviations::find(
        &case.document,
        &molecule,
        &case.selection,
        case.label.as_deref(),
    );
    anyhow::ensure!(before == case.document, "Input changed");
    match (result, &case.expected) {
        (Ok(actual), Some(expected)) => {
            anyhow::ensure!(
                &actual == expected,
                "groups {:?} != {:?}",
                actual.abbreviations,
                expected.abbreviations
            );
            if case.prepare {
                let prepared = document::prepare(&case.document)?;
                let actual = abbreviations::find(
                    &case.document,
                    &prepared,
                    &case.selection,
                    case.label.as_deref(),
                )?;
                anyhow::ensure!(&actual == expected, "Locally prepared detection differs");
            }
        }
        (Err(error), None) => anyhow::ensure!(
            Some(error.to_string()) == case.error,
            "{error} != {:?}",
            case.error
        ),
        (Ok(actual), None) => anyhow::bail!(
            "Expected {:?}, accepted {:?}",
            case.error,
            actual.abbreviations
        ),
        (Err(error), Some(_)) => anyhow::bail!("Unexpected error: {error}"),
    }
    Ok(())
}

#[test]
fn original_worker_detection_and_validation_match() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/abbreviation_detection_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().context("Missing oracle output")?).lines();
    let header: serde_json::Value =
        serde_json::from_str(&lines.next().context("Missing oracle header")??)?;
    assert_eq!(header["rdkit_version"], RDKIT_VERSION);
    assert_eq!(
        header["presets"],
        serde_json::to_value(abbreviations::presets()?)?
    );
    let mut count = 0usize;
    let mut errors = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        count += 1;
        if let Err(error) = check(&case)
            && errors.len() < 40
        {
            errors.push(format!("{}: {error}", case.name));
        }
    }
    assert!(
        child.wait()?.success(),
        "Oracle failed; differences: {}",
        errors.join("\n")
    );
    assert!(errors.is_empty(), "{}", errors.join("\n"));
    assert!(count > 5_000, "Incomplete oracle: {count}");
    eprintln!(
        "Verified {count} abbreviation detection/validation cases and all 29 native preset queries"
    );
    Ok(())
}

#[test]
fn invalid_prepared_states_fail_without_changing_the_drawing() -> anyhow::Result<()> {
    let doc: Document = serde_json::from_value(serde_json::json!({"version":15,"atoms":[
        {"id":1,"element":"N","position":{"x":0,"y":0}},
        {"id":2,"element":"C","position":{"x":42,"y":0}}
    ],"bonds":[{"a":1,"b":2,"order":1}],"annotations":[],"arrows":[]}))?;
    let molecule = document::prepare(&doc)?;
    let before = doc.clone();
    for mutation in 0..8 {
        let mut bad = molecule.clone();
        match mutation {
            0 => bad.ids.clear(),
            1 => bad.ids = vec![1, 1],
            2 => bad.ids = vec![1, 3],
            3 => bad.rdkit_version = "other",
            4 => bad.state.graph.bonds.first_mut().context("Missing bond")?.b = 99,
            5 => bad.state.rings.atoms = vec![vec![0, 1, 99]],
            6 => bad.state.rings.atoms = vec![vec![0, 1, 0]],
            _ => bad.state.graph.bonds.push(
                bad.state
                    .graph
                    .bonds
                    .first()
                    .context("Missing bond")?
                    .clone(),
            ),
        }
        assert!(
            abbreviations::find(&doc, &bad, &[], None).is_err(),
            "mutation {mutation}"
        );
        assert_eq!(doc, before);
    }
    assert!(abbreviations::find(&doc, &molecule, &vec![1; 100_001], None).is_err());
    Ok(())
}

#[test]
fn large_group_connectivity_is_iterative() -> anyhow::Result<()> {
    let mut doc = Document::default();
    let n = 20_000u64;
    for i in 1..=n {
        doc.atoms.push(serde_json::from_value(
            serde_json::json!({"id":i,"element":"C","position":{"x":0,"y":0}}),
        )?);
        if i > 1 {
            // Reverse edge order defeats repeated full scans in the old validator.
            doc.bonds.push(serde_json::from_value(
                serde_json::json!({"a":n-i+1,"b":n-i+2,"order":1}),
            )?);
        }
    }
    doc.abbreviations
        .push(reshiki::abbreviations::Abbreviation {
            alignment: Default::default(),
            label: "Polymer".into(),
            reverse_label: String::new(),
            anchor: 1,
            members: (1..=n).collect(),
        });
    abbreviations::validate(&doc)?;
    doc.bonds.pop();
    assert!(matches!(
        abbreviations::validate(&doc),
        Err(abbreviations::Error::Connected)
    ));
    Ok(())
}
