// Source-grounded ring-assembly, spiro and fused-ring bridge topology fixtures
// for ComponentProcessor.java, OPSIN 2.9.0 b91b610af5ab07560fedb20730d7aef46bb2bca0.
// Uses only Rust graph construction and the ported processing phases.

use opsin::build_state::BuildState;
use opsin::component_processor_rings::{
    determine_elements_to_resolve_into_ring_assembly, process_fused_ring_bridges,
    process_poly_cyclic_spiro_nomenclature, process_ring_assemblies, resolve_features_onto_group,
};
use opsin::graph::{AtomId, Element, FragmentId};
use opsin::parse_tree::{Arena, NodeId};
use opsin::suffix_rules::SuffixRules;
use opsin::{ParseOptions, ParsingError};

fn setup() -> (BuildState, Arena, NodeId) {
    let state = BuildState::new(ParseOptions::default());
    let mut arena = Arena::default();
    let root = arena.grouping("root");
    (state, arena, root)
}
fn token(
    arena: &mut Arena,
    root: NodeId,
    tag: &str,
    text: &str,
    attributes: &[(&str, &str)],
) -> NodeId {
    let node = arena.token(tag, text);
    for &(key, value) in attributes {
        arena[node].add_attribute(key, value);
    }
    arena.add_child(root, node);
    node
}
fn group(
    state: &mut BuildState,
    arena: &mut Arena,
    root: NodeId,
    name: &str,
    smiles: &str,
) -> (NodeId, FragmentId) {
    let node = token(arena, root, "group", name, &[("type", "ring")]);
    let ring = state
        .fragment_manager
        .build_token_smiles(smiles, arena, node, "numeric")
        .unwrap();
    arena[node].fragment = Some(ring);
    state.xml_suffix_map.insert(node, Vec::new());
    (node, ring)
}
fn atom(state: &BuildState, ring: FragmentId, locant: &str) -> AtomId {
    state.graph().atom_by_locant(ring, locant).unwrap()
}
fn run_assembly(
    state: &mut BuildState,
    arena: &mut Arena,
    root: NodeId,
) -> Result<(), ParsingError> {
    process_ring_assemblies(state, arena, &SuffixRules::new().unwrap(), root)
}
fn run_spiro(state: &mut BuildState, arena: &mut Arena, root: NodeId) -> Result<(), ParsingError> {
    process_poly_cyclic_spiro_nomenclature(state, arena, &SuffixRules::new().unwrap(), root)
}
fn bonded(state: &BuildState, ring: FragmentId, first: &str, second: &str) -> bool {
    state
        .graph()
        .bond_between(atom(state, ring, first), atom(state, ring, second))
        .is_some()
}

#[test]
fn locanted_bipyridine_allows_missing_prime_on_second_locant() {
    for locants in ["2,2'", "2,2"] {
        let (mut state, mut arena, root) = setup();
        let locant = token(&mut arena, root, "locant", locants, &[]);
        let multiplier = token(
            &mut arena,
            root,
            "ringAssemblyMultiplier",
            "bi",
            &[("value", "2")],
        );
        let (group, ring) = group(&mut state, &mut arena, root, "pyridine", "n1ccccc1");
        run_assembly(&mut state, &mut arena, root).unwrap();
        assert_eq!(state.graph().fragment(ring).atoms.len(), 12);
        assert_eq!(state.graph().fragment(ring).bonds.len(), 13);
        assert!(bonded(&state, ring, "2", "2'"));
        assert_eq!(arena.value(group), "bipyridine");
        assert!(arena[locant].parent.is_none() && arena[multiplier].parent.is_none());
    }
}

#[test]
fn terphenyl_explicit_connections_support_colon_or_semicolon() {
    for locants in ["1,1':4',1''-", "1,1';4',1''-"] {
        let (mut state, mut arena, root) = setup();
        token(
            &mut arena,
            root,
            "colonOrSemiColonDelimitedLocant",
            locants,
            &[],
        );
        token(
            &mut arena,
            root,
            "ringAssemblyMultiplier",
            "ter",
            &[("value", "3")],
        );
        let (_, ring) = group(&mut state, &mut arena, root, "phenyl", "c1ccccc1");
        run_assembly(&mut state, &mut arena, root).unwrap();
        assert_eq!(state.graph().fragment(ring).atoms.len(), 18);
        assert_eq!(state.graph().fragment(ring).bonds.len(), 20);
        assert!(bonded(&state, ring, "1", "1'"));
        assert!(bonded(&state, ring, "4'", "1''"));
    }
}

