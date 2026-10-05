use anyhow::Context;
use reshiki::chemistry::{
    RDKIT_VERSION,
    stereo::{
        cip::{
            Error,
            configuration::Target,
            digraph::Descriptor,
            label::{self, Options},
        },
        perception::State,
    },
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

#[derive(Deserialize)]
struct Case {
    name: String,
    before: State,
    options: Options,
    passes: usize,
    expected: Value,
}

fn code(descriptor: Descriptor) -> &'static str {
    match descriptor {
        Descriptor::R => "R",
        Descriptor::S => "S",
        Descriptor::PseudoR => "r",
        Descriptor::PseudoS => "s",
        Descriptor::E => "E",
        Descriptor::Z => "Z",
        Descriptor::SeqTrans => "e",
        Descriptor::SeqCis => "z",
        Descriptor::M => "M",
        Descriptor::P => "P",
        Descriptor::PseudoM => "m",
        Descriptor::PseudoP => "p",
        _ => "unsupported",
    }
}

fn evaluate(case: &Case) -> anyhow::Result<Value> {
    let mut state = case.before.clone();
    let mut passes = Vec::new();
    for _ in 0..case.passes {
        let before = serde_json::to_value(&state)?;
        let result = label::assign(&state, &case.options);
        assert_eq!(
            before,
            serde_json::to_value(&state)?,
            "Input changed: {}",
            case.name
        );
        let assignment = result?;
        let labels = assignment
            .labels
            .iter()
            .map(|label| {
                let target = match label.target {
                    Target::Atom(atom) => json!(["atom", atom]),
                    Target::Bond(bond) => json!(["bond", bond]),
                };
                json!({
                    "target":target,"code":code(label.descriptor),
                    "neighbor_order":label.neighbor_order,"bond_stereo":label.bond_stereo,
                })
            })
            .collect::<Vec<_>>();
        passes.push(json!({"state":assignment.state,"labels":labels}));
        state = assignment.state;
    }
    Ok(json!({"passes":passes}))
}

#[test]
fn complete_labeling_matches_public_native_api() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/cip_labeling_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().context("Missing native output")?).lines();
    let header: Value = serde_json::from_str(&lines.next().context("Missing version")??)?;
    assert_eq!(header["rdkit_version"], RDKIT_VERSION);
    let mut successes = 0;
    let mut failures = Vec::new();
    let mut mismatches = 0;
    let mut errors = BTreeMap::<String, usize>::new();
    let mut descriptors = BTreeSet::new();
    let mut corpus = BTreeSet::new();
    let (mut uninitialized, mut computed, mut repeats, mut implicit_h) = (0, 0, 0, 0);
    for (index, line) in lines.enumerate() {
        if index > 0 && index % 500 == 0 {
            eprintln!("CIP labeling: checked {index} inputs, {mismatches} mismatches");
        }
        let case: Case = serde_json::from_str(&line?)?;
        if case.name.starts_with("validation/") {
            corpus.insert(
                case.name
                    .split('/')
                    .nth(1)
                    .context("Missing corpus name")?
                    .to_owned(),
            );
        }
        let original = serde_json::to_value(&case.before)?;
        let result = evaluate(&case);
        assert_eq!(
            original,
            serde_json::to_value(&case.before)?,
            "Original changed: {}",
            case.name
        );
        let message = result.as_ref().err().map(ToString::to_string);
        let actual = match result {
            Ok(value) => value,
            Err(error) => {
                let error = error
                    .downcast_ref::<Error>()
                    .context("Unexpected test-harness error")?;
                let category = match error {
                    Error::Iterations => "iterations",
                    Error::Nodes => "nodes",
                    Error::Limit => "limit",
                    _ => "invalid",
                };
                // Undefined native behavior has no correct label to reproduce.
                // Require a typed, atomic rejection and keep it separately counted.
                let category = if case.expected["error"] == "native_crash"
                    && matches!(error, Error::Invalid(_))
                {
                    "native_crash"
                } else {
                    category
                };
                json!({"error":category})
            }
        };
        if actual != case.expected {
            mismatches += 1;
            if mismatches == 1 {
                std::fs::create_dir_all(root.join("artifacts"))?;
                std::fs::write(
                    root.join("artifacts/cip-labeling-first-mismatch.json"),
                    serde_json::to_vec_pretty(&json!({
                        "name":case.name,"options":case.options,"before":case.before,
                        "actual":actual,"expected":case.expected,"message":message,
                    }))?,
                )?;
            }
            if failures.len() < 20 {
                failures.push(format!("{}: {message:?}", case.name));
            }
        } else if let Some(error) = actual.get("error").and_then(Value::as_str) {
            *errors.entry(error.to_owned()).or_default() += 1;
        } else {
            successes += 1;
            repeats += usize::from(case.passes > 1);
            let passes = actual["passes"].as_array().context("Missing passes")?;
            for pass in passes {
                for label in pass["labels"].as_array().context("Missing labels")? {
                    descriptors.insert(label["code"].as_str().context("Missing code")?.to_owned());
                    implicit_h += usize::from(
                        label["neighbor_order"]
                            .as_array()
                            .context("Missing neighbors")?
                            .iter()
                            .any(Value::is_null),
                    );
                }
            }
            if case.before.rings.kind == reshiki::chemistry::stereo::perception::RingKind::None {
                let after =
                    &passes.first().context("Missing first pass")?["state"]["rings"]["kind"];
                uninitialized += usize::from(after == "none");
                computed += usize::from(after == "fast");
            }
        }
    }
    assert!(child.wait()?.success(), "Native oracle failed");
    eprintln!(
        "CIP labeling: {successes} successes, errors {errors:?}, {mismatches} mismatches; {} validation compounds; {repeats} repeated inputs, {uninitialized} retained empty ring caches, {computed} computed fast caches, {implicit_h} implicit-H orders; descriptors {descriptors:?}",
        corpus.len()
    );
    assert_eq!(mismatches, 0, "{}", failures.join("\n"));
    assert!(successes > 800 && repeats > 100 && computed > 50 && uninitialized > 30);
    assert!(errors.get("iterations").copied().unwrap_or_default() > 50);
    assert!(errors.get("invalid").copied().unwrap_or_default() > 100);
    assert!(errors.get("native_crash").copied().unwrap_or_default() > 0);
    assert!(implicit_h > 0);
    assert_eq!(
        descriptors,
        ["R", "S", "r", "s", "E", "Z", "e", "z", "M", "P", "m", "p"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    );
    assert_eq!(
        corpus.len() as u64,
        header["validation_compounds"]
            .as_u64()
            .context("Missing corpus count")?
    );
    Ok(())
}
