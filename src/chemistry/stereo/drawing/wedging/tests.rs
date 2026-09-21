use super::*;
use crate::chemistry::graph::{Atom, Bond};
type TestResult = Result<(), Box<dyn std::error::Error>>;
fn input() -> Result<(WedgeState, WedgeProperties, Conformer), String> {
    let graph = Graph {
        atoms: [6, 9, 17, 35, 53]
            .into_iter()
            .map(|atomic_number| Atom {
                atomic_number,
                no_implicit: true,
                ..Atom::default()
            })
            .collect(),
        bonds: (1..5)
            .map(|a| Bond {
                a,
                b: 0,
                order: 1,
                aromatic: false,
            })
            .collect(),
    };
    let props = WedgeProperties {
        valences: graph.provisional_valences()?,
        attachment_points: vec![false; 5],
    };
    let mut meta = Metadata::unspecified(&graph);
    meta.atoms[0].chiral_tag = 1;
    let state = WedgeState {
        graph,
        metadata: meta,
        directions: vec![Direction::None; 4],
        rings: RingCache::default(),
    };
    let conf = Conformer {
        is_3d: false,
        positions: [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (-1.0, 0.0), (0.0, -1.0)]
            .into_iter()
            .map(|(x, y)| Point3 { x, y, z: 0.0 })
            .collect(),
    };
    Ok((state, props, conf))
}
#[test]
fn molecule_orients_the_wedge_but_single_bond_preserves_endpoints() -> TestResult {
    let (state, props, conf) = input()?;
    let before = serde_json::to_value(&state)?;
    let whole = wedge_molecule(&state, &props, Some(&conf), false)?;
    assert_eq!(
        whole.directions,
        [
            Direction::Hash,
            Direction::None,
            Direction::None,
            Direction::None
        ]
    );
    assert_eq!(whole.graph.bonds[0].a, 0);
    let single = wedge_bond(&state, &props, &conf, 0, 0)?;
    assert_eq!(single.directions[0], Direction::Hash);
    assert_eq!(single.graph.bonds[0].a, 1);
    let perceived = super::super::from_directions(
        &whole.graph,
        &Metadata::unspecified(&whole.graph),
        &whole.directions,
        Some(&conf.positions),
        true,
    )?;
    assert_eq!(perceived.metadata.atoms[0].chiral_tag, 1);
    assert_eq!(serde_json::to_value(&state)?, before);
    Ok(())
}
#[test]
fn attachment_points_and_second_wedges_follow_preferences() -> TestResult {
    let (state, mut props, conf) = input()?;
    props.attachment_points[1] = true;
    let result = wedge_molecule(&state, &props, Some(&conf), false)?;
    assert_eq!(result.directions[0], Direction::None);
    assert!(wedged(result.directions[1]));
    props.attachment_points[1] = false;
    let result = wedge_molecule(&state, &props, Some(&conf), true)?;
    assert_eq!(result.directions.iter().filter(|&&v| wedged(v)).count(), 2);
    assert_eq!(result.directions[3], Direction::Wedge);
    assert_eq!(result.graph.bonds[3].a, 0);
    Ok(())
}
#[test]
fn overlapping_geometry_preserves_direction_and_errors_are_atomic() -> TestResult {
    let (state, props, mut conf) = input()?;
    conf.positions[1] = conf.positions[0];
    let out = wedge_molecule(&state, &props, Some(&conf), false)?;
    assert_eq!(out.directions, state.directions);
    let before = serde_json::to_value((&state, &props))?;
    assert!(wedge_molecule(&state, &props, None, false).is_err());
    assert!(with_work(&state, &props, Some(&conf), true, &mut Work(0)).is_err());
    assert!(wedge_bond(&state, &props, &conf, usize::MAX, 0).is_err());
    assert!(wedge_bond(&state, &props, &conf, 0, usize::MAX).is_err());
    conf.positions[0].x = f64::NAN;
    assert!(wedge_molecule(&state, &props, Some(&conf), false).is_err());
    assert_eq!(serde_json::to_value((&state, &props))?, before);
    Ok(())
}
#[test]
fn invalid_caches_and_annotations_return_errors() -> TestResult {
    let (state, props, conf) = input()?;
    let mut bad = state.clone();
    bad.directions.pop();
    assert!(wedge_molecule(&bad, &props, Some(&conf), false).is_err());
    let mut bad = state.clone();
    bad.rings = RingCache {
        kind: RingKind::Basis,
        atoms: vec![vec![0, 1, 2]],
    };
    assert!(wedge_molecule(&bad, &props, Some(&conf), false).is_err());
    let mut bad = props.clone();
    bad.attachment_points.pop();
    assert!(wedge_molecule(&state, &bad, Some(&conf), false).is_err());
    let mut bad = props.clone();
    bad.valences[0].implicit_hydrogens = u32::MAX;
    assert!(wedge_molecule(&state, &bad, Some(&conf), false).is_err());
    Ok(())
}
#[test]
fn large_neighbor_ordering_uses_bounded_dynamic_storage() -> TestResult {
    let n = 30_000;
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
                a: 0,
                b,
                order: 1,
                aromatic: false,
            })
            .collect(),
    };
    let props = WedgeProperties {
        valences: graph.provisional_valences()?,
        attachment_points: vec![false; n],
    };
    let mut meta = Metadata::unspecified(&graph);
    meta.atoms[0].chiral_tag = 1;
    let state = WedgeState {
        graph,
        metadata: meta,
        directions: vec![Direction::None; n - 1],
        rings: RingCache {
            kind: RingKind::Basis,
            atoms: Vec::new(),
        },
    };
    let mut points = vec![Point3::default()];
    for i in 1..n {
        let a = 2.0 * PI * (i as f64) / (n as f64);
        points.push(Point3 {
            x: a.cos(),
            y: a.sin(),
            z: 0.0,
        });
    }
    let conf = Conformer {
        positions: points,
        is_3d: false,
    };
    let result = wedge_molecule(&state, &props, Some(&conf), false)?;
    assert_eq!(result.directions.iter().filter(|&&v| wedged(v)).count(), 1);
    assert!(with_work(&state, &props, Some(&conf), false, &mut Work(1000)).is_err());
    Ok(())
}
