use super::*;

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
    assert_eq!(rejection.label, "Overlaps an existing atom");
    assert_eq!(rejection.outline.len(), 6);
    assert!(rejection.atom.is_some());
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
