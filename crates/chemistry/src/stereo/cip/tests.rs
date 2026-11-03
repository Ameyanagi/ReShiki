use super::*;
use crate::{
    electronic::Hybridization,
    graph::{Atom, Bond},
    kekulize::Direction,
    ranking::Metadata,
    stereo::perception::Properties,
};

fn chain(n: usize) -> anyhow::Result<State> {
    let graph = Graph {
        atoms: vec![
            Atom {
                atomic_number: 6,
                ..Atom::default()
            };
            n
        ],
        bonds: (1..n)
            .map(|b| Bond {
                a: b - 1,
                b,
                order: 1,
                aromatic: false,
            })
            .collect(),
    };
    Ok(State {
        metadata: Metadata::unspecified(&graph),
        directions: vec![Direction::None; graph.bonds.len()],
        valences: graph.provisional_valences().map_err(anyhow::Error::msg)?,
        conjugated: vec![false; graph.bonds.len()],
        hybridizations: vec![Hybridization::Sp3; graph.atoms.len()],
        rings: RingCache::default(),
        properties: Properties::unspecified(&graph),
        graph,
    })
}

#[test]
fn rejects_malformed_graph_caches_and_indices() -> anyhow::Result<()> {
    let valid = chain(4)?;
    let mut bad = valid.clone();
    bad.graph.bonds[0].b = usize::MAX;
    assert!(matches!(Molecule::new(&bad), Err(Error::Invalid(_))));
    let mut bad = valid.clone();
    bad.valences[0].implicit_hydrogens = u32::MAX;
    assert!(Molecule::new(&bad).is_err());
    let mut bad = valid.clone();
    bad.metadata.atoms.pop();
    assert!(Molecule::new(&bad).is_err());
    for ring in [vec![0, 1], vec![0, 1, 0, 2], vec![0, 1, 9], vec![0, 1, 3]] {
        let mut bad = valid.clone();
        bad.rings = RingCache {
            kind: RingKind::Fast,
            atoms: vec![ring],
        };
        assert!(Molecule::new(&bad).is_err());
    }
    let mut bad = valid.clone();
    bad.rings.atoms.push(vec![0, 1, 2]);
    assert!(Molecule::new(&bad).is_err());
    let mut bad = valid.clone();
    bad.rings = RingCache {
        kind: RingKind::Fast,
        atoms: vec![vec![0; 2_000_001]],
    };
    assert!(matches!(Molecule::new(&bad), Err(Error::Limit)));
    let mut mol = Molecule::new(&valid)?;
    assert!(mol.bond_order(usize::MAX).is_err());
    assert!(mol.is_in_ring(usize::MAX).is_err());
    assert!(mol.fraction(usize::MAX).is_err());
    assert!(mol.bond_types.is_none());
    assert!(mol.ring_bonds.is_none());
    assert!(mol.fractions.is_none());
    Ok(())
}

#[test]
fn deep_chain_is_iterative_and_preserves_the_input() -> anyhow::Result<()> {
    let mut input = chain(100_000)?;
    input.graph.atoms[99_999].isotope = 13;
    input.metadata.atoms[99_999].map_number = 73;
    let mut mol = Molecule::new(&input)?;
    assert_eq!(mol.fraction(99_999)?, Fraction(6, 1));
    assert_eq!(mol.bond_order(99_998)?, 1);
    assert!(!mol.is_in_ring(99_998)?);
    assert_eq!(input.graph.atoms[99_999].isotope, 13);
    assert_eq!(input.metadata.atoms[99_999].map_number, 73);
    assert_eq!(input.rings.kind, RingKind::None);
    Ok(())
}

#[test]
fn partial_bond_returns_a_typed_error_without_changing_the_input() -> anyhow::Result<()> {
    let mut input = chain(2)?;
    input.graph.bonds[0].order = 7;
    let before = serde_json::to_value(&input)?;
    let mut mol = Molecule::new(&input)?;
    assert!(matches!(
        mol.bond_order(0),
        Err(Error::BondOrder { index: 0, order: 7 })
    ));
    assert!(matches!(mol.fraction(0), Err(Error::BondOrder { .. })));
    assert_eq!(serde_json::to_value(&input)?, before);
    Ok(())
}

#[test]
fn negative_mancude_fractions_average_the_complete_part_independent_of_order() -> anyhow::Result<()>
{
    // IUPAC P-92.1.4.4 gives every cyclopentadienide duplicate Z = 24/5.
    // https://iupac.qmul.ac.uk/BlueBook/P9.html#92010404
    for text in ["[CH-]1C=CC=C1", "C1=C[CH-]C=C1"] {
        let original = crate::smiles::read(text)?.prepared.state;
        for reverse_atoms in [false, true] {
            let mut state = original.clone();
            if reverse_atoms {
                let n = state.graph.atoms.len();
                state.graph.atoms.reverse();
                state.metadata.atoms.reverse();
                state.valences.reverse();
                state.hybridizations.reverse();
                state.properties.atoms.reverse();
                for bond in &mut state.graph.bonds {
                    bond.a = n - 1 - bond.a;
                    bond.b = n - 1 - bond.b;
                }
                for ring in &mut state.rings.atoms {
                    for atom in ring {
                        *atom = n - 1 - *atom;
                    }
                }
            }
            let mut molecule = Molecule::new(&state)?;
            for atom in (0..state.graph.atoms.len()).rev() {
                assert_eq!(molecule.fraction(atom)?, Fraction(24, 5), "{text}, {atom}");
            }
        }
    }
    Ok(())
}

