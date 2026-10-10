// OPSIN 2.9.0 SuffixApplier source-grounded phase fixtures.
// Expected graph edits and exact failure text follow SuffixApplier.java and
// the embedded suffix rules at commit b91b610af5ab07560fedb20730d7aef46bb2bca0.
// These tests require no Java installation or network.

use opsin::ParseOptions;
use opsin::build_state::BuildState;
use opsin::graph::{AtomId, Element, FragmentId};
use opsin::parse_tree::{Arena, NodeId};
use opsin::suffix_applier::SuffixApplier;
use opsin::suffix_rules::SuffixRules;

fn parent(
    smiles: &str,
    group_type: &str,
    subtype: &str,
) -> (BuildState, Arena, NodeId, FragmentId) {
    let mut state = BuildState::new(ParseOptions::default());
    let mut arena = Arena::default();
    let root = arena.grouping("root");
    let group = arena.token("group", "parent");
    arena[group].add_attribute("type", group_type);
    arena[group].add_attribute("subType", subtype);
    arena.add_child(root, group);
    let fragment = state
        .fragment_manager
        .build_token_smiles(smiles, &mut arena, group, "numeric")
        .unwrap();
    arena[group].fragment = Some(fragment);
    (state, arena, group, fragment)
}

fn suffix(
    state: &mut BuildState,
    arena: &mut Arena,
    group: NodeId,
    value: &str,
    smiles: Option<&str>,
    labels: &str,
) -> NodeId {
    let suffix = arena.token("suffix", value);
    arena[suffix].add_attribute("value", value);
    arena[suffix].add_attribute("type", "suffix");
    arena.add_child(arena[group].parent.unwrap(), suffix);
    if let Some(smiles) = smiles {
        let fragment = state
            .fragment_manager
            .build_token_smiles(smiles, arena, suffix, labels)
            .unwrap();
        arena[suffix].fragment = Some(fragment);
    }
    suffix
}

fn resolve(
    state: &mut BuildState,
    arena: &mut Arena,
    group: NodeId,
    suffixes: &[NodeId],
) -> Result<(), opsin::ParsingError> {
    let rules = SuffixRules::new().unwrap();
    SuffixApplier::new(state, &rules).resolve_suffixes(arena, group, suffixes)
}

#[test]
fn alcoholic_suffix_replaces_dummy_bond_and_preserves_parent_locants() {
    let (mut state, mut arena, group, fragment) = parent("CC", "chain", "alkaneStem");
    let ol = suffix(&mut state, &mut arena, group, "ol", Some("[*]O"), "/O1");
    let suffix_fragment = arena[ol].fragment.unwrap();
    state.xml_suffix_map.insert(group, vec![suffix_fragment]);
    resolve(&mut state, &mut arena, group, &[ol]).unwrap();
    assert_eq!(
        state.fragment_manager.graph.fragment(fragment).atoms,
        [AtomId(0), AtomId(1), AtomId(3)]
    );
    assert_eq!(
        state.fragment_manager.graph.neighbours(AtomId(0)),
        [AtomId(1), AtomId(3)]
    );
    assert_eq!(
        state.fragment_manager.graph.atom_by_locant(fragment, "1"),
        Some(AtomId(0))
    );
    assert_eq!(
        state.fragment_manager.graph.atom_by_locant(fragment, "O1"),
        Some(AtomId(3))
    );
    assert!(!state.fragment_manager.graph.atom(AtomId(2)).active);
    assert!(arena[ol].fragment.is_none());
    assert!(state.xml_suffix_map[&group].is_empty());
    assert!(state.warnings.is_empty());
}

#[test]
fn ketone_preference_uses_internal_saturated_carbon() {
    let (mut state, mut arena, group, fragment) = parent("CCCC", "chain", "alkaneStem");
    let one = suffix(&mut state, &mut arena, group, "one", Some("[*]=O"), "none");
    resolve(&mut state, &mut arena, group, &[one]).unwrap();
    let oxygen = *state
        .fragment_manager
        .graph
        .fragment(fragment)
        .atoms
        .last()
        .unwrap();
    let bond = state
        .fragment_manager
        .graph
        .bond_between(AtomId(1), oxygen)
        .unwrap();
    assert_eq!(state.fragment_manager.graph.bond(bond).order, 2);
    assert!(
        state.warnings.is_empty(),
        "the two internal butane carbons are equivalent"
    );
}