#[test]
fn ortho_meta_para_ring_assembly_builds_sequential_primed_components() {
    let (mut state, mut arena, root) = setup();
    token(
        &mut arena,
        root,
        "locant",
        "3",
        &[("type", "orthoMetaPara")],
    );
    token(
        &mut arena,
        root,
        "ringAssemblyMultiplier",
        "quater",
        &[("value", "4")],
    );
    let (_, ring) = group(&mut state, &mut arena, root, "phenyl", "c1ccccc1");
    run_assembly(&mut state, &mut arena, root).unwrap();
    assert!(bonded(&state, ring, "1", "1'"));
    assert!(bonded(&state, ring, "3'", "1''"));
    assert!(bonded(&state, ring, "3''", "1'''"));
}

#[test]
fn single_preceding_locant_stays_available_for_later_substitution() {
    let (mut state, mut arena, root) = setup();
    let locant = token(&mut arena, root, "locant", "4", &[]);
    token(
        &mut arena,
        root,
        "ringAssemblyMultiplier",
        "bi",
        &[("value", "2")],
    );
    let (_, ring) = group(&mut state, &mut arena, root, "phenyl", "c1ccccc1");
    run_assembly(&mut state, &mut arena, root).unwrap();
    assert_eq!(arena[locant].parent, Some(root));
    assert!(bonded(&state, ring, "1", "1'"));
}

#[test]
fn ring_assembly_rejects_locant_count_disagreement_and_incomplete_pairs() {
    for (locants, message) in [
        (
            "1,1'",
            "Disagreement between number of locants(1,1') and ring assembly multiplier: 3",
        ),
        ("1,1':3'", "missing locant, expected 2 locants: 3'"),
    ] {
        let (mut state, mut arena, root) = setup();
        token(
            &mut arena,
            root,
            "colonOrSemiColonDelimitedLocant",
            locants,
            &[],
        );
        token(
            &mut arena,
            root,
            "ringAssemblyMultiplier",
            "ter",
            &[("value", "3")],
        );
        group(&mut state, &mut arena, root, "phenyl", "c1ccccc1");
        assert_eq!(
            run_assembly(&mut state, &mut arena, root).unwrap_err().0,
            message
        );
    }
}

#[test]
fn unlocanted_ter_ring_assembly_extends_the_most_recent_clone() {
    let (mut state, mut arena, root) = setup();
    token(
        &mut arena,
        root,
        "ringAssemblyMultiplier",
        "ter",
        &[("value", "3")],
    );
    let (_, ring) = group(&mut state, &mut arena, root, "cyclopropane", "C1CC1");
    run_assembly(&mut state, &mut arena, root).unwrap();
    assert!(bonded(&state, ring, "1", "1'"));
    assert!(bonded(&state, ring, "1'", "1''"));
    assert!(!bonded(&state, ring, "1", "1''"));
}

#[test]
fn ring_assembly_uses_radical_suffix_position_and_valency_when_unlocanted_bi() {
    let (mut state, mut arena, root) = setup();
    token(
        &mut arena,
        root,
        "ringAssemblyMultiplier",
        "bi",
        &[("value", "2")],
    );
    let (_, ring) = group(
        &mut state,
        &mut arena,
        root,
        "cyclohexanylidene",
        "C1CCCCC1",
    );
    let original = atom(&state, ring, "3");
    state.graph_mut().add_out_atom(ring, original, 2, true);
    run_assembly(&mut state, &mut arena, root).unwrap();
    let bond = state
        .graph()
        .bond_between(original, atom(&state, ring, "3'"))
        .unwrap();
    assert_eq!(state.graph().bond(bond).order, 2);
    assert!(state.graph().fragment(ring).out_atoms.is_empty());
    assert_eq!(state.graph().atom(original).out_valency, 0);
}

