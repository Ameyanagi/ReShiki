//! Direct stage fixtures for the pinned FusedRingBuilder branches (MIT OPSIN).
use opsin::{
    ParseOptions,
    build_state::BuildState,
    fused_ring_builder::process_fused_rings,
    graph::{Element, FragmentId},
    parse_tree::{Arena, NodeId},
};

fn skeleton() -> (BuildState, Arena, NodeId) {
    let state = BuildState::new(ParseOptions::strict());
    let mut arena = Arena::default();
    let root = arena.grouping("root");
    (state, arena, root)
}
fn token(arena: &mut Arena, root: NodeId, name: &str, value: &str) -> NodeId {
    let node = arena.token(name, value);
    arena.add_child(root, node);
    node
}
fn group(
    state: &mut BuildState,
    arena: &mut Arena,
    root: NodeId,
    name: &str,
    smiles: &str,
    subtype: &str,
) -> NodeId {
    let node = token(arena, root, "group", name);
    arena[node].add_attribute("value", smiles);
    arena[node].add_attribute("type", "ring");
    arena[node].add_attribute("subType", subtype);
    let fragment = state
        .fragment_manager
        .build_token_smiles(smiles, arena, node, "numeric")
        .unwrap();
    arena[node].fragment = Some(fragment);
    node
}
fn multiplier(arena: &mut Arena, root: NodeId, count: usize) -> NodeId {
    let node = token(arena, root, "multiplier", "di");
    arena[node].add_attribute("value", count.to_string());
    arena[node].add_attribute("type", "basic");
    node
}
fn assert_final_graph(state: &BuildState, fragment: FragmentId, atoms: usize, bonds: usize) {
    let graph = state.graph();
    assert_eq!(graph.fragment(fragment).atoms.len(), atoms);
    assert_eq!(graph.fragment(fragment).bonds.len(), bonds);
    assert_eq!(graph.atoms.iter().filter(|atom| atom.active).count(), atoms);
    assert_eq!(graph.bonds.iter().filter(|bond| bond.active).count(), bonds);
    for &atom in &graph.fragment(fragment).atoms {
        assert!(graph.atom(atom).active);
        assert_eq!(graph.atom(atom).fragment, fragment);
        assert!(!graph.atom(atom).locants.is_empty());
        for &bond in &graph.atom(atom).bonds {
            let edge = graph.bond(bond);
            let other = edge.other_atom(atom).unwrap();
            assert!(edge.active && graph.atom(other).active);
            assert!(
                graph.atom(other).bonds.contains(&bond),
                "one-sided fusion bond survived cleanup"
            );
            assert!(graph.fragment(fragment).bonds.contains(&bond));
        }
    }
}

#[test]
fn implicit_benzo_fusion_reuses_parent_token_and_finalizes_bonds() {
    let (mut state, mut arena, root) = skeleton();
    group(
        &mut state,
        &mut arena,
        root,
        "benzo",
        "c1ccccc1",
        "fusionRing",
    );
    let parent = group(&mut state, &mut arena, root, "furan", "o1cccc1", "ring");
    let fragment = arena[parent].fragment.unwrap();
    process_fused_rings(&mut state, &mut arena, root).unwrap();
    assert_eq!(arena[root].children, vec![parent]);
    assert_final_graph(&state, fragment, 9, 10);
    assert_eq!(
        state
            .graph()
            .fragment(fragment)
            .atoms
            .iter()
            .filter(|&&a| state.graph().atom(a).element == Element::O)
            .count(),
        1
    );
}

