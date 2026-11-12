use super::{ArrowStyle, FIRST_NODE_HANDLE, Head, HeadShape, NodePart, Preset, node_handle};
use crate::{
    document::{Arrow, ArrowNode, Document, Point},
    graphics::{PathCommand, PathNodeType},
};
use PathNodeType::{Corner, Smooth, Symmetric};

fn near(actual: Point, expected: Point) {
    assert!(
        actual.distance(expected) < 0.001,
        "{actual:?} != {expected:?}"
    );
}

fn path_ends(commands: &[PathCommand]) -> [Point; 2] {
    [
        commands[0].points()[0],
        *commands.last().unwrap().points().last().unwrap(),
    ]
}

fn handle(node: usize, part: NodePart) -> usize {
    FIRST_NODE_HANDLE
        + node * 3
        + match part {
            NodePart::Node => 0,
            NodePart::Arriving => 1,
            NodePart::Leaving => 2,
        }
}

fn joint() -> Arrow {
    let mut arrow = Arrow::new(
        7,
        Point::default(),
        Point::new(120., 0.),
        Preset::Curved,
        ArrowStyle::default(),
    );
    arrow.cubic = Some([Point::new(20., -30.), Point::new(100., 40.)]);
    arrow.nodes = vec![ArrowNode {
        point: Point::new(60., 0.),
        controls: [Point::new(40., 0.), Point::new(60., 35.)],
        kind: Corner,
    }];
    arrow
}

#[test]
fn legacy_arrow_json_omits_nodes_and_round_trips_bytewise() {
    for json in [
        r#"{"id":7,"start":{"x":0.0,"y":0.0},"end":{"x":120.0,"y":0.0},"kind":"forward"}"#,
        r#"{"id":7,"start":{"x":0.0,"y":0.0},"end":{"x":120.0,"y":0.0},"kind":"curved","control":{"x":50.0,"y":-30.0}}"#,
        r#"{"id":7,"start":{"x":0.0,"y":0.0},"end":{"x":120.0,"y":0.0},"kind":"curved","cubic":[{"x":20.0,"y":-30.0},{"x":100.0,"y":40.0}]}"#,
    ] {
        let arrow: Arrow = serde_json::from_str(json).unwrap();
        assert!(arrow.nodes.is_empty());
        assert_eq!(serde_json::to_string(&arrow).unwrap(), json);
    }
}

#[test]
fn nodes_use_native_version_35_and_external_metadata_is_validated() {
    let arrow = joint();
    let mut doc = Document {
        arrows: vec![arrow.clone()],
        ..Document::default()
    };
    assert_eq!(doc.file_version(), 35);
    let bytes = doc.file_json().unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["version"], 35);
    assert_eq!(value["arrows"][0]["nodes"][0]["kind"], "corner");
    assert_eq!(Document::from_json(&bytes).unwrap(), doc.current());

    for (field, bad) in [
        ("kind", serde_json::json!("bent")),
        ("control", serde_json::json!({"x": 60., "y": 30.})),
    ] {
        let mut invalid = value.clone();
        invalid["arrows"][0][field] = bad;
        assert!(Document::from_json(&serde_json::to_vec(&invalid).unwrap()).is_err());
    }
    let mut invalid = value;
    invalid["arrows"][0]["nodes"][0]["kind"] = serde_json::json!("unknown");
    assert!(Document::from_json(&serde_json::to_vec(&invalid).unwrap()).is_err());
    for part in 0..3 {
        let mut invalid = arrow.clone();
        if part == 0 {
            invalid.nodes[0].point.x = f32::NAN;
        } else {
            invalid.nodes[0].controls[part - 1].y = f32::INFINITY;
        }
        assert!(invalid.validate().is_err());
    }
    doc.arrows[0].nodes.clear();
    assert_eq!(doc.file_version(), crate::document::DISTANCE_BONDS);
}