#[test]
fn ring_assembly_rejects_multiple_out_atoms_and_unsubstitutable_components() {
    let (mut state, mut arena, root) = setup();
    token(
        &mut arena,
        root,
        "ringAssemblyMultiplier",
        "bi",
        &[("value", "2")],
    );
    let (_, ring) = group(&mut state, &mut arena, root, "cyclopropane", "C1CC1");
    let first = atom(&state, ring, "1");
    state.graph_mut().add_out_atom(ring, first, 1, true);
    state.graph_mut().add_out_atom(ring, first, 1, true);
    assert_eq!(
        run_assembly(&mut state, &mut arena, root).unwrap_err().0,
        "Ring assembly fragment should have one or no OutAtoms; not more than one!"
    );
    let (mut state, mut arena, root) = setup();
    token(
        &mut arena,
        root,
        "ringAssemblyMultiplier",
        "bi",
        &[("value", "2")],
    );
    group(
        &mut state,
        &mut arena,
        root,
        "perfluorocyclopropane",
        "C1(F)(F)C(F)(F)C1(F)F",
    );
    assert_eq!(
        run_assembly(&mut state, &mut arena, root).unwrap_err().0,
        "Unable to find suitable atom for unlocanted ring assembly construction"
    );
}

#[test]
fn bracketed_ring_assembly_detaches_inner_and_outer_structural_brackets() {
    let (mut state, mut arena, root) = setup();
    let outer_open = token(&mut arena, root, "structuralOpenBracket", "[", &[]);
    token(&mut arena, root, "locant", "2,2'", &[]);
    token(
        &mut arena,
        root,
        "ringAssemblyMultiplier",
        "bi",
        &[("value", "2")],
    );
    let inner_open = token(&mut arena, root, "structuralOpenBracket", "(", &[]);
    let (_, ring) = group(&mut state, &mut arena, root, "cyclohexane", "C1CCCCC1");
    let inner_close = token(&mut arena, root, "structuralCloseBracket", ")", &[]);
    let outer_close = token(&mut arena, root, "structuralCloseBracket", "]", &[]);
    run_assembly(&mut state, &mut arena, root).unwrap();
    assert!(bonded(&state, ring, "2", "2'"));
    for bracket in [outer_open, inner_open, inner_close, outer_close] {
        assert!(arena[bracket].parent.is_none());
    }
}

#[test]
fn ring_assembly_feature_selector_limits_inline_suffix_and_stops_at_locanted_unsaturation() {
    let (mut state, mut arena, root) = setup();
    let multiplier = token(
        &mut arena,
        root,
        "ringAssemblyMultiplier",
        "bi",
        &[("value", "2")],
    );
    let (group, _) = group(&mut state, &mut arena, root, "pyridin", "n1ccccc1");
    let charge = token(&mut arena, root, "suffix", "ium", &[("type", "charge")]);
    let inline = token(
        &mut arena,
        root,
        "suffix",
        "yl",
        &[("type", "inline"), ("locant", "2")],
    );
    let unsaturation = token(&mut arena, root, "unsaturator", "en", &[("value", "2")]);
    let stop = token(
        &mut arena,
        root,
        "unsaturator",
        "en",
        &[("value", "2"), ("locant", "3")],
    );
    let temporary =
        determine_elements_to_resolve_into_ring_assembly(&mut arena, multiplier, 0, 0).unwrap();
    assert_eq!(
        arena[temporary].children,
        [group, charge, inline, unsaturation]
    );
    assert_eq!(arena[stop].parent, Some(root));
}

#[test]
fn ring_assembly_feature_selector_rejects_remaining_inline_suffix_on_root() {
    let (mut state, mut arena, root) = setup();
    let multiplier = token(
        &mut arena,
        root,
        "ringAssemblyMultiplier",
        "bi",
        &[("value", "2")],
    );
    group(&mut state, &mut arena, root, "phenyl", "c1ccccc1");
    token(
        &mut arena,
        root,
        "suffix",
        "yl",
        &[("type", "inline"), ("multiplied", "yes")],
    );
    assert_eq!(
        determine_elements_to_resolve_into_ring_assembly(&mut arena, multiplier, 0, 0)
            .unwrap_err()
            .0,
        "Unexpected radical adding suffix on ring assembly"
    );
}