#[test]
fn repeated_suffix_values_reuse_ordered_substitution_candidates() {
    let (mut state, mut arena, group, fragment) = parent("CCC", "chain", "alkaneStem");
    let first = suffix(&mut state, &mut arena, group, "ol", Some("[*]O"), "none");
    let second = suffix(&mut state, &mut arena, group, "ol", Some("[*]O"), "none");
    resolve(&mut state, &mut arena, group, &[first, second]).unwrap();
    let oxygen_atoms: Vec<_> = state
        .fragment_manager
        .graph
        .fragment(fragment)
        .atoms
        .iter()
        .copied()
        .filter(|&id| state.fragment_manager.graph.atom(id).element == Element::O)
        .collect();
    assert_eq!(oxygen_atoms.len(), 2);
    assert_eq!(
        state.fragment_manager.graph.neighbours(oxygen_atoms[0]),
        [AtomId(0)]
    );
    assert_eq!(
        state.fragment_manager.graph.neighbours(oxygen_atoms[1]),
        [AtomId(0)]
    );
    assert_eq!(state.warnings.len(), 1);
    assert_eq!(
        state.warnings[0].message,
        "Addition of ol suffix to: parent"
    );
}

#[test]
fn locant_id_is_global_one_based_and_not_fragment_ordinal() {
    let (mut state, mut arena, group, fragment) = parent("CC", "chain", "alkaneStem");
    let ol = suffix(&mut state, &mut arena, group, "ol", Some("[*]O"), "none");
    arena[ol].add_attribute("locantID", "2");
    resolve(&mut state, &mut arena, group, &[ol]).unwrap();
    let oxygen = *state
        .fragment_manager
        .graph
        .fragment(fragment)
        .atoms
        .last()
        .unwrap();
    assert_eq!(state.fragment_manager.graph.neighbours(oxygen), [AtomId(1)]);

    let (mut state, mut arena, group, _) = parent("CC", "chain", "alkaneStem");
    let extra = state
        .fragment_manager
        .build_smiles("C", "chain", "numeric")
        .unwrap();
    let ol = suffix(&mut state, &mut arena, group, "ol", Some("[*]O"), "none");
    let foreign_id = state.fragment_manager.graph.fragment(extra).atoms[0].0 + 1;
    arena[ol].add_attribute("locantID", foreign_id.to_string());
    assert_eq!(
        resolve(&mut state, &mut arena, group, &[ol])
            .unwrap_err()
            .to_string(),
        format!("Couldn't find atom with id {foreign_id}.")
    );
}

#[test]
fn default_locant_id_and_explicit_locant_override_unlocanted_choice() {
    for (attribute, value, expected) in [
        ("defaultLocantID", "3", AtomId(2)),
        ("locant", "2", AtomId(1)),
    ] {
        let (mut state, mut arena, group, _) = parent("CCC", "chain", "alkaneStem");
        let ol = suffix(&mut state, &mut arena, group, "ol", Some("[*]O"), "none");
        arena[ol].add_attribute(attribute, value);
        resolve(&mut state, &mut arena, group, &[ol]).unwrap();
        assert_eq!(
            state.fragment_manager.graph.neighbours(AtomId(4)),
            [expected]
        );
        assert!(state.warnings.is_empty());
    }
}

