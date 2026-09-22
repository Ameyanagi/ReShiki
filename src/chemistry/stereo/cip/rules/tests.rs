use super::*;
use crate::chemistry::{
    electronic::Hybridization,
    graph::{Atom, Bond, Graph},
    kekulize::Direction,
    ranking::Metadata,
    stereo::perception::{Properties, RingCache, State},
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
fn exhausted_iterations_stay_exhausted_and_allow_a_new_pass() -> anyhow::Result<()> {
    let state = chain(15)?;
    let before = serde_json::to_value(&state)?;
    let mut mol = Molecule::new(&state)?;
    let mut graph = Digraph::new(&mol, 7, false)?;
    let edges = graph.edges(&mut mol, 0)?.to_vec();
    let rules = Rules::constitutional();
    let mut limit = Iterations::new(1);
    for _ in 0..2 {
        assert!(matches!(
            rules.sort(
                &mut Context::new(&mut mol, &mut graph, &mut limit)?,
                0,
                &edges,
                true
            ),
            Err(Error::Iterations)
        ));
        assert_eq!(limit.remaining(), 0);
    }
    let mut limit = Iterations::new(2_000);
    let sorted = rules.sort(
        &mut Context::new(&mut mol, &mut graph, &mut limit)?,
        0,
        &edges,
        true,
    )?;
    assert_eq!(sorted.edges, edges);
    assert!(!sorted.unique);
    assert!(!sorted.pseudo);
    assert_eq!(before, serde_json::to_value(&state)?);
    Ok(())
}

#[test]
fn invalid_inputs_do_not_consume_iterations_or_expand() -> anyhow::Result<()> {
    let state = chain(3)?;
    let mut mol = Molecule::new(&state)?;
    let mut graph = Digraph::new(&mol, 0, false)?;
    let mut limit = Iterations::new(500);
    let rules = Rules::constitutional();
    let mut ctx = Context::new(&mut mol, &mut graph, &mut limit)?;
    assert!(rules.compare(&mut ctx, usize::MAX, 0, true).is_err());
    assert!(rules.sort(&mut ctx, usize::MAX, &[], true).is_err());
    assert!(rules.sort(&mut ctx, 0, &vec![0; 500_001], true).is_err());
    assert!(rules.groups(&mut ctx, &[usize::MAX]).is_err());
    assert_eq!(limit.remaining(), 500);
    assert_eq!(graph.node_count(), 1);
    assert!(Rules::new(&[]).is_err());
    assert!(Rules::new(&[Rule::AtomicNumber; 17]).is_err());
    Ok(())
}

#[test]
fn long_equal_substituents_are_compared_without_rust_recursion() -> anyhow::Result<()> {
    let state = chain(20_001)?;
    let mut mol = Molecule::new(&state)?;
    let mut graph = Digraph::new(&mol, 10_000, false)?;
    let edges = graph.edges(&mut mol, 0)?.to_vec();
    let mut limit = Iterations::new(0);
    let sorted = Rules::constitutional().sort(
        &mut Context::new(&mut mol, &mut graph, &mut limit)?,
        0,
        &edges,
        true,
    )?;
    assert_eq!(sorted.edges, edges);
    assert!(!sorted.unique);
    // Native visit distances are bytes; long paths can create extra duplicates.
    // The comparison must still reach every atom and retain equal priorities.
    assert!(graph.node_count() >= state.graph.atoms.len());
    for atom in 0..state.graph.atoms.len() {
        assert!(graph.seen_atom(atom)?);
    }
    Ok(())
}

#[test]
fn many_groups_do_not_rescan_previous_group_storage() -> anyhow::Result<()> {
    let mut state = chain(3)?;
    state
        .graph
        .atoms
        .get_mut(0)
        .ok_or_else(|| invalid("Missing test atom"))?
        .atomic_number = 8;
    state.valences = state
        .graph
        .provisional_valences()
        .map_err(anyhow::Error::msg)?;
    let mut mol = Molecule::new(&state)?;
    let mut graph = Digraph::new(&mol, 1, false)?;
    let incident = graph.edges(&mut mol, 0)?.to_vec();
    let edges: Vec<_> = incident.into_iter().cycle().take(50_000).collect();
    let mut limit = Iterations::new(0);
    let groups = Rules::constitutional()
        .groups(&mut Context::new(&mut mol, &mut graph, &mut limit)?, &edges)?;
    assert_eq!(groups.len(), edges.len());
    assert!(groups.iter().all(|g| g.len() == 1));
    assert_eq!(graph.node_count(), 3);
    Ok(())
}
