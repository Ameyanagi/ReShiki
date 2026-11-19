use super::*;

#[test]
fn pasted_stereo_groups_get_fresh_numbers_and_keep_label_visibility() {
    use crate::stereo_groups::{Kind, StereoGroup};
    let mut destination = Document::default();
    destination.atom_labels.stereo_groups = false;
    let and = StereoGroup {
        kind: Kind::And,
        number: 1,
    };
    let existing = destination.add_atom("C", Point::default());
    destination.atom_mut(existing).unwrap().stereo_group = Some(and);
    let mut source = Document::default();
    for group in [
        and,
        and,
        StereoGroup {
            kind: Kind::And,
            number: 2,
        },
        StereoGroup {
            kind: Kind::Or,
            number: 1,
        },
    ] {
        let id = source.add_atom("C", Point::default());
        source.atom_mut(id).unwrap().stereo_group = Some(group);
    }
    let before = source.clone();
    let ids = append(&mut destination, &source, Point::new(84., 0.));
    assert_eq!(ids.len(), 4);
    let groups: Vec<_> = ids
        .iter()
        .map(|&id| destination.atom(id).unwrap().stereo_group.unwrap())
        .collect();
    assert_eq!(groups[0], groups[1]);
    assert_ne!(groups[0], and);
    assert_ne!(groups[2], and);
    assert_ne!(groups[0], groups[2]);
    assert_eq!(groups[3], source.atoms[3].stereo_group.unwrap());
    assert_eq!(destination.atom(existing).unwrap().stereo_group, Some(and));
    for id in ids {
        assert_eq!(
            destination.atom(id).unwrap().display.stereo_group.show,
            Some(true)
        );
    }
    assert_eq!(source, before);
}

#[test]
fn preserved_append_keeps_stereo_groups_exact() {
    use crate::stereo_groups::{Kind, StereoGroup};
    let mut source = Document::default();
    let id = source.add_atom("C", Point::default());
    let group = StereoGroup {
        kind: Kind::And,
        number: 1,
    };
    source.atom_mut(id).unwrap().stereo_group = Some(group);
    let (part, _, _) = preserved_append_part(&source, &source, Point::new(84., 0.)).unwrap();
    assert_eq!(part.atoms[0].stereo_group, Some(group));
    assert_eq!(part.atoms[0].display, source.atoms[0].display);
}

#[test]
fn fixed_hydrogen_positions_follow_reflections_and_rotations() {
    use crate::atom_labels::HydrogenPosition::{self as H, Above, Auto, Below, Left, Right};
    for (operation, expected) in [
        (Transform::FlipVertical, [Auto, Left, Right, Below, Above]),
        (Transform::FlipHorizontal, [Auto, Right, Left, Above, Below]),
        (Transform::Rotate(90.), [Auto, Above, Below, Right, Left]),
        (Transform::Rotate(-90.), [Auto, Below, Above, Left, Right]),
        (Transform::Rotate(180.), [Auto, Right, Left, Below, Above]),
        (Transform::Rotate(30.), [Auto, Left, Right, Above, Below]),
        (Transform::Rotate(60.), [Auto, Above, Below, Right, Left]),
    ] {
        let mut doc = Document::default();
        let ids: Vec<_> = H::ALL
            .into_iter()
            .enumerate()
            .map(|(i, position)| {
                let id = doc.add_atom("O", Point::new(i as f32 * 42., 0.));
                doc.atom_mut(id).unwrap().display.hydrogen_position = position;
                id
            })
            .collect();
        let outside = doc.add_atom("N", Point::new(300., 100.));
        doc.atom_mut(outside).unwrap().display.hydrogen_position = Below;
        let outside_before = doc.atom(outside).unwrap().clone();
        transform(&mut doc, &ids, operation);
        assert_eq!(doc.atom(outside).unwrap(), &outside_before);
        for (&id, expected) in ids.iter().zip(expected) {
            assert_eq!(
                doc.atom(id).unwrap().display.hydrogen_position,
                expected,
                "{operation:?}, atom {id}"
            );
        }
    }
}

#[test]
fn fixed_hydrogen_positions_rotate_with_selection_handles() {
    use crate::atom_labels::HydrogenPosition::{Auto, Below, Right};
    let mut doc = Document::default();
    let oxygen = doc.add_atom("O", Point::new(30., 20.));
    doc.atom_mut(oxygen).unwrap().display.hydrogen_position = Right;
    let automatic = doc.add_atom("N", Point::new(10., 40.));
    let outside = doc.add_atom("O", Point::new(100., 100.));
    doc.atom_mut(outside).unwrap().display.hydrogen_position = Right;
    let before = doc.clone();
    transform_about(
        &mut doc,
        &[oxygen, automatic],
        Point::new(10., 20.),
        2.,
        90.,
    );
    assert_eq!(doc.atom(oxygen).unwrap().display.hydrogen_position, Below);
    assert_eq!(doc.atom(automatic).unwrap().display.hydrogen_position, Auto);
    assert!(
        doc.atom(oxygen)
            .unwrap()
            .position
            .distance(Point::new(10., 60.))
            < 0.00001
    );
    assert!(
        doc.atom(automatic)
            .unwrap()
            .position
            .distance(Point::new(-30., 20.))
            < 0.00001
    );
    assert_eq!(doc.atom(outside), before.atom(outside));
}

#[test]
fn fixed_hydrogen_positions_survive_translation_and_positive_scaling() {
    use crate::atom_labels::HydrogenPosition as H;
    let mut doc = Document::default();
    for (i, position) in H::ALL.into_iter().enumerate() {
        let id = doc.add_atom("O", Point::new(i as f32 * 42., 30.));
        doc.atom_mut(id).unwrap().display.hydrogen_position = position;
    }
    let ids = doc.all_ids();
    doc.translate(&ids, -200., 100.);
    transform_about(&mut doc, &ids, Point::default(), 0.5, 0.);
    scale_axes_about(&mut doc, &ids, Point::default(), 2., 3.);
    for (atom, expected) in doc.atoms.iter().zip(H::ALL) {
        assert_eq!(atom.display.hydrogen_position, expected);
    }
}

#[test]
fn fixed_hydrogen_positions_rotate_in_pinned_copies_without_changing_the_source() {
    use crate::atom_labels::HydrogenPosition::{Below, Right};
    let mut doc = Document::default();
    let oxygen = doc.add_atom("O", Point::new(30., 20.));
    let carbon = doc.add_atom("C", Point::new(0., 20.));
    doc.add_bond(oxygen, carbon, 1, "plain");
    doc.atom_mut(oxygen).unwrap().display.hydrogen_position = Right;
    let before = doc.clone();
    let (copied, ids) =
        reference::rotate(&doc, &[oxygen, carbon], Point::new(10., 20.), 90., true).unwrap();
    assert_eq!(doc, before);
    for original in &before.atoms {
        assert_eq!(copied.atom(original.id), Some(original));
    }
    let rotated = copied
        .atoms
        .iter()
        .find(|a| ids.contains(&a.id) && a.element == "O")
        .unwrap();
    assert_eq!(rotated.display.hydrogen_position, Below);
    assert!(rotated.position.distance(Point::new(10., 40.)) < 0.00001);
}

