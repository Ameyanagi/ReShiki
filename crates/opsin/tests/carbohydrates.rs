// Source-derived construction tests for ComponentProcessor.java 1446–1950,
// OPSIN 2.9.0 b91b610af5ab07560fedb20730d7aef46bb2bca0. Resource SMILES and
// labels are copied from carbohydrates.xml. No Java or network is required.

use opsin::ParseOptions;
use opsin::build_state::BuildState;
use opsin::component_processor_carbohydrates::*;
use opsin::graph::{AtomId, Element, FragmentId, Graph, StereoReference};
use opsin::parse_tree::{Arena, NodeId};

const GLUCOSE: &str = "O=C[C@H](O)[C@@H](O)[C@H](O)[C@H](O)CO";
const HEXOSE_LABELS: &str = "/1/2//3//4//5//6/";
const FRUCTOSE: &str = "OCC(=O)[C@@H](O)[C@H](O)[C@H](O)CO";

fn carbohydrate(
    smiles: &str,
    labels: &str,
    stem: &str,
    subtype: &str,
    suffix_id: &str,
) -> (BuildState, Arena, NodeId, NodeId, FragmentId) {
    let mut state = BuildState::new(ParseOptions::default());
    let mut arena = Arena::default();
    let rule = arena.grouping("wordRule");
    arena[rule].add_attribute("wordRule", "simple");
    let root = arena.grouping("root");
    arena.add_child(rule, root);
    let group = arena.token("group", stem);
    arena[group].add_attribute("type", "carbohydrate");
    arena[group].add_attribute("subType", subtype);
    arena[group].add_attribute("suffixAppliesTo", suffix_id);
    arena.add_child(root, group);
    let fragment = state
        .fragment_manager
        .build_token_smiles(smiles, &mut arena, group, labels)
        .unwrap();
    arena[group].fragment = Some(fragment);
    (state, arena, root, group, fragment)
}
fn glucose() -> (BuildState, Arena, NodeId, NodeId, FragmentId) {
    carbohydrate(
        GLUCOSE,
        HEXOSE_LABELS,
        "gluc",
        "carbohydrateStemAldose",
        "2",
    )
}
fn token_after(arena: &mut Arena, root: NodeId, name: &str, value: &str) -> NodeId {
    let token = arena.token(name, value);
    arena[token].add_attribute("value", value);
    arena.add_child(root, token);
    token
}
fn token_before(arena: &mut Arena, group: NodeId, name: &str, value: &str) -> NodeId {
    let token = arena.token(name, value);
    arena[token].add_attribute("value", value);
    arena.insert_before(group, token);
    token
}
fn atom(state: &BuildState, fragment: FragmentId, locant: &str) -> AtomId {
    state.graph().atom_by_locant(fragment, locant).unwrap()
}
fn order(state: &BuildState, left: AtomId, right: AtomId) -> Option<u8> {
    state
        .graph()
        .bond_between(left, right)
        .map(|bond| state.graph().bond(bond).order)
}
fn ring_locants(state: &BuildState, fragment: FragmentId) -> Vec<String> {
    state
        .graph()
        .fragment(fragment)
        .atoms
        .iter()
        .filter_map(|&atom| {
            let atom = state.graph().atom(atom);
            if atom.in_cycle {
                Some(
                    atom.locants
                        .first()
                        .cloned()
                        .unwrap_or_else(|| atom.element.symbol().into()),
                )
            } else {
                None
            }
        })
        .collect()
}

#[test]
fn pyranose_closure_changes_carbonyl_and_marks_the_exact_ring() {
    let (mut state, mut arena, root, group, fragment) = glucose();
    let ring = token_after(&mut arena, root, "carbohydrateRingSize", "6");
    process_carbohydrates(&mut state, &mut arena, root).unwrap();
    assert_eq!(order(&state, AtomId(0), AtomId(1)), Some(1));
    assert_eq!(
        order(
            &state,
            atom(&state, fragment, "1"),
            atom(&state, fragment, "O5")
        ),
        Some(1)
    );
    assert_eq!(
        ring_locants(&state, fragment),
        ["1", "2", "3", "4", "5", "O"]
    );
    assert!(arena[ring].parent.is_none());
    assert!(arena[group].attribute("suffixAppliesTo").is_none());
    assert!(state.graph().atom(AtomId(1)).properties.is_anomeric);
    assert!(state.graph().atom(AtomId(1)).parity.is_none());
    assert_eq!(state.graph().fragment(fragment).atoms.len(), 12);
}

