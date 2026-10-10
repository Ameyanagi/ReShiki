//! Source-level foundations from OPSIN 2.9.0, MIT Daniel Lowe/contributors.
//! Pinned source b91b610af5ab07560fedb20730d7aef46bb2bca0: CycleDetectorTest,
//! FragmentTest, FragmentManager.copyAndRelabelFragment, and StereochemistryTest.
use opsin::{
    ambiguity, cycle_detector, fragment_tools as tools,
    graph::{AtomId, BondId, Element, FragmentId, Graph, StereoReference},
    smiles::build_fragment,
    stereo_analyser,
};
fn build(smiles: &str, kind: &str, labels: &str) -> (Graph, FragmentId) {
    let mut graph = Graph::default();
    let fragment = build_fragment(&mut graph, smiles, kind, labels).unwrap();
    (graph, fragment)
}
#[test]
fn source_cycle_membership_and_path_order() {
    for (smiles, acyclic) in [
        ("CCCC", vec![0, 1, 2, 3]),
        ("c1ccccc1", vec![]),
        ("c12.c23.c34.c45.c56.c61", vec![]),
        ("c1ccccc1CCc1ccccc1", vec![6, 7]),
        ("CCc1ccc(O)cc1", vec![0, 1, 6]),
        ("CC1CC(O1)C", vec![0, 5]),
    ] {
        let (mut graph, fragment) = build(smiles, "", "none");
        cycle_detector::assign_cycle_membership(&mut graph, fragment);
        for atom in &graph.fragment(fragment).atoms {
            assert_eq!(
                graph.atom(*atom).in_cycle,
                !acyclic.contains(&atom.0),
                "{smiles}: {atom:?}"
            );
        }
    }
    let (graph, fragment) = build("c1ccccc1", "", "none");
    assert_eq!(
        cycle_detector::paths_between_atoms_using_bonds(
            &graph,
            AtomId(0),
            AtomId(3),
            &graph.fragment(fragment).bonds
        ),
        vec![vec![AtomId(5), AtomId(4)], vec![AtomId(1), AtomId(2)]]
    );
    let (graph, fragment) = build("CCC1CC1", "", "none");
    assert!(!cycle_detector::is_bond_in_cycle(&graph, BondId(0)));
    assert!(cycle_detector::is_bond_in_cycle(
        &graph,
        *graph.fragment(fragment).bonds.last().unwrap()
    ));
}
#[test]
fn incorporated_source_views_keep_atom_bond_and_out_atom_identity() {
    let (mut graph, parent) = build("C", "chain", "numeric");
    let source = build_fragment(&mut graph, "N=", "suffix", "1'").unwrap();
    let source_atom = graph.fragment(source).atoms[0];
    let historical = graph.fragment(source).clone();
    graph.incorporate_fragment(source, parent).unwrap();
    assert!(!graph.fragment(source).active);
    assert_eq!(graph.fragment(source).atoms, historical.atoms);
    assert_eq!(graph.fragment(source).out_atoms, historical.out_atoms);
    assert_eq!(graph.atom(source_atom).fragment, parent);
    graph.set_out_atom_valency(parent, 0, 1);
    assert_eq!(graph.fragment(source).out_atoms[0].valency, 1);
    assert_eq!(graph.atom(source_atom).out_valency, 1);
    graph.set_out_atom_locant(source, 0, Some("N'".into()));
    assert_eq!(
        graph.fragment(parent).out_atoms[0].locant.as_deref(),
        Some("N'")
    );
    graph.set_out_atom_explicit(source, 0, false);
    assert_eq!(graph.atom(source_atom).out_valency, 0);
    assert!(!graph.fragment(parent).out_atoms[0].explicitly_set);
    graph.remove_atom_and_associated_bonds(source_atom);
    assert!(!graph.fragment(parent).atoms.contains(&source_atom));
    assert_eq!(graph.fragment(source).atoms, historical.atoms);
    assert_eq!(graph.atom_by_locant(source, "1'"), Some(source_atom));
}
#[test]
fn full_clone_remaps_stereo_properties_locants_and_independent_sets() {
    let (mut graph, original) = build("[C@@H](F)(Cl)Br", "test", "1/N/N'/N''");
    graph.make_hydrogens_explicit(original).unwrap();
    graph.fragment_mut(original).sub_type = "testSubtype".into();
    graph
        .fragment_mut(original)
        .token_attributes
        .insert("SMILES".into(), "sentinel".into());
    graph
        .fragment_mut(original)
        .functional_atoms
        .push(AtomId(3));
    graph.fragment_mut(original).default_in_atom = Some(AtomId(0));
    graph.add_out_atom(original, AtomId(3), 1, false);
    graph.set_out_atom_locant(original, 0, Some("1".into()));
    graph.atom_mut(AtomId(0)).properties.position_variation_bond = Some(vec![AtomId(1), AtomId(2)]);
    graph.set_ambiguous_element_assignment(&[AtomId(0), AtomId(1)], vec![AtomId(0), AtomId(1)]);
    let copied = graph.copy_and_relabel_fragment(original, 1).unwrap();
    let copied_atoms = graph.fragment(copied).atoms.clone();
    let first = copied_atoms[0];
    let second = copied_atoms[1];
    assert_eq!(graph.fragment(copied).sub_type, "testSubtype");
    assert!(graph.fragment(copied).token_attributes.is_empty());
    assert_eq!(graph.atom_by_locant(copied, "N'''"), Some(second));
    assert_eq!(graph.atom_by_locant(copied, "N''''"), Some(copied_atoms[2]));
    assert_eq!(
        graph.atom_by_locant(copied, "N'''''"),
        Some(copied_atoms[3])
    );
    assert_eq!(graph.atom_by_locant(copied, "1'"), Some(first));
    assert_eq!(graph.fragment(copied).functional_atoms, [copied_atoms[3]]);
    assert_eq!(graph.fragment(copied).default_in_atom, Some(first));
    assert_eq!(graph.fragment(copied).out_atoms[0].atom, copied_atoms[3]);
    assert_ne!(
        graph.fragment(copied).out_atoms[0].id,
        graph.fragment(original).out_atoms[0].id
    );
    assert_eq!(
        graph.fragment(copied).out_atoms[0].locant.as_deref(),
        Some("1'")
    );
    assert_eq!(
        graph.atom(first).properties.position_variation_bond,
        Some(vec![second, copied_atoms[2]])
    );
    let refs = graph.atom(first).parity.as_ref().unwrap().atom_refs;
    assert!(
        refs.into_iter()
            .all(|r| matches!(r,Some(StereoReference::Atom(a)) if copied_atoms.contains(&a)))
    );
    graph.remove_ambiguous_element_assignment_members(first, &[second]);
    assert_eq!(
        graph.atom(first).properties.ambiguous_element_assignment,
        [first]
    );
    assert_eq!(
        graph.atom(second).properties.ambiguous_element_assignment,
        [first, second]
    );
    assert_eq!(
        graph
            .atom(AtomId(0))
            .properties
            .ambiguous_element_assignment,
        [AtomId(0), AtomId(1)]
    );
    graph.remove_ambiguous_element_assignment_members(AtomId(0), &[AtomId(1)]);
    assert_eq!(
        graph
            .atom(AtomId(1))
            .properties
            .ambiguous_element_assignment,
        [AtomId(0)]
    );
}
#[test]
fn clone_bond_stereo_and_failure_are_transactional() {
    let (mut graph, original) = build("C/C=C/C", "", "numeric");
    let copied = graph.copy_and_relabel_fragment(original, 2).unwrap();
    let copied_bond = graph.fragment(copied).bonds[1];
    let stereo = graph.bond(copied_bond).stereo.as_ref().unwrap();
    assert!(
        stereo
            .atom_refs
            .iter()
            .all(|a| graph.atom(*a).fragment == copied)
    );
    assert_eq!(graph.atom_by_locant(copied, "1''"), Some(AtomId(4)));
    graph.atom_mut(AtomId(0)).properties.position_variation_bond = Some(vec![AtomId(999)]);
    let before = graph.clone();
    assert!(graph.copy_and_relabel_fragment(original, 0).is_err());
    assert_eq!(graph, before);
}
#[test]
fn source_element_locants_and_amino_acid_lookup() {
    for (smiles, kind, expected) in [
        (
            "C(N)(=N)N-",
            "nonCarboxylicAcid",
            vec![("N", 3), ("N'", 1), ("N''", 2)],
        ),
        (
            "C(=NN)NN",
            "nonCarboxylicAcid",
            vec![("N", 3), ("N'", 4), ("N''", 1), ("N'''", 2)],
        ),
        ("n1ccccc1", "ring", vec![("N", 0)]),
        ("N1CCNCC1", "ring", vec![("N", 0), ("N'", 3)]),
    ] {
        let (mut graph, fragment) = build(smiles, kind, "none");
        tools::assign_element_locants(&mut graph, fragment, &[]).unwrap();
        for (locant, index) in expected {
            assert_eq!(
                graph.atom_by_locant(fragment, locant),
                Some(AtomId(index)),
                "{smiles}: {locant}"
            );
        }
        if kind == "ring" {
            assert_eq!(graph.atom_by_locant(fragment, "C"), None);
        }
    }
    let (mut graph, parent) = build("C", "acidStem", "none");
    let suffix = build_fragment(&mut graph, "[R](N)=NN", "suffix", "none").unwrap();
    tools::assign_element_locants(&mut graph, parent, &[suffix]).unwrap();
    graph.incorporate_fragment(suffix, parent).unwrap();
    assert_eq!(graph.atom_by_locant(parent, "N"), Some(AtomId(2)));
    assert_eq!(graph.atom_by_locant(parent, "N'"), Some(AtomId(4)));
    assert_eq!(graph.atom_by_locant(parent, "N''"), Some(AtomId(4)));
    let (graph, fragment) = build("CC(NN)(O)C", "chain", "1/2////3");
    assert_eq!(graph.atom_by_locant(fragment, "N2"), Some(AtomId(2)));
    assert_eq!(graph.atom_by_locant(fragment, "N'2"), Some(AtomId(3)));
    assert_eq!(graph.atom_by_locant(fragment, "O2"), Some(AtomId(4)));
    assert_eq!(graph.atom_by_locant(fragment, "N1"), None);
}
#[test]
fn source_characteristic_hydroxy_and_terminal_oxygen_semantics() {
    let (mut graph, fragment) = build("CC(=O)O", "", "none");
    assert_eq!(
        tools::find_hydroxy_like_terminal_atoms(
            &graph,
            &graph.fragment(fragment).atoms,
            Element::O
        ),
        [AtomId(3)]
    );
    assert!(
        tools::find_hydroxy_groups(&graph, fragment)
            .unwrap()
            .is_empty()
    );
    assert!(tools::is_characteristic_atom(&graph, AtomId(3)));
    assert!(!tools::is_characteristic_atom(&graph, AtomId(2)));
    graph
        .fragment_mut(fragment)
        .functional_atoms
        .push(AtomId(2));
    assert!(tools::is_characteristic_atom(&graph, AtomId(2)));
    let (mut graph, fragment) = build("[N+](=O)([O-])C", "", "none");
    tools::remove_terminal_oxygen(&mut graph, AtomId(0), 2).unwrap();
    assert_eq!(
        graph.fragment(fragment).atoms,
        [AtomId(0), AtomId(2), AtomId(3)]
    );
    assert_eq!(graph.atom(AtomId(0)).charge, 1);
    tools::remove_terminal_oxygen(&mut graph, AtomId(0), 2).unwrap();
    assert_eq!(graph.atom(AtomId(0)).charge, 0);
    assert_eq!(graph.atom(AtomId(0)).protons_explicitly_added_or_removed, 0);
    let (mut graph, fragment) = build("[C@](F)(Cl)(O)Br", "", "none");
    tools::remove_terminal_atom(&mut graph, AtomId(3)).unwrap();
    assert_eq!(
        graph.atom(AtomId(0)).parity.as_ref().unwrap().atom_refs[2],
        Some(StereoReference::DeoxyHydrogen)
    );
    assert!(!graph.fragment(fragment).atoms.contains(&AtomId(3)));
}
#[test]
fn source_small_ring_and_equivalent_bond_boundaries() {
    for (smiles, large) in [("C1CCCCC1", false), ("C1CCCCCC1", true), ("CCC", true)] {
        let (graph, fragment) = build(smiles, "", "none");
        assert_eq!(
            tools::not_in_six_member_or_smaller_ring(&graph, graph.fragment(fragment).bonds[0]),
            large,
            "{smiles}"
        );
    }
    let (graph, fragment) = build("CCCC", "", "none");
    assert!(ambiguity::all_bonds_equivalent(&graph, &[BondId(0), BondId(2)]).unwrap());
    assert!(!ambiguity::all_bonds_equivalent(&graph, &graph.fragment(fragment).bonds).unwrap());
}