#[test]
fn explicit_first_order_heterocycle_fusion() {
    let (mut state, mut arena, root) = skeleton();
    group(
        &mut state,
        &mut arena,
        root,
        "imidazo",
        "[nH]1cncc1",
        "fusionRing",
    );
    token(&mut arena, root, "fusion", "[4,5-d]");
    let parent = group(&mut state, &mut arena, root, "pyridine", "n1ccccc1", "ring");
    let fragment = arena[parent].fragment.unwrap();
    process_fused_rings(&mut state, &mut arena, root).unwrap();
    assert_eq!(arena.value(parent), "imidazo[4,5-d]pyridine");
    assert_eq!(
        arena[parent].attribute("value"),
        Some("imidazo[4,5-d]pyridine")
    );
    assert_final_graph(&state, fragment, 9, 10);
    assert_eq!(
        state
            .graph()
            .fragment(fragment)
            .atoms
            .iter()
            .filter(|&&a| state.graph().atom(a).element == Element::N)
            .count(),
        3
    );
}

#[test]
fn higher_order_prime_locants_connect_three_components() {
    let (mut state, mut arena, root) = skeleton();
    group(
        &mut state,
        &mut arena,
        root,
        "cyclopenta",
        "c1cccc1",
        "fusionRing",
    );
    token(&mut arena, root, "fusion", "[1',2':3,4]");
    group(
        &mut state,
        &mut arena,
        root,
        "cyclopenta",
        "c1cccc1",
        "fusionRing",
    );
    token(&mut arena, root, "fusion", "[1,2-b]");
    let parent = group(&mut state, &mut arena, root, "benzene", "c1ccccc1", "ring");
    let fragment = arena[parent].fragment.unwrap();
    process_fused_rings(&mut state, &mut arena, root).unwrap();
    assert_eq!(arena[root].children, vec![parent]);
    assert_final_graph(&state, fragment, 12, 14);
}

#[test]
fn multiplied_fusor_has_one_descriptor_per_copy() {
    let (mut state, mut arena, root) = skeleton();
    multiplier(&mut arena, root, 2);
    group(
        &mut state,
        &mut arena,
        root,
        "furo",
        "o1cccc1",
        "fusionRing",
    );
    token(&mut arena, root, "fusion", "[2,3-b:2',3'-e]");
    let parent = group(&mut state, &mut arena, root, "benzene", "c1ccccc1", "ring");
    let fragment = arena[parent].fragment.unwrap();
    process_fused_rings(&mut state, &mut arena, root).unwrap();
    assert_eq!(arena[root].children, vec![parent]);
    assert_final_graph(&state, fragment, 12, 14);
    assert_eq!(
        state
            .graph()
            .fragment(fragment)
            .atoms
            .iter()
            .filter(|&&a| state.graph().atom(a).element == Element::O)
            .count(),
        2
    );
}

#[test]
fn multi_parent_primed_letters_fuse_distinct_parent_copies() {
    let (mut state, mut arena, root) = skeleton();
    group(
        &mut state,
        &mut arena,
        root,
        "cyclopenta",
        "c1cccc1",
        "fusionRing",
    );
    token(&mut arena, root, "fusion", "[1,2-b:3,4-b']");
    multiplier(&mut arena, root, 2);
    let parent = group(&mut state, &mut arena, root, "benzene", "c1ccccc1", "ring");
    let fragment = arena[parent].fragment.unwrap();
    process_fused_rings(&mut state, &mut arena, root).unwrap();
    assert_eq!(arena[root].children, vec![parent]);
    assert_final_graph(&state, fragment, 13, 15);
}

#[test]
fn benzo_locant_moves_heteroatom_in_numbered_fragment() {
    let (mut state, mut arena, root) = skeleton();
    token(&mut arena, root, "locant", "2");
    group(
        &mut state,
        &mut arena,
        root,
        "benzo",
        "c1ccccc1",
        "fusionRing",
    );
    let parent = group(&mut state, &mut arena, root, "furan", "o1cccc1", "ring");
    let fragment = arena[parent].fragment.unwrap();
    process_fused_rings(&mut state, &mut arena, root).unwrap();
    assert_eq!(arena[root].children, vec![parent]);
    let oxygen = state.graph().atom_by_locant(fragment, "2").unwrap();
    assert_eq!(state.graph().atom(oxygen).element, Element::O);
    assert_final_graph(&state, fragment, 9, 10);
}

