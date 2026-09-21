use super::*;
use crate::chemistry::graph::{Atom, Bond};

fn graph(numbers: &[u8], bonds: &[(usize, usize, u8)]) -> Graph {
    Graph {
        atoms: numbers
            .iter()
            .map(|&atomic_number| Atom {
                atomic_number,
                ..Default::default()
            })
            .collect(),
        bonds: bonds
            .iter()
            .map(|&(a, b, order)| Bond {
                a,
                b,
                order,
                aromatic: false,
            })
            .collect(),
    }
}

#[test]
fn explicit_and_attached_hydrogens_have_the_same_descriptors() -> Result<(), String> {
    let implicit = calculate(&graph(&[6], &[]), &[])?;
    let explicit = calculate(
        &graph(
            &[6, 1, 1, 1, 1],
            &[(0, 1, 1), (0, 2, 1), (0, 3, 1), (0, 4, 1)],
        ),
        &[],
    )?;
    assert_eq!(implicit.logp, explicit.logp);
    assert!((implicit.logp - 0.6361).abs() < 1e-12);
    assert_eq!(implicit.crippen_atoms, explicit.crippen_atoms);
    assert_eq!(implicit.crippen_types, explicit.crippen_types);
    assert_eq!(implicit.crippen_atoms.len(), 5);
    Ok(())
}

#[test]
fn amide_and_three_membered_ring_rules_remain_distinct() -> Result<(), String> {
    let amide = calculate(
        &graph(&[7, 6, 8, 6], &[(0, 1, 1), (1, 2, 2), (1, 3, 1)]),
        &[],
    )?;
    assert_eq!((amide.donors, amide.acceptors), (1, 1));
    assert!((amide.tpsa - 43.09).abs() < 1e-12);
    let epoxide = graph(&[6, 6, 8], &[(0, 1, 1), (1, 2, 1), (2, 0, 1)]);
    assert_eq!(calculate(&epoxide, &[vec![0, 1, 2]])?.tpsa, 12.53);
    assert_eq!(
        calculate(&graph(&[6, 8, 6], &[(0, 1, 1), (1, 2, 1)]), &[])?.tpsa,
        9.23
    );
    Ok(())
}

#[test]
fn unmatched_elements_do_not_receive_a_fabricated_crippen_type() -> Result<(), String> {
    let neon = calculate(&graph(&[10], &[]), &[])?;
    assert_eq!(neon.crippen_atoms, vec![[0., 0.]]);
    assert_eq!(neon.crippen_types, vec![None]);
    Ok(())
}

#[test]
fn malformed_graph_and_ring_data_do_not_mutate_input() -> Result<(), String> {
    let mut bad = graph(&[6], &[]);
    bad.atoms[0].explicit_hydrogens = 5;
    assert!(calculate(&bad, &[]).is_err());
    let good = graph(&[6, 6, 8], &[(0, 1, 1), (1, 2, 1), (2, 0, 1)]);
    let before = serde_json::to_value(&good).map_err(|e| e.to_string())?;
    for rings in [vec![vec![0, 1]], vec![vec![0, 0, 0]], vec![vec![0, 1, 999]]] {
        assert!(calculate(&good, &rings).is_err());
        assert_eq!(
            serde_json::to_value(&good).map_err(|e| e.to_string())?,
            before
        );
    }
    Ok(())
}

#[test]
fn matcher_and_hydrogen_expansion_have_explicit_limits() -> Result<(), String> {
    let target = Target::new(&graph(&[6], &[]), &[])?;
    let mut work = Work(0);
    let data = data()?;
    let mut matcher = matcher::Matcher::new(&target, &data.patterns, &mut work);
    assert!(matcher.roots(data.donors).is_err());
    let graph = Graph {
        atoms: vec![
            Atom {
                atomic_number: 0,
                explicit_hydrogens: 250,
                no_implicit: true,
                ..Default::default()
            };
            2000
        ],
        bonds: vec![],
    };
    let mut target = Target::new(&graph, &[])?;
    assert!(target.add_hydrogens().is_err());
    assert_eq!(target.nodes.len(), 2000);
    assert!(target.edges.is_empty());
    Ok(())
}
