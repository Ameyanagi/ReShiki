//! Native fixtures transcribed from OPSIN 2.9.0 ComponentProcessorTest.java.
//! Copyright Daniel Lowe and OPSIN contributors; MIT.
use opsin::ParseOptions;
use opsin::build_state::BuildState;
use opsin::component_processor::ComponentProcessor;
use opsin::graph::{AtomId, Element, StereoGroupType};
use opsin::parse_tree::{Arena, NodeId};
use opsin::suffix_rules::SuffixRules;

fn group(arena: &mut Arena, parent: NodeId, kind: &str, subtype: &str) -> NodeId {
    let group = arena.token("group", "");
    arena[group].set_attribute("type", kind);
    arena[group].set_attribute("subType", subtype);
    arena.add_child(parent, group);
    group
}
fn subtractive(arena: &mut Arena, parent: NodeId) -> NodeId {
    let prefix = arena.token("subtractivePrefix", "");
    arena[prefix].set_attribute("type", "deoxy");
    arena.add_child(parent, prefix);
    prefix
}
#[test]
fn subtractive_with_no_group_to_attach_to() {
    let mut arena = Arena::default();
    let word = arena.grouping("word");
    let sub = arena.grouping("substituent");
    arena.add_child(word, sub);
    subtractive(&mut arena, sub);
    assert!(
        ComponentProcessor::remove_and_move_to_appropriate_group_if_subtractive_prefix(
            &mut arena, sub
        )
        .is_err()
    );
}
#[test]
fn subtractive_nearest_biochemical_and_rightmost_standard_preferences() {
    for (middle_biochemical, last_biochemical, expected_middle) in [
        (false, true, false),
        (true, false, true),
        (false, false, false),
    ] {
        let mut arena = Arena::default();
        let word = arena.grouping("word");
        let sub = arena.grouping("substituent");
        arena.add_child(word, sub);
        let prefix = subtractive(&mut arena, sub);
        let middle = arena.grouping("substituent");
        arena.add_child(word, middle);
        group(
            &mut arena,
            middle,
            "simpleGroup",
            if middle_biochemical {
                "biochemical"
            } else {
                "simpleGroup"
            },
        );
        let root = arena.grouping("root");
        arena.add_child(word, root);
        group(
            &mut arena,
            root,
            "simpleGroup",
            if last_biochemical {
                "biochemical"
            } else {
                "simpleGroup"
            },
        );
        assert!(
            ComponentProcessor::remove_and_move_to_appropriate_group_if_subtractive_prefix(
                &mut arena, sub
            )
            .unwrap()
        );
        assert_eq!(arena[sub].parent, None);
        assert_eq!(
            arena[prefix].parent,
            Some(if expected_middle { middle } else { root })
        );
        assert_eq!(arena[arena[prefix].parent.unwrap()].children[0], prefix);
    }
}
#[test]
fn subtractive_with_multiplier_and_locants_keeps_source_order() {
    let mut arena = Arena::default();
    let word = arena.grouping("word");
    let sub = arena.grouping("substituent");
    arena.add_child(word, sub);
    let locant = arena.token("locant", "");
    let multiplier = arena.token("multiplier", "");
    arena.add_child(sub, locant);
    arena.add_child(sub, multiplier);
    let prefix = subtractive(&mut arena, sub);
    let root = arena.grouping("root");
    arena.add_child(word, root);
    let group = group(&mut arena, root, "", "biochemical");
    ComponentProcessor::remove_and_move_to_appropriate_group_if_subtractive_prefix(&mut arena, sub)
        .unwrap();
    assert_eq!(arena[root].children, [locant, multiplier, prefix, group]);
    assert_eq!(arena[sub].parent, None);
}
fn stereochemical_group(smiles: &str) -> (BuildState, Arena, NodeId) {
    let mut state = BuildState::new(ParseOptions::default());
    let fragment = state
        .fragment_manager
        .build_smiles(smiles, "", "none")
        .unwrap();
    let mut arena = Arena::default();
    let group = arena.token("group", "");
    arena[group].fragment = Some(fragment);
    (state, arena, group)
}
#[test]
fn dl_amino_acid_l_d_and_racemate_match_upstream() {
    let rules = SuffixRules::new().unwrap();
    for value in ["l", "d", "dl"] {
        let (mut state, arena, group) = stereochemical_group("N[C@@H](C)C");
        let before = state
            .graph()
            .atom(AtomId(1))
            .parity
            .as_ref()
            .unwrap()
            .parity;
        assert!(
            ComponentProcessor::new(&mut state, &rules)
                .apply_dl_stereochemistry_to_amino_acid(&arena, group, value)
                .unwrap()
        );
        let parity = state.graph().atom(AtomId(1)).parity.as_ref().unwrap();
        assert_eq!(parity.parity, if value == "d" { -before } else { before });
        if value == "dl" {
            assert_eq!(parity.stereo_group.kind, StereoGroupType::Racemic);
            assert_eq!(parity.stereo_group.number, 2);
        }
    }
    let (mut state, arena, group) = stereochemical_group("NC(C)C");
    assert!(
        !ComponentProcessor::new(&mut state, &rules)
            .apply_dl_stereochemistry_to_amino_acid(&arena, group, "d")
            .unwrap()
    );
}
#[test]
fn dl_carbohydrate_natural_enantiomer_and_inverted_natural_match_upstream() {
    let rules = SuffixRules::new().unwrap();
    for (value, opposite, inversion) in [
        ("d", false, false),
        ("l", false, true),
        ("d", true, true),
        ("l", true, false),
    ] {
        let (mut state, mut arena, group) = stereochemical_group("N[C@@H](C)C");
        if opposite {
            arena[group].set_attribute("naturalEntIsOpposite", "yes");
        }
        let before = state
            .graph()
            .atom(AtomId(1))
            .parity
            .as_ref()
            .unwrap()
            .parity;
        ComponentProcessor::new(&mut state, &rules)
            .apply_dl_stereochemistry_to_carbohydrate(&arena, group, value)
            .unwrap();
        assert_eq!(
            state
                .graph()
                .atom(AtomId(1))
                .parity
                .as_ref()
                .unwrap()
                .parity,
            if inversion { -before } else { before }
        );
    }
}
#[test]
fn dl_carbohydrate_configurational_prefixes_match_upstream() {
    for (value, expected) in [("d", "r/l/r/r"), ("l", "l/r/l/l"), ("dl", "?/?/?/?")] {
        let mut arena = Arena::default();
        let prefix = arena.token("stereochemistry", "");
        arena[prefix].set_attribute("value", "r/l/r/r");
        ComponentProcessor::apply_dl_stereochemistry_to_carbohydrate_configurational_prefix(
            &mut arena, prefix, value,
        )
        .unwrap();
        assert_eq!(arena[prefix].attribute("value"), Some(expected));
    }
}
#[test]
fn group_metadata_elementary_homology_functional_and_default_ids() {
    let rules = SuffixRules::new().unwrap();
    let mut state = BuildState::new(ParseOptions::default());
    let mut arena = Arena::default();
    let group = arena.token("group", "generic");
    for (key, value) in [
        ("value", "*CCO"),
        ("type", "elementaryAtom"),
        ("subType", ""),
        ("labels", "numeric"),
        ("functionalIDs", "4"),
        ("defaultInID", "2"),
        ("homology", "alkyl"),
    ] {
        arena[group].set_attribute(key, value);
    }
    let fragment = ComponentProcessor::new(&mut state, &rules)
        .resolve_group(&mut arena, group)
        .unwrap();
    let f = state.graph().fragment(fragment);
    assert_eq!(f.default_in_atom, Some(AtomId(1)));
    assert_eq!(f.functional_atoms, [AtomId(3)]);
    assert!(
        f.atoms
            .iter()
            .all(|a| !state.graph().atom(*a).implicit_hydrogen_allowed)
    );
    assert_eq!(
        state
            .graph()
            .atom(AtomId(0))
            .properties
            .homology_group
            .as_deref(),
        Some("alkyl")
    );
    assert_eq!(state.graph().atom(AtomId(3)).element, Element::O);
}
#[test]
fn multiplier_suffix_order_and_locants_are_preserved() {
    let rules = SuffixRules::new().unwrap();
    let mut state = BuildState::new(ParseOptions::default());
    let mut arena = Arena::default();
    let root = arena.grouping("root");
    let locant = arena.token("locant", "1,2,3");
    let multiplier = arena.token("multiplier", "tri");
    arena[multiplier].set_attribute("value", "3");
    let suffix = arena.token("suffix", "ol");
    arena.add_child(root, locant);
    arena.add_child(root, multiplier);
    arena.add_child(root, suffix);
    ComponentProcessor::new(&mut state, &rules)
        .process_multipliers(&mut arena, root)
        .unwrap();
    assert_eq!(arena[root].children.len(), 3);
    assert_eq!(arena[locant].parent, None);
    assert_eq!(arena[multiplier].parent, None);
    assert_eq!(
        arena[root]
            .children
            .iter()
            .map(|n| arena[*n].attribute("locant").unwrap())
            .collect::<Vec<_>>(),
        ["1", "2", "3"]
    );
}