#[test]
fn acyclic_parent_and_mismatched_heteroatoms_retain_source_errors() {
    let (mut state, mut arena, root) = skeleton();
    group(
        &mut state,
        &mut arena,
        root,
        "benzo",
        "c1ccccc1",
        "fusionRing",
    );
    group(&mut state, &mut arena, root, "toluene", "Cc1ccccc1", "ring");
    assert_eq!(
        process_fused_rings(&mut state, &mut arena, root)
            .unwrap_err()
            .to_string(),
        "Inappropriate group used in fusion nomenclature. Only groups composed entirely of atoms in cycles may be used. i.e. not: toluene"
    );
    let (mut state, mut arena, root) = skeleton();
    group(
        &mut state,
        &mut arena,
        root,
        "furo",
        "o1cccc1",
        "fusionRing",
    );
    token(&mut arena, root, "fusion", "[1,2-b]");
    group(&mut state, &mut arena, root, "benzene", "c1ccccc1", "ring");
    assert_eq!(
        process_fused_rings(&mut state, &mut arena, root)
            .unwrap_err()
            .to_string(),
        "Invalid fusion descriptor: Heteroatom placement is ambiguous as it is not present in both components of the fusion"
    );
}

#[test]
fn cyclic_alkane_ene_hint_is_consumed_for_aromatisation() {
    for (unsaturator_value, aromatised) in [(2, true), (1, false)] {
        let (mut state, mut arena, root) = skeleton();
        group(
            &mut state,
            &mut arena,
            root,
            "cyclohexa",
            "C1CCCCC1",
            "fusionRing",
        );
        token(&mut arena, root, "fusion", "[1,2-b]");
        let parent = group(
            &mut state,
            &mut arena,
            root,
            "cyclohex",
            "C1CCCCC1",
            "alkaneStem",
        );
        let fragment = arena[parent].fragment.unwrap();
        let unsaturator = token(
            &mut arena,
            root,
            "unsaturator",
            if aromatised { "ene" } else { "ane" },
        );
        arena[unsaturator].add_attribute("value", unsaturator_value.to_string());
        process_fused_rings(&mut state, &mut arena, root).unwrap();
        assert_eq!(arena[unsaturator].parent.is_none(), aromatised);
        assert_eq!(
            state
                .graph()
                .fragment(fragment)
                .atoms
                .iter()
                .filter(|&&atom| state.graph().atom(atom).spare_valency)
                .count(),
            if aromatised { 6 } else { 0 }
        );
        assert_final_graph(&state, fragment, 10, 11);
    }
}

#[test]
fn partially_unsaturated_hw_parent_consumes_its_specific_ene() {
    let (mut state, mut arena, root) = skeleton();
    group(
        &mut state,
        &mut arena,
        root,
        "cyclohexa",
        "C1CCCCC1",
        "fusionRing",
    );
    token(&mut arena, root, "fusion", "[1,2-d]");
    let parent = group(
        &mut state,
        &mut arena,
        root,
        "azepin",
        "N1CCCCCC1",
        "hantzschWidman",
    );
    arena[parent].add_attribute("addBond", "2 defaultLocant 2");
    let fragment = arena[parent].fragment.unwrap();
    let unsaturator = token(&mut arena, root, "unsaturator", "ene");
    arena[unsaturator].add_attribute("value", "2");
    process_fused_rings(&mut state, &mut arena, root).unwrap();
    assert!(arena[unsaturator].parent.is_none());
    assert_eq!(
        state
            .graph()
            .fragment(fragment)
            .atoms
            .iter()
            .filter(|&&atom| state.graph().atom(atom).spare_valency)
            .count(),
        2
    );
    assert_final_graph(&state, fragment, 11, 12);
}
