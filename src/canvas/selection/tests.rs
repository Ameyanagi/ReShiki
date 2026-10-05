use super::*;

#[test]
fn centroid_only_selection_keeps_handles_without_an_independent_rotation_site() -> Result<(), String>
{
    let mut doc = Document::default();
    let a = doc.add_atom("C", World::new(-60., -40.));
    let b = doc.add_atom("C", World::new(-20., 0.));
    let c = doc.add_atom("C", World::new(40., 40.));
    let d = doc.add_atom("C", World::new(80., 80.));
    let first = reshiki::projection::add_centroid(&mut doc, &[a, b])?;
    let second = reshiki::projection::add_centroid(&mut doc, &[c, d])?;
    let ids = [first, second];
    let before = doc.clone();

    assert_eq!(editing::rotation_center(&doc, &ids), None);
    for zoom in [0.5, 1., 3.] {
        let camera = Camera {
            center: World::new(30., -15.),
            zoom,
        };
        let bounds = Rectangle::new(Point::new(80., 100.), Size::new(400., 300.));
        let selection = SelectionBox::new(&doc, &ids, camera, bounds)
            .ok_or("Two centroid markers must keep their selection box")?;
        // Marker positions are (-40, -20) and (60, 60).
        assert_eq!(selection.pivot, World::new(10., 20.));
        for (i, grip) in selection.grips().into_iter().enumerate() {
            assert_eq!(selection.hit(grip), Some(Handle::Resize(i)));
        }
        for (i, grip) in selection.edge_grips().into_iter().enumerate() {
            assert_eq!(selection.hit(grip), Some(Handle::Edge(i)));
        }
        assert_eq!(
            selection.hit(selection.rotation_grip()),
            Some(Handle::Rotate)
        );
        assert!(SelectionBox::new(&doc, &[first], camera, bounds).is_none());
    }
    assert_eq!(doc, before, "Building and hitting the box must not edit");

    editing::transform(&mut doc, &ids, editing::Transform::Rotate(15.));
    assert_eq!(
        doc, before,
        "Derived markers add no independent rotation site"
    );
    assert_eq!(editing::rotation_center(&doc, &ids), None);
    let camera = Camera::default();
    let bounds = Rectangle::new(Point::ORIGIN, Size::new(400., 300.));
    let selection = SelectionBox::new(&doc, &ids, camera, bounds).unwrap();
    for handle in [Handle::Rotate, Handle::Resize(0), Handle::Edge(0)] {
        let start = match handle {
            Handle::Rotate => camera.world(selection.rotation_grip(), bounds),
            Handle::Resize(i) => selection.corners[i],
            Handle::Edge(i) => selection.edge_points()[i],
        };
        let drag = TransformDrag::new(selection, handle, start, &ids);
        let end = start.offset(30., -20.);
        let mut preview = doc.clone();
        drag.apply(&mut preview, end, false);
        assert_eq!(
            preview, before,
            "Derived-only handles must not preview a temporary edit"
        );
    }
    Ok(())
}

#[test]
fn side_handles_stretch_one_axis_and_match_committed_geometry() -> Result<(), String> {
    let mut doc = Document::default();
    let a = doc.add_atom("C", World::new(-20., -10.));
    let b = doc.add_atom("C", World::new(20., 10.));
    doc.add_bond(a, b, 1, "plain");
    for zoom in [0.5, 1., 3.] {
        let camera = Camera {
            center: World::new(30., -15.),
            zoom,
        };
        let bounds = Rectangle::new(Point::new(80., 100.), Size::new(400., 300.));
        let selection = SelectionBox::new(&doc, &[a, b], camera, bounds).ok_or("Selection box")?;
        for (i, grip) in selection.edge_grips().into_iter().enumerate() {
            assert!(matches!(selection.hit(grip), Some(Handle::Edge(edge)) if edge == i));
            let start = camera.world(Point::new(grip.x + 2., grip.y - 2.), bounds);
            let drag = TransformDrag::new(selection, Handle::Edge(i), start, &[a, b]);
            assert_eq!(drag.values(start, false), (1., 0.));
            let end = start.offset(drag.corner.x - drag.pivot.x, drag.corner.y - drag.pivot.y);
            assert!((drag.values(end, false).0 - 2.).abs() < 0.001);
            let mut preview = doc.clone();
            drag.apply(&mut preview, end, false);
            let edit = drag.into_edit(end, false);
            let super::super::Edit::ScaleAxes { ids, pivot, x, y } = edit else {
                return Err("Side handle did not emit an axis resize".into());
            };
            let mut committed = doc.clone();
            editing::scale_axes_about(&mut committed, &ids, pivot, x, y);
            assert_eq!(preview, committed);
            for atom in &preview.atoms {
                let old = doc.atom(atom.id).ok_or("Original atom")?;
                if i.is_multiple_of(2) {
                    assert_eq!(atom.position.x, old.position.x);
                } else {
                    assert_eq!(atom.position.y, old.position.y);
                }
            }
            assert_eq!(preview.bonds, doc.bonds);
            assert_eq!(if i.is_multiple_of(2) { x } else { y }, 1.);
        }
    }
    Ok(())
}