#[test]
fn alpha_and_beta_use_source_reference_order_and_opposite_parities() {
    for (value, expected) in [("alpha", 1), ("beta", -1)] {
        let (mut state, mut arena, root, group, _) = glucose();
        let locant = token_before(&mut arena, group, "locant", value);
        token_after(&mut arena, root, "carbohydrateRingSize", "6");
        process_carbohydrates(&mut state, &mut arena, root).unwrap();
        let parity = state.graph().atom(AtomId(1)).parity.as_ref().unwrap();
        assert_eq!(parity.parity, expected);
        assert_eq!(
            parity.atom_refs,
            [
                Some(StereoReference::Atom(AtomId(2))),
                Some(StereoReference::Atom(AtomId(0))),
                Some(StereoReference::Atom(AtomId(9))),
                Some(StereoReference::ImplicitHydrogen)
            ]
        );
        assert!(arena[locant].parent.is_none());
    }
}

#[test]
fn equivalent_reference_permutations_preserve_alpha_assignment() {
    for swap in [false, true] {
        let (mut state, mut arena, root, group, _) = glucose();
        let reference = state
            .graph_mut()
            .atom_mut(AtomId(8))
            .parity
            .as_mut()
            .unwrap();
        if swap {
            reference.atom_refs.swap(0, 1);
            reference.parity = -reference.parity;
        }
        token_before(&mut arena, group, "locant", "alpha");
        token_after(&mut arena, root, "carbohydrateRingSize", "6");
        process_carbohydrates(&mut state, &mut arena, root).unwrap();
        assert_eq!(
            state
                .graph()
                .atom(AtomId(1))
                .parity
                .as_ref()
                .unwrap()
                .parity,
            1
        );
    }
}

#[test]
fn inversion_of_reference_configuration_inverts_anomer_configuration() {
    let (mut state, mut arena, root, group, _) = glucose();
    let reference = state
        .graph_mut()
        .atom_mut(AtomId(8))
        .parity
        .as_mut()
        .unwrap();
    reference.parity = -reference.parity;
    token_before(&mut arena, group, "locant", "alpha");
    token_after(&mut arena, root, "carbohydrateRingSize", "6");
    process_carbohydrates(&mut state, &mut arena, root).unwrap();
    assert_eq!(
        state
            .graph()
            .atom(AtomId(1))
            .parity
            .as_ref()
            .unwrap()
            .parity,
        -1
    );
}

#[test]
fn unspecified_anomer_locants_are_consumed_without_creating_parity() {
    for value in ["alpha,beta", "beta,alpha"] {
        let (mut state, mut arena, root, group, _) = glucose();
        let locant = token_before(&mut arena, group, "locant", value);
        process_carbohydrates(&mut state, &mut arena, root).unwrap();
        assert!(arena[locant].parent.is_none());
        assert!(state.graph().atom(AtomId(1)).parity.is_none());
        assert!(state.graph().atom(AtomId(1)).properties.is_anomeric);
    }
}

#[test]
fn systematic_l_prefix_flips_the_pending_anomer_parity() {
    for (configuration, expected) in [("r/l/r/r", 1), ("l/r/l/l", -1)] {
        let (mut state, mut arena, root, group, _) = carbohydrate(
            "O=C[C@H](O)[C@H](O)[C@H](O)[C@H](O)CO",
            HEXOSE_LABELS,
            "hex",
            "systematicCarbohydrateStemAldose",
            "2",
        );
        token_before(&mut arena, group, "locant", "alpha");
        let prefix = token_before(&mut arena, group, "stereoChemistry", configuration);
        arena[prefix].add_attribute("type", "carbohydrateConfigurationalPrefix");
        token_after(&mut arena, root, "carbohydrateRingSize", "6");
        process_carbohydrates(&mut state, &mut arena, root).unwrap();
        assert_eq!(
            state
                .graph()
                .atom(AtomId(1))
                .parity
                .as_ref()
                .unwrap()
                .parity,
            expected
        );
        assert!(arena[prefix].parent.is_some());
    }
}

