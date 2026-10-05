use super::*;
use crate::chemistry::{smiles, stereo::Point3};

fn molecule() -> Molecule {
    Molecule {
        state: smiles::prepare("C1CCC1").unwrap().state,
        positions: Some(vec![Point3::default(); 4]),
    }
}

fn owned_encode(input: &Molecule, heap_bytes: usize) -> Result<Vec<u8>, Error> {
    input
        .validate()
        .map_err(|_| Error::Input("Invalid molecular graph, annotations or coordinates"))?;
    wire::encode(
        &wire::Request {
            heap_bytes,
            operation: wire::Operation::Generate(Box::new(input.clone())),
        },
        MAX_REQUEST_BYTES,
    )
    .map_err(|_| Error::Limit("request"))
}

#[test]
fn borrowed_generation_matches_owned_frames_and_byte_boundaries() {
    let budget = super::super::DEFAULT_HEAP_BYTES;
    for text in ["", "c1ccccc1", "N[C@@H](C)C(=O)O", "F/C=C/F", "C.C"] {
        for coordinates in [false, true] {
            let state = smiles::prepare(text).unwrap().state;
            let count = state.graph.atoms.len();
            let input = Molecule {
                state,
                positions: coordinates.then(|| {
                    (0..count)
                        .map(|i| Point3 {
                            x: i as f64 * 0.5,
                            y: -0.,
                            z: 1e-24,
                        })
                        .collect()
                }),
            };
            let actual = encode(&input, budget).unwrap();
            assert_eq!(
                actual,
                owned_encode(&input, budget).unwrap(),
                "{text}, coordinates={coordinates}"
            );
            let request: wire::Request = wire::decode(&actual, MAX_REQUEST_BYTES).unwrap();
            request.validate().unwrap();
            assert_eq!(wire::encode(&request, MAX_REQUEST_BYTES).unwrap(), actual);
        }
    }
    // Structural transport accepts untyped legacy CIP strings. Use one to
    // exercise the exact byte boundary without constructing a huge graph.
    let mut input = molecule();
    input.state.properties.atoms[0].cip_code = Some(String::new());
    let overhead = encode(&input, budget).unwrap().len();
    for extra in [0, 1] {
        input.state.properties.atoms[0].cip_code =
            Some("R".repeat(MAX_REQUEST_BYTES - overhead + extra));
        let actual = encode(&input, budget);
        let expected = owned_encode(&input, budget);
        if extra == 0 {
            let actual = actual.unwrap();
            assert_eq!(actual.len(), MAX_REQUEST_BYTES);
            assert_eq!(actual, expected.unwrap());
        } else {
            assert!(matches!(actual, Err(Error::Limit("request"))));
            assert!(matches!(expected, Err(Error::Limit("request"))));
        }
    }
}

#[test]
fn owned_wire_constructors_and_diagnostic_names_are_preserved() {
    let request = wire::Request {
        heap_bytes: super::super::DEFAULT_HEAP_BYTES,
        operation: wire::Operation::Read {
            inchi: "InChI=1S/CH4/h1H4".into(),
            options: output::Options::default(),
        },
    };
    request.validate().unwrap();
    assert!(format!("{request:?}").starts_with("Request {"));
    let bytes = wire::encode(&"invalid", MAX_REQUEST_BYTES).unwrap();
    assert!(
        wire::decode::<wire::Request>(&bytes, MAX_REQUEST_BYTES)
            .unwrap_err()
            .contains("struct Request")
    );
    let bytes = wire::encode(
        &serde_json::json!({
            "heap_bytes": super::super::DEFAULT_HEAP_BYTES,
            "operation": {"Read": "invalid"}
        }),
        MAX_REQUEST_BYTES,
    )
    .unwrap();
    assert!(
        wire::decode::<wire::Request>(&bytes, MAX_REQUEST_BYTES)
            .unwrap_err()
            .contains("struct variant Operation::Read")
    );
}

