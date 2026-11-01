use super::*;
use crate::graphics::{GraphicKind, PathCommand, PathNodeType};

fn graphic(commands: Vec<PathCommand>) -> Graphic {
    let mut graphic = Graphic::pen_curve(
        1,
        Point::default(),
        Point::new(100., 0.),
        Default::default(),
    );
    graphic.path = commands;
    graphic.node_types.clear();
    graphic
}

fn joint(angle: f32, length: f32) -> Graphic {
    let angle = angle.to_radians();
    graphic(vec![
        PathCommand::Move(Point::new(-100., -50.)),
        PathCommand::Cubic(
            Point::new(-80., -50.),
            Point::new(-10., 0.),
            Point::default(),
        ),
        PathCommand::Cubic(
            Point::new(length * angle.cos(), length * angle.sin()),
            Point::new(80., 50.),
            Point::new(100., 50.),
        ),
    ])
}

fn near(actual: Point, expected: Point) {
    assert!(
        actual.distance(expected) < 0.001,
        "{actual:?} != {expected:?}"
    );
}

#[test]
fn gentle_wobbles_align_without_moving_anchors_or_changing_handle_lengths() {
    for angle in [5., 10., 19.] {
        let mut graphic = joint(angle, 20.);
        let before = graphic.edit_points();
        assert!(graphic.smooth_path().unwrap());
        let path = Path::read(&graphic).unwrap();
        assert_eq!(path.nodes.len(), 3);
        let [incoming, anchor, outgoing] = path.controls(1).unwrap();
        let direction = (angle * 0.5_f32).to_radians();
        near(
            incoming,
            Point::new(-10. * direction.cos(), -10. * direction.sin()),
        );
        near(
            outgoing,
            Point::new(20. * direction.cos(), 20. * direction.sin()),
        );
        assert_eq!(anchor, Point::default());
        assert_eq!(graphic.node_types[1], PathNodeType::Smooth);
        for index in [0, 1, 3, 5, 6] {
            assert_eq!(graphic.edit_points()[index], before[index]);
        }
        let smoothed = graphic.clone();
        assert!(!graphic.smooth_path().unwrap());
        assert_eq!(graphic, smoothed);
    }
}

#[test]
fn symmetric_nodes_keep_their_constraint_when_aligned() {
    let mut graphic = joint(8., 10.);
    graphic.node_types = vec![
        PathNodeType::Corner,
        PathNodeType::Symmetric,
        PathNodeType::Corner,
    ];
    assert!(graphic.smooth_path().unwrap());
    assert_eq!(graphic.node_types[1], PathNodeType::Symmetric);
    let path = Path::read(&graphic).unwrap();
    let [incoming, anchor, outgoing] = path.controls(1).unwrap();
    near(
        incoming,
        anchor.offset(anchor.x - outgoing.x, anchor.y - outgoing.y),
    );
}

#[test]
fn an_explicit_aligned_corner_is_promoted_and_an_unequal_symmetric_node_is_repaired() {
    let mut corner = joint(0., 20.);
    corner.node_types = vec![PathNodeType::Corner; 3];
    let before = corner.commands();
    assert!(corner.smooth_path().unwrap());
    assert_eq!(corner.commands(), before);
    assert_eq!(corner.node_types[1], PathNodeType::Smooth);
    assert!(!corner.smooth_path().unwrap());

    let mut symmetric = joint(0., 20.);
    symmetric.node_types = vec![
        PathNodeType::Corner,
        PathNodeType::Symmetric,
        PathNodeType::Corner,
    ];
    assert!(symmetric.smooth_path().unwrap());
    assert_eq!(symmetric.node_types[1], PathNodeType::Symmetric);
    let path = Path::read(&symmetric).unwrap();
    let [incoming, anchor, outgoing] = path.controls(1).unwrap();
    near(incoming, Point::new(-15., 0.));
    near(outgoing, Point::new(15., 0.));
    assert_eq!(anchor, Point::default());
    assert!(!symmetric.smooth_path().unwrap());
}

#[test]
fn twenty_and_forty_five_degree_corners_survive_even_a_large_line_width() {
    for mut graphic in [
        joint(20., 10.),
        joint(45., 10.),
        graphic(vec![
            PathCommand::Move(Point::default()),
            PathCommand::Line(Point::new(1., 0.)),
            PathCommand::Line(Point::new(2., 1.)),
        ]),
    ] {
        graphic.style.width_pt = 12.;
        let before = graphic.clone();
        assert!(!graphic.smooth_path().unwrap());
        assert_eq!(graphic, before);
    }
}