#[test]
fn source_geometry_resonance_and_tautomer_examples() {
    for (input, expected, geometry) in [
        (r#"C(N)(O)(Cl)Br"#, true, true),
        (r#"[Si](N)(O)(Cl)Br"#, true, true),
        (r#"[Ge](N)(O)(Cl)Br"#, true, true),
        (r#"[N+](N)(O)(Cl)Br"#, true, true),
        (r#"[P+](N)(O)(Cl)Br"#, true, true),
        (r#"[As+](N)(O)(Cl)Br"#, true, true),
        (r#"[B-](N)(O)(Cl)Br"#, true, true),
        (r#"[Sn](N)(O)(Cl)Br"#, true, true),
        (r#"[N](=N)(O)(Cl)Br"#, true, true),
        (r#"[P](=N)(O)(Cl)Br"#, true, true),
        (r#"[S](=N)(=O)(Cl)Br"#, true, true),
        (r#"[S+](=N)(O)(Cl)Br"#, true, true),
        (r#"[S](=O)(Cl)Br"#, true, true),
        (r#"[S+](O)(Cl)Br"#, true, true),
        (r#"N1(C)(OS1)"#, true, true),
        (r#"[Se](=N)(=O)(Cl)Br"#, true, true),
        (r#"[Se+](=N)(O)(Cl)Br"#, true, true),
        (r#"[Se](=O)(Cl)Br"#, true, true),
        (r#"[Se+](O)(Cl)Br"#, true, true),
        (r#"[S](=N)(=O)([O-])Br"#, true, false),
        (r#"[S](=O)([O-])Br"#, true, false),
        (r#"[S](=S)([O-])Br"#, false, false),
        (r#"C(N)([O-])(Cl)Br"#, false, false),
        (r#"[S](=N)(=O)([OH])Br"#, true, false),
        (r#"[S](=O)([OH])Br"#, true, false),
        (r#"[S](=S)([OH])Br"#, false, false),
        (r#"C(N)([OH])(Cl)Br"#, false, false),
        (r#"N([H])(CC)(C)"#, true, false),
        (r#"N1(C)(OS1)"#, false, false),
    ] {
        let (graph, _) = build(input, "", "none");
        let actual = if geometry {
            stereo_analyser::is_known_potentially_stereogenic(&graph, AtomId(0))
        } else {
            stereo_analyser::is_achiral_due_to_resonance_or_tautomerism(&graph, AtomId(0))
        };
        assert_eq!(actual, expected, "{input}");
    }
}

// Frozen source dev oracle with the same pinned Maven provenance as graph_resource_oracle.
#[test]
fn source_odd_spare_valency_and_invalid_pairing_boundaries() {
    for (input, expected) in [
        (
            r#"c(c)(c)c"#,
            r#"ERROR:Failed to assign all double bonds! (Check that indicated hydrogens have been appropriately specified)"#,
        ),
        (r#"c1cc1"#, r#"000/121/111/"#),
        (r#"c1ccc1"#, r#"0000/2121/1111/"#),
        (r#"c.c"#, r#"00//00/"#),
        (r#"c1ccccc1c"#, r#"0000000/2121211/1111110/"#),
        (
            r#"[nH]1cc[nH]c1"#,
            r#"ERROR:Failed to assign all double bonds! (Check that indicated hydrogens have been appropriately specified)"#,
        ),
        (r#"[nH]1nccn1"#, r#"00000/12121/11111/0"#),
        (r#"c12c(ccc1)ccc2"#, r#"00000000/121211212/11111111/"#),
        (r#"c1cCccC1"#, r#"000000/211211/111111/"#),
        (r#"C(=C)C=C"#, r#"0000/212/0000/"#),
        (r#"c1cccc2ccccc12"#, r#"0000000000/21212121211/1111111111/"#),
        (r#"c(c)(c)(c)c"#, r#"00000/1111/00000/"#),
        (r#"[nH]1cc[nH]cc1"#, r#"000000/121121/111111/0,3"#),
        (
            r#"[nH]1cc[nH]ccc1"#,
            r#"ERROR:Failed to assign all double bonds! (Check that indicated hydrogens have been appropriately specified)"#,
        ),
    ] {
        let (mut graph, fragment) = build(input, "", "none");
        let before = graph.clone();
        let result = tools::convert_spare_valencies_to_double_bonds(&mut graph, fragment);
        if let Some(message) = expected.strip_prefix("ERROR:") {
            assert_eq!(result.unwrap_err().0, message, "{input}");
            assert_eq!(graph, before);
        } else {
            result.unwrap();
            let orders: String = graph
                .fragment(fragment)
                .bonds
                .iter()
                .map(|b| char::from(b'0' + graph.bond(*b).order))
                .collect();
            assert_eq!(orders, expected.split('/').nth(1).unwrap(), "{input}");
            assert!(
                graph
                    .fragment(fragment)
                    .atoms
                    .iter()
                    .all(|a| !graph.atom(*a).spare_valency)
            );
        }
    }
    let mut graph = Graph::default();
    assert!(build_fragment(&mut graph, "[bH]1ccccc1", "", "none").is_err());
    assert!(graph.atoms.is_empty());
}

// Frozen dev oracle: pinned OPSIN 2.9.0 Maven jar SHA256
// 627ee5da4af551f9c4d1d766f545eb7cf519a344776e0bb677247a47abac0252.
// FragmentManager.createBond on an existing pair rejects all orders 1..3,
// both orientations, because Bond.equals165 compares unordered endpoints.
#[test]
fn source_normal_bond_creation_rejects_equivalent_endpoint_pairs() {
    const MESSAGE: &str = "Atom already has given bond (This is not allowed as this would give two bonds between the same atoms!)";
    let (mut graph, fragment) = build("CC", "", "none");
    let original = graph.fragment(fragment).bonds[0];
    for reversed in [false, true] {
        let (from, to) = if reversed {
            (AtomId(1), AtomId(0))
        } else {
            (AtomId(0), AtomId(1))
        };
        for order in 1..=3 {
            let before = graph.clone();
            assert_eq!(graph.add_bond(from, to, order).unwrap_err().0, MESSAGE);
            assert_eq!(graph, before);
            assert_eq!(graph.bond_between(from, to), Some(original));
        }
    }
    for (from, to, order) in [
        (AtomId(0), AtomId(0), 1),
        (AtomId(0), AtomId(1), 0),
        (AtomId(0), AtomId(1), 4),
    ] {
        let before = graph.clone();
        assert!(graph.add_bond(from, to, order).is_err());
        assert_eq!(graph, before);
    }
    graph.remove_bond(original);
    let replacement = graph.add_bond(AtomId(1), AtomId(0), 2).unwrap();
    assert_ne!(replacement, original);
    assert_eq!(graph.atom(AtomId(0)).bonds, [replacement]);
    assert_eq!(graph.fragment(fragment).bonds, [replacement]);
    assert_eq!(graph.incoming_valency(AtomId(0)), 2);

    let mut empty = Graph::default();
    assert_eq!(
        build_fragment(&mut empty, "C1C1", "", "none")
            .unwrap_err()
            .0,
        MESSAGE
    );
    assert_eq!(empty, Graph::default());
}

// Frozen source oracle: makeHydrogensExplicit("CO") adds four H atoms with
// implicitHydrogenAllowed=true; build("[TiH2]") adds two with the same default.
#[test]
fn materialized_hydrogens_preserve_source_atom_defaults() {
    let (mut graph, fragment) = build("CO", "", "none");
    graph.make_hydrogens_explicit(fragment).unwrap();
    let hydrogens: Vec<_> = graph
        .fragment(fragment)
        .atoms
        .iter()
        .copied()
        .filter(|atom| graph.atom(*atom).element == Element::H)
        .collect();
    assert_eq!(hydrogens.len(), 4);
    for hydrogen in hydrogens {
        let atom = graph.atom(hydrogen);
        assert!(atom.implicit_hydrogen_allowed);
        assert_eq!(atom.atom_type, "");
        assert_eq!(atom.bonds.len(), 1);
        assert_eq!(atom.properties.smiles_hydrogen_count, None);
        assert_eq!(graph.incoming_valency(hydrogen), 1);
    }
    let before = graph.clone();
    graph.make_hydrogens_explicit(fragment).unwrap();
    assert_eq!(graph, before);

    let (graph, fragment) = build("[TiH2]", "testType", "none");
    let hydrogens: Vec<_> = graph
        .fragment(fragment)
        .atoms
        .iter()
        .filter(|atom| graph.atom(**atom).element == Element::H)
        .collect();
    assert_eq!(hydrogens.len(), 2);
    for hydrogen in hydrogens {
        assert!(graph.atom(*hydrogen).implicit_hydrogen_allowed);
        assert_eq!(graph.atom(*hydrogen).atom_type, "testType");
        assert_eq!(graph.incoming_valency(*hydrogen), 1);
    }
}
