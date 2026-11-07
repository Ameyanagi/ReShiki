use super::*;
use crate::graph::{Atom, Bond};
type TestResult = anyhow::Result<()>;
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
    let (state, props, conf) = input().map_err(anyhow::Error::msg)?;
    let before = serde_json::to_value(&state)?;
    let whole = wedge_molecule(&state, &props, Some(&conf), false).map_err(anyhow::Error::msg)?;
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
    let single = wedge_bond(&state, &props, &conf, 0, 0).map_err(anyhow::Error::msg)?;
    assert_eq!(single.directions[0], Direction::Hash);
    assert_eq!(single.graph.bonds[0].a, 1);
    let perceived = super::super::from_directions(
        &whole.graph,
        &Metadata::unspecified(&whole.graph),
        &whole.directions,
        Some(&conf.positions),
        true,
    )
    .map_err(anyhow::Error::msg)?;
    assert_eq!(perceived.metadata.atoms[0].chiral_tag, 1);
    assert_eq!(serde_json::to_value(&state)?, before);
    Ok(())
}
#[test]
fn attachment_points_and_second_wedges_follow_preferences() -> TestResult {
    let (state, mut props, conf) = input().map_err(anyhow::Error::msg)?;
    props.attachment_points[1] = true;
    let result = wedge_molecule(&state, &props, Some(&conf), false).map_err(anyhow::Error::msg)?;
    assert_eq!(result.directions[0], Direction::None);
    assert!(wedged(result.directions[1]));
    props.attachment_points[1] = false;
    let result = wedge_molecule(&state, &props, Some(&conf), true).map_err(anyhow::Error::msg)?;
    assert_eq!(result.directions.iter().filter(|&&v| wedged(v)).count(), 2);
    assert_eq!(result.directions[3], Direction::Wedge);
    assert_eq!(result.graph.bonds[3].a, 0);
    Ok(())
}
#[test]
fn overlapping_geometry_preserves_direction_and_errors_are_atomic() -> TestResult {
    let (state, props, mut conf) = input().map_err(anyhow::Error::msg)?;
    conf.positions[1] = conf.positions[0];
    let out = wedge_molecule(&state, &props, Some(&conf), false).map_err(anyhow::Error::msg)?;
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
    let (state, props, conf) = input().map_err(anyhow::Error::msg)?;
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
        valences: graph.provisional_valences().map_err(anyhow::Error::msg)?,
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
    let result = wedge_molecule(&state, &props, Some(&conf), false).map_err(anyhow::Error::msg)?;
    assert_eq!(result.directions.iter().filter(|&&v| wedged(v)).count(), 1);
    assert!(with_work(&state, &props, Some(&conf), false, &mut Work(1000)).is_err());
    Ok(())
}

const SMILES: &str = "O=C1C=CNC(=O)N1C(=C)[C@]([C@H](C)Br)([C@@H](F)C)[C@H](Cl)C";

/// RDKit's AtropManyChirals.cxsmi in its macOS/Windows fresh layout, with
/// the N-vinyl axis N8-C9 (zero-based 7-8) and optionally moved atoms.
fn atrop_fixture(
    smiles: &str,
    moved: &[(usize, (f64, f64))],
) -> anyhow::Result<(WedgeState, WedgeProperties, Conformer, usize)> {
    let prepared = crate::smiles::prepare(smiles)?.state;
    let axis = prepared
        .graph
        .bonds
        .iter()
        .position(|b| (b.a, b.b) == (7, 8))
        .ok_or_else(|| anyhow::anyhow!("Missing axis"))?;
    let mut metadata = prepared.metadata;
    metadata.bonds[axis].stereo = 7;
    let state = WedgeState {
        directions: vec![Direction::None; prepared.graph.bonds.len()],
        graph: prepared.graph,
        metadata,
        rings: prepared.rings,
    };
    let props = WedgeProperties {
        attachment_points: vec![false; prepared.valences.len()],
        valences: prepared.valences,
    };
    let mut conf = Conformer {
        is_3d: false,
        positions: [
            (-61.08, 75.62),
            (-69.01, 34.38),
            (-108.70, 20.63),
            (-116.63, -20.62),
            (-84.88, -48.11),
            (-45.20, -34.36),
            (-29.66, -50.39),
            (-37.26, 6.88),
            (1.07, 24.04),
            (1.72, 66.04),
            (37.12, 2.48),
            (58.68, 38.53),
            (38.24, 75.22),
            (100.67, 37.88),
            (73.16, -19.08),
            (109.86, 1.36),
            (72.51, -61.07),
            (18.78, -35.30),
            (-1.74, -44.07),
            (42.34, -70.07),
        ]
        .into_iter()
        .map(|(x, y)| Point3 { x, y, z: 0.0 })
        .collect(),
    };
    for (atom, (x, y)) in moved {
        conf.positions[*atom] = Point3 {
            x: *x,
            y: *y,
            z: 0.0,
        };
    }
    Ok((state, props, conf, axis))
}

#[test]
fn a_drawing_that_hides_an_axis_is_refused() -> TestResult {
    // The reference wedges C11 towards axis end C9; read as C9's end, that
    // hash hides the N-vinyl axis whatever the N8 wedge. The raw chooser keeps
    // the reference pick; a drawing refuses it instead of losing the axis.
    let (state, props, conf, axis) = atrop_fixture(SMILES, &[])?;
    let raw = wedge_molecule(&state, &props, Some(&conf), false).map_err(anyhow::Error::msg)?;
    let mut cleared = raw.metadata.clone();
    cleared.bonds[axis].stereo = 0;
    let read =
        super::super::detect_atropisomers(&raw.graph, &cleared, &raw.directions, Some(&conf))?;
    assert_eq!(read.bonds[axis].stereo, 0);
    for (two_bonds, ring_preference) in [(false, false), (false, true), (true, true)] {
        assert!(
            generated::wedge_drawing(&state, &props, &conf, two_bonds, ring_preference).is_err()
        );
    }
    // Without the axis the same drawing is the reference's.
    let mut plain = state.clone();
    plain.metadata.bonds[axis].stereo = 0;
    let drawn = generated::wedge_drawing(&plain, &props, &conf, false, false)
        .map_err(anyhow::Error::msg)?;
    let reference =
        wedge_molecule(&plain, &props, Some(&conf), false).map_err(anyhow::Error::msg)?;
    assert_eq!(drawn.directions, reference.directions);
    Ok(())
}
