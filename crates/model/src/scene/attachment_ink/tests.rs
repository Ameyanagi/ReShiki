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

fn dative_projection_end_fixture(display: &str, alpha: f32) -> Document {
    let mut doc = Document::default();
    // Every carbon has degree two: reversing the dative graph changes no labels.
    let ids: Vec<_> = [
        (-60., 0., -10.),
        (60., 0., 10.),
        (70., 70., 40.),
        (-70., 70., -20.),
    ]
    .into_iter()
    .map(|(x, y, z)| {
        let id = doc.add_atom("C", Point::new(x, y));
        doc.atom_mut(id).unwrap().depth = z;
        id
    })
    .collect();
    doc.add_bond(ids[0], ids[1], crate::bonds::DATIVE, display);
    doc.bonds[0].projection = true;
    for (a, b) in [(1, 2), (2, 3), (3, 0)] {
        doc.add_bond(ids[a], ids[b], 1, "plain");
    }
    crate::depth_appearance::set_rear_opacity(&mut doc, &ids, alpha).unwrap();
    doc
}

#[test]
fn dative_projection_end_attachment_ink_matches_rendered_orientation() {
    let mut failures = Vec::new();
    for display in ["wedge_end", "hash_end", "hollow_wedge_end"] {
        let opaque = dative_projection_end_fixture(display, 1.);
        let paint = crate::rear_opacity::Paint::new(&opaque);
        assert!(!paint.has_visibility(), "{display}: opaque visibility");
        let ink = AttachmentInk::new(&opaque);
        for t in [0., 0.25, 0.5, 0.75, 1.] {
            assert!(ink.bond(0, t), "{display}: opaque fraction {t}");
        }
        assert!(ink.bonds.get().is_none(), "{display}: opaque bond cache");

        let doc = dative_projection_end_fixture(display, 0.25);
        let mut twin = doc.clone();
        twin.bonds[0].display = display.trim_end_matches("_end").into();
        twin.bonds[0].reverse();
        let paint = crate::rear_opacity::Paint::new(&doc);
        let twin_paint = crate::rear_opacity::Paint::new(&twin);
        assert!(
            paint.has_visibility() && twin_paint.has_visibility(),
            "{display}: faded visibility"
        );
        assert_eq!(
            format!("{:?}", crate::scene::primitives(&doc)),
            format!("{:?}", crate::scene::primitives(&twin)),
            "{display}: renderer must agree with its normalized twin"
        );
        if format!("{:?}", rendered_bond_parts(&doc, &paint)[0])
            != format!("{:?}", rendered_bond_parts(&twin, &twin_paint)[0])
        {
            failures.push(format!("{display}: attachment bond geometry differs"));
        }
        let ink = AttachmentInk::new(&doc);
        let twin_ink = AttachmentInk::new(&twin);
        for step in 0..=64 {
            let t = step as f32 / 64.;
            // Reversed endpoint fractions represent the same absolute position.
            if ink.bond(0, t) != twin_ink.bond(0, 1. - t) {
                failures.push(format!("{display}: attachment fraction {t} differs"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
