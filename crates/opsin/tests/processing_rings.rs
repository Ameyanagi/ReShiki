// Java-free phase fixtures derived from OPSIN 2.9.0 ComponentProcessor.java
// (2826–3924), its specialHwRings table, and upstream spiro/hwRings examples.
// These assert graph topology and locant behavior independently of final
// serialization or the completeness of the full name-conversion pipeline.

use opsin::ParseOptions;
use opsin::build_state::BuildState;
use opsin::component_processor_rings::{assign_element_symbol_locants, process_hw};
use opsin::graph::{AtomId, Element, FragmentId};
use opsin::parse_tree::{Arena, NodeId};

type HwDefaultCase<'a> = (&'a str, &'a [(&'a str, &'a str)], &'a [Element]);

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
    let fragment = state
        .fragment_manager
        .build_token_smiles(smiles, arena, node, "numeric")
        .unwrap();
    arena[node].fragment = Some(fragment);
    state.xml_suffix_map.insert(node, Vec::new());
    (node, fragment)
}
fn carbon_ring(size: usize) -> String {
    format!("C1{}1", "C".repeat(size - 1))
}
fn hw(
    size: usize,
    stem: &str,
    heteroatoms: &[(&str, &str)],
) -> (BuildState, Arena, NodeId, NodeId, FragmentId, Vec<NodeId>) {
    let (mut state, mut arena, root) = setup();
    let heteroatoms = heteroatoms
        .iter()
        .map(|&(text, element)| token(&mut arena, root, "heteroatom", text, &[("value", element)]))
        .collect();
    let (group, ring) = group(&mut state, &mut arena, root, stem, &carbon_ring(size));
    arena[group].add_attribute("subType", "hantzschWidman");
    (state, arena, root, group, ring, heteroatoms)
}
fn atom(state: &BuildState, ring: FragmentId, locant: &str) -> AtomId {
    state.graph().atom_by_locant(ring, locant).unwrap()
}
fn elements(state: &BuildState, ring: FragmentId) -> Vec<Element> {
    state
        .graph()
        .fragment(ring)
        .atoms
        .iter()
        .map(|&a| state.graph().atom(a).element)
        .collect()
}

#[test]
fn all_special_hw_default_element_orders_follow_upstream_table() {
    use Element::*;
    let cases: &[HwDefaultCase<'_>] = &[
        ("in", &[("selena", "Se")], &[Se, C, C, C, C, C]),
        ("in", &[("tellura", "Te")], &[Te, C, C, C, C, C]),
        ("ol", &[("thia", "S")], &[S, C, C, C, C]),
        ("ol", &[("selena", "Se")], &[Se, C, C, C, C]),
        ("ol", &[("tellura", "Te")], &[Te, C, C, C, C]),
        ("azol", &[("oxa", "O")], &[O, C, N, C, C]),
        ("azol", &[("thia", "S")], &[S, C, N, C, C]),
        ("azol", &[("selena", "Se")], &[Se, C, N, C, C]),
        ("azol", &[("tellura", "Te")], &[Te, C, N, C, C]),
        ("azolidin", &[("oxa", "O")], &[O, C, N, C, C]),
        ("azolidin", &[("thia", "S")], &[S, C, N, C, C]),
        ("azolidin", &[("selena", "Se")], &[Se, C, N, C, C]),
        ("azolidin", &[("tellura", "Te")], &[Te, C, N, C, C]),
        ("azolid", &[("oxa", "O")], &[O, C, N, C, C]),
        ("azolid", &[("thia", "S")], &[S, C, N, C, C]),
        ("azolid", &[("selena", "Se")], &[Se, C, N, C, C]),
        ("azolid", &[("tellura", "Te")], &[Te, C, N, C, C]),
        ("azolin", &[("oxa", "O")], &[O, C, N, C, C]),
        ("azolin", &[("thia", "S")], &[S, C, N, C, C]),
        ("azolin", &[("selena", "Se")], &[Se, C, N, C, C]),
        ("azolin", &[("tellura", "Te")], &[Te, C, N, C, C]),
        (
            "in",
            &[("aza", "N"), ("aza", "N"), ("aza", "N")],
            &[N, C, N, C, N, C],
        ),
        ("olan", &[("oxa", "O"), ("oxa", "O")], &[O, C, O, C, C]),
        ("ol", &[("oxa", "O"), ("oxa", "O")], &[O, C, O, C, C]),
        ("an", &[("oxa", "O"), ("oxa", "O")], &[O, C, C, O, C, C]),
        ("in", &[("oxa", "O"), ("oxa", "O")], &[O, C, C, O, C, C]),
        (
            "an",
            &[("oxa", "O"), ("oxa", "O"), ("oxa", "O")],
            &[O, C, O, C, O, C],
        ),
        ("in", &[("bora", "B"), ("oxa", "O")], &[O, B, O, B, O, B]),
        ("in", &[("bora", "B"), ("aza", "N")], &[N, B, N, B, N, B]),
        ("in", &[("bora", "B"), ("thia", "S")], &[S, B, S, B, S, B]),
    ];
    for &(stem, replacements, expected) in cases {
        let (mut state, mut arena, root, group, ring, heteroatoms) =
            hw(expected.len(), stem, replacements);
        // The simple thiol-like names require their HW interpretation to have
        // an explicitly following token (the source's lexical disambiguator).
        arena[group].add_attribute("subsequentUnsemanticToken", "e");
        process_hw(&mut state, &mut arena, root).unwrap();
        assert_eq!(elements(&state, ring), expected, "{}", arena.value(group));
        assert!(heteroatoms.iter().all(|&id| arena[id].parent.is_none()));
    }
}

