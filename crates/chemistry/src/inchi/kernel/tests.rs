use super::*;
use crate::smiles;

#[test]
#[ignore = "isolated requested-Rust-allocation and callback refresh timing measurement"]
fn measure_toolkit_refresh() {
    use crate::allocation_metrics;
    use std::{hint::black_box, time::Instant};
    let state = smiles::prepare(&"C".repeat(1024)).unwrap().state;
    let molecule = to_adapter(&state, vec![], &[]).unwrap();
    for cached in [false, true] {
        let mut toolkit = Toolkit {
            state: cached.then(|| state.clone()),
            unspecified: vec![],
        };
        let baseline = allocation_metrics::reset();
        let start = Instant::now();
        for _ in 0..100 {
            black_box(toolkit.refresh(&molecule).unwrap());
        }
        let elapsed = start.elapsed();
        let snapshot = allocation_metrics::snapshot();
        println!(
            "cached={cached} atoms=1024 count=100 elapsed={elapsed:?} allocations={} allocated={} peak_extra={} retained_extra={}",
            snapshot.allocation_count,
            snapshot.allocated_bytes,
            snapshot.peak_bytes.saturating_sub(baseline),
            snapshot.live_bytes.saturating_sub(baseline)
        );
    }
}

#[test]
fn rust_kernel_generates_methane_without_reference_dependencies() {
    let imported = read("InChI=1S/CH4/h1H4", output::Options::default()).unwrap();
    let molecule = Molecule::prepare(&imported.state.unwrap(), None).unwrap();
    let result = generate(&molecule).unwrap();
    assert_eq!(result.inchi, "InChI=1S/CH4/h1H4");
    assert_eq!(result.status, 0);
}

#[test]
fn unchanged_graph_preserves_prepared_caches() {
    let state = smiles::prepare("C1CC1.C").unwrap().state;
    let molecule = to_adapter(&state, vec![], &[]).unwrap();
    let mut toolkit = Toolkit {
        state: Some(state.clone()),
        unspecified: vec![],
    };
    assert_eq!(
        serde_json::to_value(toolkit.refresh(&molecule).unwrap()).unwrap(),
        serde_json::to_value(state).unwrap()
    );
}

#[test]
fn cleanup_graph_edits_with_unchanged_counts_invalidate_computed_caches() {
    // Rewire a cycle into a chain, change bond orders, and reorder unlike atoms.
    // Each replacement keeps the sizes checked by the former reuse condition.
    for (before, after) in [("C1CC1.C", "CCCC"), ("C=CC", "CCC"), ("CCO", "OCC")] {
        let previous = smiles::prepare(before).unwrap().state;
        let replacement = smiles::prepare(after).unwrap().state;
        assert_eq!(previous.graph.atoms.len(), replacement.graph.atoms.len());
        assert_eq!(previous.graph.bonds.len(), replacement.graph.bonds.len());
        let mut molecule = to_adapter(&replacement, vec![], &[]).unwrap();
        let mut toolkit = Toolkit {
            state: Some(previous),
            unspecified: vec![],
        };
        toolkit.synchronize_after_cleanup(&molecule).unwrap();
        let refreshed = toolkit.state.as_ref().unwrap();
        assert_eq!(refreshed.rings.kind, RingKind::None, "{before} -> {after}");
        assert!(refreshed.rings.atoms.is_empty());
        assert!(refreshed.properties.done.is_none());
        assert!(refreshed.conjugated.iter().all(|value| !value));
        assert!(
            refreshed
                .hybridizations
                .iter()
                .all(|value| *value == Hybridization::Unspecified)
        );

        // A later callback must perceive the replacement graph's rings. This
        // checks the adapter contract, not the current kernel's callback order.
        toolkit.kekulize(&mut molecule, false, false).unwrap();
        let updated = toolkit.state.as_ref().unwrap();
        assert_eq!(updated.rings.kind, RingKind::Basis);
        assert!(updated.rings.atoms.is_empty());
    }
}

#[test]
fn canonical_kekulization_is_rejected_rather_than_ignored() {
    let state = smiles::prepare("c1ccccc1").unwrap().state;
    let mut molecule = to_adapter(&state, vec![], &[]).unwrap();
    let mut toolkit = Toolkit {
        state: Some(state),
        unspecified: vec![],
    };
    let error = toolkit.kekulize(&mut molecule, false, true).unwrap_err();
    assert!(error.message.contains("Canonical"), "{}", error.message);
    toolkit.kekulize(&mut molecule, false, false).unwrap();
}