#[test]
fn fixed_hydrogen_positions_keep_reflected_porphine_labels_inward() {
    use crate::atom_labels::HydrogenPosition::{Above, Below};
    let before = Document::from_native_file(include_bytes!(
        "../../../../tests/fixtures/porphine-core/porphine-placed-desktop.rsk"
    ))
    .unwrap();
    let mut doc = before.clone();
    let ids = doc.all_ids();
    let pivot = center(&doc, &ids);
    transform(&mut doc, &ids, Transform::FlipVertical);
    assert_eq!(doc.atom(21).unwrap().display.hydrogen_position, Above);
    assert_eq!(doc.atom(23).unwrap().display.hydrogen_position, Below);
    assert_eq!(doc.bonds, before.bonds);
    for original in &before.atoms {
        let atom = doc.atom(original.id).unwrap();
        assert!((atom.position.x - original.position.x).abs() < 0.00003);
        assert!((atom.position.y - (2. * pivot.y - original.position.y)).abs() < 0.00003);
        let mut restored = atom.clone();
        restored.position = original.position;
        restored.display.hydrogen_position = original.display.hydrogen_position;
        assert_eq!(
            &restored, original,
            "Only coordinates and label direction may change"
        );
    }
    let reopened = Document::from_native_file(&doc.file_json().unwrap()).unwrap();
    assert_eq!(reopened, doc.current());
}

fn hydrogen_ink_extent(doc: &Document, atom: u64, direction: Point) -> f32 {
    let atom = doc.atom(atom).unwrap();
    crate::scene::atom_label_ink_boxes(atom, doc)
        .into_iter()
        .flat_map(|(lo, hi)| [lo, hi, Point::new(lo.x, hi.y), Point::new(hi.x, lo.y)])
        .map(|p| (p.x - atom.position.x) * direction.x + (p.y - atom.position.y) * direction.y)
        .fold(0., f32::max)
}

fn fixed_hydrogen_arrow_fixture(overlap: bool) -> (Document, u64, f32) {
    use crate::{
        arrow_anchors::{Anchor, Target},
        arrows::{ArrowStyle, Preset},
        atom_labels::HydrogenPosition::{Above, Below},
        document::Arrow,
    };
    let mut doc = Document::default();
    let oxygen = doc.add_atom("O", Point::default());
    let atom = doc.atom_mut(oxygen).unwrap();
    atom.no_implicit = true;
    atom.explicit_h = 1;
    atom.label_h = 1;
    atom.display.hydrogens = Some(true);
    atom.display.hydrogen_position = Above;
    let style = ArrowStyle::preset(Preset::Curved);
    let pen = crate::style::DEFAULT.world(style.width_pt) * 0.5;
    let up = Point::new(0., -1.);
    let down = Point::new(0., 1.);
    let mut final_label = doc.clone();
    final_label
        .atom_mut(oxygen)
        .unwrap()
        .display
        .hydrogen_position = Below;
    let distance =
        hydrogen_ink_extent(&doc, oxygen, down).max(hydrogen_ink_extent(&final_label, oxygen, up))
            + pen
            + 1.;
    assert!(distance < hydrogen_ink_extent(&doc, oxygen, up) + pen);
    let start = Point::new(0., if overlap { 0. } else { distance });
    let mut arrow = Arrow::new(
        doc.next_id(),
        start,
        Point::new(100., 80.),
        Preset::Curved,
        style,
    );
    arrow.start_anchor = Some(Anchor {
        target: Target::Atom { atom: oxygen },
        offset: start,
        direction: down,
        gap: 0.,
    });
    arrow.cubic = Some([start.offset(20., 10.), Point::new(80., 70.)]);
    doc.arrows.push(arrow);
    (doc, oxygen, pen)
}

#[test]
fn fixed_hydrogen_positions_precede_attached_arrow_clearance() {
    for mode in 0..3 {
        let (mut doc, oxygen, _) = fixed_hydrogen_arrow_fixture(false);
        let ids = doc.all_ids();
        let before = doc.arrows[0].clone();
        let pivot = if mode == 0 {
            rotation_center(&doc, &ids).unwrap()
        } else {
            Point::default()
        };
        let expected = |p: Point| Point::new(2. * pivot.x - p.x, 2. * pivot.y - p.y);
        match mode {
            0 => transform(&mut doc, &ids, Transform::Rotate(180.)),
            1 => transform_about(&mut doc, &ids, pivot, 1., 180.),
            _ => doc = reference::rotate(&doc, &ids, pivot, 180., false).unwrap().0,
        }
        assert_eq!(
            doc.atom(oxygen).unwrap().display.hydrogen_position,
            crate::atom_labels::HydrogenPosition::Below
        );
        assert!(
            doc.arrows[0].start.distance(expected(before.start)) < 0.0001,
            "Already clear endpoint must not grow against the old H direction: mode {mode}"
        );
        assert!(
            doc.arrows[0].cubic.unwrap()[0].distance(expected(before.cubic.unwrap()[0])) < 0.0001
        );
        let once = doc.clone();
        crate::arrow_anchors::reconcile(&mut doc);
        assert_eq!(doc, once);
    }
}

#[test]
fn fixed_hydrogen_positions_still_clear_overlapping_attached_arrows() {
    let (mut doc, oxygen, pen) = fixed_hydrogen_arrow_fixture(true);
    let ids = doc.all_ids();
    transform_about(&mut doc, &ids, Point::default(), 1., 180.);
    let arrow = &doc.arrows[0];
    let anchor = arrow.start_anchor.as_ref().unwrap();
    let center = doc.atom(oxygen).unwrap().position;
    let projection = (arrow.start.x - center.x) * anchor.direction.x
        + (arrow.start.y - center.y) * anchor.direction.y;
    assert!(projection >= hydrogen_ink_extent(&doc, oxygen, anchor.direction) + pen);
    assert!(projection > 0.);
    let once = doc.clone();
    crate::arrow_anchors::reconcile(&mut doc);
    assert_eq!(doc, once);
}

