use super::*;
use crate::bonds::DoublePosition;

fn chain(points: &[(f32, f32)], edges: &[(usize, usize, u8)]) -> Document {
    let mut doc = Document::default();
    let ids: Vec<_> = points
        .iter()
        .map(|&(x, y)| doc.add_atom("C", Point::new(x, y)))
        .collect();
    for &(a, b, order) in edges {
        doc.add_bond(ids[a], ids[b], order, "plain");
    }
    doc
}

/// Euclidean distance to the measured glyph's conservative ink silhouette,
/// independent of the renderer's ray/box intersection arithmetic.
fn glyph_distance(point: Point, ((lo, hi), radius): ((Point, Point), f32)) -> f32 {
    let x = (point.x - (lo.x + hi.x) * 0.5).abs() - (hi.x - lo.x) * 0.5 + radius;
    let y = (point.y - (lo.y + hi.y) * 0.5).abs() - (hi.y - lo.y) * 0.5 + radius;
    x.max(0.).hypot(y.max(0.)) + x.max(y).min(0.) - radius
}

fn rotated_carbonyl(degrees: f32, reversed: bool) -> Document {
    let mut doc = chain(&[(0., 0.), (42., 0.)], &[(0, 1, 2)]);
    doc.atoms[1].element = "O".into();
    doc.drawing_style.font_family = "Liberation Sans".into();
    if reversed {
        doc.bonds[0].reverse();
    }
    let ids = doc.all_ids();
    crate::editing::transform_about(&mut doc, &ids, Point::default(), 1., degrees);
    doc
}

fn rendered_double_rails(doc: &Document) -> Vec<(Point, Point, f32)> {
    let rails: Vec<_> = primitives(doc)
        .into_iter()
        .filter_map(|part| match part {
            Primitive::Line(a, b, width) => Some((a, b, width)),
            _ => None,
        })
        .collect();
    assert_eq!(
        rails.len(),
        2,
        "The full scene must retain both double rails"
    );
    rails
}

#[test]
fn rotating_carbonyl_rails_do_not_stop_at_empty_oxygen_box_corners() {
    crate::atom_labels::test_support::with_fonts(|| {
        for reversed in [false, true] {
            for degrees in (0..360).step_by(15) {
                let doc = rotated_carbonyl(degrees as f32, reversed);
                assert!(
                    (doc.atoms[0].position.distance(doc.atoms[1].position) - 42.).abs() < 0.001
                );
                let oxygen = &doc.atoms[1];
                let shapes = atom_label_ink_shapes(oxygen, &doc);
                assert_eq!(shapes.len(), 1, "The fixture must have just the O glyph");
                let shape = shapes[0];
                assert!(shape.1 > 5., "The pinned O must have rounded corners");
                let saved = doc.clone();
                let rails = rendered_double_rails(&doc);
                let margin = doc.drawing_style.world(doc.drawing_style.margin_width_pt);
                for (a, b, width) in rails {
                    let end = if a.distance(oxygen.position) < b.distance(oxygen.position) {
                        a
                    } else {
                        b
                    };
                    let distance = glyph_distance(end, shape);
                    assert!(
                        distance <= margin + width * 0.5 + 0.15,
                        "Excess O gap: angle={degrees}, reversed={reversed}, distance={distance}, margin={margin}, width={width}"
                    );
                    assert!(
                        distance >= width * 0.5 - 0.03,
                        "The round rail cap overlaps O ink: angle={degrees}, reversed={reversed}"
                    );
                }
                assert_eq!(doc, saved, "Rendering must leave the bond geometry intact");
            }
        }
    });
}

