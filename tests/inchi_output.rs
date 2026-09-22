//! Independent captures from the exact pinned native import adapter.
use anyhow::Context;
use reshiki::chemistry::{RDKIT_VERSION, inchi::output};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::BTreeMap,
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
    let (mut topology, mut assembly, mut native_failures) = (0, 0, 0);
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        if case.operation == "cleanup" {
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
    }
    anyhow::ensure!(child.wait()?.success(), "Fixture reader failed");
    anyhow::ensure!(topology > 5000 && assembly > 5000 && native_failures > 10);
    eprintln!(
        "InChI captures: {topology} topologies, {assembly} stereo assemblies, {native_failures} rejected stages"
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
    Ok(())
}
