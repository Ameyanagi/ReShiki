use super::*;
use crate::graph::{Atom, Bond};
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
fn cx_atrop_axis_parallel_to_z_retains_3d_geometry_through_bond_assignment() -> TestResult {
    // RDKit 2026.09.1 #9190 passes the actual conformer when bond directions
    // trigger axial assignment. Its 3D frame also supports an axis along Z.
    let imported = crate::smiles::read(
        "CC1=CC=CC(I)=C1N1C(C)=CC=C1Br |(0,1.732,0;-0.5,0.866,0;-1.5,0.866,0;-2,0,0;-1.5,-0.866,0;-0.5,-0.866,0;0,-1.732,0;0,0,0;0,0,1.5;0,0.951,2.191;0,1.9,2;0,0.588,3.309;0,-0.588,3.309;0,-0.951,2.191;0,-1.9,2),wU:7.7|",
    )?;
    let state = imported.prepared.state;
    let conformer = state.conformer.as_ref().unwrap();
    assert!(conformer.is_3d);
    assert_eq!(conformer.positions[7].x, conformer.positions[8].x);
    assert_eq!(conformer.positions[7].y, conformer.positions[8].y);
    assert_ne!(conformer.positions[7].z, conformer.positions[8].z);
    assert_eq!(state.metadata.bonds[7].stereo, 7);
    Ok(())
}

#[test]
fn cached_stereo_defers_geometry_validation_until_forced() -> TestResult {
    let mut state = crate::smiles::prepare("CC")?.state;
    state.conformer = Some(wedging::Conformer {
        positions: Vec::new(),
        is_3d: true,
    });
    let options = crate::stereo::perception::Options {
        clean: true,
        force: false,
        flag_possible: false,
    };
    let cached =
        crate::stereo::perception::perceive(&state, options).map_err(anyhow::Error::msg)?;
    assert_eq!(
        serde_json::to_value(&cached)?,
        serde_json::to_value(&state)?
    );
    assert!(
        crate::stereo::perception::perceive(
            &state,
            crate::stereo::perception::Options {
                force: true,
                ..options
            },
        )
        .is_err()
    );
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
