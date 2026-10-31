use super::*;
use crate::graphics::{GraphicKind, PathCommand, PathNodeType};

fn path(commands: Vec<PathCommand>) -> Graphic {
    let mut graphic =
        Graphic::pen_curve(1, Point::default(), Point::new(1., 1.), Default::default());
    graphic.path = commands;
    graphic.node_types.clear();
    graphic
}

fn lobe() -> Graphic {
    use PathCommand::*;
    let p = Point::new;
    path(vec![
        Move(p(0., -6.)),
        Cubic(p(1., -5.), p(4., -2.), p(4., 1.)),
        Cubic(p(4., 4.), p(2., 7.), p(0., 7.)),
        Cubic(p(-2., 7.), p(-4., 4.), p(-4., 1.)),
        Cubic(p(-4., -2.), p(-1., -5.), p(0., -6.)),
        Close,
    ])
}

fn bracket() -> Graphic {
    use PathCommand::*;
    let p = Point::new;
    path(vec![
        Move(p(4., -5.)),
        Cubic(p(1., -5.), p(-1., -5.), p(-1., -4.)),
        Line(p(-2., 0.)),
        Line(p(-1., 4.)),
        Cubic(p(-1., 5.), p(1., 5.), p(4., 5.)),
    ])
}

fn rotate(graphic: &mut Graphic, angle: f64) {
    let (sine, cosine) = angle.sin_cos();
    graphic.path = graphic
        .path
        .iter()
        .map(|command| {
            command.map(|point| {
                Point::new(
                    (f64::from(point.x) * cosine - f64::from(point.y) * sine + 2.) as f32,
                    (f64::from(point.x) * sine + f64::from(point.y) * cosine - 1.) as f32,
                )
            })
        })
        .collect();
}

fn near(a: Point, b: Point) {
    let magnitude = a.x.abs().max(a.y.abs()).max(b.x.abs()).max(b.y.abs());
    assert!(
        a.distance(b) <= 1e-6 + magnitude * f32::EPSILON * 4.,
        "{a:?} != {b:?}"
    );
}

fn reflected(point: Point, axis_a: Point, axis_b: Point) -> (f64, f64) {
    let (dx, dy) = (
        f64::from(axis_b.x) - f64::from(axis_a.x),
        f64::from(axis_b.y) - f64::from(axis_a.y),
    );
    let length = dx.hypot(dy);
    let (dx, dy) = (dx / length, dy / length);
    let (x, y) = (
        f64::from(point.x) - f64::from(axis_a.x),
        f64::from(point.y) - f64::from(axis_a.y),
    );
    let along = x * dx + y * dy;
    (
        f64::from(axis_a.x) + 2. * along * dx - x,
        f64::from(axis_a.y) + 2. * along * dy - y,
    )
}

fn assert_reflection(graphic: &Graphic, offset: usize, axis_a: Point, axis_b: Point) -> f64 {
    let parsed = Path::read(graphic).unwrap();
    let count = parsed.nodes.len();
    let partner = |node| {
        if parsed.closed {
            (offset + count - node) % count
        } else {
            count - 1 - node
        }
    };
    let mut maximum: f64 = 0.;
    let mut check = |a, b: Point| {
        let (x, y) = reflected(a, axis_a, axis_b);
        near(Point::new(x as f32, y as f32), b);
        maximum = maximum.max((x - f64::from(b.x)).hypot(y - f64::from(b.y)));
    };
    for (node, item) in parsed.nodes.iter().enumerate() {
        check(item.point, parsed.nodes[partner(node)].point);
    }
    for (segment, item) in parsed.segments.iter().enumerate() {
        let other = &parsed.segments[partner((segment + 1) % count)];
        match (item.controls, other.controls) {
            (Some([a, b]), Some([c, d])) => {
                check(a, d);
                check(b, c);
            }
            (None, None) => {}
            _ => panic!("reflection changed line/cubic topology"),
        }
    }
    maximum
}

#[test]
fn tilted_closed_lobe_becomes_symmetric_about_its_fitted_axis_and_is_idempotent() {
    let mut graphic = lobe();
    if let PathCommand::Cubic(a, b, end) = &mut graphic.path[1] {
        a.x += 0.12;
        b.y -= 0.08;
        end.x += 0.22;
    }
    rotate(&mut graphic, 0.47);
    assert_eq!(graphic.symmetrise_path(false), Ok(true));
    let parsed = Path::read(&graphic).unwrap();
    let (a, b) = (parsed.nodes[0].point, parsed.nodes[2].point);
    assert!((a.x - b.x).abs() > 1., "the fitted axis stays tilted");
    assert!(assert_reflection(&graphic, 0, a, b) <= 1e-6);
    let stored = graphic.clone();
    assert_eq!(graphic.symmetrise_path(false), Ok(false));
    assert_eq!(graphic, stored);
}