#[test]
fn insertion_splits_lines_quadratics_and_cubics_without_changing_the_shaft() {
    for preset in [Preset::Forward, Preset::Curved, Preset::Fishhook] {
        let mut arrow = Arrow::new(
            7,
            Point::default(),
            Point::new(120., 0.),
            preset,
            ArrowStyle::preset(preset),
        );
        if preset == Preset::Fishhook {
            arrow.cubic = Some([Point::new(15., -70.), Point::new(100., 55.)]);
        }
        let before = arrow.clone();
        let probe = before.point(0.37);
        let (split, projected, distance) = before.nearest_shaft_point(probe);
        near(projected, probe);
        assert!(distance < 0.001);
        let inserted = arrow.insert_node(probe).unwrap();
        assert_eq!(inserted, handle(0, NodePart::Node));
        assert_eq!(arrow.nodes.len(), 1);
        near(arrow.nodes[0].point, projected);
        assert_eq!(arrow.control, None);
        for i in 0..=40 {
            let t = i as f32 / 40.;
            let remapped = if t <= split {
                0.5 * t / split
            } else {
                0.5 + 0.5 * (t - split) / (1. - split)
            };
            near(arrow.point(remapped), before.point(t));
        }
        assert_eq!(arrow.appearance(), before.appearance());
        arrow.validate().unwrap();
    }
    let mut arrow = joint();
    arrow.nodes[0].controls = [Point::new(40., 0.), Point::new(80., 0.)];
    arrow.nodes[0].kind = Symmetric;
    arrow.insert_node(arrow.point(0.2)).unwrap();
    let existing = arrow.nodes[1];
    assert_eq!(existing.kind, Smooth);
    near(
        super::unit(existing.controls[0], existing.point),
        super::unit(existing.point, existing.controls[1]),
    );
    assert!(
        (existing.controls[0].distance(existing.point)
            - existing.controls[1].distance(existing.point))
        .abs()
            > 1.
    );
}

#[test]
fn shaft_pieces_keep_exact_corner_controls_in_both_directions_and_when_trimmed() {
    use PathCommand::{Cubic, Move};
    let arrow = joint();
    assert_eq!(
        arrow.shaft_body(Point::new(0., 1.), 0., 1., 0.),
        vec![
            Move(arrow.start),
            Cubic(
                Point::new(20., -30.),
                Point::new(40., 0.),
                Point::new(60., 0.)
            ),
            Cubic(Point::new(60., 35.), Point::new(100., 40.), arrow.end),
        ]
    );
    assert_eq!(
        arrow.shaft_body(Point::new(0., 1.), 0.25, 0.75, 0.),
        vec![
            Move(Point::new(30., -11.25)),
            Cubic(
                Point::new(40., -7.5),
                Point::new(50., 0.),
                Point::new(60., 0.)
            ),
            Cubic(
                Point::new(60., 17.5),
                Point::new(70., 27.5),
                Point::new(82.5, 28.125)
            ),
        ]
    );
    assert_eq!(
        arrow.shaft_body(Point::new(0., 1.), 0.75, 0.25, 0.),
        vec![
            Move(Point::new(82.5, 28.125)),
            Cubic(
                Point::new(70., 27.5),
                Point::new(60., 17.5),
                Point::new(60., 0.)
            ),
            Cubic(
                Point::new(50., 0.),
                Point::new(40., -7.5),
                Point::new(30., -11.25)
            ),
        ]
    );
}

