use super::*;
use crate::graph::{Atom, Bond};

fn fixture(points: &[(f64, f64)], edges: &[(usize, usize)]) -> (Graph, Fragment) {
    let graph = Graph {
        atoms: vec![
            Atom {
                atomic_number: 6,
                ..Atom::default()
            };
            points.len()
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
    };
    let fragment = Fragment {
        atoms: points
            .iter()
            .enumerate()
            .map(|(id, &(x, y))| {
                (
                    id,
                    EmbeddedAtom {
                        id,
                        location: Point { x, y },
                        normal: Point { x: 0.0, y: 1.0 },
                        angle: -1.0,
                        neighbor1: None,
                        neighbor2: None,
                        cis_trans_neighbor: None,
                        counter_clockwise: true,
                        rotation_direction: 0,
                        neighbors: Vec::new(),
                        density: -1.0,
                        fixed: false,
                    },
                )
            })
            .collect(),
        done: true,
        bounds: geometry::Bounds {
            positive_x: 0.0,
            negative_x: 0.0,
            positive_y: 0.0,
            negative_y: 0.0,
        },
        attachment_points: Vec::new(),
    };
    (graph, fragment)
}

fn positions(fragment: &Fragment) -> Vec<Point> {
    fragment.atoms.values().map(|a| a.location).collect()
}

fn distance(fragment: &Fragment, a: usize, b: usize) -> f64 {
    let a = fragment.atoms[&a].location;
    let b = fragment.atoms[&b].location;
    (a.x - b.x).hypot(a.y - b.y)
}

#[test]
fn path_angle_expansion_can_cross_a_temporary_extra_clash() -> anyhow::Result<()> {
    let (graph, fragment) = fixture(
        &[
            (0.0, 0.0),
            (1.5, 0.0),
            (0.75, -1.299038105676658),
            (0.0, 0.0),
            (1.448888739433602, -0.388228567653781),
            (0.698888739433601, 0.910809538022877),
        ],
        &[(0, 1), (1, 2), (2, 3), (3, 4), (4, 5)],
    );
    let metadata = Metadata::unspecified(&graph);
    let rings = RingCache::default();
    let input = Input::new(&graph, &metadata, &rings)?;
    // A single-step rejection gets stuck with non-bonded atoms 0.52 apart.
    let expanded = input.apply(&fragment, Stage::PathAngleExpansion)?;
    assert!(input.find(&expanded, true)?.pairs.is_empty());
    for bond in &graph.bonds {
        assert!((distance(&expanded, bond.a, bond.b) - 1.5).abs() < 1e-12);
    }
    Ok(())
}

#[test]
fn path_angle_expansion_rotates_the_free_side_and_preserves_fixed_atoms() -> anyhow::Result<()> {
    let (graph, mut fragment) = fixture(
        &[(1.5, 0.0), (0.0, 0.0), (1.4, 0.35), (2.8, 0.7)],
        &[(0, 1), (1, 2), (2, 3)],
    );
    fragment.atoms.get_mut(&0).unwrap().fixed = true;
    fragment.atoms.get_mut(&1).unwrap().fixed = true;
    let metadata = Metadata::unspecified(&graph);
    let rings = RingCache::default();
    let input = Input::new(&graph, &metadata, &rings)?;
    assert!(input.find(&fragment, false)?.pairs.contains(&(2, 0)));
    let before = fragment.clone();
    let expanded = input.apply(&fragment, Stage::PathAngleExpansion)?;
    assert_eq!(
        fragment, before,
        "The public stage must leave its input detached"
    );
    for id in [0, 1] {
        assert_eq!(expanded.atoms[&id].location, before.atoms[&id].location);
    }
    for id in [2, 3] {
        assert_ne!(expanded.atoms[&id].location, before.atoms[&id].location);
    }
    assert!(distance(&expanded, 0, 2) >= COLLISION_THRESHOLD);
    for (a, b) in [(1, 2), (2, 3)] {
        assert!((distance(&expanded, a, b) - distance(&before, a, b)).abs() < 1e-12);
    }
    // The smaller side is fixed; the larger side must rotate as a rigid unit.
    // Pinning its terminal atom as well leaves neither side free to move.
    fragment.atoms.get_mut(&3).unwrap().fixed = true;
    let pinned = input.apply(&fragment, Stage::PathAngleExpansion)?;
    assert_eq!(positions(&pinned), positions(&fragment));
    Ok(())
}

#[test]
fn path_angle_expansion_does_not_distort_cycles_or_bridged_sides() -> anyhow::Result<()> {
    for bridged in [false, true] {
        let mut points = vec![(0.0, 0.0), (1.5, 0.0), (0.25, 0.1), (-0.75, 1.3)];
        let mut edges = vec![(0, 1), (1, 2), (2, 3), (3, 0)];
        if bridged {
            points.push((-0.75, -1.3));
            edges.extend([(0, 4), (4, 2)]);
        }
        let (graph, fragment) = fixture(&points, &edges);
        let metadata = Metadata::unspecified(&graph);
        for known_rings in [false, true] {
            let mut rings = RingCache::default();
            if known_rings {
                rings.atoms.push(vec![0, 1, 2, 3]);
                if bridged {
                    rings.atoms.push(vec![0, 1, 2, 4]);
                }
            }
            let input = Input::new(&graph, &metadata, &rings)?;
            assert!(input.find(&fragment, false)?.pairs.contains(&(2, 0)));
            let expanded = input.apply(&fragment, Stage::PathAngleExpansion)?;
            assert_eq!(positions(&expanded), positions(&fragment));
        }
    }
    Ok(())
}

#[test]
fn path_angle_expansion_keeps_clear_drawings_and_shortened_bonds_in_place() -> anyhow::Result<()> {
    for length in [1.5, 0.45] {
        let (graph, fragment) =
            fixture(&[(-length, 0.0), (0.0, 0.0), (1.5, 0.0)], &[(0, 1), (1, 2)]);
        let metadata = Metadata::unspecified(&graph);
        let rings = RingCache::default();
        let input = Input::new(&graph, &metadata, &rings)?;
        let expected_pairs = if length < COLLISION_THRESHOLD {
            vec![(1, 0)]
        } else {
            vec![]
        };
        assert_eq!(input.find(&fragment, false)?.pairs, expected_pairs);
        let expanded = input.apply(&fragment, Stage::PathAngleExpansion)?;
        assert_eq!(positions(&expanded), positions(&fragment));
    }
    Ok(())
}

#[test]
fn spiro_refinement_replaces_ring_shortening_and_preserves_fixed_atoms() -> anyhow::Result<()> {
    use crate::{depict::expansion, smiles};
    let state = smiles::prepare("O=C1Cc2ccccc2C12CCC(=O)c1ccccc12")?.state;
    let initial = expansion::compute_initial(
        &state,
        &vec![None; state.graph.atoms.len()],
        None,
        expansion::Options {
            use_ring_templates: false,
            ..Default::default()
        },
    )?;
    let input = Input::new(
        &initial.state.graph,
        &initial.state.metadata,
        &initial.state.rings,
    )?;
    let original = &initial.fragments[0];
    let flipped = input.apply(original, Stage::BondAndSpiroFlip)?;
    let opened = input.apply(&flipped, Stage::OpenAngles)?;
    for fragment in [original, &flipped, &opened] {
        for bond in &state.graph.bonds {
            assert!((distance(fragment, bond.a, bond.b) - 1.5).abs() < 1e-12);
        }
    }
    let shortened = input.apply(&opened, Stage::ShortenBonds)?;
    assert!((distance(&shortened, 15, 16) - 0.8).abs() < 1e-12);
    for fixed in [false, true] {
        let mut original = original.clone();
        if fixed {
            original.atoms.get_mut(&0).unwrap().fixed = true;
            original.atoms.get_mut(&9).unwrap().fixed = true;
        }
        let repaired =
            input.preserve_spiro_rings_with_work(&original, &shortened, 1.5, &mut { MAX_WORK })?;
        for bond in &state.graph.bonds {
            assert!((distance(&repaired, bond.a, bond.b) - 1.5).abs() < 1e-12);
        }
        assert!(input.find(&repaired, true)?.pairs.is_empty());
        if fixed {
            for id in [0, 9] {
                assert_eq!(repaired.atoms[&id].location, original.atoms[&id].location);
            }
        }
    }
    Ok(())
}

#[test]
fn spiro_refinement_keeps_unaffected_layout_bits_and_budget_fallback() -> anyhow::Result<()> {
    use crate::{depict, smiles};
    let mut shortened_fallback = false;
    for text in [
        "C1Cc2ccccc2C12CCCc1ccccc12",
        "C1CCC2(C1)CCCCC2",
        "c1ccccc1.CCCCCCCC",
        "O=C1Cc2ccccc2C12CCC(=O)c1ccccc12",
    ] {
        let state = smiles::prepare(text)?.state;
        for work_limit in [100, 1_000, 5_000, 10_000, 15_000, 20_000, 50_000] {
            let run = |preserve_spiro_rings| {
                depict::compute(
                    &state,
                    &vec![None; state.graph.atoms.len()],
                    None,
                    depict::Options {
                        preserve_spiro_rings,
                        work_limit,
                        ..Default::default()
                    },
                )
            };
            match (run(false), run(true)) {
                (Ok(before), Ok(after)) => {
                    let distorted = state.rings.atoms.iter().any(|ring| {
                        ring.iter().enumerate().any(|(i, &a)| {
                            let b = ring[(i + 1) % ring.len()];
                            let (a, b) = (before.positions[a], before.positions[b]);
                            ((a.x - b.x).hypot(a.y - b.y) - 1.5).abs() > 1e-6
                        })
                    });
                    let identical = before.positions.iter().zip(&after.positions).all(|(a, b)| {
                        a.x.to_bits() == b.x.to_bits() && a.y.to_bits() == b.y.to_bits()
                    });
                    assert!(distorted || identical, "{text}/{work_limit}");
                    shortened_fallback |= distorted && identical;
                }
                (Err(before), Err(after)) => assert_eq!(before.to_string(), after.to_string()),
                result => panic!("refinement changed success for {text}/{work_limit}: {result:?}"),
            }
        }
    }
    assert!(
        shortened_fallback,
        "exercise exhaustion during the optional retry"
    );
    Ok(())
}

#[test]
fn spiro_refinement_ignores_an_irregular_pinned_ring_elsewhere() -> anyhow::Result<()> {
    use crate::{depict, smiles};
    let state = smiles::prepare("O=C1Cc2ccccc2C12CCC(=O)c1ccccc12.C1CCCCC1")?.state;
    // Pins join both components into one fragment; the cyclohexane edge is
    // not the standard length, which must not veto the squashed benzene.
    let pins = depict::geometry::Coordinates::from([
        (9, Point { x: 0., y: 0. }),
        (10, Point { x: 1.5, y: 0. }),
        (20, Point { x: 20., y: 0. }),
        (21, Point { x: 21.5001, y: 0. }),
    ]);
    let positions = depict::compute(
        &state,
        &vec![None; state.graph.atoms.len()],
        Some(&pins),
        depict::Options {
            canonical_orientation: false,
            preserve_spiro_rings: true,
            ..Default::default()
        },
    )?
    .positions;
    for ring in state.rings.atoms.iter().filter(|ring| ring.len() == 6) {
        if ring.contains(&21) {
            continue;
        }
        for (i, &a) in ring.iter().enumerate() {
            let (a, b) = (positions[a], positions[ring[(i + 1) % ring.len()]]);
            assert!(((a.x - b.x).hypot(a.y - b.y) - 1.5).abs() < 1e-9);
        }
    }
    Ok(())
}
