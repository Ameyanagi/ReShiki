use super::*;
use crate::{
    depict::attachment::AtomData,
    electronic::Hybridization,
    graph::Graph,
    ranking::Metadata,
    stereo::perception::{RingCache, RingKind},
};

fn seeded(
    smiles: &str,
    coordinates: &Coordinates,
) -> anyhow::Result<(Graph, Vec<AtomData>, Fragment)> {
    let graph = crate::smiles::parse(smiles)?.graph;
    let metadata = Metadata::unspecified(&graph);
    let data = vec![
        AtomData {
            hybridization: Hybridization::Sp3,
            cip_rank: None,
            chiral_rank: None,
        };
        graph.atoms.len()
    ];
    let rings = RingCache {
        kind: RingKind::Symmetric,
        atoms: Vec::new(),
    };
    let fragment = Input::new(&graph, &metadata, &data, &rings)?.from_coordinates(coordinates)?;
    Ok((graph, data, fragment))
}

fn assert_position(actual: Point, expected: Point) {
    assert!(
        (actual.x - expected.x).hypot(actual.y - expected.y) < 1.0e-6,
        "{actual:?} differs from {expected:?}"
    );
}

fn assert_mapped_atoms(fragment: &Fragment, coordinates: &Coordinates) {
    for (&id, &expected) in coordinates {
        assert_eq!(fragment.atoms[&id].location, expected);
        assert!(fragment.atoms[&id].fixed);
    }
}

// RDKit #9650: Depictor/catch_tests.cpp attachment regressions.
#[test]
fn attachments_use_the_exterior_gap_larger_than_pi() -> anyhow::Result<()> {
    for side in [-1.0, 1.0] {
        // The free 270-degree sector crosses the atan2 branch cut on one side.
        let coordinates = [
            (0, Point::default()),
            (1, Point { x: side, y: -1.0 }),
            (2, Point { x: side, y: 0.0 }),
            (3, Point { x: side, y: 1.0 }),
        ]
        .into();
        let (graph, data, fragment) = seeded("C(F)(Cl)(Br)C", &coordinates)?;
        let input = attachment::Input::new(&graph, &data)?;
        let fragment = input.add_non_ring_atom(&fragment, 4, 0, 1.5)?;
        assert_position(
            fragment.atoms[&4].location,
            Point {
                x: -side * 2.0_f64.sqrt(),
                y: 0.0,
            },
        );
        assert_mapped_atoms(&fragment, &coordinates);
    }
    Ok(())
}

#[test]
fn crowding_does_not_reverse_a_selected_attachment_gap() -> anyhow::Result<()> {
    let point_at = |angle: f64| Point {
        x: 1.5 * angle.cos(),
        y: 1.5 * angle.sin(),
    };
    for half_width in [PI / 2.0, 5.0 * PI / 9.0] {
        let step = (2.0 * PI - 2.0 * half_width) / 3.0;
        let crowded = point_at(half_width + step);
        // Atom 5 crowds the first new position in the 180- or 160-degree gap.
        let coordinates = [
            (0, Point::default()),
            (1, point_at(-half_width)),
            (2, point_at(0.0)),
            (3, point_at(half_width)),
            (
                5,
                Point {
                    x: crowded.x * 1.05,
                    y: crowded.y * 1.05,
                },
            ),
        ]
        .into();
        let (graph, data, mut fragment) = seeded("P(F)(Cl)(Br)(CC)C", &coordinates)?;
        let input = attachment::Input::new(&graph, &data)?;
        assert_eq!(fragment.atoms[&0].rotation_direction, 1);
        for (id, multiple) in [(4, 1.0), (6, 2.0)] {
            fragment = input.add_non_ring_atom(&fragment, id, 0, 1.5)?;
            assert_position(
                fragment.atoms[&id].location,
                point_at(half_width + multiple * step),
            );
            assert_eq!(fragment.atoms[&0].rotation_direction, 1);
            assert_mapped_atoms(&fragment, &coordinates);
        }
    }
    Ok(())
}

#[test]
fn attachments_retain_a_direction_chosen_to_avoid_crowding() -> anyhow::Result<()> {
    // The two opposite bonds leave either half-plane available; atom 4 crowds
    // the right side, so both remaining substituents belong on the left.
    let coordinates = [
        (0, Point::default()),
        (1, Point { x: 0.0, y: -1.5 }),
        (2, Point { x: 0.0, y: 1.5 }),
        (4, Point { x: 1.3, y: 0.75 }),
    ]
    .into();
    let (graph, data, mut fragment) = seeded("C(F)(Cl)(CC)C", &coordinates)?;
    let input = attachment::Input::new(&graph, &data)?;
    assert_eq!(fragment.atoms[&0].rotation_direction, 0);
    for (id, y) in [(3, 0.75), (5, -0.75)] {
        fragment = input.add_non_ring_atom(&fragment, id, 0, 1.5)?;
        assert_position(
            fragment.atoms[&id].location,
            Point {
                x: -3.0_f64.sqrt() * 0.75,
                y,
            },
        );
        assert_eq!(fragment.atoms[&0].rotation_direction, 1);
        assert_mapped_atoms(&fragment, &coordinates);
    }
    Ok(())
}
