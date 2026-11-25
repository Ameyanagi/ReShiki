use super::{PathCommand, Point};
use super::{closed, open, region, runs};
use std::time::{Duration, Instant};

type DPoint = [f64; 2];

// Geometry here deliberately uses only the emitted commands, rather than any
// fitting implementation helpers. Flattening error is much smaller than the
// public error budgets exercised below.
const FLATNESS: f64 = 0.000_02;
const NUMERICAL_TOLERANCE: f64 = 0.000_2;

#[derive(Debug)]
struct FlatSubpath {
    points: Vec<DPoint>,
    closed: bool,
}

fn dp(p: Point) -> DPoint {
    [f64::from(p.x), f64::from(p.y)]
}

fn distance(a: DPoint, b: DPoint) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn midpoint(a: DPoint, b: DPoint) -> DPoint {
    [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5]
}

fn segment_distance(p: DPoint, a: DPoint, b: DPoint) -> f64 {
    let v = [b[0] - a[0], b[1] - a[1]];
    let denominator = v[0] * v[0] + v[1] * v[1];
    let t = if denominator == 0.0 {
        0.0
    } else {
        (((p[0] - a[0]) * v[0] + (p[1] - a[1]) * v[1]) / denominator).clamp(0.0, 1.0)
    };
    distance(p, [a[0] + t * v[0], a[1] + t * v[1]])
}

fn flatten_cubic(curve: [DPoint; 4], depth: usize, points: &mut Vec<DPoint>) {
    let [a, b, c, d] = curve;
    if depth >= 24 || segment_distance(b, a, d).max(segment_distance(c, a, d)) <= FLATNESS {
        points.push(d);
        return;
    }
    let ab = midpoint(a, b);
    let bc = midpoint(b, c);
    let cd = midpoint(c, d);
    let abc = midpoint(ab, bc);
    let bcd = midpoint(bc, cd);
    let center = midpoint(abc, bcd);
    flatten_cubic([a, ab, abc, center], depth + 1, points);
    flatten_cubic([center, bcd, cd, d], depth + 1, points);
}

fn flatten(commands: &[PathCommand]) -> Vec<FlatSubpath> {
    let mut result = Vec::new();
    let mut current: Option<FlatSubpath> = None;
    for command in commands {
        match *command {
            PathCommand::Move(p) => {
                if let Some(path) = current.take() {
                    result.push(path);
                }
                current = Some(FlatSubpath {
                    points: vec![dp(p)],
                    closed: false,
                });
            }
            PathCommand::Line(p) => current
                .as_mut()
                .expect("line must follow a move")
                .points
                .push(dp(p)),
            PathCommand::Cubic(b, c, d) => {
                let path = current.as_mut().expect("cubic must follow a move");
                let a = *path.points.last().unwrap();
                flatten_cubic([a, dp(b), dp(c), dp(d)], 0, &mut path.points);
            }
            PathCommand::Close => {
                let mut path = current.take().expect("close must follow a move");
                let first = path.points[0];
                if *path.points.last().unwrap() != first {
                    path.points.push(first);
                }
                path.closed = true;
                result.push(path);
            }
        }
    }
    if let Some(path) = current {
        result.push(path);
    }
    result
}

fn assert_finite(commands: &[PathCommand]) {
    let check = |p: Point| {
        assert!(p.x.is_finite() && p.y.is_finite(), "nonfinite point: {p:?}");
    };
    for command in commands {
        match *command {
            PathCommand::Move(p) | PathCommand::Line(p) => check(p),
            PathCommand::Cubic(a, b, c) => {
                check(a);
                check(b);
                check(c);
            }
            PathCommand::Close => {}
        }
    }
}

fn assert_vertices_within(points: &[[f32; 2]], commands: &[PathCommand], error: f64) {
    assert_finite(commands);
    let paths = flatten(commands);
    for (index, point) in points.iter().enumerate() {
        let p = [f64::from(point[0]), f64::from(point[1])];
        let nearest = paths
            .iter()
            .flat_map(|path| path.points.windows(2))
            .map(|segment| segment_distance(p, segment[0], segment[1]))
            .fold(f64::INFINITY, f64::min);
        assert!(
            nearest <= error + NUMERICAL_TOLERANCE,
            "vertex {index} {point:?} is {nearest} from the output (budget {error})"
        );
    }
}

fn segment_count(commands: &[PathCommand]) -> usize {
    commands
        .iter()
        .filter(|command| matches!(command, PathCommand::Line(_) | PathCommand::Cubic(..)))
        .count()
}

fn cubic_count(commands: &[PathCommand]) -> usize {
    commands
        .iter()
        .filter(|command| matches!(command, PathCommand::Cubic(..)))
        .count()
}

fn circle(count: usize, radius: f32) -> Vec<[f32; 2]> {
    (0..count)
        .map(|i| {
            let angle = std::f64::consts::TAU * i as f64 / count as f64;
            [radius * angle.cos() as f32, radius * angle.sin() as f32]
        })
        .collect()
}

fn rectangle(x: f32, y: f32, width: f32, height: f32) -> Vec<[f32; 2]> {
    vec![
        [x, y],
        [x + width, y],
        [x + width, y + height],
        [x, y + height],
    ]
}

fn unit_direction(from: DPoint, candidates: &[DPoint]) -> DPoint {
    for &to in candidates {
        let length = distance(from, to);
        if length > 1e-9 {
            return [(to[0] - from[0]) / length, (to[1] - from[1]) / length];
        }
    }
    panic!("zero-length emitted segment");
}

