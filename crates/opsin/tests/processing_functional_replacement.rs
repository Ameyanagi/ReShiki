//! Cases grounded in FunctionalReplacement.java and the pinned infixes.xml.
use opsin::ParseOptions;
use opsin::build_state::BuildState;
use opsin::functional_replacement::{
    process_infix_functional_replacement_nomenclature as infix,
    process_prefix_functional_replacement_nomenclature as prefix,
};
use opsin::graph::{AtomId, Element};
use opsin::parse_tree::Arena;

fn acid_suffix(
    smiles: &str,
    infix_value: &str,
) -> (
    BuildState,
    Arena,
    opsin::parse_tree::NodeId,
    opsin::graph::FragmentId,
) {
    let mut state = BuildState::new(ParseOptions::default());
    let fragment = state
        .fragment_manager
        .build_smiles(smiles, "suffix", "none")
        .unwrap();
    let atoms = state.graph().fragment(fragment).atoms.clone();
    let functional = *atoms.last().unwrap();
    state
        .graph_mut()
        .fragment_mut(fragment)
        .functional_atoms
        .push(functional);
    let mut arena = Arena::default();
    let root = arena.grouping("root");
    let group = arena.token("group", "ethan");
    arena[group].set_attribute("type", "chain");
    arena.add_child(root, group);
    let suffix = arena.token("suffix", "oic acid");
    arena[suffix].set_attribute("infix", infix_value);
    arena[suffix].fragment = Some(fragment);
    arena.add_child(root, suffix);
    (state, arena, suffix, fragment)
}
#[test]
fn ambiguous_thio_infix_prefers_double_oxygen_and_shares_candidate_set() {
    let (mut state, mut arena, suffix, fragment) = acid_suffix("[R]C(=O)O", "=O,-O:S");
    let mut suffixes = vec![suffix];
    let mut fragments = vec![fragment];
    infix(&mut state, &mut arena, &mut suffixes, &mut fragments).unwrap();
    let graph = state.graph();
    let sulfur = *graph
        .fragment(fragment)
        .atoms
        .iter()
        .find(|&&a| graph.atom(a).element == Element::S)
        .unwrap();
    let oxygen = graph.fragment(fragment).functional_atoms[0];
    assert_eq!(graph.incoming_valency(sulfur), 2);
    assert_eq!(graph.atom(oxygen).element, Element::O);
    assert_eq!(
        graph.atom(sulfur).properties.ambiguous_element_assignment,
        [sulfur, oxygen]
    );
    assert_eq!(
        graph.atom(oxygen).properties.ambiguous_element_assignment,
        [sulfur, oxygen]
    );
    assert_eq!(
        graph
            .atom(sulfur)
            .properties
            .ambiguous_element_assignment_id,
        graph
            .atom(oxygen)
            .properties
            .ambiguous_element_assignment_id
    );
}
#[test]
fn specific_imid_infix_is_applied_before_ambiguous_thio() {
    let (mut state, mut arena, suffix, fragment) = acid_suffix("[R]C(=O)O", "=O,-O:S;=O:=N");
    let mut suffixes = vec![suffix];
    let mut fragments = vec![fragment];
    infix(&mut state, &mut arena, &mut suffixes, &mut fragments).unwrap();
    let graph = state.graph();
    let atoms = &graph.fragment(fragment).atoms;
    let nitrogen = *atoms
        .iter()
        .find(|&&a| graph.atom(a).element == Element::N)
        .unwrap();
    let sulfur = *atoms
        .iter()
        .find(|&&a| graph.atom(a).element == Element::S)
        .unwrap();
    assert_eq!(graph.incoming_valency(nitrogen), 2);
    assert_eq!(graph.incoming_valency(sulfur), 1);
    assert_eq!(graph.fragment(fragment).functional_atoms, [sulfur]);
}
#[test]
fn nitrido_infix_removes_acidic_hydroxy_and_raises_bond_order() {
    let (mut state, mut arena, suffix, fragment) = acid_suffix("[R]P(=O)O", "#O:#N");
    let mut suffixes = vec![suffix];
    let mut fragments = vec![fragment];
    infix(&mut state, &mut arena, &mut suffixes, &mut fragments).unwrap();
    let graph = state.graph();
    assert_eq!(graph.fragment(fragment).atoms.len(), 3);
    assert!(graph.fragment(fragment).functional_atoms.is_empty());
    let n = *graph
        .fragment(fragment)
        .atoms
        .iter()
        .find(|&&a| graph.atom(a).element == Element::N)
        .unwrap();
    assert_eq!(graph.incoming_valency(n), 3);
}
#[test]
fn infix_multiplier_can_multiply_the_suffix_rather_than_one_oxygen() {
    let (mut state, mut arena, suffix, fragment) = acid_suffix("[R]C=O", "=O,-O:S");
    state
        .graph_mut()
        .fragment_mut(fragment)
        .functional_atoms
        .clear();
    let root = arena[suffix].parent.unwrap();
    let multiplier = arena.token("multiplier", "di");
    arena[multiplier].set_attribute("value", "2");
    let token = arena.token("infix", "thio");
    arena.insert_before(suffix, multiplier);
    arena.insert_before(suffix, token);
    let mut suffixes = vec![suffix];
    let mut fragments = vec![fragment];
    infix(&mut state, &mut arena, &mut suffixes, &mut fragments).unwrap();
    assert_eq!(suffixes.len(), 2);
    assert_eq!(fragments.len(), 2);
    assert_eq!(arena[multiplier].parent, None);
    assert_eq!(arena[token].parent, None);
    for &f in &fragments {
        assert!(
            state
                .graph()
                .fragment(f)
                .atoms
                .iter()
                .any(|&a| state.graph().atom(a).element == Element::S)
        );
    }
    assert_eq!(arena[root].children.len(), 3);
}
#[test]
fn prefix_chalcogen_detaches_substituent_and_records_oxygen_ambiguity() {
    let mut state = BuildState::new(ParseOptions::default());
    let mut arena = Arena::default();
    let word = arena.grouping("word");
    let sub = arena.grouping("substituent");
    let root = arena.grouping("root");
    arena.add_child(word, sub);
    arena.add_child(word, root);
    let replacing = arena.token("group", "thio");
    arena[replacing].set_attribute("value", "S");
    arena[replacing].set_attribute("type", "simpleGroup");
    let prefix_frag = state
        .fragment_manager
        .build_smiles("S", "simpleGroup", "none")
        .unwrap();
    arena[replacing].fragment = Some(prefix_frag);
    arena.add_child(sub, replacing);
    let target = arena.token("group", "acetic");
    arena[target].set_attribute("type", "acidStem");
    let target_frag = state
        .fragment_manager
        .build_smiles("CC(=O)O", "acidStem", "numeric")
        .unwrap();
    arena[target].fragment = Some(target_frag);
    arena.add_child(root, target);
    let mut groups = vec![replacing, target];
    let mut substituents = vec![sub];
    assert!(prefix(&mut state, &mut arena, &mut groups, &mut substituents).unwrap());
    assert_eq!(groups, [target]);
    assert!(substituents.is_empty());
    assert_eq!(arena[sub].parent, None);
    let graph = state.graph();
    let candidates: Vec<_> = graph
        .fragment(target_frag)
        .atoms
        .iter()
        .copied()
        .filter(|&a| matches!(graph.atom(a).element, Element::O | Element::S))
        .collect();
    assert_eq!(graph.atom(candidates[0]).element, Element::S);
    assert_eq!(
        graph
            .atom(candidates[0])
            .properties
            .ambiguous_element_assignment,
        candidates
    );
}
#[test]
fn malformed_infix_returns_the_upstream_error() {
    let (mut state, mut arena, suffix, fragment) = acid_suffix("[R]C(=O)O", "=C:S");
    let error = infix(
        &mut state,
        &mut arena,
        &mut vec![suffix],
        &mut vec![fragment],
    )
    .unwrap_err();
    assert_eq!(
        error.0,
        "Only replacement by oxygen is supported. Check infix defintions"
    );
    assert_eq!(state.graph().atom(AtomId(2)).element, Element::O);
}

