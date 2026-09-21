use super::*;
use crate::chemistry::graph::{Atom, Bond};

fn ring(size: usize) -> Graph {
    Graph {
        atoms: vec![
            Atom {
                atomic_number: 6,
                ..Default::default()
            };
            size
        ],
        bonds: (0..size)
            .map(|i| Bond {
                a: i,
                b: (i + 1) % size,
                order: if i % 2 == 0 { 1 } else { 2 },
                aromatic: false,
            })
            .collect(),
    }
}

#[test]
fn aromaticity_is_bounded_and_invalid_rings_leave_input_unchanged() -> Result<(), String> {
    let graph = ring(6);
    let before = serde_json::to_value(&graph).map_err(|e| e.to_string())?;
    for rings in [
        vec![vec![]],
        vec![vec![0, 1]],
        vec![vec![0, 1, 2, 2]],
        vec![vec![0, 1, 5]],
        vec![vec![0, 1, usize::MAX]],
    ] {
        assert!(perceive(&graph, &rings).is_err());
    }
    assert!(perceive_with_work(&graph, &[vec![0, 1, 2, 3, 4, 5]], &mut Work(0)).is_err());
    assert_eq!(
        serde_json::to_value(&graph).map_err(|e| e.to_string())?,
        before
    );
    Ok(())
}

#[test]
fn large_single_ring_uses_an_iterative_traversal() -> Result<(), String> {
    // 4n+2 electrons; the fused-system size cap must not exclude a single ring.
    let size = 20_002;
    let graph = ring(size);
    let result = perceive(&graph, &[(0..size).collect()])?;
    assert_eq!(result.aromatic_rings, 1);
    assert!(result.graph.atoms.iter().all(|a| a.aromatic));
    assert!(
        result
            .graph
            .bonds
            .iter()
            .all(|b| b.aromatic && b.order == 4)
    );
    Ok(())
}

#[test]
fn hydrogen_adjustment_rejects_invalid_cache_size_and_overflow() -> Result<(), String> {
    let graph = ring(6);
    assert!(adjust_hydrogens(&graph, &[]).is_err());
    let mut previous = graph.provisional_valences()?;
    previous[0].implicit_hydrogens = u32::MAX;
    assert!(adjust_hydrogens(&graph, &previous).is_err());
    assert!(graph.atoms.iter().all(|a| a.explicit_hydrogens == 0));
    Ok(())
}
