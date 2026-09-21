use super::*;
use crate::chemistry::graph::{Atom, Bond};

fn ring(size: usize) -> Graph {
    Graph {
        atoms: vec![
            Atom {
                atomic_number: 6,
                aromatic: true,
                ..Default::default()
            };
            size
        ],
        bonds: (0..size)
            .map(|i| Bond {
                a: i,
                b: (i + 1) % size,
                order: 4,
                aromatic: true,
            })
            .collect(),
    }
}

#[test]
fn alternating_bonds_and_atomic_failure() -> Result<(), String> {
    let graph = ring(6);
    let dirs = vec![Direction::None; 6];
    let result = assign(&graph, &[vec![0, 1, 2, 3, 4, 5]], &dirs, Options::default())?;
    assert_eq!(
        result.graph.bonds.iter().filter(|b| b.order == 2).count(),
        3
    );
    assert!(result.graph.atoms.iter().all(|a| !a.aromatic));
    let odd = ring(5);
    let before = serde_json::to_value(&odd).map_err(|e| e.to_string())?;
    assert!(assign(&odd, &[vec![0, 1, 2, 3, 4]], &dirs[..5], Options::default()).is_err());
    assert_eq!(
        serde_json::to_value(&odd).map_err(|e| e.to_string())?,
        before
    );
    assert!(graph.bonds.iter().all(|b| b.order == 4));
    Ok(())
}

#[test]
fn invalid_metadata_and_exhausted_work_return_errors() {
    let graph = ring(6);
    let dirs = vec![Direction::None; 6];
    let rings = vec![vec![0, 1, 2, 3, 4, 5]];
    assert!(assign(&graph, &rings, &[], Options::default()).is_err());
    for ranks in [vec![], vec![u32::MAX; 6], vec![0; 7]] {
        assert!(
            assign(
                &graph,
                &rings,
                &dirs,
                Options {
                    ranks: Some(&ranks),
                    ..Options::default()
                }
            )
            .is_err()
        );
    }
    for rings in [
        vec![vec![]],
        vec![vec![0, 1]],
        vec![vec![0, 1, 2, 2]],
        vec![vec![0, 1, 5]],
        vec![vec![0, 1, usize::MAX]],
    ] {
        assert!(assign(&graph, &rings, &dirs, Options::default()).is_err());
    }
    assert!(assign_with_work(&graph, &rings, &dirs, Options::default(), &mut Work(0)).is_err());
}

#[test]
fn backtracking_undoes_a_dead_end_matching() -> Result<(), String> {
    // Start at 1 and try 2 first: this strands 0 and 3. Retrying 1--0
    // permits 2--3. Test the search independently of candidate perception.
    let mut graph = ring(4);
    graph.bonds.pop();
    let topology = Topology::new(&graph, &[])?;
    let ranks = [3, 0, 1, 2];
    for max_backtracks in [0, 1] {
        let mut result = Assignment {
            graph: graph.clone(),
            directions: vec![Direction::Unknown; 3],
        };
        for bond in &mut result.graph.bonds {
            bond.order = 1;
        }
        let candidates = Candidates {
            eligible: vec![true; 4],
            done: Vec::new(),
            questions: Vec::new(),
        };
        let success = search::fused(
            &mut result,
            &[0, 1, 2, 3],
            candidates,
            &topology,
            &ranks,
            max_backtracks,
            &mut Work(10_000),
        );
        assert_eq!(success.is_ok(), max_backtracks == 1);
        if success.is_ok() {
            assert_eq!(
                result
                    .graph
                    .bonds
                    .iter()
                    .map(|b| b.order)
                    .collect::<Vec<_>>(),
                [2, 1, 2]
            );
            // Even the abandoned double bond loses its direction in RDKit.
            assert!(result.directions.iter().all(|d| *d == Direction::None));
        }
    }
    Ok(())
}

#[test]
fn large_ring_assignment_does_not_recurse() -> Result<(), String> {
    let size = 20_002;
    let graph = ring(size);
    let result = assign(
        &graph,
        &[(0..size).collect()],
        &vec![Direction::None; size],
        Options::default(),
    )?;
    assert_eq!(
        result.graph.bonds.iter().filter(|b| b.order == 2).count(),
        size / 2
    );
    Ok(())
}