#[test]
fn vertex_attachment_keeps_substituents_outside_at_any_orientation_and_scale() {
    for degrees in (0..360).step_by(30) {
        for length in [21.0, 42.0, 63.0] {
            let angle = (degrees as f32).to_radians();
            let mut doc = Document::default();
            let anchor = Point::new(100.0, 50.0);
            let substituent = anchor.offset(length * angle.cos(), length * angle.sin());
            let a = doc.add_atom("C", anchor);
            let b = doc.add_atom("C", substituent);
            doc.add_bond(a, b, 1, "plain");
            let ids = ring(&mut doc, anchor, 6, false, 5.0);
            let center = center(&doc, &ids);
            let dot = (center.x - anchor.x) * (substituent.x - anchor.x)
                + (center.y - anchor.y) * (substituent.y - anchor.y);
            assert!(
                dot < 0.0,
                "substituent must point away from the ring center"
            );
            assert_eq!(doc.atom(b).unwrap().position, substituent);
            assert_eq!(doc.atom(a).unwrap().position, anchor);
            assert_eq!((doc.atoms.len(), doc.bonds.len()), (7, 7));
            for bond in &doc.bonds {
                assert!(
                    (doc.atom(bond.a)
                        .unwrap()
                        .position
                        .distance(doc.atom(bond.b).unwrap().position)
                        - length)
                        .abs()
                        < 0.001
                );
            }
            doc.validate().unwrap();
        }
    }
}

#[test]
fn ring_interior_hit_includes_aromatic_hetero_and_substituted_rings() -> Result<(), String> {
    let mut doc = Document::default();
    let ids = ring(&mut doc, Point::default(), 6, true, 5.);
    let nitrogen = *ids.first().ok_or("ring atom")?;
    doc.atom_mut(nitrogen).ok_or("nitrogen")?.element = "N".into();
    let attach = *ids.get(2).ok_or("substituted atom")?;
    let position = doc.atom(attach).ok_or("atom")?.position;
    let methyl = doc.add_atom("C", position.offset(60., 0.));
    doc.add_bond(attach, methyl, 1, "plain");
    let before = doc.clone();
    let mut hit = ring_at(&doc, center(&doc, &ids)).ok_or("ring interior")?;
    let mut expected = ids.clone();
    hit.sort_unstable();
    expected.sort_unstable();
    assert_eq!(hit, expected);
    assert_eq!(ring_at(&doc, Point::new(500., 500.)), None);
    assert!(snap_ring(&mut doc, &ids, Point::new(40., 0.), 15.).is_none());
    assert_eq!(
        doc, before,
        "Selecting an aromatic ring must not enable fusion"
    );
    crate::projection::tilt(&mut doc, &ids, 60., true);
    let mut tilted = ring_at(&doc, center(&doc, &ids)).ok_or("tilted interior")?;
    tilted.sort_unstable();
    assert_eq!(tilted, expected);
    Ok(())
}

#[test]
fn rejected_rings_name_the_blocking_atom_and_keep_their_outline() {
    let mut doc = Document::default();
    let center = doc.add_atom("C", Point::default());
    for (x, y) in [(42., 0.), (-42., 0.), (0., 42.), (0., -42.)] {
        let other = doc.add_atom("C", Point::new(x, y));
        doc.add_bond(center, other, 1, "plain");
    }
    let before = doc.clone();
    let rejection = ring_placement(&doc, Point::default(), 6, false, 5., None).unwrap_err();
    assert_eq!(rejection.label, "C would have 6 bonds");
    assert_eq!(rejection.atom, Some(center));
    assert_eq!(rejection.outline.len(), 6);
    assert!(rejection.outline.contains(&Point::default()));
    assert_eq!(
        ring_oriented(&mut doc, Point::default(), 6, false, 5., None),
        Err(rejection.message)
    );
    assert_eq!(doc, before);
    // The circle form falls back to a phenyl bond, which does not fit either.
    let rejection = ring_placement(&doc, Point::default(), 6, true, 5., None).unwrap_err();
    assert_eq!(rejection.label, "C would have 5 bonds");
    assert_eq!(rejection.atom, Some(center));

    let mut methane = Document::default();
    let carbon = methane.add_atom("C", Point::default());
    if let Some(atom) = methane.atom_mut(carbon) {
        atom.explicit_h = 4;
    }
    let rejection = ring_placement(&methane, Point::default(), 5, false, 5., None).unwrap_err();
    assert_eq!(rejection.label, "C has fixed hydrogens");

    let ring = crate::rings::Preset::Regular.document(42., false);
    let bond = &ring.bonds[0];
    let (a, b) = (ring.atom(bond.a).unwrap(), ring.atom(bond.b).unwrap());
    let middle = Point::new(
        (a.position.x + b.position.x) / 2.,
        (a.position.y + b.position.y) / 2.,
    );
    let rejection =
        ring_placement(&ring, middle, 6, false, 5., Some(Point::default())).unwrap_err();
    // Every vertex is an atom of the ring already there, and so is every edge.
    assert_eq!(rejection.label, "The ring already exists");
    assert_eq!(rejection.outline.len(), 6);
    assert_eq!(rejection.atom, None);
}

/// The owner's drawing: three hexagons fused in a row. A fourth fused onto
/// the upper-left bond of the right ring has a vertex on the middle ring's
/// top atom. Returns the drawing, that bond's midpoint and the top atom.
fn three_rings_in_a_row() -> (Document, Point, u64) {
    let mut doc = Document::default();
    ring(&mut doc, Point::default(), 6, false, 5.);
    for x in [36.373, 109.119] {
        ring_oriented(
            &mut doc,
            Point::new(x, 0.),
            6,
            false,
            5.,
            Some(Point::new(x + 40., 0.)),
        )
        .unwrap();
    }
    assert_eq!((doc.atoms.len(), doc.bonds.len()), (14, 16));
    let top = doc.nearest(Point::new(72.746, -42.), 1.).unwrap();
    (doc, Point::new(127.3, -31.5), top)
}

#[test]
fn fused_ring_shares_the_existing_atom_under_a_new_vertex() {
    let (mut doc, bond, top) = three_rings_in_a_row();
    let (placed, ids) = ring_placement(&doc, bond, 6, false, 5., None).unwrap();
    // Three new atoms; the edge from the top atom to the fused bond exists.
    assert_eq!((placed.atoms.len(), placed.bonds.len()), (17, 20));
    assert_eq!(ids.len(), 6);
    assert!(ids.contains(&top));
    assert!(placed.validate().is_ok());
    let mut expected = ids.clone();
    expected.sort_unstable();
    let mut perceived = ring_at(&placed, center(&placed, &ids)).unwrap();
    perceived.sort_unstable();
    assert_eq!(perceived, expected);
    assert_eq!(
        ring_oriented(&mut doc, bond, 6, false, 5., None),
        Ok(ids.clone())
    );
    assert_eq!(doc, placed);
    // The same ring again is a complete duplicate.
    let again = ring_placement(&doc, bond, 6, false, 5., Some(Point::new(109., -84.)));
    assert_eq!(again.unwrap_err().label, "The ring already exists");
}