#[test]
fn rotating_double_rails_keep_both_round_and_rectangular_labels_clear() {
    crate::atom_labels::test_support::with_fonts(|| {
        for first in ["O", "N"] {
            for degrees in (0..360).step_by(15) {
                let mut doc = chain(&[(0., 0.), (70., 0.)], &[(0, 1, 2)]);
                doc.atoms[0].element = first.into();
                doc.atoms[1].element = "O".into();
                doc.drawing_style.font_family = "Liberation Sans".into();
                let ids = doc.all_ids();
                crate::editing::transform_about(
                    &mut doc,
                    &ids,
                    Point::default(),
                    1.,
                    degrees as f32,
                );
                let first_shapes = atom_label_ink_shapes(&doc.atoms[0], &doc);
                if first == "N" {
                    assert!(first_shapes.iter().all(|(_, radius)| *radius < 0.1));
                }
                let shapes: Vec<_> = first_shapes
                    .into_iter()
                    .chain(atom_label_ink_shapes(&doc.atoms[1], &doc))
                    .collect();
                let rails = rendered_double_rails(&doc);
                // Sampling each actual scene stroke also covers its round caps.
                // Unlike a length invariant, this remains valid for upright N/O
                // glyphs whose visible outlines change with the bond direction.
                for (a, b, width) in rails {
                    for step in 0..=200 {
                        let t = step as f32 / 200.;
                        let point = a.offset((b.x - a.x) * t, (b.y - a.y) * t);
                        for &shape in &shapes {
                            assert!(
                                glyph_distance(point, shape) >= width * 0.5 - 0.03,
                                "Rail ink overlaps {first}=O label: angle={degrees}, point={point:?}"
                            );
                        }
                    }
                }
            }
        }
    });
}

#[test]
fn rotating_carbonyl_visible_rail_tips_remain_mechanism_arrow_targets() {
    crate::atom_labels::test_support::with_fonts(|| {
        for reversed in [false, true] {
            for degrees in [0., 15., 30., 45., 60., 75., 90., 135., 225., 315.] {
                let doc = rotated_carbonyl(degrees, reversed);
                let oxygen = &doc.atoms[1];
                let boxes = atom_label_ink_boxes(oxygen, &doc);
                for (a, b, _) in rendered_double_rails(&doc) {
                    let (tip, other) = if a.distance(oxygen.position) < b.distance(oxygen.position)
                    {
                        (a, b)
                    } else {
                        (b, a)
                    };
                    // Use real scene ink just inside the round cap, rather than
                    // guessing where the end should be from clipping helpers.
                    let step = 0.1 / tip.distance(other);
                    let point = tip.offset((other.x - tip.x) * step, (other.y - tip.y) * step);
                    assert!(boxes.iter().all(|(lo, hi)| {
                        point.x < lo.x || point.x > hi.x || point.y < lo.y || point.y > hi.y
                    }));
                    let picked = crate::arrow_anchors::pick(&doc, point, 0.02);
                    assert!(
                        matches!(
                            picked,
                            Some(crate::arrow_anchors::Pick::Bond { a: 1, b: 2, .. })
                        ),
                        "Visible C=O tip was not a bond target: angle={degrees}, reversed={reversed}, point={point:?}, picked={picked:?}"
                    );
                }
            }
        }
    });
}

#[test]
fn rotating_carbonyl_small_margins_keep_label_box_ownership() {
    crate::atom_labels::test_support::with_fonts(|| {
        for margin in [0., 0.001, 0.2] {
            for degrees in (0..360).step_by(15) {
                let mut doc = rotated_carbonyl(degrees as f32, false);
                doc.drawing_style.margin_width_pt = margin;
                let oxygen = &doc.atoms[1];
                let boxes = atom_label_ink_boxes(oxygen, &doc);
                for (a, b, _) in rendered_double_rails(&doc) {
                    for step in 0..=200 {
                        let t = step as f32 / 200.;
                        let point = a.offset((b.x - a.x) * t, (b.y - a.y) * t);
                        assert!(
                            boxes.iter().all(|(lo, hi)| {
                                point.x <= lo.x + 0.01
                                    || point.x >= hi.x - 0.01
                                    || point.y <= lo.y + 0.01
                                    || point.y >= hi.y - 0.01
                            }),
                            "A rail center entered label-owned ink bounds: angle={degrees}, margin={margin}, point={point:?}"
                        );
                    }
                }
                let (lo, hi) = boxes[0];
                let inset = (hi.x - lo.x).min(hi.y - lo.y) * 0.02;
                let corner = lo.offset(inset, inset);
                assert!(glyph_distance(corner, atom_label_ink_shapes(oxygen, &doc)[0]) > 0.);
                assert!(matches!(
                    crate::arrow_anchors::pick(&doc, corner, 0.02),
                    Some(crate::arrow_anchors::Pick::Atom(2))
                ));
            }
        }
    });
}

