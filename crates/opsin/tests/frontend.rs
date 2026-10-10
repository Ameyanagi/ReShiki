//! Java-free cases derived from pinned OPSIN ParserTest and tree contracts.
use opsin::{
    ParseOptions, Parser,
    frontend::process_stoichiometry_indication,
    graph::FragmentId,
    parse_tree::{Arena, ParseTree, sort_parses, xml_encode},
};

#[test]
fn parser_upstream_cases() {
    let parser = Parser::new().unwrap();
    let options = ParseOptions::default();
    assert!(parser.parse_trees("chunky bacon", &options).is_err());
    assert!(
        !parser
            .parse_trees("Piperidine, 1-(1-oxopropyl)-", &options)
            .unwrap()
            .is_empty()
    );
    let trees = parser.parse_trees("benzene; ethane", &options).unwrap();
    assert_eq!(
        trees[0]
            .arena
            .children_named(trees[0].root, "wordRule")
            .len(),
        2
    );
    assert!(parser.parse_trees("chloro", &options).is_err());
    assert!(
        parser
            .parse_trees("pyridine salt", &options)
            .unwrap_err()
            .to_string()
            .contains("only contained one component")
    );
}

#[test]
fn upstream_component_ratio_examples_and_precise_mismatch() {
    assert_eq!(process_stoichiometry_indication("(1:2)").unwrap(), [1, 2]);
    assert_eq!(
        process_stoichiometry_indication("[1/1/2]").unwrap(),
        [1, 1, 2]
    );
    assert_eq!(
        process_stoichiometry_indication("(1:2:?)").unwrap(),
        [1, 2, 1]
    );
    assert_eq!(
        process_stoichiometry_indication("(-1:+2)").unwrap(),
        [-1, 2]
    );
    assert!(
        process_stoichiometry_indication("(1:2/3)")
            .unwrap_err()
            .to_string()
            .contains("Unexpected /")
    );
    let parser = Parser::new().unwrap();
    let tree = parser
        .parse_trees("benzene; ethane (1:2)", &ParseOptions::default())
        .unwrap()
        .remove(0);
    let rules = tree.arena.children_named(tree.root, "wordRule");
    assert_eq!(tree.arena[rules[0]].attribute("stoichiometry"), Some("1"));
    assert_eq!(tree.arena[rules[1]].attribute("stoichiometry"), Some("2"));
    assert!(
        parser
            .parse_trees("benzene (1:2)", &ParseOptions::default())
            .unwrap_err()
            .to_string()
            .contains("1 components but 2 ratios")
    );
}

#[test]
fn ordered_word_rule_rewrites_and_ionic_special_cases() {
    let parser = Parser::new().unwrap();
    for (name, rules) in [
        ("ethyl acetate", vec!["ester"]),
        ("ethyl methyl ether", vec!["divalentFunctionalGroup"]),
        ("sodium chloride", vec!["simple", "simple"]),
        ("aluminium chloride", vec!["additionCompound"]),
        ("titanium tetrachloride", vec!["additionCompound"]),
        ("sodium dioxide", vec!["simple", "simple"]),
        ("acetone O-methyloxime", vec!["carbonylDerivative"]),
        ("methyl amide", vec!["simple"]),
        ("dihydrogen", vec!["simple"]),
    ] {
        let trees = parser.parse_trees(name, &ParseOptions::default()).unwrap();
        let found = trees.iter().any(|tree| {
            tree.arena
                .children_named(tree.root, "wordRule")
                .iter()
                .map(|&id| tree.arena[id].attribute("wordRule").unwrap())
                .collect::<Vec<_>>()
                == rules
        });
        assert!(
            found,
            "{name}: {:?}",
            trees.iter().map(ParseTree::to_xml).collect::<Vec<_>>()
        );
    }
    let trees = parser
        .parse_trees("sodium dioxide", &ParseOptions::default())
        .unwrap();
    assert!(trees.iter().any(|tree| {
        tree.arena
            .descendants_named(tree.root, "group")
            .iter()
            .any(|&id| tree.arena[id].attribute("value") == Some("[O-][O-]"))
    }));
}

#[test]
fn radicals_and_implicit_hyphens_follow_upstream_options() {
    let parser = Parser::new().unwrap();
    let radical = ParseOptions {
        allow_radicals: true,
        ..Default::default()
    };
    let trees = parser.parse_trees("methyl", &radical).unwrap();
    let rule = trees[0]
        .arena
        .first_child_named(trees[0].root, "wordRule")
        .unwrap();
    assert_eq!(
        trees[0].arena[rule].attribute("wordRule"),
        Some("substituent")
    );
    let trees = parser
        .parse_trees("1-chloro 2-bromo ethane", &ParseOptions::default())
        .unwrap();
    assert!(
        trees
            .iter()
            .all(|tree| tree.arena.children_named(tree.root, "wordRule").len() == 1)
    );
    assert!(
        trees
            .iter()
            .any(|tree| tree.arena.descendants_named(tree.root, "hyphen").len() >= 2)
    );
}

#[test]
fn arena_preserves_identity_order_duplicate_attributes_and_copy_semantics() {
    let mut arena = Arena::default();
    let root = arena.grouping("root");
    let first = arena.token("group", "a&b");
    let second = arena.token("suffix", "c");
    arena[first].add_attribute("type", "one");
    arena[first].add_attribute("type", "two");
    arena[first].fragment = Some(FragmentId(7));
    arena.add_child(root, first);
    arena.add_child(root, second);
    assert_eq!(arena.value(root), "a&bc");
    let copy = arena.copy(root);
    assert_eq!(arena[copy].parent, None);
    let first_copy = arena[copy].children[0];
    assert_eq!(arena[first_copy].attribute("type"), Some("one"));
    assert_eq!(arena[first_copy].fragment, None);
    assert_eq!(arena[first].fragment, Some(FragmentId(7)));
    arena.detach(first);
    arena.insert_after(second, first);
    assert_eq!(arena[root].children, [second, first]);
    assert_eq!(arena[first].parent, Some(root));
    assert!(arena.to_xml(copy).contains("type=\"one\" type=\"two\""));
    assert_eq!(
        xml_encode("\t\n\r\"&<>'"),
        "&#x09;&#x0A;&#x0D;&quot;&amp;&lt;&gt;'"
    );
}

#[test]
fn sort_prefers_non_radical_then_fewer_leaves_then_fewer_elements() {
    fn tree(rule: &str, leaves: usize, extra_group: bool) -> ParseTree {
        let mut arena = Arena::default();
        let root = arena.grouping("molecule");
        let wr = arena.grouping("wordRule");
        arena[wr].add_attribute("wordRule", rule);
        arena.add_child(root, wr);
        let parent = if extra_group {
            let group = arena.grouping("word");
            arena.add_child(wr, group);
            group
        } else {
            wr
        };
        for _ in 0..leaves {
            let token = arena.token("group", "");
            arena.add_child(parent, token);
        }
        ParseTree { arena, root }
    }
    let mut trees = vec![
        tree("substituent", 1, false),
        tree("simple", 2, false),
        tree("simple", 1, true),
        tree("simple", 1, false),
    ];
    sort_parses(&mut trees);
    assert_eq!(trees[0].arena.counts(trees[0].root), (2, 1));
    assert_eq!(trees[1].arena.counts(trees[1].root), (3, 1));
    assert_eq!(trees[2].arena.counts(trees[2].root), (3, 2));
    assert_eq!(
        trees[3].arena[trees[3].arena.children_named(trees[3].root, "wordRule")[0]]
            .attribute("wordRule"),
        Some("substituent")
    );
}