#[test]
fn offset_shafts_share_the_smooth_join_and_remain_one_finite_path() {
    let mut arrow = joint();
    arrow.nodes[0].controls = [Point::new(40., 0.), Point::new(80., 0.)];
    arrow.nodes[0].kind = Symmetric;
    for (from, to) in [(0., 1.), (1., 0.)] {
        for offset in [-3., 3.] {
            let commands = arrow.shaft_body(Point::new(0., 1.), from, to, offset);
            assert!(matches!(commands[0], PathCommand::Move(_)));
            assert!(
                !commands[1..]
                    .iter()
                    .any(|c| matches!(c, PathCommand::Move(_)))
            );
            assert!(
                commands
                    .iter()
                    .flat_map(PathCommand::points)
                    .all(Point::is_finite)
            );
            assert!(commands.iter().any(|command| match command {
                PathCommand::Cubic(_, _, end) => end.distance(Point::new(60., offset)) < 0.001,
                _ => false,
            }));
        }
    }
}

#[test]
fn fractional_segment_boundaries_keep_exact_nodes_and_the_correct_side_tangent() {
    for segments in [13, 22] {
        let mut arrow = joint();
        arrow.end = Point::new(segments as f32 * 30., 0.);
        arrow.cubic = Some([Point::new(15., 10.), arrow.end.offset(-15., 10.)]);
        arrow.nodes = (1..segments)
            .map(|i| {
                let point = Point::new(i as f32 * 30., if i % 2 == 0 { 50. } else { -50. });
                ArrowNode {
                    point,
                    controls: [point.offset(-8., 0.), point.offset(0., 9.)],
                    kind: Corner,
                }
            })
            .collect();
        let commands = arrow.shaft_body(Point::new(0., 1.), 0., 1., 0.);
        assert_eq!(commands.len(), segments + 1);
        for (i, node) in arrow.nodes.iter().enumerate() {
            let t = (i + 1) as f32 / segments as f32;
            assert_eq!(arrow.point(t), node.point, "boundary {i} of {segments}");
            near(
                arrow.shaft_tangent(t, false),
                super::unit(node.controls[0], node.point),
            );
            near(
                arrow.shaft_tangent(t, true),
                super::unit(node.controls[1], node.point),
            );
            let PathCommand::Cubic(_, _, end) = commands[i + 1] else {
                panic!("Cubic segment")
            };
            assert_eq!(end, node.point, "shaft boundary {i} of {segments}");
        }
        assert_eq!(commands.last().unwrap().points().last(), Some(&arrow.end));
    }
}

#[test]
fn unequal_equilibrium_reverse_head_uses_the_outgoing_side_of_a_corner() {
    let mut arrow = joint();
    arrow.kind = Preset::Equilibrium.kind().into();
    // Four straight 50-unit sides, so the reverse shaft spans nodes 0 to 2.
    let corners = [
        Point::new(0., 0.),
        Point::new(50., 0.),
        Point::new(50., 50.),
        Point::new(100., 50.),
        Point::new(100., 100.),
    ];
    arrow.end = corners[4];
    arrow.cubic = Some([
        corners[0].lerp(corners[1], 1. / 3.),
        corners[4].lerp(corners[3], 1. / 3.),
    ]);
    arrow.nodes = corners
        .windows(3)
        .map(|w| ArrowNode {
            point: w[1],
            controls: [w[1].lerp(w[0], 1. / 3.), w[1].lerp(w[2], 1. / 3.)],
            kind: Corner,
        })
        .collect();
    let style = arrow.style.as_mut().unwrap();
    style.head = Head::Full;
    style.tail = Head::Full;
    style.shape = HeadShape::Open;
    style.equilibrium_ratio = 0.5;
    let style = arrow.appearance();
    let gap = crate::style::DEFAULT.world(style.gap_pt) * 0.5;
    let tip = arrow.nodes[0].point.offset(-gap, 0.);
    let paths = arrow.paths();
    let [
        PathCommand::Move(a),
        PathCommand::Line(actual_tip),
        PathCommand::Line(b),
    ] = paths[3].commands.as_slice()
    else {
        panic!("Open reverse head")
    };
    near(*actual_tip, tip);
    near(
        a.lerp(*b, 0.5),
        tip.offset(0., crate::style::DEFAULT.world(style.head_length_pt)),
    );
    near(
        *paths[2].commands.last().unwrap().points().last().unwrap(),
        tip,
    );
}

