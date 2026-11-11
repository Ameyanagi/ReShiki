use super::*;
use crate::graph::{Atom, Bond};
type TestResult = anyhow::Result<()>;

fn ring(size: usize) -> Graph {
    Graph {
        atoms: vec![
            Atom {
                atomic_number: 6,
                aromatic: true,
                ..Atom::default()
            };
            size
        ],
        bonds: (0..size)
            .map(|a| Bond {
                a,
                b: (a + 1) % size,
                order: 4,
                aromatic: true,
            })
            .collect(),
    }
}

#[test]
fn failure_after_normalization_keeps_input_unchanged() -> TestResult {
    let mut graph = ring(5); // Cannot assign alternating Kekule bonds.
    // This neutral nitro group would be normalized before the later failure.
    graph.atoms.extend([7, 8, 8, 6].map(|atomic_number| Atom {
        atomic_number,
        ..Atom::default()
    }));
    graph
        .bonds
        .extend([(5, 6, 2), (5, 7, 2), (5, 8, 1)].map(|(a, b, order)| Bond {
            a,
            b,
            order,
            aromatic: false,
        }));
    let meta = Metadata::unspecified(&graph);
    let dirs = vec![Direction::None; graph.bonds.len()];
    let before = serde_json::to_value((&graph, &meta, &dirs))?;
    let error = sanitize(&graph, &meta, &dirs).expect_err("Odd aromatic carbon ring must fail");
    assert_eq!(error.stage, Stage::Kekulize);
    assert_eq!(serde_json::to_value((&graph, &meta, &dirs))?, before);
    Ok(())
}

#[test]
fn invalid_input_and_cache_fail_without_panics() -> TestResult {
    let graph = ring(6);
    let mut meta = Metadata::unspecified(&graph);
    let dirs = vec![Direction::None; graph.bonds.len()];
    assert_eq!(
        sanitize(&graph, &meta, &[])
            .expect_err("Missing directions")
            .stage,
        Stage::Input
    );
    meta.atoms[0].chiral_tag = 9;
    assert_eq!(
        sanitize(&graph, &meta, &dirs)
            .expect_err("Invalid chiral tag")
            .stage,
        Stage::Input
    );
    assert!(graph.cached_valences(Some(&[])).is_err());
    let mut cache = graph.valences().map_err(anyhow::Error::msg)?;
    cache[0].explicit_valence = u32::MAX;
    assert!(graph.refresh_implicit(&cache).is_err());
    let mut invalid = graph.clone();
    invalid.bonds[0].a = usize::MAX;
    assert_eq!(
        sanitize(&invalid, &Metadata::unspecified(&invalid), &dirs)
            .expect_err("Invalid endpoint")
            .stage,
        Stage::Input
    );
    Ok(())
}

#[test]
fn long_chain_pipeline_is_iterative() -> TestResult {
    let mut graph = ring(30_000);
    graph.bonds.pop();
    for atom in &mut graph.atoms {
        atom.aromatic = false;
    }
    for bond in &mut graph.bonds {
        bond.order = 1;
        bond.aromatic = false;
    }
    let result = sanitize(
        &graph,
        &Metadata::unspecified(&graph),
        &vec![Direction::None; graph.bonds.len()],
    )?;
    assert_eq!(result.valences[0].implicit_hydrogens, 3);
    assert_eq!(result.valences[1].implicit_hydrogens, 2);
    assert!(result.rings.is_empty());
    assert!(
        result
            .hybridizations
            .iter()
            .all(|&h| h == Hybridization::Sp3)
    );
    Ok(())
}

#[test]
fn dense_ring_preparation_preserves_the_input_graph() -> TestResult {
    let graph: Graph = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/ring-order-dependent.json"
    ))?;
    let original = serde_json::to_value(&graph)?;
    let state = sanitize(
        &graph,
        &Metadata::unspecified(&graph),
        &vec![Direction::None; graph.bonds.len()],
    )?;
    assert!(!state.rings.is_empty());
    assert_eq!(serde_json::to_value(&graph)?, original);
    Ok(())
}

