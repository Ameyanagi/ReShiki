//! Java-free phase tests for the pinned StructureBuildingMethods port.
use opsin::{
    ParseOptions,
    build_results::BuildResults,
    build_state::BuildState,
    graph::{AtomId, Element, FragmentId},
    parse_tree::{Arena, NodeId},
    structure_builder,
    structure_building_methods::{self, Assembly},
};

fn state() -> BuildState {
    BuildState::new(ParseOptions::strict())
}
fn group(
    arena: &mut Arena,
    state: &mut BuildState,
    parent: NodeId,
    name: &str,
    smiles: &str,
    labels: &str,
) -> (NodeId, FragmentId) {
    let node = arena.token("group", name);
    arena.add_child(parent, node);
    let fragment = state
        .fragment_manager
        .build_token_smiles(smiles, arena, node, labels)
        .unwrap();
    // Production resolve_group sets the primary group-to-fragment association;
    // FragmentManager also builds suffix fragments without replacing it.
    arena[node].fragment = Some(fragment);
    state.xml_suffix_map.insert(node, Vec::new());
    (node, fragment)
}
fn word(arena: &mut Arena, rule: NodeId, kind: &str) -> NodeId {
    let word = arena.grouping("word");
    arena[word].set_attribute("type", kind);
    arena.add_child(rule, word);
    word
}
fn rule(arena: &mut Arena, molecule: NodeId, kind: &str) -> NodeId {
    let rule = arena.grouping("wordRule");
    arena[rule].set_attribute("wordRule", kind);
    arena.add_child(molecule, rule);
    rule
}
fn scope(arena: &mut Arena, parent: NodeId, kind: &str) -> NodeId {
    let node = arena.grouping(kind);
    arena.add_child(parent, node);
    node
}

#[test]
fn upstream_explicit_bracket_depth_controls_primed_locant_fallback() {
    let mut arena = Arena::default();
    let mut state = state();
    let word = arena.grouping("word");
    let b = scope(&mut arena, word, "bracket");
    let b2 = scope(&mut arena, b, "bracket");
    let sub = scope(&mut arena, b2, "substituent");
    let assembly = Assembly {
        state: &mut state,
        arena: &mut arena,
    };
    assert_eq!(assembly.bracketed_primed_locant(sub, "4"), None);
    assert_eq!(assembly.bracketed_primed_locant(sub, "4'"), None);
    assert_eq!(
        assembly.bracketed_primed_locant(sub, "4''"),
        Some("4".into())
    );
    assembly.arena[b2].set_attribute("type", "implicit");
    assert_eq!(
        assembly.bracketed_primed_locant(sub, "4'"),
        Some("4".into())
    );
    assert_eq!(assembly.bracketed_primed_locant(sub, "4''"), None);
    assembly.arena[b].set_attribute("type", "implicit");
    assert_eq!(assembly.bracketed_primed_locant(sub, "4'"), None);
}

fn substitution(phospho: bool, locanted: bool) -> Element {
    let mut arena = Arena::default();
    let mut state = state();
    let word = arena.grouping("word");
    let sub = scope(&mut arena, word, "substituent");
    let (token, fragment) = group(
        &mut arena,
        &mut state,
        sub,
        if phospho { "phospho" } else { "amino" },
        if phospho { "-P(=O)O" } else { "-N" },
        "none",
    );
    if phospho {
        arena[token].set_attribute("subType", "phospho");
    }
    let root = scope(&mut arena, word, "root");
    group(
        &mut arena,
        &mut state,
        root,
        "alcohol",
        if locanted { "CCCCO" } else { "CO" },
        if locanted { "1/2/3/4/" } else { "none" },
    );
    if locanted {
        arena[sub].set_attribute("locant", "4");
    }
    let mut assembly = Assembly {
        state: &mut state,
        arena: &mut arena,
    };
    if locanted {
        assembly.resolve_root_or_substituent_locanted(sub).unwrap();
    } else {
        assembly
            .resolve_root_or_substituent_unlocanted(sub)
            .unwrap();
    }
    let bonds = assembly
        .state
        .fragment_manager
        .inter_fragment_bonds(fragment)
        .unwrap();
    assert_eq!(bonds.len(), 1);
    let first = assembly.graph().fragment(fragment).atoms[0];
    let other = assembly.graph().bond(bonds[0]).other_atom(first).unwrap();
    assembly.graph().atom(other).element
}
#[test]
fn upstream_amino_substitution_prefers_carbon() {
    assert_eq!(substitution(false, false), Element::C);
}
#[test]
fn upstream_unlocanted_phospho_substitution_prefers_hydroxy_oxygen() {
    assert_eq!(substitution(true, false), Element::O);
}
#[test]
fn upstream_locanted_phospho_substitution_uses_hydroxy_neighbor() {
    assert_eq!(substitution(true, true), Element::O);
}