/// A C–C bond with a ring of `n` dragged onto one side, and an atom `c` of
/// `element` bonded to the bond's first atom where the ring's last vertex
/// lands. Returns the drawing, `c` and the click and drag points.
fn bond_with_an_atom_under_the_ring(n: u8, element: &str) -> (Document, u64, Point, Point) {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::default());
    let b = doc.add_atom("C", Point::new(42., 0.));
    doc.add_bond(a, b, 1, "plain");
    let (click, drag) = (Point::new(21., 0.), Point::new(21., -50.));
    let (ring, ids) = ring_placement(&doc, click, n, false, 5., Some(drag)).unwrap();
    let last = ring.atom(ids[n as usize - 1]).unwrap().position;
    let c = doc.add_atom(element, last);
    doc.add_bond(a, c, 1, "plain");
    (doc, c, click, drag)
}

#[test]
fn rings_of_every_size_share_an_atom_they_land_on() {
    for n in 3..=8_u8 {
        let (mut doc, c, click, drag) = bond_with_an_atom_under_the_ring(n, "C");
        doc.bonds[1].order = 2;
        let (placed, ids) = ring_placement(&doc, click, n, false, 5., Some(drag)).unwrap();
        assert_eq!(ids.last(), Some(&c), "size {n}");
        // The double bond from `c` to the fused bond is kept as drawn.
        assert_eq!(placed.bonds[..2], doc.bonds[..], "size {n}");
        let size = n as usize;
        assert_eq!(
            (placed.atoms.len(), placed.bonds.len()),
            (size, size),
            "size {n}"
        );
        assert!(placed.validate().is_ok());

        // A ring attached at an atom shares a separate atom the same way.
        let mut doc = Document::default();
        let anchor = doc.add_atom("C", Point::default());
        let drag = Some(Point::new(60., 20.));
        let (ring, ids) = ring_placement(&doc, Point::default(), n, false, 5., drag).unwrap();
        let other = doc.add_atom("C", ring.atom(ids[2]).unwrap().position);
        let (placed, ids) = ring_placement(&doc, Point::default(), n, false, 5., drag).unwrap();
        assert_eq!((ids[0], ids[2]), (anchor, other), "size {n}");
        assert_eq!((placed.atoms.len(), placed.bonds.len()), (size, size));
    }
}

#[test]
fn rings_refuse_a_vertex_they_cannot_share() {
    // Two atoms under one vertex: which to share is ambiguous.
    let (mut doc, c, click, drag) = bond_with_an_atom_under_the_ring(6, "C");
    let near = doc.atom(c).unwrap().position.offset(1., 0.);
    let second = doc.add_atom("C", near);
    doc.add_bond(c, second, 1, "plain");
    let rejection = ring_placement(&doc, click, 6, false, 5., Some(drag)).unwrap_err();
    assert_eq!(rejection.label, "Overlaps an existing atom");
    assert_eq!(rejection.outline.len(), 6);

    // Oxygen with two bonds has no valence left for the ring's new bond.
    let (mut doc, c, click, drag) = bond_with_an_atom_under_the_ring(6, "O");
    let methyl = doc.add_atom("C", Point::new(-60., -60.));
    doc.add_bond(c, methyl, 1, "plain");
    let before = doc.clone();
    let rejection = ring_placement(&doc, click, 6, false, 5., Some(drag)).unwrap_err();
    assert_eq!(rejection.label, "O would have 3 bonds");
    assert_eq!(rejection.atom, Some(c));
    assert_eq!(
        ring_oriented(&mut doc, click, 6, false, 5., Some(drag)),
        Err(rejection.message)
    );
    assert_eq!(doc, before);

    // A contracted abbreviation is never opened by a ring.
    let (mut doc, c, click, drag) = bond_with_an_atom_under_the_ring(6, "C");
    doc.abbreviations.push(crate::abbreviations::Abbreviation {
        label: "Me".into(),
        reverse_label: String::new(),
        anchor: c,
        exit: None,
        members: vec![c],
        alignment: Default::default(),
        label_style: None,
        label_color_override: false,
        highlight: None,
    });
    let rejection = ring_placement(&doc, click, 6, false, 5., Some(drag)).unwrap_err();
    assert_eq!(rejection.label, "Expand the abbreviation first");
    assert_eq!(rejection.atom, Some(c));

    // A delocalized ring would rewrite the bonds between shared atoms.
    let (doc, _, click, drag) = bond_with_an_atom_under_the_ring(5, "C");
    let rejection = ring_placement(&doc, click, 5, true, 5., Some(drag)).unwrap_err();
    assert_eq!(rejection.label, "Overlaps an existing atom");

    // A ring dropped on empty canvas still never joins what it lands on.
    let mut doc = Document::default();
    let (ring, ids) = ring_placement(&doc, Point::default(), 6, false, 5., None).unwrap();
    doc.add_atom("C", ring.atom(ids[0]).unwrap().position);
    let rejection = ring_placement(&doc, Point::default(), 6, false, 5., None).unwrap_err();
    assert_eq!(rejection.label, "Overlaps an existing atom");
}

#[test]
fn free_ring_drag_starts_at_the_press_and_points_its_first_bond_at_the_pointer() {
    let start = Point::new(10.0, 10.0);
    for n in [3_u8, 5, 6, 8] {
        let (doc, ids) = ring_placement(
            &Document::default(),
            start,
            n,
            false,
            5.0,
            Some(Point::new(70.0, 10.0)),
        )
        .unwrap();
        assert_eq!(ids.len(), n as usize);
        let position = |id: u64| doc.atom(id).unwrap().position;
        assert!(position(ids[0]).distance(start) < 0.001);
        assert!(position(ids[1]).distance(Point::new(52.0, 10.0)) < 0.001);
        // Dragging right puts the ring above the first bond on screen.
        for id in &ids[2..] {
            assert!(position(*id).y < 10.0);
        }
        for bond in &doc.bonds {
            assert!((position(bond.a).distance(position(bond.b)) - 42.0).abs() < 0.001);
        }
    }
}

#[test]
fn rings_dragged_from_an_atom_aim_exactly_at_the_pointer() {
    // Free (Option/Alt) drags pass any angle; neither ring path quantizes it.
    for aromatic in [false, true] {
        let mut doc = Document::default();
        let atom = doc.add_atom("C", Point::default());
        let angle = 20_f32.to_radians();
        let aim = Point::new(80. * angle.cos(), 80. * angle.sin());
        let (result, ids) =
            ring_placement(&doc, Point::default(), 6, aromatic, 5., Some(aim)).unwrap();
        assert!(ids.contains(&atom));
        let added: Vec<_> = result.atoms.iter().filter(|a| a.id != atom).collect();
        let x = added.iter().map(|a| a.position.x).sum::<f32>() / added.len() as f32;
        let y = added.iter().map(|a| a.position.y).sum::<f32>() / added.len() as f32;
        assert!((y.atan2(x) - angle).abs() < 0.001, "aromatic: {aromatic}");
    }
}