fn assert_smooth_closed(commands: &[PathCommand]) {
    let mut cursor = None;
    let mut first = None;
    let mut tangents = Vec::new();
    let mut did_close = false;
    for command in commands {
        match *command {
            PathCommand::Move(p) => {
                assert!(cursor.is_none(), "expected exactly one contour");
                cursor = Some(dp(p));
                first = cursor;
            }
            PathCommand::Line(p) => {
                let from = cursor.expect("line without a move");
                let to = dp(p);
                let tangent = unit_direction(from, &[to]);
                tangents.push((tangent, tangent));
                cursor = Some(to);
            }
            PathCommand::Cubic(a, b, p) => {
                let from = cursor.expect("cubic without a move");
                let (a, b, to) = (dp(a), dp(b), dp(p));
                let start = unit_direction(from, &[a, b, to]);
                let reverse_end = unit_direction(to, &[b, a, from]);
                tangents.push((start, [-reverse_end[0], -reverse_end[1]]));
                cursor = Some(to);
            }
            PathCommand::Close => {
                let (last, first) = (cursor.unwrap(), first.unwrap());
                if distance(last, first) > 1e-6 {
                    let tangent = unit_direction(last, &[first]);
                    tangents.push((tangent, tangent));
                }
                did_close = true;
            }
        }
    }
    assert!(did_close, "closed fit must emit Close");
    assert!(
        tangents.len() >= 2,
        "expected multiple nondegenerate segments"
    );
    let min_dot = (1.0_f64.to_radians()).cos();
    for i in 0..tangents.len() {
        let incoming = tangents[i].1;
        let outgoing = tangents[(i + 1) % tangents.len()].0;
        let dot = incoming[0] * outgoing[0] + incoming[1] * outgoing[1];
        assert!(
            dot >= min_dot,
            "join {i}, including the cyclic seam, is not smooth: dot={dot}"
        );
    }
}

fn assert_equal_control_handles_closed(commands: &[PathCommand]) {
    let mut curves = Vec::new();
    let mut cursor = None;
    for command in commands {
        match *command {
            PathCommand::Move(p) => cursor = Some(dp(p)),
            PathCommand::Cubic(b, c, d) => {
                curves.push([cursor.expect("cubic without move"), dp(b), dp(c), dp(d)]);
                cursor = Some(dp(d));
            }
            PathCommand::Line(_) => panic!("expected cubic circle segments"),
            PathCommand::Close => {}
        }
    }
    assert!(curves.len() >= 2);
    let ulp = |value: f64| {
        let value = (value as f32).abs();
        f64::from(f32::from_bits(value.to_bits() + 1)) - f64::from(value)
    };
    for i in 0..curves.len() {
        let prev = curves[i];
        let next = curves[(i + 1) % curves.len()];
        assert_eq!(prev[3], next[0], "cubic {i} does not meet the next cubic");
        for axis in 0..2 {
            let incoming = prev[3][axis] - prev[2][axis];
            let outgoing = next[1][axis] - next[0][axis];
            // Both control points round the same ideal shared handle. Each
            // contributes at most half an f32 ULP of coordinate error.
            let rounding = 0.5 * (ulp(prev[2][axis]) + ulp(next[1][axis]));
            assert!(
                (incoming - outgoing).abs() <= rounding + 1e-12,
                "join {i} axis {axis}, including the seam: incoming={incoming}, \
             outgoing={outgoing}, f32 rounding bound={rounding}"
            );
        }
    }
}

fn cross(a: DPoint, b: DPoint) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn difference(a: DPoint, b: DPoint) -> DPoint {
    [a[0] - b[0], a[1] - b[1]]
}

// Exhaustive segment pairs provide an independent oracle for the optimized
// intersection sweep. Touches and overlapping nonadjacent edges count too.
fn independent_intersects(points: &[DPoint], cyclic: bool) -> bool {
    let mut points = points.to_vec();
    points.dedup();
    if cyclic && points.len() > 1 && points.first() == points.last() {
        points.pop();
    }
    if points.len() < 3 {
        return false;
    }
    let count = points.len() - usize::from(!cyclic);
    let edges: Vec<_> = (0..count)
        .map(|i| (points[i], points[(i + 1) % points.len()]))
        .collect();
    for i in usize::from(!cyclic)..count {
        let (a, b) = edges[(i + count - 1) % count];
        let (_, c) = edges[i];
        let incoming = difference(b, a);
        let outgoing = difference(c, b);
        if cross(incoming, outgoing) == 0.0
            && incoming[0] * outgoing[0] + incoming[1] * outgoing[1] < 0.0
        {
            return true;
        }
    }
    let straddles = |a: f64, b: f64| a.min(b) <= 0.0 && a.max(b) >= 0.0;
    for i in 0..count {
        let (a, b) = edges[i];
        for (j, &(c, d)) in edges[..i].iter().enumerate() {
            if i - j == 1 || (cyclic && i - j == count - 1) {
                continue;
            }
            if (0..2).any(|axis| {
                a[axis].max(b[axis]) < c[axis].min(d[axis])
                    || c[axis].max(d[axis]) < a[axis].min(b[axis])
            }) {
                continue;
            }
            let ab = difference(b, a);
            let cd = difference(d, c);
            if straddles(cross(ab, difference(c, a)), cross(ab, difference(d, a)))
                && straddles(cross(cd, difference(a, c)), cross(cd, difference(b, c)))
            {
                return true;
            }
        }
    }
    false
}

fn sample_steps(commands: &[PathCommand], steps: u32) -> Vec<DPoint> {
    let mut points = Vec::new();
    for command in commands {
        match *command {
            PathCommand::Move(p) | PathCommand::Line(p) => points.push(dp(p)),
            PathCommand::Cubic(b, c, d) => {
                let [a, b, c, d] = [*points.last().unwrap(), dp(b), dp(c), dp(d)];
                for step in 1..=steps {
                    let t = f64::from(step) / f64::from(steps);
                    let s = 1.0 - t;
                    points.push([
                        s * s * s * a[0]
                            + 3.0 * s * s * t * b[0]
                            + 3.0 * s * t * t * c[0]
                            + t * t * t * d[0],
                        s * s * s * a[1]
                            + 3.0 * s * s * t * b[1]
                            + 3.0 * s * t * t * c[1]
                            + t * t * t * d[1],
                    ]);
                }
            }
            PathCommand::Close => {}
        }
    }
    points
}

fn nonzero_contains(paths: &[FlatSubpath], point: DPoint) -> bool {
    let mut winding = 0;
    for path in paths {
        assert!(path.closed, "region output must contain closed contours");
        for edge in path.points.windows(2) {
            let (a, b) = (edge[0], edge[1]);
            if (a[1] > point[1]) != (b[1] > point[1]) {
                let crossing = a[0] + (point[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1]);
                if point[0] < crossing {
                    winding += if b[1] > a[1] { 1 } else { -1 };
                }
            }
        }
    }
    winding != 0
}

