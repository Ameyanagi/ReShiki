// OPSIN 2.9.0 ComponentProcessor implicit-bracket phase fixtures.
// Tree outcomes follow source lines4191–4650 and the cited examples there.
// No Java, remote service, or chemistry fallback is used by these tests.
use opsin::{
    ParseOptions,
    build_state::BuildState,
    component_processor_brackets::implicitly_bracket_to_previous_substituent_if_appropriate,
    graph::FragmentId,
    parse_tree::{Arena, NodeId},
};

struct Fixture {
    state: BuildState,
    arena: Arena,
    word: NodeId,
    brackets: Vec<NodeId>,
}
impl Fixture {
    fn new() -> Self {
        let mut arena = Arena::default();
        let word = arena.grouping("word");
        Self {
            state: BuildState::new(ParseOptions::default()),
            arena,
            word,
            brackets: Vec::new(),
        }
    }
    fn group(
        &mut self,
        container: NodeId,
        text: &str,
        smiles: &str,
        kind: &str,
        subtype: &str,
        labels: &str,
    ) -> NodeId {
        let group = self.arena.token("group", text);
        self.arena[group].add_attribute("type", kind);
        self.arena[group].add_attribute("subType", subtype);
        self.arena.add_child(container, group);
        let fragment = self
            .state
            .fragment_manager
            .build_token_smiles(smiles, &mut self.arena, group, labels)
            .unwrap();
        self.arena[group].fragment = Some(fragment);
        group
    }
    fn sub(
        &mut self,
        text: &str,
        smiles: &str,
        kind: &str,
        subtype: &str,
        joiner: bool,
    ) -> (NodeId, NodeId) {
        let substituent = self.arena.grouping("substituent");
        self.arena.add_child(self.word, substituent);
        let group = self.group(substituent, text, smiles, kind, subtype, "numeric");
        if joiner {
            self.arena[group].add_attribute("usableAsAJoiner", "yes");
        }
        (substituent, group)
    }
    fn alkyl(&mut self, text: &str, smiles: &str, joiner: bool) -> (NodeId, NodeId) {
        let (sub, group) = self.sub(text, smiles, "chain", "alkaneStem", joiner);
        self.token(sub, "suffix", "yl", Some("value"), Some("yl"));
        (sub, group)
    }
    fn amino(&mut self) -> (NodeId, NodeId) {
        self.sub("amino", "N", "substituent", "simpleGroup", true)
    }
    fn root(&mut self, smiles: &str) -> NodeId {
        let root = self.arena.grouping("root");
        self.arena.add_child(self.word, root);
        self.group(root, "root", smiles, "chain", "alkaneStem", "numeric");
        self.token(root, "unsaturator", "ane", Some("value"), Some("1"));
        root
    }
    fn token(
        &mut self,
        parent: NodeId,
        name: &str,
        value: &str,
        attribute: Option<&str>,
        attribute_value: Option<&str>,
    ) -> NodeId {
        let token = self.arena.token(name, value);
        if let (Some(attribute), Some(attribute_value)) = (attribute, attribute_value) {
            self.arena[token].add_attribute(attribute, attribute_value);
        }
        self.arena.add_child(parent, token);
        token
    }
    fn prepend(&mut self, parent: NodeId, name: &str, value: &str) -> NodeId {
        let token = self.arena.token(name, value);
        self.arena.insert_child(parent, token, 0);
        token
    }
    fn multiply(&mut self, sub: NodeId, count: &str, kind: &str, index: usize) -> NodeId {
        let multiplier = self.arena.token("multiplier", count);
        self.arena[multiplier].add_attribute("value", count);
        self.arena[multiplier].add_attribute("type", kind);
        self.arena.insert_child(sub, multiplier, index);
        multiplier
    }
    fn apply(&mut self, sub: NodeId) {
        implicitly_bracket_to_previous_substituent_if_appropriate(
            &mut self.state,
            &mut self.arena,
            sub,
            &mut self.brackets,
        )
        .unwrap();
    }
    fn fragment(&self, group: NodeId) -> FragmentId {
        self.arena[group].fragment.unwrap()
    }
}

