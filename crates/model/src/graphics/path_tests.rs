use crate::document::Point;
use crate::graphics::{Graphic, GraphicStyle};
use crate::{document::Document, graphics::PathCommand};

fn fixture() -> Document {
    Document::from_native_file(include_bytes!(
        "../../../../tests/fixtures/tunable-pen-lines-67/before.rsk"
    ))
    .unwrap()
}
fn near(a: Point, b: Point) {
    assert!(a.distance(b) < 0.001, "{a:?} != {b:?}");
}

#[test]
fn compound_paths_keep_all_generic_points_and_edit_only_the_indexed_control() {
    let points = [
        Point::new(0., 0.),
        Point::new(40., 0.),
        Point::new(40., 40.),
        Point::new(80., 0.),
        Point::new(100., 20.),
        Point::new(110., 40.),
        Point::new(140., 40.),
        Point::new(160., 20.),
    ];
    let mut graphic = Graphic::pen_curve(1, points[0], points[1], Default::default());
    graphic.path = vec![
        PathCommand::Move(points[0]),
        PathCommand::Line(points[1]),
        PathCommand::Line(points[2]),
        PathCommand::Move(points[3]),
        PathCommand::Line(points[4]),
        PathCommand::Cubic(points[5], points[6], points[7]),
    ];
    assert_eq!(graphic.path_handles(), None);
    assert_eq!(graphic.path_summary(), None);
    assert_eq!(graphic.edit_points(), points);
    assert_eq!(
        graphic
            .move_path_point(5, Point::new(f32::NAN, 0.))
            .unwrap_err(),
        "Select one continuous path"
    );
    let replacement = points[5].offset(-12., 17.);
    graphic.edit_point(5, replacement).unwrap();
    let mut expected = points;
    expected[5] = replacement;
    assert_eq!(graphic.edit_points(), expected);
    assert_eq!(graphic.path_handles(), None);
    assert_eq!(graphic.path_summary(), None);
    let doc = Document {
        graphics: vec![graphic],
        ..Document::default()
    };
    assert_eq!(
        Document::from_native_file(&doc.file_json().unwrap()).unwrap(),
        doc.current()
    );
}

#[test]
fn path_summary_preserves_handle_order_and_centralizes_node_limits() {
    use crate::graphics::{MIN_CLOSED_NODES, MIN_OPEN_NODES};
    assert_eq!(MIN_OPEN_NODES, 2);
    assert_eq!(MIN_CLOSED_NODES, 3);
    for graphic in fixture().graphics {
        let summary = graphic.path_summary().unwrap();
        assert_eq!(summary.handles, graphic.path_handles().unwrap());
        assert_eq!(summary.closed, graphic.path_closed());
        assert_eq!(
            summary.nodes().map(|node| node.index).collect::<Vec<_>>(),
            vec![0, 3, 6, 9]
        );
        assert_eq!(summary.node_count(), 4);
        assert!(summary.can_close());
        assert!(summary.can_delete_node());
        for ordinal in 0..summary.node_count() {
            assert_eq!(
                summary.has_following(ordinal),
                summary.closed || ordinal < 3
            );
        }
        if summary.closed {
            assert!(!summary.handles.iter().any(|handle| handle.index == 12));
        }
    }
    let mut graphic = Graphic::pen_curve(
        1,
        Point::new(0., 0.),
        Point::new(80., 20.),
        Default::default(),
    );
    let summary = graphic.path_summary().unwrap();
    assert_eq!(summary.node_count(), MIN_OPEN_NODES);
    assert!(!summary.can_close());
    assert!(!summary.can_delete_node());
    assert!(summary.has_following(0));
    assert!(!summary.has_following(1));
    assert!(!summary.has_following(usize::MAX));
    graphic.append_pen_node(Point::new(160., 0.), None).unwrap();
    let summary = graphic.path_summary().unwrap();
    assert_eq!(summary.node_count(), MIN_CLOSED_NODES);
    assert!(summary.can_close());
    assert!(summary.can_delete_node());
    assert!(!summary.has_following(2));
    graphic.set_path_closed(true).unwrap();
    let summary = graphic.path_summary().unwrap();
    assert!(summary.closed);
    assert!(summary.can_close());
    assert!(!summary.can_delete_node());
    assert!(summary.has_following(2));
    assert_eq!(
        summary.nodes().map(|node| node.index).collect::<Vec<_>>(),
        vec![0, 3, 4]
    );
}