#[test]
fn circle_64_uses_at_most_eight_cubics_and_stays_radially_accurate() {
    let points = circle(64, 50.0);
    let commands = closed(&points, 0.1);
    let cubics = cubic_count(&commands);
    assert!(
        (1..=8).contains(&cubics),
        "expected 1..=8 cubics, got {cubics}"
    );
    assert_vertices_within(&points, &commands, 0.1);
    let paths = flatten(&commands);
    assert_eq!(paths.len(), 1);
    assert!(paths[0].closed);
    for p in &paths[0].points {
        let radial_error = (p[0].hypot(p[1]) - 50.0).abs();
        assert!(
            radial_error <= 0.1 + NUMERICAL_TOLERANCE,
            "curve point {p:?} has radial error {radial_error}"
        );
    }
    assert_smooth_closed(&commands);
    assert_equal_control_handles_closed(&commands);
}

#[test]
fn unit_circle_64_does_not_turn_smooth_samples_into_corners_or_lines() {
    let points = circle(64, 1.0);
    let error = 0.12_f32;
    let prepared = super::prepare(&super::clean(&points), f64::from(error), true);
    assert!(
        prepared.iter().all(|knot| !knot.corner),
        "a coarse circle must not acquire corners from the tangent search window"
    );
    let commands = closed(&points, error);
    assert_vertices_within(&points, &commands, f64::from(error));
    assert!(cubic_count(&commands) > 0);
    assert!(
        commands
            .iter()
            .all(|command| !matches!(command, PathCommand::Line(_))),
        "a smooth circular contour must contain only cubic segments"
    );
    assert_smooth_closed(&commands);
    assert_equal_control_handles_closed(&commands);
}

#[test]
fn dense_square_interpolates_all_four_corners() {
    for (side, error) in [(10.0_f32, 0.1_f32), (0.5, 0.12)] {
        let corners = [[0.0, 0.0], [side, 0.0], [side, side], [0.0, side]];
        let mut points = Vec::new();
        for edge in 0..4 {
            let a = corners[edge];
            let b = corners[(edge + 1) % 4];
            for step in 0..40 {
                let t = step as f32 / 40.0;
                points.push([a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]);
            }
        }
        // Dense samples localize each actual corner even when the whole
        // half-unit square fits within the tangent search window.
        let prepared = super::prepare(&super::clean(&points), f64::from(error), true);
        let detected: Vec<_> = prepared[..prepared.len() - 1]
            .iter()
            .filter(|knot| knot.corner)
            .map(|knot| [knot.p.x as f32, knot.p.y as f32])
            .collect();
        assert_eq!(
            detected.len(),
            4,
            "side-{side} square must retain exactly its four corners: {detected:?}"
        );
        let commands = closed(&points, error);
        assert_vertices_within(&points, &commands, f64::from(error));
        let endpoints: Vec<DPoint> = commands
            .iter()
            .filter_map(|command| match *command {
                PathCommand::Move(p) | PathCommand::Line(p) | PathCommand::Cubic(_, _, p) => {
                    Some(dp(p))
                }
                PathCommand::Close => None,
            })
            .collect();
        for corner in corners {
            assert!(
                detected.contains(&corner),
                "side-{side} square corner {corner:?} must be detected"
            );
            let corner = [f64::from(corner[0]), f64::from(corner[1])];
            assert!(
                endpoints.iter().any(|&p| distance(p, corner) < 0.000_01),
                "side-{side} square corner {corner:?} was not interpolated by a segment endpoint"
            );
        }
        assert!(flatten(&commands).iter().all(|path| path.closed));
    }
}

#[test]
fn square_with_small_zigzags_detects_only_its_four_actual_corners() {
    let corners = rectangle(0.0, 0.0, 10.0, 10.0);
    let mut points = Vec::new();
    for edge in 0..4 {
        let a = corners[edge];
        let b = corners[(edge + 1) % 4];
        for step in 0..40 {
            let t = step as f32 / 40.0;
            let noise = if step == 0 {
                0.0
            } else if step % 2 == 0 {
                0.05
            } else {
                -0.05
            };
            points.push([
                a[0] + t * (b[0] - a[0]) - noise * (b[1] - a[1]) / 10.0,
                a[1] + t * (b[1] - a[1]) + noise * (b[0] - a[0]) / 10.0,
            ]);
        }
    }
    let error = 0.12_f32;
    let prepared = super::prepare(&super::clean(&points), f64::from(error), true);
    // prepare repeats the seam knot for a closed chain; count it only once.
    let detected: Vec<_> = prepared[..prepared.len() - 1]
        .iter()
        .filter(|knot| knot.corner)
        .map(|knot| [knot.p.x as f32, knot.p.y as f32])
        .collect();
    assert_eq!(
        detected.len(),
        4,
        "the edge zigzags must not create additional corners: {detected:?}"
    );
    for corner in &corners {
        assert!(
            detected.contains(corner),
            "actual square corner {corner:?} was not detected"
        );
    }
    let commands = closed(&points, error);
    assert_vertices_within(&points, &commands, f64::from(error));
    let endpoints: Vec<_> = commands
        .iter()
        .filter_map(|command| match *command {
            PathCommand::Move(p) | PathCommand::Line(p) | PathCommand::Cubic(_, _, p) => {
                Some([p.x, p.y])
            }
            PathCommand::Close => None,
        })
        .collect();
    for corner in &corners {
        assert!(
            endpoints.contains(corner),
            "actual square corner {corner:?} must remain a segment endpoint"
        );
    }
    let paths = flatten(&commands);
    assert_eq!(paths.len(), 1);
    assert!(paths[0].closed);
}

#[test]
fn noisy_circle_does_not_create_corners() {
    for count in [48, 512] {
        let points: Vec<_> = circle(count, 10.0)
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let radius = 10.0 + if i % 2 == 0 { 0.3 } else { -0.3 };
                [p[0] * radius / 10.0, p[1] * radius / 10.0]
            })
            .collect();
        let error = 0.25_f32;
        let prepared = super::prepare(&super::clean(&points), f64::from(error), true);
        assert!(
            prepared.iter().all(|knot| !knot.corner),
            "alternating radial noise must not create corners with {count} samples"
        );
        let commands = closed(&points, error);
        assert_vertices_within(&points, &commands, f64::from(error));
        assert!(cubic_count(&commands) > 0);
        assert!(
            commands
                .iter()
                .all(|command| !matches!(command, PathCommand::Line(_))),
            "noise must not introduce line segments with {count} samples"
        );
        assert_smooth_closed(&commands);
        assert_equal_control_handles_closed(&commands);
    }
}