#[test]
fn methyl_amino_link_is_bracketed_before_following_root() {
    let mut f = Fixture::new();
    let (methyl, _) = f.alkyl("methyl", "C", false);
    let (amino, _) = f.amino();
    let root = f.root("CCC");
    f.apply(amino);
    let bracket = f.brackets[0];
    assert_eq!(f.arena[bracket].attribute("type"), Some("implicit"));
    assert_eq!(f.arena[f.word].children, [bracket, root]);
    assert_eq!(f.arena[bracket].children, [methyl, amino]);
}

#[test]
fn leading_locant_multiplier_or_stereochemistry_on_joiner_prevents_bracket() {
    for (name, text) in [
        ("locant", "2"),
        ("multiplier", "2"),
        ("stereoChemistry", "R"),
    ] {
        let mut f = Fixture::new();
        f.alkyl("methyl", "C", false);
        let (amino, _) = f.amino();
        f.prepend(amino, name, text);
        f.root("CCC");
        let before = f.arena.to_xml(f.word);
        f.apply(amino);
        assert_eq!(f.arena.to_xml(f.word), before);
        assert!(f.brackets.is_empty());
    }
}

#[test]
fn joiner_attribute_and_adjacent_substituent_are_required() {
    let mut f = Fixture::new();
    let (amino, _) = f.amino();
    f.root("CCC");
    f.apply(amino);
    assert!(f.brackets.is_empty());
    let mut f = Fixture::new();
    f.alkyl("methyl", "C", false);
    let (amino, group) = f.amino();
    f.arena[group].remove_attribute("usableAsAJoiner");
    f.root("CCC");
    f.apply(amino);
    assert!(f.brackets.is_empty());
}

#[test]
fn hyphen_suppresses_ordinary_joiner_but_carbonyl_still_brackets() {
    for carbonyl in [false, true] {
        let mut f = Fixture::new();
        let (methyl, _) = f.alkyl("methyl", "C", false);
        f.token(methyl, "hyphen", "-", None, None);
        let (joiner, group) = if carbonyl {
            f.sub("carbonyl", "-C(=O)-", "substituent", "simpleGroup", true)
        } else {
            f.amino()
        };
        if carbonyl {
            f.arena[group].add_attribute("acceptsAdditiveBonds", "yes");
        }
        f.root("CCC");
        f.apply(joiner);
        assert_eq!(f.brackets.len(), usize::from(carbonyl));
    }
}

#[test]
fn substituent_between_two_explicit_brackets_remains_separate() {
    let mut f = Fixture::new();
    let (left, _) = f.alkyl("benzyl", "C", false);
    let left_bracket = f.arena.grouping("bracket");
    f.arena.detach(left);
    f.arena.add_child(left_bracket, left);
    f.arena.add_child(f.word, left_bracket);
    let (joiner, _) = f.amino();
    let (right, _) = f.alkyl("phenyl", "C", false);
    let right_bracket = f.arena.grouping("bracket");
    f.arena.detach(right);
    f.arena.add_child(right_bracket, right);
    f.arena.add_child(f.word, right_bracket);
    f.apply(joiner);
    assert!(f.brackets.is_empty());
    assert_eq!(
        f.arena[f.word].children,
        [left_bracket, joiner, right_bracket]
    );
}

#[test]
fn end_to_end_alkyls_remain_unbracketed_but_locanted_alkyl_can_join() {
    for locanted in [false, true] {
        let mut f = Fixture::new();
        let (methyl, _) = f.alkyl("methyl", "C", false);
        if locanted {
            f.prepend(methyl, "locant", "2");
        }
        let (ethyl, _) = f.alkyl("ethyl", "CC", true);
        f.root("CCC");
        f.apply(ethyl);
        assert_eq!(f.brackets.len(), usize::from(locanted));
        if locanted {
            assert_eq!(
                f.arena[methyl].children.len(),
                3,
                "locant 2 stays on methyl and attaches it to ethyl"
            );
        }
    }
}

