// OPSIN 2.9.0 construction fixtures, source commit
// b91b610af5ab07560fedb20730d7aef46bb2bca0. Expected parity and valency
// values are transcribed from SMILESFragmentBuilderTest.java (MIT).
use opsin::graph::{
    AtomId, BondStereoValue, Element, Graph, StereoGroup, StereoGroupType, StereoReference,
};
use opsin::smiles::{build_fragment, write_semantic_cxsmiles};
use opsin::valence;

fn build(smiles: &str, labels: &str) -> (Graph, opsin::graph::FragmentId) {
    let mut graph = Graph::default();
    let fragment = build_fragment(&mut graph, smiles, "test", labels).unwrap();
    (graph, fragment)
}

#[test]
fn neighbour_order_locants_and_branch_attachments() {
    let (graph, fragment) = build("CCN(CC)CC", "numeric");
    assert_eq!(
        graph.neighbours(AtomId(2)),
        [AtomId(1), AtomId(3), AtomId(5)]
    );
    assert_eq!(graph.atom_by_locant(fragment, "3"), Some(AtomId(2)));
    let (graph, fragment) = build("=C(=O)-", "1,alpha/");
    assert_eq!(
        graph
            .fragment(fragment)
            .out_atoms
            .iter()
            .map(|out| (out.atom, out.valency))
            .collect::<Vec<_>>(),
        [(AtomId(0), 2), (AtomId(0), 1)]
    );
    assert_eq!(graph.atom(AtomId(0)).out_valency, 3);
    assert_eq!(graph.atom(AtomId(0)).locants, ["1", "alpha"]);
}

#[test]
fn aromatic_resource_state_is_preserved_until_kekulization() {
    let (graph, fragment) = build("Nc1[nH]c(=O)c2c(n1)nc[nH]2", "none");
    assert_eq!(
        graph.fragment(fragment).indicated_hydrogens,
        [AtomId(2), AtomId(10)]
    );
    assert!(graph.atom(AtomId(2)).spare_valency);
    assert!(graph.atom(AtomId(2)).in_cycle);
    assert!(!graph.atom(AtomId(0)).in_cycle);
    assert!(
        write_semantic_cxsmiles(&graph, fragment)
            .unwrap_err()
            .to_string()
            .contains("Spare valency")
    );
}

#[test]
fn fused_resource_labels_use_insertion_order_and_bridgeheads() {
    let (graph, _) = build("c1cccc2ccccc12", "fusedRing");
    assert_eq!(
        graph
            .atoms
            .iter()
            .map(|atom| atom.locants[0].as_str())
            .collect::<Vec<_>>(),
        ["1", "2", "3", "4", "4a", "5", "6", "7", "8", "8a"]
    );
}

#[test]
fn bracket_hydrogens_are_valency_hints_not_fixed_counts() {
    let fixtures = [
        ("[OH3+]", 3, Some(1), None, None),
        ("[SH2]", 2, Some(0), None, None),
        ("[SH4]", 4, Some(0), Some(4), None),
        ("[SH6]", 6, Some(0), Some(6), None),
        ("[SH3]", 3, Some(0), Some(3), None),
        ("[SH3+]", 3, Some(1), None, None),
        ("[SH+]", 1, Some(-1), None, None),
        ("[SH3-]", 3, Some(1), None, None),
        ("[SH-]", 1, Some(-1), None, None),
        ("[SH5+]", 5, Some(1), None, Some(4)),
        ("[Li+]", 0, Some(0), None, None),
        ("[SeH?]", 2, Some(0), None, None),
        ("[P|5]", 5, Some(0), None, Some(5)),
    ];
    for (input, valency, proton_delta, minimum, lambda) in fixtures {
        let (graph, _) = build(input, "none");
        let atom = graph.atom(AtomId(0));
        assert_eq!(graph.determine_valency(AtomId(0), true), valency, "{input}");
        assert_eq!(
            Some(atom.protons_explicitly_added_or_removed),
            proton_delta,
            "{input}"
        );
        assert_eq!(atom.minimum_valency, minimum, "{input}");
        assert_eq!(atom.lambda_convention_valency, lambda, "{input}");
    }
    let (graph, fragment) = build("[NaH]", "1");
    assert_eq!(graph.fragment(fragment).atoms.len(), 2);
    assert_eq!(graph.atom(AtomId(1)).element, Element::H);
    assert!(graph.atom(AtomId(1)).locants.is_empty());
}

