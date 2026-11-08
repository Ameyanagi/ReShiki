// Copied from crates/chemistry/src/stereo/cumulene/tests.rs at be63a799aa03a83dc6ec74bb294f4d42b89298d3; changed: pass explicit default preparation/import budgets; format affected calls
use super::*;

#[test]
fn maximal_odd_chains_have_distinct_terminal_ligands() -> Result<(), String> {
    for (text, lengths) in [
        ("CC=C=C=CC", vec![3]),
        ("CC=C=C=C=C=CC", vec![5]),
        ("CC=C=C=CC.CC=C=C=CC", vec![3, 3]),
        ("C1=C=C=CCCCC1", vec![3]),
        ("C1=C=C=C=1", vec![]),
        ("CC=CC", vec![]),
        ("CC=C=CC", vec![]),
        ("CC=C=C=C=CC", vec![]),
        ("C=C=C=CC", vec![]),
        ("[H]C=C=C=CC", vec![]),
        ("[H:1]C=C=C=CC", vec![]),
        ("[2H]C=C=C=CC", vec![3]),
        ("[3H]C=C=C=CC", vec![3]),
        ("[2H]C=C=C=C=C=CC", vec![5]),
        ("[2H]C([H])=C=C=CC", vec![3]),
        ("[2H]C([2H])=C=C=CC", vec![]),
        ("CC(C)=C=C=CC", vec![]),
        ("[CH3:1]C([CH3:2])=C=C=CC", vec![]),
        ("[H]N=C=C=CC", vec![3]),
        ("CN=C=C=CC", vec![3]),
        (r"C/C=C/C(=C=C=C/F)/C=C\C", vec![3]),
        (r"C/C=C/C(=C=C=C\F)/C=C\C", vec![3]),
        ("C/C=C/C(/C=C/C)=C=C=C/C=C/C", vec![]),
        ("[CH3:1]/C([CH3:2])=C/C(=C=C=C/F)/C=C(C)C", vec![]),
        ("[CH3:0]/C(C)=C/C(=C=C=C/F)/C=C(C)C", vec![]),
        ("C[C@H](F)C([C@@H](F)C)=C=C=CC", vec![]),
        ("C[C@H](F)C([C@H](F)C)=C=C=CC", vec![3]),
    ] {
        let prepared = crate::smiles::prepare(text, &crate::budget::Budget::default())
            .map_err(|e| e.to_string())?;
        let chains = stereogenic_odd_cumulenes(&prepared.state)?;
        assert_eq!(
            chains.iter().map(|c| c.bonds.len()).collect::<Vec<_>>(),
            lengths,
            "{text}"
        );
        for chain in chains {
            let [(start, first), (end, last)] = chain.ends;
            let mut current = start;
            let mut atoms = vec![start];
            for id in chain.bonds {
                let bond = &prepared.state.graph.bonds[id];
                assert_eq!(bond.order, 2, "{text}");
                assert!(bond.a == current || bond.b == current, "{text}");
                current = if bond.a == current { bond.b } else { bond.a };
                atoms.push(current);
            }
            assert_eq!(current, end, "{text}");
            assert_eq!(atoms[1], first, "{text}");
            assert_eq!(atoms[atoms.len() - 2], last, "{text}");
        }
    }
    Ok(())
}

