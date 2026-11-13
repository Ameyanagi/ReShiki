use super::*;
use crate::{
    electronic::Hybridization,
    graph::{Atom, Bond, Graph},
    kekulize::Direction,
    ranking::Metadata,
    stereo::perception::{Properties, RingCache, RingKind},
};

fn chain(n: usize) -> anyhow::Result<State> {
    let graph = Graph {
        atoms: vec![
            Atom {
                atomic_number: 6,
                no_implicit: true,
                ..Atom::default()
            };
            n
        ],
        bonds: (1..n)
            .map(|b| Bond {
                a: b - 1,
                b,
                order: 1,
                aromatic: false,
            })
            .collect(),
    };
    Ok(State {
        conformer: None,
        metadata: Metadata::unspecified(&graph),
        directions: vec![Direction::None; graph.bonds.len()],
        valences: graph.provisional_valences().map_err(anyhow::Error::msg)?,
        conjugated: vec![false; graph.bonds.len()],
        hybridizations: vec![Hybridization::Sp3; graph.atoms.len()],
        rings: RingCache::default(),
        properties: Properties::unspecified(&graph),
        graph,
    })
}

#[test]
fn protium_mass_matches_implicit_hydrogen_without_losing_the_label() -> anyhow::Result<()> {
    for (text, equal) in [("[1H]C", true), ("[2H]C", false), ("[3H]C", false)] {
        let state = crate::smiles::prepare(text)?.state;
        let mut mol = Molecule::new(&state)?;
        let mut graph = Digraph::new(&mol, 1, false)?;
        graph.edges(&mut mol, 0)?;
        let labeled = graph.nodes.iter().find(|n| n.atom == Some(0)).unwrap();
        let implicit = graph
            .nodes
            .iter()
            .find(|n| n.flags & IMPLICIT_HYDROGEN != 0)
            .unwrap();
        assert_eq!(labeled.mass == implicit.mass, equal, "{text}");
        assert_eq!(labeled.isotope, state.graph.atoms[0].isotope, "{text}");
        let mut ordinary = state.clone();
        ordinary.graph.atoms[0].isotope = 0;
        let ordinary = Molecule::new(&ordinary)?;
        let ordinary = Digraph::new(&ordinary, 0, false)?;
        assert_eq!(labeled.mass == ordinary.node(0)?.mass, equal, "{text}");
    }
    Ok(())
}

#[test]
fn unknown_isotope_mass_uses_the_mass_number_except_for_dummy_atoms() -> anyhow::Result<()> {
    let state = crate::smiles::read("[999C]O[14C]")?.prepared.state;
    let mut molecule = Molecule::new(&state)?;
    let mut graph = Digraph::new(&molecule, 1, false)?;
    graph.edges(&mut molecule, 0)?;
    let unknown = graph.nodes.iter().find(|node| node.isotope == 999).unwrap();
    let known = graph.nodes.iter().find(|node| node.isotope == 14).unwrap();
    assert_eq!(unknown.mass, 999.);
    assert!(unknown.mass > known.mass);
    let state = crate::smiles::read("[999*]O[14C]")?.prepared.state;
    let molecule = Molecule::new(&state)?;
    let graph = Digraph::new(&molecule, 0, false)?;
    assert_eq!(graph.node(0)?.number, 0);
    assert_eq!(graph.node(0)?.isotope, 999);
    assert_eq!(graph.node(0)?.mass, 0.);
    Ok(())
}

#[test]
fn integer_mancude_average_controls_negative_duplicate_expansion() -> anyhow::Result<()> {
    let state = crate::smiles::read("[CH-]1C=C1")?.prepared.state;
    let mut molecule = Molecule::new(&state)?;
    let mut graph = Digraph::new(&molecule, 1, false)?;
    let negative = graph
        .edges(&mut molecule, 0)?
        .to_vec()
        .into_iter()
        .map(|edge| graph.edge(edge).unwrap().end)
        .find(|&node| graph.node(node).unwrap().atom == Some(0))
        .unwrap();
    let duplicates = graph
        .edges(&mut molecule, negative)?
        .to_vec()
        .into_iter()
        .filter_map(|edge| {
            let edge = graph.edge(edge).unwrap();
            let node = graph.node(edge.end).unwrap();
            (edge.begin == negative && node.flags & BOND_DUPLICATE != 0).then_some(node)
        })
        .collect::<Vec<_>>();
    assert_eq!(duplicates.len(), 1);
    assert_eq!(duplicates[0].fraction, Fraction::integer(4));
    Ok(())
}

