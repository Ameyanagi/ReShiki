//! Post-component fixtures exercise OPSIN's omitted-space decisions separately
//! from fragment construction. No graph facts are inferred from name text.
use opsin::{
    ParsingError,
    graph::{Element, FragmentId},
    parse_tree::{Arena, NodeId, ParseTree},
    word_rules_omitted_space::{
        FragmentFacts, OmittedSpaceContext, OutAtomFacts, correct_omitted_spaces,
    },
};
use std::collections::BTreeMap;

#[derive(Default)]
struct Context {
    facts: BTreeMap<usize, FragmentFacts>,
    clone_count: usize,
}
impl OmittedSpaceContext for Context {
    fn fragment_facts(&self, fragment: FragmentId) -> Result<FragmentFacts, ParsingError> {
        self.facts
            .get(&fragment.0)
            .cloned()
            .ok_or_else(|| ParsingError("Missing fixture fragment".into()))
    }
    fn clone_element(
        &mut self,
        arena: &mut Arena,
        element: NodeId,
    ) -> Result<NodeId, ParsingError> {
        self.clone_count += 1;
        let clone = arena.copy(element);
        let original = arena.descendants(element);
        let copies = arena.descendants(clone);
        for (original, copy) in original.into_iter().zip(copies) {
            arena[copy].fragment = arena[original].fragment;
        }
        Ok(clone)
    }
}
fn facts(out: Vec<(Element, u32)>, functional: usize, hydrogens: &[&str]) -> FragmentFacts {
    FragmentFacts {
        out_atoms: out
            .into_iter()
            .map(|(element, valency)| OutAtomFacts { element, valency })
            .collect(),
        functional_atom_count: functional,
        has_default_in_atom: false,
        substitutable_hydrogen_environments: hydrogens.iter().map(|s| (*s).into()).collect(),
    }
}
fn skeleton(rule: &str, value: &str, word_type: &str) -> (ParseTree, NodeId, NodeId) {
    let mut arena = Arena::default();
    let root = arena.grouping("molecule");
    let word_rule = arena.grouping("wordRule");
    arena[word_rule].add_attribute("wordRule", rule);
    arena[word_rule].add_attribute("value", value);
    arena.add_child(root, word_rule);
    let word = arena.grouping("word");
    arena[word].add_attribute("type", word_type);
    arena.add_child(word_rule, word);
    (ParseTree { arena, root }, word_rule, word)
}
fn chunk(
    tree: &mut ParseTree,
    word: NodeId,
    tag: &str,
    value: &str,
    fragment: usize,
    group_type: &str,
    sub_type: &str,
) -> NodeId {
    let chunk = tree.arena.grouping(tag);
    tree.arena.add_child(word, chunk);
    let group = tree.arena.token("group", value);
    tree.arena[group].fragment = Some(FragmentId(fragment));
    tree.arena[group].add_attribute("type", group_type);
    tree.arena[group].add_attribute("subType", sub_type);
    tree.arena.add_child(chunk, group);
    chunk
}
fn ether(locanted: bool, out_valency: u32) -> (ParseTree, NodeId, Context) {
    let (mut tree, rule, word) = skeleton(
        "divalentFunctionalGroup",
        "methylethyl ether",
        "substituent",
    );
    let first = chunk(
        &mut tree,
        word,
        "substituent",
        "methyl",
        0,
        "chain",
        "alkaneStem",
    );
    chunk(
        &mut tree,
        word,
        "substituent",
        "ethyl",
        1,
        "chain",
        "alkaneStem",
    );
    if locanted {
        tree.arena[first].add_attribute("locant", "2");
    }
    let mut context = Context::default();
    context
        .facts
        .insert(0, facts(vec![(Element::C, out_valency)], 0, &[]));
    (tree, rule, context)
}

#[test]
fn divalent_rule_splits_two_unlocanted_carbon_radicals_only() {
    let (mut tree, rule, mut context) = ether(false, 1);
    correct_omitted_spaces(&mut tree, &mut context).unwrap();
    assert_eq!(tree.arena.children_named(rule, "word").len(), 2);
    for (locanted, valency) in [(true, 1), (false, 2)] {
        let (mut tree, rule, mut context) = ether(locanted, valency);
        correct_omitted_spaces(&mut tree, &mut context).unwrap();
        assert_eq!(tree.arena.children_named(rule, "word").len(), 1);
    }
}

fn ester(
    sub_name: &str,
    root_name: &str,
    root_hydrogens: &[&str],
    default_in: bool,
) -> (ParseTree, NodeId, NodeId, Context) {
    let (mut tree, rule, word) = skeleton("simple", "potentialate", "full");
    let sub = chunk(
        &mut tree,
        word,
        "substituent",
        sub_name,
        0,
        "simpleGroup",
        "simpleGroup",
    );
    let hyphen = tree.arena.token("hyphen", "-");
    tree.arena.add_child(sub, hyphen);
    chunk(
        &mut tree,
        word,
        "root",
        root_name,
        1,
        "simpleGroup",
        "simpleGroup",
    );
    let mut context = Context::default();
    context
        .facts
        .insert(0, facts(vec![(Element::C, 1)], 0, &[]));
    let mut root = facts(vec![], 2, root_hydrogens);
    root.has_default_in_atom = default_in;
    context.facts.insert(1, root);
    (tree, rule, sub, context)
}

