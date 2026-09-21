use super::*;
use crate::chemistry::graph::{Atom, Bond};
type TestResult = Result<(), Box<dyn std::error::Error>>;

fn carbon_graph(n: usize, edges: impl Iterator<Item = (usize, usize, u8)>) -> Graph {
    Graph {
        atoms: vec![
            Atom {
                atomic_number: 6,
                ..Atom::default()
            };
            n
        ],
        bonds: edges
            .map(|(a, b, order)| Bond {
                a,
                b,
                order,
                aromatic: false,
            })
            .collect(),
    }
}

#[test]
fn long_polyenes_and_high_degree_centers_have_bounded_traversal() -> TestResult {
    let n = 30_000;
    let graph = carbon_graph(
        n,
        (1..n).map(|i| (i - 1, i, if i % 2 == 1 { 2 } else { 1 })),
    );
    let flags = conjugation(&graph)?;
    assert!(flags.iter().all(|&flag| flag));
    assert!(
        hybridization(&graph, &vec![0; n], &flags)?
            .iter()
            .all(|&h| h == Hybridization::Sp2)
    );
    let star = carbon_graph(n, (1..n).map(|i| (0, i, 1)));
    // A naive all-pairs bond scan at the center would be quadratic. The
    // candidate/degree gate excludes this center before comparing its bonds.
    assert!(conjugation(&star)?.iter().all(|&flag| !flag));
    assert_eq!(
        hybridization(&star, &vec![0; n], &vec![false; n - 1])?[0],
        Hybridization::Unspecified
    );
    Ok(())
}

#[test]
fn rejected_graphs_and_metadata_leave_inputs_unchanged() -> TestResult {
    for kind in 0..5 {
        let mut graph = carbon_graph(3, [(0, 1, 2), (1, 2, 1)].into_iter());
        match kind {
            0 => graph.bonds[0].b = 99,
            1 => graph.bonds.push(graph.bonds[0].clone()),
            2 => graph.bonds[0].order = 8,
            3 => graph.atoms[0].atomic_number = 119,
            _ => graph.bonds[0].b = 0,
        }
        let before = serde_json::to_value(&graph)?;
        assert!(pi_electrons(&graph).is_err());
        assert!(conjugation(&graph).is_err());
        assert!(hybridization(&graph, &[0; 3], &vec![false; graph.bonds.len()]).is_err());
        assert_eq!(serde_json::to_value(&graph)?, before);
    }
    let graph = carbon_graph(3, [(0, 1, 2), (1, 2, 1)].into_iter());
    let before = serde_json::to_value(&graph)?;
    assert!(hybridization(&graph, &[0; 2], &[false; 2]).is_err());
    assert!(hybridization(&graph, &[0; 3], &[false; 1]).is_err());
    assert!(hybridization(&graph, &[0, 9, 0], &[false; 2]).is_err());
    assert_eq!(serde_json::to_value(&graph)?, before);
    Ok(())
}
