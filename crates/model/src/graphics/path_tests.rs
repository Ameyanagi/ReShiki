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
        pen.edit_point(1, *a);
        pen.edit_point(2, *b);
        for command in rest {
            match *command {
                PathCommand::Cubic(a, b, end) if original.path_closed() && end == *start => {
                    let last = pen
                        .path_handles()
                        .unwrap()
                        .into_iter()
                        .filter(|handle| handle.node)
                        .next_back()
                        .unwrap()
                        .index;
                    pen.set_path_closed(true).unwrap();
                    pen.set_path_segment_curved(last, true).unwrap();
                    pen.edit_point(last + 1, a);
                    pen.edit_point(last + 2, b);
                }
                PathCommand::Cubic(a, b, end) => {
                    let index = pen
                        .append_pen_node(end, Some(Point::new(2. * end.x - b.x, 2. * end.y - b.y)))
                        .unwrap();
                    pen.edit_point(index - 2, a);
                    pen.edit_point(index - 1, b);
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
    doc.graphics[0].edit_point(3, original[3].offset(20., -10.));
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
    closed.edit_point(0, points[0].offset(-15., 20.));
    let changed = closed.edit_points();
    for i in [0, 1, 11, 12] {
        assert_eq!(changed[i], points[i].offset(-15., 20.));
    }
    closed.edit_point(1, changed[1].offset(30., 0.));
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
        graphic.edit_point(handle.index, handle.point);
        assert_eq!(graphic, before);
    }
    graphic.edit_point(usize::MAX, Point::new(0., 0.));
    assert_eq!(graphic, before);
    graphic.set_path_segment_curved(0, true).unwrap();
    assert_eq!(graphic, before);
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