#[test]
fn allene_generation_uses_real_2d_coordinates_and_terminal_wedges() {
    for explicit in [false, true] {
        let (text, center, points, terminal, methyl, hydrogen) = if explicit {
            (
                "[H]C(C)=C=C(C)[H]",
                3,
                vec![
                    [-2., -1., 0.],
                    [-1., 0., 0.],
                    [-2., 1., 0.],
                    [0., 0., 0.],
                    [1., 0., 0.],
                    [2., 1., 0.],
                    [2., -1., 0.],
                ],
                1,
                2,
                Some(0),
            )
        } else {
            (
                "CC=C=CC",
                2,
                vec![
                    [-2., 1., 0.],
                    [-1., 0., 0.],
                    [0., 0., 0.],
                    [1., 0., 0.],
                    [2., 1., 0.],
                ],
                1,
                0,
                None,
            )
        };
        for (hash, existing_wedges) in [(false, false), (true, false), (false, true), (true, true)]
        {
            let mut state = smiles::prepare(text).unwrap().state;
            if explicit {
                let parsed = smiles::parse(text).unwrap();
                state.graph = parsed.graph;
                state.metadata = parsed.metadata;
                state.directions = parsed.directions;
                state.valences = state.graph.valences().unwrap();
                state.properties = Properties::unspecified(&state.graph);
                state.hybridizations = vec![Hybridization::Unspecified; state.graph.atoms.len()];
                state.conjugated = vec![false; state.graph.bonds.len()];
                state.rings = RingCache::default();
            }
            state.metadata.atoms[center].chiral_tag = 5;
            state.metadata.atoms[center].chiral_permutation =
                Some(if explicit != hash { 1 } else { 2 });
            for (bond, direction) in state
                .graph
                .bonds
                .iter_mut()
                .zip(&mut state.directions)
                .filter(|_| existing_wedges)
            {
                if [bond.a, bond.b].contains(&terminal) && [bond.a, bond.b].contains(&methyl) {
                    bond.a = terminal;
                    bond.b = methyl;
                    *direction = if hash {
                        Direction::Hash
                    } else {
                        Direction::Wedge
                    };
                } else if hydrogen.is_some_and(|h| {
                    [bond.a, bond.b].contains(&terminal) && [bond.a, bond.b].contains(&h)
                }) {
                    bond.a = terminal;
                    bond.b = hydrogen.unwrap();
                    *direction = if hash {
                        Direction::Wedge
                    } else {
                        Direction::Hash
                    };
                }
            }
            assert_eq!(state.graph.atoms.len(), points.len());
            let positions = points
                .iter()
                .map(|&[x, y, z]| Point3 { x, y, z })
                .collect::<Vec<_>>();
            let molecule = Molecule::prepare(&state, Some(&positions)).unwrap();
            let before = serde_json::to_value(&molecule).unwrap();
            let result = generate(&molecule).unwrap();
            assert_eq!(
                result.inchi,
                if hash {
                    "InChI=1S/C5H8/c1-3-5-4-2/h3-4H,1-2H3/t5-/m1/s1"
                } else {
                    "InChI=1S/C5H8/c1-3-5-4-2/h3-4H,1-2H3/t5-/m0/s1"
                },
                "explicit H={explicit}, existing wedges={existing_wedges}"
            );
            assert_eq!(result.status, 0);
            assert_eq!(serde_json::to_value(&molecule).unwrap(), before);
        }
    }
}

#[test]
fn allene_generation_rejects_ambiguous_or_nonplanar_geometry() {
    let mut state = smiles::prepare("CC=C=CC").unwrap().state;
    state.metadata.atoms[2].chiral_tag = 5;
    state.metadata.atoms[2].chiral_permutation = Some(1);
    let points =
        [(-2., 1.), (-1., 0.), (0., 0.), (1., 0.), (2., 1.)].map(|(x, y)| Point3 { x, y, z: 0. });
    let mut molecule = Molecule::prepare(&state, Some(&points)).unwrap();
    molecule.positions.as_mut().unwrap()[0].z = 1.;
    assert!(generate(&molecule).unwrap_err().contains("2D coordinates"));

    molecule.positions = Some(vec![Point3::default(); 5]);
    assert!(generate(&molecule).is_err());

    // A wedge from the near terminal to its methyl depicts M, in conflict
    // with the stored P configuration. Do not silently choose either one.
    molecule.positions = Some(points.to_vec());
    molecule.state.graph.bonds[0].a = 1;
    molecule.state.graph.bonds[0].b = 0;
    molecule.state.directions[0] = Direction::Wedge;
    assert!(generate(&molecule).unwrap_err().contains("conflicting"));
}

