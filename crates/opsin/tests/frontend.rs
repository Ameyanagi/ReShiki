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
    assert_eq!(process_stoichiometry_indication("(+1:+2)").unwrap(), [1, 2]);
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
fn compound_with_separates_word_rules() {
    let parser = Parser::new().unwrap();
    for delimiter in ["compound with", "compd. with", "and", "COMPOUND WITH"] {
        let name = format!("methanol {delimiter} acetate");
        let trees = parser.parse_trees(&name, &ParseOptions::default()).unwrap();
        assert!(!trees.is_empty());
        for tree in trees {
            assert_eq!(
                tree.arena.children_named(tree.root, "wordRule").len(),
                2,
                "{name}"
            );
        }
    }
}

#[test]
fn compound_with_allows_multiword_components() {
    let parser = Parser::new().unwrap();
    let name = "ethyl acetate compound with methyl acetate compound with methanol";
    for tree in parser.parse_trees(name, &ParseOptions::default()).unwrap() {
        assert_eq!(tree.arena.children_named(tree.root, "wordRule").len(), 3);
    }
    let tree = parser
        .parse_trees("methanol acetate", &ParseOptions::default())
        .unwrap()
        .remove(0);
    assert_eq!(tree.arena.children_named(tree.root, "wordRule").len(), 1);
}

#[test]
fn semicolon_separates_word_rules() {
    let parser = Parser::new().unwrap();
    for name in [
        "methanol; acetate",
        "ethyl acetate; methyl acetate; methanol",
    ] {
        for tree in parser.parse_trees(name, &ParseOptions::default()).unwrap() {
            assert_eq!(
                tree.arena.children_named(tree.root, "wordRule").len(),
                name.split("; ").count()
            );
        }
    }
}

#[test]
fn non_positive_component_ratios_are_rejected() {
    for ratio in ["(0:1)", "(1:0)", "[1/0]", "(-1:1)", "(1:-2)"] {
        let error = process_stoichiometry_indication(ratio).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Component ratios must be at least 1"),
            "{ratio}: {error}"
        );
    }
}

#[test]
fn invalid_integer_component_ratios_are_rejected() {
    for ratio in [
        "(1:2147483648)",
        "(1:99999999999999999999)",
        "(1:1.5)",
        "(1:abc)",
    ] {
        let error = process_stoichiometry_indication(ratio).unwrap_err();
        assert!(
            error
                .to_string()
                .starts_with("Invalid stoichiometry ratio:"),
            "{ratio}: {error}"
        );
    }
}

#[test]
fn parser_rejects_invalid_stoichiometry() {
    let parser = Parser::new().unwrap();
    for ratio in ["(1:0)", "(1:2147483648)"] {
        let name = format!("methanol compound with ethanol {ratio}");
        assert!(
            parser.parse_trees(&name, &ParseOptions::default()).is_err(),
            "{name}"
        );
    }
}

#[test]
fn tokenizer_rejects_semicolon_after_substituent() {
    let parser = Parser::new().unwrap();
    for remove_spaces in [false, true] {
        assert!(
            !parser
                .tokenize("ethyl; benzene", remove_spaces)
                .unwrap()
                .is_successful()
        );
    }
}

#[test]
fn spaced_hyphen_after_substituent_requires_space_removal() {
    let parser = Parser::new().unwrap();
    let result = parser.tokenize("ethyl - benzene", false).unwrap();
    assert!(!result.is_successful());
    assert_eq!(result.unparsed_name, "ethyl - benzene");
    assert!(result.words.is_empty());
}

#[test]
fn spaced_hyphen_after_substituent_does_not_end_a_component() {
    let parser = Parser::new().unwrap();
    let result = parser.tokenize("ethyl - benzene", true).unwrap();
    assert!(result.is_successful());
    assert_eq!(result.words.len(), 1);
    assert_eq!(result.words[0].word, "ethyl-benzene");
    assert!(!result.words[0].ends_component);
}

#[test]
fn semicolon_after_full_word_ends_a_component() {
    assert_methanol_benzene_component_boundary("methanol; benzene");
}

#[test]
fn spaced_hyphen_after_full_word_ends_a_component() {
    assert_methanol_benzene_component_boundary("methanol - benzene");
}

fn assert_methanol_benzene_component_boundary(name: &str) {
    let parser = Parser::new().unwrap();
    for remove_spaces in [false, true] {
        let result = parser.tokenize(name, remove_spaces).unwrap();
        assert!(result.is_successful());
        assert_eq!(result.words.len(), 2);
        assert_eq!(result.words[0].word, "methanol");
        assert_eq!(result.words[1].word, "benzene");
        assert!(result.words[0].ends_component);
        assert!(!result.words[1].ends_component);
    }
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