#[test]
fn spirobi_default_and_missing_prime_merge_one_center_and_preserve_all_locants() {
    for locants in [None, Some("2,2"), Some("3,3'")] {
        let (mut state, mut arena, root) = setup();
        if let Some(locants) = locants {
            token(&mut arena, root, "locant", locants, &[]);
        }
        token(
            &mut arena,
            root,
            "polyCyclicSpiro",
            "spirobi",
            &[("value", "spirobi")],
        );
        let (group, ring) = group(&mut state, &mut arena, root, "cyclohexane", "C1CCCCC1");
        run_spiro(&mut state, &mut arena, root).unwrap();
        let center = match locants {
            None => "1",
            Some("2,2") => "2",
            _ => "3",
        };
        assert_eq!(state.graph().fragment(ring).atoms.len(), 11);
        assert_eq!(state.graph().fragment(ring).bonds.len(), 12);
        assert_eq!(
            atom(&state, ring, center),
            atom(&state, ring, &format!("{center}'"))
        );
        assert_eq!(
            state.graph().neighbours(atom(&state, ring, center)).len(),
            4
        );
        assert_eq!(arena.value(group), "spirobicyclohexane");
    }
}

#[test]
fn spiroter_preserves_high_lambda_valency_and_three_ring_center() {
    let (mut state, mut arena, root) = setup();
    token(&mut arena, root, "locant", "1,1',1''", &[]);
    token(
        &mut arena,
        root,
        "polyCyclicSpiro",
        "spiroter",
        &[("value", "spiroter")],
    );
    let (_, ring) = group(&mut state, &mut arena, root, "phospholane", "P1CCCC1");
    let center = atom(&state, ring, "1");
    state.graph_mut().atom_mut(center).lambda_convention_valency = Some(7);
    run_spiro(&mut state, &mut arena, root).unwrap();
    assert_eq!(state.graph().fragment(ring).atoms.len(), 13);
    assert_eq!(state.graph().neighbours(center).len(), 6);
    assert_eq!(
        state.graph().atom(center).lambda_convention_valency,
        Some(7)
    );
    assert_eq!(center, atom(&state, ring, "1'"));
    assert_eq!(center, atom(&state, ring, "1''"));
}

#[test]
fn dispiroter_creates_two_distinct_centers() {
    let (mut state, mut arena, root) = setup();
    token(
        &mut arena,
        root,
        "polyCyclicSpiro",
        "1,1':4',1''-dispiroter",
        &[("value", "dispiroter")],
    );
    let (group, ring) = group(&mut state, &mut arena, root, "cyclohexane", "C1CCCCC1");
    run_spiro(&mut state, &mut arena, root).unwrap();
    assert_eq!(state.graph().fragment(ring).atoms.len(), 16);
    assert_eq!(state.graph().fragment(ring).bonds.len(), 18);
    let first = atom(&state, ring, "1");
    let second = atom(&state, ring, "4'");
    assert_ne!(first, second);
    assert_eq!(first, atom(&state, ring, "1'"));
    assert_eq!(second, atom(&state, ring, "1''"));
    assert_eq!(state.graph().neighbours(first).len(), 4);
    assert_eq!(state.graph().neighbours(second).len(), 4);
    assert_eq!(arena.value(group), "dispirotercyclohexane");
}

#[test]
fn nonidentical_spiro_fuses_cyclopentane_and_cyclobutane_and_copies_heteroatom_center() {
    let (mut state, mut arena, root) = setup();
    token(
        &mut arena,
        root,
        "polyCyclicSpiro",
        "spiro",
        &[("value", "spiro")],
    );
    let open = token(&mut arena, root, "structuralOpenBracket", "[", &[]);
    let (first_group, first_ring) = group(&mut state, &mut arena, root, "cyclopentane", "C1CCCC1");
    let first = atom(&state, first_ring, "1");
    state.graph_mut().atom_mut(first).element = Element::P;
    let locant = token(&mut arena, root, "spiroLocant", "1,1", &[]);
    let (last_group, ring) = group(&mut state, &mut arena, root, "cyclobutane", "C1CCC1");
    let close = token(&mut arena, root, "structuralCloseBracket", "]", &[]);
    run_spiro(&mut state, &mut arena, root).unwrap();
    assert_eq!(state.graph().fragment(ring).atoms.len(), 8);
    assert_eq!(state.graph().fragment(ring).bonds.len(), 9);
    let center = atom(&state, ring, "1'");
    assert_eq!(center, atom(&state, ring, "1"));
    assert_eq!(state.graph().atom(center).element, Element::P);
    assert_eq!(arena.value(last_group), "spirocyclopentanecyclobutane");
    for detached in [first_group, open, locant, close] {
        assert!(arena[detached].parent.is_none());
    }
}

