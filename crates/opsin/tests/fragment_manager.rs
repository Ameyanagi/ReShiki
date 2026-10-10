//! Source-grounded FragmentManager tests and cross-fragment mutation invariants.
use opsin::{
    ParseOptions,
    build_state::BuildState,
    fragment_manager::FragmentManager,
    fragment_tools,
    graph::{Element, StereoReference},
    parse_tree::Arena,
};

#[test]
fn upstream_unified_fragment_contains_every_atom_and_inter_fragment_bond() {
    let mut manager = FragmentManager::new();
    let first = manager.build_smiles("CC", "", "none").unwrap();
    let second = manager.build_smiles("CNC", "", "none").unwrap();
    let first_atom = manager.graph.fragment(first).atoms[0];
    let second_atom = manager.graph.fragment(second).atoms[0];
    let connection = manager.create_bond(first_atom, second_atom, 1).unwrap();
    let unified = manager.unified_fragment();
    assert_eq!(manager.graph.fragment(unified).atoms.len(), 5);
    assert_eq!(manager.graph.fragment(unified).bonds.len(), 4);
    assert!(manager.graph.fragment(unified).bonds.contains(&connection));
    assert_eq!(manager.graph.fragment(first).atoms.len(), 2);
    assert_eq!(manager.graph.fragment(second).atoms.len(), 3);
    assert_eq!(manager.graph.atom(first_atom).fragment, unified);
}

#[test]
fn upstream_fused_ring_relabeling() {
    let mut manager = FragmentManager::new();
    let ring = manager
        .build_smiles("C1=CC=CC2=CC=CC=C12", "", "none")
        .unwrap();
    let atoms = manager.graph.fragment(ring).atoms.clone();
    fragment_tools::relabel_locants_as_fused_ring_system(&mut manager.graph, &atoms);
    for (locant, offset) in [("1", 0), ("4a", 4), ("8", 8), ("8a", 9)] {
        assert_eq!(
            manager.graph.atom_by_locant(ring, locant),
            Some(atoms[offset])
        );
    }
    assert_eq!(manager.graph.atom_by_locant(ring, "9"), None);
}

#[test]
fn upstream_urea_copy_allocates_noncolliding_element_primes() {
    let mut manager = FragmentManager::new();
    let urea = manager.build_smiles("NC(=O)N", "", "none").unwrap();
    fragment_tools::assign_element_locants(&mut manager.graph, urea, &[]).unwrap();
    assert!(manager.graph.atom_by_locant(urea, "N").is_some());
    assert!(manager.graph.atom_by_locant(urea, "N'").is_some());
    assert_eq!(manager.graph.atom_by_locant(urea, "N''"), None);
    let copy = manager.copy_and_relabel_fragment(urea, 1).unwrap();
    assert_eq!(manager.graph.fragment(copy).atoms.len(), 4);
    assert_eq!(manager.graph.atom_by_locant(copy, "N"), None);
    assert_eq!(manager.graph.atom_by_locant(copy, "N'"), None);
    assert!(manager.graph.atom_by_locant(copy, "N''").is_some());
    assert!(manager.graph.atom_by_locant(copy, "N'''").is_some());
}

#[test]
fn token_builder_records_metadata_without_overwriting_primary_group_fragment() {
    let mut manager = FragmentManager::new();
    let mut arena = Arena::default();
    let group = arena.token("group", "benz");
    arena[group].add_attribute("type", "standardGroup");
    arena[group].add_attribute("subType", "aryl");
    arena[group].add_attribute("value", "c1ccccc1");
    let original = manager
        .build_token_smiles("c1ccccc1", &mut arena, group, "numeric")
        .unwrap();
    assert_eq!(arena[group].fragment, None);
    arena[group].fragment = Some(original);
    let added = manager
        .build_token_smiles("C", &mut arena, group, "none")
        .unwrap();
    assert_eq!(arena[group].fragment, Some(original));
    assert_eq!(manager.token_for_fragment(added), Some(group));
    assert_eq!(manager.graph.fragment(added).sub_type, "aryl");
}

