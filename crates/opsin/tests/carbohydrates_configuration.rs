// Port of ComponentProcessorTest carbohydrate D/L cases plus exact source
// branches from ComponentProcessor.java 1364–1439, pinned OPSIN 2.9.0.

use opsin::ParseOptions;
use opsin::build_state::BuildState;
use opsin::component_processor::ComponentProcessor;
use opsin::graph::{AtomId, StereoGroup, StereoGroupType};
use opsin::parse_tree::{Arena, NodeId};
use opsin::suffix_rules::SuffixRules;

fn fixture(smiles: &str) -> (BuildState, Arena, NodeId) {
    let mut state = BuildState::new(ParseOptions::default());
    let mut arena = Arena::default();
    let group = arena.token("group", "carbohydrate");
    arena[group].add_attribute("type", "carbohydrate");
    let fragment = state
        .fragment_manager
        .build_token_smiles(smiles, &mut arena, group, "none")
        .unwrap();
    arena[group].fragment = Some(fragment);
    (state, arena, group)
}

#[test]
fn dl_carbohydrate_normal_and_opposite_natural_enantiomer_cases() {
    // Upstream tests deliberately use this generic tetrahedral fragment to
    // exercise the prefix transformation without assigning a sugar identity.
    for (value, opposite, invert) in [
        ("d", false, false),
        ("dg", false, false),
        ("l", false, true),
        ("lg", false, true),
        ("d", true, true),
        ("l", true, false),
    ] {
        let (mut state, mut arena, group) = fixture("N[C@@H](C)C");
        if opposite {
            arena[group].add_attribute("naturalEntIsOpposite", "yes");
        }
        let before = state
            .graph()
            .atom(AtomId(1))
            .parity
            .as_ref()
            .unwrap()
            .parity;
        let rules = SuffixRules::new().unwrap();
        ComponentProcessor::new(&mut state, &rules)
            .apply_dl_stereochemistry_to_carbohydrate(&arena, group, value)
            .unwrap();
        let parity = state.graph().atom(AtomId(1)).parity.as_ref().unwrap();
        assert_eq!(parity.parity, if invert { -before } else { before });
        assert_eq!(parity.stereo_group.kind, StereoGroupType::Absolute);
        // Source sets group 0 when inversion occurs; unchanged parity retains
        // its original default absolute group 1.
        assert_eq!(parity.stereo_group.number, if invert { 0 } else { 1 });
    }
}

#[test]
fn dl_carbohydrate_sets_one_racemic_group_and_applies_natural_opposite_flip() {
    for opposite in [false, true] {
        let (mut state, mut arena, group) = fixture("O=C[C@H](O)[C@H](O)CO");
        if opposite {
            arena[group].add_attribute("naturalEntIsOpposite", "yes");
        }
        let before: Vec<_> = [AtomId(2), AtomId(4)]
            .map(|atom| state.graph().atom(atom).parity.as_ref().unwrap().parity)
            .into();
        let rules = SuffixRules::new().unwrap();
        ComponentProcessor::new(&mut state, &rules)
            .apply_dl_stereochemistry_to_carbohydrate(&arena, group, "dl")
            .unwrap();
        assert_eq!(state.racemic_group_count, 2);
        for (atom, before) in [AtomId(2), AtomId(4)].into_iter().zip(before) {
            let parity = state.graph().atom(atom).parity.as_ref().unwrap();
            assert_eq!(parity.parity, if opposite { -before } else { before });
            assert_eq!(
                parity.stereo_group,
                StereoGroup {
                    kind: StereoGroupType::Racemic,
                    number: 2
                }
            );
        }
    }
}

#[test]
fn d_prefix_preserves_a_preexisting_stereo_group_when_no_inversion_occurs() {
    let (mut state, arena, group) = fixture("N[C@@H](C)C");
    let group_before = StereoGroup {
        kind: StereoGroupType::Relative,
        number: 9,
    };
    state
        .graph_mut()
        .atom_mut(AtomId(1))
        .parity
        .as_mut()
        .unwrap()
        .stereo_group = group_before;
    let rules = SuffixRules::new().unwrap();
    ComponentProcessor::new(&mut state, &rules)
        .apply_dl_stereochemistry_to_carbohydrate(&arena, group, "d")
        .unwrap();
    assert_eq!(
        state
            .graph()
            .atom(AtomId(1))
            .parity
            .as_ref()
            .unwrap()
            .stereo_group,
        group_before
    );
}

#[test]
fn dl_carbohydrate_rejects_achiral_and_unknown_descriptor_cases() {
    let rules = SuffixRules::new().unwrap();
    let (mut state, arena, group) = fixture("CCO");
    assert_eq!(
        ComponentProcessor::new(&mut state, &rules)
            .apply_dl_stereochemistry_to_carbohydrate(&arena, group, "d")
            .unwrap_err()
            .to_string(),
        "D/L stereochemistry :d found before achiral carbohydrate"
    );
    let (mut state, arena, group) = fixture("N[C@@H](C)C");
    assert_eq!(
        ComponentProcessor::new(&mut state, &rules)
            .apply_dl_stereochemistry_to_carbohydrate(&arena, group, "ds")
            .unwrap_err()
            .to_string(),
        "Unexpected value for D/L stereochemistry found before carbohydrate: ds"
    );
}

#[test]
fn configurational_prefix_d_l_dl_transformations_follow_source_tokens() {
    for (value, input, expected) in [
        ("d", "l/r", "l/r"),
        ("dg", "r/l", "r/l"),
        ("l", "r/l", "l/r"),
        ("lg", "r/l/r/r", "l/r/l/l"),
        ("dl", "l/r", "?/?"),
        ("dl", "r/l/r/r", "?/?/?/?"),
        ("dl", "l/r/", "?/?"),
        ("dl", "", "?"),
        ("dl", "/", "?"),
    ] {
        let mut arena = Arena::default();
        let prefix = arena.token("stereoChemistry", "prefix");
        arena[prefix].add_attribute("type", "carbohydrateConfigurationalPrefix");
        arena[prefix].add_attribute("value", input);
        ComponentProcessor::apply_dl_stereochemistry_to_carbohydrate_configurational_prefix(
            &mut arena, prefix, value,
        )
        .unwrap();
        assert_eq!(arena[prefix].attribute("value"), Some(expected));
    }
}

#[test]
fn configurational_prefix_invalid_values_are_not_accepted_as_l_configuration() {
    for input in ["r/?", "r/l/", "", "/"] {
        let mut arena = Arena::default();
        let prefix = arena.token("stereoChemistry", "prefix");
        arena[prefix].add_attribute("value", input);
        assert_eq!(
            ComponentProcessor::apply_dl_stereochemistry_to_carbohydrate_configurational_prefix(
                &mut arena, prefix, "l"
            )
            .unwrap_err()
            .to_string(),
            format!("OPSIN Bug: Invalid carbohydrate prefix value: {input}")
        );
        assert_eq!(arena[prefix].attribute("value"), Some(input));
    }
    let mut arena = Arena::default();
    let prefix = arena.token("stereoChemistry", "prefix");
    arena[prefix].add_attribute("value", "r/l");
    assert_eq!(
        ComponentProcessor::apply_dl_stereochemistry_to_carbohydrate_configurational_prefix(
            &mut arena, prefix, "ls"
        )
        .unwrap_err()
        .to_string(),
        "Unexpected value for D/L stereochemistry found before carbohydrate prefix: ls"
    );
}