#[test]
fn chain_doubles_count_substituents_and_respect_the_style_and_explicit_positions() {
    // E-but-2-ene has one substituent on each side. ChemDraw selects the
    // negative side of the stored endpoint order, independent of atom IDs.
    let mut doc = chain(
        &[(0., 0.), (42., 0.), (-21., -36.), (63., 36.), (-21., 36.)],
        &[(0, 1, 2), (0, 2, 1), (1, 3, 1)],
    );
    assert_eq!(
        effective_double_position(&doc, &doc.bonds[0]),
        DoublePosition::Left
    );
    let mut reversed = doc.bonds[0].clone();
    reversed.reverse();
    assert_eq!(
        effective_double_position(&doc, &reversed),
        DoublePosition::Left
    );
    // Two methyl groups below the alkene beat the single methyl above it.
    doc.add_bond(doc.atoms[0].id, doc.atoms[4].id, 1, "plain");
    assert_eq!(
        effective_double_position(&doc, &doc.bonds[0]),
        DoublePosition::Right
    );
    assert_eq!(
        effective_double_position(&doc, &reversed),
        DoublePosition::Left
    );
    doc.bonds[0].order = 7;
    assert_eq!(
        effective_double_position(&doc, &doc.bonds[0]),
        DoublePosition::Right
    );
    doc.drawing_style.center_chain_double_bonds = true;
    assert_eq!(
        effective_double_position(&doc, &doc.bonds[0]),
        DoublePosition::Center
    );
    for position in [
        DoublePosition::Left,
        DoublePosition::Right,
        DoublePosition::Center,
    ] {
        doc.bonds[0].double_position = position;
        assert_eq!(effective_double_position(&doc, &doc.bonds[0]), position);
    }
}

#[test]
fn terminal_carbon_doubles_offset_but_heteroatoms_and_labels_stay_centered() {
    for element in ["C", "O", "N", "S"] {
        let mut doc = chain(&[(0., 0.), (42., 0.), (-21., 36.)], &[(0, 1, 2), (0, 2, 1)]);
        doc.atoms[1].element = element.into();
        assert_eq!(
            effective_double_position(&doc, &doc.bonds[0]),
            if element == "C" {
                DoublePosition::Right
            } else {
                DoublePosition::Center
            },
            "terminal {element}"
        );
        doc.atoms[1].display.carbons = Some(crate::atom_labels::Carbons::All);
        assert_eq!(
            effective_double_position(&doc, &doc.bonds[0]),
            DoublePosition::Center
        );
    }
    let mut doc = chain(&[(0., 0.), (42., 0.), (-21., 36.)], &[(0, 1, 2), (0, 2, 1)]);
    let mut reversed = doc.bonds[0].clone();
    reversed.reverse();
    assert_eq!(
        effective_double_position(&doc, &reversed),
        DoublePosition::Left
    );
    doc.drawing_style.center_chain_double_bonds = true;
    assert_eq!(
        effective_double_position(&doc, &doc.bonds[0]),
        DoublePosition::Center
    );
    doc.drawing_style.center_chain_double_bonds = false;
    doc.bonds[0].order = 3;
    doc.atoms[1].element = "N".into();
    assert_eq!(
        effective_double_position(&doc, &doc.bonds[0]),
        DoublePosition::Center
    );
}

#[test]
fn explicit_terminal_hydrogens_do_not_count_as_alkene_substituents() {
    // Explicit H positions must not outvote the heavy substituent, even when
    // both are drawn on the opposite side of the alkene axis.
    let mut doc = chain(
        &[(0., 0.), (42., 0.), (-21., 36.), (63., -36.), (21., -36.)],
        &[(0, 1, 2), (0, 2, 1), (1, 3, 1), (1, 4, 1)],
    );
    doc.atoms[3].element = "H".into();
    doc.atoms[4].element = "H".into();
    assert_eq!(
        effective_double_position(&doc, &doc.bonds[0]),
        DoublePosition::Right
    );
    doc.atoms[1].element = "N".into();
    assert_eq!(
        effective_double_position(&doc, &doc.bonds[0]),
        DoublePosition::Center
    );
}