#[test]
fn map_free_perception_preserves_stored_and_absent_stereo() -> Result<(), String> {
    use crate::kekulize::Direction;

    for (text, chains) in [
        (r"C/C=C/C(=C=C=C/F)/C=C\C", 1),
        ("C/C=C/C(/C=C/C)=C=C=C/C=C/C", 0),
        ("[CH3:1]/C([CH3:2])=C/C(=C=C=C/F)/C=C(C)C", 0),
        // Mapping swaps the preferred control on the left alkene, but these
        // two ligands have the same geometry once maps are ignored.
        ("[CH3:1]/C(CC)=C/C(=C=C=C/F)/C=C(CC)/C", 0),
        (r"[CH3:1]/C(CC)=C/C(=C=C=C/F)/C=C(CC)\C", 1),
    ] {
        let mut state = crate::smiles::prepare(text, &crate::budget::Budget::default())
            .map_err(|e| e.to_string())?
            .state;
        state.metadata.atoms[0].map_present = true;
        state.directions.fill(Direction::None);
        assert_eq!(stereogenic_odd_cumulenes(&state)?.len(), chains, "{text}");
    }

    let mut state = crate::smiles::prepare("C/C=C/C=C/C=C/C", &crate::budget::Budget::default())
        .map_err(|e| e.to_string())?
        .state;
    state.metadata.atoms[0].map_present = true;
    state.metadata.bonds[3].stereo = 0;
    state.metadata.bonds[3].stereo_atoms.clear();
    state.directions.fill(Direction::None);
    let unmapped = without_maps(&state)?;
    assert_eq!(unmapped.metadata.bonds[1].stereo, 3);
    assert_eq!(unmapped.metadata.bonds[3].stereo, 0);
    assert_eq!(unmapped.metadata.bonds[5].stereo, 3);
    Ok(())
}

#[test]
fn canonical_symmetry_distinguishes_e_and_z_ligands() -> Result<(), String> {
    for (text, different) in [
        (r"C/C=C/C(=C=C=C/F)/C=C\C", true),
        (r"C/C=C/C(=C=C=C\F)/C=C\C", true),
        ("C/C=C/C(/C=C/C)=C=C=C/C=C/C", false),
    ] {
        let state = crate::smiles::prepare(text, &crate::budget::Budget::default())
            .map_err(|e| e.to_string())?
            .state;
        // The branching terminal is atom 3; its propenyl ligand roots are 2
        // and the other single-bonded neighbor. Perception has assigned E/Z.
        let roots: Vec<_> = state
            .graph
            .bonds
            .iter()
            .filter(|b| b.order == 1 && (b.a == 3 || b.b == 3))
            .map(|b| if b.a == 3 { b.b } else { b.a })
            .collect();
        assert_eq!(roots.len(), 2);
        for include_chirality in [false, true] {
            let ranks = ranking::rank(
                &state.graph,
                &state.rings.atoms,
                &state.metadata,
                ranking::Options {
                    break_ties: false,
                    include_chirality,
                    include_isotopes: true,
                    include_maps: false,
                    ..ranking::Options::default()
                },
            )?;
            assert_eq!(
                ranks[roots[0]] != ranks[roots[1]],
                include_chirality && different,
                "{text}, include_chirality={include_chirality}"
            );
        }
    }
    Ok(())
}

#[test]
fn lost_directions_follow_retained_and_removed_hydrogens() {
    for (text, dropped) in [
        ("[H]/N=C=C=C/C", true),
        (r"[H]/N=C=C=C\C", true),
        ("[2H]/C([H])=C=C=C/C", true),
        ("[H]/C([2H])=C=C=C/C", true),
        ("[H]/C=C=C=C/C", false),
        ("[1H]/C=C=C=C/C", false),
        (r"[1H]/C=C=C=C\C", false),
        ("[1H]/C([H])=C=C=C/C", false),
        ("[2H]/C([2H])=C=C=C/C", false),
        ("[CH3:1]/C([CH3:2])=C=C=C/C", false),
        ("[H]/N=C=C=CC", false),
        ("[H]N=C=C=C/C", false),
        ("C[C@H](F)/C([C@H](F)C)=C=C=C/[C@H](O)Cl", true),
        ("C[C@H](F)/C([C@H](F)C)=C=C=C/[C@H](O)Cl |r|", true),
    ] {
        let imported = crate::smiles::read(text, &crate::budget::Budget::default()).unwrap();
        assert_eq!(imported.dropped_cumulene_ez, dropped, "{text}");
    }
    let imported =
        crate::smiles::read("[H]/C([2H])=C=C=C/C", &crate::budget::Budget::default()).unwrap();
    assert!(
        imported
            .prepared
            .state
            .graph
            .atoms
            .iter()
            .all(|a| a.atomic_number != 1 || a.isotope != 0),
        "directional ordinary H was removed"
    );
}