#[test]
fn topology_intersection_sweep_matches_exhaustive_geometry() {
    let fixtures: Vec<(&str, Vec<[f32; 2]>, bool, bool)> = vec![
        ("simple square", rectangle(0.0, 0.0, 2.0, 2.0), true, false),
        (
            "bow tie",
            vec![[0.0, 0.0], [2.0, 2.0], [0.0, 2.0], [2.0, 0.0]],
            true,
            true,
        ),
        (
            "nonadjacent endpoint touch",
            vec![[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [1.0, 0.0]],
            false,
            true,
        ),
        (
            "nonadjacent overlap",
            vec![[0.0, 0.0], [4.0, 0.0], [4.0, 2.0], [1.0, 0.0], [3.0, 0.0]],
            false,
            true,
        ),
        (
            "monotone collinear",
            vec![[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]],
            false,
            false,
        ),
        (
            "dense vertical sweep",
            (0..70).map(|y| [0.0, y as f32]).collect(),
            false,
            false,
        ),
        (
            "adjacent reversal",
            vec![[0.0, 0.0], [2.0, 0.0], [1.0, 0.0]],
            false,
            true,
        ),
        (
            "cyclic seam reversal",
            vec![[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0], [1.0, 0.0]],
            true,
            true,
        ),
        (
            "disjoint collinear edges",
            vec![
                [0.0, 0.0],
                [1.0, 0.0],
                [1.0, 1.0],
                [2.0, 1.0],
                [2.0, 0.0],
                [3.0, 0.0],
            ],
            false,
            false,
        ),
    ];
    for (name, points, cyclic, expected) in fixtures {
        let independent: Vec<_> = points
            .iter()
            .map(|p| [f64::from(p[0]), f64::from(p[1])])
            .collect();
        assert_eq!(
            independent_intersects(&independent, cyclic),
            expected,
            "oracle fixture: {name}"
        );
        assert_eq!(
            super::intersects(&super::clean(&points), cyclic),
            expected,
            "sweep fixture: {name}"
        );
    }
}

#[test]
fn short_loop_between_eight_step_samples_is_refined_before_accepting_a_cubic() {
    let points = [
        [0.5, 0.0],
        [2.0, 3.0],
        [-2.0, 3.0],
        [-3.0, 0.0],
        [-2.0, -3.5],
        [1.5, -3.0],
    ];
    let commands = closed(&points, 1.1);
    assert!(cubic_count(&commands) > 0, "retain the fitted smooth arc");
    assert_vertices_within(&points, &commands, 1.1);
    assert!(
        !independent_intersects(&sample_steps(&commands, 64), true),
        "dense sampling must not reveal a loop between the guard's samples: {commands:?}"
    );
    assert!(!independent_intersects(&flatten(&commands)[0].points, true));
}

#[test]
fn topology_guard_detects_short_loop_between_coarse_samples() {
    let points = [
        [-0.025, -0.022],
        [-0.356, -0.753],
        [-0.004, -0.191],
        [1.078, -2.574],
        [0.860, -0.301],
    ];
    let commands = closed(&points, 0.12);
    assert!(
        !independent_intersects(&sample_steps(&commands, 64), true),
        "dense sampling must not reveal a short loop between the topology guard's samples"
    );
    assert_vertices_within(&points, &commands, 0.12);
}

#[test]
fn interpolated_short_edges_bound_shared_handles_without_losing_smooth_joins() {
    let points = [
        [1.8075448, 0.0],
        [5.8782153, 5.8782153],
        [0.0, 2.4336567],
        [-0.24110684, 0.24110684],
        [-5.0910563, 0.0],
        [-5.105465, -5.105465],
        [0.0, -1.4674914],
        [6.9647756, -6.9647756],
    ];
    let commands = open(&points, 0.8332968);
    assert_vertices_within(&points, &commands, 0.8332968);
    let mut cursor = dp(Point::new(points[0][0], points[0][1]));
    let mut previous: Option<DPoint> = None;
    let mut checked_joins = 0;
    for command in commands {
        let (incoming, outgoing, end) = match command {
            PathCommand::Move(p) => {
                cursor = dp(p);
                continue;
            }
            PathCommand::Line(p) => {
                let d = difference(dp(p), cursor);
                (d, d, dp(p))
            }
            PathCommand::Cubic(b, c, p) => {
                let chord = distance(cursor, dp(p));
                assert!(
                    distance(cursor, dp(b)) <= chord,
                    "start handle exceeds chord"
                );
                assert!(distance(dp(c), dp(p)) <= chord, "end handle exceeds chord");
                let scale = |v: DPoint| [3.0 * v[0], 3.0 * v[1]];
                (
                    scale(difference(dp(b), cursor)),
                    scale(difference(dp(p), dp(c))),
                    dp(p),
                )
            }
            PathCommand::Close => panic!("unexpected close"),
        };
        if [points[2], points[3]]
            .iter()
            .any(|p| cursor == p.map(f64::from))
        {
            assert!(
                distance(
                    previous.expect("smooth join has an incoming segment"),
                    incoming
                ) < 0.000_005
            );
            checked_joins += 1;
        }
        previous = Some(outgoing);
        cursor = end;
    }
    assert_eq!(checked_joins, 2, "check both ends of the short source edge");
}

fn topology_retry_points() -> [[f32; 2]; 21] {
    [
        [6.6268587, 0.0],
        [3.6215594, 1.1171031],
        [0.20376891, 0.1389273],
        [1.4661759, 1.8385265],
        [1.1586034, 2.9520733],
        [0.5000431, 6.672611],
        [-0.8884022, 3.8923445],
        [-4.3805947, 7.5874124],
        [-3.2550445, 3.0202398],
        [-8.607507, 4.145157],
        [-1.4203819, 0.21408813],
        [-2.0537448, -0.30955222],
        [-7.2432675, -3.4881737],
        [-5.1018667, -4.7338405],
        [-0.5582003, -0.9668312],
        [-0.9101922, -3.9878125],
        [0.68779165, -9.177942],
        [2.580135, -6.574077],
        [5.0543323, -6.337932],
        [1.2242402, -0.8346728],
        [0.62970054, -0.19423689],
    ]
}

#[test]
fn topology_guard_retries_at_half_tolerance_before_returning_a_curve() {
    let points = topology_retry_points();
    let error = 3.3029964_f32;
    let clean = super::clean(&points);
    let input: Vec<_> = points
        .iter()
        .map(|p| [f64::from(p[0]), f64::from(p[1])])
        .collect();
    assert!(!independent_intersects(&input, true));
    // Both candidates retain the same designated corners and closing seam.
    let mut prepared = super::prepare(&clean, f64::from(error), true);
    let first = super::fit_path(&prepared, f64::from(error), true);
    super::protect_close_edges(&mut prepared, f64::from(error) * 0.5, true);
    let half = super::fit_path(&prepared, f64::from(error) * 0.5, true);
    assert!(
        independent_intersects(&sample_steps(&first, 8), true),
        "fixture must reject the first fit"
    );
    assert!(
        !independent_intersects(&sample_steps(&half, 8), true),
        "fixture must accept the half-error fit"
    );
    let commands = closed(&points, error);
    assert_eq!(commands, half, "guard must select the half-tolerance retry");
    assert!(
        cubic_count(&commands) > 0,
        "retry should preserve a fitted curve"
    );
    assert_vertices_within(&points, &commands, f64::from(error) * 0.5);
    let paths = flatten(&commands);
    assert_eq!(paths.len(), 1);
    assert!(
        !independent_intersects(&paths[0].points, true),
        "adaptive flattening must also be simple"
    );
}

#[test]
fn topology_guard_falls_back_to_the_input_polygon_after_two_crossing_fits() {
    let points = [
        [0.83473796, 0.0],
        [3.893268, 3.893268],
        [2.518849e-16, 4.1135926],
        [-2.6385393, 2.6385393],
        [-0.07743218, 9.482707e-18],
        [-0.055554464, -0.055554464],
        [-6.612783e-16, -3.5998313],
        [0.14615715, -0.14615715],
    ];
    let error = 0.01_f32;
    let clean = super::clean(&points);
    let input: Vec<_> = points
        .iter()
        .map(|p| [f64::from(p[0]), f64::from(p[1])])
        .collect();
    for cyclic in [false, true] {
        assert!(!independent_intersects(&input, cyclic));
        let mut prepared = super::prepare(&clean, f64::from(error), cyclic);
        let first = super::fit_path(&prepared, f64::from(error), cyclic);
        assert!(
            independent_intersects(&sample_steps(&first, 8), cyclic),
            "fixture must reject its first fit (cyclic={cyclic})"
        );
        super::protect_close_edges(&mut prepared, f64::from(error) * 0.5, cyclic);
        let half = super::fit_path(&prepared, f64::from(error) * 0.5, cyclic);
        assert!(
            independent_intersects(&sample_steps(&half, 8), cyclic),
            "fixture must also reject its protected half-error fit (cyclic={cyclic})"
        );
        let commands = if cyclic {
            closed(&points, error)
        } else {
            open(&points, error)
        };
        let mut expected = vec![PathCommand::Move(Point::new(points[0][0], points[0][1]))];
        expected.extend(
            points[1..]
                .iter()
                .map(|p| PathCommand::Line(Point::new(p[0], p[1]))),
        );
        if cyclic {
            expected.push(PathCommand::Close);
        }
        assert_eq!(
            commands, expected,
            "guard must return the exact original polygon (cyclic={cyclic})"
        );
        assert_vertices_within(&points, &commands, 0.0);
        let paths = flatten(&commands);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].closed, cyclic);
        assert!(!independent_intersects(&paths[0].points, cyclic));
    }
}