#[test]
fn looping_shaft_keeps_a_full_size_head_when_its_endpoints_are_close() {
    let mut arrow = joint();
    arrow.end = Point::new(1., 0.);
    arrow.cubic = Some([Point::new(30., 0.), Point::new(1., 30.)]);
    arrow.nodes = vec![
        ArrowNode {
            point: Point::new(100., 0.),
            controls: [Point::new(70., 0.), Point::new(100., 30.)],
            kind: Corner,
        },
        ArrowNode {
            point: Point::new(100., 100.),
            controls: [Point::new(100., 70.), Point::new(70., 100.)],
            kind: Corner,
        },
        ArrowNode {
            point: Point::new(0., 100.),
            controls: [Point::new(30., 100.), Point::new(0., 70.)],
            kind: Corner,
        },
    ];
    for shape in [HeadShape::Open, HeadShape::Solid, HeadShape::Hollow] {
        arrow.style.as_mut().unwrap().shape = shape;
        let paths = arrow.paths();
        let points = &paths[1].commands;
        let (tip, a, b) = if shape == HeadShape::Open {
            (
                points[1].points()[0],
                points[0].points()[0],
                points[2].points()[0],
            )
        } else {
            (
                points[0].points()[0],
                points[1].points()[0],
                points[3].points()[0],
            )
        };
        near(tip, arrow.end);
        let length = tip.distance(a.lerp(b, 0.5));
        assert!(
            (length - crate::style::DEFAULT.world(arrow.appearance().head_length_pt)).abs() < 0.001
        );
        assert!(length > arrow.start.distance(arrow.end));
    }
}

#[test]
fn adding_a_node_keeps_hollow_heads_and_both_shaft_ends_in_place() {
    let arrow = Arrow::new(
        1,
        Point::default(),
        Point::new(120., 0.),
        Preset::Forward,
        ArrowStyle {
            shape: HeadShape::Hollow,
            tail: Head::Full,
            ..ArrowStyle::default()
        },
    );
    let before = arrow.paths();
    for x in [10., 60., 100., 104., 112.] {
        let mut split = arrow.clone();
        split.insert_node(Point::new(x, 0.)).unwrap();
        let after = split.paths();
        assert_eq!(after.len(), before.len());
        for (actual, expected) in path_ends(&after[0].commands)
            .into_iter()
            .zip(path_ends(&before[0].commands))
        {
            near(actual, expected);
        }
        for (actual, expected) in after[1..].iter().zip(&before[1..]) {
            let actual: Vec<_> = actual
                .commands
                .iter()
                .flat_map(PathCommand::points)
                .collect();
            let expected: Vec<_> = expected
                .commands
                .iter()
                .flat_map(PathCommand::points)
                .collect();
            assert_eq!(actual.len(), expected.len());
            for (actual, expected) in actual.into_iter().zip(expected) {
                near(actual, expected);
            }
        }
    }
}

#[test]
fn adding_a_node_keeps_both_retro_shafts_in_place() {
    let arrow = Arrow::new(
        1,
        Point::default(),
        Point::new(120., 0.),
        Preset::Retro,
        ArrowStyle::preset(Preset::Retro),
    );
    let before = arrow.paths();
    for x in [24., 60., 100.] {
        let mut split = arrow.clone();
        split.insert_node(Point::new(x, 0.)).unwrap();
        let after = split.paths();
        for index in [0, 1] {
            for (actual, expected) in path_ends(&after[index].commands)
                .into_iter()
                .zip(path_ends(&before[index].commands))
            {
                near(actual, expected);
            }
        }
    }
}

