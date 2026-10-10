//! Java-free integration of pinned resources, ComponentGenerator and ordered
//! ComponentProcessor. Assertions inspect construction identities and metadata.
use opsin::build_state::BuildState;
use opsin::component_generator::{ComponentGenerationContext, process_components as generate};
use opsin::component_processor::process_components as process;
use opsin::graph::{Element, FragmentId};
use opsin::parse_tree::ParseTree;
use opsin::suffix_rules::SuffixRules;
use opsin::{ParseOptions, Parser};

fn processed(name: &str) -> (ParseTree, BuildState, FragmentId) {
    let parser = Parser::new().unwrap();
    let options = ParseOptions::default();
    let rules = SuffixRules::new().unwrap();
    let trees = parser.parse_trees(name, &options).unwrap();
    let mut failures = Vec::new();
    for mut tree in trees {
        let mut context = ComponentGenerationContext {
            options,
            ..Default::default()
        };
        if let Err(error) = generate(&mut tree, &mut context) {
            failures.push(error.to_string());
            continue;
        }
        let mut state = BuildState::new(options);
        state.warnings.extend(context.warnings);
        if let Err(error) = process(&mut tree, &mut state, &rules) {
            failures.push(error.to_string());
            continue;
        }
        let groups = tree.arena.descendants_named(tree.root, "group");
        let fragment = tree.arena[*groups.last().unwrap()].fragment.unwrap();
        return (tree, state, fragment);
    }
    panic!("{name} failed component processing: {failures:?}")
}
#[test]
fn locanted_alcohol_suffix_is_attached_to_the_indicated_carbon() {
    let (_tree, state, fragment) = processed("butan-2-ol");
    let graph = state.graph();
    let carbon = graph.atom_by_locant(fragment, "2").unwrap();
    let oxygen = *graph
        .fragment(fragment)
        .atoms
        .iter()
        .find(|&&a| graph.atom(a).element == Element::O)
        .unwrap();
    assert_eq!(graph.neighbours(oxygen), [carbon]);
}
#[test]
fn multiplied_terminal_acid_suffixes_preserve_both_functional_atoms() {
    let (_tree, state, fragment) = processed("butanedioic acid");
    let graph = state.graph();
    assert_eq!(graph.fragment(fragment).functional_atoms.len(), 2);
    let terminal: Vec<_> = graph
        .fragment(fragment)
        .functional_atoms
        .iter()
        .map(|&o| graph.neighbours(o)[0])
        .collect();
    assert!(terminal.contains(&graph.atom_by_locant(fragment, "1").unwrap()));
    assert!(terminal.contains(&graph.atom_by_locant(fragment, "4").unwrap()));
}
#[test]
fn conjunctive_suffix_is_resolved_and_connected_to_the_ring() {
    let (tree, state, fragment) = processed("benzenemethanol");
    let graph = state.graph();
    assert_eq!(
        tree.arena
            .descendants_named(tree.root, "conjunctiveSuffixGroup")
            .len(),
        1
    );
    assert_eq!(graph.fragment(fragment).atoms.len(), 8);
    let alpha = graph.atom_by_locant(fragment, "alpha").unwrap();
    assert_eq!(graph.neighbours(alpha).len(), 2);
}
#[test]
fn infix_and_prefix_thio_passes_precede_element_locant_assignment() {
    for name in ["ethanthioic acid", "thioacetic acid"] {
        let (_tree, state, fragment) = processed(name);
        let graph = state.graph();
        assert!(
            graph
                .fragment(fragment)
                .atoms
                .iter()
                .any(|&a| graph.atom(a).element == Element::S)
        );
        assert_eq!(graph.fragment(fragment).functional_atoms.len(), 1);
        assert!(graph.atom_by_locant(fragment, "S").is_some());
    }
}
#[test]
fn early_chalcogen_suffix_resolution_clears_the_suffix_fragment_inventory() {
    for name in [
        "methylsulfonamidobenzene",
        "2-(N-(2-ethylphenyl)methylsulfonamido)-acetamide",
    ] {
        let (tree, state, _) = processed(name);
        let stems: Vec<_> = tree
            .arena
            .descendants_named(tree.root, "group")
            .into_iter()
            .filter(|&group| tree.arena[group].attribute("type") == Some("chalcogenAcidStem"))
            .collect();
        assert!(!stems.is_empty(), "{name}");
        for group in stems {
            assert!(state.xml_suffix_map[&group].is_empty(), "{name}");
        }
    }
}
#[test]
fn retained_group_front_locant_changes_xylene_attachment_position() {
    let (_tree, state, fragment) = processed("1,4-xylene");
    let graph = state.graph();
    let attached: Vec<_> = graph
        .fragment(fragment)
        .atoms
        .iter()
        .copied()
        .filter(|&a| graph.atom(a).element == Element::C && graph.neighbours(a).len() == 1)
        .map(|a| graph.neighbours(a)[0])
        .collect();
    assert_eq!(attached.len(), 2);
    assert!(attached.contains(&graph.atom_by_locant(fragment, "1").unwrap()));
    assert!(attached.contains(&graph.atom_by_locant(fragment, "4").unwrap()));
}
#[test]
fn dl_amino_acid_prefix_applies_before_suffix_resolution() {
    let (_tree, l_state, l_frag) = processed("L-alanine");
    let (_tree, d_state, d_frag) = processed("D-alanine");
    let l = l_state
        .graph()
        .fragment(l_frag)
        .atoms
        .iter()
        .find_map(|&a| l_state.graph().atom(a).parity.as_ref())
        .unwrap();
    let d = d_state
        .graph()
        .fragment(d_frag)
        .atoms
        .iter()
        .find_map(|&a| d_state.graph().atom(a).parity.as_ref())
        .unwrap();
    assert_eq!(l.parity, -d.parity);
}