#[test]
fn radical_suffix_can_reference_previous_suffix_atoms_after_deferred_merge() {
    let (mut state, mut arena, group, fragment) = parent("CC", "chain", "alkaneStem");
    let ol = suffix(&mut state, &mut arena, group, "ol", Some("[*]O"), "/O1");
    let yl = suffix(&mut state, &mut arena, group, "yl", None, "none");
    arena[yl].add_attribute("locant", "O1");
    resolve(&mut state, &mut arena, group, &[ol, yl]).unwrap();
    let out = &state.fragment_manager.graph.fragment(fragment).out_atoms[0];
    assert_eq!(
        (out.atom, out.valency, out.explicitly_set),
        (AtomId(3), 1, true)
    );
    assert_eq!(state.fragment_manager.graph.atom(AtomId(3)).out_valency, 1);
}

#[test]
fn suffix_locants_support_amino_acid_style_heteroatom_lookup() {
    let (mut state, mut arena, group, fragment) = parent("CC(N)C", "chain", "alkaneStem");
    state.fragment_manager.graph.clear_locants(AtomId(2));
    state.fragment_manager.graph.add_locant(AtomId(2), "N");
    let yl = suffix(&mut state, &mut arena, group, "yl", None, "none");
    arena[yl].add_attribute("locant", "N2");
    resolve(&mut state, &mut arena, group, &[yl]).unwrap();
    assert_eq!(
        state.fragment_manager.graph.fragment(fragment).out_atoms[0].atom,
        AtomId(2)
    );
}

#[test]
fn unlocanted_and_locanted_radical_valencies_follow_rules() {
    for (value, valency) in [("yl", 1), ("ylidene", 2), ("ylidyne", 3)] {
        let (mut state, mut arena, group, fragment) = parent("CCC", "chain", "alkaneStem");
        let yl = suffix(&mut state, &mut arena, group, value, None, "none");
        resolve(&mut state, &mut arena, group, &[yl]).unwrap();
        let out = &state.fragment_manager.graph.fragment(fragment).out_atoms[0];
        assert_eq!(
            (out.atom, out.valency, out.explicitly_set),
            (AtomId(0), valency, false)
        );
        assert_eq!(state.fragment_manager.graph.atom(AtomId(0)).out_valency, 0);
    }
}

#[test]
fn aldehyde_property_distinguishes_formaldehyde_and_carbaldehyde() {
    for (smiles, suffix_smiles, labels, expected) in [
        ("C", "[*]=O", "none", None),
        ("CC", "[*]=O", "none", Some(AtomId(0))),
        ("C1CCCCC1", "[*]C=O", "/X/", Some(AtomId(7))),
    ] {
        let (mut state, mut arena, group, _) = parent(smiles, "chain", "alkaneStem");
        let al = suffix(
            &mut state,
            &mut arena,
            group,
            "al",
            Some(suffix_smiles),
            labels,
        );
        arena[al].add_attribute("locant", "1");
        resolve(&mut state, &mut arena, group, &[al]).unwrap();
        let aldehydes: Vec<_> = state
            .fragment_manager
            .graph
            .atoms
            .iter()
            .filter(|a| a.active && a.properties.is_aldehyde)
            .map(|a| a.id)
            .collect();
        assert_eq!(aldehydes, expected.into_iter().collect::<Vec<_>>());
    }
}

#[test]
fn charge_suffix_prefers_neutral_nitrogen_then_other_heteroatoms() {
    for (smiles, expected) in [("CON", AtomId(2)), ("CO", AtomId(1)), ("CC", AtomId(0))] {
        let (mut state, mut arena, group, _) = parent(smiles, "chain", "alkaneStem");
        let ium = suffix(&mut state, &mut arena, group, "ium", None, "none");
        resolve(&mut state, &mut arena, group, &[ium]).unwrap();
        assert_eq!(state.fragment_manager.graph.atom(expected).charge, 1);
        assert_eq!(
            state
                .fragment_manager
                .graph
                .atom(expected)
                .protons_explicitly_added_or_removed,
            1
        );
        assert_eq!(
            state
                .fragment_manager
                .graph
                .atoms
                .iter()
                .filter(|a| a.charge != 0)
                .count(),
            1
        );
    }
}