#[test]
fn blocked_hw_names_and_saturated_silicon_six_member_system_are_rejected() {
    for (prefix, element, stem, message) in [
        ("oxa", "O", "in", "Blocked Hantzsch-Widman system"),
        ("aza", "N", "in", "Blocked Hantzsch-Widman system"),
        (
            "sila",
            "Si",
            "an",
            "Blocked Hantzsch-Widman system (6 member saturated ring with no nitrogen but has Si/Ge/Sn/Pb)",
        ),
        (
            "germa",
            "Ge",
            "an",
            "Blocked Hantzsch-Widman system (6 member saturated ring with no nitrogen but has Si/Ge/Sn/Pb)",
        ),
        (
            "stanna",
            "Sn",
            "an",
            "Blocked Hantzsch-Widman system (6 member saturated ring with no nitrogen but has Si/Ge/Sn/Pb)",
        ),
        (
            "plumba",
            "Pb",
            "an",
            "Blocked Hantzsch-Widman system (6 member saturated ring with no nitrogen but has Si/Ge/Sn/Pb)",
        ),
    ] {
        let (mut state, mut arena, root, _, _, _) = hw(6, stem, &[(prefix, element)]);
        assert_eq!(
            process_hw(&mut state, &mut arena, root).unwrap_err().0,
            message
        );
    }
}

#[test]
fn nitrogen_or_unsaturation_allows_silicon_hw_ring() {
    let (mut state, mut arena, root, _, ring, _) = hw(6, "an", &[("sila", "Si"), ("aza", "N")]);
    process_hw(&mut state, &mut arena, root).unwrap();
    assert_eq!(elements(&state, ring)[..2], [Element::Si, Element::N]);
    let (mut state, mut arena, root, _, ring, _) = hw(6, "an", &[("sila", "Si")]);
    let first = atom(&state, ring, "1");
    state.graph_mut().atom_mut(first).spare_valency = true;
    process_hw(&mut state, &mut arena, root).unwrap();
    assert_eq!(state.graph().atom(first).element, Element::Si);
}

#[test]
fn saturated_borazine_removes_spare_valency() {
    let (mut state, mut arena, root, _, ring, _) = hw(6, "in", &[("bora", "B"), ("aza", "N")]);
    for atom in state.graph().fragment(ring).atoms.clone() {
        state.graph_mut().atom_mut(atom).spare_valency = true;
    }
    process_hw(&mut state, &mut arena, root).unwrap();
    assert!(
        state
            .graph()
            .fragment(ring)
            .atoms
            .iter()
            .all(|&a| !state.graph().atom(a).spare_valency)
    );
}