#[test]
fn world_to_local_inverts_the_drawing_frame() {
    let mut graphic = Graphic::dragged(
        1,
        super::GraphicKind::Curve,
        Point::new(17., -9.),
        Point::new(100., 80.),
        GraphicStyle::default(),
        super::BracketSides::Both,
        false,
    );
    graphic.axis_x = Point::new(4., 1.5);
    graphic.axis_y = Point::new(-2., 3.);
    let determinant = graphic.frame_determinant();
    assert_eq!(
        determinant,
        graphic.axis_x.x * graphic.axis_y.y - graphic.axis_x.y * graphic.axis_y.x
    );
    assert_eq!(determinant, 15.);
    for (u, v) in [(0., 0.), (0.25, 0.75), (-2., 3.)] {
        let world = Point::new(
            graphic.origin.x + graphic.axis_x.x * u + graphic.axis_y.x * v,
            graphic.origin.y + graphic.axis_x.y * u + graphic.axis_y.y * v,
        );
        let local = graphic.world_to_local(world, determinant);
        assert!((local.x - u).abs() < 1e-5);
        assert!((local.y - v).abs() < 1e-5);
    }
}

#[test]
fn reset_frame_restores_the_identity_frame() {
    let mut graphic = Graphic::dragged(
        1,
        super::GraphicKind::Curve,
        Point::new(17., -9.),
        Point::new(100., 80.),
        GraphicStyle::default(),
        super::BracketSides::Both,
        false,
    );
    graphic.axis_x = Point::new(4., 1.5);
    graphic.axis_y = Point::new(-2., 3.);
    graphic.reset_frame();
    assert_eq!(graphic.origin, Point::default());
    assert_eq!(graphic.axis_x, Point::new(1., 0.));
    assert_eq!(graphic.axis_y, Point::new(0., 1.));
}

fn point(command: &PathCommand, start: Point, t: f32) -> Point {
    let lerp =
        |a: Point, b: Point, t: f32| Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
    match *command {
        PathCommand::Cubic(a, b, end) => lerp(
            lerp(lerp(start, a, t), lerp(a, b, t), t),
            lerp(lerp(a, b, t), lerp(b, end, t), t),
            t,
        ),
        PathCommand::Line(end) => lerp(start, end, t),
        _ => panic!("segment"),
    }
}

#[test]
fn retained_pen_shapes_are_connected_editable_paths_with_native_geometry() {
    let doc = Document::from_native_file(include_bytes!(
        "../../../../tests/fixtures/tunable-pen-lines-67/before.rsk"
    ))
    .unwrap();
    assert_eq!(doc.graphics.len(), 2);
    for graphic in &doc.graphics {
        assert_eq!(
            graphic
                .commands()
                .iter()
                .filter(|c| matches!(c, PathCommand::Move(_)))
                .count(),
            1
        );
        let point = graphic.edit_points()[0];
        assert!(graphic.hit(point, 2.));
        let (lo, hi) = graphic.bounds();
        assert!(lo.x <= point.x && point.x <= hi.x && lo.y <= point.y && point.y <= hi.y);
    }
    assert_eq!(
        Document::from_native_file(&doc.file_json().unwrap()).unwrap(),
        doc.current()
    );
}