#[test]
fn redundant_line_nodes_are_removed_but_open_endpoints_stay() {
    let start = Point::new(10., 4.);
    let end = Point::new(90., 4.);
    let mut graphic = graphic(vec![
        PathCommand::Move(start),
        PathCommand::Line(Point::new(20., 4.)),
        PathCommand::Line(Point::new(40., 4.)),
        PathCommand::Line(Point::new(70., 4.)),
        PathCommand::Line(end),
    ]);
    assert!(graphic.smooth_path().unwrap());
    assert_eq!(
        graphic.commands(),
        vec![PathCommand::Move(start), PathCommand::Line(end)]
    );
    assert_eq!(graphic.node_types.len(), 2);
    let before = graphic.clone();
    assert!(!graphic.smooth_path().unwrap());
    assert_eq!(graphic, before);
}

#[test]
fn exact_de_casteljau_splits_recover_the_original_cubic() {
    let [start, a, b, end] = [
        Point::default(),
        Point::new(30., 60.),
        Point::new(70., -30.),
        Point::new(100., 20.),
    ];
    for t in [0.17, 0.37, 0.5, 0.83] {
        let ab = a.lerp(b, t);
        let first = start.lerp(a, t);
        let last = b.lerp(end, t);
        let incoming = first.lerp(ab, t);
        let outgoing = ab.lerp(last, t);
        let anchor = incoming.lerp(outgoing, t);
        let mut graphic = graphic(vec![
            PathCommand::Move(start),
            PathCommand::Cubic(first, incoming, anchor),
            PathCommand::Cubic(outgoing, last, end),
        ]);
        assert!(graphic.smooth_path().unwrap());
        assert_eq!(graphic.path_summary().unwrap().node_count(), 2);
        let commands = graphic.commands();
        let PathCommand::Cubic(actual_a, actual_b, actual_end) = commands[1] else {
            panic!("merged cubic");
        };
        near(actual_a, a);
        near(actual_b, b);
        assert_eq!(actual_end, end);
        let merged = Curve {
            start,
            controls: Some([actual_a, actual_b]),
            end,
        };
        let original = Curve {
            start,
            controls: Some([a, b]),
            end,
        };
        for sample in 0..=100 {
            let t = f64::from(sample) / 100.;
            near(merged.point(t), original.point(t));
        }
    }
}

#[test]
fn removing_a_split_keeps_the_tangent_and_bend_at_the_retained_neighbor() {
    let start = Point::default();
    let a = Point::new(30., 50.);
    let b = Point::new(70., 0.);
    let end = Point::new(100., 0.);
    let first = start.lerp(a, 0.5);
    let middle = a.lerp(b, 0.5);
    let last = b.lerp(end, 0.5);
    let incoming = first.lerp(middle, 0.5);
    let outgoing = middle.lerp(last, 0.5);
    let anchor = incoming.lerp(outgoing, 0.5);
    for angle in [0_f32, 45.] {
        let radians = angle.to_radians();
        let next = end.offset(30. * radians.cos(), 30. * radians.sin());
        let mut graphic = graphic(vec![
            PathCommand::Move(start),
            PathCommand::Cubic(first, incoming, anchor),
            PathCommand::Cubic(outgoing, last, end),
            PathCommand::Cubic(next, Point::new(170., 100.), Point::new(200., 0.)),
        ]);
        assert!(graphic.smooth_path().unwrap());
        let path = Path::read(&graphic).unwrap();
        assert_eq!(path.nodes.len(), 3);
        let [incoming, retained, outgoing] = path.controls(1).unwrap();
        assert_eq!(retained, end);
        near(incoming, b);
        assert_eq!(outgoing, next);
        let (incoming, _) = unit(vector(incoming, retained)).unwrap();
        let (outgoing, _) = unit(vector(retained, outgoing)).unwrap();
        let bend = (incoming.0 * outgoing.1 - incoming.1 * outgoing.0)
            .abs()
            .atan2(incoming.0 * outgoing.0 + incoming.1 * outgoing.1)
            .to_degrees();
        assert!((bend - f64::from(angle)).abs() < 0.0001);
        assert_eq!(
            graphic.node_types[1],
            if angle == 0. {
                PathNodeType::Smooth
            } else {
                PathNodeType::Corner
            }
        );
    }
}

#[test]
fn a_real_curve_change_survives_an_aligned_joint_without_materializing_metadata() {
    let mut graphic = graphic(vec![
        PathCommand::Move(Point::new(-100., 0.)),
        PathCommand::Cubic(
            Point::new(-70., 80.),
            Point::new(-10., 0.),
            Point::default(),
        ),
        PathCommand::Cubic(
            Point::new(10., 0.),
            Point::new(70., -80.),
            Point::new(100., 0.),
        ),
    ]);
    let before = graphic.clone();
    assert!(!graphic.smooth_path().unwrap());
    assert_eq!(graphic, before);
    assert!(graphic.node_types.is_empty());
}

