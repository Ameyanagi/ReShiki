//! Independent captures from the exact pinned native import adapter.
use anyhow::Context;
use reshiki::chemistry::{RDKIT_VERSION, inchi::output};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

#[derive(Deserialize)]
struct Case {
    name: String,
    operation: String,
    raw: Option<output::Output>,
    stages: BTreeMap<String, Value>,
    error: Option<String>,
    expected: Option<Value>,
    #[serde(default)]
    sanitize: bool,
    #[serde(default)]
    remove: bool,
    cleanup_rules: Vec<String>,
}

#[test]
fn native_import_assembly() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    };
    let mut child = Command::new(root.join(python))
        .arg(root.join("tests/inchi_output_reference.py"))
        .stdout(Stdio::piped())
        .spawn()?;
    let stdout = child.stdout.take().context("Missing fixture output")?;
    let mut lines = BufReader::new(stdout).lines();
    let header: Value = serde_json::from_str(&lines.next().context("Missing fixture version")??)?;
    assert_eq!(header["rdkit_version"], RDKIT_VERSION);
    assert_eq!(
        header["adapter_sha256"],
        "68c9b20d1d5920ed602ea931c1429395280c3d040971593073917618015183d1"
    );
    assert_eq!(
        header["capture_sha256"],
        format!(
            "{:x}",
            Sha256::digest(include_bytes!("inchi_output_reference.cpp"))
        )
    );
    let mut rules = BTreeSet::new();
    let (
        mut topology,
        mut assembly,
        mut native_failures,
        mut cleanup,
        mut complete,
        mut duplicate_boundaries,
    ) = (0, 0, 0, 0, 0, 0);
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        rules.extend(case.cleanup_rules.iter().cloned());
        if case.operation == "cleanup" {
            let state: reshiki::chemistry::stereo::perception::State = serde_json::from_value(
                case.stages
                    .get("assembled")
                    .context("Missing cleanup input")?
                    .clone(),
            )?;
            let input = output::Assembly {
                unspecified_bonds: vec![false; state.graph.bonds.len()],
                state: Some(state),
                warnings: Vec::new(),
            };
            let before = serde_json::to_value(&input)?;
            let result = output::clean_up(&input).with_context(|| case.name.clone())?;
            anyhow::ensure!(
                serde_json::to_value(&input)? == before,
                "{} cleanup mutated input",
                case.name
            );
            let value = serde_json::to_value(result.state)?;
            let expected = case.expected.context("Missing cleanup expectation")?;
            anyhow::ensure!(
                value == expected,
                "{} cleanup\nactual: {value}\nexpected: {expected}",
                case.name
            );
            cleanup += 1;
            continue;
        }
        let raw = case.raw.context("Missing native records")?;
        let before = raw.clone();
        for (name, result) in [
            ("topology", output::topology(&raw)),
            ("assembled", output::assemble(&raw)),
        ] {
            match (case.stages.get(name), result) {
                (Some(expected), Err(output::Error::NativeCacheBoundary))
                    if expected["native_cache_boundary"] == true =>
                {
                    native_failures += 1;
                }
                (Some(expected), Ok(actual)) => {
                    let state = actual.state.context("Unexpected null assembly")?;
                    let value = serde_json::to_value(&state)?;
                    anyhow::ensure!(
                        value == *expected,
                        "{} {name}\nactual: {value}\nexpected: {expected}",
                        case.name
                    );
                    anyhow::ensure!(
                        actual.unspecified_bonds.len() == state.graph.bonds.len(),
                        "{} lost bond identities",
                        case.name
                    );
                    if name == "topology" {
                        topology += 1;
                    } else {
                        assembly += 1;
                    }
                }
                (None, Ok(actual)) if actual.state.is_none() && case.error.is_none() => {
                    native_failures += 1;
                }
                (None, Err(_)) if case.error.is_some() => {
                    native_failures += 1;
                }
                (None, Err(error)) => {
                    anyhow::bail!("{} {name} error instead of native null: {error}", case.name)
                }
                (Some(_), Err(error)) => anyhow::bail!("{} {name}: {error}", case.name),
                (None, Ok(_)) => anyhow::bail!("{} unexpected {name} success", case.name),
            }
        }
        assert_eq!(raw, before, "{} mutated native records", case.name);
        if case.operation == "duplicate_boundary" {
            // Native raw injection accepts these records. Preserve their exact
            // assembly, while making the later shared-metadata limitation
            // observable rather than certifying native rejection or truncation.
            anyhow::ensure!(case.expected.is_some(), "Native duplicate fixture failed");
            let assembled = output::assemble(&raw)?;
            assert!(matches!(
                output::clean_up(&assembled),
                Err(output::Error::RepeatedStereoRecords { indices: 4, .. })
            ));
            assert!(matches!(
                output::reconstruct(
                    &raw,
                    output::Options {
                        sanitize: case.sanitize,
                        remove_hydrogens: case.remove
                    }
                ),
                Err(output::Error::RepeatedStereoRecords { indices: 4, .. })
            ));
            duplicate_boundaries += 1;
            continue;
        }
        if let Some(expected) = case.stages.get("cleaned")
            && expected["native_cache_boundary"] != true
        {
            let assembled = output::assemble(&raw)?;
            let result = output::clean_up(&assembled).with_context(|| case.name.clone())?;
            let value = serde_json::to_value(result.state)?;
            anyhow::ensure!(
                value == *expected,
                "{} cleanup\nactual: {value}\nexpected: {expected}",
                case.name
            );
            cleanup += 1;
        }
        let options = output::Options {
            sanitize: case.sanitize,
            remove_hydrogens: case.remove,
        };
        if let Some(expected) = case.stages.get("prepared")
            && expected["native_cache_boundary"] != true
        {
            let cleaned = output::clean_up(&output::assemble(&raw)?)?;
            let result = output::prepare(&cleaned, options).with_context(|| case.name.clone())?;
            let value = serde_json::to_value(result.state)?;
            anyhow::ensure!(
                value == *expected,
                "{} preparation\nactual: {value}\nexpected: {expected}",
                case.name
            );
        }
        match (case.expected.as_ref(), output::reconstruct(&raw, options)) {
            (Some(expected), Err(output::Error::NativeCacheBoundary))
                if expected["native_cache_boundary"] == true => {}
            (Some(expected), Ok(actual)) => {
                let value = serde_json::to_value(actual.state)?;
                anyhow::ensure!(
                    value == *expected,
                    "{} final\nactual: {value}\nexpected: {expected}",
                    case.name
                );
                complete += 1;
            }
            (None, Err(_)) if case.error.is_some() => {}
            (None, Ok(actual)) if actual.state.is_none() && case.error.is_none() => {}
            (Some(_), Err(error)) => anyhow::bail!("{} final: {error}", case.name),
            (None, Err(error)) => {
                anyhow::bail!("{} final error instead of native null: {error}", case.name)
            }
            (None, Ok(_)) => anyhow::bail!("{} unexpected final success", case.name),
        }
        assert_eq!(
            raw, before,
            "{} reconstruction mutated native records",
            case.name
        );
    }
    anyhow::ensure!(child.wait()?.success(), "Fixture reader failed");
    anyhow::ensure!(topology > 5000 && assembly > 5000 && native_failures > 10);
    let expected_rules = [
        "_Valence3ClCleanUp1",
        "_Valence4NCleanUp1",
        "_Valence4NCleanUp2",
        "_Valence5NCleanUp1",
        "_Valence5NCleanUp2",
        "_Valence5NCleanUp3",
        "_Valence5NCleanUp4",
        "_Valence5NCleanUp5",
        "_Valence5NCleanUp6",
        "_Valence5NCleanUp7",
        "_Valence5NCleanUp8",
        "_Valence5NCleanUp9",
        "_Valence5NCleanUpA",
        "_Valence5NCleanUpB",
        "_Valence7SCleanUp1",
        "_Valence7SCleanUp2",
        "_Valence7SCleanUp3",
        "_Valence8ClCleanUp1",
        "_Valence8SCleanUp1",
    ];
    assert_eq!(
        rules,
        expected_rules.into_iter().map(str::to_owned).collect()
    );
    eprintln!(
        "InChI captures: {topology} topologies, {assembly} stereo assemblies, {cleanup} cleanups, {complete} final states, {native_failures} rejected stages; {duplicate_boundaries} explicit duplicate-record boundaries"
    );
    Ok(())
}