#[test]
fn node_handles_have_stable_ids_and_replace_the_bend_handle() {
    let arrow = joint();
    assert_eq!(FIRST_NODE_HANDLE, 5);
    assert!(!arrow.has_bend_handle());
    assert_eq!(arrow.handle_point(2), None);
    assert_eq!(arrow.handle_point(0), Some(arrow.start));
    assert_eq!(arrow.handle_point(1), Some(arrow.end));
    assert_eq!(arrow.handle_point(3), Some(arrow.cubic.unwrap()[0]));
    assert_eq!(arrow.handle_point(4), Some(arrow.cubic.unwrap()[1]));
    let handles = arrow.node_handles();
    assert_eq!(handles.len(), 3);
    for (part, expected) in [
        (NodePart::Node, arrow.nodes[0].point),
        (NodePart::Arriving, arrow.nodes[0].controls[0]),
        (NodePart::Leaving, arrow.nodes[0].controls[1]),
    ] {
        let index = handle(0, part);
        assert_eq!(node_handle(index), Some((0, part)));
        assert!(handles.contains(&(index, expected, part)));
        assert_eq!(arrow.handle_point(index), Some(expected));
        assert_eq!(arrow.node_type(index), Some(Corner));
        assert_eq!(arrow.handle_at(expected, 0.1), Some(index));
    }
    assert_eq!(arrow.handle_point(usize::MAX), None);
    assert_eq!(arrow.node_type(usize::MAX), None);
    assert_eq!(node_handle(4), None);
    assert_eq!(
        node_handle(handle(2, NodePart::Leaving)),
        Some((2, NodePart::Leaving))
    );
}

#[test]
fn moving_nodes_and_endpoints_transports_only_the_adjacent_controls() {
    let mut arrow = joint();
    let original = arrow.clone();
    arrow.edit_handle(handle(0, NodePart::Node), Point::new(65., -7.));
    assert_eq!(arrow.nodes[0].point, Point::new(65., -7.));
    assert_eq!(
        arrow.nodes[0].controls,
        [Point::new(45., -7.), Point::new(65., 28.)]
    );
    assert_eq!(arrow.cubic, original.cubic);
    assert_eq!((arrow.start, arrow.end), (original.start, original.end));
    arrow.edit_handle(0, Point::new(-10., 20.));
    assert_eq!(
        arrow.cubic.unwrap(),
        [Point::new(10., -10.), Point::new(100., 40.)]
    );
    arrow.edit_handle(1, Point::new(130., -10.));
    assert_eq!(
        arrow.cubic.unwrap(),
        [Point::new(10., -10.), Point::new(110., 30.)]
    );
    assert_eq!(arrow.nodes[0].point, Point::new(65., -7.));
    let before = arrow.clone();
    arrow.edit_handle(handle(0, NodePart::Leaving), Point::new(f32::NAN, 0.));
    arrow.edit_handle(usize::MAX, Point::default());
    assert_eq!(arrow, before);
}

#[test]
fn smooth_and_symmetric_drags_constrain_the_opposite_control_and_free_drag_breaks_it() {
    for kind in [Corner, Smooth, Symmetric] {
        for selected in [NodePart::Arriving, NodePart::Leaving] {
            let mut arrow = joint();
            arrow.nodes[0].controls = [Point::new(50., 0.), Point::new(80., 0.)];
            arrow
                .set_node_type(handle(0, NodePart::Node), kind)
                .unwrap();
            let before = arrow.nodes[0];
            if kind == Symmetric {
                assert_eq!(before.controls, [Point::new(45., 0.), Point::new(75., 0.)]);
            }
            let (selected_slot, opposite_slot) = if selected == NodePart::Arriving {
                (0, 1)
            } else {
                (1, 0)
            };
            let opposite_length = before.controls[opposite_slot].distance(before.point);
            let destination = before.point.offset(0., 30.);
            arrow.edit_handle(handle(0, selected), destination);
            assert_eq!(arrow.nodes[0].controls[selected_slot], destination);
            near(
                arrow.nodes[0].controls[opposite_slot],
                match kind {
                    Corner => before.controls[opposite_slot],
                    Smooth => before.point.offset(0., -opposite_length),
                    Symmetric => before.point.offset(0., -30.),
                },
            );
            assert_eq!(arrow.node_type(handle(0, selected)), Some(kind));
            let before = arrow.nodes[0];
            arrow.edit_handle_free(handle(0, selected), before.point.offset(25., 0.));
            assert_eq!(
                arrow.nodes[0].controls[opposite_slot],
                before.controls[opposite_slot]
            );
            assert_eq!(arrow.nodes[0].kind, Corner);
        }
    }
}