#[test]
fn explicit_ring_locants_can_select_a_reversed_closure() {
    let (mut state, mut arena, root, _, fragment) = glucose();
    token_after(&mut arena, root, "locant", "5,1");
    token_after(&mut arena, root, "carbohydrateRingSize", "6");
    process_carbohydrates(&mut state, &mut arena, root).unwrap();
    assert_eq!(
        order(
            &state,
            atom(&state, fragment, "5"),
            atom(&state, fragment, "O1")
        ),
        Some(1)
    );
    // The explicit first atom is used, so the pre-existing 1-carbonyl remains.
    assert_eq!(order(&state, AtomId(0), AtomId(1)), Some(2));
    assert!(
        state
            .graph()
            .atom(atom(&state, fragment, "5"))
            .properties
            .is_anomeric
    );
}

#[test]
fn explicit_ring_locants_are_validated_before_closure() {
    for (locants, expected) in [
        (
            "1",
            "Expected 2 locants in front of sugar ring size specifier but found: 1",
        ),
        (
            "1,4",
            "Mismatch between ring size: 6 and ring size specified by locants: 5",
        ),
        (
            "one,5",
            "Locants for ring should be numeric but were: one,5",
        ),
    ] {
        let (mut state, mut arena, root, _, _) = glucose();
        token_after(&mut arena, root, "locant", locants);
        token_after(&mut arena, root, "carbohydrateRingSize", "6");
        assert_eq!(
            process_carbohydrates(&mut state, &mut arena, root)
                .unwrap_err()
                .to_string(),
            expected
        );
        assert_eq!(order(&state, AtomId(0), AtomId(1)), Some(2));
    }
}

#[test]
fn impossible_length_and_second_ring_specifier_keep_source_failure_text() {
    let (mut state, mut arena, root, _, _) = glucose();
    token_after(&mut arena, root, "carbohydrateRingSize", "8");
    assert_eq!(
        process_carbohydrates(&mut state, &mut arena, root)
            .unwrap_err()
            .to_string(),
        "Carbohydrate was not an inappropriate length to form a ring of size: 8"
    );
    let (mut state, mut arena, root, _, _) = glucose();
    token_after(&mut arena, root, "carbohydrateRingSize", "6");
    token_after(&mut arena, root, "carbohydrateRingSize", "5");
    assert_eq!(
        process_carbohydrates(&mut state, &mut arena, root)
            .unwrap_err()
            .to_string(),
        "OPSIN bug: Carbohydate cyclised twice!"
    );
}

#[test]
fn implicit_ring_size_uses_the_exact_rib_and_fruct_stem_exceptions() {
    for (smiles, labels, stem, subtype, suffix_id, carbonyl, oxygen) in [
        (
            GLUCOSE,
            HEXOSE_LABELS,
            "gluc",
            "carbohydrateStemAldose",
            "2",
            "1",
            "O5",
        ),
        (
            "O=C[C@H](O)[C@H](O)[C@H](O)CO",
            "/1/2//3//4//5/",
            "rib",
            "carbohydrateStemAldose",
            "2",
            "1",
            "O4",
        ),
        (
            FRUCTOSE,
            HEXOSE_LABELS,
            "fruct",
            "carbohydrateStemKetose",
            "3",
            "2",
            "O5",
        ),
    ] {
        let (mut state, mut arena, root, _, fragment) =
            carbohydrate(smiles, labels, stem, subtype, suffix_id);
        let yl = token_after(&mut arena, root, "suffix", "yl");
        process_carbohydrates(&mut state, &mut arena, root).unwrap();
        assert_eq!(
            order(
                &state,
                atom(&state, fragment, carbonyl),
                atom(&state, fragment, oxygen)
            ),
            Some(1)
        );
        assert_eq!(arena[yl].attribute("locant"), Some(carbonyl));
        assert!(
            arena[root]
                .children
                .iter()
                .all(|&node| arena[node].name != "carbohydrateRingSize")
        );
    }
}