#[test]
fn hw_generic_class_name_checks_respect_locanted_suffix_and_lexical_override() {
    for suffix_locanted in [false, true] {
        let (mut state, mut arena, root, _, _, _) = hw(6, "in", &[("selena", "Se")]);
        let suffix = token(&mut arena, root, "suffix", "ic", &[("value", "ic")]);
        if suffix_locanted {
            arena[suffix].add_attribute("locant", "2");
        }
        let result = process_hw(&mut state, &mut arena, root);
        if suffix_locanted {
            result.unwrap();
        } else {
            assert_eq!(
                result.unwrap_err().0,
                "seleninic appears to be a generic class name, not a Hantzsch-Widman ring"
            );
        }
    }
    for (suffix, lexical_override) in [
        (None, false),
        (Some("ate"), false),
        (Some("ol"), false),
        (None, true),
    ] {
        let (mut state, mut arena, root, group, _, _) = hw(5, "ol", &[("thia", "S")]);
        if let Some(suffix) = suffix {
            token(&mut arena, root, "suffix", suffix, &[("value", suffix)]);
        }
        if lexical_override {
            arena[group].add_attribute("subsequentUnsemanticToken", "e");
        }
        let result = process_hw(&mut state, &mut arena, root);
        if lexical_override || suffix == Some("ol") {
            result.unwrap();
        } else {
            assert_eq!(
                result.unwrap_err().0,
                "thiol has the syntax for a Hantzsch-Widman ring but probably does not mean that in this context"
            );
        }
    }
}

#[test]
fn hw_explicit_locants_override_conventional_defaults_and_set_lambda() {
    let (mut state, mut arena, root, _, ring, heteroatoms) =
        hw(5, "ol", &[("oxa", "[O]"), ("aza", "[N]")]);
    arena[heteroatoms[0]].add_attribute("locant", "2");
    arena[heteroatoms[1]].add_attribute("locant", "4");
    arena[heteroatoms[1]].add_attribute("lambda", "5");
    process_hw(&mut state, &mut arena, root).unwrap();
    assert_eq!(
        elements(&state, ring),
        [Element::C, Element::O, Element::C, Element::N, Element::C]
    );
    assert_eq!(
        state
            .graph()
            .atom(atom(&state, ring, "4"))
            .lambda_convention_valency,
        Some(5)
    );
}

#[test]
fn duplicate_hw_locants_and_unextractable_elements_are_rejected() {
    let (mut state, mut arena, root, _, _, heteroatoms) =
        hw(5, "ol", &[("oxa", "O"), ("aza", "N")]);
    for heteroatom in heteroatoms {
        arena[heteroatom].add_attribute("locant", "1");
    }
    assert_eq!(
        process_hw(&mut state, &mut arena, root).unwrap_err().0,
        "Duplicate locants present in Hantzsch-Widman system"
    );
    let (mut state, mut arena, root, _, _, heteroatoms) = hw(5, "ol", &[("phospha", "invalid")]);
    arena[heteroatoms[0]].add_attribute("locant", "1");
    assert_eq!(
        process_hw(&mut state, &mut arena, root).unwrap_err().0,
        "Failed to extract element from Hantzsch-Widman heteroatom"
    );
}

#[test]
fn hw_delta_double_bonds_and_unlocanted_delta_tokens_follow_source() {
    let (mut state, mut arena, root, group, ring, _) = hw(5, "ol", &[]);
    let locanted = token(&mut arena, root, "delta", "2", &[]);
    let unlocanted = token(&mut arena, root, "delta", "", &[]);
    process_hw(&mut state, &mut arena, root).unwrap();
    let bond = state
        .graph()
        .bond_between(atom(&state, ring, "2"), atom(&state, ring, "3"))
        .unwrap();
    assert_eq!(state.graph().bond(bond).order, 2);
    assert!(arena[locanted].parent.is_none() && arena[unlocanted].parent.is_none());
    let next = arena.next_sibling(group).unwrap();
    assert_eq!(arena[next].name, "unsaturator");
    assert_eq!(arena[next].attribute("value"), Some("2"));
}

