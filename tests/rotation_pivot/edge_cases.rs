//! Rotation acceptance cases whose geometry must survive a freshly computed pivot.
use super::{assert_points, geometry, rotated, tolerance};
use reshiki::{
    chemistry::cdxml::{graphics, presentation},
    document::{Annotation, AtomStereo, Document, Point},
    editing::{self, Transform},
    graphics::{GraphicKind, PathCommand},
};

fn reopen(doc: &Document) -> Document {
    let reopened: Document = serde_json::from_slice(&serde_json::to_vec(doc).unwrap()).unwrap();
    assert_eq!(
        &reopened, doc,
        "Serialization alone must retain the exact snapshot"
    );
    reopened
}

/// Ignore only the geometry that rotation may change, including an implicit
/// arrow control becoming explicit. All IDs, text, styles and depth stay exact.
fn assert_metadata(actual: &Document, original: &Document) {
    let mut restored = actual.clone();
    for (a, b) in restored.atoms.iter_mut().zip(&original.atoms) {
        a.position = b.position;
    }
    for (a, b) in restored.annotations.iter_mut().zip(&original.annotations) {
        a.position = b.position;
    }
    for (a, b) in restored.arrows.iter_mut().zip(&original.arrows) {
        a.start = b.start;
        a.end = b.end;
        a.control = b.control;
    }
    for (a, b) in restored.graphics.iter_mut().zip(&original.graphics) {
        a.origin = b.origin;
        a.axis_x = b.axis_x;
        a.axis_y = b.axis_y;
    }
    assert_eq!(&restored, original);
}

fn assert_rotated(doc: &Document, source: &Document, ids: &[u64], pivot: Point, degrees: f64) {
    assert_points(
        geometry(doc, ids),
        geometry(source, ids)
            .into_iter()
            .map(|p| rotated(p, pivot, degrees))
            .collect(),
    );
    assert_points(
        vec![editing::rotation_center(doc, ids).unwrap()],
        vec![pivot],
    );
    assert_metadata(doc, source);
    for atom in &source.atoms {
        if !ids.contains(&atom.id) {
            assert_eq!(doc.atom(atom.id), Some(atom));
        }
    }
}

#[test]
fn legacy_and_explicit_arrow_controls_keep_a_fixed_mixed_pivot_after_reopen() {
    for kind in ["bent", "curved", "fishhook"] {
        for explicit in [false, true] {
            let mut source = super::mixed();
            let ids = source.all_ids();
            let arrow = &mut source.arrows[0];
            arrow.kind = kind.into();
            arrow.control = explicit.then_some(Point::new(165., -65.));
            arrow.style = explicit.then(|| reshiki::arrows::ArrowStyle {
                width_pt: 1.7,
                ..Default::default()
            });
            // The legacy midpoint + perpendicular-half-span rule is known
            // from these endpoints, independent of the rotation implementation.
            assert_eq!(
                arrow.control_point(),
                Some(if explicit {
                    Point::new(165., -65.)
                } else {
                    Point::new(160., 20.)
                })
            );
            source.atoms[0].depth = 7.;
            source.graphics[0].depth = [3., -4., 5.];
            source.add_atom("O", Point::new(400., 130.));
            source.validate().unwrap();
            // Two atoms, caption insertion, arrow midpoint and rectangle center.
            let pivot = Point::new(110., 6.);
            for step in [1., 15.] {
                let mut doc = source.clone();
                for _ in 0..3 {
                    editing::transform(&mut doc, &ids, Transform::Rotate(step));
                    assert!(doc.arrows[0].control.is_some());
                    assert_rotated(&doc, &source, &ids, pivot, f64::from(step));
                    doc = reopen(&doc);
                    editing::transform(&mut doc, &ids, Transform::Rotate(-step));
                    assert_rotated(&doc, &source, &ids, pivot, 0.);
                }
            }
            let mut doc = source.clone();
            for step in 1..=12 {
                editing::transform(&mut doc, &ids, Transform::Rotate(15.));
                assert_rotated(&doc, &source, &ids, pivot, f64::from(step * 15));
            }
        }
    }
}

