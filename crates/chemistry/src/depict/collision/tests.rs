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
fn crossed_bonds_use_the_farthest_topology_endpoints() -> anyhow::Result<()> {
    // RDKit #9378: the midpoints are 4.5 units apart, but the bonds cross.
    let (graph, fragment) = fixture(
        &[(-10.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)],
        &[(0, 1), (1, 2), (2, 3)],
    );
    let metadata = Metadata::unspecified(&graph);
    let rings = RingCache::default();
    let input = Input::new(&graph, &metadata, &rings)?;
    assert!(input.find(&fragment, false)?.pairs.is_empty());
    assert_eq!(input.find(&fragment, true)?.pairs, vec![(0, 3)]);
    Ok(())
}

#[test]
fn segment_intersections_include_overlap_and_endpoint_contact() -> anyhow::Result<()> {
    for (points, expected) in [
        ([(0., 0.), (4., 0.), (2., -2.), (2., 2.)], true),
        ([(0., 0.), (4., 0.), (2., 0.), (6., 0.)], true),
        ([(0., 0.), (4., 0.), (4., 0.), (4., 3.)], true),
        ([(0., 0.), (1., 0.), (1. + 0.5e-6, 0.), (2., 0.)], true),
        ([(0., 0.), (1., 0.), (1. + 2e-6, 0.), (2., 0.)], false),
        // The second segment straddles the first line beyond its endpoint.
        ([(0., 0.), (0.2, 0.), (0.3, -1.), (0.3, 1.)], false),
        ([(0., 0.), (4., 0.), (0., 1.), (4., 1.)], false),
        ([(0., 0.), (0., 4.), (1., 0.), (1., 4.)], false),
    ] {
        let [a, b, c, d] = points.map(|(x, y)| Point { x, y });
        for (first, second) in [(a, b), (b, a)] {
            for (third, fourth) in [(c, d), (d, c)] {
                assert_eq!(segments_intersect(first, second, third, fourth)?, expected);
                assert_eq!(segments_intersect(third, fourth, first, second)?, expected);
            }
        }
    }
    Ok(())
}

#[test]
fn adjacent_bonds_do_not_count_as_crossings() -> anyhow::Result<()> {
    let (graph, fragment) = fixture(&[(0., 0.), (4., 0.), (2., 0.)], &[(0, 1), (1, 2)]);
    let metadata = Metadata::unspecified(&graph);
    let rings = RingCache::default();
    let input = Input::new(&graph, &metadata, &rings)?;
    assert!(input.find(&fragment, true)?.pairs.is_empty());
    Ok(())
}

#[test]
fn bonded_atom_filter_preserves_density_and_crossing_representatives() -> anyhow::Result<()> {
    let (graph, mut fragment) = fixture(&[(0., 0.), (0.4, 0.)], &[(0, 1)]);
    let metadata = Metadata::unspecified(&graph);
    let rings = RingCache::default();
    let input = Input::new(&graph, &metadata, &rings)?;
    let report = input.find(&fragment, true)?;
    assert_eq!(report.pairs, vec![(1, 0)]);
    assert!(
        input
            .find_in_place(&mut fragment, true, false, &mut input.session())?
            .is_empty()
    );
    assert_eq!(Input::density(&fragment)?, report.total_density);

    // Every endpoint pair is bonded, so the farthest-distance tie keeps (0, 2).
    // Suppressing bonded atom proximity must still retain this crossing pair.
    let (graph, mut fragment) = fixture(
        &[(-2., 0.), (2., 0.), (0., -2.), (0., 2.)],
        &[(0, 1), (2, 3), (0, 2), (0, 3), (1, 2), (1, 3)],
    );
    let metadata = Metadata::unspecified(&graph);
    let input = Input::new(&graph, &metadata, &rings)?;
    assert_eq!(
        input.find_in_place(&mut fragment, true, false, &mut input.session())?,
        vec![(0, 2)]
    );
    Ok(())
}

#[test]
fn path_angle_expansion_counts_duplicate_skips_toward_iteration_limit() -> anyhow::Result<()> {
    // Collinear K2,6 edges yield 30 copies of (0, 1). Every shortest path
    // between them lies within a four-cycle and cannot expand. Upstream #9378
    // spends one iteration on each copy, reaching 20 before the chain crossing.
    let mut points = vec![(0., 100.), (10., 100.)];
    points.extend((2..8).map(|id| (20. + 10. * (id - 2) as f64, 100.)));
    points.extend([(-10., 0.), (1., 0.), (0., -1.), (0., 1.)]);
    let mut edges = (2..8).map(|id| (0, id)).collect::<Vec<_>>();
    edges.extend((2..8).map(|id| (1, id)));
    edges.extend([(8, 9), (9, 10), (10, 11)]);
    let (graph, fragment) = fixture(&points, &edges);
    let metadata = Metadata::unspecified(&graph);
    let mut rings = RingCache::default();
    for i in 2..8 {
        for j in i + 1..8 {
            rings.atoms.push(vec![0, i, 1, j]);
        }
    }
    for extra_trials in [false, true] {
        let input = Input::new(&graph, &metadata, &rings)?
            .with_path_angle_expansion_extra_trials(extra_trials);
        let pairs = input.find(&fragment, true)?.pairs;
        assert_eq!(pairs.len(), 31);
        assert!(pairs[..30].iter().all(|&pair| pair == (0, 1)));
        assert_eq!(pairs[30], (8, 11));
        let expanded = input.apply(&fragment, Stage::PathAngleExpansion)?;
        assert_eq!(positions(&expanded), positions(&fragment));
    }
    Ok(())
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
    let upstream = Input::new(&graph, &metadata, &rings)?
        .with_path_angle_expansion_extra_trials(false)
        .apply(&fragment, Stage::PathAngleExpansion)?;
    assert!(!input.find(&upstream, false)?.pairs.is_empty());
    let expanded = input.apply(&fragment, Stage::PathAngleExpansion)?;
    assert!(input.find(&expanded, true)?.pairs.is_empty());
    assert_ne!(positions(&expanded), positions(&upstream));
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
        let ranks = vec![None; state.graph.atoms.len()];
        let work_used = |preserve_spiro_rings| -> anyhow::Result<usize> {
            let mut remaining = MAX_WORK;
            depict::compute_with_work(
                &state,
                &ranks,
                None,
                depict::Options {
                    preserve_spiro_rings,
                    ..Default::default()
                },
                &mut remaining,
            )?;
            Ok(MAX_WORK - remaining)
        };
        // #9378 changes collision counts and their work cost. Derive an
        // allowance that stops just before the optional retry can finish.
        let mandatory_work = work_used(false)?;
        let refined_work = work_used(true)?;
        assert!(refined_work > mandatory_work);
        for work_limit in [100, 1_000, 5_000, 10_000, 15_000, 20_000, 50_000]
            .into_iter()
            .chain([mandatory_work, refined_work - 1])
        {
            let run = |preserve_spiro_rings| {
                depict::compute(
                    &state,
                    &ranks,
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
                    if work_limit == mandatory_work || work_limit == refined_work - 1 {
                        assert!(identical, "optional retry fallback for {text}/{work_limit}");
                    }
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