#[test]
fn acid_then_yl_also_indicates_implicit_ring_closure() {
    let (mut state, mut arena, root, _, fragment) = glucose();
    let uron = token_after(&mut arena, root, "suffix", "uronic acid");
    let yl = token_after(&mut arena, root, "suffix", "yl");
    process_carbohydrates(&mut state, &mut arena, root).unwrap();
    assert_eq!(arena[uron].attribute("locant"), Some("6"));
    assert_eq!(arena[yl].attribute("locant"), Some("1"));
    assert_eq!(
        order(
            &state,
            atom(&state, fragment, "1"),
            atom(&state, fragment, "O5")
        ),
        Some(1)
    );
}

#[test]
fn acyclic_sugar_with_no_cyclisation_indicator_stays_acyclic() {
    let (mut state, mut arena, root, _, _) = glucose();
    let itol = token_after(&mut arena, root, "suffix", "itol");
    process_carbohydrates(&mut state, &mut arena, root).unwrap();
    assert_eq!(order(&state, AtomId(0), AtomId(1)), Some(2));
    assert_eq!(arena[itol].attribute("locant"), Some("1"));
    assert!(!state.graph().atom(AtomId(1)).properties.is_anomeric);
}

#[test]
fn ul_suffix_replaces_aldose_but_osul_adds_a_ketone() {
    for (value, expected_carbonyl_order) in [("ulose", 1), ("osulose", 2)] {
        let (mut state, mut arena, root, _, _) = glucose();
        let suffix = token_after(&mut arena, root, "suffix", value);
        process_carbohydrates(&mut state, &mut arena, root).unwrap();
        assert_eq!(
            order(&state, AtomId(0), AtomId(1)),
            Some(expected_carbonyl_order)
        );
        assert_eq!(order(&state, AtomId(2), AtomId(3)), Some(2));
        assert!(state.graph().atom(AtomId(2)).parity.is_none());
        assert!(arena[suffix].parent.is_none());
    }
}

#[test]
fn ulose_moves_subsequent_ring_closure_and_changes_systematic_subtype() {
    let (mut state, mut arena, root, group, fragment) = glucose();
    arena[group].set_attribute("subType", "systematicCarbohydrateStemAldose");
    token_after(&mut arena, root, "suffix", "ulose");
    token_after(&mut arena, root, "carbohydrateRingSize", "5");
    process_carbohydrates(&mut state, &mut arena, root).unwrap();
    assert_eq!(
        arena[group].attribute("subType"),
        Some("systematicCarbohydrateStemKetose")
    );
    assert_eq!(
        order(
            &state,
            atom(&state, fragment, "2"),
            atom(&state, fragment, "O5")
        ),
        Some(1)
    );
    assert!(state.graph().atom(AtomId(2)).properties.is_anomeric);
    assert!(!state.graph().atom(AtomId(1)).properties.is_anomeric);
}

#[test]
fn ul_suffix_accepts_front_locant_or_adjacent_locant() {
    for front in [false, true] {
        let (mut state, mut arena, root, group, _) = glucose();
        let locant = if front {
            token_before(&mut arena, group, "locant", "3")
        } else {
            token_after(&mut arena, root, "locant", "3")
        };
        token_after(&mut arena, root, "suffix", "ulose");
        process_carbohydrates(&mut state, &mut arena, root).unwrap();
        assert_eq!(order(&state, AtomId(4), AtomId(5)), Some(2));
        assert_eq!(order(&state, AtomId(2), AtomId(3)), Some(1));
        assert!(arena[locant].parent.is_none());
    }
}

#[test]
fn ul_multiplier_defaults_to_consecutive_positions_or_uses_matching_locants() {
    for locants in [None, Some("2,4")] {
        let (mut state, mut arena, root, _, _) = glucose();
        let locant = locants.map(|value| token_after(&mut arena, root, "locant", value));
        let multiplier = token_after(&mut arena, root, "multiplier", "2");
        token_after(&mut arena, root, "suffix", "osulose");
        process_carbohydrates(&mut state, &mut arena, root).unwrap();
        assert_eq!(order(&state, AtomId(2), AtomId(3)), Some(2));
        assert_eq!(
            order(&state, AtomId(4), AtomId(5)),
            Some(if locants.is_none() { 2 } else { 1 })
        );
        assert_eq!(
            order(&state, AtomId(6), AtomId(7)),
            Some(if locants.is_some() { 2 } else { 1 })
        );
        assert!(arena[multiplier].parent.is_none());
        assert!(locant.is_none_or(|node| arena[node].parent.is_none()));
    }
}