#[test]
fn transforms_reverse_and_flip_include_every_node_and_its_controls() {
    let mut original = joint();
    original.nodes.push(ArrowNode {
        point: Point::new(90., 40.),
        controls: [Point::new(80., 50.), Point::new(100., 30.)],
        kind: Symmetric,
    });
    let mut arrow = original.clone();
    arrow.map_points(|p| Point::new(10. - 2. * p.y, -7. + 2. * p.x));
    for i in 0..=20 {
        let t = i as f32 / 20.;
        let p = original.point(t);
        near(arrow.point(t), Point::new(10. - 2. * p.y, -7. + 2. * p.x));
    }
    let mut arrow = original.clone();
    arrow.reverse();
    for (actual, before) in arrow.nodes.iter().zip(original.nodes.iter().rev()) {
        assert_eq!(actual.point, before.point);
        assert_eq!(actual.controls, [before.controls[1], before.controls[0]]);
        assert_eq!(actual.kind, before.kind);
    }
    for i in 0..=20 {
        let t = i as f32 / 20.;
        near(arrow.point(t), original.point(1. - t));
    }
    arrow.reverse();
    assert_eq!(arrow, original);
    arrow.flip_bend();
    for i in 0..=20 {
        let t = i as f32 / 20.;
        let p = original.point(t);
        near(arrow.point(t), Point::new(p.x, -p.y));
    }
}

#[test]
fn invalid_node_operations_and_bow_cleanup_leave_the_arrow_unchanged() {
    let mut arrow = joint();
    let before = arrow.clone();
    assert!(arrow.delete_node(usize::MAX).is_err());
    assert!(arrow.set_node_type(usize::MAX, Smooth).is_err());
    assert!(arrow.insert_node(Point::new(f32::NAN, 0.)).is_err());
    assert!(!arrow.smooth_bow().unwrap());
    assert_eq!(
        arrow.symmetrise().unwrap_err(),
        "Not symmetric enough: arrow nodes are too asymmetric"
    );
    assert_eq!(arrow, before);
    arrow.delete_node(handle(0, NodePart::Node)).unwrap();
    assert!(arrow.nodes.is_empty());
    assert_eq!(
        (arrow.start, arrow.end, arrow.cubic),
        (before.start, before.end, before.cubic)
    );
    arrow.validate().unwrap();
}

#[test]
fn smoothing_aligns_gentle_joints_and_removes_redundant_split_nodes() {
    let mut arrow = joint();
    arrow.start = Point::new(-100., -50.);
    arrow.end = Point::new(100., 50.);
    arrow.cubic = Some([Point::new(-80., -50.), Point::new(80., 50.)]);
    let angle = 10_f32.to_radians();
    arrow.nodes[0].point = Point::default();
    arrow.nodes[0].controls = [
        Point::new(-10., 0.),
        Point::new(20. * angle.cos(), 20. * angle.sin()),
    ];
    let before = arrow.clone();
    assert!(arrow.smooth_bow().unwrap());
    assert_eq!(arrow.nodes.len(), 1);
    assert_eq!(arrow.nodes[0].point, before.nodes[0].point);
    assert_eq!(arrow.nodes[0].kind, Smooth);
    let angle = 5_f32.to_radians();
    near(
        arrow.nodes[0].controls[0],
        Point::new(-10. * angle.cos(), -10. * angle.sin()),
    );
    near(
        arrow.nodes[0].controls[1],
        Point::new(20. * angle.cos(), 20. * angle.sin()),
    );
    assert_eq!(
        (arrow.start, arrow.end, arrow.cubic),
        (before.start, before.end, before.cubic)
    );
    let smoothed = arrow.clone();
    assert!(!arrow.smooth_bow().unwrap());
    assert_eq!(arrow, smoothed);

    let mut arrow = before;
    arrow.nodes.clear();
    let original = arrow.clone();
    arrow.insert_node(arrow.point(0.37)).unwrap();
    assert!(arrow.smooth_bow().unwrap());
    assert!(arrow.nodes.is_empty());
    for i in 0..=20 {
        let t = i as f32 / 20.;
        near(arrow.point(t), original.point(t));
    }
}

