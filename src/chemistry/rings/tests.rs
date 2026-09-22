use super::*;
use crate::chemistry::graph::{Atom, Bond};
type TestResult = anyhow::Result<()>;

fn graph(n: usize, edges: &[(usize, usize)]) -> Graph {
    Graph {
        atoms: vec![
            Atom {
                atomic_number: 6,
                ..Atom::default()
            };
            n
        ],
        bonds: edges
            .iter()
            .map(|&(a, b)| Bond {
                a,
                b,
                order: 1,
                aromatic: false,
            })
            .collect(),
    }
}

fn assert_cycles(graph: &Graph, rings: &Rings) {
    assert_eq!(rings.atoms.len(), rings.bonds.len());
    let mut seen = BTreeSet::new();
    for (atoms, bonds) in rings.atoms.iter().zip(&rings.bonds) {
        assert!(atoms.len() >= 3);
        assert_eq!(atoms.len(), bonds.len());
        assert_eq!(atoms.len(), atoms.iter().collect::<BTreeSet<_>>().len());
        assert!(seen.insert(invariant(atoms)));
        for ((&a, &b), &edge) in atoms.iter().zip(atoms.iter().cycle().skip(1)).zip(bonds) {
            let bond = &graph.bonds[edge];
            assert_eq!(
                (a.min(b), a.max(b)),
                (bond.a.min(bond.b), bond.a.max(bond.b))
            );
        }
    }
}

#[test]
fn cubane_has_six_symmetric_rings_and_five_basis_rings() -> TestResult {
    let edges = (0usize..8)
        .flat_map(|a| {
            [1, 2, 4].into_iter().filter_map(move |bit| {
                let b = a ^ bit;
                (a < b).then_some((a, b))
            })
        })
        .collect::<Vec<_>>();
    let graph = graph(8, &edges);
    let rings = perceive(&graph, Options::default())?;
    assert_eq!(rings.basis_count, 5);
    assert_eq!(rings.atoms.len(), 6);
    assert!(!rings.approximate);
    assert!(rings.atoms.iter().all(|r| r.len() == 4));
    assert_cycles(&graph, &rings);
    Ok(())
}

#[test]
fn spiro_rings_and_excluded_bonds_preserve_topology() -> TestResult {
    let mut graph = graph(6, &[(0, 1), (1, 2), (2, 0), (0, 3), (3, 4), (4, 5), (5, 0)]);
    for order in [0, 5] {
        graph.bonds[0].order = order;
        let rings = perceive(&graph, Options::default())?;
        assert_eq!(rings.atoms.len(), 1);
        assert_eq!(rings.atoms[0].len(), 4);
        let rings = perceive(
            &graph,
            Options {
                include_dative: true,
                include_hydrogen: true,
            },
        )?;
        assert_eq!(rings.atoms.len(), 2);
        assert_cycles(&graph, &rings);
    }
    Ok(())
}

#[test]
fn long_chains_and_macrocycles_do_not_use_recursive_traversal() -> TestResult {
    let n = 30_000;
    let edges = (1..n).map(|i| (i - 1, i)).collect::<Vec<_>>();
    let mut graph = graph(n, &edges);
    assert!(perceive(&graph, Options::default())?.atoms.is_empty());
    assert!(fast(&graph).map_err(anyhow::Error::msg)?.atoms.is_empty());
    graph.bonds.push(Bond {
        a: n - 1,
        b: 0,
        order: 1,
        aromatic: false,
    });
    let rings = perceive(&graph, Options::default())?;
    assert_eq!(rings.atoms.len(), 1);
    assert_eq!(rings.atoms[0].len(), n);
    assert_cycles(&graph, &rings);
    let rings = fast(&graph).map_err(anyhow::Error::msg)?;
    assert_eq!(rings.atoms.len(), 1);
    assert_eq!(rings.atoms[0].len(), n);
    assert_cycles(&graph, &rings);
    Ok(())
}

#[test]
fn fast_ring_work_and_storage_are_bounded() -> TestResult {
    let graph = graph(3, &[(0, 1), (1, 2), (2, 0)]);
    let top = Topology::new(&graph).map_err(anyhow::Error::msg)?;
    assert!(search::fast(&top, &mut Budget { work: 0, stored: 3 }).is_err());
    assert!(
        search::fast(
            &top,
            &mut Budget {
                work: 100,
                stored: 2
            }
        )
        .is_err()
    );
    assert_eq!(
        search::fast(
            &top,
            &mut Budget {
                work: 100,
                stored: 3
            }
        )
        .map_err(anyhow::Error::msg)?
        .len(),
        1
    );
    Ok(())
}

#[test]
fn dense_degree_four_graph_uses_iterative_reference_fallback() -> TestResult {
    let edges = (0..5)
        .flat_map(|a| ((a + 1)..5).map(move |b| (a, b)))
        .collect::<Vec<_>>();
    let graph = graph(5, &edges);
    let rings = perceive(&graph, Options::default())?;
    assert!(rings.approximate);
    assert_eq!(rings.basis_count, 6);
    assert_cycles(&graph, &rings);
    Ok(())
}

#[test]
fn dense_ordering_is_local_and_search_is_budgeted() -> TestResult {
    let graph: Graph = serde_json::from_str(include_str!(
        "../../../tests/fixtures/ring-order-dependent.json"
    ))?;
    let rings = perceive(&graph, Options::default())?;
    assert_cycles(&graph, &rings);
    let mut budget = Budget { work: 0, stored: 0 };
    let top = Topology::new(&graph).map_err(anyhow::Error::msg)?;
    assert!(search::smallest(&top, 0, &vec![true; graph.bonds.len()], &[], &mut budget).is_err());
    Ok(())
}

#[test]
fn malformed_graphs_fail_without_mutation() -> TestResult {
    let original = graph(3, &[(0, 1), (1, 2), (2, 0)]);
    for kind in 0..5 {
        let mut bad = original.clone();
        match kind {
            0 => bad.bonds[0].b = 999,
            1 => bad.bonds[0].a = bad.bonds[0].b,
            2 => bad.bonds.push(bad.bonds[0].clone()),
            3 => bad.bonds[0].order = 8,
            _ => bad.atoms[0].atomic_number = 119,
        }
        let before = serde_json::to_value(&bad)?;
        assert!(matches!(
            perceive(&bad, Options::default()),
            Err(RingError::Failed(_))
        ));
        assert!(fast(&bad).is_err());
        assert_eq!(serde_json::to_value(&bad)?, before);
    }
    Ok(())
}