#[test]
fn incorporation_reclassifies_connections_and_retains_source_views() {
    let mut manager = FragmentManager::new();
    let parent = manager.build_smiles("C", "", "numeric").unwrap();
    let child = manager.build_smiles("O", "", "numeric").unwrap();
    let external = manager.build_smiles("N", "", "numeric").unwrap();
    let p = manager.graph.fragment(parent).atoms[0];
    let c = manager.graph.fragment(child).atoms[0];
    let e = manager.graph.fragment(external).atoms[0];
    let internal = manager.create_bond(p, c, 1).unwrap();
    let remaining = manager.create_bond(c, e, 1).unwrap();
    manager.incorporate_fragment(child, parent).unwrap();
    assert_eq!(manager.inter_fragment_bonds(parent).unwrap(), &[remaining]);
    assert_eq!(
        manager.inter_fragment_bonds(external).unwrap(),
        &[remaining]
    );
    assert!(manager.graph.fragment(parent).bonds.contains(&internal));
    assert!(manager.inter_fragment_bonds(child).is_err());
    assert_eq!(manager.graph.fragment(child).atoms, vec![c]);
    assert_eq!(manager.graph.atom(c).fragment, parent);
    manager.remove_bond(remaining);
    assert!(manager.inter_fragment_bonds(parent).unwrap().is_empty());
    assert!(manager.inter_fragment_bonds(external).unwrap().is_empty());
}

fn group(
    state: &mut BuildState,
    arena: &mut Arena,
    parent: opsin::parse_tree::NodeId,
    smiles: &str,
    value: &str,
) -> opsin::parse_tree::NodeId {
    let group = arena.token("group", value);
    arena[group].add_attribute("type", "standardGroup");
    arena[group].add_attribute("labels", "numeric");
    arena[group].add_attribute("value", smiles);
    arena.add_child(parent, group);
    let fragment = state
        .fragment_manager
        .build_token_smiles(smiles, arena, group, "numeric")
        .unwrap();
    arena[group].fragment = Some(fragment);
    state.xml_suffix_map.insert(group, vec![]);
    group
}

#[test]
fn xml_clone_copies_graphs_suffixes_and_inter_fragment_connections_in_xml_order() {
    let mut state = BuildState::new(ParseOptions::strict());
    let mut arena = Arena::default();
    let bracket = arena.grouping("bracket");
    let first = group(&mut state, &mut arena, bracket, "CO", "methoxy");
    let second = group(&mut state, &mut arena, bracket, "C", "methyl");
    let f = arena[first].fragment.unwrap();
    let s = arena[second].fragment.unwrap();
    let a = state.graph().fragment(f).atoms[1];
    let b = state.graph().fragment(s).atoms[0];
    state.fragment_manager.create_bond(a, b, 1).unwrap();
    let suffix = state
        .fragment_manager
        .build_smiles("=O", "suffix", "none")
        .unwrap();
    state.xml_suffix_map.get_mut(&first).unwrap().push(suffix);
    let copy = state.clone_element(&mut arena, bracket, 1).unwrap();
    let groups = arena.descendants_named(copy, "group");
    let copied_first = arena[groups[0]].fragment.unwrap();
    let copied_second = arena[groups[1]].fragment.unwrap();
    assert_ne!(copied_first, f);
    assert_ne!(copied_second, s);
    assert_eq!(
        state.fragment_manager.token_for_fragment(copied_first),
        Some(groups[0])
    );
    assert_eq!(
        state
            .graph()
            .fragment(copied_first)
            .token_attributes
            .get("value")
            .map(String::as_str),
        Some("CO")
    );
    let copied_bond = state
        .fragment_manager
        .inter_fragment_bonds(copied_first)
        .unwrap()[0];
    assert_eq!(
        state.graph().bond(copied_bond).from,
        state.graph().fragment(copied_first).atoms[1]
    );
    assert_eq!(
        state.graph().bond(copied_bond).to,
        state.graph().fragment(copied_second).atoms[0]
    );
    assert_eq!(state.xml_suffix_map[&groups[0]].len(), 1);
    assert_ne!(state.xml_suffix_map[&groups[0]][0], suffix);
    assert!(state.graph().atom_by_locant(copied_first, "1'").is_some());
}

#[test]
fn xml_clone_rejects_a_connection_leaving_clone_scope() {
    let mut state = BuildState::new(ParseOptions::strict());
    let mut arena = Arena::default();
    let bracket = arena.grouping("bracket");
    let first = group(&mut state, &mut arena, bracket, "C", "methyl");
    let fragment = arena[first].fragment.unwrap();
    let outside = state
        .fragment_manager
        .build_smiles("C", "", "none")
        .unwrap();
    state
        .fragment_manager
        .create_bond(
            state.graph().fragment(fragment).atoms[0],
            state.graph().fragment(outside).atoms[0],
            1,
        )
        .unwrap();
    assert_eq!(
        state
            .clone_element(&mut arena, bracket, 0)
            .unwrap_err()
            .to_string(),
        "An element that was a clone contained a bond that went outside the scope of the cloning"
    );
}