#[test]
fn symmetrise_pairs_odd_and_even_interior_nodes_and_is_inert_when_symmetric() {
    for even in [false, true] {
        let mut arrow = joint();
        arrow.cubic = Some([Point::new(20., -30.), Point::new(98., -28.)]);
        arrow.nodes[0] = ArrowNode {
            point: Point::new(58., 40.),
            controls: [Point::new(40., 42.), Point::new(80., 38.)],
            kind: Smooth,
        };
        if even {
            arrow.nodes = vec![
                ArrowNode {
                    point: Point::new(35., 30.),
                    controls: [Point::new(25., 20.), Point::new(55., 50.)],
                    kind: Smooth,
                },
                ArrowNode {
                    point: Point::new(85., 30.),
                    controls: [Point::new(75., 50.), Point::new(95., 10.)],
                    kind: Symmetric,
                },
            ];
        }
        let before = arrow.clone();
        assert!(arrow.symmetrise().unwrap());
        assert_eq!(arrow.nodes.len(), before.nodes.len());
        assert_eq!((arrow.start, arrow.end), (before.start, before.end));
        assert_eq!(arrow.style, before.style);
        for (node, before) in arrow.nodes.iter().zip(&before.nodes) {
            assert_eq!(node.kind, if even { Corner } else { before.kind });
        }
        for i in 0..=20 {
            let t = i as f32 / 20.;
            let opposite = arrow.point(1. - t);
            near(arrow.point(t), Point::new(120. - opposite.x, opposite.y));
        }
        let symmetric = arrow.clone();
        assert!(!arrow.symmetrise().unwrap());
        assert_eq!(arrow, symmetric);
        arrow.validate().unwrap();
    }
}

#[test]
fn adding_a_node_leaves_no_go_marks_and_unequal_equilibrium_shafts_in_place() {
    let ends = |commands: &[PathCommand]| {
        let points: Vec<_> = commands.iter().flat_map(PathCommand::iter_points).collect();
        [points[0], points[points.len() - 1]]
    };
    let no_go = Arrow::new(
        1,
        Point::default(),
        Point::new(120., 0.),
        Preset::NoGo,
        ArrowStyle::preset(Preset::NoGo),
    );
    let equilibrium = Arrow::new(
        2,
        Point::default(),
        Point::new(120., 0.),
        Preset::Equilibrium,
        ArrowStyle {
            equilibrium_ratio: 0.5,
            ..ArrowStyle::preset(Preset::Equilibrium)
        },
    );
    // The two strokes of the cross, and the shorter reverse shaft.
    for (mut arrow, decorations) in [(no_go, [2, 3]), (equilibrium, [2, 2])] {
        let before = arrow.paths();
        arrow.insert_node(Point::new(24., 0.)).unwrap();
        let after = arrow.paths();
        for index in decorations {
            for (a, b) in ends(&before[index].commands)
                .into_iter()
                .zip(ends(&after[index].commands))
            {
                assert!(a.distance(b) < 0.01, "{a:?} != {b:?}");
            }
        }
    }
}
