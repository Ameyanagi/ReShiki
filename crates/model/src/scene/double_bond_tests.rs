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
