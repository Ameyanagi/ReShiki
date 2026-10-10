// Source-grounded OPSIN IsotopeSpecificationParser and suffix isotope fixtures.
use opsin::{
    ParseOptions,
    build_state::BuildState,
    graph::{AtomId, Element},
    isotope_specification_parser::parse_isotope_specification_value,
    parse_tree::{Arena, NodeId},
    suffix_applier::SuffixApplier,
    suffix_rules::SuffixRules,
};

fn isotope_suffix(
    specification: &str,
    syntax: &str,
    suffix_smiles: &str,
    suffix_value: &str,
    labels: &str,
) -> (BuildState, Arena, NodeId, NodeId, NodeId) {
    let mut state = BuildState::new(ParseOptions::default());
    let mut arena = Arena::default();
    let root = arena.grouping("root");
    let group = arena.token("group", "methan");
    arena[group].add_attribute("type", "chain");
    arena[group].add_attribute("subType", "alkaneStem");
    arena.add_child(root, group);
    let parent = state
        .fragment_manager
        .build_token_smiles("C", &mut arena, group, "numeric")
        .unwrap();
    arena[group].fragment = Some(parent);
    let isotope = arena.token("isotopeSpecification", specification);
    arena[isotope].add_attribute("type", syntax);
    let suffix = arena.token("suffix", suffix_value);
    arena[suffix].add_attribute("type", "suffix");
    arena[suffix].add_attribute("value", suffix_value);
    if syntax == "iupacSystem" {
        arena.add_child(root, isotope);
    }
    arena.add_child(root, suffix);
    if syntax == "boughtonSystem" {
        arena.add_child(root, isotope);
    }
    let suffix_fragment = state
        .fragment_manager
        .build_token_smiles(suffix_smiles, &mut arena, suffix, labels)
        .unwrap();
    arena[suffix].fragment = Some(suffix_fragment);
    (state, arena, group, suffix, isotope)
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
fn parses_boughton_and_iupac_mass_element_multiplier_and_locants() {
    for (syntax, value, element, mass, multiplier, locants) in [
        ("boughtonSystem", "-d3", Element::H, 2, 3, None),
        ("boughtonSystem", "-15N2", Element::N, 15, 2, None),
        (
            "boughtonSystem",
            "-1A,2-13C",
            Element::C,
            13,
            2,
            Some(vec!["1a".to_string(), "2".to_string()]),
        ),
        (
            "iupacSystem",
            "O-18O",
            Element::O,
            18,
            1,
            Some(vec!["O".to_string()]),
        ),
        (
            "iupacSystem",
            "1,2-2H2",
            Element::H,
            2,
            2,
            Some(vec!["1".to_string(), "2".to_string()]),
        ),
    ] {
        let parsed = parse_isotope_specification_value(syntax, value).unwrap();
        assert_eq!(
            (
                parsed.element,
                parsed.isotope,
                parsed.multiplier,
                parsed.locants
            ),
            (element, mass, multiplier, locants)
        );
    }
}

#[test]
fn isotope_parser_failure_text_matches_upstream() {
    assert_eq!(
        parse_isotope_specification_value("iupacSystem", "1,2-13C3")
            .unwrap_err()
            .to_string(),
        "Mismatch between number of locants: 2 and number of C isotopes requested: 3"
    );
    assert_eq!(
        parse_isotope_specification_value("boughtonSystem", "D3")
            .unwrap_err()
            .to_string(),
        "Malformed isotope specification: D3"
    );
    assert_eq!(
        parse_isotope_specification_value("other", "13C")
            .unwrap_err()
            .to_string(),
        "Unsupported isotope specification syntax"
    );
    assert!(
        parse_isotope_specification_value("iupacSystem", "١٣C").is_err(),
        "Java's isotope regex uses ASCII digits"
    );
}

#[test]
fn boughton_isotope_applies_to_preceding_suffix_and_detaches() {
    let (mut state, mut arena, group, suffix, isotope) =
        isotope_suffix("-18O", "boughtonSystem", "[*]O", "ol", "/O");
    apply(&mut state, &mut arena, group, suffix).unwrap();
    assert_eq!(
        state.fragment_manager.graph.atom(AtomId(2)).isotope,
        Some(18)
    );
    assert!(arena[isotope].parent.is_none());
}

#[test]
fn locanted_boughton_specification_remains_for_the_parent_group() {
    let (mut state, mut arena, group, suffix, isotope) =
        isotope_suffix("-O-18O", "boughtonSystem", "[*]O", "ol", "/O");
    apply(&mut state, &mut arena, group, suffix).unwrap();
    assert_eq!(state.fragment_manager.graph.atom(AtomId(2)).isotope, None);
    assert!(arena[isotope].parent.is_some());
}

#[test]
fn boughton_failure_to_match_suffix_atoms_is_optional() {
    let (mut state, mut arena, group, suffix, isotope) =
        isotope_suffix("-15N", "boughtonSystem", "[*]O", "ol", "none");
    apply(&mut state, &mut arena, group, suffix).unwrap();
    assert!(arena[isotope].parent.is_some());
    assert!(
        state
            .fragment_manager
            .graph
            .atoms
            .iter()
            .all(|a| a.isotope.is_none())
    );
}

#[test]
fn locanted_iupac_specification_applies_to_following_suffix_atom() {
    let (mut state, mut arena, group, suffix, isotope) =
        isotope_suffix("O-18O", "iupacSystem", "[*]O", "ol", "/O");
    apply(&mut state, &mut arena, group, suffix).unwrap();
    assert_eq!(
        state.fragment_manager.graph.atom(AtomId(2)).isotope,
        Some(18)
    );
    assert!(arena[isotope].parent.is_none());
}

#[test]
fn multiple_iupac_specifications_apply_backwards_in_source_sibling_order() {
    let (mut state, mut arena, group, suffix, oxygen_isotope) =
        isotope_suffix("O-18O", "iupacSystem", "[*]NO", "hydroxamic", "/N/O");
    // Use amine's ordinary AddGroup rule with a prepared hydroxylamine
    // fragment; the source phase trusts ComponentProcessor's fragment.
    arena[suffix].set_attribute("value", "amine");
    let nitrogen_isotope = arena.token("isotopeSpecification", "N-15N");
    arena[nitrogen_isotope].add_attribute("type", "iupacSystem");
    arena.insert_before(oxygen_isotope, nitrogen_isotope);
    apply(&mut state, &mut arena, group, suffix).unwrap();
    assert_eq!(
        state.fragment_manager.graph.atom(AtomId(2)).isotope,
        Some(15)
    );
    assert_eq!(
        state.fragment_manager.graph.atom(AtomId(3)).isotope,
        Some(18)
    );
    assert!(arena[oxygen_isotope].parent.is_none());
    assert!(arena[nitrogen_isotope].parent.is_none());
}

#[test]
fn ambiguous_isotope_positions_emit_source_warning() {
    let (mut state, mut arena, group, suffix, _) =
        isotope_suffix("15N", "iupacSystem", "[*](=N)N", "amidine", "/N2/N1");
    apply(&mut state, &mut arena, group, suffix).unwrap();
    assert_eq!(
        state.fragment_manager.graph.atom(AtomId(2)).isotope,
        Some(15)
    );
    assert_eq!(state.warnings.len(), 1);
    assert_eq!(state.warnings[0].message, "Position of isotope on amidine");
}

#[test]
fn unlocanted_hydrogen_isotope_creates_real_hydrogen_atoms_on_suffix() {
    let (mut state, mut arena, group, suffix, isotope) =
        isotope_suffix("2H2", "iupacSystem", "[*]N", "amine", "/N");
    apply(&mut state, &mut arena, group, suffix).unwrap();
    let graph = &state.fragment_manager.graph;
    assert_eq!(
        graph.neighbours(AtomId(2)),
        [AtomId(3), AtomId(4), AtomId(0)]
    );
    for id in [AtomId(3), AtomId(4)] {
        assert_eq!(
            (graph.atom(id).element, graph.atom(id).isotope),
            (Element::H, Some(2))
        );
    }
    assert!(arena[isotope].parent.is_none());
    assert!(state.warnings.is_empty());
}

#[test]
fn locanted_hydrogen_isotope_is_attached_to_the_named_suffix_atom() {
    let (mut state, mut arena, group, suffix, _) =
        isotope_suffix("N-2H", "iupacSystem", "[*]N", "amine", "/N");
    apply(&mut state, &mut arena, group, suffix).unwrap();
    assert_eq!(
        state.fragment_manager.graph.atom(AtomId(3)).isotope,
        Some(2)
    );
    assert!(
        state
            .fragment_manager
            .graph
            .bond_between(AtomId(2), AtomId(3))
            .is_some()
    );
}

#[test]
fn mandatory_iupac_isotope_failures_match_upstream() {
    for (specification, smiles, value, labels, expected) in [
        (
            "15N",
            "[*]O",
            "ol",
            "none",
            "Failed to find sufficient atoms for N isotope replacement",
        ),
        (
            "2H2",
            "[*]O",
            "ol",
            "none",
            "Failed to find sufficient hydrogen atoms for unlocanted hydrogen isotope replacement",
        ),
        (
            "O-15N",
            "[*]O",
            "ol",
            "/O",
            "The atom at locant: O was not a N",
        ),
    ] {
        let (mut state, mut arena, group, suffix, _) =
            isotope_suffix(specification, "iupacSystem", smiles, value, labels);
        assert_eq!(
            apply(&mut state, &mut arena, group, suffix)
                .unwrap_err()
                .to_string(),
            expected
        );
    }
}