#[test]
fn bad_indices_and_a_different_source_cannot_edit_the_graph() -> anyhow::Result<()> {
    let state = chain(3)?;
    let mut mol = Molecule::new(&state)?;
    let other_state = state.clone();
    let mut other = Molecule::new(&other_state)?;
    assert!(Digraph::new(&mol, usize::MAX, false).is_err());
    let mut graph = Digraph::new(&mol, 0, false)?;
    assert!(graph.edges(&mut other, 0).is_err());
    assert!(graph.edges(&mut mol, usize::MAX).is_err());
    assert!(graph.change_root(&mut mol, usize::MAX).is_err());
    assert!(graph.set_rule6_reference(Some(usize::MAX)).is_err());
    assert_eq!(graph.node_count(), 1);
    assert_eq!(graph.edge_count(), 0);
    assert!(!graph.node(0)?.is_expanded());
    graph.edges(&mut mol, 0)?;
    assert!(graph.edge(0)?.other(usize::MAX).is_err());
    assert!(graph.seen_atom(usize::MAX).is_err());
    Ok(())
}

#[test]
fn deep_expansion_returns_a_limit_without_recursion_or_partial_reuse() -> anyhow::Result<()> {
    let mut state = chain(100_000)?;
    state.graph.atoms[99_999].explicit_hydrogens = 1;
    state.valences = state
        .graph
        .provisional_valences()
        .map_err(anyhow::Error::msg)?;
    let mut mol = Molecule::new(&state)?;
    let mut graph = Digraph::new(&mol, 0, false)?;
    assert!(matches!(
        graph.nodes_for_atom(&mut mol, 99_999),
        Err(Error::Nodes)
    ));
    assert_eq!(graph.node_count(), 100_000);
    assert!(graph.edges(&mut mol, 0).is_err());
    assert_eq!(state.rings.kind, RingKind::None);
    assert_eq!(state.graph.bonds.len(), 99_999);
    Ok(())
}

#[test]
fn node_limit_applies_to_every_insertion() -> anyhow::Result<()> {
    let state = chain(1)?;
    let molecule = Molecule::new(&state)?;
    let mut graph = Digraph::new(&molecule, 0, false)?;
    for _ in 1..100_000 {
        graph.add_node(None, None, 1, IMPLICIT_HYDROGEN, None)?;
    }
    assert_eq!(graph.node_count(), 100_000);
    assert!(matches!(
        graph.add_node(None, None, 1, IMPLICIT_HYDROGEN, None),
        Err(Error::Nodes)
    ));
    assert_eq!(graph.node_count(), 100_000);
    Ok(())
}

#[test]
fn long_path_visits_and_duplicate_distances_do_not_wrap() -> anyhow::Result<()> {
    let state = chain(260)?;
    let mut molecule = Molecule::new(&state)?;
    let mut graph = Digraph::new(&molecule, 0, false)?;
    let terminal = graph.nodes_for_atom(&mut molecule, 259)?;
    assert_eq!(graph.node_count(), 260);
    assert_eq!(terminal.len(), 1);
    assert_eq!(graph.node(terminal[0])?.distance, 260);
    for (atom, expected) in [(255, 256), (259, 260)] {
        let duplicate = graph.terminal(&mut molecule, terminal[0], Some(atom), RING_DUPLICATE)?;
        assert_eq!(graph.node(duplicate)?.distance, expected);
    }
    Ok(())
}

#[test]
fn persistent_visits_keep_independent_histories_with_bounded_work() -> anyhow::Result<()> {
    let mut visits = Visits::new(100_000);
    let mut work = Work(1_000);
    let original = visits.set(None, 70, 1, &mut work)?;
    let left = visits.set(original, 99_999, 65_536, &mut work)?;
    let right = visits.set(original, 70, 0, &mut work)?;
    assert_eq!(visits.get(original, 99_999, &mut work)?, 0);
    assert_eq!(visits.get(left, 99_999, &mut work)?, 65_536);
    assert_eq!(visits.get(left, 70, &mut work)?, 1);
    assert_eq!(visits.get(right, 70, &mut work)?, 0);
    assert_eq!(visits.get(original, 70, &mut work)?, 1);
    assert!(visits.set(original, usize::MAX, 1, &mut work).is_err());
    assert!(matches!(
        visits.set(original, 1, 1, &mut Work(0)),
        Err(Error::Limit)
    ));
    assert!(visits.cells_len() < 100);
    Ok(())
}