#[test]
fn equivalent_cyclopentadienide_axis_ligands_do_not_receive_helicity() -> anyhow::Result<()> {
    let mut state = crate::smiles::read("OC1=C(C=CC=C1)[C-]1C=CC=C1")?
        .prepared
        .state;
    let axis = state
        .graph
        .bonds
        .iter()
        .position(|bond| (bond.a, bond.b) == (2, 7) || (bond.a, bond.b) == (7, 2))
        .unwrap();
    state.metadata.bonds[axis].stereo = 6;
    state.metadata.bonds[axis].stereo_atoms = vec![1, 8];
    state.properties.bond_codes[axis] = Some("old".into());
    // The uniform negative-mancude average cannot distinguish these ring paths.
    // https://iupac.qmul.ac.uk/BlueBook/P9.html#92010404
    let result = label::assign(&state, &Default::default())?;
    assert!(result.labels.is_empty());
    assert_eq!(result.state.properties.bond_codes[axis], None);
    Ok(())
}

#[test]
fn complete_labeling_bounds_retained_graphs_and_selection_without_partial_edits()
-> anyhow::Result<()> {
    let mut state = chain(20_000)?;
    let inner_count = state.metadata.atoms.len().saturating_sub(2);
    for atom in state.metadata.atoms.iter_mut().skip(1).take(inner_count) {
        atom.chiral_tag = 1;
    }
    for atom in &mut state.properties.atoms {
        atom.cip_code = Some("old".into());
    }
    let before = serde_json::to_value(&state)?;
    assert!(matches!(
        label::assign(&state, &Default::default()),
        Err(Error::Limit)
    ));
    assert_eq!(serde_json::to_value(&state)?, before);

    let options = label::Options {
        atoms: Some(vec![1; 300_001]),
        ..Default::default()
    };
    assert!(matches!(label::assign(&state, &options), Err(Error::Limit)));
    assert_eq!(serde_json::to_value(&state)?, before);
    Ok(())
}

#[test]
fn complete_labeling_without_centers_preserves_large_graph_and_uninitialized_rings()
-> anyhow::Result<()> {
    let state = chain(100_000)?;
    let before = serde_json::to_value(&state)?;
    let result = label::assign(&state, &Default::default())?;
    assert!(result.labels.is_empty());
    assert_eq!(serde_json::to_value(&result.state)?, before);
    assert_eq!(serde_json::to_value(&state)?, before);
    Ok(())
}

#[test]
fn long_stereo_chain_labels_every_center() -> anyhow::Result<()> {
    // 400 centres once exhausted the storage of graphs retained for all centres.
    let text = format!("C{}", "[C@H](O)C".repeat(400));
    let state = crate::smiles::read(&text)?.prepared.state;
    let result = label::assign(&state, &Default::default())?;
    let codes = result
        .state
        .properties
        .atoms
        .iter()
        .filter_map(|atom| atom.cip_code.as_deref())
        .collect::<String>();
    // The two halves mirror each other, so the descriptor flips at the middle.
    assert_eq!(codes, "S".repeat(200) + &"R".repeat(200));
    assert_eq!(result.labels.len(), 400);
    Ok(())
}

fn allene_state(text: &str, center: usize, permutation: u32) -> anyhow::Result<State> {
    let mut state = crate::smiles::read(text)?.prepared.state;
    state.metadata.atoms[center].chiral_tag = 5;
    state.metadata.atoms[center].chiral_permutation = Some(permutation);
    Ok(state)
}

#[test]
fn extended_tetrahedral_labels_the_central_atom_with_exact_helicity() -> anyhow::Result<()> {
    // Centres ExtendedTetrahedral LEFT=1/RIGHT=2 maps priority-ordered
    // [near high, near low, far high, far low] to P/M respectively. Its SMILES
    // example CH3[CH]=[C@]=[CH]CH3 has [CH3,H,H,CH3] carriers and is therefore M.
    // Our graph order keeps H last at each end: [CH3,H,CH3,H].
    for (permutation, expected) in [(1, "P"), (2, "M")] {
        let state = allene_state("CC=C=CC", 2, permutation)?;
        let before = serde_json::to_value(&state)?;
        let result = label::assign(&state, &Default::default())?;
        assert_eq!(
            result.state.properties.atoms[2].cip_code.as_deref(),
            Some(expected)
        );
        assert_eq!(result.labels.len(), 1);
        assert_eq!(result.labels[0].target, configuration::Target::Atom(2));
        assert_eq!(
            result.labels[0].neighbor_order,
            [Some(0), None, Some(4), None]
        );
        assert_eq!(serde_json::to_value(&state)?, before);
    }
    // Rank each end independently, with the near pair before the far pair,
    // even though both far substituents have higher atomic numbers.
    let result = label::assign(&allene_state("FC(Cl)=C=C(I)Br", 3, 1)?, &Default::default())?;
    assert_eq!(
        result.state.properties.atoms[3].cip_code.as_deref(),
        Some("M")
    );
    assert_eq!(
        result.labels[0].neighbor_order,
        [Some(2), Some(0), Some(5), Some(6)]
    );
    Ok(())
}