#[test]
fn heteroatom_locanted_simple_alkyls_do_not_join() {
    let mut f = Fixture::new();
    let (ethyl, _) = f.alkyl("ethyl", "CC", false);
    f.prepend(ethyl, "locant", "N,N");
    f.multiply(ethyl, "2", "basic", 1);
    let (methyl, _) = f.alkyl("methyl", "C", true);
    f.root("P");
    f.apply(methyl);
    assert!(f.brackets.is_empty());
}

#[test]
fn chain_suffix_out_atom_allows_a_non_alkyl_extension_to_bracket() {
    let mut f = Fixture::new();
    let (alkoxy, group) = f.alkyl("ethyl", "CC", false);
    let yl = f.arena.next_sibling_named(group, "suffix").unwrap();
    f.arena[yl].set_value("oxy");
    let fragment = f
        .state
        .fragment_manager
        .build_token_smiles("O-", &mut f.arena, yl, "none")
        .unwrap();
    f.arena[yl].fragment = Some(fragment);
    assert_eq!(
        f.state
            .fragment_manager
            .graph
            .fragment(fragment)
            .out_atoms
            .len(),
        1
    );
    let (ethyl, _) = f.alkyl("ethyl", "CC", true);
    f.root("CCC");
    f.apply(ethyl);
    assert_eq!(f.arena[f.brackets[0]].children, [alkoxy, ethyl]);
}

#[test]
fn multiradicals_without_additive_or_imino_attributes_do_not_bracket() {
    for preceding in [false, true] {
        let mut f = Fixture::new();
        let (_, before_group) = f.alkyl("methyl", "C", false);
        let (amino, group) = f.amino();
        f.arena[if preceding { before_group } else { group }]
            .add_attribute("isAMultiRadical", "yes");
        f.root("CCC");
        f.apply(amino);
        assert!(f.brackets.is_empty());
    }
}

#[test]
fn unsubstitutable_multiradical_respects_multiplicative_root() {
    for count in ["2", "3"] {
        let mut f = Fixture::new();
        f.alkyl("methyl", "C", false);
        let (sulfonyl, group) = f.sub(
            "sulfonyl",
            "-S(=O)(=O)-",
            "substituent",
            "simpleGroup",
            true,
        );
        f.arena[group].add_attribute("isAMultiRadical", "yes");
        f.arena[group].add_attribute("acceptsAdditiveBonds", "yes");
        let root = f.root("C");
        f.multiply(root, count, "basic", 0);
        f.apply(sulfonyl);
        assert_eq!(f.brackets.len(), usize::from(count != "2"));
    }
}

#[test]
fn two_imino_like_groups_remain_available_for_additive_operation() {
    let mut f = Fixture::new();
    let (_, before) = f.sub("imino", "N", "substituent", "simpleGroup", false);
    let (imino, group) = f.amino();
    f.arena[before].add_attribute("iminoLike", "yes");
    f.arena[group].add_attribute("iminoLike", "yes");
    f.root("CCC");
    f.apply(imino);
    assert!(f.brackets.is_empty());
}

#[test]
fn sulfanylidene_loses_joiner_attribute_without_bracketing() {
    let mut f = Fixture::new();
    f.alkyl("methyl", "C", false);
    let (sulf, group) = f.sub("sulf", "S", "substituent", "simpleGroup", true);
    f.token(sulf, "unsaturator", "an", Some("value"), Some("1"));
    f.token(sulf, "suffix", "ylidene", Some("value"), Some("ylidene"));
    f.root("CCC");
    f.apply(sulf);
    assert!(f.arena[group].attribute("usableAsAJoiner").is_none());
    assert!(f.brackets.is_empty());
}

#[test]
fn perhalogeno_terms_only_join_the_simple_alkyl_alkane_case_when_unlocanted() {
    for simple_alkane in [false, true] {
        let mut f = Fixture::new();
        f.sub("perfluoro", "F", "substituent", "perhalogeno", false);
        let (ethyl, _) = f.alkyl("ethyl", "CC", true);
        let root = f.root("CCC");
        if !simple_alkane {
            f.token(root, "suffix", "ol", Some("value"), Some("ol"));
        }
        f.apply(ethyl);
        assert_eq!(f.brackets.len(), usize::from(simple_alkane));
    }
}