#[test]
fn amino_acid_charge_prefers_alpha_nitrogen_without_ambiguity() {
    let (mut state, mut arena, group, _) = parent("NCCN", "aminoAcid", "endInIne");
    let ium = suffix(&mut state, &mut arena, group, "ium", None, "none");
    resolve(&mut state, &mut arena, group, &[ium]).unwrap();
    assert_eq!(state.fragment_manager.graph.atom(AtomId(0)).charge, 1);
    assert_eq!(state.fragment_manager.graph.atom(AtomId(3)).charge, 0);
    assert!(state.warnings.is_empty());
}

#[test]
fn charge_warning_uses_fragment_token_when_group_is_an_alias() {
    let (mut state, mut arena, group, fragment) = parent("NCCN(C)C", "chain", "heteroStem");
    let ium = suffix(&mut state, &mut arena, group, "ium", None, "none");
    let alias = arena.token("group", "alias");
    arena[alias].fragment = Some(fragment);
    resolve(&mut state, &mut arena, alias, &[ium]).unwrap();
    assert_eq!(state.warnings.len(), 1);
    assert_eq!(
        state.warnings[0].message,
        "Addition of charge suffix to: parent"
    );
}

#[test]
fn deprotonating_charge_suffix_skips_atoms_without_substitutable_hydrogen() {
    let (mut state, mut arena, group, _) = parent("N(C)(C)CCN", "chain", "heteroStem");
    let ide = suffix(&mut state, &mut arena, group, "ide", None, "none");
    resolve(&mut state, &mut arena, group, &[ide]).unwrap();
    assert_eq!(state.fragment_manager.graph.atom(AtomId(0)).charge, 0);
    assert_eq!(state.fragment_manager.graph.atom(AtomId(5)).charge, -1);
    assert_eq!(
        state
            .fragment_manager
            .graph
            .atom(AtomId(5))
            .protons_explicitly_added_or_removed,
        -1
    );
}

#[test]
fn prefixed_acylium_modifies_the_suffix_side_of_interfragment_bond() {
    let (mut state, mut arena, group, fragment) = parent("C1CCCCC1", "chain", "alkaneStem");
    let acylium = suffix(
        &mut state,
        &mut arena,
        group,
        "acylium",
        Some("[*]C=O"),
        "none",
    );
    arena[acylium].add_attribute("suffixPrefix", "C");
    arena[acylium].add_attribute("locant", "1");
    resolve(&mut state, &mut arena, group, &[acylium]).unwrap();
    assert_eq!(state.fragment_manager.graph.atom(AtomId(7)).charge, 1);
    assert_eq!(
        state
            .fragment_manager
            .graph
            .atom(AtomId(7))
            .protons_explicitly_added_or_removed,
        -1
    );
    assert_eq!(state.fragment_manager.graph.atom(AtomId(0)).charge, 0);
    assert_eq!(
        state.fragment_manager.graph.fragment(fragment).atoms.len(),
        8
    );
}

#[test]
fn prefixed_oyl_sets_attachment_on_carbonyl_suffix_atom() {
    let (mut state, mut arena, group, fragment) = parent("C1CCCCC1", "chain", "alkaneStem");
    let oyl = suffix(&mut state, &mut arena, group, "oyl", Some("[*]C=O"), "none");
    arena[oyl].add_attribute("suffixPrefix", "C");
    arena[oyl].add_attribute("locant", "1");
    resolve(&mut state, &mut arena, group, &[oyl]).unwrap();
    let out = &state.fragment_manager.graph.fragment(fragment).out_atoms[0];
    assert_eq!(
        (out.atom, out.valency, out.explicitly_set),
        (AtomId(7), 1, true)
    );
}

#[test]
fn prefixed_suffix_rejects_more_than_one_interfragment_bond() {
    let (mut state, mut arena, group, _) = parent("CC", "chain", "alkaneStem");
    let acylium = suffix(
        &mut state,
        &mut arena,
        group,
        "acylium",
        Some("[*](=O)N"),
        "none",
    );
    arena[acylium].add_attribute("suffixPrefix", "C");
    arena[acylium].add_attribute("locant", "1");
    assert_eq!(
        resolve(&mut state, &mut arena, group, &[acylium])
            .unwrap_err()
            .to_string(),
        "OPSIN bug: Wrong number of bonds between suffix and group"
    );
}