fn carbon() -> output::Atom {
    output::Atom {
        position: [0.; 3],
        element: "C".into(),
        isotopic_mass: 0,
        charge: 0,
        hydrogens: [4, 0, 0, 0],
        radical: 0,
        bonds: Vec::new(),
    }
}
fn one_atom() -> output::Output {
    output::Output {
        status: 0,
        message: String::new(),
        log: String::new(),
        warning_flags: [[0; 2]; 2],
        atoms: vec![carbon()],
        stereo: Vec::new(),
    }
}

#[test]
fn native_record_bounds_and_ignored_fields() -> anyhow::Result<()> {
    let baseline = one_atom();
    let mut input = baseline.clone();
    input.atoms = vec![carbon(); i16::MAX as usize + 1];
    assert!(matches!(
        output::assemble(&input),
        Err(output::Error::Limit(_))
    ));
    input = baseline.clone();
    input.atoms[0].radical = 1;
    assert_eq!(
        output::assemble(&input)?.warnings,
        [output::Warning::IgnoredRadical { atom: 0, value: 1 }]
    );
    input = baseline.clone();
    input.atoms[0].hydrogens = [0, -1, 0, 0];
    assert!(matches!(
        output::assemble(&input),
        Err(output::Error::Limit(_))
    ));
    input = baseline.clone();
    input.atoms[0].bonds.push(output::Bond {
        neighbor: -1,
        kind: 1,
        stereo: 0,
    });
    assert!(matches!(
        output::assemble(&input),
        Err(output::Error::Invalid(_))
    ));
    input = baseline.clone();
    input.atoms[0].bonds = vec![
        output::Bond {
            neighbor: 0,
            kind: 1,
            stereo: 0
        };
        21
    ];
    assert!(matches!(
        output::assemble(&input),
        Err(output::Error::Invalid(_))
    ));
    input = baseline.clone();
    input.atoms[0].bonds.push(output::Bond {
        neighbor: 0,
        kind: 1,
        stereo: 0,
    });
    assert!(matches!(
        output::assemble(&input),
        Err(output::Error::Chemistry(_))
    ));
    input = baseline.clone();
    input.atoms[0].position = [f64::MAX, -f64::MAX, f64::MIN_POSITIVE];
    input.warning_flags = [[u64::MAX; 2]; 2];
    assert_eq!(
        serde_json::to_value(output::assemble(&baseline)?)?,
        serde_json::to_value(output::assemble(&input)?)?
    );
    for status in [-100, -2, -1, 2, 3, 4, 5] {
        input.status = status;
        assert!(output::assemble(&input)?.state.is_none());
    }
    let mut malformed = output::assemble(&baseline)?;
    malformed.unspecified_bonds.push(false);
    assert!(matches!(
        output::clean_up(&malformed),
        Err(output::Error::Invalid(_))
    ));
    assert!(matches!(
        output::prepare(
            &malformed,
            output::Options {
                sanitize: false,
                remove_hydrogens: false
            }
        ),
        Err(output::Error::Invalid(_))
    ));
    let mut nitrogen = carbon();
    nitrogen.element = "N".into();
    nitrogen.hydrogens = [5, 0, 0, 0];
    let mut expensive = baseline;
    expensive.atoms = vec![nitrogen; 20_000];
    let assembled = output::assemble(&expensive)?;
    assert!(matches!(
        output::clean_up(&assembled),
        Err(output::Error::Limit(_))
    ));
    Ok(())
}