#[test]
fn silyl_collects_three_preceding_substituents() {
    let mut f = Fixture::new();
    let (ethoxy, _) = f.sub("ethoxy", "CCO", "substituent", "simpleGroup", false);
    let (methyl1, _) = f.alkyl("methyl", "C", false);
    let (methyl2, _) = f.alkyl("methyl", "C", false);
    let (silyl, _) = f.sub("silyl", "[SiH3]-", "substituent", "simpleGroup", true);
    f.root("CCC");
    f.apply(silyl);
    assert_eq!(
        f.arena[f.brackets[0]].children,
        [ethoxy, methyl1, methyl2, silyl]
    );
}

#[test]
fn phosphoryl_collects_two_preceding_substituents() {
    let mut f = Fixture::new();
    let (ethoxy, _) = f.sub("ethoxy", "CCO", "substituent", "simpleGroup", false);
    let (methyl, _) = f.alkyl("methyl", "C", false);
    let (phosphoryl, group) = f.sub("phosphoryl", "P=O", "substituent", "simpleGroup", true);
    f.arena[group].add_attribute("acceptsAdditiveBonds", "yes");
    let fragment = f.fragment(group);
    let atom = f.state.fragment_manager.graph.fragment(fragment).atoms[0];
    for _ in 0..3 {
        f.state
            .fragment_manager
            .graph
            .add_out_atom(fragment, atom, 1, true);
    }
    f.root("CCC");
    f.apply(phosphoryl);
    assert_eq!(
        f.arena[f.brackets[0]].children,
        [ethoxy, methyl, phosphoryl]
    );
}

#[test]
fn stereo_always_moves_to_bracket_and_external_locant_moves_with_it() {
    let mut f = Fixture::new();
    let (methyl, _) = f.alkyl("methyl", "C", false);
    let locant = f.prepend(methyl, "locant", "4");
    let stereo = f.prepend(methyl, "stereoChemistry", "R");
    let (amino, _) = f.amino();
    f.root("CCCC");
    f.apply(amino);
    assert_eq!(
        f.arena[f.brackets[0]].children,
        [stereo, locant, methyl, amino]
    );
}

#[test]
fn one_atom_joiner_locant_can_stay_when_no_potential_root_has_it() {
    let mut f = Fixture::new();
    let (methyl, _) = f.alkyl("methyl", "C", false);
    let locant = f.prepend(methyl, "locant", "1");
    let (amino, _) = f.amino();
    let root = f.root("C");
    let root_group = f.arena.first_child_named(root, "group").unwrap();
    let root_atom = f
        .state
        .fragment_manager
        .graph
        .fragment(f.fragment(root_group))
        .atoms[0];
    f.state.fragment_manager.graph.clear_locants(root_atom);
    f.apply(amino);
    assert_eq!(f.arena[f.brackets[0]].children, [methyl, amino]);
    assert_eq!(f.arena[locant].parent, Some(methyl));
}

#[test]
fn multi_locants_and_group_multiplier_move_together() {
    let mut f = Fixture::new();
    let (methyl, _) = f.alkyl("methyl", "C", false);
    let locant = f.prepend(methyl, "locant", "2,5");
    let multiplier = f.multiply(methyl, "2", "group", 1);
    let (amino, _) = f.amino();
    f.root("CCCCC");
    f.apply(amino);
    assert_eq!(
        f.arena[f.brackets[0]].children,
        [locant, multiplier, methyl, amino]
    );
}

#[test]
fn complex_basic_multiplier_can_prevent_an_ambiguous_implicit_bracket() {
    let mut f = Fixture::new();
    let (amine, _) = f.sub("amino", "N", "substituent", "simpleGroup", false);
    f.prepend(amine, "locant", "3,4");
    f.multiply(amine, "2", "basic", 1);
    let (ethyl, _) = f.alkyl("ethyl", "CC", true);
    f.root("CCCC");
    f.apply(ethyl);
    assert!(f.brackets.is_empty());
}