#[test]
fn ul_suffix_rejects_locant_count_mismatch_and_missing_hydroxy() {
    for (locants, multiplier, expected) in [
        (
            "2,3",
            None,
            "Incorrect number of locants for ul suffix: 2,3",
        ),
        (
            "2",
            Some("2"),
            "Mismatch between locant and multiplier counts (1 and 2):2",
        ),
    ] {
        let (mut state, mut arena, root, _, _) = glucose();
        token_after(&mut arena, root, "locant", locants);
        if let Some(value) = multiplier {
            token_after(&mut arena, root, "multiplier", value);
        }
        token_after(&mut arena, root, "suffix", "ulose");
        assert_eq!(
            process_carbohydrates(&mut state, &mut arena, root)
                .unwrap_err()
                .to_string(),
            expected
        );
    }
    let (mut state, mut arena, root, _, _) = glucose();
    state
        .fragment_manager
        .remove_atom_and_associated_bonds(AtomId(3));
    token_after(&mut arena, root, "suffix", "osulose");
    assert_eq!(
        process_carbohydrates(&mut state, &mut arena, root)
            .unwrap_err()
            .to_string(),
        "Failed to find hydroxy group at position:2"
    );
}

#[test]
fn di_suffixes_replace_the_terminal_hydroxy_and_preserve_group_fragment() {
    for value in ["dialdose", "aric acid", "arate"] {
        let (mut state, mut arena, root, group, fragment) = glucose();
        let suffix = token_after(&mut arena, root, "suffix", value);
        process_carbohydrates(&mut state, &mut arena, root).unwrap();
        assert!(!state.graph().atom(AtomId(11)).active);
        assert_eq!(order(&state, AtomId(10), AtomId(12)), Some(2));
        assert_eq!(arena[group].fragment, Some(fragment));
        assert!(arena[suffix].parent.is_none());
        let expected_functional = if value == "dialdose" {
            vec![]
        } else {
            vec![AtomId(13), AtomId(14)]
        };
        assert_eq!(
            state.graph().fragment(fragment).functional_atoms,
            expected_functional
        );
        assert_eq!(
            state.fragment_manager.overall_charge(),
            if value == "arate" { -2 } else { 0 }
        );
        for functional in expected_functional {
            assert_eq!(
                state
                    .graph()
                    .atom(functional)
                    .protons_explicitly_added_or_removed,
                if value == "arate" { -1 } else { 0 }
            );
        }
    }
}

#[test]
fn di_suffix_removal_updates_manager_registry_for_an_auxiliary_terminal_oxygen() {
    let (mut state, mut arena, root, _, fragment) = glucose();
    state
        .fragment_manager
        .remove_atom_and_associated_bonds(AtomId(11));
    let auxiliary = state
        .fragment_manager
        .build_smiles("O", "carbohydrate", "none")
        .unwrap();
    let oxygen = state.graph().fragment(auxiliary).atoms[0];
    let old_bond = state
        .fragment_manager
        .create_bond(AtomId(10), oxygen, 1)
        .unwrap();
    assert!(
        state
            .fragment_manager
            .inter_fragment_bonds(fragment)
            .unwrap()
            .contains(&old_bond)
    );
    token_after(&mut arena, root, "suffix", "aric acid");
    process_carbohydrates(&mut state, &mut arena, root).unwrap();
    assert!(!state.graph().atom(oxygen).active);
    assert!(
        !state
            .fragment_manager
            .inter_fragment_bonds(fragment)
            .unwrap()
            .contains(&old_bond)
    );
    assert!(
        !state
            .fragment_manager
            .inter_fragment_bonds(auxiliary)
            .unwrap()
            .contains(&old_bond)
    );
    let unified = state.fragment_manager.unified_fragment();
    let final_fragment = state.graph().fragment(unified);
    assert!(!final_fragment.atoms.contains(&oxygen));
    assert!(!final_fragment.bonds.contains(&old_bond));
    opsin::stereo_analyser::analyse(state.graph(), unified).unwrap();
}