#[test]
fn handles_keep_grab_offsets_and_snap_rotation_without_reflecting() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", World::new(-20.0, -10.0));
    let b = doc.add_atom("C", World::new(20.0, 10.0));
    doc.add_bond(a, b, 1, "plain");
    for zoom in [0.5, 1.0, 3.0] {
        let camera = Camera {
            center: World::new(30.0, -15.0),
            zoom,
        };
        let bounds = Rectangle::new(Point::new(80.0, 100.0), Size::new(400.0, 300.0));
        let selection = SelectionBox::new(&doc, &[a, b], camera, bounds).unwrap();
        let grip = selection.grips()[2];
        let start = camera.world(Point::new(grip.x + 3.0, grip.y - 2.0), bounds);
        let drag = TransformDrag::new(selection, Handle::Resize(2), start, &[a, b]);
        assert_eq!(drag.values(start, false), (1.0, 0.0));
        assert!((drag.values(start.offset(40.0, 20.0), false).0 - 2.0).abs() < 0.001);
        assert!(drag.values(start.offset(-100.0, -100.0), false).0 > 0.0);
        let start = camera.world(selection.rotation_grip(), bounds);
        let drag = TransformDrag::new(selection, Handle::Rotate, start, &[a, b]);
        let angle = 22.0_f32.to_radians();
        let radius = start.distance(drag.pivot);
        let end = drag
            .pivot
            .offset(radius * angle.sin(), -radius * angle.cos());
        assert!((drag.values(end, false).1 - 22.0).abs() < 0.001);
        assert_eq!(drag.values(end, true), (1.0, 15.0));
    }
}

#[test]
fn repeated_asymmetric_rotation_drags_keep_the_pivot_and_preview_geometry() {
    use reshiki::{
        document::{Annotation, Arrow},
        graphics::{Graphic, GraphicKind},
    };
    let points = |doc: &Document| {
        doc.atoms
            .iter()
            .map(|a| a.position)
            .chain(doc.annotations.iter().map(|a| a.position))
            .chain(doc.arrows.iter().flat_map(|a| [a.start, a.end]))
            .chain(
                doc.graphics
                    .iter()
                    .flat_map(|g| g.commands().into_iter().flat_map(|c| c.points())),
            )
            .collect::<Vec<_>>()
    };
    for mixed in [false, true] {
        let mut source = Document::default();
        source.add_atom("C", World::new(-20., -10.));
        source.add_atom("C", World::new(40., 0.));
        source.add_atom("N", World::new(0., 50.));
        let expected_pivot = if mixed {
            source.annotations.push(Annotation {
                id: source.next_id(),
                position: World::new(120., 60.),
                text: "Upright".into(),
                format: Default::default(),
            });
            source.arrows.push(Arrow::new(
                source.next_id(),
                World::new(80., -60.),
                World::new(160., -20.),
                Default::default(),
                Default::default(),
            ));
            source.graphics.push(Graphic::dragged(
                source.next_id(),
                GraphicKind::Rectangle,
                World::new(-140., -40.),
                World::new(-80., 60.),
                Default::default(),
                Default::default(),
                false,
            ));
            // Three atoms, caption, arrow midpoint, rectangle frame center.
            World::new(25., 70. / 6.)
        } else {
            World::new(20. / 3., 40. / 3.)
        };
        let ids = source.all_ids();
        for zoom in [0.5, 1., 3.] {
            let camera = Camera {
                center: World::new(30., -15.),
                zoom,
            };
            let bounds = Rectangle::new(Point::new(80., 100.), Size::new(400., 300.));
            let mut doc = source.clone();
            for _ in 0..6 {
                let selection = SelectionBox::new(&doc, &ids, camera, bounds).unwrap();
                assert!(selection.pivot.distance(expected_pivot) < 0.0001);
                let start = camera.world(selection.rotation_grip(), bounds);
                let drag = TransformDrag::new(selection, Handle::Rotate, start, &ids);
                let (s, c) = 15_f32.to_radians().sin_cos();
                let x = start.x - drag.pivot.x;
                let y = start.y - drag.pivot.y;
                let end = drag.pivot.offset(x * c - y * s, x * s + y * c);
                assert_eq!(drag.values(end, true), (1., 15.));
                let before = doc.clone();
                let mut preview = before.clone();
                drag.apply(&mut preview, end, true);
                assert_eq!(doc, before);
                let super::super::Edit::Transform {
                    ids,
                    pivot,
                    scale,
                    rotation,
                } = drag.into_edit(end, true)
                else {
                    panic!("Rotation must emit a transform");
                };
                editing::transform_about(&mut doc, &ids, pivot, scale, rotation);
                assert_eq!(doc, preview, "Preview and release must use the same pivot");
            }
            // Independent 90-degree oracle, with no call to the production center.
            let expected: Vec<_> = points(&source)
                .into_iter()
                .map(|p| {
                    World::new(
                        expected_pivot.x - (p.y - expected_pivot.y),
                        expected_pivot.y + (p.x - expected_pivot.x),
                    )
                })
                .collect();
            let scale = expected
                .iter()
                .fold(1_f32, |m, p| m.max(p.x.abs()).max(p.y.abs()));
            let epsilon = (8. * f32::EPSILON * scale).max(0.0001);
            for (actual, expected) in points(&doc).into_iter().zip(expected) {
                assert!(
                    actual.distance(expected) <= epsilon,
                    "{actual:?} != {expected:?}"
                );
            }
        }
    }
}
