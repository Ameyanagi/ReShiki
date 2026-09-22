#[path = "common/fixture.rs"]
mod fixture;

use reshiki::chemistry::{
    RDKIT_VERSION,
    electronic::Hybridization,
    graph::{Atom, Graph},
    inchi::input::{self, Input},
    ranking::Metadata,
    stereo::{Point3, perception::State},
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Capture {
    rdkit_version: String,
    rdkit_commit: String,
    adapter_sha256: String,
    header_sha256: String,
    rows: Vec<Case>,
}

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
    let capture: Capture = serde_json::from_reader(fixture::open("inchi-input-native.json.gz")?)?;
    assert_eq!(capture.rdkit_version, RDKIT_VERSION);
    assert_eq!(
        capture.rdkit_commit,
        "0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985"
    );
    assert_eq!(
        capture.adapter_sha256,
        "68c9b20d1d5920ed602ea931c1429395280c3d040971593073917618015183d1"
    );
    assert_eq!(
        capture.header_sha256,
        "2d41d745be35a47853bf67fea03a46b1f518e85c42af9027a104662dbcb20116"
    );
    let mut accepted = 0;
    let mut rejected = 0;
    let mut undefined = 0;
    let mut failures = Vec::new();
    assert_eq!(capture.rows.len(), 11_886, "Native input corpus changed");
    for case in capture.rows {
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
    println!(
        "Native input arrays: {accepted} accepted, {rejected} native failures, {undefined} undefined-native inputs rejected"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert_eq!((accepted, rejected, undefined), (11_772, 64, 50));
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