#[test]
fn tilted_open_bracket_pairs_nodes_and_reverses_its_mixed_segments() {
    let mut graphic = bracket();
    if let PathCommand::Cubic(a, b, _) = &mut graphic.path[1] {
        a.x += 0.15;
        b.y -= 0.08;
    }
    rotate(&mut graphic, -0.38);
    let kinds: Vec<_> = Path::read(&graphic)
        .unwrap()
        .segments
        .iter()
        .map(|segment| segment.controls.is_some())
        .collect();
    assert_eq!(graphic.symmetrise_path(false), Ok(true));
    let parsed = Path::read(&graphic).unwrap();
    let a = parsed.nodes[2].point;
    let midpoint = parsed.nodes[0].point.lerp(parsed.nodes[4].point, 0.5);
    assert!(assert_reflection(&graphic, 0, a, midpoint) <= 1e-6);
    assert_eq!(
        parsed
            .segments
            .iter()
            .map(|segment| segment.controls.is_some())
            .collect::<Vec<_>>(),
        kinds
    );
    assert_eq!(graphic.symmetrise_path(false), Ok(false));
}

#[test]
fn upright_uses_the_nearest_world_axis_and_keeps_the_sample_centre() {
    let mut graphic = lobe();
    rotate(&mut graphic, 0.04);
    let centre = Samples::read(&Path::read(&graphic).unwrap())
        .unwrap()
        .centre;
    assert_eq!(graphic.symmetrise_path(true), Ok(true));
    let axis = Point::new(centre.0 as f32, centre.1 as f32);
    assert_reflection(&graphic, 0, axis, axis.offset(0., 1.));
    let after = Samples::read(&Path::read(&graphic).unwrap())
        .unwrap()
        .centre;
    assert!((after.0 - centre.0).abs() < 1e-6);
    assert!((after.1 - centre.1).abs() < 1e-6);
    let stored = graphic.clone();
    assert_eq!(graphic.symmetrise_path(true), Ok(false));
    assert_eq!(graphic, stored);
}

#[test]
fn upright_also_supports_a_nearly_horizontal_axis() {
    let mut graphic = bracket();
    rotate(&mut graphic, -0.035);
    let centre = Samples::read(&Path::read(&graphic).unwrap())
        .unwrap()
        .centre;
    assert_eq!(graphic.symmetrise_path(true), Ok(true));
    let axis = Point::new(centre.0 as f32, centre.1 as f32);
    assert_reflection(&graphic, 0, axis, axis.offset(1., 0.));
}

#[test]
fn asymmetric_and_far_tilted_upright_requests_refuse_without_writing() {
    let mut graphic = path(vec![
        PathCommand::Move(Point::new(0., 0.)),
        PathCommand::Line(Point::new(12., 0.)),
        PathCommand::Line(Point::new(3., 4.)),
    ]);
    let before = graphic.clone();
    let error = graphic.symmetrise_path(false).unwrap_err();
    assert_eq!(
        error,
        "Not symmetric enough: 2 nodes can't be paired across an axis"
    );
    assert_eq!(graphic, before);

    let mut graphic = lobe();
    rotate(&mut graphic, 0.6);
    let before = graphic.clone();
    assert!(graphic.symmetrise_path(true).is_err());
    assert_eq!(graphic, before);
    assert_eq!(graphic.symmetrise_path(false), Ok(false));
}

#[test]
fn a_mismatched_line_and_cubic_cannot_pair_even_when_their_nodes_fit() {
    let mut graphic = bracket();
    graphic.path[4] = PathCommand::Line(Point::new(4., 5.));
    let before = graphic.clone();
    let error = graphic.symmetrise_path(false).unwrap_err();
    assert!(error.starts_with("Not symmetric enough: "), "{error}");
    assert!(
        error.contains("straight and curved segments differ"),
        "{error}"
    );
    assert_eq!(graphic, before);
}

#[test]
fn fitting_scales_with_control_bounds_and_refuses_a_single_extreme_control() {
    for scale in [0.0001, 10_000.] {
        let mut graphic = bracket();
        if let PathCommand::Cubic(a, _, _) = &mut graphic.path[1] {
            a.x += 0.15;
        }
        graphic.path = graphic
            .path
            .iter()
            .map(|command| command.map(|point| Point::new(point.x * scale, point.y * scale)))
            .collect();
        assert_eq!(graphic.symmetrise_path(false), Ok(true));
        assert_eq!(graphic.symmetrise_path(false), Ok(false));
    }
    let mut graphic = bracket();
    if let PathCommand::Cubic(a, _, _) = &mut graphic.path[1] {
        a.x = 1_000_000.;
    }
    let before = graphic.clone();
    assert!(graphic.symmetrise_path(false).is_err());
    assert_eq!(graphic, before);
}