#[test]
#[ignore = "isolated requested-Rust-allocation and transport timing measurement"]
fn measure_borrowed_generation_transport() {
    use crate::allocation_metrics;
    use std::{hint::black_box, time::Instant};
    let state = smiles::prepare(&["C"; 1024].join(".")).unwrap().state;
    let input = Molecule {
        state,
        positions: Some(vec![Point3::default(); 1024]),
    };
    let budget = super::super::DEFAULT_HEAP_BYTES;
    for borrowed in [false, true] {
        let baseline = allocation_metrics::reset();
        let start = Instant::now();
        for _ in 0..20 {
            black_box(if borrowed {
                encode(&input, budget)
            } else {
                owned_encode(&input, budget)
            })
            .unwrap();
        }
        let elapsed = start.elapsed();
        let snapshot = allocation_metrics::snapshot();
        println!(
            "borrowed={borrowed} count=20 atoms=1024 elapsed={elapsed:?} allocations={} allocated={} peak_extra={} retained_extra={}",
            snapshot.allocation_count,
            snapshot.allocated_bytes,
            snapshot.peak_bytes.saturating_sub(baseline),
            snapshot.live_bytes.saturating_sub(baseline)
        );
    }
}

fn imported(molecule: &Molecule) -> Vec<u8> {
    wire::encode(
        &wire::Response {
            version: kernel::VERSION.into(),
            result: Ok(wire::Reply::Imported(Box::new(kernel::Imported {
                status: 0,
                message: String::new(),
                log: String::new(),
                state: Some(molecule.state.clone()),
                unspecified_bonds: vec![false; molecule.state.graph.bonds.len()],
                diagnostics: vec![],
            }))),
        },
        super::super::MAX_RESPONSE_BYTES,
    )
    .unwrap()
}

fn assert_invalid_state(molecule: Molecule, name: &str) {
    let budget = super::super::DEFAULT_HEAP_BYTES;
    assert!(molecule.validate().is_err(), "{name}: molecule validation");
    assert!(
        matches!(encode(&molecule, budget), Err(Error::Input(_))),
        "{name}: request encoding"
    );
    assert!(
        matches!(decode_import(&imported(&molecule)), Err(Error::Protocol(_))),
        "{name}: response decoding"
    );
    assert!(
        Molecule::prepare(&molecule.state, molecule.positions.as_deref()).is_err(),
        "{name}: application preparation"
    );
    assert!(kernel::generate(&molecule).is_err(), "{name}: kernel entry");
    assert!(
        wire::Request {
            heap_bytes: budget,
            operation: wire::Operation::Generate(Box::new(molecule)),
        }
        .validate()
        .is_err(),
        "{name}: helper request validation"
    );
}

#[test]
fn serialized_annotation_dimensions_are_checked_at_every_boundary() {
    let original = serde_json::to_value(molecule()).unwrap();
    for field in [
        "/state/metadata/atoms",
        "/state/metadata/bonds",
        "/state/directions",
        "/state/valences",
        "/state/conjugated",
        "/state/hybridizations",
        "/state/properties/atoms",
        "/state/properties/bond_codes",
    ] {
        for grow in [false, true] {
            let mut value = original.clone();
            let array = value.pointer_mut(field).unwrap().as_array_mut().unwrap();
            if grow {
                array.push(array.first().unwrap().clone());
            } else {
                array.pop();
            }
            assert_invalid_state(
                serde_json::from_value(value).unwrap(),
                &format!("{field}, grow={grow}"),
            );
        }
    }
}

#[test]
fn serialized_ring_and_stereo_indices_are_checked() {
    for ring in [vec![0, 1], vec![0, 1, 1], vec![0, 1, 4], vec![0, 1, 3]] {
        let mut molecule = molecule();
        molecule.state.rings.atoms = vec![ring];
        assert_invalid_state(molecule, "invalid cached cycle");
    }
    let mut uninitialized = molecule();
    uninitialized.state.rings.kind = crate::chemistry::stereo::perception::RingKind::None;
    assert_invalid_state(uninitialized, "uninitialized ring cache with cycles");
    for member in [0, 5, -5, i32::MIN] {
        let mut molecule = molecule();
        molecule
            .state
            .properties
            .atoms
            .first_mut()
            .unwrap()
            .ring_members = Some(vec![member]);
        assert_invalid_state(molecule, "invalid stereo ring member");
    }
    let mut molecule = molecule();
    molecule
        .state
        .metadata
        .bonds
        .first_mut()
        .unwrap()
        .stereo_atoms = vec![4];
    assert_invalid_state(molecule, "invalid stereo atom");
}

