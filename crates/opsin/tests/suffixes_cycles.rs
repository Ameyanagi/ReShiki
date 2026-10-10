// Source-grounded OPSIN SuffixApplier.processCycleFormingSuffix fixtures.
use opsin::{
    ParseOptions,
    build_state::BuildState,
    graph::{AtomId, Element, FragmentId, StereoReference},
    parse_tree::{Arena, NodeId},
    suffix_applier::SuffixApplier,
    suffix_rules::SuffixRules,
};

fn setup(
    smiles: &str,
    kind: &str,
    labels: &str,
) -> (BuildState, Arena, NodeId, FragmentId, NodeId) {
    let mut state = BuildState::new(ParseOptions::default());
    let mut arena = Arena::default();
    let root = arena.grouping("root");
    let group = arena.token("group", "parent");
    arena[group].add_attribute("type", kind);
    arena[group].add_attribute("subType", "alkaneStem");
    arena.add_child(root, group);
    let parent = state
        .fragment_manager
        .build_token_smiles(smiles, &mut arena, group, labels)
        .unwrap();
    arena[group].fragment = Some(parent);
    let suffix = arena.token("suffix", "lactone");
    arena[suffix].add_attribute("type", "suffix");
    arena[suffix].add_attribute("value", "lactone");
    arena[suffix].add_attribute("subType", "cycleformer");
    arena.add_child(root, suffix);
    let suffix_fragment = state
        .fragment_manager
        .build_token_smiles("[*](=O)O[*]", &mut arena, suffix, "none")
        .unwrap();
    arena[suffix].fragment = Some(suffix_fragment);
    (state, arena, group, parent, suffix)
}

fn apply(
    state: &mut BuildState,
    arena: &mut Arena,
    group: NodeId,
    suffix: NodeId,
) -> Result<(), opsin::ParsingError> {
    SuffixApplier::new(state, &SuffixRules::new().unwrap()).resolve_suffixes(
        arena,
        group,
        &[suffix],
    )
}

#[test]
fn cyclic_suffix_defaults_to_ends_of_a_fully_locanted_chain() {
    let (mut state, mut arena, group, parent, suffix) = setup("CCCC", "chain", "numeric");
    apply(&mut state, &mut arena, group, suffix).unwrap();
    assert_eq!(state.fragment_manager.graph.fragment(parent).atoms.len(), 6);
    assert_eq!(
        state.fragment_manager.graph.neighbours(AtomId(0)),
        [AtomId(1), AtomId(5), AtomId(6)]
    );
    assert_eq!(
        state.fragment_manager.graph.neighbours(AtomId(3)),
        [AtomId(2), AtomId(6)]
    );
    assert!(!state.fragment_manager.graph.atom(AtomId(4)).active);
    assert!(!state.fragment_manager.graph.atom(AtomId(7)).active);
    assert!(
        state
            .fragment_manager
            .graph
            .fragment(parent)
            .atoms
            .iter()
            .filter(|&&id| state.fragment_manager.graph.atom(id).element == Element::C)
            .all(|&id| state.fragment_manager.graph.atom(id).in_cycle)
    );
    assert!(!state.fragment_manager.graph.atom(AtomId(5)).in_cycle);
    assert!(state.fragment_manager.graph.atom(AtomId(6)).in_cycle);
}

#[test]
fn cyclic_suffix_accepts_two_locants_one_locant_or_two_global_ids() {
    for (attribute, value) in [("locant", "1,4"), ("locant", "4"), ("locantID", "1,4")] {
        let (mut state, mut arena, group, _, suffix) = setup("CCCC", "chain", "numeric");
        arena[suffix].add_attribute(attribute, value);
        apply(&mut state, &mut arena, group, suffix).unwrap();
        assert!(
            state
                .fragment_manager
                .graph
                .bond_between(AtomId(0), AtomId(6))
                .is_some()
        );
        assert!(
            state
                .fragment_manager
                .graph
                .bond_between(AtomId(3), AtomId(6))
                .is_some()
        );
    }
}

#[test]
fn cyclic_suffix_removes_existing_hydroxy_oxygen_before_forming_ester() {
    let (mut state, mut arena, group, parent, suffix) = setup("CCCCO", "chain", "1/2/3/4/");
    apply(&mut state, &mut arena, group, suffix).unwrap();
    assert!(!state.fragment_manager.graph.atom(AtomId(4)).active);
    assert!(
        state
            .fragment_manager
            .graph
            .bond_between(AtomId(3), AtomId(7))
            .is_some()
    );
    assert_eq!(state.fragment_manager.graph.fragment(parent).atoms.len(), 6);
}