#[test]
fn allene_generation_keeps_both_end_wedges_unspecified_without_mutation() {
    let mut state = smiles::prepare("CC=C=CC").unwrap().state;
    state.graph.bonds[0].a = 1;
    state.graph.bonds[0].b = 0;
    state.directions[0] = Direction::Wedge;
    state.directions[3] = Direction::Hash;
    let points =
        [(-2., 1.), (-1., 0.), (0., 0.), (1., 0.), (2., 1.)].map(|(x, y)| Point3 { x, y, z: 0. });
    let molecule = Molecule::prepare(&state, Some(&points)).unwrap();
    let before = serde_json::to_value(&molecule).unwrap();
    let generated = generate(&molecule).unwrap();
    assert_eq!(generated.inchi, "InChI=1S/C5H8/c1-3-5-4-2/h3-4H,1-2H3");
    assert_eq!(serde_json::to_value(&molecule).unwrap(), before);

    // Low-level callers can still supply one unambiguous native wedge.
    let mut single_end = molecule.clone();
    single_end.state.directions[3] = Direction::None;
    assert_eq!(
        generate(&single_end).unwrap().inchi,
        "InChI=1S/C5H8/c1-3-5-4-2/h3-4H,1-2H3/t5-/m0/s1"
    );

    // Explicit allene metadata in conflict with both-ended wedges is still
    // rejected by recovery validation instead of suppressing its annotation.
    let mut specified = molecule;
    specified.state.metadata.atoms[2].chiral_tag = 5;
    specified.state.metadata.atoms[2].chiral_permutation = Some(1);
    assert!(generate(&specified).unwrap_err().contains("conflicting"));
}

#[test]
fn scalar_allene_import_retains_the_dependency_limitation_diagnostic() {
    let imported = read(
        "InChI=1S/C5H8/c1-3-5-4-2/h3-4H,1-2H3/t5-/m0/s1",
        output::Options::default(),
    )
    .unwrap();
    assert!(
        imported
            .diagnostics
            .iter()
            .any(|message| message.contains("Allene-style stereochemistry is not supported"))
    );
    assert!(
        imported
            .state
            .unwrap()
            .metadata
            .atoms
            .iter()
            .all(|atom| atom.chiral_tag != 5)
    );
}

#[test]
fn allene_generation_distinguishes_fully_substituted_enantiomers() {
    let mut identifiers = Vec::new();
    for text in ["FC(Cl)=[C@AL1]=C(Br)I", "FC(Cl)=[C@AL2]=C(Br)I"] {
        let state = smiles::prepare(text).unwrap().state;
        let positions = crate::depict::compute(
            &state,
            &vec![None; state.graph.atoms.len()],
            None,
            Default::default(),
        )
        .unwrap()
        .positions;
        let molecule = Molecule::prepare(&state, Some(&positions)).unwrap();
        let generated = generate(&molecule).unwrap();
        assert!(
            generated.inchi.contains("/t"),
            "{text}: {}",
            generated.inchi
        );
        assert!(
            generated.diagnostics.is_empty(),
            "{:?}",
            generated.diagnostics
        );
        identifiers.push(generated.inchi);
    }
    assert_ne!(identifiers[0], identifiers[1]);
}

#[test]
fn allene_generation_is_independent_of_smiles_carrier_order() {
    let mut identifiers = Vec::new();
    for text in ["CC(F)=[C@AL1]=CC", "CC=[C@AL2]=C(C)F"] {
        let state = smiles::prepare(text).unwrap().state;
        let positions = crate::depict::compute(
            &state,
            &vec![None; state.graph.atoms.len()],
            None,
            Default::default(),
        )
        .unwrap()
        .positions;
        let molecule = Molecule::prepare(&state, Some(&positions)).unwrap();
        let generated = generate(&molecule).unwrap();
        assert!(
            generated.inchi.contains("/t"),
            "{text}: {}",
            generated.inchi
        );
        identifiers.push(generated.inchi);
    }
    assert_eq!(identifiers[0], identifiers[1]);
}