#[test]
fn ring_drag_chooses_the_requested_side_and_matches_the_target_edge() {
    for side in [-1.0, 1.0] {
        let mut doc = Document::default();
        let a = doc.add_atom("C", Point::default());
        let b = doc.add_atom("C", Point::new(60.0, 0.0));
        doc.add_bond(a, b, 1, "plain");
        let ids = ring_oriented(
            &mut doc,
            Point::new(30.0, 0.0),
            5,
            false,
            5.0,
            Some(Point::new(30.0, side * 50.0)),
        )
        .unwrap();
        assert_eq!(&ids[..2], &[a, b]);
        assert_eq!((doc.atoms.len(), doc.bonds.len()), (5, 5));
        for id in &ids[2..] {
            assert!(doc.atom(*id).unwrap().position.y * side > 0.0);
        }
        for bond in &doc.bonds {
            assert!(
                (doc.atom(bond.a)
                    .unwrap()
                    .position
                    .distance(doc.atom(bond.b).unwrap().position)
                    - 60.0)
                    .abs()
                    < 0.001
            );
        }
    }
}

#[test]
fn dragging_an_existing_ring_rotates_scales_and_reuses_the_target_atoms() {
    let mut doc = Document::default();
    let fixed = ring(&mut doc, Point::default(), 6, false, 5.0);
    let fixed_atoms = doc.atoms.clone();
    let moving = ring(&mut doc, Point::new(250.0, 200.0), 5, false, 5.0);
    transform(&mut doc, &moving, Transform::Rotate(37.0));
    for id in &moving {
        let p = &mut doc.atom_mut(*id).unwrap().position;
        *p = Point::new(250.0 + (p.x - 250.0) * 0.6, 200.0 + (p.y - 200.0) * 0.6);
    }
    assert_eq!(ring_at(&doc, center(&doc, &moving)).unwrap().len(), 5);
    let midpoint = |ids: &[u64]| {
        let a = doc.atom(ids[0]).unwrap().position;
        let b = doc.atom(ids[1]).unwrap().position;
        Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)
    };
    let a = midpoint(&fixed);
    let b = midpoint(&moving);
    let snapped = snap_ring(&mut doc, &moving, Point::new(a.x - b.x, a.y - b.y), 5.0).unwrap();
    assert_eq!((doc.atoms.len(), doc.bonds.len()), (9, 10));
    assert_eq!(snapped.len(), 5);
    assert!(snapped.contains(&fixed[0]) && snapped.contains(&fixed[1]));
    for original in fixed_atoms {
        assert_eq!(doc.atom(original.id).unwrap().position, original.position);
    }
    for bond in &doc.bonds {
        assert!(
            (doc.atom(bond.a)
                .unwrap()
                .position
                .distance(doc.atom(bond.b).unwrap().position)
                - 42.0)
                .abs()
                < 0.001
        );
    }
    for (i, atom) in doc.atoms.iter().enumerate() {
        for other in &doc.atoms[i + 1..] {
            assert!(atom.position.distance(other.position) > 10.0);
        }
    }
    doc.validate().unwrap();
    let before = doc.clone();
    assert!(
        snap_ring(&mut doc, &snapped, Point::default(), 5.0).is_none(),
        "an attached ring must not silently discard external bonds"
    );
    assert_eq!(doc, before);
}

/// Fusing deletes the two shared ring atoms, so it must not take a radical,
/// a mark or a stereo group with them.
#[test]
fn ring_snap_refuses_ring_atoms_with_state_it_would_delete() {
    use crate::{
        scientific::{AtomMark, MarkKind},
        stereo_groups::{Kind, StereoGroup},
    };
    let mut base = Document::default();
    let fixed = ring(&mut base, Point::default(), 6, false, 5.0);
    let moving = ring(&mut base, Point::new(250.0, 200.0), 5, false, 5.0);
    let midpoint = |ids: &[u64]| {
        let a = base.atom(ids[0]).unwrap().position;
        let b = base.atom(ids[1]).unwrap().position;
        Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)
    };
    let (a, b) = (midpoint(&fixed), midpoint(&moving));
    let delta = Point::new(a.x - b.x, a.y - b.y);
    assert!(snap_ring(&mut base.clone(), &moving, delta, 5.0).is_some());
    let changes: [fn(&mut crate::document::Atom); 3] = [
        |atom| atom.radical_electrons = 1,
        |atom| {
            atom.marks.push(AtomMark {
                id: Some(1),
                kind: MarkKind::LonePair,
                offset: Point::new(0., -20.),
                angle: 0.,
                size_pt: None,
            })
        },
        |atom| {
            atom.stereo_group = Some(StereoGroup {
                kind: Kind::And,
                number: 1,
            })
        },
    ];
    // The atoms it fuses onto are held to the same rule as any shared atom.
    for id in [moving[0], fixed[0]] {
        for change in changes {
            let mut doc = base.clone();
            change(doc.atom_mut(id).unwrap());
            let before = doc.clone();
            assert!(snap_ring(&mut doc, &moving, delta, 5.0).is_none());
            assert_eq!(doc, before);
        }
    }
}

#[test]
fn growth_uses_open_space_at_either_end_and_at_a_branch() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::default());
    let b = doc.add_atom("C", Point::new(36.373066, -21.0));
    let c = doc.add_atom("C", Point::new(72.74613, 0.0));
    doc.add_bond(a, b, 1, "plain");
    doc.add_bond(b, c, 1, "plain");
    let left = bond_extension(&doc, doc.atom(a).unwrap().position, Some(a), 1);
    let right = bond_extension(&doc, doc.atom(c).unwrap().position, Some(c), 1);
    let branch = bond_extension(&doc, doc.atom(b).unwrap().position, Some(b), 1);
    assert!(left.x < -36.0 && (left.y + 21.0).abs() < 0.01);
    assert!(right.x > 109.0 && (right.y + 21.0).abs() < 0.01);
    assert!((branch.x - 36.373066).abs() < 0.01 && (branch.y + 63.0).abs() < 0.01);

    // An occupied preferred endpoint should choose the other 120° turn.
    doc.add_atom("O", right);
    let alternate = bond_extension(&doc, doc.atom(c).unwrap().position, Some(c), 1);
    assert!(alternate.distance(right) > 60.0);
    assert!((alternate.x - 72.74613).abs() < 0.01 && (alternate.y - 42.0).abs() < 0.01);
}

