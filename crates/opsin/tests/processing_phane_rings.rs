// Native graph regressions for the ComponentProcessor/FusedRingBuilder phane
// changes in upstream commit 42e87ac6. No live upstream runtime is required.

use opsin::build_state::BuildState;
use opsin::component_processor::ComponentProcessor;
use opsin::component_processor_rings::{process_fused_ring_bridges, process_ring_assemblies};
use opsin::fused_ring_builder::process_fused_rings;
use opsin::graph::{AtomId, FragmentId};
use opsin::parse_tree::{Arena, NodeId};
use opsin::suffix_rules::SuffixRules;
use opsin::{ParseOptions, ParsingError};

fn setup() -> (BuildState, Arena, NodeId) {
    let state = BuildState::new(ParseOptions::default());
    let mut arena = Arena::default();
    let word = arena.grouping("word");
    let root = arena.grouping("root");
    arena.add_child(word, root);
    (state, arena, root)
}

fn token(
    arena: &mut Arena,
    parent: NodeId,
    name: &str,
    text: &str,
    attributes: &[(&str, &str)],
) -> NodeId {
    let node = arena.token(name, text);
    for &(key, value) in attributes {
        arena[node].add_attribute(key, value);
    }
    arena.add_child(parent, node);
    node
}

fn group(
    state: &mut BuildState,
    arena: &mut Arena,
    parent: NodeId,
    name: &str,
    smiles: &str,
) -> (NodeId, FragmentId) {
    let node = token(arena, parent, "group", name, &[("type", "ring")]);
    let fragment = state
        .fragment_manager
        .build_token_smiles(smiles, arena, node, "numeric")
        .unwrap();
    arena[node].fragment = Some(fragment);
    state.xml_suffix_map.insert(node, Vec::new());
    (node, fragment)
}

fn atom(state: &BuildState, ring: FragmentId, locant: &str) -> AtomId {
    state.graph().atom_by_locant(ring, locant).unwrap()
}

fn bonded(state: &BuildState, ring: FragmentId, first: &str, second: &str) -> bool {
    state
        .graph()
        .bond_between(atom(state, ring, first), atom(state, ring, second))
        .is_some()
}

fn assemble(state: &mut BuildState, arena: &mut Arena, root: NodeId) -> Result<(), ParsingError> {
    process_ring_assemblies(state, arena, &SuffixRules::new().unwrap(), root)
}

#[test]
fn superscript_assembly_connections_and_aliases_preserve_ring_identity() {
    for connections in ["1^1,2^1:2^4,3^1-", "1^1,2^1;2^4,3^1-"] {
        let (mut state, mut arena, root) = setup();
        let locant = token(
            &mut arena,
            root,
            "colonOrSemiColonDelimitedLocant",
            connections,
            &[],
        );
        token(
            &mut arena,
            root,
            "ringAssemblyMultiplier",
            "ter",
            &[("value", "3")],
        );
        let (_, ring) = group(&mut state, &mut arena, root, "phenyl", "c1ccccc1");
        assemble(&mut state, &mut arena, root).unwrap();
        assert_eq!(state.graph().fragment(ring).atoms.len(), 18);
        assert_eq!(state.graph().fragment(ring).bonds.len(), 20);
        assert!(bonded(&state, ring, "1^1", "2^1"));
        assert!(bonded(&state, ring, "2^4", "3^1"));
        for component in 1..=3 {
            for position in 1..=6 {
                let primed = format!("{position}{}", "'".repeat(component - 1));
                let superscript = format!("{component}^{position}");
                let atom = atom(&state, ring, &primed);
                assert_eq!(
                    atom,
                    state.graph().atom_by_locant(ring, &superscript).unwrap()
                );
                assert_eq!(state.graph().atom(atom).locants, [primed, superscript]);
            }
        }
        assert!(arena[locant].parent.is_none());
    }
}

#[test]
fn primed_ring_assemblies_gain_superscript_aliases_including_letter_locants() {
    let (mut state, mut arena, root) = setup();
    token(&mut arena, root, "locant", "1,1'", &[]);
    token(
        &mut arena,
        root,
        "ringAssemblyMultiplier",
        "bi",
        &[("value", "2")],
    );
    let (_, ring) = group(&mut state, &mut arena, root, "cyclopropyl", "C1CC1");
    let first = atom(&state, ring, "1");
    state.graph_mut().add_locant(first, "2a");
    state.graph_mut().add_locant(first, "N");
    assemble(&mut state, &mut arena, root).unwrap();
    assert_eq!(atom(&state, ring, "1^2a"), first);
    assert_eq!(atom(&state, ring, "2^2a"), atom(&state, ring, "2a'"));
    assert!(state.graph().atom_by_locant(ring, "1^N").is_none());
    assert_eq!(
        state.graph().atom(first).locants,
        ["1", "2a", "N", "1^1", "1^2a"]
    );
}