#[test]
fn non_covalent_neighbors_leave_terminal_carbonyls_centered() {
    for (order, display) in [(0, "dotted"), (5, "plain")] {
        let mut doc = chain(
            &[(0., 0.), (42., 0.), (-21., -36.), (-21., 36.), (63., 36.)],
            &[(0, 1, 2), (0, 2, 1), (0, 3, 1)],
        );
        doc.atoms[1].element = "O".into();
        doc.atoms[4].element = "N".into();
        doc.atoms[4].label_h = 1;
        // An N-H···O interaction must not make acetone's oxygen substituted.
        doc.add_bond(doc.atoms[4].id, doc.atoms[1].id, order, display);
        assert_eq!(
            effective_double_position(&doc, &doc.bonds[0]),
            DoublePosition::Center,
            "non-covalent order {order}"
        );
    }
}

#[test]
fn non_covalent_neighbors_do_not_vote_on_chain_double_side() {
    for (order, display) in [(0, "dotted"), (5, "plain")] {
        let mut doc = chain(
            &[(0., 0.), (42., 0.), (-21., -36.), (63., 36.), (21., 36.)],
            &[(0, 1, 2), (0, 2, 1), (1, 3, 1)],
        );
        doc.atoms[4].element = "N".into();
        doc.add_bond(doc.atoms[0].id, doc.atoms[4].id, order, display);
        assert_eq!(
            effective_double_position(&doc, &doc.bonds[0]),
            DoublePosition::Left,
            "non-covalent order {order}"
        );
    }
}

#[test]
fn non_covalent_neighbors_do_not_trim_an_explicit_carbonyl_offset() {
    for (order, display) in [(0, "dotted"), (5, "plain")] {
        let mut doc = chain(&[(0., 0.), (42., 0.), (-21., 36.)], &[(0, 1, 2), (0, 2, 1)]);
        doc.atoms[1].element = "O".into();
        doc.bonds[0].double_position = DoublePosition::Right;
        let donor = doc.add_atom("N", Point::new(63., 36.));
        let expected = bond_rails(&FaceIndex::new(&doc), &doc.bonds[0])[1].trim;
        assert_eq!(expected.1, 0.);
        doc.add_bond(donor, doc.atoms[1].id, order, display);
        assert_eq!(
            bond_rails(&FaceIndex::new(&doc), &doc.bonds[0])[1].trim,
            expected,
            "non-covalent order {order}"
        );
    }
}

#[test]
fn unsubstituted_ethene_stays_centered() {
    let doc = chain(&[(0., 0.), (42., 0.)], &[(0, 1, 2)]);
    assert_eq!(
        effective_double_position(&doc, &doc.bonds[0]),
        DoublePosition::Center
    );
}

#[test]
fn attached_ring_center_takes_precedence_over_the_substituent_majority() {
    // A six-ring above a styryl alkene, joined through a single bond. Two
    // substituents below would otherwise select the opposite side.
    let doc = chain(
        &[
            (0., 0.),
            (42., 0.),
            (-21., -36.),
            (63., 36.),
            (-21., 36.),
            (-63., -36.),
            (-84., -72.),
            (-63., -108.),
            (-21., -108.),
            (0., -72.),
        ],
        &[
            (0, 1, 2),
            (0, 2, 1),
            (1, 3, 1),
            (0, 4, 1),
            (2, 5, 1),
            (5, 6, 2),
            (6, 7, 1),
            (7, 8, 2),
            (8, 9, 1),
            (9, 2, 2),
        ],
    );
    assert_eq!(
        effective_double_position(&doc, &doc.bonds[0]),
        DoublePosition::Left
    );
}