#[test]
fn generation_boundaries_reject_invalid_coordinates() {
    for positions in [
        vec![],
        vec![
            Point3 {
                x: f64::INFINITY,
                y: 0.0,
                z: 0.0
            };
            4
        ],
    ] {
        let mut molecule = molecule();
        molecule.positions = Some(positions);
        assert!(molecule.validate().is_err());
        assert!(encode(&molecule, super::super::DEFAULT_HEAP_BYTES).is_err());
        assert!(
            wire::Request {
                heap_bytes: super::super::DEFAULT_HEAP_BYTES,
                operation: wire::Operation::Generate(Box::new(molecule)),
            }
            .validate()
            .is_err()
        );
    }
}

#[test]
fn structural_boundaries_do_not_run_chemical_preparation() {
    // An odd aromatic carbon cycle cannot be kekulized. It is still a
    // structurally valid unsanitized state and can cross the Read boundary.
    let mut state = smiles::prepare("C1CCCC1").unwrap().state;
    for atom in &mut state.graph.atoms {
        atom.aromatic = true;
    }
    for bond in &mut state.graph.bonds {
        bond.aromatic = true;
        bond.order = 4;
    }
    state.valences = state.graph.provisional_valences().unwrap();
    let molecule = Molecule {
        state,
        positions: None,
    };
    assert!(matches!(
        Molecule::prepare(&molecule.state, None),
        Err(crate::chemistry::inchi::input::Error::Kekule(_))
    ));
    let budget = super::super::DEFAULT_HEAP_BYTES;
    let request = encode(&molecule, budget).unwrap();
    wire::decode::<wire::Request>(&request, MAX_REQUEST_BYTES)
        .unwrap()
        .validate()
        .unwrap();
    let actual = decode_import(&imported(&molecule)).unwrap();
    assert_eq!(
        serde_json::to_value(actual.state.unwrap()).unwrap(),
        serde_json::to_value(molecule.state).unwrap()
    );
}

fn generated(status: Status, inchi: &str) -> Vec<u8> {
    wire::encode(
        &wire::Response {
            version: kernel::VERSION.into(),
            result: Ok(wire::Reply::Generated(kernel::Generated {
                status: i32::from(status.code()),
                inchi: inchi.into(),
                message: String::new(),
                log: String::new(),
                auxiliary: String::new(),
                diagnostics: vec![],
            })),
        },
        super::super::MAX_RESPONSE_BYTES,
    )
    .unwrap()
}

#[test]
fn failure_statuses_cannot_publish_an_identifier() {
    for status in [
        Status::Break,
        Status::Skipped,
        Status::Empty,
        Status::Error,
        Status::Fatal,
        Status::Unknown,
        Status::Busy,
    ] {
        assert!(
            matches!(
                decode(&generated(status, "InChI=1S/CH4/h1H4")),
                Err(Error::Protocol(_))
            ),
            "{status:?} with an identifier must be a protocol error"
        );
        let empty = decode(&generated(status, "")).unwrap();
        assert_eq!(empty.status, status);
        assert!(empty.inchi.is_empty());
    }
}

#[test]
fn successful_and_warning_statuses_preserve_identifiers() {
    for status in [Status::Success, Status::Warning] {
        for inchi in ["", "InChI=1S/CH4/h1H4"] {
            let result = decode(&generated(status, inchi)).unwrap();
            assert_eq!(result.status, status);
            assert_eq!(result.inchi, inchi);
        }
    }
}