#[test]
fn superscript_ring_assembly_errors_preserve_upstream_messages() {
    for (locants, expected) in [
        (
            "1^1,2^1:2^4,3^1-",
            "Disagreement between number of locants(1,1':4',1'') and ring assembly multiplier: 2",
        ),
        ("2147483648^1,2^1", "For input string: \"2147483648\""),
    ] {
        let (mut state, mut arena, root) = setup();
        token(
            &mut arena,
            root,
            "colonOrSemiColonDelimitedLocant",
            locants,
            &[],
        );
        token(
            &mut arena,
            root,
            "ringAssemblyMultiplier",
            "bi",
            &[("value", "2")],
        );
        group(&mut state, &mut arena, root, "phenyl", "c1ccccc1");
        assert_eq!(
            assemble(&mut state, &mut arena, root).unwrap_err().0,
            expected
        );
    }
}

#[test]
fn bridges_attach_to_their_next_group_and_number_each_group_independently() {
    let (mut state, mut arena, root) = setup();
    token(&mut arena, root, "locant", "1,3", &[]);
    let first_bridge = token(
        &mut arena,
        root,
        "fusedRingBridge",
        "methano",
        &[("value", "-C-")],
    );
    let (_, first_ring) = group(&mut state, &mut arena, root, "cyclopropane", "C1CC1");
    token(&mut arena, root, "locant", "2,5", &[]);
    let second_bridge = token(
        &mut arena,
        root,
        "fusedRingBridge",
        "ethano",
        &[("value", "-CC-")],
    );
    let (_, second_ring) = group(&mut state, &mut arena, root, "cyclohexane", "C1CCCCC1");
    process_fused_ring_bridges(&mut state, &mut arena, root).unwrap();
    assert_eq!(state.graph().fragment(first_ring).atoms.len(), 4);
    assert!(bonded(&state, first_ring, "4", "1"));
    assert!(bonded(&state, first_ring, "4", "3"));
    assert_eq!(state.graph().fragment(second_ring).atoms.len(), 8);
    assert!(bonded(&state, second_ring, "7", "5"));
    assert!(bonded(&state, second_ring, "8", "2"));
    assert!(arena[first_bridge].parent.is_none());
    assert!(arena[second_bridge].parent.is_none());
}

#[test]
fn phane_group_ends_the_preceding_fused_system() {
    let (mut state, mut arena, root) = setup();
    let (benzo, _) = group(&mut state, &mut arena, root, "benzo", "c1ccccc1");
    arena[benzo].add_attribute("subType", "fusionRing");
    let (furan, fused_ring) = group(&mut state, &mut arena, root, "furan", "o1cccc1");
    let (phane, skeleton) = group(&mut state, &mut arena, root, "cyclobutaphane", "C1CCC1");
    arena[phane].add_attribute("subType", "phane");
    process_fused_rings(&mut state, &mut arena, root).unwrap();
    assert_eq!(arena.children_named(root, "group"), [furan, phane]);
    assert_eq!(state.graph().fragment(fused_ring).atoms.len(), 9);
    assert_eq!(state.graph().fragment(skeleton).atoms.len(), 4);
    assert_eq!(arena[phane].fragment, Some(skeleton));
}

#[test]
fn amino_acid_superscript_locant_remains_available_for_substitution() {
    let (mut state, mut arena, root) = setup();
    let locant = token(&mut arena, root, "locant", "N2^3", &[]);
    let (group, _) = group(&mut state, &mut arena, root, "ethyl", "CC");
    let suffix = state
        .fragment_manager
        .build_smiles("*CN", "suffix", "none")
        .unwrap();
    let atoms = state.graph().fragment(suffix).atoms.clone();
    state.graph_mut().add_locant(atoms[1], "1");
    state.graph_mut().add_locant(atoms[2], "N2^3");
    state.xml_suffix_map.insert(group, vec![suffix]);
    ComponentProcessor::new(&mut state, &SuffixRules::new().unwrap())
        .match_locants_to_indirect_features(&mut arena, root)
        .unwrap();
    assert_eq!(arena[locant].parent, Some(root));
    assert!(state.graph().bond_between(atoms[0], atoms[1]).is_some());
    assert!(state.graph().bond_between(atoms[0], atoms[2]).is_none());
}