#[test]
fn nonidentical_spiro_detects_conflicting_heteroatom_centers() {
    let (mut state, mut arena, root) = setup();
    token(
        &mut arena,
        root,
        "polyCyclicSpiro",
        "spiro",
        &[("value", "spiro")],
    );
    token(&mut arena, root, "structuralOpenBracket", "[", &[]);
    group(&mut state, &mut arena, root, "phospholane", "P1CCCC1");
    token(&mut arena, root, "spiroLocant", "1,1'", &[]);
    group(&mut state, &mut arena, root, "arsolane", "[AsH]1CCCC1");
    token(&mut arena, root, "structuralCloseBracket", "]", &[]);
    assert_eq!(
        run_spiro(&mut state, &mut arena, root).unwrap_err().0,
        "Disagreement between which element the spiro atom should be: P and As"
    );
}

#[test]
fn nonidentical_dispiro_removes_matching_multiplier_and_keeps_three_ring_connectivity() {
    let (mut state, mut arena, root) = setup();
    let multiplier = token(&mut arena, root, "multiplier", "di", &[("value", "2")]);
    token(
        &mut arena,
        root,
        "polyCyclicSpiro",
        "spiro",
        &[("value", "spiro")],
    );
    token(&mut arena, root, "structuralOpenBracket", "[", &[]);
    group(&mut state, &mut arena, root, "cyclopropane", "C1CC1");
    token(&mut arena, root, "spiroLocant", "1,1'", &[]);
    group(&mut state, &mut arena, root, "cyclobutane", "C1CCC1");
    token(&mut arena, root, "spiroLocant", "3',1''", &[]);
    let (_, ring) = group(&mut state, &mut arena, root, "cyclopentane", "C1CCCC1");
    token(&mut arena, root, "structuralCloseBracket", "]", &[]);
    run_spiro(&mut state, &mut arena, root).unwrap();
    assert_eq!(state.graph().fragment(ring).atoms.len(), 10);
    assert_eq!(state.graph().fragment(ring).bonds.len(), 12);
    assert!(arena[multiplier].parent.is_none());
    assert_eq!(atom(&state, ring, "1"), atom(&state, ring, "1'"));
    assert_eq!(atom(&state, ring, "3'"), atom(&state, ring, "1''"));
}

#[test]
fn spiro_added_hydrogen_brackets_are_applied_before_priming() {
    let (mut state, mut arena, root) = setup();
    token(
        &mut arena,
        root,
        "polyCyclicSpiro",
        "spiro",
        &[("value", "spiro")],
    );
    token(&mut arena, root, "structuralOpenBracket", "[", &[]);
    let (_, first_ring) = group(&mut state, &mut arena, root, "cyclopentene", "C1CCCC1");
    let first = atom(&state, first_ring, "2");
    state.graph_mut().atom_mut(first).spare_valency = true;
    token(&mut arena, root, "spiroLocant", "1(2H),1'(2'H)", &[]);
    let (_, ring) = group(&mut state, &mut arena, root, "cyclobutene", "C1CCC1");
    let second = atom(&state, ring, "2");
    state.graph_mut().atom_mut(second).spare_valency = true;
    token(&mut arena, root, "structuralCloseBracket", "]", &[]);
    run_spiro(&mut state, &mut arena, root).unwrap();
    assert!(!state.graph().atom(first).spare_valency);
    assert!(!state.graph().atom(second).spare_valency);
    assert!(arena.children_named(root, "addedHydrogen").is_empty());
    assert_eq!(atom(&state, ring, "2'"), second);
}