#[test]
fn extended_tetrahedral_identical_terminal_ligands_clear_the_label() -> anyhow::Result<()> {
    for (text, center) in [("CC(C)=C=CC", 3), ("FC(F)=C=C(Cl)Br", 3)] {
        let mut state = allene_state(text, center, 1)?;
        state.properties.atoms[center].cip_code = Some("old".into());
        let result = label::assign(&state, &Default::default())?;
        assert!(result.labels.is_empty(), "{text}");
        assert_eq!(
            result.state.properties.atoms[center].cip_code, None,
            "{text}"
        );
    }
    Ok(())
}

#[test]
fn extended_tetrahedral_survives_bond_order_and_endpoint_permutations() -> anyhow::Result<()> {
    let original = allene_state("FC(Cl)=C=C(I)Br", 3, 1)?;
    for swap_near in [false, true] {
        for swap_far in [false, true] {
            for reverse_axis in [false, true] {
                let mut state = original.clone();
                for (swap, a, b) in [(swap_near, 0, 1), (swap_far, 4, 5), (reverse_axis, 2, 3)] {
                    if swap {
                        state.graph.bonds.swap(a, b);
                        state.metadata.bonds.swap(a, b);
                        state.directions.swap(a, b);
                        state.conjugated.swap(a, b);
                        state.properties.bond_codes.swap(a, b);
                    }
                }
                for bond in &mut state.graph.bonds {
                    std::mem::swap(&mut bond.a, &mut bond.b);
                }
                state.metadata.atoms[3].chiral_permutation =
                    Some(if swap_near != swap_far { 2 } else { 1 });
                let result = label::assign(&state, &Default::default())?;
                assert_eq!(
                    result.state.properties.atoms[3].cip_code.as_deref(),
                    Some("M")
                );
            }
        }
    }
    Ok(())
}

#[test]
fn extended_tetrahedral_labels_an_auxiliary_center_after_rerooting() -> anyhow::Result<()> {
    use configuration::Configuration;
    use digraph::{Descriptor, Digraph};
    use rules::{Iterations, Rules};
    let state = allene_state("FC(Cl)=C=C(I)Br", 3, 1)?;
    let mut mol = Molecule::new(&state)?;
    let mut configuration = Configuration::extended_tetrahedral(&mol, 3)?;
    let mut graph = Digraph::new(&mol, 0, false)?;
    let nodes = graph.nodes_for_atom(&mut mol, 3)?;
    let center = nodes
        .into_iter()
        .find(|&node| !graph.node(node).unwrap().is_duplicate())
        .unwrap();
    for _ in 0..2 {
        assert_eq!(
            configuration.label_at(
                &mut mol,
                &mut graph,
                center,
                &Rules::full(),
                &mut Iterations::new(0)
            )?,
            Descriptor::M
        );
    }
    Ok(())
}

#[test]
fn extended_tetrahedral_uses_pseudo_helicity_for_one_enantiomorphic_pair() -> anyhow::Result<()> {
    let state = allene_state("C([C@H](F)Cl)([C@@H](F)Cl)=C=C(I)Br", 7, 1)?;
    let result = label::assign(&state, &Default::default())?;
    assert_eq!(
        result.state.properties.atoms[1].cip_code.as_deref(),
        Some("R")
    );
    assert_eq!(
        result.state.properties.atoms[4].cip_code.as_deref(),
        Some("S")
    );
    let code = result.state.properties.atoms[7]
        .cip_code
        .as_deref()
        .unwrap();
    // R precedes S at the near end and I precedes Br at the far end, so
    // storage AL1 already has priority order and gives p.
    assert_eq!(code, "p");
    // Reflect every stereogenic unit. A pseudoasymmetric axis is invariant.
    let mut reflected = state;
    for atom in &mut reflected.metadata.atoms {
        if matches!(atom.chiral_tag, 1 | 2) {
            atom.chiral_tag ^= 3;
        }
    }
    reflected.metadata.atoms[7].chiral_permutation = Some(2);
    let mirror = label::assign(&reflected, &Default::default())?;
    assert_eq!(
        mirror.state.properties.atoms[7].cip_code.as_deref(),
        Some(code)
    );

    // Two enantiomorphic pairs do not make the axis pseudoasymmetric.
    let state = allene_state(
        "C([C@H](F)Cl)([C@@H](F)Cl)=C=C([C@H](F)Cl)[C@@H](F)Cl",
        7,
        1,
    )?;
    let result = label::assign(&state, &Default::default())?;
    assert_eq!(
        result.state.properties.atoms[7].cip_code.as_deref(),
        Some("P")
    );
    Ok(())
}