#[test]
fn retained_non_arc_shapes_can_be_constructed_as_one_pen_path() {
    for original in fixture().graphics {
        let commands = original.commands();
        let [
            PathCommand::Move(start),
            PathCommand::Cubic(a, b, end),
            rest @ ..,
        ] = commands.as_slice()
        else {
            panic!("retained pen outline")
        };
        let mut pen = Graphic::pen_curve(original.id, *start, *end, original.style.clone());
        let _ = pen.edit_point(1, *a);
        let _ = pen.edit_point(2, *b);
        for command in rest {
            match *command {
                PathCommand::Cubic(a, b, end) if original.path_closed() && end == *start => {
                    let last = pen
                        .path_handles()
                        .unwrap()
                        .into_iter()
                        .rfind(|handle| handle.node)
                        .unwrap()
                        .index;
                    pen.set_path_closed(true).unwrap();
                    pen.set_path_segment_curved(last, true).unwrap();
                    let _ = pen.edit_point(last + 1, a);
                    let _ = pen.edit_point(last + 2, b);
                }
                PathCommand::Cubic(a, b, end) => {
                    let index = pen
                        .append_pen_node(end, Some(Point::new(2. * end.x - b.x, 2. * end.y - b.y)))
                        .unwrap();
                    let _ = pen.edit_point(index - 2, a);
                    let _ = pen.edit_point(index - 1, b);
                }
                PathCommand::Close => pen.set_path_closed(true).unwrap(),
                _ => panic!("unexpected retained segment"),
            }
        }
        assert_eq!(pen.commands(), commands);
        assert_eq!(
            pen.path_handles()
                .unwrap()
                .iter()
                .filter(|handle| handle.node)
                .count(),
            original
                .path_handles()
                .unwrap()
                .iter()
                .filter(|handle| handle.node)
                .count()
        );
        pen.validate().unwrap();
    }
}

#[test]
fn node_drags_transport_adjacent_controls_and_closed_seam_exactly_once() {
    let mut doc = fixture();
    let original = doc.graphics[0].edit_points();
    let _ = doc.graphics[0].edit_point(3, original[3].offset(20., -10.));
    let changed = doc.graphics[0].edit_points();
    for i in [2, 3, 4] {
        assert_eq!(changed[i], original[i].offset(20., -10.));
    }
    for i in [0, 1, 5, 6, 7, 8, 9] {
        assert_eq!(changed[i], original[i]);
    }
    let mut closed = doc.graphics[1].clone();
    let handles = closed.path_handles().unwrap();
    assert_eq!(handles.iter().filter(|h| h.node).count(), 4);
    let points = closed.edit_points();
    let _ = closed.edit_point(0, points[0].offset(-15., 20.));
    let changed = closed.edit_points();
    for i in [0, 1, 11, 12] {
        assert_eq!(changed[i], points[i].offset(-15., 20.));
    }
    let _ = closed.edit_point(1, changed[1].offset(30., 0.));
    let control_edit = closed.edit_points();
    for i in [0, 2, 11, 12] {
        assert_eq!(control_edit[i], changed[i]);
    }
    closed.validate().unwrap();
}

#[test]
fn path_noop_edits_preserve_native_type_and_affine_frame() {
    let mut graphic = Graphic::dragged(
        1,
        super::GraphicKind::Curve,
        Point::new(10., 20.),
        Point::new(100., 80.),
        GraphicStyle::default(),
        super::BracketSides::Both,
        false,
    );
    let before = graphic.clone();
    for handle in graphic.path_handles().unwrap() {
        let _ = graphic.edit_point(handle.index, handle.point);
        assert_eq!(graphic, before);
    }
    let _ = graphic.edit_point(usize::MAX, Point::new(0., 0.));
    assert_eq!(graphic, before);
    graphic.set_path_segment_curved(0, true).unwrap();
    assert_eq!(graphic, before);
}