#[test]
fn alternative_scope_order_prefers_unbracketed_root_then_substituent() {
    let mut arena = Arena::default();
    let mut state = state();
    let word = arena.grouping("word");
    let start = scope(&mut arena, word, "substituent");
    let adjacent = scope(&mut arena, word, "substituent");
    let (_, adjacent_fragment) = group(&mut arena, &mut state, adjacent, "adjacent", "C", "none");
    let bracket = scope(&mut arena, word, "bracket");
    let bracket_sub = scope(&mut arena, bracket, "substituent");
    let (_, bracket_fragment) = group(
        &mut arena,
        &mut state,
        bracket_sub,
        "bracketed",
        "N",
        "none",
    );
    let root = scope(&mut arena, word, "root");
    let (_, root_fragment) = group(&mut arena, &mut state, root, "root", "O", "none");
    assert_eq!(
        structure_building_methods::find_alternative_fragments(&arena, start).unwrap(),
        vec![root_fragment, adjacent_fragment, bracket_fragment]
    );
    arena[bracket].set_attribute("type", "implicit");
    assert_eq!(
        structure_building_methods::find_alternative_fragments(&arena, start).unwrap(),
        vec![root_fragment, bracket_fragment, adjacent_fragment]
    );
    arena[root].set_attribute("multiplier", "2");
    assert_eq!(
        structure_building_methods::find_alternative_fragments(&arena, start).unwrap(),
        vec![bracket_fragment, adjacent_fragment]
    );
}