#[test]
fn ortho_meta_para_locant_is_reduced_before_bracket_movement() {
    let mut f = Fixture::new();
    let (methyl, _) = f.alkyl("methyl", "C", false);
    let locant = f.prepend(methyl, "locant", "1,4");
    f.arena[locant].add_attribute("type", "orthoMetaPara");
    let multiplier = f.multiply(methyl, "2", "basic", 1);
    let (amino, _) = f.amino();
    f.root("CCCC");
    f.apply(amino);
    assert_eq!(f.arena.value(locant), "4");
    assert_eq!(f.arena[f.brackets[0]].children, [locant, methyl, amino]);
    assert_eq!(f.arena[multiplier].parent, Some(methyl));
}

#[test]
fn unlocanted_group_multiplier_moves_before_whole_bracket() {
    let mut f = Fixture::new();
    let (methyl, _) = f.alkyl("methyl", "C", false);
    let multiplier = f.multiply(methyl, "3", "group", 0);
    let (amino, _) = f.amino();
    f.root("P");
    f.apply(amino);
    assert_eq!(f.arena[f.brackets[0]].children, [multiplier, methyl, amino]);
}

#[test]
fn locanted_terminal_additive_group_retains_needed_connections() {
    for earlier_matches in [false, true] {
        let mut f = Fixture::new();
        if earlier_matches {
            let (earlier, _) = f.alkyl("methyl", "C", false);
            f.prepend(earlier, "locant", "S");
        }
        let (methyl, _) = f.alkyl("methyl", "C", false);
        f.prepend(methyl, "locant", "N");
        let (amino, _) = f.amino();
        let (_, additive) = f.sub("sulfonimidoyl", "S", "substituent", "simpleGroup", false);
        f.arena[additive].add_attribute("acceptsAdditiveBonds", "yes");
        let atom = f
            .state
            .fragment_manager
            .graph
            .fragment(f.fragment(additive))
            .atoms[0];
        f.state.fragment_manager.graph.add_locant(atom, "S");
        f.apply(amino);
        assert_eq!(f.brackets.len(), usize::from(earlier_matches));
    }
}

#[test]
fn locanted_amine_root_prevents_bracket_which_would_hide_the_locant() {
    let mut f = Fixture::new();
    let (methyl, _) = f.alkyl("methyl", "C", false);
    f.prepend(methyl, "locant", "N");
    let (amino, _) = f.amino();
    let root = f.arena.grouping("root");
    f.arena.add_child(f.word, root);
    f.group(root, "amine", "N", "simpleGroup", "simpleGroup", "none");
    f.apply(amino);
    assert!(f.brackets.is_empty());
}

#[test]
fn ester_like_word_rules_allow_terminal_two_substituent_bracket() {
    for rule in [
        "ester",
        "functionalClassEster",
        "multiEster",
        "acetal",
        "simple",
    ] {
        let mut f = Fixture::new();
        let (methyl, _) = f.alkyl("methyl", "C", false);
        let (amino, _) = f.amino();
        f.state.current_word_rule = Some(rule.into());
        f.apply(amino);
        assert_eq!(f.brackets.len(), usize::from(rule != "simple"));
        if rule != "simple" {
            assert_eq!(f.arena[f.brackets[0]].children, [methyl, amino]);
        }
    }
}

#[test]
fn dimethyl_ethyl_silyl_exception_avoids_alkyl_bracket() {
    let mut f = Fixture::new();
    let (methyl, _) = f.alkyl("methyl", "C", false);
    f.prepend(methyl, "locant", "1,1");
    f.multiply(methyl, "2", "basic", 1);
    let (ethyl, _) = f.alkyl("ethyl", "CC", true);
    f.sub("silyl", "[SiH3]-", "substituent", "simpleGroup", false);
    f.apply(ethyl);
    assert!(f.brackets.is_empty());
}

#[test]
fn missing_preceding_group_reports_phase_invariant_failure() {
    let mut f = Fixture::new();
    let before = f.arena.grouping("substituent");
    f.token(before, "suffix", "yl", Some("value"), Some("yl"));
    f.arena.add_child(f.word, before);
    let (amino, _) = f.amino();
    f.root("CCC");
    let failure = implicitly_bracket_to_previous_substituent_if_appropriate(
        &mut f.state,
        &mut f.arena,
        amino,
        &mut f.brackets,
    )
    .unwrap_err();
    assert_eq!(failure.to_string(), "No group where group was expected");
}