#[test]
fn carbohydrate_lactone_removes_carbonyl_and_hydroxyl_and_retains_parity() {
    let (mut state, mut arena, group, parent, suffix) =
        setup("C(=O)(O)C[C@H](O)C", "carbohydrate", "1///2/3//4");
    arena[suffix].add_attribute("locant", "1,3");
    let before = state
        .fragment_manager
        .graph
        .atom(AtomId(4))
        .parity
        .clone()
        .unwrap();
    apply(&mut state, &mut arena, group, suffix).unwrap();
    assert!(!state.fragment_manager.graph.atom(AtomId(1)).active);
    assert!(!state.fragment_manager.graph.atom(AtomId(2)).active);
    assert!(!state.fragment_manager.graph.atom(AtomId(5)).active);
    let after = state
        .fragment_manager
        .graph
        .atom(AtomId(4))
        .parity
        .as_ref()
        .unwrap();
    assert_eq!(after.parity, before.parity);
    assert!(
        after
            .atom_refs
            .contains(&Some(StereoReference::DeoxyHydrogen))
    );
    assert!(
        state
            .fragment_manager
            .graph
            .fragment(parent)
            .atoms
            .iter()
            .all(|&id| state.fragment_manager.graph.atom(id).active)
    );
}

#[test]
fn carbohydrate_lactone_removal_clears_interfragment_bond_registry() {
    let (mut state, mut arena, group, parent, suffix) = setup("CCC", "carbohydrate", "numeric");
    arena[suffix].add_attribute("locant", "1,3");
    let mut oxygen_fragments = Vec::new();
    let mut removed_bonds = Vec::new();
    for (parent_atom, order) in [(AtomId(0), 2), (AtomId(0), 1), (AtomId(2), 1)] {
        let oxygen_fragment = state
            .fragment_manager
            .build_smiles("O", "suffix", "none")
            .unwrap();
        let oxygen = state.fragment_manager.graph.fragment(oxygen_fragment).atoms[0];
        let bond = state
            .fragment_manager
            .create_bond(parent_atom, oxygen, order)
            .unwrap();
        oxygen_fragments.push(oxygen_fragment);
        removed_bonds.push(bond);
    }
    apply(&mut state, &mut arena, group, suffix).unwrap();
    assert!(
        state
            .fragment_manager
            .inter_fragment_bonds(parent)
            .unwrap()
            .is_empty()
    );
    for oxygen_fragment in oxygen_fragments {
        assert!(
            state
                .fragment_manager
                .inter_fragment_bonds(oxygen_fragment)
                .unwrap()
                .is_empty()
        );
    }
    let unified = state.fragment_manager.unified_fragment();
    let graph = &state.fragment_manager.graph;
    assert!(
        graph
            .fragment(unified)
            .bonds
            .iter()
            .all(|bond| graph.bond(*bond).active)
    );
    assert!(
        removed_bonds
            .iter()
            .all(|bond| !graph.fragment(unified).bonds.contains(bond))
    );
}

#[test]
fn cyclic_suffix_failure_messages_match_upstream() {
    for (attribute, value, expected) in [
        (
            "locant",
            "1,1",
            "cycle forming suffix: lactone attempted to form a cycle involving the same atom twice!",
        ),
        (
            "locant",
            "1,2,3",
            "Incorrect number of locants associated with cycle forming suffix, expected 2 found: 3",
        ),
        (
            "locantID",
            "1",
            "OPSIN bug: Should be exactly 2 locants associated with a cyclic suffix",
        ),
    ] {
        let (mut state, mut arena, group, _, suffix) = setup("CCCC", "chain", "numeric");
        arena[suffix].add_attribute(attribute, value);
        assert_eq!(
            apply(&mut state, &mut arena, group, suffix)
                .unwrap_err()
                .to_string(),
            expected
        );
    }
    let (mut state, mut arena, group, _, suffix) = setup("CC(C)C", "chain", "1/2/4/3");
    assert_eq!(
        apply(&mut state, &mut arena, group, suffix)
            .unwrap_err()
            .to_string(),
        "cycle forming suffix: lactone should be locanted!"
    );
}
