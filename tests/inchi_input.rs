use anyhow::Context;
use reshiki::chemistry::{
    RDKIT_VERSION,
    electronic::Hybridization,
    graph::{Atom, Graph},
    inchi::input::{self, Input},
    ranking::Metadata,
    stereo::{Point3, perception::State},
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
    state: State,
    positions: Option<Vec<Point3>>,
    undefined_native: bool,
    expected: Option<Input>,
}

#[test]
fn prepared_arrays_match_the_pinned_native_adapter() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/inchi_input_reference.py"))
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(
        child
            .stdout
            .take()
            .context("Missing native adapter output")?,
    )
    .lines();
    let header: serde_json::Value =
        serde_json::from_str(&lines.next().context("Missing native adapter header")??)?;
    assert_eq!(header["rdkit_version"], RDKIT_VERSION);
    assert_eq!(
        header["rdkit_commit"],
        "0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985"
    );
    assert_eq!(
        header["adapter_sha256"],
        "68c9b20d1d5920ed602ea931c1429395280c3d040971593073917618015183d1"
    );
    assert_eq!(
        header["header_sha256"],
        "2d41d745be35a47853bf67fea03a46b1f518e85c42af9027a104662dbcb20116"
    );
    let mut accepted = 0;
    let mut rejected = 0;
    let mut undefined = 0;
    let mut failures = Vec::new();
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        let before = serde_json::to_value(&case.state)?;
        let result = input::prepare(&case.state, case.positions.as_deref());
        assert_eq!(
            serde_json::to_value(&case.state)?,
            before,
            "Source mutation: {}",
            case.name
        );
        match (case.expected, result) {
            (Some(expected), Ok(actual)) if actual == expected => accepted += 1,
            (None, Err(_)) => {
                if case.undefined_native {
                    undefined += 1;
                } else {
                    rejected += 1;
                }
            }
            (expected, actual) => {
                if failures.len() < 20 {
                    failures.push(format!(
                        "{}\nexpected={expected:?}\nactual={actual:?}",
                        case.name
                    ));
                }
            }
        }
    }
    assert!(child.wait()?.success());
    println!(
        "Native input arrays: {accepted} accepted, {rejected} native failures, {undefined} undefined-native inputs rejected"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(accepted > 10_000 && rejected > 20 && undefined > 0);
    Ok(())
}

#[test]
fn malformed_arrays_and_unrepresentable_input_fail_without_mutation() -> anyhow::Result<()> {
    let graph = Graph {
        atoms: vec![Atom {
            atomic_number: 6,
            ..Atom::default()
        }],
        bonds: vec![],
    };
    let mut state = State {
        valences: graph.valences().map_err(anyhow::Error::msg)?,
        metadata: Metadata::unspecified(&graph),
        directions: vec![],
        hybridizations: vec![Hybridization::Sp3],
        conjugated: vec![],
        rings: Default::default(),
        properties: reshiki::chemistry::stereo::perception::Properties::unspecified(&graph),
        graph,
    };
    let empty = input::prepare(&state, None)?;
    let explicit_zero = input::prepare(
        &state,
        Some(&[Point3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        }]),
    )?;
    assert_eq!(empty.atoms, explicit_zero.atoms);
    assert!(!empty.has_coordinates && explicit_zero.has_coordinates);
    let before = serde_json::to_value(&state)?;
    for positions in [
        vec![],
        vec![Point3 {
            x: f64::NAN,
            y: 0.0,
            z: 0.0,
        }],
        vec![Point3 {
            x: 0.0,
            y: f64::INFINITY,
            z: 0.0,
        }],
        vec![Point3 {
            x: 0.0,
            y: 0.0,
            z: f64::NEG_INFINITY,
        }],
    ] {
        assert!(matches!(
            input::prepare(&state, Some(&positions)),
            Err(input::Error::Invalid(_))
        ));
        assert_eq!(serde_json::to_value(&state)?, before);
    }
    state.metadata.atoms.clear();
    assert!(matches!(
        input::prepare(&state, None),
        Err(input::Error::Invalid(_))
    ));
    state.metadata = Metadata::unspecified(&state.graph);
    state.valences.clear();
    assert!(matches!(
        input::prepare(&state, None),
        Err(input::Error::Invalid(_))
    ));
    state.graph.atoms.resize(32768, Atom::default());
    assert!(matches!(
        input::prepare(&state, None),
        Err(input::Error::Limit(_))
    ));
    Ok(())
}