#[test]
fn dummy_kekulization_uses_normalized_basis_before_symmetric_rings() -> TestResult {
    // RDKit #9427 and #9208, checked with 2026.09.1 legacy ring finding.
    for (text, order, success) in [
        ("*12cc(C1)c2", vec![0, 1, 2, 3, 4], true),
        ("*12cc(C1)c2", vec![4, 3, 2, 1, 0], true),
        ("c1cc*CC1", vec![0, 1, 2, 3, 4, 5], true),
        ("c1cc*CC1", vec![1, 0, 2, 3, 4, 5], false),
        ("c1cc*CC1", vec![1, 2, 0, 3, 4, 5], true),
        ("c1cc*CC1", vec![0, 2, 3, 4, 5, 1], false),
    ] {
        let parsed = crate::smiles::parse(text)?;
        let mut inverse = vec![0; order.len()];
        for (new, &old) in order.iter().enumerate() {
            inverse[old] = new;
        }
        let graph = Graph {
            atoms: order
                .iter()
                .map(|&id| parsed.graph.atoms[id].clone())
                .collect(),
            bonds: parsed
                .graph
                .bonds
                .iter()
                .map(|bond| Bond {
                    a: inverse[bond.a],
                    b: inverse[bond.b],
                    ..bond.clone()
                })
                .collect(),
        };
        let rings = rings::perceive(&graph, rings::Options::default())?;
        let result = sanitize(
            &graph,
            &Metadata::unspecified(&graph),
            &vec![Direction::None; graph.bonds.len()],
        );
        assert_eq!(result.is_ok(), success, "{text} {order:?}: {result:?}");
        if let Ok(state) = result {
            assert_eq!(state.rings, rings.atoms);
            if text == "*12cc(C1)c2" {
                assert_eq!(rings.basis_count, 2);
                assert_eq!(state.rings.len(), 3);
            }
        } else {
            assert_eq!(
                result.expect_err("Invalid Kekule assignment").stage,
                Stage::Kekulize
            );
        }
    }
    Ok(())
}

#[test]
fn normalized_dummy_ring_sanitizes_mixed_1457() -> TestResult {
    // sanitize_reference.py mixed 1457: RDKit #9208 changes dummy eligibility.
    let mut graph = ring(6);
    graph.atoms[0] = Atom {
        atomic_number: 15,
        explicit_hydrogens: 1,
        no_implicit: true,
        radical_electrons: 1,
        ..Atom::default()
    };
    graph.atoms[3] = Atom {
        atomic_number: 16,
        explicit_hydrogens: 1,
        no_implicit: true,
        aromatic: true,
        radical_electrons: 1,
        ..Atom::default()
    };
    graph.atoms[5] = Atom {
        charge: -1,
        no_implicit: true,
        ..Atom::default()
    };
    graph.bonds[1].aromatic = false;
    let mut directions = vec![Direction::None; 6];
    directions[1] = Direction::Hash;
    let metadata = Metadata::unspecified(&graph);
    let state = sanitize(&graph, &metadata, &directions)?;
    for atom in &mut graph.atoms {
        atom.aromatic = false;
    }
    graph.atoms[0].radical_electrons = 0;
    for (bond, order) in graph.bonds.iter_mut().zip([1, 4, 1, 1, 2, 1]) {
        bond.order = order;
        bond.aromatic = false;
    }
    assert_eq!(state.graph, graph);
    assert_eq!(state.directions, directions);
    assert_eq!(
        serde_json::to_value(state.metadata)?,
        serde_json::to_value(metadata)?
    );
    assert_eq!(state.rings, [vec![0, 1, 2, 3, 4, 5]]);
    assert_eq!(state.conjugated, [false; 6]);
    assert_eq!(
        state.hybridizations,
        [
            Hybridization::Sp3,
            Hybridization::Sp2,
            Hybridization::Sp2,
            Hybridization::Sp3,
            Hybridization::Sp2,
            Hybridization::Unspecified
        ]
    );
    assert_eq!(
        state.valences,
        [0, 1, 1, 0, 1, 0].map(|implicit_hydrogens| Valence {
            explicit_valence: 3,
            implicit_hydrogens,
        })
    );
    Ok(())
}

#[test]
fn canonical_retry_preserves_fused_aromatic_graph() -> TestResult {
    // #9380's retry still uses the SSSR after #9427 (sanitize oracle special /4).
    let mut graph = crate::smiles::parse(concat!(
        "c1cc2ccc3c4c(ccc(c1)c24)c1c2c4ccc5cccc6ccc(c4c65)c4c5cccc6c7cccc8c9cc",
        "cc%10c%11cccc%12c3c1c1c(c%11%12)c(c9%10)c(c87)c(c65)c1c42"
    ))?
    .graph;
    let order = [
        52, 19, 35, 61, 34, 6, 42, 47, 49, 44, 58, 27, 15, 38, 29, 53, 48, 0, 46, 17, 2, 50, 43,
        37, 51, 24, 5, 33, 30, 45, 11, 21, 54, 20, 3, 16, 28, 4, 57, 25, 31, 39, 12, 32, 22, 56,
        36, 10, 13, 14, 18, 23, 41, 55, 26, 40, 7, 8, 9, 60, 1, 59,
    ];
    let atoms = graph.atoms.clone();
    for (old, &new) in order.iter().enumerate() {
        graph.atoms[new] = atoms[old].clone();
    }
    for bond in &mut graph.bonds {
        bond.a = order[bond.a];
        bond.b = order[bond.b];
    }
    let state = sanitize(
        &graph,
        &Metadata::unspecified(&graph),
        &vec![Direction::None; graph.bonds.len()],
    )?;
    assert!(state.canonical_retry);
    assert_eq!(state.graph, graph);
    assert_eq!(
        state.rings,
        rings::perceive(&graph, rings::Options::default())?.atoms
    );
    Ok(())
}