/// The new bond at `id` must leave every ring outline, and its midpoint too.
fn assert_exocyclic(doc: &Document, id: u64, rings: &[Vec<u64>]) {
    let start = doc.atom(id).unwrap().position;
    let end = bond_extension(doc, start, Some(id), 1);
    let middle = Point::new((start.x + end.x) / 2.0, (start.y + end.y) / 2.0);
    for ring in rings {
        let outline: Vec<_> = ring
            .iter()
            .map(|i| doc.atom(*i).unwrap().position)
            .collect();
        assert!(
            !encloses(&outline, middle) && !encloses(&outline, end),
            "bond from {id} at {start:?} runs to {end:?}, into ring {ring:?}"
        );
    }
}

#[test]
fn bonds_from_joined_ring_atoms_stay_outside_both_rings() {
    for size in [5, 6] {
        for joined in 0..6 {
            let mut doc = Document::default();
            let hexagon = ring(&mut doc, Point::default(), 6, false, 5.0);
            let id = hexagon[joined];
            let start = doc.atom(id).unwrap().position;
            let end = bond_extension(&doc, start, Some(id), 1);
            let other = doc.add_atom("C", end);
            doc.add_bond(id, other, 1, "plain");
            let second = ring_oriented(&mut doc, end, size, false, 5.0, None).unwrap();
            assert_eq!(second.len(), size as usize);
            let rings = [hexagon.clone(), second];
            assert_exocyclic(&doc, id, &rings);
            assert_exocyclic(&doc, other, &rings);
        }
    }
}

#[test]
fn bonds_from_fusion_atoms_leave_the_fused_system() {
    for size in [5, 6, 7, 8] {
        for edge in 0..6 {
            let mut doc = Document::default();
            let hexagon = ring(&mut doc, Point::default(), 6, false, 5.0);
            let (a, b) = (hexagon[edge], hexagon[(edge + 1) % 6]);
            let (pa, pb) = (doc.atom(a).unwrap().position, doc.atom(b).unwrap().position);
            let fused = ring(
                &mut doc,
                Point::new((pa.x + pb.x) / 2.0, (pa.y + pb.y) / 2.0),
                size,
                false,
                5.0,
            );
            assert_eq!(fused.len(), size as usize);
            let rings = [hexagon.clone(), fused];
            assert_exocyclic(&doc, a, &rings);
            assert_exocyclic(&doc, b, &rings);
        }
    }
}

#[test]
fn a_narrow_ring_counts_and_a_hydrogen_bond_closes_no_ring() {
    // A thin triangle whose far edge lies closer than any fixed probe.
    let mut doc = Document::default();
    let s = doc.add_atom("C", Point::default());
    for p in [
        Point::new(3.0, -11.2),
        Point::new(3.0, 11.2),
        Point::new(-42.0, 0.0),
    ] {
        let id = doc.add_atom("C", p);
        doc.add_bond(s, id, 1, "plain");
    }
    doc.add_bond(2, 3, 1, "plain");
    assert!(bond_extension(&doc, Point::default(), Some(s), 1).x < 0.0);

    // O–H···O closes no ring, so the widest gap, east, stays free.
    let mut doc = Document::default();
    let s = doc.add_atom("C", Point::default());
    let a = doc.add_atom("O", Point::new(14.0, -40.0));
    let b = doc.add_atom("O", Point::new(14.0, 40.0));
    let w = doc.add_atom("C", Point::new(-42.0, 0.0));
    let h = doc.add_atom("H", Point::new(80.0, 0.0));
    for id in [a, b, w] {
        doc.add_bond(s, id, 1, "plain");
    }
    doc.add_bond(a, h, 1, "plain");
    doc.add_bond(h, b, 0, "dotted");
    assert!(bond_extension(&doc, Point::default(), Some(s), 1).x > 30.0);
}

#[test]
fn a_hydrogen_or_dative_link_at_the_atom_closes_no_ring() {
    for (order, display) in [(0, "dotted"), (crate::bonds::DATIVE, "plain")] {
        let mut doc = Document::default();
        let s = doc.add_atom("C", Point::default());
        let a = doc.add_atom("O", Point::new(14.0, -40.0));
        let w = doc.add_atom("C", Point::new(-42.0, 0.0));
        let b = doc.add_atom("O", Point::new(14.0, 40.0));
        let bridge = doc.add_atom("C", Point::new(84.0, 0.0));
        doc.add_bond(s, a, 1, "plain");
        doc.add_bond(s, w, 1, "plain");
        doc.add_bond(s, b, order, display);
        doc.add_bond(b, bridge, 1, "plain");
        doc.add_bond(bridge, a, 1, "plain");

        // The widest gap points east, inside the outline s–b–bridge–a.
        // The link from s to b cannot close that ring.
        let end = bond_extension(&doc, Point::default(), Some(s), 1);
        assert!(end.x > 30.0, "bond order {order} gave endpoint {end:?}");
    }
}

#[test]
fn triple_and_cumulative_double_bonds_extend_linearly() {
    for (previous, next) in [(3, 1), (1, 3), (2, 2)] {
        let mut doc = Document::default();
        let a = doc.add_atom("C", Point::default());
        let b = doc.add_atom("C", Point::new(42.0, 0.0));
        doc.add_bond(a, b, previous, "plain");
        let end = bond_extension(&doc, doc.atom(b).unwrap().position, Some(b), next);
        assert!(end.distance(Point::new(84.0, 0.0)) < 0.001);
    }
}
#[test]
fn fused_ring_reuses_shared_atoms_and_chooses_free_side() {
    let mut d = Document::default();
    let ids = ring(&mut d, Point::default(), 6, false, 5.0);
    let a = d.atom(ids[0]).unwrap().position;
    let b = d.atom(ids[1]).unwrap().position;
    ring(
        &mut d,
        Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0),
        6,
        false,
        5.0,
    );
    assert_eq!((d.atoms.len(), d.bonds.len()), (10, 11));
    assert!(d.validate().is_ok());
    for (i, a) in d.atoms.iter().enumerate() {
        for b in &d.atoms[i + 1..] {
            assert!(a.position.distance(b.position) > 10.0);
        }
    }
}
#[test]
fn clipboard_remaps_stereo_ids_and_keeps_annotations() {
    let mut d: Document = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/ui-drawn-ethanol.reshiki"
    ))
    .unwrap();
    let original = d.clone();
    let ids = append(&mut d, &original, Point::new(100.0, 100.0));
    assert_eq!(ids.len(), original.all_ids().len());
    assert!(d.validate().is_ok());
    assert_eq!(selection(&d, &ids).atoms.len(), 3);
    assert_eq!(d.arrows.len(), 2);
}
#[test]
fn alignment_does_not_collapse_bonded_atoms() {
    let mut d = Document::default();
    let a = d.add_atom("C", Point::default());
    let b = d.add_atom("O", Point::new(42.0, 0.0));
    d.add_bond(a, b, 1, "plain");
    d.add_atom("N", Point::new(100.0, 100.0));
    let ids = d.all_ids();
    arrange(&mut d, &ids, Arrange::AlignVertical);
    assert_eq!(
        d.atom(a)
            .unwrap()
            .position
            .distance(d.atom(b).unwrap().position),
        42.0
    );
    assert_eq!(groups(&d, &ids).len(), 2);
}