#[test]
fn imported_open_and_closed_multicubic_paths_keep_their_local_anchor() {
    // CDXML stores incoming, anchor, outgoing triples. The closed case uses
    // the final outgoing and first incoming points for a third cubic.
    const POINTS: &str = "-40 5 -20 10 -10 -45 55 -15 70 20 90 65 145 50 130 -10 170 -30";
    for closed in [false, true] {
        let xml = format!(
            "<CDXML><page><curve id='7' Closed='{}' LineType='Dashed' CurvePoints='{POINTS}'/></page></CDXML>",
            if closed { "yes" } else { "no" }
        );
        let parsed = presentation::parse(&xml).unwrap();
        let mut imported = graphics::read(parsed.root_element(), 1., 10, &Default::default())
            .unwrap()
            .into_document()
            .unwrap();
        assert_eq!(imported.len(), 1);
        let mut graphic = imported.remove(0);
        let mut expected_path = vec![
            PathCommand::Move(Point::new(-20., 10.)),
            PathCommand::Cubic(
                Point::new(-10., -45.),
                Point::new(55., -15.),
                Point::new(70., 20.),
            ),
            PathCommand::Cubic(
                Point::new(90., 65.),
                Point::new(145., 50.),
                Point::new(130., -10.),
            ),
        ];
        if closed {
            expected_path.extend([
                PathCommand::Cubic(
                    Point::new(170., -30.),
                    Point::new(-40., 5.),
                    Point::new(-20., 10.),
                ),
                PathCommand::Close,
            ]);
        }
        assert_eq!(graphic.kind, GraphicKind::Path);
        assert_eq!(graphic.path, expected_path);
        graphic.origin = Point::new(110., -75.);
        graphic.axis_x = Point::new(0.8, 0.6);
        graphic.axis_y = Point::new(-0.9, 1.2);
        graphic.depth = [3., -4., 5.];
        let local_anchor = Point::new(if closed { 65. } else { 62.5 }, 10.);
        // Explicit affine image of the known local control-box center.
        let anchor = Point::new(
            (110. + 0.8 * f64::from(local_anchor.x) - 0.9 * f64::from(local_anchor.y)) as f32,
            (-75. + 0.6 * f64::from(local_anchor.x) + 1.2 * f64::from(local_anchor.y)) as f32,
        );
        let mut source = Document {
            graphics: vec![graphic],
            ..Default::default()
        };
        assert_points(
            vec![editing::rotation_center(&source, &[10]).unwrap()],
            vec![anchor],
        );
        source.add_atom("N", Point::new(-25., 80.));
        source.annotations.push(Annotation {
            id: source.next_id(),
            position: Point::new(250., -60.),
            text: "Keep upright".into(),
            format: Default::default(),
        });
        let ids = source.all_ids();
        source.group_selection(&ids).unwrap();
        source.add_atom("O", Point::new(800., -600.));
        source.validate().unwrap();
        let pivot = Point::new(
            ((-25. + f64::from(anchor.x) + 250.) / 3.) as f32,
            ((80. + f64::from(anchor.y) - 60.) / 3.) as f32,
        );
        let mut doc = source.clone();
        for _ in 0..3 {
            editing::transform(&mut doc, &ids, Transform::Rotate(15.));
            assert_rotated(&doc, &source, &ids, pivot, 15.);
            assert_points(
                vec![editing::rotation_center(&doc, &[10]).unwrap()],
                vec![rotated(anchor, pivot, 15.)],
            );
            doc = reopen(&doc);
            editing::transform(&mut doc, &ids, Transform::Rotate(-15.));
            assert_rotated(&doc, &source, &ids, pivot, 0.);
        }
        doc = source.clone();
        for step in 1..=12 {
            editing::transform(&mut doc, &ids, Transform::Rotate(15.));
            assert_rotated(&doc, &source, &ids, pivot, f64::from(step * 15));
        }
        assert_eq!(doc.graphics[0].path, expected_path);
    }
}