fn fused() -> Document {
    chain(
        &[
            (0., 0.),
            (42., 0.),
            (55., -40.),
            (21., -64.),
            (-13., -40.),
            (63., 36.),
            (42., 72.),
            (0., 72.),
            (-21., 36.),
        ],
        &[
            (0, 1, 2),
            (1, 2, 1),
            (2, 3, 2),
            (3, 4, 1),
            (4, 0, 1),
            (1, 5, 1),
            (5, 6, 2),
            (6, 7, 1),
            (7, 8, 2),
            (8, 0, 1),
        ],
    )
}

#[test]
fn fusion_doubles_prefer_unsaturation_then_six_members() {
    let mut doc = fused();
    assert_eq!(
        effective_double_position(&doc, &doc.bonds[0]),
        DoublePosition::Right
    );
    doc.bonds[8].order = 1;
    // Equal counts still prefer the six-ring, over the old smallest five-ring.
    assert_eq!(
        effective_double_position(&doc, &doc.bonds[0]),
        DoublePosition::Right
    );
    doc.bonds[6].order = 1;
    assert_eq!(
        effective_double_position(&doc, &doc.bonds[0]),
        DoublePosition::Left
    );
    // Aromatic bonds contribute to the same count as Kekule doubles.
    doc.bonds[6].order = 4;
    doc.bonds[8].order = 4;
    assert_eq!(
        effective_double_position(&doc, &doc.bonds[0]),
        DoublePosition::Right
    );
}

#[test]
fn small_ring_inner_rail_endpoints_follow_bisectors_and_large_rings_keep_their_trim() {
    for (size, alternating) in [(3, false), (4, false), (5, false), (6, false), (6, true)] {
        let mut doc = Document::default();
        let half_angle = std::f32::consts::PI / size as f32;
        let radius = 21. / half_angle.sin();
        let ids: Vec<_> = (0..size)
            .map(|i| {
                let angle = -std::f32::consts::FRAC_PI_2 - half_angle + 2. * half_angle * i as f32;
                doc.add_atom(
                    "C",
                    Point::new(
                        21. + radius * angle.cos(),
                        radius * half_angle.cos() + radius * angle.sin(),
                    ),
                )
            })
            .collect();
        for i in 0..size {
            doc.add_bond(
                ids[i],
                ids[(i + 1) % size],
                if i == 0 || alternating && i % 2 == 0 {
                    2
                } else {
                    1
                },
                "plain",
            );
        }
        let spacing = doc.drawing_style.bond_length_world * doc.drawing_style.bond_spacing_ratio;
        let theta = std::f32::consts::PI - 2. * half_angle;
        let trim = if size <= 4 {
            spacing / (theta / 2.).tan()
        } else {
            spacing * 0.75
        };
        let expected = (Point::new(trim, spacing), Point::new(42. - trim, spacing));
        assert!(
            primitives(&doc)
                .iter()
                .any(|p| matches!(p, Primitive::Line(a, b, _)
            if a.distance(expected.0) < 0.001 && b.distance(expected.1) < 0.001)),
            "{size}-ring rail missed angle bisectors"
        );
    }
}

#[test]
fn short_acute_neighbor_keeps_the_inner_rail_and_its_clearance() {
    use crate::graphics::segment_distance;

    let doc = chain(
        &[(0., 0.), (42., 0.), (9.84808, 1.73648)],
        &[(0, 1, 2), (0, 2, 1)],
    );
    let spacing = doc.drawing_style.bond_length_world * doc.drawing_style.bond_spacing_ratio;
    let (a, b) = primitives(&doc)
        .into_iter()
        .find_map(|p| match p {
            Primitive::Line(a, b, _)
                if (a.y - spacing).abs() < 0.001 && (b.y - spacing).abs() < 0.001 =>
            {
                Some((a, b))
            }
            _ => None,
        })
        .expect("The short neighbor must not erase the inner rail");
    let (origin, end) = (doc.atoms[0].position, doc.atoms[2].position);
    let gap = [
        segment_distance(a, origin, end),
        segment_distance(b, origin, end),
        segment_distance(origin, a, b),
        segment_distance(end, a, b),
    ]
    .into_iter()
    .fold(f32::INFINITY, f32::min);
    assert!(gap >= spacing - 0.001, "Inner rail crowds neighbor: {gap}");
}