#[test]
fn analysis_covers_whole_abbreviations_in_document_order_without_labels() {
    let mut doc = Document::default();
    let member = doc.add_atom("C", Point::default());
    let anchor = doc.add_atom("O", Point::new(42., 0.));
    let outside = doc.add_atom("C", Point::new(84., 0.));
    doc.add_bond(member, anchor, 1, "plain");
    doc.add_bond(anchor, outside, 1, "plain");
    doc.contract(&[anchor, member], "OMe", "MeO").unwrap();
    for atom in &mut doc.atoms {
        atom.cip_label = Some("R".into());
    }
    for bond in &mut doc.bonds {
        bond.cip_label = Some("E".into());
    }
    let before = doc.clone();
    assert_eq!(analysis_atoms(&doc, &[member]), [member, anchor]);
    assert_eq!(
        analysis_atoms(&doc, &[outside, anchor]),
        [member, anchor, outside]
    );
    assert!(analysis_atoms(&doc, &[u64::MAX]).is_empty());
    let part = analysis_document(&doc, &[member, anchor]);
    let mut expected = selection(&doc, &[member, anchor]);
    crate::atom_labels::clear_computed(&mut expected);
    assert_eq!(part, expected);
    assert_eq!(
        part.atoms.iter().map(|a| a.id).collect::<Vec<_>>(),
        [member, anchor]
    );
    assert!(part.atoms.iter().all(|a| a.cip_label.is_none()));
    assert!(part.bonds.iter().all(|b| b.cip_label.is_none()));
    assert_eq!(doc, before);
}

#[test]
fn selection_and_paste_preserve_manual_label_scripts() {
    let mut source = Document::default();
    let id = source.add_atom("*", Point::default());
    source = crate::atom_text::apply_with_scripts(
        &source,
        id,
        "R1",
        &[crate::typography::ScriptRun {
            start: 1,
            end: 2,
            script: crate::typography::Script::Subscript,
        }],
        crate::atom_text::Mode::Text,
    )
    .unwrap();
    let part = super::selection(&source, &[id]);
    let mut destination = source.clone();
    let ids = super::append(&mut destination, &part, Point::new(84., 0.));
    let pasted = destination.atom(ids[0]).unwrap();
    assert_eq!(pasted.display, source.atom(id).unwrap().display);
    destination.validate().unwrap();
}