#[test]
fn topology_guard_protects_a_thin_strip_when_unprotected_half_error_still_crosses() {
    // Two matching sampled sine arcs separated by 0.02 drawing units form a
    // simple strip. Independent smooth fits can cross this narrow clearance.
    let mut points: Vec<_> = (0..=4)
        .map(|i| {
            let x = f64::from(i) * 0.5;
            [x as f32, (x.sin() + 0.01) as f32]
        })
        .collect();
    points.extend((0..=4).rev().map(|i| {
        let x = f64::from(i) * 0.5;
        [x as f32, (x.sin() - 0.01) as f32]
    }));
    let error = 0.12_f32;
    let clean = super::clean(&points);
    let input: Vec<_> = points
        .iter()
        .map(|p| [f64::from(p[0]), f64::from(p[1])])
        .collect();
    for cyclic in [false, true] {
        assert!(!independent_intersects(&input, cyclic));
        let mut prepared = super::prepare(&clean, f64::from(error), cyclic);
        for tolerance in [f64::from(error), f64::from(error) * 0.5] {
            let unprotected = super::fit_path(&prepared, tolerance, cyclic);
            assert!(
                independent_intersects(&sample_steps(&unprotected, 8), cyclic),
                "unprotected tolerance {tolerance} must cross the strip (cyclic={cyclic})"
            );
        }
        super::protect_close_edges(&mut prepared, f64::from(error) * 0.5, cyclic);
        let half = super::fit_path(&prepared, f64::from(error) * 0.5, cyclic);
        assert!(
            !independent_intersects(&sample_steps(&half, 8), cyclic),
            "clearance-limited retry must pass the guard (cyclic={cyclic})"
        );
        let commands = if cyclic {
            closed(&points, error)
        } else {
            open(&points, error)
        };
        assert_eq!(
            commands, half,
            "guard must preserve the protected cubic fit (cyclic={cyclic})"
        );
        assert!(cubic_count(&commands) > 0);
        assert_vertices_within(&points, &commands, f64::from(error) * 0.5);
        let paths = flatten(&commands);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].closed, cyclic);
        assert!(
            !independent_intersects(&paths[0].points, cyclic),
            "adaptive flattening must also remain simple (cyclic={cyclic})"
        );
    }
}