#[test]
fn mixed_line_and_cubic_segments_are_preserved() {
    let mut graphic = graphic(vec![
        PathCommand::Move(Point::default()),
        PathCommand::Line(Point::new(10., 0.)),
        PathCommand::Cubic(
            Point::new(20., 0.),
            Point::new(30., 0.),
            Point::new(40., 0.),
        ),
    ]);
    let before = graphic.clone();
    assert!(!graphic.smooth_path().unwrap());
    assert_eq!(graphic, before);
}

#[test]
fn gentle_mixed_joints_follow_the_line_and_keep_handle_length_and_node_type() {
    for line_first in [true, false] {
        for (height, changes) in [
            (1., true),
            (3., true),
            (10. * 20_f32.to_radians().tan(), false),
            (10., false),
        ] {
            let start = Point::default();
            let anchor = Point::new(50., 0.);
            let end = Point::new(100., 30.);
            let commands = if line_first {
                vec![
                    PathCommand::Move(start),
                    PathCommand::Line(anchor),
                    PathCommand::Cubic(Point::new(60., height), Point::new(80., 30.), end),
                ]
            } else {
                vec![
                    PathCommand::Move(end),
                    PathCommand::Cubic(Point::new(80., 30.), Point::new(60., height), anchor),
                    PathCommand::Line(start),
                ]
            };
            let mut graphic = graphic(commands);
            let before = graphic.clone();
            let types = graphic.path_node_types().unwrap();
            assert_eq!(graphic.smooth_path().unwrap(), changes);
            if changes {
                let mut expected_points = before.edit_points();
                expected_points[2] = Point::new(50. + 10_f32.hypot(height), 0.);
                for (actual, expected) in graphic.edit_points().into_iter().zip(expected_points) {
                    near(actual, expected);
                }
                assert_eq!(graphic.path_node_types().unwrap(), types);
            } else {
                assert_eq!(graphic, before);
            }
            let smoothed = graphic.clone();
            assert!(!graphic.smooth_path().unwrap());
            assert_eq!(graphic, smoothed);
        }
    }
}

#[test]
fn geometrically_redundant_cubic_node_is_removed_despite_different_parameter_speeds() {
    let mut graphic = graphic(vec![
        PathCommand::Move(Point::default()),
        PathCommand::Cubic(Point::new(1., 0.), Point::new(9., 0.), Point::new(10., 0.)),
        PathCommand::Cubic(
            Point::new(11., 0.),
            Point::new(19., 0.),
            Point::new(20., 0.),
        ),
    ]);
    assert!(graphic.smooth_path().unwrap());
    assert_eq!(
        graphic.commands(),
        vec![
            PathCommand::Move(Point::default()),
            PathCommand::Cubic(Point::new(2., 0.), Point::new(18., 0.), Point::new(20., 0.)),
        ]
    );
    assert!(!graphic.smooth_path().unwrap());
}

#[test]
fn geometric_deviation_checks_excursions_in_both_directions() {
    let direct = [Point::default(), Point::new(20., 0.)];
    let detour = [Point::default(), Point::new(10., 5.), Point::new(20., 0.)];
    // Both direct endpoints lie on the detour, but its interior leaves the line.
    assert_eq!(deviation(&direct, &detour), 5.);
    assert_eq!(deviation(&detour, &direct), 5.);
    assert_eq!(deviation(&[Point::default(); 2], &direct), 20.);
}

#[test]
fn a_closed_cubic_seam_is_aligned_as_one_node() {
    let angle = 8_f32.to_radians();
    let mut graphic = graphic(vec![
        PathCommand::Move(Point::default()),
        PathCommand::Cubic(
            Point::new(10. * angle.cos(), 10. * angle.sin()),
            Point::new(100., -10.),
            Point::new(100., 0.),
        ),
        PathCommand::Cubic(
            Point::new(100., 10.),
            Point::new(10., 100.),
            Point::new(0., 100.),
        ),
        PathCommand::Cubic(
            Point::new(-10., 100.),
            Point::new(-10., 0.),
            Point::default(),
        ),
        PathCommand::Close,
    ]);
    assert!(graphic.smooth_path().unwrap());
    let path = Path::read(&graphic).unwrap();
    assert!(path.closed);
    assert_eq!(path.nodes.len(), 3);
    assert_eq!(graphic.node_types[0], PathNodeType::Smooth);
    let [incoming, anchor, outgoing] = path.controls(0).unwrap();
    near(incoming, anchor.offset(-outgoing.x, -outgoing.y));
    assert_eq!(graphic.commands().last(), Some(&PathCommand::Close));
}