#[test]
fn analysis_caps_fixed_hydrogen_atoms_only_at_cut_bonds() {
    // ChemDraw labels carry NumHydrogens, imported as fixed hydrogens (#334):
    // formamide, H2N-CHO, with every count fixed.
    let mut doc = Document::default();
    let n = doc.add_atom("N", Point::default());
    let c = doc.add_atom("C", Point::new(42., 0.));
    let o = doc.add_atom("O", Point::new(84., 0.));
    doc.add_bond(n, c, 1, "plain");
    doc.add_bond(c, o, 2, "plain");
    for atom in &mut doc.atoms {
        atom.no_implicit = true;
        atom.explicit_h = [(n, 2), (c, 1)]
            .into_iter()
            .find_map(|(id, h)| (id == atom.id).then_some(h))
            .unwrap_or(0);
    }
    // (hydrogens, radical electrons) per atom.
    let analyse = |part: &Document| {
        let molecule = crate::chemistry::document::prepare(part).unwrap();
        let state = &molecule.state;
        state
            .graph
            .atoms
            .iter()
            .zip(&state.valences)
            .map(|(a, v)| {
                (
                    u32::from(a.explicit_hydrogens) + v.implicit_hydrogens,
                    a.radical_electrons,
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(analyse(&analysis_document(&doc, &[n])), [(3, 0)]);
    let part = analysis_document(&doc, &[c, o]);
    assert_eq!(analyse(&part), [(2, 0), (0, 0)]);
    assert_eq!(part.atom(o).unwrap(), doc.atom(o).unwrap());
    // A cut dative bond removes no donor valence, so its count is unchanged.
    let metal = doc.add_atom("Co", Point::new(0., 42.));
    doc.add_bond(n, metal, 5, "plain");
    doc.atom_mut(metal).unwrap().no_implicit = true;
    let part = analysis_document(&doc, &[n, c, o]);
    assert_eq!(part.atom(n).unwrap(), doc.atom(n).unwrap());
    assert_eq!(analysis_document(&doc, &[metal]).atoms[0].explicit_h, 1);
    assert_eq!(analyse(&analysis_document(&doc, &[n, c, o]))[0], (2, 0));

    // A fixed count that leaves a radical keeps it: a typed CH2 end of
    // ethane is an ethyl radical, so alone it is a methyl radical.
    let mut ethyl = Document::default();
    let a = ethyl.add_atom("C", Point::default());
    let b = ethyl.add_atom("C", Point::new(42., 0.));
    ethyl.add_bond(a, b, 1, "plain");
    let typed = ethyl.atom_mut(b).unwrap();
    (typed.no_implicit, typed.explicit_h) = (true, 2);
    assert_eq!(analyse(&ethyl), [(3, 0), (2, 1)]);
    assert_eq!(analyse(&analysis_document(&ethyl, &[b])), [(3, 1)]);

    // A fixed NH3 singly bonded to a metal is a dative ligand after
    // preparation, so it gains no hydrogen when cut.
    let mut ammine = Document::default();
    let n = ammine.add_atom("N", Point::default());
    let metal = ammine.add_atom("Co", Point::new(42., 0.));
    ammine.add_bond(n, metal, 1, "plain");
    let ligand = ammine.atom_mut(n).unwrap();
    (ligand.no_implicit, ligand.explicit_h) = (true, 3);
    assert_eq!(analyse(&analysis_document(&ammine, &[n])), [(3, 0)]);
    // A fixed CH2 on a metal keeps a covalent bond and its radical.
    let methylene = ammine.add_atom("C", Point::new(84., 0.));
    ammine.add_bond(metal, methylene, 1, "plain");
    let carbon = ammine.atom_mut(methylene).unwrap();
    (carbon.no_implicit, carbon.explicit_h) = (true, 2);
    assert_eq!(analyse(&analysis_document(&ammine, &[methylene])), [(3, 1)]);
    // Only one bond of a bridging chloride becomes dative.
    let chloride = ammine.add_atom("Cl", Point::new(42., 42.));
    let iron = ammine.add_atom("Fe", Point::new(84., 42.));
    ammine.add_bond(metal, chloride, 1, "plain");
    ammine.add_bond(chloride, iron, 1, "plain");
    ammine.atom_mut(chloride).unwrap().no_implicit = true;
    assert_eq!(analyse(&analysis_document(&ammine, &[chloride])), [(1, 0)]);
    // A neutral pentavalent nitro N on a metal is reinterpreted by
    // preparation, so capping it would exceed its valence: it recalculates.
    let nitro = ammine.add_atom("N", Point::new(0., 84.));
    let oxygens = [(-42., 126.), (42., 126.)].map(|(x, y)| ammine.add_atom("O", Point::new(x, y)));
    ammine.add_bond(metal, nitro, 1, "plain");
    for oxygen in oxygens {
        ammine.add_bond(nitro, oxygen, 2, "plain");
    }
    for id in [nitro, oxygens[0], oxygens[1]] {
        ammine.atom_mut(id).unwrap().no_implicit = true;
    }
    let part = analysis_document(&ammine, &[nitro, oxygens[0]]);
    assert_eq!(analyse(&part), [(1, 0), (0, 0)]);
    // A fixed methylamine ligand cut from its carbon stays a dative H3N->Co.
    let amine = ammine.add_atom("N", Point::new(-42., 0.));
    let methyl = ammine.add_atom("C", Point::new(-84., 0.));
    ammine.add_bond(amine, metal, 1, "plain");
    ammine.add_bond(amine, methyl, 1, "plain");
    let ligand = ammine.atom_mut(amine).unwrap();
    (ligand.no_implicit, ligand.explicit_h) = (true, 2);
    let part = analysis_document(&ammine, &[amine, metal]);
    assert_eq!(part.atom(amine).unwrap().explicit_h, 3);
    crate::chemistry::document::prepare(&part).unwrap();

    // Aromatic bonds have no whole valence: a typed NH in aromatic pyrrole
    // recalculates its hydrogens when cut.
    let mut pyrrole = Document::default();
    let ring: Vec<_> = (0..5)
        .map(|i| {
            let angle = std::f32::consts::TAU * i as f32 / 5.;
            pyrrole.add_atom(
                if i == 0 { "N" } else { "C" },
                Point::new(36. * angle.cos(), 36. * angle.sin()),
            )
        })
        .collect();
    for (i, &a) in ring.iter().enumerate() {
        pyrrole.add_bond(a, ring[(i + 1) % 5], 4, "plain");
    }
    let nh = pyrrole.atom_mut(ring[0]).unwrap();
    (nh.no_implicit, nh.explicit_h) = (true, 1);
    assert_eq!(analyse(&analysis_document(&pyrrole, &[ring[0]])), [(3, 0)]);
}

#[test]
fn prepared_ring_interiors_match_live_hits_including_ties_and_hidden_drawings() {
    fn polygon(doc: &mut Document, radius: f32, count: usize, aromatic: bool) -> Vec<u64> {
        let ids: Vec<_> = (0..count)
            .map(|i| {
                let angle = std::f32::consts::TAU * i as f32 / count as f32;
                doc.add_atom("C", Point::new(radius * angle.cos(), radius * angle.sin()))
            })
            .collect();
        for (&a, &b) in ids.iter().zip(ids.iter().cycle().skip(1)).take(ids.len()) {
            doc.add_bond(a, b, if aromatic { 4 } else { 1 }, "plain");
        }
        ids
    }
    let mut doc = Document::default();
    let inner = polygon(&mut doc, 25., 6, true);
    polygon(&mut doc, 25., 6, false); // Equal-radius tie keeps enumeration order.
    polygon(&mut doc, 60., 5, false);
    let compare = |doc: &Document| {
        let prepared = RingInteriors::new(doc);
        for x in -8..=8 {
            for y in -8..=8 {
                let p = Point::new(x as f32 * 9., y as f32 * 9.);
                assert_eq!(prepared.at(p), ring_at(doc, p), "{p:?}");
            }
        }
    };
    compare(&doc);
    crate::projection::tilt(&mut doc, &inner, 52., true);
    compare(&doc);
    doc.abbreviations.push(crate::abbreviations::Abbreviation {
        label: "Ph".into(),
        reverse_label: String::new(),
        anchor: inner[0],
        exit: None,
        members: inner.clone(),
        alignment: Default::default(),
        label_style: None,
        label_color_override: false,
        highlight: None,
    });
    compare(&doc);
    doc.abbreviations.clear();
    let placed = inner
        .iter()
        .map(|id| {
            let p = doc.atom(*id).unwrap().position;
            crate::models3d::Placed {
                id: *id,
                xyz: [p.x, p.y, 0.],
            }
        })
        .collect();
    let mut model =
        crate::models3d::Model3d::new(&doc, placed, vec![], crate::models3d::ForceField::Mmff94s);
    model.display = crate::models3d::Display::ThreeD;
    doc.models.push(model);
    compare(&doc);
    doc.bonds.retain(|b| !b.connects(inner[0], inner[1]));
    compare(&doc);
}

/// Ring tool clicks on empty canvas: drawn at the document's bond length, and
/// like ChemDraw, even rings keep vertical sides while odd rings point right.
#[test]
fn free_rings_follow_the_style_length_and_keep_a_vertical_side() {
    for aromatic in [false, true] {
        for size in 3..=8 {
            if aromatic && size != 6 {
                continue;
            }
            let mut doc = Document::default();
            doc.drawing_style.set_bond_length(10.8);
            let length = doc.drawing_style.bond_length_world;
            assert!((length - crate::style::DEFAULT.bond_length_world).abs() > 1.);
            let ids = ring(&mut doc, Point::new(100., 50.), size, aromatic, 0.);
            assert_eq!(ids.len(), usize::from(size));
            let points: Vec<_> = ids
                .iter()
                .map(|id| doc.atom(*id).unwrap().position)
                .collect();
            let sides: Vec<_> = points
                .iter()
                .zip(points.iter().cycle().skip(1))
                .map(|(a, b)| (*a, *b))
                .collect();
            for (a, b) in &sides {
                assert!(
                    (a.distance(*b) - length).abs() < 0.01,
                    "{size}: {a:?} {b:?}"
                );
            }
            let vertical = sides
                .iter()
                .filter(|(a, b)| (a.x - b.x).abs() < 0.01)
                .count();
            let top = points.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
            let at_top = points.iter().filter(|p| (p.y - top).abs() < 0.01).count();
            match size {
                // Upright: two vertical sides and one vertex at the top.
                6 => assert_eq!((vertical, at_top), (2, 1), "{aromatic}"),
                4 | 8 => assert_eq!(vertical, 2, "{size}"),
                _ => {
                    assert_eq!(vertical, 1, "{size}");
                    let right = points.iter().map(|p| p.x).fold(f32::MIN, f32::max);
                    let tip = points.iter().filter(|p| (p.x - right).abs() < 0.01).count();
                    assert_eq!(tip, 1, "odd rings point right");
                }
            }
        }
    }
}