#[test]
fn region_drops_small_area_and_keeps_its_neighbour() {
    let neighbour = rectangle(0.0, 0.0, 10.0, 10.0);
    let small = vec![[20.0, 0.0], [20.2, 0.0], [20.0, 0.2]];
    let commands = region(&[neighbour.clone(), small], 0.1, 0.03, 0.0);
    let paths = flatten(&commands);
    assert_eq!(
        paths.len(),
        1,
        "only the small-area contour should be dropped"
    );
    assert!(nonzero_contains(&paths, [5.0, 5.0]));
    assert!(!nonzero_contains(&paths, [20.03, 0.03]));
    assert_vertices_within(&neighbour, &commands, 0.1);
}

#[test]
fn region_drops_narrow_sliver_and_keeps_its_neighbour() {
    let neighbour = rectangle(0.0, 0.0, 10.0, 10.0);
    let narrow = rectangle(20.0, 0.0, 10.0, 0.01);
    let commands = region(&[narrow, neighbour.clone()], 0.1, 0.0, 0.02);
    let paths = flatten(&commands);
    assert_eq!(paths.len(), 1, "only the narrow contour should be dropped");
    assert!(nonzero_contains(&paths, [5.0, 5.0]));
    assert!(!nonzero_contains(&paths, [25.0, 0.005]));
    assert_vertices_within(&neighbour, &commands, 0.1);
}

#[test]
fn region_drops_a_thin_annulus_by_its_net_width() {
    let outer = circle(256, 20.0);
    let mut hole = circle(80, 19.997);
    hole.reverse();
    let commands = region(&[outer, hole], 0.12, 0.05, 0.12);
    assert!(
        commands.is_empty(),
        "large contours can enclose a shape narrower than the width threshold"
    );
}

#[test]
fn visible_thin_annulus_drops_after_shape_filtering() {
    use super::super::visible::{Input, visible};

    const OUTER: u32 = 256;
    const INNER: u32 = 80;
    let positions: Vec<_> = circle(OUTER as usize, 20.0)
        .into_iter()
        .chain(circle(INNER as usize, 19.997))
        .map(|[x, y]| [x, y, 0.0])
        .collect();
    let mut triangles = Vec::new();
    let (mut outer, mut inner) = (0, 0);
    // Connect the rings in angular order, advancing one boundary at a time.
    while outer < OUTER || inner < INNER {
        let a = outer % OUTER;
        let b = OUTER + inner % INNER;
        if (outer + 1) * INNER <= (inner + 1) * OUTER {
            outer += 1;
            triangles.push([a, outer % OUTER, b]);
        } else {
            inner += 1;
            triangles.push([a, OUTER + inner % INNER, b]);
        }
    }
    let front = vec![true; triangles.len()];
    let result = visible(&Input {
        positions: &positions,
        triangles: &triangles,
        positive: &front,
        front: &front,
        lightness: &vec![1.0; positions.len()],
        occluders: &[],
        tones: &[1],
        lines: &[],
        tolerance: 0.03,
    });
    assert_eq!(result.layers.len(), 1);
    assert_eq!(result.layers[0].region.len(), 2);
    assert!(region(&result.layers[0].region, 0.12, 0.05, 0.12).is_empty());
}

#[test]
fn region_drops_a_hole_with_its_sliver_island_inside_a_larger_hole() {
    let outer = rectangle(0.0, 0.0, 200.0, 200.0);
    let mut larger_hole = rectangle(10.0, 10.0, 180.0, 180.0);
    larger_hole.reverse();
    let island = vec![
        [20.0, 20.0],
        [22.0, 20.0],
        [22.0, 20.99],
        [122.0, 20.99],
        [122.0, 21.01],
        [22.0, 21.01],
        [22.0, 22.0],
        [20.0, 22.0],
    ];
    let mut island_hole = rectangle(20.5, 20.5, 1.0, 1.0);
    island_hole.reverse();
    let commands = region(
        &[island_hole, outer.clone(), island, larger_hole.clone()],
        0.12,
        0.05,
        0.12,
    );
    let paths = flatten(&commands);
    assert_eq!(
        paths.len(),
        2,
        "only the outer and larger hole must survive"
    );
    assert!(nonzero_contains(&paths, [5.0, 5.0]));
    assert!(!nonzero_contains(&paths, [21.0, 21.0]));
    assert_vertices_within(&outer, &commands, 0.12);
    assert_vertices_within(&larger_hole, &commands, 0.12);
}

#[test]
fn region_preserves_a_square_hole_after_shape_filtering() {
    let outer = rectangle(0.0, 0.0, 10.0, 10.0);
    let mut hole = rectangle(3.0, 3.0, 4.0, 4.0);
    hole.reverse();
    let commands = region(&[hole.clone(), outer.clone()], 0.12, 0.05, 0.12);
    let paths = flatten(&commands);
    assert_eq!(paths.len(), 2);
    assert!(nonzero_contains(&paths, [1.0, 5.0]));
    assert!(!nonzero_contains(&paths, [5.0, 5.0]));
    assert_vertices_within(&outer, &commands, 0.12);
    assert_vertices_within(&hole, &commands, 0.12);
}

#[test]
fn visible_region_has_counterclockwise_outer_winding() {
    use super::super::visible::{Input, visible};

    let result = visible(&Input {
        positions: &[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0]],
        triangles: &[[0, 1, 2]],
        positive: &[true],
        front: &[true],
        lightness: &[1.0; 3],
        occluders: &[],
        tones: &[1],
        lines: &[],
        tolerance: 0.0,
    });
    assert_eq!(result.layers.len(), 1);
    assert_eq!(result.layers[0].region.len(), 1);
    let outer = &result.layers[0].region[0];
    let twice_area: f64 = outer
        .iter()
        .zip(outer.iter().cycle().skip(1))
        .take(outer.len())
        .map(|(a, b)| f64::from(a[0]) * f64::from(b[1]) - f64::from(a[1]) * f64::from(b[0]))
        .sum();
    assert!(
        twice_area > 0.0,
        "visible outers must have positive signed area"
    );
}

