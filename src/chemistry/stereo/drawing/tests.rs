use super::*;
use crate::chemistry::graph::{Atom, Bond};
type TestResult = anyhow::Result<()>;

fn molecule() -> (Graph, Vec<Point3>, Vec<Direction>) {
    let graph = Graph {
        atoms: [6, 9, 17, 35]
            .map(|atomic_number| Atom {
                atomic_number,
                ..Atom::default()
            })
            .to_vec(),
        bonds: (1..4)
            .map(|b| Bond {
                a: 0,
                b,
                order: 1,
                aromatic: false,
            })
            .collect(),
    };
    let positions = [(0.0, 0.0), (0.0, 1.0), (-1.0, -0.5), (1.0, -0.5)]
        .map(|(x, y)| Point3 { x, y, z: 0.0 })
        .to_vec();
    (
        graph,
        positions,
        vec![Direction::Wedge, Direction::None, Direction::None],
    )
}

#[test]
fn reflecting_wedge_geometry_reverses_winding_and_promotes_h() -> TestResult {
    let (graph, positions, dirs) = molecule();
    let meta = Metadata::unspecified(&graph);
    let before = serde_json::to_value((&graph, &meta))?;
    let result = from_directions(&graph, &meta, &dirs, Some(&positions), true)
        .map_err(anyhow::Error::msg)?;
    let reflected = positions
        .iter()
        .map(|p| Point3 { x: -p.x, ..*p })
        .collect::<Vec<_>>();
    let mirrored = from_directions(&graph, &meta, &dirs, Some(&reflected), true)
        .map_err(anyhow::Error::msg)?;
    assert!(matches!(result.metadata.atoms[0].chiral_tag, 1 | 2));
    assert_eq!(
        result.metadata.atoms[0].chiral_tag + mirrored.metadata.atoms[0].chiral_tag,
        3
    );
    assert_eq!(result.graph.atoms[0].explicit_hydrogens, 1);
    assert_eq!(result.valences[0].implicit_hydrogens, 0);
    assert_eq!(serde_json::to_value((&graph, &meta))?, before);
    Ok(())
}

#[test]
fn invalid_geometry_and_metadata_are_rejected_atomically() -> TestResult {
    let (graph, positions, dirs) = molecule();
    let meta = Metadata::unspecified(&graph);
    let before = serde_json::to_value((&graph, &meta))?;
    for number in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 1e101] {
        let mut bad = positions.clone();
        bad[0].x = number;
        assert!(from_directions(&graph, &meta, &dirs, Some(&bad), true).is_err());
    }
    assert!(from_directions(&graph, &meta, &dirs, Some(&[]), true).is_err());
    assert!(from_directions(&graph, &meta, &[], Some(&positions), true).is_err());
    let mut bad = meta.clone();
    bad.atoms[0].chiral_tag = 9;
    assert!(from_directions(&graph, &bad, &dirs, Some(&positions), true).is_err());
    assert_eq!(serde_json::to_value((&graph, &meta))?, before);
    Ok(())
}

#[test]
fn many_promotions_and_high_degree_atoms_stay_linear() -> TestResult {
    let (base, points, directions) = molecule();
    let mut graph = Graph {
        atoms: Vec::new(),
        bonds: Vec::new(),
    };
    let mut positions = Vec::new();
    let mut dirs = Vec::new();
    for i in 0..5000 {
        let offset = graph.atoms.len();
        graph.atoms.extend(base.atoms.clone());
        graph.bonds.extend(base.bonds.iter().map(|b| Bond {
            a: b.a + offset,
            b: b.b + offset,
            ..b.clone()
        }));
        positions.extend(points.iter().map(|p| Point3 {
            x: p.x + i as f64 * 4.0,
            ..*p
        }));
        dirs.extend_from_slice(&directions);
    }
    let result = from_directions(
        &graph,
        &Metadata::unspecified(&graph),
        &dirs,
        Some(&positions),
        true,
    )
    .map_err(anyhow::Error::msg)?;
    assert_eq!(
        result
            .graph
            .atoms
            .iter()
            .filter(|a| a.explicit_hydrogens == 1)
            .count(),
        5000
    );
    let mut star = Graph {
        atoms: vec![Atom::default(); 30_000],
        bonds: (1..30_000)
            .map(|b| Bond {
                a: 0,
                b,
                order: 1,
                aromatic: false,
            })
            .collect(),
    };
    // A dummy hub permits the high degree without imposing carbon valence.
    star.atoms[0].no_implicit = true;
    let positions = vec![Point3::default(); star.atoms.len()];
    let result = from_directions(
        &star,
        &Metadata::unspecified(&star),
        &vec![Direction::Wedge; star.bonds.len()],
        Some(&positions),
        true,
    )
    .map_err(anyhow::Error::msg)?;
    assert_eq!(result.metadata.atoms[0].chiral_tag, 0);
    for bond in star.bonds.iter_mut().skip(1) {
        bond.order = 2;
    }
    let mut directions = vec![Direction::None; star.bonds.len()];
    directions[0] = Direction::Up;
    let result = bond_stereo_from_directions(&star, &Metadata::unspecified(&star), &directions)
        .map_err(anyhow::Error::msg)?;
    assert!(result.bonds.iter().all(|b| b.stereo == 0));
    Ok(())
}
