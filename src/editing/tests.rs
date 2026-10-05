use super::*;

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
        "../../tests/fixtures/ui-drawn-ethanol.reshiki"
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