#[test]
fn region_drops_holes_when_sliver_filter_removes_their_outer_contour() {
    // The long thin arm makes 2 * area / perimeter less than 0.12, even
    // though the square contains a hole that passes both sliver thresholds.
    let outer = vec![
        [0.0, 0.0],
        [2.0, 0.0],
        [2.0, 0.99],
        [102.0, 0.99],
        [102.0, 1.01],
        [2.0, 1.01],
        [2.0, 2.0],
        [0.0, 2.0],
    ];
    let mut hole = rectangle(0.5, 0.5, 1.0, 1.0);
    hole.reverse();
    let commands = region(&[outer, hole], 0.12, 0.05, 0.12);
    assert!(
        commands.is_empty(),
        "an orphaned hole would paint a filled island under the nonzero rule"
    );
}

#[test]
fn region_preserves_oppositely_wound_holes_with_nonzero_fill() {
    // Visibility emits holes with the opposite winding to their outer contour.
    let outer = rectangle(0.0, 0.0, 10.0, 10.0);
    let mut hole = rectangle(3.0, 3.0, 4.0, 4.0);
    hole.reverse();
    let commands = region(&[outer.clone(), hole.clone()], 0.1, 0.0, 0.0);
    let paths = flatten(&commands);
    assert_eq!(paths.len(), 2);
    assert!(nonzero_contains(&paths, [1.0, 5.0]));
    assert!(!nonzero_contains(&paths, [5.0, 5.0]));
    assert!(!nonzero_contains(&paths, [15.0, 5.0]));
    assert_vertices_within(&outer, &commands, 0.1);
    assert_vertices_within(&hole, &commands, 0.1);
}

#[test]
fn small_zigzag_on_a_straight_line_needs_only_one_segment() {
    let points: Vec<_> = (0..=200)
        .map(|i| {
            let y = if i == 0 || i == 200 {
                0.0
            } else if i % 2 == 0 {
                0.02
            } else {
                -0.02
            };
            [i as f32 * 0.05, y]
        })
        .collect();
    let commands = open(&points, 0.1);
    assert_eq!(
        segment_count(&commands),
        1,
        "small noise should fit one line or cubic"
    );
    assert_vertices_within(&points, &commands, 0.1);
    let paths = flatten(&commands);
    assert_eq!(paths.len(), 1);
    assert!(!paths[0].closed);
    assert!(distance(paths[0].points[0], [0.0, 0.0]) < 0.000_01);
    assert!(distance(*paths[0].points.last().unwrap(), [10.0, 0.0]) < 0.000_01);
}

#[test]
fn runs_filters_by_length_and_dispatches_open_and_closed_paths() {
    let short = vec![[-10.0, 0.0], [-9.5, 0.0]];
    let long = vec![[0.0, 0.0], [2.0, 0.0], [4.0, 0.0]];
    let mut loop_points = rectangle(10.0, 0.0, 2.0, 2.0);
    loop_points.push(loop_points[0]);
    let commands = runs(&[short, long.clone(), loop_points.clone()], 0.1, 1.0);
    let paths = flatten(&commands);
    assert_eq!(paths.len(), 2);
    assert_eq!(paths.iter().filter(|path| path.closed).count(), 1);
    assert_eq!(paths.iter().filter(|path| !path.closed).count(), 1);
    assert_vertices_within(&long, &commands, 0.1);
    assert_vertices_within(&loop_points, &commands, 0.1);
    assert!(
        paths
            .iter()
            .all(|path| path.points.iter().all(|p| p[0] >= -0.000_01))
    );
}

#[test]
fn empty_and_degenerate_inputs_are_safe_and_finite() {
    assert!(open(&[], 0.1).is_empty());
    assert!(closed(&[], 0.1).is_empty());
    assert!(region(&[], 0.1, 0.0, 0.0).is_empty());
    assert!(runs(&[], 0.1, 0.0).is_empty());
    for points in [
        vec![[1.0, 2.0]],
        vec![[1.0, 2.0]; 8],
        vec![[0.0, 0.0], [0.0, 0.0], [1.0, 0.0], [1.0, 0.0]],
        vec![[0.0, 0.0], [1.0, 0.0], [2.0, 0.0], [0.0, 0.0]],
    ] {
        assert_finite(&open(&points, 0.1));
        assert_finite(&closed(&points, 0.1));
        assert_finite(&region(std::slice::from_ref(&points), 0.1, 0.0, 0.0));
        assert_finite(&runs(std::slice::from_ref(&points), 0.1, 0.0));
    }
    assert_finite(&open(&[[0.0, 0.0], [1.0, 0.0]], 0.0));
}

#[test]
fn topology_open_guard_retries_after_a_crossing_fit() {
    let points = topology_retry_points();
    let error = 3.3029964_f32;
    let input: Vec<_> = points
        .iter()
        .map(|p| [f64::from(p[0]), f64::from(p[1])])
        .collect();
    assert!(!independent_intersects(&input, false));
    let mut prepared = super::prepare(&super::clean(&points), f64::from(error), false);
    let first = super::fit_path(&prepared, f64::from(error), false);
    assert!(independent_intersects(&sample_steps(&first, 8), false));
    super::protect_close_edges(&mut prepared, f64::from(error) * 0.5, false);
    let half = super::fit_path(&prepared, f64::from(error) * 0.5, false);
    assert!(!independent_intersects(&sample_steps(&half, 8), false));
    let commands = open(&points, error);
    assert_eq!(
        commands, half,
        "open guard must select its simple half-error retry"
    );
    assert_vertices_within(&points, &commands, f64::from(error) * 0.5);
    let paths = flatten(&commands);
    assert_eq!(paths.len(), 1);
    assert!(!paths[0].closed);
    assert!(!independent_intersects(&paths[0].points, false));
}

#[test]
fn two_point_open_normalizes_to_an_exact_line() {
    let (a, b) = ([-2.0, 1.0], [3.0, 4.0]);
    let expected = vec![
        PathCommand::Move(Point::new(a[0], a[1])),
        PathCommand::Line(Point::new(b[0], b[1])),
    ];
    assert_eq!(open(&[a, b], 0.1), expected);
    assert_eq!(open(&[a, a, b, b], 0.1), expected);
}