#[test]
fn di_suffixes_reject_ketoses_and_placement_after_ring_size() {
    let (mut state, mut arena, root, _, _) = carbohydrate(
        FRUCTOSE,
        HEXOSE_LABELS,
        "fruct",
        "carbohydrateStemKetose",
        "3",
    );
    token_after(&mut arena, root, "suffix", "arate");
    assert_eq!(
        process_carbohydrates(&mut state, &mut arena, root)
            .unwrap_err()
            .to_string(),
        "arate may only be used with aldoses"
    );
    let (mut state, mut arena, root, _, _) = glucose();
    token_after(&mut arena, root, "carbohydrateRingSize", "6");
    token_after(&mut arena, root, "suffix", "dialdose");
    assert_eq!(
        process_carbohydrates(&mut state, &mut arena, root)
            .unwrap_err()
            .to_string(),
        "OPSIN bug: dialdose not expected after carbohydrate cycliser"
    );
}

#[test]
fn glycoside_suffix_is_locanted_but_requires_its_word_rule() {
    for simple in [false, true] {
        let (mut state, mut arena, root, _, _) = glucose();
        if !simple {
            let rule = arena[root].parent.unwrap();
            arena[rule].set_attribute("wordRule", "glycoside");
        }
        token_after(&mut arena, root, "carbohydrateRingSize", "6");
        let suffix = token_after(&mut arena, root, "suffix", "glycoside");
        let result = process_carbohydrates(&mut state, &mut arena, root);
        if simple {
            assert_eq!(
                result.unwrap_err().to_string(),
                "A glycoside requires a space-separated substituent e.g. methyl alpha-D-glucopyranoside"
            );
        } else {
            result.unwrap();
        }
        assert_eq!(arena[suffix].attribute("locant"), Some("1"));
    }
}

#[test]
fn trivial_apiofuranose_suffix_uses_global_one_based_atom_id() {
    let (mut state, mut arena, root, group, _) = carbohydrate(
        "O[C@@H]1[C@H](O)[C@](CO)(O)CO1",
        "/1/2//3/5///4/",
        "apio-alpha-d-furan",
        "apioFuranose",
        "2",
    );
    // Build after an unrelated preceding fragment, so relative atom ID 2 is
    // distinct from the one-based global ID written on the suffix.
    let fragment = state
        .fragment_manager
        .build_token_smiles(
            "O[C@@H]1[C@H](O)[C@](CO)(O)CO1",
            &mut arena,
            group,
            "/1/2//3/5///4/",
        )
        .unwrap();
    arena[group].fragment = Some(fragment);
    let global_id = state.graph().fragment(fragment).atoms[0].0 + 2;
    let suffix = token_after(&mut arena, root, "suffix", "yl");
    process_carbohydrates(&mut state, &mut arena, root).unwrap();
    assert_eq!(
        arena[suffix].attribute("locantID"),
        Some(global_id.to_string().as_str())
    );
    assert!(arena[group].attribute("suffixAppliesTo").is_none());
    assert_eq!(arena[group].fragment, Some(fragment));
    assert!(arena[suffix].attribute("locant").is_none());
}

#[test]
fn group_metadata_and_traversal_failures_match_source_behavior() {
    let (mut state, mut arena, root, group, _) = glucose();
    arena[group].remove_attribute("suffixAppliesTo");
    assert_eq!(
        process_carbohydrates(&mut state, &mut arena, root)
            .unwrap_err()
            .to_string(),
        "OPSIN bug: Missing suffixAppliesTo on: gluc"
    );
    arena[group].add_attribute("suffixAppliesTo", "99");
    assert_eq!(
        process_carbohydrates(&mut state, &mut arena, root)
            .unwrap_err()
            .to_string(),
        "OPSIN bug: 99 did not point to an atom on: gluc"
    );
    let (mut state, mut arena, root, _, _) = glucose();
    token_after(&mut arena, root, "unrelated", "stop");
    let ring = token_after(&mut arena, root, "carbohydrateRingSize", "6");
    process_carbohydrates(&mut state, &mut arena, root).unwrap();
    assert!(arena[ring].parent.is_some());
    assert_eq!(order(&state, AtomId(0), AtomId(1)), Some(2));
}