#[test]
fn partial_rotation_preserves_internal_lengths_and_invalidates_boundary_stereo() {
    let mut source = Document::default();
    let a = source.add_atom("C", Point::new(-40., 0.));
    let b = source.add_atom("C", Point::new(20., 40.));
    let c = source.add_atom("C", Point::new(65., 10.));
    let d = source.add_atom("C", Point::new(110., 10.));
    let e = source.add_atom("F", Point::new(140., 40.));
    let f = source.add_atom("O", Point::new(25., 80.));
    let g = source.add_atom("F", Point::new(60., 55.));
    for (from, to, order, display) in [
        (a, b, 1, "plain"),
        (b, c, 1, "plain"),
        (c, d, 2, "plain"),
        (d, e, 1, "plain"),
        (b, f, 1, "wedge"),
        (b, g, 1, "plain"),
    ] {
        source.add_bond(from, to, order, display);
    }
    source.atom_mut(b).unwrap().stereo = Some(AtomStereo {
        winding: "ccw".into(),
        neighbors: vec![a, c, f, g],
    });
    source.bonds[2].stereo = Some("E".into());
    source.bonds[2].stereo_atoms = vec![b, e];
    let unrelated = source.add_atom("C", Point::new(300., 150.));
    let neighbors: Vec<_> = [
        ("N", Point::new(270., 130.)),
        ("O", Point::new(320., 115.)),
        ("F", Point::new(325., 175.)),
        ("Cl", Point::new(280., 185.)),
    ]
    .into_iter()
    .map(|(element, point)| {
        let id = source.add_atom(element, point);
        source.add_bond(unrelated, id, 1, "plain");
        id
    })
    .collect();
    source.atom_mut(unrelated).unwrap().stereo = Some(AtomStereo {
        winding: "cw".into(),
        neighbors,
    });
    source.validate().unwrap();
    let ids = [a, b, f, g];
    let pivot = Point::new(16.25, 43.75);
    // A partial rigid transform intentionally invalidates the chemical stereo
    // whose frame crosses the selection. The inverse must not resurrect it.
    let mut expected_metadata = source.clone();
    expected_metadata.atom_mut(b).unwrap().stereo = None;
    expected_metadata.bonds[2].stereo = None;
    expected_metadata.bonds[2].stereo_atoms.clear();
    let internal = |doc: &Document| {
        [(a, b), (b, f), (b, g)].map(|(a, b)| {
            doc.atom(a)
                .unwrap()
                .position
                .distance(doc.atom(b).unwrap().position)
        })
    };
    let lengths = internal(&source);
    let mut doc = source.clone();
    for _ in 0..3 {
        for degrees in [15., -15.] {
            editing::transform(&mut doc, &ids, Transform::Rotate(degrees));
            let total = if degrees > 0. { 15. } else { 0. };
            assert_points(
                ids.iter()
                    .map(|id| doc.atom(*id).unwrap().position)
                    .collect(),
                ids.iter()
                    .map(|id| rotated(source.atom(*id).unwrap().position, pivot, total))
                    .collect(),
            );
            let epsilon = tolerance(ids.iter().map(|id| source.atom(*id).unwrap().position));
            for (actual, expected) in internal(&doc).into_iter().zip(lengths) {
                assert!((actual - expected).abs() <= (2. * epsilon).max(0.0002));
            }
            for atom in &source.atoms {
                if !ids.contains(&atom.id) {
                    assert_eq!(doc.atom(atom.id).unwrap().position, atom.position);
                }
            }
            assert_metadata(&doc, &expected_metadata);
            assert_eq!(doc.atom(unrelated), source.atom(unrelated));
            assert!(doc.atom(b).unwrap().stereo.is_none());
            assert!(doc.bonds[2].stereo.is_none());
            assert!(doc.bonds[2].stereo_atoms.is_empty());
            assert_eq!(doc.bonds[4].display, "wedge");
            doc.validate().unwrap();
            doc = reopen(&doc);
        }
    }
}
