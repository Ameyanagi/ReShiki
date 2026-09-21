use super::*;
use crate::chemistry::{
    graph::{Atom, Bond},
    stereo::bond_stereo_from_directions,
};
type TestResult = anyhow::Result<()>;

fn chain(n: usize) -> (Graph, Vec<Point3>) {
    let graph = Graph {
        atoms: vec![
            Atom {
                atomic_number: 6,
                ..Atom::default()
            };
            n
        ],
        bonds: (1..n)
            .map(|b| Bond {
                a: b - 1,
                b,
                order: if b % 2 == 0 { 2 } else { 1 },
                aromatic: false,
            })
            .collect(),
    };
    let positions = (0..n)
        .map(|i| Point3 {
            x: i as f64,
            y: (i % 2) as f64,
            z: 0.0,
        })
        .collect();
    (graph, positions)
}

#[test]
fn long_conjugated_chain_propagates_without_recursion() -> TestResult {
    let (graph, positions) = chain(30_000);
    let result = detect_bond_stereo(
        &graph,
        &Metadata::unspecified(&graph),
        &vec![Direction::None; graph.bonds.len()],
        Some(&positions),
        &[],
    )
    .map_err(anyhow::Error::msg)?;
    let meta = bond_stereo_from_directions(&graph, &result.metadata, &result.directions)
        .map_err(anyhow::Error::msg)?;
    for (bond, meta) in graph.bonds.iter().zip(meta.bonds) {
        if bond.order == 2 {
            assert_eq!(meta.stereo, 5);
        }
    }
    Ok(())
}

#[test]
fn rejected_coordinates_metadata_and_exhausted_work_are_atomic() -> TestResult {
    let (graph, positions) = chain(6);
    let meta = Metadata::unspecified(&graph);
    let directions = vec![Direction::None; graph.bonds.len()];
    let before = serde_json::to_value((&graph, &meta, &directions))?;
    for number in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 1e38] {
        let mut bad = positions.clone();
        bad[0].x = number;
        assert!(detect_bond_stereo(&graph, &meta, &directions, Some(&bad), &[]).is_err());
    }
    assert!(detect_bond_stereo(&graph, &meta, &directions, Some(&[]), &[]).is_err());
    assert!(double_bond_directions(&graph, &meta, &[], None, &[]).is_err());
    for rings in [
        vec![vec![0, 1]],
        vec![vec![0, 1, 2]],
        vec![vec![0, 1, 1]],
        vec![vec![0, 1, 99]],
    ] {
        assert!(
            double_bond_directions(&graph, &meta, &directions, Some(&positions), &rings).is_err()
        );
    }
    let mut bad = meta.clone();
    bad.bonds[1].stereo = 3;
    assert!(double_bond_directions(&graph, &bad, &directions, None, &[]).is_err());
    assert!(
        with_work(
            &graph,
            &meta,
            &directions,
            Some(&positions),
            &[],
            &mut Work {
                remaining: 0,
                stored: 0
            }
        )
        .is_err()
    );
    assert!(
        with_work(
            &graph,
            &meta,
            &directions,
            Some(&positions),
            &[],
            &mut Work {
                remaining: 1000,
                stored: 2_000_000
            }
        )
        .is_err()
    );
    assert_eq!(serde_json::to_value((&graph, &meta, &directions))?, before);
    Ok(())
}

#[test]
fn no_conformer_detection_differs_from_direction_assignment() -> TestResult {
    let (graph, _) = chain(4);
    let mut meta = Metadata::unspecified(&graph);
    meta.bonds[1].stereo = 3;
    meta.bonds[1].stereo_atoms = vec![0, 3];
    let dirs = vec![Direction::None; graph.bonds.len()];
    let untouched =
        detect_bond_stereo(&graph, &meta, &dirs, None, &[]).map_err(anyhow::Error::msg)?;
    assert_eq!(untouched.directions, dirs);
    let directed =
        double_bond_directions(&graph, &meta, &dirs, None, &[]).map_err(anyhow::Error::msg)?;
    assert_ne!(directed.directions, dirs);
    assert_eq!(
        bond_stereo_from_directions(&graph, &directed.metadata, &directed.directions)
            .map_err(anyhow::Error::msg)?
            .bonds[1]
            .stereo,
        5
    );
    Ok(())
}