#[test]
fn reference_selection_uses_highest_numeric_first_locant_with_defined_parity() {
    let (mut state, _, _, _, fragment) = glucose();
    assert_eq!(
        get_anomeric_reference_atom(state.graph(), fragment),
        Some(AtomId(8))
    );
    state.graph_mut().clear_locants(AtomId(8));
    state.graph_mut().add_locant(AtomId(8), "O99");
    state.graph_mut().add_locant(AtomId(8), "99");
    assert_eq!(
        get_anomeric_reference_atom(state.graph(), fragment),
        Some(AtomId(6))
    );
    state.graph_mut().clear_locants(AtomId(8));
    state.graph_mut().add_locant(AtomId(8), "2147483648");
    assert_eq!(
        get_anomeric_reference_atom(state.graph(), fragment),
        Some(AtomId(6))
    );
}

#[test]
fn trivial_glycosyl_reference_preserves_both_dummy_hydrogen_kinds() {
    let mut graph = Graph::default();
    let fragment = graph.add_fragment("carbohydrate");
    let anomer = graph.add_atom(fragment, Element::C);
    let carbon = graph.add_atom(fragment, Element::C);
    let ring_oxygen = graph.add_atom(fragment, Element::O);
    let other_carbon = graph.add_atom(fragment, Element::C);
    graph.add_bond(anomer, carbon, 1).unwrap();
    graph.add_bond(anomer, ring_oxygen, 1).unwrap();
    graph.add_bond(ring_oxygen, other_carbon, 1).unwrap();
    graph.atom_mut(carbon).in_cycle = true;
    graph.add_out_atom(fragment, anomer, 1, true);
    let references = get_deterministic_atom_refs4_for_anomeric_atom(&graph, anomer).unwrap();
    assert_eq!(
        references,
        [
            Some(StereoReference::Atom(carbon)),
            Some(StereoReference::DeoxyHydrogen),
            Some(StereoReference::Atom(ring_oxygen)),
            Some(StereoReference::ImplicitHydrogen)
        ]
    );
    assert_eq!(graph.fragment(fragment).atoms.len(), 4);
    graph.atom_mut(anomer).out_valency = 0;
    assert_eq!(
        get_deterministic_atom_refs4_for_anomeric_atom(&graph, anomer)
            .unwrap_err()
            .to_string(),
        "OPSIN bug: Unexpected number of atoms connected to anomeric atom of carbohydrate"
    );
}

#[test]
fn ketose_anomer_uses_its_exocyclic_carbon_as_fourth_reference() {
    let (mut state, mut arena, root, group, _) = carbohydrate(
        FRUCTOSE,
        HEXOSE_LABELS,
        "fruct",
        "carbohydrateStemKetose",
        "3",
    );
    token_before(&mut arena, group, "locant", "alpha");
    token_after(&mut arena, root, "carbohydrateRingSize", "5");
    process_carbohydrates(&mut state, &mut arena, root).unwrap();
    let parity = state.graph().atom(AtomId(2)).parity.as_ref().unwrap();
    assert_eq!(
        parity.atom_refs,
        [
            Some(StereoReference::Atom(AtomId(4))),
            Some(StereoReference::Atom(AtomId(3))),
            Some(StereoReference::Atom(AtomId(9))),
            Some(StereoReference::Atom(AtomId(1)))
        ]
    );
}