#[test]
fn acid_stem_suffix_uses_first_parent_atom_explicitly() {
    let (mut state, mut arena, group, fragment) = parent("CC", "acidStem", "ylForAcyl");
    let oyl = suffix(&mut state, &mut arena, group, "oyl", Some("[*]=O"), "none");
    resolve(&mut state, &mut arena, group, &[oyl]).unwrap();
    assert_eq!(
        state.fragment_manager.graph.neighbours(AtomId(3)),
        [AtomId(0)]
    );
    assert_eq!(
        state.fragment_manager.graph.fragment(fragment).out_atoms[0].atom,
        AtomId(0)
    );
    assert!(state.fragment_manager.graph.fragment(fragment).out_atoms[0].explicitly_set);
}

#[test]
fn numeric_suffix_locants_are_removed_only_when_parent_has_them() {
    let (mut state, mut arena, group, fragment) = parent("CC", "chain", "alkaneStem");
    let amine = suffix(&mut state, &mut arena, group, "amine", Some("[*]N"), "/2,N");
    resolve(&mut state, &mut arena, group, &[amine]).unwrap();
    assert_eq!(
        state.fragment_manager.graph.atom_by_locant(fragment, "2"),
        Some(AtomId(1))
    );
    assert_eq!(
        state.fragment_manager.graph.atom_by_locant(fragment, "N"),
        Some(AtomId(3))
    );
    assert_eq!(state.fragment_manager.graph.atom(AtomId(3)).locants, ["N"]);
}

#[test]
fn acidic_element_swap_preserves_functional_atom_and_exchanges_locants() {
    let (mut state, mut arena, group, fragment) = parent("CC", "acidStem", "ylForAcyl");
    let acid = suffix(
        &mut state,
        &mut arena,
        group,
        "ic_S_acid",
        Some("[*](=S)O"),
        "/S/O",
    );
    let sf = arena[acid].fragment.unwrap();
    let assignment = vec![AtomId(3), AtomId(4)];
    state
        .fragment_manager
        .graph
        .fragment_mut(sf)
        .functional_atoms
        .push(AtomId(4));
    state
        .fragment_manager
        .graph
        .set_ambiguous_element_assignment(&assignment, assignment.clone());
    resolve(&mut state, &mut arena, group, &[acid]).unwrap();
    assert_eq!(
        state.fragment_manager.graph.atom(AtomId(3)).element,
        Element::O
    );
    assert_eq!(
        state.fragment_manager.graph.atom(AtomId(4)).element,
        Element::S
    );
    assert_eq!(
        state.fragment_manager.graph.atom_by_locant(fragment, "S"),
        Some(AtomId(4))
    );
    assert_eq!(
        state.fragment_manager.graph.atom_by_locant(fragment, "O"),
        Some(AtomId(3))
    );
    assert_eq!(
        state
            .fragment_manager
            .graph
            .fragment(fragment)
            .functional_atoms,
        [AtomId(4)]
    );
    assert!(
        state
            .fragment_manager
            .graph
            .atom(AtomId(3))
            .properties
            .ambiguous_element_assignment
            .is_empty()
    );
    assert!(
        state
            .fragment_manager
            .graph
            .atom(AtomId(4))
            .properties
            .ambiguous_element_assignment
            .is_empty()
    );
}