#[test]
fn ester_ambiguity_uses_default_attachment_and_stereo_environments() {
    for (environments, default_in, is_ester) in [
        (vec!["a", "b"], false, true),
        (vec!["a", "a"], false, false),
        (vec!["a", "b"], true, false),
        (vec!["a"], false, false),
    ] {
        let (mut tree, rule, sub, mut context) =
            ester("phenyl", "benzoate", &environments, default_in);
        correct_omitted_spaces(&mut tree, &mut context).unwrap();
        assert_eq!(
            tree.arena[rule].attribute("wordRule"),
            Some(if is_ester { "ester" } else { "simple" })
        );
        if is_ester {
            assert_eq!(tree.arena.children_named(rule, "word").len(), 2);
            assert!(
                tree.arena
                    .last_child(sub)
                    .is_some_and(|id| tree.arena[id].name != "hyphen")
            );
        }
    }
}

#[test]
fn multiplied_ester_clones_fragments_and_removes_multiplier() {
    let (mut tree, rule, sub, mut context) =
        ester("phenyl", "benzoate", &["a", "b", "c", "d"], false);
    tree.arena[sub].add_attribute("multiplier", "2");
    correct_omitted_spaces(&mut tree, &mut context).unwrap();
    assert_eq!(tree.arena[rule].attribute("wordRule"), Some("ester"));
    assert_eq!(context.clone_count, 1);
    assert_eq!(tree.arena.children_named(rule, "word").len(), 3);
    assert_eq!(tree.arena[sub].attribute("multiplier"), None);
    assert_eq!(
        tree.arena
            .descendants_named(rule, "group")
            .iter()
            .filter(|&&id| tree.arena[id].fragment == Some(FragmentId(0)))
            .count(),
        2
    );
}

#[test]
fn locanted_prefix_can_move_with_next_unlocanted_group() {
    let (mut tree, rule, first, mut context) = ester("chloro", "benzoate", &["a", "b"], false);
    tree.arena[first].add_attribute("locant", "4");
    let word = tree.arena[first].parent.unwrap();
    let second = chunk(
        &mut tree,
        word,
        "substituent",
        "phenyl",
        2,
        "simpleGroup",
        "simpleGroup",
    );
    tree.arena.detach(second);
    tree.arena.insert_after(first, second);
    context
        .facts
        .insert(2, facts(vec![(Element::C, 1)], 0, &[]));
    correct_omitted_spaces(&mut tree, &mut context).unwrap();
    assert_eq!(tree.arena[rule].attribute("wordRule"), Some("ester"));
    let ester_word = tree.arena.children_named(rule, "word")[0];
    assert_eq!(tree.arena[ester_word].children, [first, second]);
}

#[test]
fn insufficient_root_hydrogen_and_locants_prefer_ester() {
    for locanted in [true, false] {
        let (mut tree, rule, first, mut context) = ester("phenyl", "benzoate", &["a"], true);
        let word = tree.arena[first].parent.unwrap();
        let second = chunk(
            &mut tree,
            word,
            "substituent",
            "methyl",
            2,
            "simpleGroup",
            "simpleGroup",
        );
        tree.arena.detach(second);
        tree.arena.insert_after(first, second);
        if locanted {
            tree.arena[second].add_attribute("locant", "2");
        }
        context
            .facts
            .insert(2, facts(vec![(Element::C, 1)], 0, &[]));
        correct_omitted_spaces(&mut tree, &mut context).unwrap();
        assert_eq!(tree.arena[rule].attribute("wordRule"), Some("ester"));
    }
}

#[test]
fn ester_count_limits_and_silicon_radicals_follow_upstream() {
    let (mut tree, rule, sub, mut context) = ester("silyl", "benzoate", &["a", "b"], false);
    context.facts.get_mut(&0).unwrap().out_atoms[0].element = Element::Si;
    correct_omitted_spaces(&mut tree, &mut context).unwrap();
    assert_eq!(tree.arena[rule].attribute("wordRule"), Some("ester"));
    assert_eq!(tree.arena[sub].attribute("multiplier"), None);

    let (mut tree, rule, sub, mut context) = ester("phenyl", "benzoate", &["a", "b"], false);
    tree.arena[sub].add_attribute("multiplier", "3");
    correct_omitted_spaces(&mut tree, &mut context).unwrap();
    assert_eq!(tree.arena[rule].attribute("wordRule"), Some("simple"));

    let (mut tree, rule, sub, mut context) = ester("phenyl", "benzoate", &["a", "b"], false);
    let word = tree.arena[sub].parent.unwrap();
    let root = tree.arena.children_named(word, "root")[0];
    tree.arena[root].add_attribute("multiplier", "2");
    correct_omitted_spaces(&mut tree, &mut context).unwrap();
    assert_eq!(tree.arena[rule].attribute("wordRule"), Some("simple"));
}