#[test]
fn unsupported_reference_elements_and_oxygen_valence_fail_at_the_source_checks() {
    let (mut state, _, _, _, _) = glucose();
    state.graph_mut().atom_mut(AtomId(9)).element = Element::N;
    assert_eq!(
        get_deterministic_atom_refs4_for_reference_atom(state.graph(), AtomId(8))
            .unwrap_err()
            .to_string(),
        "OPSIN bug: Unexpected atom element type connected to for anomeric reference atom"
    );
    let (mut state, mut arena, root, _, _) = glucose();
    token_after(&mut arena, root, "carbohydrateRingSize", "6");
    process_carbohydrates(&mut state, &mut arena, root).unwrap();
    let bond = state.graph().bond_between(AtomId(0), AtomId(1)).unwrap();
    state.graph_mut().bond_mut(bond).order = 3;
    assert_eq!(
        get_deterministic_atom_refs4_for_anomeric_atom(state.graph(), AtomId(1))
            .unwrap_err()
            .to_string(),
        "OPSIN bug: Unexpected valency on oxygen in carbohydrate"
    );
}

#[test]
fn missing_reference_parity_only_fails_when_a_locant_requires_reference_lookup() {
    let (mut state, mut arena, root, group, fragment) = glucose();
    for atom in state.graph().fragment(fragment).atoms.clone() {
        state.graph_mut().atom_mut(atom).parity = None;
    }
    token_before(&mut arena, group, "locant", "alpha");
    token_after(&mut arena, root, "carbohydrateRingSize", "6");
    assert_eq!(
        process_carbohydrates(&mut state, &mut arena, root)
            .unwrap_err()
            .to_string(),
        "OPSIN bug: Unable to determine anomeric reference atom in: gluc"
    );
}

#[test]
fn reference_missing_required_carbon_or_oxygen_reports_incomplete_reference_array() {
    let (mut state, _, _, _, _) = glucose();
    state.graph_mut().clear_locants(AtomId(6));
    state
        .graph_mut()
        .add_locant(AtomId(6), "not-the-next-lower-locant");
    assert_eq!(
        get_deterministic_atom_refs4_for_reference_atom(state.graph(), AtomId(8))
            .unwrap_err()
            .to_string(),
        "OPSIN bug: Unable to determine atomRefs4 for anomeric reference atom"
    );
    let (mut state, _, _, _, _) = glucose();
    state.graph_mut().atom_mut(AtomId(9)).element = Element::C;
    assert_eq!(
        get_deterministic_atom_refs4_for_reference_atom(state.graph(), AtomId(8))
            .unwrap_err()
            .to_string(),
        "OPSIN bug: Unable to determine atomRefs4 for anomeric reference atom"
    );
}

#[test]
fn precyclised_trivial_sugar_consumes_alpha_and_leaves_existing_topology() {
    let (mut state, mut arena, root, group, fragment) = carbohydrate(
        "O[C@@H]1[C@H](O)[C@@H](O)[C@H](O)[C@@H](CO)O1",
        "/1/2//3//4//5/6//",
        "trivial",
        "trivialCarbohydrate",
        "2",
    );
    let bonds = state.graph().fragment(fragment).bonds.len();
    let alpha = token_before(&mut arena, group, "locant", "alpha");
    process_carbohydrates(&mut state, &mut arena, root).unwrap();
    assert!(arena[alpha].parent.is_none());
    assert_eq!(state.graph().fragment(fragment).bonds.len(), bonds);
    assert!(state.graph().atom(AtomId(1)).properties.is_anomeric);
    assert_eq!(
        state
            .graph()
            .atom(AtomId(1))
            .parity
            .as_ref()
            .unwrap()
            .parity,
        1
    );
}

#[test]
fn anomer_assignment_creates_absolute_parity_as_in_atom_set_atom_parity() {
    let (mut state, mut arena, root, group, _) = glucose();
    // The reference group is not propagated by the upstream alpha/beta helper.
    state
        .graph_mut()
        .atom_mut(AtomId(8))
        .parity
        .as_mut()
        .unwrap()
        .stereo_group
        .kind = opsin::graph::StereoGroupType::Racemic;
    token_before(&mut arena, group, "locant", "alpha");
    token_after(&mut arena, root, "carbohydrateRingSize", "6");
    process_carbohydrates(&mut state, &mut arena, root).unwrap();
    assert_eq!(
        state
            .graph()
            .atom(AtomId(1))
            .parity
            .as_ref()
            .unwrap()
            .stereo_group,
        opsin::graph::StereoGroup::default()
    );
}