#[test]
fn standalone_triazine_defaults_are_disabled_for_fusion_component() {
    let (mut state, mut arena, root, group_node, ring, _) =
        hw(6, "in", &[("aza", "N"), ("aza", "N"), ("aza", "N")]);
    let (benz, _) = group(&mut state, &mut arena, root, "benz", "c1ccccc1");
    arena.detach(benz);
    arena.insert_before(group_node, benz);
    // Heteroatom prefixes need to stay directly adjacent to their HW group.
    arena.detach(benz);
    arena.insert_child(root, benz, 0);
    process_hw(&mut state, &mut arena, root).unwrap();
    assert_eq!(
        elements(&state, ring),
        [
            Element::N,
            Element::N,
            Element::N,
            Element::C,
            Element::C,
            Element::C
        ]
    );
    assert!(state.warnings.is_empty());
}

#[test]
fn unlocanted_hw_assignment_reports_ambiguity_except_fusion_prefix() {
    for prefix in [false, true] {
        let (mut state, mut arena, root, group, ring, heteroatoms) =
            hw(7, "epin", &[("aza", "N"), ("aza", "N")]);
        arena[heteroatoms[0]].add_attribute("lambda", "3");
        if prefix {
            arena[group].add_attribute("subsequentUnsemanticToken", "o");
        }
        process_hw(&mut state, &mut arena, root).unwrap();
        assert_eq!(elements(&state, ring)[..2], [Element::N, Element::N]);
        assert_eq!(
            state
                .graph()
                .atom(atom(&state, ring, "1"))
                .lambda_convention_valency,
            Some(3)
        );
        assert_eq!(state.warnings.len(), usize::from(!prefix));
    }
}

#[test]
fn excessive_hw_heteroatoms_fail_and_dithiazolium_charge_defaults_to_sulfur() {
    let (mut state, mut arena, root, _, _, _) = hw(
        3,
        "ir",
        &[("aza", "N"), ("aza", "N"), ("aza", "N"), ("aza", "N")],
    );
    assert_eq!(
        process_hw(&mut state, &mut arena, root).unwrap_err().0,
        "4 heteroatoms were specified for a Hantzsch-Widman ring with only 3 atoms"
    );
    let (mut state, mut arena, root, _, _, _) =
        hw(5, "ol", &[("thia", "S"), ("thia", "S"), ("aza", "N")]);
    let suffix = token(
        &mut arena,
        root,
        "suffix",
        "ium",
        &[("type", "charge"), ("value", "ium")],
    );
    process_hw(&mut state, &mut arena, root).unwrap();
    assert_eq!(arena[suffix].attribute("locant"), Some("1"));
}

#[test]
fn suffix_and_conjunctive_element_locants_precede_parent_element_locants() {
    let (mut state, mut arena, root) = setup();
    let (group, ring) = group(&mut state, &mut arena, root, "piperidine", "N1CCCCC1");
    let suffix = state
        .fragment_manager
        .build_smiles("[R]N", "suffix", "none")
        .unwrap();
    state.xml_suffix_map.insert(group, vec![suffix]);
    let conjunctive = token(&mut arena, root, "conjunctiveSuffixGroup", "imino", &[]);
    let conjunctive_fragment = state
        .fragment_manager
        .build_token_smiles("[R]N", &mut arena, conjunctive, "none")
        .unwrap();
    arena[conjunctive].fragment = Some(conjunctive_fragment);
    assign_element_symbol_locants(&mut state, &arena, root).unwrap();
    assert!(state.graph().atom_by_locant(suffix, "N").is_some());
    assert!(
        state
            .graph()
            .atom_by_locant(conjunctive_fragment, "N'")
            .is_some()
    );
    // Once suffixes use an element symbol, the parent's atoms of that element
    // do not acquire colliding element locants (FragmentTools.java 222–235).
    assert!(state.graph().atom_by_locant(ring, "N").is_none());
}
