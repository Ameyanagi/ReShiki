use super::{Cycle, Shape, outline};
use crate::{
    document::{Annotation, DRAWING_ELEMENTS, Document, Point},
    graphics::{Graphic, GraphicKind, PathCommand},
};

fn graphic(shape: Shape) -> Graphic {
    let mut graphic = Graphic::dragged(
        1,
        GraphicKind::Ellipse,
        Point::new(10., 20.),
        Point::new(130., 100.),
        Default::default(),
        Default::default(),
        false,
    );
    graphic.cycle = Some(Box::new(Cycle {
        shape,
        ..Default::default()
    }));
    graphic
}

fn near(actual: Point, expected: Point) {
    assert!(
        actual.distance(expected) < 0.01,
        "{actual:?} != {expected:?}"
    );
}

fn scalar_near(actual: f64, expected: f64) {
    let tolerance = 0.002 + expected.abs() * 0.000_02;
    assert!(
        (actual - expected).abs() < tolerance,
        "{actual} != {expected} (tolerance {tolerance})"
    );
}

fn first(commands: &[PathCommand]) -> Point {
    let Some(PathCommand::Move(point)) = commands.first() else {
        panic!("subpath must start with Move: {commands:?}");
    };
    *point
}

fn last(commands: &[PathCommand]) -> Point {
    commands
        .iter()
        .rev()
        .find_map(|command| match command {
            PathCommand::Move(point)
            | PathCommand::Line(point)
            | PathCommand::Cubic(_, _, point) => Some(*point),
            PathCommand::Close => None,
        })
        .unwrap()
}

// Composite Simpson integration of the analytic cubic derivative is independent
// of the perimeter service's distance lookup and subdivision decisions.
fn cubic_length(points: [Point; 4], end: f64) -> f64 {
    const STEPS: usize = 2048;
    let speed = |t: f64| {
        let u = 1. - t;
        let component = |coordinate: fn(Point) -> f32| {
            let p = points.map(|point| f64::from(coordinate(point)));
            3. * u * u * (p[1] - p[0]) + 6. * u * t * (p[2] - p[1]) + 3. * t * t * (p[3] - p[2])
        };
        component(|point| point.x).hypot(component(|point| point.y))
    };
    let step = end / STEPS as f64;
    let interior: f64 = (1..STEPS)
        .map(|index| speed(index as f64 * step) * if index % 2 == 0 { 2. } else { 4. })
        .sum();
    step / 3. * (speed(0.) + interior + speed(end))
}

fn cubic_point(points: [Point; 4], t: f64) -> Point {
    let u = 1. - t;
    let component = |coordinate: fn(Point) -> f32| {
        let p = points.map(|point| f64::from(coordinate(point)));
        (u * u * u * p[0] + 3. * u * u * t * p[1] + 3. * u * t * t * p[2] + t * t * t * p[3]) as f32
    };
    Point::new(component(|point| point.x), component(|point| point.y))
}

fn path_length(commands: &[PathCommand]) -> f64 {
    let (mut start, mut at, mut length) = (None, None, 0.);
    let distance = |a: Point, b: Point| {
        (f64::from(a.x) - f64::from(b.x)).hypot(f64::from(a.y) - f64::from(b.y))
    };
    for command in commands {
        match *command {
            PathCommand::Move(point) => {
                start = Some(point);
                at = Some(point);
            }
            PathCommand::Line(point) => {
                length += distance(at.unwrap(), point);
                at = Some(point);
            }
            PathCommand::Cubic(a, b, point) => {
                length += cubic_length([at.unwrap(), a, b, point], 1.);
                at = Some(point);
            }
            PathCommand::Close => {
                length += distance(at.unwrap(), start.unwrap());
                at = start;
            }
        }
    }
    length
}