#[test]
fn charged_hetero_replacement_preserves_existing_proton_delta_and_numeric_locants() {
    let mut manager = FragmentManager::new();
    let fragment = manager.build_smiles("C", "", "numeric").unwrap();
    let atom = manager.graph.fragment(fragment).atoms[0];
    manager.graph.add_locant(atom, "C");
    manager
        .graph
        .atom_mut(atom)
        .protons_explicitly_added_or_removed = 2;
    manager.replace_atom_with_smiles(atom, "[NH4+]").unwrap();
    assert_eq!(manager.graph.atom(atom).element, Element::N);
    assert_eq!(manager.graph.atom(atom).charge, 1);
    assert_eq!(
        manager.graph.atom(atom).protons_explicitly_added_or_removed,
        3
    );
    assert_eq!(manager.graph.atom(atom).locants, vec!["1"]);
    manager.replace_atom_with_smiles(atom, "[NH4+]").unwrap();
    assert_eq!(
        manager.graph.atom(atom).protons_explicitly_added_or_removed,
        1
    );
    assert!(manager.replace_atom_with_smiles(atom, "[O-]").is_err());
}

#[test]
fn atom_replacement_remaps_neighbour_parity_and_updates_connection_registry() {
    let mut manager = FragmentManager::new();
    let fragment = manager.build_smiles("N[C@H](F)C", "", "none").unwrap();
    let atoms = manager.graph.fragment(fragment).atoms.clone();
    let old = atoms[0];
    let centre = atoms[1];
    let replacement_fragment = manager.build_smiles("O", "", "none").unwrap();
    let replacement = manager.graph.fragment(replacement_fragment).atoms[0];
    manager
        .replace_atom_preserving_connectivity(old, replacement)
        .unwrap();
    assert!(!manager.graph.atom(old).active);
    assert!(
        manager
            .graph
            .atom(centre)
            .parity
            .as_ref()
            .unwrap()
            .atom_refs
            .contains(&Some(StereoReference::Atom(replacement)))
    );
    assert!(
        !manager
            .graph
            .atom(centre)
            .parity
            .as_ref()
            .unwrap()
            .atom_refs
            .contains(&Some(StereoReference::Atom(old)))
    );
    assert_eq!(manager.inter_fragment_bonds(fragment).unwrap().len(), 1);
}

#[test]
fn same_fragment_atom_replacement_keeps_transferred_locant_mapping() {
    let mut manager = FragmentManager::new();
    let fragment = manager.build_smiles("CC.C", "", "numeric").unwrap();
    let atoms = manager.graph.fragment(fragment).atoms.clone();
    manager.graph.add_locant(atoms[0], "1'");
    manager
        .replace_atom_preserving_connectivity(atoms[0], atoms[2])
        .unwrap();
    assert_eq!(manager.graph.atom_by_locant(fragment, "1"), Some(atoms[2]));
    assert_eq!(manager.graph.atom_by_locant(fragment, "1'"), Some(atoms[2]));
    assert!(manager.graph.bond_between(atoms[1], atoms[2]).is_some());
}

#[test]
fn terminal_oxygen_removal_cleans_cross_fragment_registry_before_unification() {
    let mut manager = FragmentManager::new();
    let carbon = manager.build_smiles("C", "", "none").unwrap();
    let oxygen = manager.build_smiles("O", "suffix", "none").unwrap();
    let carbon_atom = manager.graph.fragment(carbon).atoms[0];
    let oxygen_atom = manager.graph.fragment(oxygen).atoms[0];
    let bond = manager.create_bond(carbon_atom, oxygen_atom, 1).unwrap();
    manager.remove_terminal_oxygen(carbon_atom, 1).unwrap();
    assert!(manager.inter_fragment_bonds(carbon).unwrap().is_empty());
    assert!(manager.inter_fragment_bonds(oxygen).unwrap().is_empty());
    assert!(!manager.graph.bond(bond).active);
    let unified = manager.unified_fragment();
    assert_eq!(manager.graph.fragment(unified).atoms, vec![carbon_atom]);
    assert!(manager.graph.fragment(unified).bonds.is_empty());
}

#[test]
fn unregistering_a_fragment_removes_only_registry_connections() {
    let mut manager = FragmentManager::new();
    let first = manager.build_smiles("C", "", "none").unwrap();
    let second = manager.build_smiles("O", "", "none").unwrap();
    let first_atom = manager.graph.fragment(first).atoms[0];
    let second_atom = manager.graph.fragment(second).atoms[0];
    let bond = manager.create_bond(first_atom, second_atom, 1).unwrap();
    manager.remove_fragment(first).unwrap();
    assert!(manager.inter_fragment_bonds(second).unwrap().is_empty());
    assert!(manager.graph.bond(bond).active);
    assert_eq!(manager.atom_by_id(first_atom), None);
    assert!(manager.atom_by_id(second_atom).is_some());
}