fn prefix_case(
    name: &str,
    prefix_smiles: &str,
    prefix_subtype: &str,
    acid_smiles: &str,
    acid_type: &str,
    functional_indices: &[usize],
) -> (
    BuildState,
    Arena,
    opsin::parse_tree::NodeId,
    opsin::parse_tree::NodeId,
    opsin::parse_tree::NodeId,
    opsin::graph::FragmentId,
) {
    let mut state = BuildState::new(ParseOptions::default());
    let mut arena = Arena::default();
    let word = arena.grouping("word");
    let sub = arena.grouping("substituent");
    let root = arena.grouping("root");
    arena.add_child(word, sub);
    arena.add_child(word, root);
    let replacement = arena.token("group", name);
    arena[replacement].set_attribute("value", prefix_smiles);
    arena[replacement].set_attribute("subType", prefix_subtype);
    let prefix_frag = state
        .fragment_manager
        .build_smiles(prefix_smiles, "simpleGroup", "none")
        .unwrap();
    arena[replacement].fragment = Some(prefix_frag);
    arena.add_child(sub, replacement);
    let target = arena.token("group", "acid");
    arena[target].set_attribute("type", acid_type);
    let f = state
        .fragment_manager
        .build_smiles(acid_smiles, acid_type, "numeric")
        .unwrap();
    arena[target].fragment = Some(f);
    arena.add_child(root, target);
    for &i in functional_indices {
        let atom = state.graph().fragment(f).atoms[i];
        state
            .graph_mut()
            .fragment_mut(f)
            .functional_atoms
            .push(atom);
    }
    (state, arena, sub, replacement, target, f)
}
#[test]
fn peroxy_prefix_inserts_oxygen_at_both_hydroxy_and_etheric_sites() {
    for (acid, indices, expected_atoms) in [("CC(=O)O", vec![3], 5), ("COC", vec![], 4)] {
        let (mut state, mut arena, sub, replacement, target, f) =
            prefix_case("peroxy", "OO", "", acid, "acidStem", &indices);
        assert!(
            prefix(
                &mut state,
                &mut arena,
                &mut vec![replacement, target],
                &mut vec![sub]
            )
            .unwrap()
        );
        let graph = state.graph();
        assert_eq!(graph.fragment(f).atoms.len(), expected_atoms);
        assert!(
            graph
                .fragment(f)
                .bonds
                .iter()
                .any(|&b| graph.atom(graph.bond(b).from).element == Element::O
                    && graph.atom(graph.bond(b).to).element == Element::O)
        );
        assert_eq!(graph.fragment(f).functional_atoms.len(), indices.len());
    }
}
#[test]
fn dedicated_imido_prefix_prefers_bridging_oxygen() {
    let (mut state, mut arena, sub, replacement, target, f) = prefix_case(
        "imido",
        "=N",
        "dedicatedFunctionalReplacementPrefix",
        "OP(=O)OP(=O)O",
        "nonCarboxylicAcid",
        &[0, 6],
    );
    assert!(
        prefix(
            &mut state,
            &mut arena,
            &mut vec![replacement, target],
            &mut vec![sub]
        )
        .unwrap()
    );
    let graph = state.graph();
    let nitrogen = *graph
        .fragment(f)
        .atoms
        .iter()
        .find(|&&a| graph.atom(a).element == Element::N)
        .unwrap();
    assert_eq!(graph.neighbours(nitrogen).len(), 2);
    assert!(
        graph
            .neighbours(nitrogen)
            .iter()
            .all(|&a| graph.atom(a).element == Element::P)
    );
    assert_eq!(graph.fragment(f).functional_atoms.len(), 2);
}
#[test]
fn chloride_prefix_uses_available_substitution_hydrogen_before_replacement() {
    for (acid, replace, indices) in [
        ("P(=O)(O)O", false, vec![2, 3]),
        ("P(=O)(O)(O)O", true, vec![2, 3, 4]),
    ] {
        let (mut state, mut arena, sub, replacement, target, f) = prefix_case(
            "chloro",
            "-Cl",
            "halideOrPseudoHalide",
            acid,
            "nonCarboxylicAcid",
            &indices,
        );
        let performed = prefix(
            &mut state,
            &mut arena,
            &mut vec![replacement, target],
            &mut vec![sub],
        )
        .unwrap();
        assert_eq!(performed, replace);
        assert_eq!(
            state
                .graph()
                .fragment(f)
                .atoms
                .iter()
                .filter(|&&a| state.graph().atom(a).element == Element::Cl)
                .count(),
            usize::from(replace)
        );
    }
}