#[test]
fn legacy_missing_shape_stays_a_mean_radius_circle_even_on_singular_frames() {
    assert_eq!(
        serde_json::from_str::<Cycle>("{}").unwrap().shape,
        Shape::Circle
    );
    for (x, y) in [
        (Point::new(400., 0.), Point::new(0., 40.)),
        (Point::new(200., 0.), Point::default()),
        (Point::new(200., 0.), Point::new(100., 0.)),
        (Point::new(-200., 30.), Point::new(40., 80.)),
    ] {
        let mut g = graphic(Shape::Circle);
        (g.axis_x, g.axis_y) = (x, y);
        g.validate().unwrap();
        let (center, radius) = g.cycle_circle();
        let perimeter = outline(&g).unwrap();
        scalar_near(
            perimeter.length(),
            f64::from(radius) * std::f64::consts::TAU,
        );
        near(perimeter.point(0.).unwrap(), center.offset(radius, 0.));
        near(
            perimeter.point(perimeter.length() * 0.25).unwrap(),
            center.offset(0., radius),
        );
        let doc = Document {
            graphics: vec![g],
            ..Default::default()
        };
        let json = doc.file_json().unwrap();
        let wire: serde_json::Value = serde_json::from_slice(&json).unwrap();
        assert!(wire["graphics"][0]["cycle"].get("shape").is_none());
        let back = Document::from_json(&json).unwrap();
        assert_eq!(
            back.graphics[0].cycle.as_ref().unwrap().shape,
            Shape::Circle
        );
        assert_eq!(
            super::drawn(&back, &back.graphics[0]).arrows,
            super::drawn(&doc, &doc.graphics[0]).arrows,
        );
    }
}