#[test]
fn stereo_dummy_references_survive_ring_opening_and_lone_pair_order() {
    let (graph, _) = build("[C@@H]231.C2.N1.F3", "none");
    let parity = graph.atom(AtomId(0)).parity.as_ref().unwrap();
    assert_eq!(parity.parity, 1);
    assert_eq!(
        parity.atom_refs,
        [
            Some(StereoReference::ImplicitHydrogen),
            Some(StereoReference::Atom(AtomId(1))),
            Some(StereoReference::Atom(AtomId(3))),
            Some(StereoReference::Atom(AtomId(2))),
        ]
    );
    let (graph, _) = build("C[S@](N)=O", "none");
    let parity = graph.atom(AtomId(1)).parity.as_ref().unwrap();
    assert_eq!(
        parity.atom_refs,
        [0, 1, 2, 3].map(|id| Some(StereoReference::Atom(AtomId(id))))
    );
    assert_eq!(parity.parity, -1);
    let (graph, _) = build("-[C@H](F)Cl", "none");
    let refs = graph.atom(AtomId(0)).parity.as_ref().unwrap().atom_refs;
    assert_eq!(refs[0], Some(StereoReference::DeoxyHydrogen));
    assert_eq!(refs[1], Some(StereoReference::ImplicitHydrogen));
}

#[test]
fn cis_trans_relative_references_are_not_cip_descriptors() {
    for (input, expected) in [
        ("F/C=C/F", BondStereoValue::Trans),
        ("F\\C=C/F", BondStereoValue::Cis),
        ("C(/F)=C/F", BondStereoValue::Cis),
        ("C(\\F)=C/F", BondStereoValue::Trans),
    ] {
        let (graph, fragment) = build(input, "none");
        let stereo = graph
            .fragment(fragment)
            .bonds
            .iter()
            .find_map(|id| graph.bond(*id).stereo.as_ref())
            .unwrap();
        assert_eq!(stereo.value, expected, "{input}");
        assert!(
            graph
                .bonds
                .iter()
                .all(|bond| bond.smiles_direction.is_none())
        );
    }
}

#[test]
fn malformed_resources_roll_back_and_do_not_accept_external_smiles_extensions() {
    let (mut graph, _) = build("CC", "numeric");
    let original = graph.clone();
    for input in [
        "C1CC",
        "C%1CC",
        "C=1CC#1",
        "C(C",
        "C)",
        "[C+-]",
        "[C@](F)Cl",
        "[C@@@](F)(Cl)Br",
        "[C:1]",
        "C:C",
        "[Se|]",
        "[n+2]",
        "C/C=C\\1\\NC1",
    ] {
        assert!(
            build_fragment(&mut graph, input, "", "none").is_err(),
            "{input}"
        );
        assert_eq!(graph, original, "Failed parse must roll back {input}");
    }
}

#[test]
fn fragment_incorporation_preserves_ids_types_and_out_atom_state() {
    let (mut graph, parent) = build("CC", "numeric");
    let child = build_fragment(&mut graph, "-O", "suffix", "none").unwrap();
    let oxygen = graph.fragment(child).atoms[0];
    let out = graph.remove_out_atom(child, 0);
    assert_eq!(out.atom, oxygen);
    assert_eq!(graph.atom(oxygen).out_valency, 0);
    let bond = graph.add_bond(AtomId(1), oxygen, 1).unwrap();
    graph.incorporate_fragment(child, parent).unwrap();
    assert_eq!(oxygen, AtomId(2));
    assert_eq!(graph.atom(oxygen).atom_type, "suffix");
    assert_eq!(graph.atom(oxygen).fragment, parent);
    assert!(graph.fragment(parent).bonds.contains(&bond));
    assert!(!graph.fragment(child).active);
    graph.make_hydrogens_explicit(parent).unwrap();
    assert_eq!(write_semantic_cxsmiles(&graph, parent).unwrap(), "CCO");
}

#[test]
fn out_atom_explicitness_and_valency_checks_match_upstream() {
    let (mut graph, fragment) = build("C", "none");
    graph.add_out_atom(fragment, AtomId(0), 2, false);
    assert_eq!(graph.atom(AtomId(0)).out_valency, 0);
    graph.set_out_atom_explicit(fragment, 0, true);
    assert_eq!(graph.atom(AtomId(0)).out_valency, 2);
    graph.set_out_atom_valency(fragment, 0, 5);
    assert!(!valence::check_valency(&graph, AtomId(0)));
    assert!(valence::check_valency_available_for_bond(
        &graph,
        AtomId(0),
        4
    ));
    graph.remove_out_atom(fragment, 0);
    assert_eq!(graph.atom(AtomId(0)).out_valency, 0);
    assert_eq!(
        valence::possible_valencies(Element::P, -2),
        Some(&[1, 3, 5, 7][..])
    );
    assert_eq!(valence::maximum_valency(Element::Fe, 0), None);
    assert_eq!(valence::default_valency(Element::Sn), Some(4));
    assert_eq!(
        valence::possible_valencies(Element::Sn, 0),
        Some(&[2, 4][..])
    );
}