fn acid_class_case(
    acid_smiles: &str,
    functional_indices: &[usize],
    class_name: &str,
    class_smiles: &str,
    count: usize,
    full: bool,
) -> (
    BuildState,
    Arena,
    opsin::parse_tree::NodeId,
    opsin::parse_tree::NodeId,
    opsin::graph::FragmentId,
) {
    let mut state = BuildState::new(ParseOptions::default());
    let mut arena = Arena::default();
    let rule = arena.grouping("wordRule");
    arena[rule].set_attribute("wordRule", "acidReplacingFunctionalGroup");
    let acid_word = arena.grouping("word");
    arena[acid_word].set_attribute("type", "full");
    arena.add_child(rule, acid_word);
    let root = arena.grouping("root");
    arena.add_child(acid_word, root);
    let group = arena.token("group", "acid");
    arena[group].set_attribute("type", "acidStem");
    arena.add_child(root, group);
    let f = state
        .fragment_manager
        .build_smiles(acid_smiles, "acidStem", "numeric")
        .unwrap();
    arena[group].fragment = Some(f);
    for &i in functional_indices {
        let atom = state.graph().fragment(f).atoms[i];
        state
            .graph_mut()
            .fragment_mut(f)
            .functional_atoms
            .push(atom);
    }
    let class_word = arena.grouping("word");
    arena[class_word].set_attribute("type", if full { "full" } else { "functionalTerm" });
    arena.add_child(rule, class_word);
    let term = arena.grouping(if full { "root" } else { "functionalTerm" });
    arena.add_child(class_word, term);
    if count > 1 {
        let multiplier = arena.token("multiplier", "di");
        arena[multiplier].set_attribute("value", count.to_string());
        arena.add_child(term, multiplier);
    }
    let replacing = arena.token(if full { "group" } else { "functionalGroup" }, class_name);
    arena[replacing].set_attribute("value", class_smiles);
    arena[replacing].set_attribute("labels", "numeric");
    arena.add_child(term, replacing);
    if full {
        let replacement = state
            .fragment_manager
            .build_smiles(class_smiles, "", "numeric")
            .unwrap();
        arena[replacing].fragment = Some(replacement);
    }
    (state, arena, root, acid_word, f)
}
#[test]
fn acid_class_amide_changes_all_requested_functional_oxygens_to_nitrogen() {
    let (mut state, mut arena, root, word, f) =
        acid_class_case("OC(=O)C(=O)O", &[0, 5], "amide", "N", 2, false);
    opsin::functional_replacement::process_acid_replacing_functional_class_nomenclature(
        &mut state, &mut arena, root, word,
    )
    .unwrap();
    let graph = state.graph();
    assert!(graph.fragment(f).functional_atoms.is_empty());
    assert_eq!(graph.atom(AtomId(0)).element, Element::N);
    assert_eq!(graph.atom(AtomId(5)).element, Element::N);
}
#[test]
fn full_word_amide_neutralises_replacement_and_preserves_interfragment_connection() {
    let (mut state, mut arena, root, word, f) =
        acid_class_case("CC(=O)O", &[3], "amide", "[NH2-]", 1, true);
    opsin::functional_replacement::process_acid_replacing_functional_class_nomenclature(
        &mut state, &mut arena, root, word,
    )
    .unwrap();
    let graph = state.graph();
    let n = AtomId(4);
    assert_eq!(graph.atom(n).charge, 0);
    assert_eq!(graph.neighbours(n), [AtomId(1)]);
    assert!(graph.fragment(f).functional_atoms.is_empty());
    assert_eq!(
        graph.fragment(graph.atom(n).fragment).locants.get("N"),
        Some(&n)
    );
    assert_eq!(graph.atom(n).locants, ["4"]);
}
#[test]
fn acid_class_dihydrazide_copies_replacement_before_incorporation() {
    let (mut state, mut arena, root, word, f) =
        acid_class_case("OC(=O)C(=O)O", &[0, 5], "hydrazide", "NN", 2, false);
    opsin::functional_replacement::process_acid_replacing_functional_class_nomenclature(
        &mut state, &mut arena, root, word,
    )
    .unwrap();
    let graph = state.graph();
    assert!(graph.fragment(f).functional_atoms.is_empty());
    assert_eq!(graph.fragment(f).atoms.len(), 8);
    assert_eq!(
        graph
            .fragment(f)
            .atoms
            .iter()
            .filter(|&&a| graph.atom(a).element == Element::N)
            .count(),
        4
    );
}