#[test]
fn explicit_shapes_roundtrip_at_version_40_and_survive_copying_and_remapping() {
    for (shape, wire_name) in [
        (Shape::Circle, "circle"),
        (Shape::Ellipse, "ellipse"),
        (Shape::RoundedRectangle, "rounded_rectangle"),
        (Shape::Rectangle, "rectangle"),
    ] {
        let parsed: Cycle =
            serde_json::from_value(serde_json::json!({"shape": wire_name})).unwrap();
        assert_eq!(parsed.shape, shape);
        let mut g = graphic(shape);
        let cycle = g.cycle.as_deref_mut().unwrap();
        cycle.intermediates.push(vec![2]);
        cycle.steps.push(Default::default());
        let doc = Document {
            graphics: vec![g],
            annotations: vec![Annotation {
                id: 2,
                angle: 0.,
                position: Point::new(10., 20.),
                text: "Pd".into(),
                format: Default::default(),
            }],
            ..Default::default()
        };
        doc.validate().unwrap();
        assert_eq!(doc.file_version(), DRAWING_ELEMENTS);
        let back = Document::from_json(&doc.file_json().unwrap()).unwrap();
        assert_eq!(back.graphics[0].cycle, doc.graphics[0].cycle);
        let copy = crate::editing::selection(&doc, &[1, 2]);
        let mut pasted = Document::default();
        assert!(!crate::editing::append(&mut pasted, &copy, Point::new(300., 0.)).is_empty());
        pasted.validate().unwrap();
        assert_eq!(pasted.graphics[0].cycle.as_ref().unwrap().shape, shape);
        let alone = crate::editing::selection(&doc, &[1]);
        let cycle = alone.graphics[0].cycle.as_ref().unwrap();
        assert_eq!(cycle.shape, shape);
        assert!(cycle.intermediates.is_empty());
    }
    assert!(serde_json::from_str::<Cycle>(r#"{"shape":"triangle"}"#).is_err());
}

#[test]
fn rectangle_distances_and_one_sided_corner_tangents_are_exact() {
    let g = graphic(Shape::Rectangle);
    let perimeter = outline(&g).unwrap();
    scalar_near(perimeter.length(), 400.);
    for (s, point) in [
        (0., Point::new(10., 20.)),
        (120., Point::new(130., 20.)),
        (200., Point::new(130., 100.)),
        (320., Point::new(10., 100.)),
        (400., Point::new(10., 20.)),
        (-20., Point::new(10., 40.)),
        (420., Point::new(30., 20.)),
    ] {
        near(perimeter.point(s).unwrap(), point);
    }
    let corner = perimeter.tangents(120.).unwrap();
    near(corner.incoming, Point::new(1., 0.));
    near(corner.outgoing, Point::new(0., 1.));
    let seam = perimeter.tangents(0.).unwrap();
    near(seam.incoming, Point::new(0., -1.));
    near(seam.outgoing, Point::new(1., 0.));
    let mut square = g;
    square.axis_y = Point::new(0., 120.);
    let perimeter = outline(&square).unwrap();
    let tied = perimeter.project(Point::new(70., 80.)).unwrap();
    scalar_near(tied.s, 60.);
    scalar_near(tied.distance, 60.);
    near(tied.point, Point::new(70., 20.));
    scalar_near(perimeter.project(square.origin).unwrap().s, 0.);
}

#[test]
fn projection_does_not_turn_distinct_near_seam_or_tiny_shape_points_into_ties() {
    let mut g = graphic(Shape::Rectangle);
    g.origin = Point::default();
    let query = Point::new(0., 0.000_000_5);
    let perimeter = outline(&g).unwrap();
    let projected = perimeter.project(query).unwrap();
    assert!(projected.distance < 1e-12, "{projected:?}");
    assert_eq!(projected.point, query);
    assert!((projected.s - (400. - f64::from(query.y))).abs() < 1e-10);

    let side = 1e-8_f32;
    g.axis_x = Point::new(side, 0.);
    g.axis_y = Point::new(0., side);
    let query = Point::new(side * 0.5, side);
    let projected = outline(&g).unwrap().project(query).unwrap();
    assert!(projected.distance < f64::from(side) * 1e-6, "{projected:?}");
    assert_eq!(projected.point, query);
    assert!((projected.s - f64::from(side) * 2.5).abs() < f64::from(side) * 1e-6);
}

#[test]
fn far_query_projection_compares_resolved_distance_differences_without_false_ties() {
    let mut g = graphic(Shape::Rectangle);
    g.origin = Point::default();
    let perimeter = outline(&g).unwrap();
    let projected = perimeter.project(Point::new(0., 1e15)).unwrap();
    assert_eq!(projected.point, Point::new(0., 80.));
    scalar_near(projected.s, 320.);
}

#[test]
fn affine_outlines_keep_the_commands_seam_and_world_clockwise_traversal() {
    for shape in [Shape::Ellipse, Shape::RoundedRectangle, Shape::Rectangle] {
        for (x, y) in [
            (Point::new(120., 35.), Point::new(30., 80.)),
            (Point::new(-120., -35.), Point::new(30., 80.)),
            (Point::new(-120., -35.), Point::new(-30., -80.)),
        ] {
            let mut g = graphic(shape);
            (g.axis_x, g.axis_y) = (x, y);
            let mut host = g.clone();
            host.kind = match shape {
                Shape::Ellipse => GraphicKind::Ellipse,
                Shape::RoundedRectangle => GraphicKind::RoundedRectangle,
                _ => GraphicKind::Rectangle,
            };
            let perimeter = outline(&g).unwrap();
            near(perimeter.point(0.).unwrap(), first(&host.commands()));
            let points: Vec<_> = (0..128)
                .map(|i| {
                    perimeter
                        .point(perimeter.length() * i as f64 / 128.)
                        .unwrap()
                })
                .collect();
            let twice_area: f64 = points
                .iter()
                .zip(points.iter().cycle().skip(1))
                .map(|(a, b)| f64::from(a.x) * f64::from(b.y) - f64::from(a.y) * f64::from(b.x))
                .sum();
            assert!(
                twice_area > 0.,
                "{shape:?}: reflected twice or not corrected"
            );
            g.cycle.as_deref_mut().unwrap().counterclockwise = true;
            let other = outline(&g).unwrap();
            for s in [0., perimeter.length() * 0.17, perimeter.length() * 0.63] {
                near(other.point(s).unwrap(), perimeter.point(s).unwrap());
            }
        }
    }
    let mut reflected = graphic(Shape::Rectangle);
    reflected.axis_x = Point::new(-120., 0.);
    let perimeter = outline(&reflected).unwrap();
    near(perimeter.point(80.).unwrap(), Point::new(10., 100.));
    let seam = perimeter.tangents(0.).unwrap();
    near(seam.incoming, Point::new(1., 0.));
    near(seam.outgoing, Point::new(0., 1.));
}

#[test]
fn rounded_zero_and_capped_corners_skip_degenerate_segments() {
    let mut rounded = graphic(Shape::RoundedRectangle);
    rounded.style.corner_pt = Some(0.);
    let perimeter = outline(&rounded).unwrap();
    scalar_near(perimeter.length(), 400.);
    let corner = perimeter.tangents(120.).unwrap();
    near(corner.incoming, Point::new(1., 0.));
    near(corner.outgoing, Point::new(0., 1.));
    rounded.style.corner_pt = Some(1000.);
    let capped = outline(&rounded).unwrap();
    rounded.style.corner_pt = Some(40. * crate::style::DEFAULT.points_per_world());
    let exact = outline(&rounded).unwrap();
    scalar_near(capped.length(), exact.length());
    for i in 0..32 {
        let s = capped.length() * i as f64 / 32.;
        near(capped.point(s).unwrap(), exact.point(s).unwrap());
        let tangents = capped.tangents(s).unwrap();
        for tangent in [tangents.incoming, tangents.outgoing] {
            assert!(tangent.is_finite());
            scalar_near(f64::from(tangent.x).hypot(f64::from(tangent.y)), 1.);
        }
    }
}

#[test]
fn cubic_world_lengths_and_distance_points_match_independent_integration() {
    let mut g = graphic(Shape::Ellipse);
    g.axis_x = Point::new(600., 45.);
    g.axis_y = Point::new(95., 40.);
    let commands = g.commands();
    let perimeter = outline(&g).unwrap();
    scalar_near(perimeter.length(), path_length(&commands));
    let PathCommand::Cubic(a, b, end) = commands[1] else {
        panic!("ellipse cubic")
    };
    let points = [first(&commands), a, b, end];
    let s = cubic_length(points, 0.37);
    let expected = cubic_point(points, 0.37);
    near(perimeter.point(s).unwrap(), expected);
    let projected = perimeter.project(expected).unwrap();
    near(projected.point, expected);
    scalar_near(projected.s, s);
    assert!(projected.distance < 0.01);
}

#[test]
fn subpaths_split_cubics_reverse_cross_the_seam_and_clamp_to_one_loop() {
    let g = graphic(Shape::Ellipse);
    let perimeter = outline(&g).unwrap();
    let length = perimeter.length();
    let (start, travel) = (length * 0.13, length * 0.42);
    let forward = perimeter.subpath(start, travel).unwrap();
    let reverse = perimeter.subpath(start + travel, -travel).unwrap();
    assert!(forward.iter().any(|c| matches!(c, PathCommand::Cubic(..))));
    assert!(!forward.iter().any(|c| matches!(c, PathCommand::Close)));
    near(first(&forward), perimeter.point(start).unwrap());
    near(last(&forward), perimeter.point(start + travel).unwrap());
    near(first(&reverse), last(&forward));
    near(last(&reverse), first(&forward));
    scalar_near(path_length(&forward), travel);
    scalar_near(path_length(&reverse), travel);
    for direction in [-1., 1.] {
        let full = perimeter
            .subpath(-length * 0.2, direction * length * 3.)
            .unwrap();
        scalar_near(path_length(&full), length);
        near(first(&full), last(&full));
    }
    let zero = perimeter.subpath(start, 0.).unwrap();
    assert_eq!(zero.len(), 1);
    near(first(&zero), perimeter.point(start).unwrap());
    let rectangle = outline(&graphic(Shape::Rectangle)).unwrap();
    let across_seam = rectangle.subpath(385., 30.).unwrap();
    near(first(&across_seam), Point::new(10., 35.));
    near(last(&across_seam), Point::new(25., 20.));
    scalar_near(path_length(&across_seam), 30.);
}

#[test]
fn noncircle_singular_frames_and_nonfinite_queries_fail_without_nan_results() {
    for shape in [Shape::Ellipse, Shape::RoundedRectangle, Shape::Rectangle] {
        let mut g = graphic(shape);
        g.axis_y = g.axis_x;
        assert!(outline(&g).is_none(), "{shape:?}: singular frame");
        g.axis_y = Point::new(0., f32::NAN);
        assert!(outline(&g).is_none(), "{shape:?}: nonfinite frame");
    }
    let perimeter = outline(&graphic(Shape::Rectangle)).unwrap();
    for s in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(perimeter.point(s).is_none());
        assert!(perimeter.tangents(s).is_none());
        assert!(perimeter.subpath(s, 1.).is_none());
        assert!(perimeter.subpath(0., s).is_none());
    }
    assert!(perimeter.project(Point::new(f32::NAN, 0.)).is_none());
    assert!(perimeter.project(Point::new(0., f32::INFINITY)).is_none());
}

#[test]
fn unaddressable_small_edges_at_large_perimeter_distances_fail_construction() {
    let mut g = graphic(Shape::Rectangle);
    g.origin = Point::default();
    g.axis_x = Point::new(1e20, 0.);
    g.axis_y = Point::new(0., 1e-3);
    // The frame is valid, but adding a short edge to the first long edge's
    // f64 arclength cannot distinguish its two endpoints. Do not skip it.
    g.validate().unwrap();
    assert!(outline(&g).is_none());
}
