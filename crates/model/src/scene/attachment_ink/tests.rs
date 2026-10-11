use super::*;
#[test]
fn fully_opaque_and_remote_pointer_queries_leave_visibility_caches_cold() {
    let mut doc = Document::from_native_file(include_bytes!(
        "../../../../../tests/fixtures/rear-opacity/c60-rear-opacity-25.rsk"
    ))
    .unwrap();
    let ids = doc.all_ids();
    crate::depth_appearance::set_rear_opacity(&mut doc, &ids, 1.).unwrap();
    let ink = AttachmentInk::new(&doc);
    assert!(ink.atom(52));
    assert!(ink.bond(0, 0.5));
    assert!(ink.opacity.get().is_none());
    assert!(ink.bonds.get().is_none());
    assert!(ink.incidents.get().is_none());
    crate::depth_appearance::set_rear_opacity(&mut doc, &ids, 0.).unwrap();
    let ink = AttachmentInk::new(&doc);
    let remote = Point::new(50_000., 50_000.);
    assert_eq!(ink.label_hit(remote, 2.), None);
    assert_eq!(ink.nearest_atom(remote, 2.), None);
    assert!(!ink.bond(0, -1.));
    assert!(ink.opacity.get().is_none());
    assert!(ink.bonds.get().is_none());
    assert!(
        !ink.bond(
            doc.bonds
                .iter()
                .position(|b| b.a == 52 && b.b == 57)
                .unwrap(),
            0.5
        )
    );
    let cache = ink.bonds.get().unwrap().as_ptr();
    assert!(
        ink.bond(
            doc.bonds
                .iter()
                .position(|b| b.a == 38 && b.b == 48)
                .unwrap(),
            0.5
        )
    );
    assert_eq!(
        ink.bonds.get().unwrap().as_ptr(),
        cache,
        "All nearby candidates share one actual bond-scene construction"
    );
}
#[test]
fn clipped_ink_fraction_does_not_bridge_hidden_middle_or_accept_zero_alpha() {
    let a = Point::new(-80., 0.);
    let axis = Point::new(1., 0.);
    let pieces = [
        Primitive::Line(a, Point::new(-5., 0.), 2.),
        Primitive::Line(Point::new(5., 0.), Point::new(80., 0.), 2.),
    ];
    assert!(!pieces.iter().any(|p| intersects_fraction(p, a, axis, 80.)));
    assert!(pieces.iter().any(|p| intersects_fraction(p, a, axis, 40.)));
    for alpha in [0., 0.25] {
        let part = Primitive::Opacity {
            alpha,
            primitive: Box::new(Primitive::Line(Point::new(-5., 0.), Point::new(5., 0.), 2.)),
        };
        assert_eq!(intersects_fraction(&part, a, axis, 80.), alpha > 0.);
    }
    // A genuine surviving secondary rail is eligible even away from the
    // chemical backbone; absent neighboring bonds/arrows do not count.
    assert!(intersects_fraction(
        &Primitive::Line(Point::new(-5., 6.), Point::new(5., 6.), 1.),
        a,
        axis,
        80.
    ));
}
#[test]
fn actual_crossing_clip_hides_middle_and_keeps_exposed_bond_parts() {
    let mut doc = Document::default();
    let ids: Vec<_> = [
        (-80., 0., -20.),
        (80., 0., -20.),
        (0., -50., 20.),
        (0., 50., 20.),
    ]
    .into_iter()
    .map(|(x, y, z)| {
        let id = doc.add_atom("C", Point::new(x, y));
        doc.atom_mut(id).unwrap().depth = z;
        id
    })
    .collect();
    doc.add_bond(ids[0], ids[1], 1, "plain");
    doc.add_bond(ids[2], ids[3], 1, "plain");
    doc.add_bond(ids[0], ids[2], 1, "plain");
    crate::depth_appearance::set_rear_opacity(&mut doc, &ids, 0.).unwrap();
    let ink = AttachmentInk::new(&doc);
    assert!(!ink.bond(0, 0.5));
    assert!(ink.bond(0, 0.25));
    assert!(ink.bond(0, 0.75));
    assert!(ink.bond(1, 0.5));
    crate::depth_appearance::set_rear_opacity(&mut doc, &ids, 0.25).unwrap();
    assert!(AttachmentInk::new(&doc).bond(0, 0.5));
}