#[test]
fn semantic_export_retains_enhanced_stereo_labels_polymers_and_position_variation() {
    let (mut graph, fragment) = build("N[C@@H](F)C", "numeric");
    graph
        .atom_mut(AtomId(1))
        .parity
        .as_mut()
        .unwrap()
        .stereo_group = StereoGroup {
        kind: StereoGroupType::Racemic,
        number: 1,
    };
    graph.make_hydrogens_explicit(fragment).unwrap();
    assert_eq!(
        write_semantic_cxsmiles(&graph, fragment).unwrap(),
        "N[C@@H](F)C |r|"
    );
    let (mut graph, fragment) = build("*CC*", "none");
    graph.fragment_mut(fragment).polymer_attachment_points = Some(vec![AtomId(0), AtomId(3)]);
    graph.make_hydrogens_explicit(fragment).unwrap();
    assert_eq!(
        write_semantic_cxsmiles(&graph, fragment).unwrap(),
        "*CC* |$star_e;;;star_e$,Sg:n:1,2::ht|"
    );
    let (mut graph, fragment) = build("*CC", "none");
    graph.atom_mut(AtomId(0)).properties.position_variation_bond = Some(vec![AtomId(1), AtomId(2)]);
    graph.make_hydrogens_explicit(fragment).unwrap();
    assert_eq!(
        write_semantic_cxsmiles(&graph, fragment).unwrap(),
        "*CC |$_AP1$,m:0:1.2|"
    );
}

#[test]
fn all_element_symbols_have_exact_atomic_numbers() {
    assert_eq!(Element::R.atomic_number(), 0);
    assert_eq!(Element::C.atomic_number(), 6);
    assert_eq!(Element::Og.atomic_number(), 118);
    assert_eq!(Element::from_symbol("Xx"), None);
}

#[test]
fn stereo_substitution_resolves_deoxy_and_implicit_hydrogen_with_source_cip() {
    let (mut graph, fragment) = build("-[C@](F)(Cl)Br", "none");
    graph.remove_out_atom(fragment, 0);
    let oxygen = graph.add_atom(fragment, Element::O);
    graph.add_bond(AtomId(0), oxygen, 1).unwrap();
    graph.atom_mut(AtomId(0)).implicit_hydrogen_allowed = false;
    graph.make_hydrogens_explicit(fragment).unwrap();
    assert_eq!(
        graph.atom(AtomId(0)).parity.as_ref().unwrap().atom_refs[0],
        Some(StereoReference::Atom(oxygen))
    );

    let (mut graph, fragment) = build("-[C@H](F)Cl", "none");
    graph.remove_out_atom(fragment, 0);
    let carbon = graph.add_atom(fragment, Element::C);
    graph.add_bond(AtomId(0), carbon, 1).unwrap();
    graph.make_hydrogens_explicit(fragment).unwrap();
    let refs = graph.atom(AtomId(0)).parity.as_ref().unwrap().atom_refs;
    assert_eq!(refs[0], Some(StereoReference::Atom(carbon)));
    let Some(StereoReference::Atom(hydrogen)) = refs[1] else {
        panic!("implicit hydrogen was not resolved")
    };
    assert_eq!(graph.atom(hydrogen).element, Element::H);
}

#[test]
fn export_rejects_invalid_stereo_references_without_losing_them() {
    let (mut graph, fragment) = build("N[C@@H](F)C", "none");
    graph.make_hydrogens_explicit(fragment).unwrap();
    graph.atom_mut(AtomId(1)).parity.as_mut().unwrap().atom_refs[0] =
        Some(StereoReference::Atom(AtomId(999)));
    assert!(
        write_semantic_cxsmiles(&graph, fragment)
            .unwrap_err()
            .to_string()
            .contains("missing atom")
    );
    assert_eq!(valence::pauling_electronegativity(Element::C), Some(2.55));
    assert_eq!(valence::pauling_electronegativity(Element::Og), None);
    assert_eq!(valence::hw_priority(Element::O), Some(19));
}

#[test]
fn finalized_radicals_preserve_missing_valence_and_wildcard_options() {
    for (resource, expected) in [("-C", "[CH3]"), ("-CC", "[CH2]C"), ("C=", "[CH2]")] {
        let (mut graph, fragment) = build(resource, "none");
        graph.make_hydrogens_explicit(fragment).unwrap();
        let before = graph.clone();
        assert_eq!(
            write_semantic_cxsmiles(&graph, fragment).unwrap(),
            expected,
            "{resource}"
        );
        assert_eq!(graph, before); // Serialization retains the out-atom state.
    }
    let (mut graph, fragment) = build("-CC", "none");
    graph.make_hydrogens_explicit(fragment).unwrap();
    graph
        .convert_out_atoms_to_attachment_atoms(fragment)
        .unwrap();
    assert!(graph.fragment(fragment).out_atoms.is_empty());
    assert_eq!(
        write_semantic_cxsmiles(&graph, fragment).unwrap(),
        "*CC |$_AP1$|"
    );
}
