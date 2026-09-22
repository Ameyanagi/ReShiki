use super::*;
use crate::chemistry::{
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
    let state = chain(100_000)?;
    let mut mol = Molecule::new(&state)?;
    let mut graph = Digraph::new(&mol, 0, false)?;
    assert!(matches!(
        graph.nodes_for_atom(&mut mol, 99_999),
        Err(Error::Nodes)
    ));
    assert!(graph.node_count() >= 100_000);
    assert!(graph.edges(&mut mol, 0).is_err());
    assert_eq!(state.rings.kind, RingKind::None);
    assert_eq!(state.graph.bonds.len(), 99_999);
    Ok(())
}

#[test]
fn persistent_visits_keep_independent_histories_with_bounded_work() -> anyhow::Result<()> {
    let mut visits = Visits::new(100_000);
    let mut work = Work(1_000);
    let original = visits.set(None, 70, 1, &mut work)?;
    let left = visits.set(original, 99_999, 7, &mut work)?;
    let right = visits.set(original, 70, 0, &mut work)?;
    assert_eq!(visits.get(original, 99_999, &mut work)?, 0);
    assert_eq!(visits.get(left, 99_999, &mut work)?, 7);
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