#[test]
fn old_method_spiro_transfers_charge_and_replaces_center() {
    let (mut state, mut arena, root) = setup();
    let (first_group, first_ring) = group(&mut state, &mut arena, root, "cyclopentane", "C1CCCC1");
    let old = atom(&state, first_ring, "1");
    state.graph_mut().atom_mut(old).charge = 1;
    state
        .graph_mut()
        .atom_mut(old)
        .protons_explicitly_added_or_removed = -1;
    token(&mut arena, root, "locant", "1", &[]);
    token(
        &mut arena,
        root,
        "polyCyclicSpiro",
        "spiro",
        &[("value", "spiroOldMethod")],
    );
    token(&mut arena, root, "locant", "1'", &[]);
    let (last_group, ring) = group(&mut state, &mut arena, root, "cyclobutane", "C1CCC1");
    run_spiro(&mut state, &mut arena, root).unwrap();
    let center = atom(&state, ring, "1'");
    assert_eq!(state.graph().fragment(ring).atoms.len(), 8);
    assert_eq!(state.graph().atom(center).charge, 1);
    assert_eq!(
        state
            .graph()
            .atom(center)
            .protons_explicitly_added_or_removed,
        -1
    );
    assert!(arena[first_group].parent.is_none());
    assert_eq!(arena.value(last_group), "cyclopentanespirocyclobutane");
}

#[test]
fn spiro_descriptor_checks_nested_missing_and_wrong_locant_count() {
    for (kind, locants, message) in [
        (
            "spiroter",
            None,
            "Unable to find locant indicating atoms to form polycyclic spiro system!",
        ),
        (
            "spirobi",
            Some("1,1',1''"),
            "Mismatch between spiro descriptor and number of locants provided",
        ),
        ("unknown", None, "Unsupported spiro system encountered"),
    ] {
        let (mut state, mut arena, root) = setup();
        if let Some(locants) = locants {
            token(&mut arena, root, "locant", locants, &[]);
        }
        token(
            &mut arena,
            root,
            "polyCyclicSpiro",
            kind,
            &[("value", kind)],
        );
        group(&mut state, &mut arena, root, "cyclopentane", "C1CCCC1");
        assert_eq!(
            run_spiro(&mut state, &mut arena, root).unwrap_err().0,
            message
        );
    }
    let (mut state, mut arena, root) = setup();
    token(
        &mut arena,
        root,
        "polyCyclicSpiro",
        "spirobi",
        &[("value", "spirobi")],
    );
    token(
        &mut arena,
        root,
        "polyCyclicSpiro",
        "spirobi",
        &[("value", "spirobi")],
    );
    assert_eq!(
        run_spiro(&mut state, &mut arena, root).unwrap_err().0,
        "Nested polyspiro systems are not supported"
    );
}

#[test]
fn isolated_features_assign_indirect_locants_and_restore_survivors_in_order() {
    let (mut state, mut arena, root) = setup();
    let before = token(&mut arena, root, "hyphen", "-", &[]);
    let locant = token(&mut arena, root, "locant", "3", &[]);
    let (group, ring) = group(&mut state, &mut arena, root, "cyclohexane", "C1CCCCC1");
    let unsaturator = token(&mut arena, root, "unsaturator", "en", &[("value", "2")]);
    let after = token(&mut arena, root, "hyphen", "-", &[]);
    resolve_features_onto_group(
        &mut state,
        &mut arena,
        &SuffixRules::new().unwrap(),
        &[locant, group, unsaturator],
    )
    .unwrap();
    assert_eq!(arena[root].children, [before, group, after]);
    let bond = state
        .graph()
        .bond_between(atom(&state, ring, "3"), atom(&state, ring, "4"))
        .unwrap();
    assert_eq!(state.graph().bond(bond).order, 2);
    assert!(arena[locant].parent.is_none() && arena[unsaturator].parent.is_none());
}