#[test]
fn redundant_closed_seam_is_removed_without_opening_the_path() {
    let mut graphic = graphic(vec![
        PathCommand::Move(Point::new(50., 0.)),
        PathCommand::Line(Point::new(100., 0.)),
        PathCommand::Line(Point::new(100., 100.)),
        PathCommand::Line(Point::new(0., 100.)),
        PathCommand::Line(Point::default()),
        PathCommand::Close,
    ]);
    assert!(graphic.smooth_path().unwrap());
    let path = Path::read(&graphic).unwrap();
    assert!(path.closed);
    assert_eq!(path.nodes.len(), 4);
    assert_eq!(path.segments.len(), 4);
    assert_eq!(path.nodes[0].point, Point::new(100., 0.));
    assert_eq!(graphic.node_types.len(), 4);
    assert!(!graphic.smooth_path().unwrap());
}

#[test]
fn repeated_removals_stay_within_a_quarter_width_of_the_original_path() {
    let points: Vec<_> = (0..=10)
        .map(|i| Point::new(i as f32 * 10., 0.03 * (i as f32 - 5.).powi(2)))
        .collect();
    let mut graphic = graphic(
        std::iter::once(PathCommand::Move(points[0]))
            .chain(points.iter().skip(1).copied().map(PathCommand::Line))
            .collect(),
    );
    let tolerance = graphic.style.width() * 0.25;
    assert!(graphic.smooth_path().unwrap());
    let path = Path::read(&graphic).unwrap();
    assert!(path.nodes.len() < points.len());
    assert!(
        path.nodes.len() > 2,
        "merging only the latest geometry would drift"
    );
    assert_eq!(path.nodes.first().unwrap().point, points[0]);
    assert_eq!(path.nodes.last().unwrap().point, *points.last().unwrap());
    for pair in points.windows(2) {
        for sample in 0..=100 {
            let point = pair[0].lerp(pair[1], sample as f32 / 100.);
            let distance = path
                .nodes
                .windows(2)
                .map(|segment| {
                    let a = segment[0].point;
                    let b = segment[1].point;
                    let ab = vector(a, b);
                    let ap = vector(a, point);
                    let t =
                        ((ap.0 * ab.0 + ap.1 * ab.1) / (ab.0 * ab.0 + ab.1 * ab.1)).clamp(0., 1.);
                    let projected = along(a, ab, t);
                    point.distance(projected)
                })
                .fold(f32::INFINITY, f32::min);
            assert!(distance <= tolerance + 0.0001, "{distance} > {tolerance}");
        }
    }
}

#[test]
fn smoothing_a_projected_path_preserves_its_frame_and_depth() {
    let mut graphic = graphic(vec![
        PathCommand::Move(Point::default()),
        PathCommand::Line(Point::new(1., 0.)),
        PathCommand::Line(Point::new(2., 0.)),
    ]);
    graphic.origin = Point::new(17., -9.);
    graphic.axis_x = Point::new(4., 1.5);
    graphic.axis_y = Point::new(-2., 3.);
    graphic.depth = [2., 3., 4.];
    let before = graphic.clone();
    assert!(graphic.smooth_path().unwrap());
    assert_eq!(graphic.origin, before.origin);
    assert_eq!(graphic.axis_x, before.axis_x);
    assert_eq!(graphic.axis_y, before.axis_y);
    assert_eq!(graphic.depth, before.depth);
    assert_eq!(graphic.path_summary().unwrap().node_count(), 2);
    assert_eq!(graphic.commands().first(), before.commands().first());
    assert_eq!(graphic.commands().last(), before.commands().last());
}

#[test]
fn parametric_curves_are_accepted_without_rewriting_an_unchanged_curve() {
    let mut graphic = Graphic::dragged(
        1,
        GraphicKind::Curve,
        Point::new(17., -9.),
        Point::new(100., 80.),
        Default::default(),
        Default::default(),
        false,
    );
    let before = graphic.clone();
    assert!(!graphic.smooth_path().unwrap());
    assert_eq!(graphic, before);
}

#[test]
fn unsupported_paths_and_failed_storage_leave_the_graphic_unchanged() {
    let mut compound = graphic(vec![
        PathCommand::Move(Point::default()),
        PathCommand::Line(Point::new(1., 0.)),
        PathCommand::Move(Point::new(2., 0.)),
        PathCommand::Line(Point::new(3., 0.)),
    ]);
    let before = compound.clone();
    assert_eq!(
        compound.smooth_path().unwrap_err(),
        "Select one continuous path"
    );
    assert_eq!(compound, before);

    let mut projected = graphic(vec![
        PathCommand::Move(Point::default()),
        PathCommand::Line(Point::new(1., 0.)),
        PathCommand::Line(Point::new(2., 0.)),
    ]);
    projected.axis_y = Point::default();
    projected.depth = [0., 0., 1.];
    let before = projected.clone();
    assert!(projected.smooth_path().unwrap_err().contains("edge-on"));
    assert_eq!(projected, before);
}