#[test]
fn edit_point_reports_connected_path_results_with_the_same_noop_rules() {
    let mut graphic = Graphic::dragged(
        1,
        super::GraphicKind::Curve,
        Point::new(10., 20.),
        Point::new(100., 80.),
        GraphicStyle::default(),
        super::BracketSides::Both,
        false,
    );
    let before = graphic.clone();
    graphic.edit_point(0, Point::new(f32::NAN, 0.)).unwrap();
    assert_eq!(graphic, before);

    let mut closed = fixture().graphics.remove(1);
    let before = closed.clone();
    let point = closed.edit_points()[0];
    closed.edit_point(12, point).unwrap();
    assert_eq!(closed, before);
    closed.move_path_point(12, point.offset(10., -15.)).unwrap();
    let moved = closed.edit_points();
    assert_eq!(moved[0], point.offset(10., -15.));
    assert_eq!(moved[12], moved[0]);

    let mut singular = fixture().graphics.remove(0);
    singular.axis_y = Point::default();
    singular.depth = [10., 0., 1.];
    let before = singular.clone();
    let node = singular.edit_points()[3];
    assert!(
        singular
            .edit_point(3, node.offset(10., 10.))
            .unwrap_err()
            .contains("Rotate this edge-on projected path")
    );
    assert_eq!(singular, before);
}

#[test]
fn projected_pen_edits_keep_the_frame_that_defines_their_depth() {
    for around_x in [false, true] {
        let mut doc = fixture();
        doc.graphics.truncate(1);
        let id = doc.graphics[0].id;
        crate::projection::tilt(&mut doc, &[id], 35., around_x);
        let graphic = &mut doc.graphics[0];
        let frame = (
            graphic.origin,
            graphic.axis_x,
            graphic.axis_y,
            graphic.depth,
        );
        assert!(graphic.depth.iter().any(|z| z.abs() > 0.01));
        let before = graphic.edit_points();
        let _ = graphic.edit_point(3, before[3].offset(20., -10.));
        let moved = graphic.edit_points();
        for index in [2, 3, 4] {
            near(moved[index], before[index].offset(20., -10.));
        }
        for index in [0, 1, 5, 6, 7, 8, 9] {
            near(moved[index], before[index]);
        }
        let inserted = graphic.insert_path_node(0).unwrap();
        graphic.delete_path_node(inserted).unwrap();
        graphic.set_path_segment_curved(0, false).unwrap();
        graphic.set_path_segment_curved(0, true).unwrap();
        graphic
            .append_pen_node(Point::new(350., 30.), None)
            .unwrap();
        graphic.set_path_closed(true).unwrap();
        graphic.set_path_closed(false).unwrap();
        assert_eq!(
            (
                graphic.origin,
                graphic.axis_x,
                graphic.axis_y,
                graphic.depth,
            ),
            frame,
            "all edits retain the projected plane's affine coordinates"
        );
        graphic.validate().unwrap();
        assert_eq!(
            Document::from_native_file(&doc.file_json().unwrap()).unwrap(),
            doc.current()
        );
    }
}

#[test]
fn singular_projected_pen_edits_reject_atomically_and_flat_lines_remain_editable() {
    let mut graphic = fixture().graphics.remove(0);
    graphic.axis_y = Point::default();
    graphic.depth = [10., 0., 1.];
    let before = graphic.clone();
    let node = graphic.edit_points()[3];
    let _ = graphic.edit_point(3, node.offset(10., 10.));
    assert_eq!(graphic, before);
    assert!(
        graphic
            .move_path_point(3, node.offset(10., 10.))
            .unwrap_err()
            .contains("Rotate this edge-on projected path")
    );
    assert_eq!(graphic, before);
    assert!(graphic.insert_path_node(0).is_err());
    assert_eq!(graphic, before);
    assert!(
        graphic
            .append_pen_node(node.offset(30., 10.), None)
            .is_err()
    );
    assert_eq!(graphic, before);

    graphic.depth = [10., 0., 0.];
    let before = graphic.edit_points();
    let _ = graphic.edit_point(3, before[3].offset(10., 10.));
    let moved = graphic.edit_points();
    for index in [2, 3, 4] {
        near(moved[index], before[index].offset(10., 10.));
    }
    assert_eq!(graphic.depth, [10., 0., 0.]);
    assert_eq!(graphic.axis_x, Point::new(1., 0.));
    assert_eq!(graphic.axis_y, Point::new(0., 1.));
    graphic.validate().unwrap();
}

