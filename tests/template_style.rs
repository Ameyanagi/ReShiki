use reshiki::{
    document::{Annotation, Document, Point},
    document_styles::Preset,
    graphics::{Graphic, GraphicKind},
    rings,
    templates::{Anchor, Connection, LIBRARY, Template, place_with_mode},
};

fn builtin(name: &str) -> &'static Template {
    LIBRARY.iter().find(|t| t.name == name).unwrap()
}

fn nature() -> Document {
    Document {
        drawing_style: Preset::Nature.style(),
        ..Default::default()
    }
}

#[test]
fn free_builtins_follow_journal_size_and_retain_authored_projections() {
    for preset in Preset::ALL {
        let host = Document {
            drawing_style: preset.style(),
            ..Default::default()
        };
        for name in [
            "Cyclohexane",
            "Benzene",
            "Cyclohexane · Chair A",
            "α-D-glucopyranose · Haworth",
        ] {
            let template = builtin(name);
            let original = template.document.clone();
            let source = &original.atoms[0];
            let point = Point::new(160., 120.);
            let scale = host.drawing_style.bond_length_pt / original.drawing_style.bond_length_pt;
            for direction in [None, Some(point.offset(35., 60.))] {
                let (placed, ids) = template
                    .place(
                        &host,
                        point,
                        direction,
                        5.,
                        Anchor::Atom(source.id),
                        Connection::Connect,
                    )
                    .unwrap();
                placed.validate().unwrap();
                assert!(placed.atom(ids[0]).unwrap().position.distance(point) < 0.001);
                assert_eq!(placed.drawing_style, host.drawing_style);
                assert_eq!(placed.all_ids(), original.all_ids());
                assert_eq!(placed.bonds, original.bonds);
                assert_eq!(placed.abbreviations, original.abbreviations);
                for (before, after) in original.atoms.iter().zip(&placed.atoms) {
                    assert_eq!(before.stereo, after.stereo);
                    assert_eq!(before.centroid, after.centroid);
                    assert_eq!(before.element, after.element);
                    assert!((after.depth - before.depth * scale).abs() < 0.001);
                }
                for bond in &original.bonds {
                    let distance = |doc: &Document| {
                        doc.atom(bond.a)
                            .unwrap()
                            .position
                            .distance(doc.atom(bond.b).unwrap().position)
                    };
                    assert!(
                        (distance(&placed) - distance(&original) * scale).abs() < 0.001,
                        "{preset}/{name}/{direction:?}"
                    );
                }
                if preset == Preset::Jacs {
                    let baseline = place_with_mode(
                        &host,
                        &original,
                        point,
                        direction,
                        5.,
                        Anchor::Atom(source.id),
                        Connection::Connect,
                    )
                    .unwrap();
                    assert_eq!(ids, baseline.1);
                    assert_eq!(
                        serde_json::to_vec(&placed).unwrap(),
                        serde_json::to_vec(&baseline.0).unwrap()
                    );
                }
            }
            assert_eq!(template.document, original);
            assert!(host.atoms.is_empty());
        }
    }
}

#[test]
fn library_attachment_keeps_the_generic_result_and_float_bits() {
    let template = builtin("Cyclopentane");
    for length in [31.496584, 70.] {
        let mut host = nature();
        let a = host.add_atom("C", Point::new(20., 30.));
        let b = host.add_atom("C", Point::new(20., 30. + length));
        host.add_bond(a, b, 1, "plain");
        for mode in [
            Connection::Auto,
            Connection::Connect,
            Connection::ShareAtom,
            Connection::FuseBond,
        ] {
            let point = if mode == Connection::FuseBond {
                Point::new(20., 30. + length / 2.)
            } else {
                Point::new(20., 30.)
            };
            for direction in [None, Some(point.offset(80., -20.))] {
                let baseline = place_with_mode(
                    &host,
                    &template.document,
                    point,
                    direction,
                    5.,
                    Anchor::Auto,
                    mode,
                )
                .unwrap();
                let actual = template
                    .place(&host, point, direction, 5., Anchor::Auto, mode)
                    .unwrap();
                assert_eq!(actual.1, baseline.1);
                assert_eq!(
                    serde_json::to_vec(&actual.0).unwrap(),
                    serde_json::to_vec(&baseline.0).unwrap(),
                    "{length}/{mode:?}/{direction:?}"
                );
            }
        }
    }
}

#[test]
fn personal_templates_and_toolbar_presets_keep_their_existing_sizes() {
    let mut personal = builtin("Cyclohexane").clone();
    personal.id = "personal:copy".into();
    personal.document.annotations.push(Annotation {
        id: personal.document.next_id(),
        position: Point::new(70., -50.),
        text: "Saved artwork".into(),
        format: Default::default(),
    });
    personal.document.graphics.push(Graphic::dragged(
        personal.document.next_id(),
        GraphicKind::Rectangle,
        Point::new(-60., -50.),
        Point::new(60., 50.),
        Default::default(),
        Default::default(),
        false,
    ));
    let mut host = nature();
    let existing = host.add_atom("N", Point::new(-400., -300.));
    let before = host.clone();
    let point = Point::new(160., 120.);
    let direction = Some(point.offset(-60., 20.));
    let baseline = place_with_mode(
        &host,
        &personal.document,
        point,
        direction,
        5.,
        Anchor::Auto,
        Connection::Connect,
    )
    .unwrap();
    let actual = personal
        .place(
            &host,
            point,
            direction,
            5.,
            Anchor::Auto,
            Connection::Connect,
        )
        .unwrap();
    assert_eq!(actual.1, baseline.1);
    assert_eq!(actual.0, baseline.0);
    assert_eq!(actual.0.atom(existing), before.atom(existing));
    assert_eq!(host, before);
    let (placed, _) = rings::Drawing {
        preset: rings::Preset::Regular,
        length: host.drawing_style.bond_length_world,
        alternate: false,
        connect: false,
    }
    .place(&host, point, None, 5.)
    .unwrap();
    for bond in &placed.bonds {
        let length = placed
            .atom(bond.a)
            .unwrap()
            .position
            .distance(placed.atom(bond.b).unwrap().position);
        assert!((length - host.drawing_style.bond_length_world).abs() < 0.001);
    }
}

#[test]
fn library_placement_keeps_validation_errors_and_destination_atomicity() {
    let template = builtin("Benzene");
    let host = nature();
    let before = host.clone();
    for (point, radius, anchor) in [
        (Point::default(), 0., Anchor::Auto),
        (Point::new(f32::NAN, 0.), 5., Anchor::Auto),
        (Point::default(), 5., Anchor::Atom(u64::MAX)),
    ] {
        let baseline = place_with_mode(
            &host,
            &template.document,
            point,
            None,
            radius,
            anchor,
            Connection::Connect,
        )
        .unwrap_err();
        let error = template
            .place(&host, point, None, radius, anchor, Connection::Connect)
            .unwrap_err();
        assert_eq!(error, baseline);
        assert_eq!(host, before);
    }
}