#[test]
fn additive_join_consumes_both_out_atoms_and_preserves_interfragment_identity() {
    let mut arena = Arena::default();
    let mut state = state();
    let word = arena.grouping("word");
    let sub = scope(&mut arena, word, "substituent");
    let (_, one) = group(&mut arena, &mut state, sub, "oxy", "O", "none");
    let root = scope(&mut arena, word, "root");
    let (_, two) = group(&mut arena, &mut state, root, "ethylene", "CC", "none");
    let oxygen = state.graph().fragment(one).atoms[0];
    let carbon = state.graph().fragment(two).atoms[0];
    state.graph_mut().add_out_atom(one, oxygen, 1, true);
    state.graph_mut().add_out_atom(two, carbon, 1, true);
    Assembly {
        state: &mut state,
        arena: &mut arena,
    }
    .join_fragments_additively(one, two)
    .unwrap();
    assert!(state.graph().fragment(one).out_atoms.is_empty());
    assert!(state.graph().fragment(two).out_atoms.is_empty());
    assert_eq!(
        state
            .graph()
            .bond(state.graph().bond_between(oxygen, carbon).unwrap())
            .order,
        1
    );
    assert_eq!(
        state
            .fragment_manager
            .inter_fragment_bonds(one)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn epoxide_locants_form_two_contacts_and_consume_each_bridge_out_atom() {
    let mut arena = Arena::default();
    let mut state = state();
    let word = arena.grouping("word");
    let sub = scope(&mut arena, word, "substituent");
    let (token, one) = group(&mut arena, &mut state, sub, "epoxy", "O", "none");
    arena[token].set_attribute("subType", "epoxyLike");
    let root = scope(&mut arena, word, "root");
    let (_, two) = group(&mut arena, &mut state, root, "ethane", "CC", "1/2");
    let oxygen = state.graph().fragment(one).atoms[0];
    let carbons = state.graph().fragment(two).atoms.clone();
    for (index, locant) in ["1", "2"].iter().enumerate() {
        state.graph_mut().add_out_atom(one, oxygen, 1, true);
        state
            .graph_mut()
            .set_out_atom_locant(one, index, Some((*locant).into()));
    }
    let targets =
        structure_building_methods::form_epoxide(&mut state, &mut arena, one, carbons[0]).unwrap();
    assert_eq!(targets, [carbons[0], carbons[1]]);
    assert!(state.graph().fragment(one).out_atoms.is_empty());
    assert!(
        carbons
            .iter()
            .all(|&carbon| state.graph().bond_between(oxygen, carbon).is_some())
    );
}

#[test]
fn locanted_unsaturation_and_isotope_are_applied_before_unlocanted_radicals() {
    let mut arena = Arena::default();
    let mut state = state();
    let root = arena.grouping("root");
    let (_, fragment) = group(&mut arena, &mut state, root, "butene", "CCCC", "1/2/3/4");
    let unsaturation = arena.token("unsaturator", "en");
    arena[unsaturation].set_attribute("locant", "2");
    arena[unsaturation].set_attribute("value", "2");
    arena.add_child(root, unsaturation);
    let isotope = arena.token("isotopeSpecification", "1-13C");
    arena[isotope].set_attribute("type", "iupac");
    arena.add_child(root, isotope);
    // Exact resource type, rather than depending on a handwritten grammar alias.
    arena[isotope].set_attribute("type", opsin::xml_declarations::IUPACSYSTEM_TYPE_VAL);
    structure_building_methods::resolve_locanted_features(&mut state, &mut arena, root).unwrap();
    let atoms = state.graph().fragment(fragment).atoms.clone();
    assert_eq!(state.graph().atom(atoms[0]).isotope, Some(13));
    assert_eq!(
        state
            .graph()
            .bond(state.graph().bond_between(atoms[1], atoms[2]).unwrap())
            .order,
        2
    );
    assert!(arena[unsaturation].parent.is_none());
    assert!(arena[isotope].parent.is_none());
}

#[test]
fn build_results_out_atom_removal_updates_owner_and_retains_order() {
    let mut arena = Arena::default();
    let mut state = state();
    let word = arena.grouping("word");
    let sub = scope(&mut arena, word, "substituent");
    let (_, one) = group(&mut arena, &mut state, sub, "ethyl", "-CC", "none");
    let root = scope(&mut arena, word, "root");
    let (_, two) = group(&mut arena, &mut state, root, "amine", "N", "none");
    let mut results = BuildResults::from_node(&arena, state.graph(), word).unwrap();
    assert_eq!(results.fragments, vec![one, two]);
    let atom = results.out_atom(state.graph(), 0).unwrap().atom;
    let out = results.remove_out_atom(state.graph_mut(), 0).unwrap();
    assert_eq!(out.atom, atom);
    assert!(results.out_atoms.is_empty());
    assert!(state.graph().fragment(one).out_atoms.is_empty());
    assert_eq!(state.graph().atom(atom).out_valency, 0);
}

fn heavy_atoms(state: &BuildState, fragment: FragmentId) -> Vec<AtomId> {
    state
        .graph()
        .fragment(fragment)
        .atoms
        .iter()
        .copied()
        .filter(|&a| state.graph().atom(a).element != Element::H)
        .collect()
}
#[test]
fn simple_word_finalization_is_native_and_keeps_ethanol_topology() {
    let mut arena = Arena::default();
    let mut state = state();
    let molecule = arena.grouping("molecule");
    let r = rule(&mut arena, molecule, "simple");
    let w = word(&mut arena, r, "full");
    let root = scope(&mut arena, w, "root");
    let (_, ethanol) = group(&mut arena, &mut state, root, "ethanol", "CCO", "1/2/");
    let expected = state.graph().fragment(ethanol).atoms.clone();
    let f = structure_builder::build_fragment(&mut state, &mut arena, molecule).unwrap();
    assert_eq!(heavy_atoms(&state, f), expected);
    assert_eq!(state.graph().fragment(f).atoms.len(), 9);
    assert_eq!(state.graph().atom(expected[2]).element, Element::O);
    assert!(
        state
            .graph()
            .bond_between(expected[1], expected[2])
            .is_some()
    );
}

#[test]
fn ester_word_rule_connects_ethyl_to_acid_oxygen_and_neutralizes_ate() {
    let mut arena = Arena::default();
    let mut state = state();
    let molecule = arena.grouping("molecule");
    let r = rule(&mut arena, molecule, "ester");
    let sub = word(&mut arena, r, "substituent");
    let s = scope(&mut arena, sub, "substituent");
    let (_, ethyl) = group(&mut arena, &mut state, s, "ethyl", "-CC", "none");
    let acid = word(&mut arena, r, "full");
    let root = scope(&mut arena, acid, "root");
    let (_, ate) = group(
        &mut arena,
        &mut state,
        root,
        "ethanoate",
        "CC(=O)[O-]",
        "1/2//",
    );
    let carbon = state.graph().fragment(ethyl).atoms[0];
    let oxygen = *state.graph().fragment(ate).atoms.last().unwrap();
    state
        .graph_mut()
        .fragment_mut(ate)
        .functional_atoms
        .push(oxygen);
    state
        .graph_mut()
        .atom_mut(oxygen)
        .protons_explicitly_added_or_removed = -1;
    let f = structure_builder::build_fragment(&mut state, &mut arena, molecule).unwrap();
    assert_eq!(heavy_atoms(&state, f).len(), 6);
    assert_eq!(state.graph().atom(oxygen).charge, 0);
    assert_eq!(
        state
            .graph()
            .atom(oxygen)
            .protons_explicitly_added_or_removed,
        0
    );
    assert!(state.graph().bond_between(carbon, oxygen).is_some());
    assert!(state.graph().fragment(f).out_atoms.is_empty());
}

#[test]
fn locanted_multiplier_clones_after_expansion_without_joining_substituents_to_each_other() {
    let mut arena = Arena::default();
    let mut state = state();
    let word = arena.grouping("word");
    let sub = scope(&mut arena, word, "substituent");
    arena[sub].set_attribute("locant", "2,3");
    arena[sub].set_attribute("multiplier", "2");
    let multiplier = arena.token("multiplier", "di");
    arena[multiplier].set_attribute("value", "2");
    arena.add_child(sub, multiplier);
    let (_, methyl) = group(&mut arena, &mut state, sub, "methyl", "-C", "1");
    let root = scope(&mut arena, word, "root");
    let (_, parent) = group(&mut arena, &mut state, root, "butane", "CCCC", "1/2/3/4");
    let chain = state.graph().fragment(parent).atoms.clone();
    Assembly {
        state: &mut state,
        arena: &mut arena,
    }
    .resolve_word_or_bracket(word)
    .unwrap();
    let groups = arena.descendants_named(word, "group");
    assert_eq!(groups.len(), 3);
    let methyls = groups
        .iter()
        .filter_map(|&node| arena[node].fragment)
        .filter(|&f| f != parent)
        .collect::<Vec<_>>();
    assert_eq!(methyls.len(), 2);
    assert!(methyls.contains(&methyl));
    let terminal = methyls
        .iter()
        .map(|&f| state.graph().fragment(f).atoms[0])
        .collect::<Vec<_>>();
    assert!(
        state
            .graph()
            .bond_between(terminal[0], terminal[1])
            .is_none()
    );
    for target in &chain[1..3] {
        assert_eq!(
            terminal
                .iter()
                .filter(|&&from| state.graph().bond_between(from, *target).is_some())
                .count(),
            1
        );
    }
    assert!(
        methyls
            .iter()
            .all(|&f| state.graph().fragment(f).out_atoms.is_empty())
    );
}

#[test]
fn substitutive_methylene_combines_same_atom_out_valencies() {
    let mut arena = Arena::default();
    let mut state = state();
    let word = arena.grouping("word");
    let sub = scope(&mut arena, word, "substituent");
    arena[sub].set_attribute("locant", "1");
    let (_, methylene) = group(&mut arena, &mut state, sub, "methylene", "C", "none");
    let atom = state.graph().fragment(methylene).atoms[0];
    for _ in 0..2 {
        state.graph_mut().add_out_atom(methylene, atom, 1, true);
    }
    let root = scope(&mut arena, word, "root");
    let (_, cycle) = group(
        &mut arena,
        &mut state,
        root,
        "cyclohexane",
        "C1CCCCC1",
        "1/2/3/4/5/6",
    );
    let target = state.graph().fragment(cycle).atoms[0];
    Assembly {
        state: &mut state,
        arena: &mut arena,
    }
    .resolve_root_or_substituent_locanted(sub)
    .unwrap();
    let bond = state.graph().bond_between(atom, target).unwrap();
    assert_eq!(state.graph().bond(bond).order, 2);
    assert!(state.graph().fragment(methylene).out_atoms.is_empty());
    assert_eq!(state.graph().atom(atom).out_valency, 0);
}

#[test]
fn deoxy_replacement_preserves_stereochemical_reference_as_hydrogen() {
    let mut arena = Arena::default();
    let mut state = state();
    let root = arena.grouping("root");
    let (_, fragment) = group(
        &mut arena,
        &mut state,
        root,
        "stereocentre",
        "[C@@](F)(Cl)(Br)O",
        "1////",
    );
    let atoms = state.graph().fragment(fragment).atoms.clone();
    let centre = atoms[0];
    let oxygen = *atoms.last().unwrap();
    let original = state.graph().atom(centre).parity.clone().unwrap();
    structure_building_methods::apply_subtractive_prefix(&mut state, fragment, Element::O, "1")
        .unwrap();
    assert!(!state.graph().atom(oxygen).active);
    let parity = state.graph().atom(centre).parity.as_ref().unwrap();
    assert_eq!(parity.parity, original.parity);
    assert!(
        parity
            .atom_refs
            .contains(&Some(opsin::graph::StereoReference::DeoxyHydrogen))
    );
    assert_eq!(state.graph().atom(centre).element, Element::C);
}

#[test]
fn divalent_functional_rule_duplicates_implicit_methyl_before_ether_join() {
    let mut arena = Arena::default();
    let mut state = state();
    let molecule = arena.grouping("molecule");
    let r = rule(&mut arena, molecule, "divalentFunctionalGroup");
    let sub = word(&mut arena, r, "substituent");
    let s = scope(&mut arena, sub, "substituent");
    group(&mut arena, &mut state, s, "methyl", "-C", "none");
    let term = word(&mut arena, r, "functionalTerm");
    let root = scope(&mut arena, term, "root");
    let oxygen = arena.token("functionalGroup", "ether");
    arena[oxygen].set_attribute("value", "O");
    arena.add_child(root, oxygen);
    let final_fragment =
        structure_builder::build_fragment(&mut state, &mut arena, molecule).unwrap();
    let atoms = heavy_atoms(&state, final_fragment);
    assert_eq!(atoms.len(), 3);
    let oxygen = *atoms
        .iter()
        .find(|&&a| state.graph().atom(a).element == Element::O)
        .unwrap();
    assert_eq!(state.graph().neighbours(oxygen).len(), 2);
    assert!(
        state
            .graph()
            .neighbours(oxygen)
            .iter()
            .all(|&a| state.graph().atom(a).element == Element::C)
    );
    assert_eq!(arena.children_named(r, "word").len(), 3);
}

#[test]
fn strict_finalization_rejects_unconsumed_radicals() {
    let mut arena = Arena::default();
    let mut state = state();
    let molecule = arena.grouping("molecule");
    let r = rule(&mut arena, molecule, "simple");
    let w = word(&mut arena, r, "full");
    let root = scope(&mut arena, w, "root");
    group(&mut arena, &mut state, root, "radical", "-C", "none");
    let error = structure_builder::build_fragment(&mut state, &mut arena, molecule).unwrap_err();
    assert!(
        error
            .0
            .contains("Radicals are currently set to not convert")
    );
}

#[test]
fn functional_oxygen_locant_uses_upstream_amino_acid_style_alias() {
    let mut arena = Arena::default();
    let mut state = state();
    let molecule = arena.grouping("molecule");
    let r = rule(&mut arena, molecule, "ester");
    let sub = word(&mut arena, r, "substituent");
    arena[sub].set_attribute("locant", "O5");
    let s = scope(&mut arena, sub, "substituent");
    let (_, ethyl) = group(&mut arena, &mut state, s, "ethyl", "-CC", "none");
    let acid = word(&mut arena, r, "full");
    let root = scope(&mut arena, acid, "root");
    let (_, ate) = group(
        &mut arena,
        &mut state,
        root,
        "dicarboxylate",
        "C(=O)([O-])CCCC(=O)[O-]",
        "1///2/3/4/5//",
    );
    let atoms = state.graph().fragment(ate).atoms.clone();
    let oxygens = [atoms[2], atoms[8]];
    for oxygen in oxygens {
        state
            .graph_mut()
            .fragment_mut(ate)
            .functional_atoms
            .push(oxygen);
        state
            .graph_mut()
            .atom_mut(oxygen)
            .protons_explicitly_added_or_removed = -1;
    }
    let carbon = state.graph().fragment(ethyl).atoms[0];
    structure_builder::build_fragment(&mut state, &mut arena, molecule).unwrap();
    assert!(state.graph().bond_between(carbon, oxygens[1]).is_some());
    assert!(state.graph().bond_between(carbon, oxygens[0]).is_none());
}

#[test]
fn thioacetal_modifier_is_not_required_to_have_a_constructed_fragment() {
    let mut arena = Arena::default();
    let mut state = state();
    let molecule = arena.grouping("molecule");
    let r = rule(&mut arena, molecule, "acetal");
    let aldehyde = word(&mut arena, r, "full");
    let root = scope(&mut arena, aldehyde, "root");
    group(&mut arena, &mut state, root, "pentanal", "CCCCC=O", "none");
    for _ in 0..2 {
        let w = word(&mut arena, r, "substituent");
        let s = scope(&mut arena, w, "substituent");
        group(&mut arena, &mut state, s, "ethyl", "-CC", "none");
    }
    let term = word(&mut arena, r, "functionalTerm");
    let root = scope(&mut arena, term, "root");
    let multiplier = arena.token("multiplier", "di");
    arena[multiplier].set_attribute("value", "2");
    arena.add_child(root, multiplier);
    let thio = arena.token("group", "thio");
    arena[thio].set_attribute("value", "S");
    arena.add_child(root, thio);
    let class = arena.token("functionalClass", "acetal");
    arena[class].set_attribute("value", "O,O");
    arena.add_child(root, class);
    let f = structure_builder::build_fragment(&mut state, &mut arena, molecule).unwrap();
    assert!(arena[thio].fragment.is_none());
    let heavy = heavy_atoms(&state, f);
    assert_eq!(heavy.len(), 11);
    assert_eq!(
        heavy
            .iter()
            .filter(|&&a| state.graph().atom(a).element == Element::S)
            .count(),
        2
    );
    assert!(
        heavy
            .iter()
            .all(|&a| state.graph().atom(a).element != Element::O)
    );
    assert!(state.graph().fragment(f).out_atoms.is_empty());
}

#[test]
fn clone_primes_canonical_stereo_chemistry_node_before_late_assignment() {
    let mut arena = Arena::default();
    let mut state = state();
    let word = arena.grouping("word");
    let sub = scope(&mut arena, word, "substituent");
    let stereo = arena.token(opsin::xml_declarations::STEREOCHEMISTRY_EL, "R");
    arena[stereo].set_attribute("locant", "2");
    arena.add_child(sub, stereo);
    group(&mut arena, &mut state, sub, "substituent", "-CC", "1/2");
    let copy = Assembly {
        state: &mut state,
        arena: &mut arena,
    }
    .clone_element(sub, 2)
    .unwrap();
    let cloned = arena.descendants_named(copy, opsin::xml_declarations::STEREOCHEMISTRY_EL);
    assert_eq!(cloned.len(), 1);
    assert_eq!(arena[cloned[0]].attribute("locant"), Some("2''"));
    assert_eq!(arena[stereo].attribute("locant"), Some("2"));
}