#[test]
fn inserting_cubic_nodes_is_exact_and_delete_retains_outer_tangents() {
    let mut graphic = fixture().graphics[0].clone();
    let before = graphic.commands();
    let start = graphic.edit_points()[0];
    let inserted = graphic.insert_path_node(0).unwrap();
    assert_eq!(inserted, 3);
    let after = graphic.commands();
    for i in 0..=32 {
        let t = i as f32 / 32.;
        let actual = if t <= 0.5 {
            point(&after[1], start, t * 2.)
        } else {
            point(&after[2], point(&after[1], start, 1.), (t - 0.5) * 2.)
        };
        near(actual, point(&before[1], start, t));
    }
    let outer = graphic.edit_points();
    graphic.delete_path_node(inserted).unwrap();
    let merged = graphic.edit_points();
    assert_eq!(merged[1], outer[1]);
    assert_eq!(merged[2], outer[5]);
    assert_eq!(merged[3], outer[6]);
    for node in [0, 3, 6, 9] {
        let mut closed = fixture().graphics[1].clone();
        closed.delete_path_node(node).unwrap();
        assert_eq!(
            closed
                .path_handles()
                .unwrap()
                .iter()
                .filter(|h| h.node)
                .count(),
            3
        );
        assert!(closed.path_closed());
        closed.validate().unwrap();
        assert!(closed.delete_path_node(0).is_err());
    }
}

#[test]
fn pen_segments_share_one_path_with_straight_curve_and_open_close_edits() {
    let mut graphic = Graphic::pen_curve(
        1,
        Point::new(0., 0.),
        Point::new(80., 20.),
        GraphicStyle::default(),
    );
    let initial = graphic.commands();
    let index = graphic.append_pen_node(Point::new(160., 0.), None).unwrap();
    assert_eq!(index, 4);
    assert!(matches!(graphic.commands()[2], PathCommand::Line(_)));
    graphic
        .append_pen_node(Point::new(240., 20.), Some(Point::new(240., 70.)))
        .unwrap();
    let commands = graphic.commands();
    assert_eq!(commands[0], initial[0]);
    assert_eq!(commands[1], initial[1]);
    let PathCommand::Cubic(_, arrival, end) = commands[3] else {
        panic!("curved segment")
    };
    assert_eq!(arrival, Point::new(240., -30.));
    assert_eq!(end, Point::new(240., 20.));
    graphic.set_path_segment_curved(3, true).unwrap();
    assert!(matches!(graphic.commands()[2], PathCommand::Cubic(..)));
    graphic.set_path_segment_curved(3, false).unwrap();
    assert!(
        matches!(graphic.commands()[2], PathCommand::Line(point) if point == Point::new(160.,0.))
    );
    graphic.set_path_closed(true).unwrap();
    assert!(graphic.path_closed());
    graphic.set_path_closed(false).unwrap();
    assert!(!graphic.path_closed());
    assert_eq!(
        graphic
            .path_handles()
            .unwrap()
            .iter()
            .filter(|h| h.node)
            .count(),
        4
    );
    graphic.validate().unwrap();
    let mut doc = Document {
        graphics: vec![graphic.clone()],
        ..Document::default()
    };
    let ids = crate::editing::append(
        &mut doc,
        &Document {
            graphics: vec![graphic.clone()],
            ..Document::default()
        },
        Point::new(30., 40.),
    );
    crate::editing::transform_about(&mut doc, &ids, Point::new(30., 40.), 2., 90.);
    for (a, b) in graphic
        .edit_points()
        .into_iter()
        .zip(doc.graphics[1].edit_points())
    {
        near(b, Point::new(30. - 2. * a.y, 40. + 2. * a.x));
    }
    assert_eq!(
        Document::from_native_file(&doc.file_json().unwrap()).unwrap(),
        doc.current()
    );
}