#[test]
fn copied_ambiguous_element_sets_are_independent_despite_equal_members() {
    let (mut state, mut arena, group, _) = parent("CC", "acidStem", "ylForAcyl");
    let original = suffix(
        &mut state,
        &mut arena,
        group,
        "ic_S_acid",
        Some("[*](=S)O"),
        "/S/O",
    );
    let original_fragment = arena[original].fragment.unwrap();
    let assignment = vec![AtomId(3), AtomId(4)];
    state
        .fragment_manager
        .graph
        .fragment_mut(original_fragment)
        .functional_atoms
        .push(AtomId(4));
    state
        .fragment_manager
        .graph
        .set_ambiguous_element_assignment(&assignment, assignment.clone());
    let copy_fragment = state
        .fragment_manager
        .copy_fragment(original_fragment)
        .unwrap();
    let copy = suffix(&mut state, &mut arena, group, "ic_S_acid", None, "none");
    arena[copy].fragment = Some(copy_fragment);
    resolve(&mut state, &mut arena, group, &[copy]).unwrap();
    // FragmentManager copies each atom's Java Set separately. Acid selection
    // removes members only from the functional atom's set in this copy.
    assert_eq!(
        state
            .fragment_manager
            .graph
            .atom(AtomId(6))
            .properties
            .ambiguous_element_assignment,
        [AtomId(6), AtomId(7)]
    );
    assert!(
        state
            .fragment_manager
            .graph
            .atom(AtomId(7))
            .properties
            .ambiguous_element_assignment
            .is_empty()
    );
    assert_eq!(
        state
            .fragment_manager
            .graph
            .atom(AtomId(3))
            .properties
            .ambiguous_element_assignment,
        assignment
    );
}

#[test]
fn numeric_locant_collision_check_uses_java_decimal_digit_category() {
    for (locant, expected) in [("١", AtomId(0)), ("Ⅳ", AtomId(2)), ("𝟙", AtomId(2))] {
        let (mut state, mut arena, group, fragment) = parent("C", "chain", "alkaneStem");
        state.fragment_manager.graph.add_locant(AtomId(0), locant);
        let ol = suffix(
            &mut state,
            &mut arena,
            group,
            "ol",
            Some("[*]O"),
            &format!("/{locant}"),
        );
        resolve(&mut state, &mut arena, group, &[ol]).unwrap();
        assert_eq!(
            state
                .fragment_manager
                .graph
                .atom_by_locant(fragment, locant),
            Some(expected),
            "locant {locant}"
        );
    }
}

#[test]
fn explicit_acidic_element_without_replacement_candidates_fails_exactly() {
    let (mut state, mut arena, group, _) = parent("CC", "acidStem", "ylForAcyl");
    let acid = suffix(
        &mut state,
        &mut arena,
        group,
        "ic_S_acid",
        Some("[*](=O)O"),
        "none",
    );
    let sf = arena[acid].fragment.unwrap();
    state
        .fragment_manager
        .graph
        .fragment_mut(sf)
        .functional_atoms
        .push(AtomId(4));
    assert_eq!(
        resolve(&mut state, &mut arena, group, &[acid])
            .unwrap_err()
            .to_string(),
        "Unable to find potential acidic atom with element: S"
    );
}

#[test]
fn phase_failure_messages_match_upstream_validation() {
    let (mut state, mut arena, group, _) = parent("C", "chain", "alkaneStem");
    let ol = suffix(&mut state, &mut arena, group, "ol", Some("[*]"), "none");
    assert_eq!(
        resolve(&mut state, &mut arena, group, &[ol])
            .unwrap_err()
            .to_string(),
        "OPSIN Bug: Dummy atom in suffix should have at least one bond to it"
    );

    let (mut state, mut arena, group, _) = parent("C", "chain", "alkaneStem");
    let ol = suffix(&mut state, &mut arena, group, "ol", Some("[*]O"), "none");
    arena[ol].add_attribute("locant", "99");
    assert_eq!(
        resolve(&mut state, &mut arena, group, &[ol])
            .unwrap_err()
            .to_string(),
        "Could not find the atom with locant 99."
    );

    let (mut state, mut arena, group, _) = parent("C(F)(F)(F)F", "chain", "alkaneStem");
    let one = suffix(&mut state, &mut arena, group, "one", Some("[*]=O"), "none");
    assert_eq!(
        resolve(&mut state, &mut arena, group, &[one])
            .unwrap_err()
            .to_string(),
        "No suitable atom found to attach one suffix"
    );
}