#[test]
fn two_point_span_between_smooth_knots_is_cubic_and_follows_both_tangents() {
    // Unevenly spaced samples force a one-edge subdivision between the two
    // interior knots. Their smooth tangents both differ from that edge's chord.
    let points: Vec<_> = [0.0_f64, 0.3, 0.6, 1.5]
        .into_iter()
        .map(|angle| [(20.0 * angle.cos()) as f32, (20.0 * angle.sin()) as f32])
        .collect();
    let error = 0.12_f32;
    let prepared = super::prepare(&super::clean(&points), f64::from(error), false);
    assert!(prepared.iter().all(|knot| !knot.corner));
    let start = dp(prepared[1].p.point());
    let end = dp(prepared[2].p.point());
    let commands = open(&points, error);
    assert_vertices_within(&points, &commands, f64::from(error));
    let mut cursor = None;
    let mut checked = false;
    for command in commands {
        match command {
            PathCommand::Move(p) => cursor = Some(dp(p)),
            PathCommand::Line(p) => {
                assert!(
                    cursor != Some(start) || dp(p) != end,
                    "a two-point span between smooth knots must remain cubic"
                );
                cursor = Some(dp(p));
            }
            PathCommand::Cubic(b, c, p) => {
                if cursor == Some(start) && dp(p) == end {
                    let left = unit_direction(start, &[dp(b)]);
                    let reverse_right = unit_direction(end, &[dp(c)]);
                    let right = [-reverse_right[0], -reverse_right[1]];
                    for (name, actual, knot) in
                        [("start", left, prepared[1]), ("end", right, prepared[2])]
                    {
                        let tangent = knot.tangent();
                        assert!(
                            distance(actual, [tangent.x, tangent.y]) < 0.000_01,
                            "{name} handle {actual:?} must follow its smooth knot tangent {tangent:?}"
                        );
                    }
                    checked = true;
                }
                cursor = Some(dp(p));
            }
            PathCommand::Close => panic!("an open fit must not close the path"),
        }
    }
    assert!(
        checked,
        "fixture must emit the interior one-edge cubic span"
    );
}

#[test]
fn smooth_closed_seam_starts_at_a_farthest_input_vertex() {
    let mut points = circle(96, 1.0);
    for point in &mut points {
        *point = [3.0 + 9.0 * point[0], -2.0 + 4.0 * point[1]];
    }
    points.rotate_left(7);
    let commands = closed(&points, 0.1);
    let Some(PathCommand::Move(start)) = commands.first() else {
        panic!("missing initial move");
    };
    assert!(
        points.contains(&[start.x, start.y]),
        "seam must interpolate an input vertex"
    );
    let center = points.iter().fold([0.0_f64; 2], |sum, p| {
        [sum[0] + f64::from(p[0]), sum[1] + f64::from(p[1])]
    });
    let center = [
        center[0] / points.len() as f64,
        center[1] / points.len() as f64,
    ];
    let radius2 = |p: [f32; 2]| {
        let delta = [f64::from(p[0]) - center[0], f64::from(p[1]) - center[1]];
        delta[0] * delta[0] + delta[1] * delta[1]
    };
    let maximum = points.iter().map(|&p| radius2(p)).fold(0.0_f64, f64::max);
    assert!((radius2([start.x, start.y]) - maximum).abs() <= 1e-10);
    assert_smooth_closed(&commands);
}

#[test]
fn zero_error_extreme_finite_coordinates_stay_finite() {
    let m = f32::MAX;
    let points = [
        [-m, 0.9 * m],
        [-0.5 * m, 0.95 * m],
        [0.0, m],
        [0.5 * m, 0.995 * m],
        [m, 0.99 * m],
    ];
    let commands = open(&points, 0.0);
    assert!(!commands.is_empty());
    assert_finite(&commands);
}

#[test]
#[ignore = "release-only timing check; run cargo test --release benchmark_10k -- --ignored --nocapture"]
fn benchmark_10k_closed_circle_under_twenty_milliseconds() {
    let points = circle(10_000, 50.0);
    // Warm up allocation/code paths without including fixture construction in
    // the timed interval. black_box keeps the result observable to the optimizer.
    std::hint::black_box(closed(&points, 0.1));
    let started = Instant::now();
    let commands = std::hint::black_box(closed(std::hint::black_box(&points), 0.1));
    let elapsed = started.elapsed();
    eprintln!(
        "10k closed-circle fit: {elapsed:?}, {} cubic segments",
        cubic_count(&commands)
    );
    assert!(!commands.is_empty());
    assert!(
        elapsed < Duration::from_millis(20),
        "10k fit took {elapsed:?}"
    );
}

#[test]
fn uneven_smooth_samples_keep_derivatives_when_an_edge_would_force_a_kink() {
    let points = [
        [0.0, 0.0],
        [1.0, 0.3],
        [4.0, 0.8],
        [4.5, 0.9],
        [6.0, 1.0],
        [6.5, 1.1],
        [8.0, 1.1],
    ];
    let path = open(&points, 0.0);
    assert_vertices_within(&points, &path, 0.0);
    assert!(segment_count(&path) > 1);
    let mut cursor = dp(Point::new(0.0, 0.0));
    let mut previous: Option<DPoint> = None;
    for command in path {
        let (incoming, outgoing, end) = match command {
            PathCommand::Move(p) => {
                cursor = dp(p);
                continue;
            }
            PathCommand::Line(p) => {
                let d = difference(dp(p), cursor);
                (d, d, dp(p))
            }
            PathCommand::Cubic(b, c, p) => {
                let scale = |v: DPoint| [3.0 * v[0], 3.0 * v[1]];
                (
                    scale(difference(dp(b), cursor)),
                    scale(difference(dp(p), dp(c))),
                    dp(p),
                )
            }
            PathCommand::Close => panic!("unexpected close"),
        };
        if let Some(prev) = previous {
            assert!(distance(prev, incoming) < 0.000_005);
        }
        previous = Some(outgoing);
        cursor = end;
    }
}

#[test]
fn a_contour_touching_a_sliver_at_one_vertex_is_not_its_child() {
    // A thin strip ending at the square's first vertex: the strip is a sliver,
    // the square must survive on its own.
    let strip = vec![[-20., -0.0625], [0., -0.0625], [0., 0.], [-20., 0.]];
    let square = vec![[0., 0.], [1., 0.], [1., 1.], [0., 1.]];
    let path = region(&[strip, square], 0.04, 0.05, 0.12);
    let moves = path
        .iter()
        .filter(|c| matches!(c, PathCommand::Move(..)))
        .count();
    assert_eq!(moves, 1, "{path:?}");
}