#[test]
fn ring_inner_rail_clears_an_acute_second_neighbor_at_either_end() {
    // A bridge can meet the rail past the usual ring neighbor's setback.
    for (side, endpoint) in [(-1., 0), (-1., 1), (1., 0), (1., 1)] {
        let mut doc = chain(
            &[
                (0., 0.),
                (42., 0.),
                (63., 36. * side),
                (42., 72. * side),
                (0., 72. * side),
                (-21., 36. * side),
            ],
            &[
                (0, 1, 2),
                (1, 2, 1),
                (2, 3, 1),
                (3, 4, 1),
                (4, 5, 1),
                (5, 0, 1),
            ],
        );
        let spacing = doc.drawing_style.bond_length_world * doc.drawing_style.bond_spacing_ratio;
        let inner = |doc: &Document| {
            primitives(doc)
                .into_iter()
                .find_map(|p| match p {
                    Primitive::Line(a, b, _)
                        if (a.y - spacing * side).abs() < 0.001
                            && (b.y - spacing * side).abs() < 0.001 =>
                    {
                        Some(if a.x < b.x { [a, b] } else { [b, a] })
                    }
                    _ => None,
                })
                .expect("The ring must retain its inner rail")
        };
        let normal = inner(&doc);
        let origin = doc.atoms[endpoint].position;
        let direction = if endpoint == 0 { 1. } else { -1. };
        let angle = 25_f32.to_radians();
        let bridge = doc.add_atom(
            "C",
            origin.offset(42. * angle.cos() * direction, 42. * angle.sin() * side),
        );
        doc.add_bond(doc.atoms[endpoint].id, bridge, 1, "plain");
        let ends = inner(&doc);
        assert!(ends[1 - endpoint].distance(normal[1 - endpoint]) < 0.001);
        let (dx, dy) = (angle.cos() * direction, angle.sin() * side);
        let gap = side
            * direction
            * (dy * (ends[endpoint].x - origin.x) - dx * (ends[endpoint].y - origin.y));
        assert!(
            gap >= spacing - 0.001,
            "Inner rail touches the bridge: side={side}, endpoint={endpoint}, gap={gap}"
        );
    }
}

#[test]
fn forced_terminal_offset_leaves_the_bare_end_untrimmed() {
    let mut doc = chain(&[(0., 0.), (42., 0.), (-21., 36.)], &[(0, 1, 2), (0, 2, 1)]);
    doc.bonds[0].double_position = DoublePosition::Right;
    let spacing = doc.drawing_style.bond_length_world * doc.drawing_style.bond_spacing_ratio;
    let end = Point::new(42., spacing);
    assert!(
        primitives(&doc)
            .iter()
            .any(|p| matches!(p, Primitive::Line(_, b, _) if b.distance(end) < 0.001))
    );
    let rails = bond_rails(&FaceIndex::new(&doc), &doc.bonds[0]);
    assert_eq!(rails[1].trim, (spacing * 0.75, 0.));
}

#[test]
fn ring_doubles_do_not_walk_long_substituents_for_an_opposite_ring() {
    // Cyclopentene whose double-bond carbons each carry a long chain below.
    let mut doc = chain(
        &[(0., 0.), (42., 0.), (55., -40.), (21., -64.), (-13., -40.)],
        &[(0, 1, 2), (1, 2, 1), (2, 3, 1), (3, 4, 1), (4, 0, 1)],
    );
    for (start, dx) in [(0, -1.), (1, 1.)] {
        let mut previous = doc.atoms[start].id;
        for i in 1..1000 {
            let next = doc.add_atom("C", Point::new(21. + dx * (21. + i as f32), 40.));
            doc.add_bond(previous, next, 1, "plain");
            previous = next;
        }
    }
    let before = RING_SEARCH_VISITS.with(std::cell::Cell::get);
    assert_eq!(
        effective_double_position(&doc, &doc.bonds[0]),
        DoublePosition::Left
    );
    assert!(RING_SEARCH_VISITS.with(std::cell::Cell::get) - before < 100);
}