#[test]
fn a_closed_path_tries_every_node_pairing_instead_of_fixing_the_seam() {
    let mut graphic = lobe();
    let parsed = Path::read(&graphic).unwrap();
    let mut commands = vec![PathCommand::Move(parsed.nodes[1].point)];
    for segment in [1, 2, 3, 0] {
        let [a, b] = parsed.segments[segment].controls.unwrap();
        commands.push(PathCommand::Cubic(
            a,
            b,
            parsed.nodes[(segment + 1) % 4].point,
        ));
    }
    commands.push(PathCommand::Close);
    graphic.path = commands;
    if let PathCommand::Cubic(a, _, _) = &mut graphic.path[1] {
        a.x += 0.16;
    }
    rotate(&mut graphic, 0.31);
    assert_eq!(graphic.symmetrise_path(false), Ok(true));
    let parsed = Path::read(&graphic).unwrap();
    assert_reflection(&graphic, 2, parsed.nodes[1].point, parsed.nodes[3].point);
}

#[test]
fn a_pairing_with_every_node_in_tolerance_beats_a_lower_residual() {
    use PathCommand::*;
    let p = Point::new;
    let mut graphic = path(vec![
        Move(p(10., -1.)),
        Line(p(3., 10.)),
        Line(p(-3., 7.)),
        Line(p(-11., 0.)),
        Line(p(-6., -10.)),
        Line(p(4., -9.)),
        Close,
    ]);
    let parsed = Path::read(&graphic).unwrap();
    let samples = Samples::read(&parsed).unwrap();
    let fit = |offset| {
        samples
            .fit(samples.pairing(&parsed, offset).unwrap(), false)
            .unwrap()
    };
    let accepted = fit(1);
    let lower_residual = fit(4);
    assert!(lower_residual.residual < accepted.residual);
    assert!(
        lower_residual
            .distances
            .iter()
            .any(|distance| *distance > samples.tolerance)
    );
    assert!(
        accepted
            .distances
            .iter()
            .all(|distance| *distance <= samples.tolerance)
    );

    assert_eq!(graphic.symmetrise_path(false), Ok(true));
    let parsed = Path::read(&graphic).unwrap();
    for (node, &other) in parsed.nodes.iter().zip(&accepted.paired) {
        let (x, y) = accepted.reflect(parsed.nodes[other].point);
        assert!((x - f64::from(node.point.x)).hypot(y - f64::from(node.point.y)) <= 1e-6);
    }
    assert_eq!(graphic.symmetrise_path(false), Ok(false));
}

#[test]
fn symmetric_legacy_paths_do_not_materialize_geometry_or_node_metadata() {
    let mut graphic = lobe();
    rotate(&mut graphic, 0.29);
    let before = graphic.clone();
    assert_eq!(graphic.symmetrise_path(false), Ok(false));
    assert_eq!(graphic, before);
    assert!(graphic.node_types.is_empty());

    graphic.kind = GraphicKind::Curve;
    graphic.path.clear();
    graphic.origin = Point::new(2., -1.);
    graphic.axis_x = Point::new(5., 2.);
    graphic.axis_y = Point::default();
    let before = graphic.clone();
    assert_eq!(graphic.symmetrise_path(false), Ok(false));
    assert_eq!(graphic, before);
}

#[test]
fn edited_paths_keep_the_affine_frame_depth_and_other_metadata() {
    let mut graphic = lobe();
    if let PathCommand::Cubic(a, _, _) = &mut graphic.path[1] {
        a.x += 0.15;
    }
    graphic.origin = Point::new(8., -3.);
    graphic.axis_x = Point::new(1.5, 0.);
    graphic.axis_y = Point::new(0., 1.2);
    graphic.depth = [7., 3., -2.];
    graphic.node_types = vec![PathNodeType::Corner; 4];
    let before = graphic.clone();
    assert_eq!(graphic.symmetrise_path(false), Ok(true));
    assert_eq!(graphic.origin, before.origin);
    assert_eq!(graphic.axis_x, before.axis_x);
    assert_eq!(graphic.axis_y, before.axis_y);
    assert_eq!(graphic.depth, before.depth);
    assert_eq!(graphic.style, before.style);
    assert_eq!(graphic.node_types, before.node_types);
    assert_eq!(graphic.symmetrise_path(false), Ok(false));
}

#[test]
fn compound_paths_and_uneditable_projected_frames_are_unchanged() {
    let mut graphic = bracket();
    graphic.path.push(PathCommand::Move(Point::new(10., 10.)));
    let before = graphic.clone();
    assert_eq!(
        graphic.symmetrise_path(false),
        Err("Select one continuous path".into())
    );
    assert_eq!(graphic, before);

    let mut graphic = bracket();
    graphic.axis_x = Point::new(1., 0.);
    graphic.axis_y = Point::default();
    graphic.depth = [1., 0., 2.];
    if let PathCommand::Cubic(a, _, _) = &mut graphic.path[1] {
        a.x += 0.15;
    }
    let before = graphic.clone();
    assert_eq!(
        graphic.symmetrise_path(true),
        Err("Rotate this edge-on projected path toward the page before editing it".into())
    );
    assert_eq!(graphic, before);
}