#[test]
fn locanted_bridge_atom_numbers_start_at_higher_locanted_bridgehead() {
    for (locants, high_locant, low_locant) in [("4,7", "7", "4"), ("7,4", "7", "4")] {
        let (mut state, mut arena, root) = setup();
        token(&mut arena, root, "locant", locants, &[]);
        token(
            &mut arena,
            root,
            "fusedRingBridge",
            "ethano",
            &[("value", "-CC-")],
        );
        let (_, ring) = group(&mut state, &mut arena, root, "cyclooctane", "C1CCCCCCC1");
        process_fused_ring_bridges(&mut state, &mut arena, root).unwrap();
        assert_eq!(state.graph().fragment(ring).atoms.len(), 10);
        assert_eq!(state.graph().fragment(ring).bonds.len(), 11);
        assert!(bonded(&state, ring, "9", high_locant));
        assert!(bonded(&state, ring, "10", low_locant));
        assert!(bonded(&state, ring, "9", "10"));
        assert!(state.graph().fragment(ring).out_atoms.is_empty());
    }
}

#[test]
fn bridge_numbering_sorts_by_highest_bridgehead_before_assigning_numbers() {
    let (mut state, mut arena, root) = setup();
    token(&mut arena, root, "locant", "2,3", &[]);
    token(
        &mut arena,
        root,
        "fusedRingBridge",
        "ethano",
        &[("value", "-CC-")],
    );
    token(&mut arena, root, "locant", "4,7", &[]);
    token(
        &mut arena,
        root,
        "fusedRingBridge",
        "methano",
        &[("value", "-C-")],
    );
    let (_, ring) = group(&mut state, &mut arena, root, "cyclooctane", "C1CCCCCCC1");
    process_fused_ring_bridges(&mut state, &mut arena, root).unwrap();
    assert!(bonded(&state, ring, "9", "4") && bonded(&state, ring, "9", "7"));
    assert!(bonded(&state, ring, "10", "3"));
    assert!(bonded(&state, ring, "11", "2"));
}

#[test]
fn multiplied_bridges_use_per_instance_locants_and_remove_prefix_tokens() {
    let (mut state, mut arena, root) = setup();
    let locant = token(
        &mut arena,
        root,
        "colonOrSemiColonDelimitedLocant",
        "1,3:4,6-",
        &[],
    );
    let multiplier = token(&mut arena, root, "multiplier", "di", &[("value", "2")]);
    let bridge = token(
        &mut arena,
        root,
        "fusedRingBridge",
        "methano",
        &[("value", "-C-")],
    );
    let (_, ring) = group(&mut state, &mut arena, root, "cyclohexane", "C1CCCCC1");
    process_fused_ring_bridges(&mut state, &mut arena, root).unwrap();
    assert!(bonded(&state, ring, "7", "4") && bonded(&state, ring, "7", "6"));
    assert!(bonded(&state, ring, "8", "1") && bonded(&state, ring, "8", "3"));
    for node in [locant, multiplier, bridge] {
        assert!(arena[node].parent.is_none());
    }
}

#[test]
fn malformed_multiplied_bridge_locants_fail_with_source_messages() {
    for (locants, expected) in [
        (
            "1,3",
            "Mismatch between locant and multiplier counts (1 and 2): 1,3",
        ),
        ("1,3:4", "Expected two locants per bridge, but was: 1,3:4"),
    ] {
        let (mut state, mut arena, root) = setup();
        token(
            &mut arena,
            root,
            "colonOrSemiColonDelimitedLocant",
            locants,
            &[],
        );
        token(&mut arena, root, "multiplier", "di", &[("value", "2")]);
        token(
            &mut arena,
            root,
            "fusedRingBridge",
            "methano",
            &[("value", "-C-")],
        );
        group(&mut state, &mut arena, root, "cyclohexane", "C1CCCCC1");
        assert_eq!(
            process_fused_ring_bridges(&mut state, &mut arena, root)
                .unwrap_err()
                .0,
            expected
        );
    }
}

#[test]
fn unlocanted_bridge_uses_source_atom_selection_and_retains_topology() {
    let (mut state, mut arena, root) = setup();
    token(
        &mut arena,
        root,
        "fusedRingBridge",
        "methano",
        &[("value", "-C-")],
    );
    let (_, ring) = group(&mut state, &mut arena, root, "cyclohexane", "C1CCCCC1");
    process_fused_ring_bridges(&mut state, &mut arena, root).unwrap();
    assert_eq!(state.graph().fragment(ring).atoms.len(), 7);
    assert_eq!(state.graph().fragment(ring).bonds.len(), 8);
    assert!(bonded(&state, ring, "7", "1"));
    assert!(bonded(&state, ring, "7", "2"));
}
