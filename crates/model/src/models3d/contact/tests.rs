use super::*;
use crate::graphics::PathCommand;

fn ball(p: V, r: f32) -> Ball {
    Ball {
        p,
        r,
        fading: false,
    }
}

/// Orthographic, one drawing unit per ångström, y down.
fn flat(q: V) -> Point {
    Point::new(q[0], -q[1])
}

fn inside(polygons: &[Vec<Point>], p: Point) -> bool {
    let mut odd = false;
    for polygon in polygons {
        let n = polygon.len();
        for k in 0..n {
            let (a, b) = (polygon[k], polygon[(k + 1) % n]);
            if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
                odd = !odd;
            }
        }
    }
    odd
}

fn region(seen: &Option<Seen>) -> &[Vec<Point>] {
    match seen {
        Some(Seen::Shown { region, .. }) => region,
        other => panic!("expected a shown ball, got {other:?}"),
    }
}

/// The region is the whole disc: one contour, all on the rim.
fn whole(seen: &Option<Seen>) -> bool {
    matches!(seen, Some(Seen::Shown { region, .. })
    if region.len() == 1 && {
        let centre = region[0].iter().fold(Point::default(), |c, p| c.offset(p.x, p.y));
        let n = region[0].len() as f32;
        let centre = Point::new(centre.x / n, centre.y / n);
        let r = region[0][0].distance(centre);
        region[0].iter().all(|p| (p.distance(centre) - r).abs() < 1e-3 * r)
    })
}

fn cut(seen: &Option<Seen>) -> bool {
    matches!(seen, Some(Seen::Shown { .. })) && !whole(seen)
}

fn contacts(balls: &[Ball], project: impl Fn(V) -> Point) -> Contacts {
    super::contacts(balls, None, project)
}

/// The region's boundary points off the rim: the cut.
fn cut_points(seen: &Option<Seen>, centre: Point, r: f32) -> Vec<Point> {
    region(seen)
        .iter()
        .flatten()
        .copied()
        .filter(|p| p.distance(centre) < r * 0.99)
        .collect()
}

#[test]
fn a_region_leaves_out_every_point_whose_front_surface_is_inside_a_neighbour() {
    let balls = [
        ball([0., 0., 0.], 1.7),
        ball([1.4, 0.5, 0.8], 1.2),
        ball([-0.6, -1.5, -0.4], 1.5),
    ];
    let found = contacts(&balls, flat);
    for (i, a) in balls.iter().enumerate() {
        let polygons = region(&found.seen[i]);
        let mut shown = 0;
        for gx in -40..=40 {
            for gy in -40..=40 {
                let (x, y) = (a.p[0] + gx as f32 * 0.05, a.p[1] + gy as f32 * 0.05);
                let q = a.r * a.r - (x - a.p[0]).powi(2) - (y - a.p[1]).powi(2);
                if q <= 0.01 || !inside(polygons, Point::new(x, -y)) {
                    continue;
                }
                shown += 1;
                let surface = [x, y, a.p[2] + q.sqrt()];
                for (j, b) in balls.iter().enumerate() {
                    if j != i {
                        assert!(
                            len(sub(surface, b.p)) >= b.r - 0.02,
                            "ball {i} shows a point inside ball {j}"
                        );
                    }
                }
            }
        }
        assert!(shown > 50, "ball {i} shows only {shown} points");
    }
}

#[test]
fn a_ball_inside_another_is_hidden_and_the_other_whole() {
    let balls = [ball([0., 0., 0.], 1.7), ball([0.3, 0.2, 0.1], 1.1)];
    let found = contacts(&balls, flat);
    assert!(whole(&found.seen[0]));
    assert_eq!(found.seen[1], Some(Seen::Hidden));
    assert!(found.seams.is_empty());
}

#[test]
fn a_ball_covered_from_the_front_is_hidden() {
    // The big ball's front cap holds all of the small one's front.
    let balls = [ball([0., 0., -1.], 1.2), ball([0., 0., 0.], 1.7)];
    let found = contacts(&balls, flat);
    assert_eq!(found.seen[0], Some(Seen::Hidden));
}

#[test]
fn a_ball_touching_nothing_has_no_contact() {
    let found = contacts(&[ball([0., 0., 0.], 1.7), ball([4., 0., 0.], 1.2)], flat);
    assert_eq!(found.seen, [None, None]);
}

#[test]
fn balls_side_by_side_meet_in_a_straight_cut_and_seam() {
    let balls = [ball([0., 0., 0.], 1.7), ball([2., 0., 0.], 1.2)];
    let found = contacts(&balls, flat);
    // The meeting plane x = h, seen edge on.
    let h = (4. + 1.7f32.powi(2) - 1.2f32.powi(2)) / 4.;
    let rho = (1.7f32.powi(2) - h * h).sqrt();
    for (i, seen) in found.seen.iter().enumerate() {
        let points = cut_points(seen, flat(balls[i].p), balls[i].r);
        assert!(
            points.iter().all(|p| (p.x - h).abs() < 1e-3),
            "ball {i}: {points:?}"
        );
        // The cut runs straight from rim to rim.
        let on_line: Vec<f32> = region(seen)
            .iter()
            .flatten()
            .filter(|p| (p.x - h).abs() < 1e-3)
            .map(|p| p.y)
            .collect();
        let top = on_line.iter().fold(f32::MIN, |m, y| m.max(*y));
        let bottom = on_line.iter().fold(f32::MAX, |m, y| m.min(*y));
        // Within the sagitta of the disc's polygon.
        assert!(
            (top - bottom - 2. * rho).abs() < 1e-2,
            "ball {i}: {on_line:?}"
        );
    }
    let [((0, 1), runs)] = found.seams.as_slice() else {
        panic!("one seam expected: {:?}", found.seams);
    };
    let run = runs.concat();
    assert!(run.iter().all(|p| (p.x - h).abs() < 1e-3));
    // Its visible run spans the front half, rim to rim.
    let ys: Vec<f32> = run.iter().map(|p| p.y).collect();
    let (lo, hi) = ys
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), y| (lo.min(*y), hi.max(*y)));
    assert!((lo + rho).abs() < 1e-3 && (hi - rho).abs() < 1e-3);
}

#[test]
fn a_ball_in_front_of_another_meets_it_in_a_curve() {
    let balls = [ball([0., 0., -0.5], 1.7), ball([1.2, 0., 0.6], 1.2)];
    let found = contacts(&balls, flat);
    let spread = |points: &[Point]| {
        let xs = points.iter().map(|p| p.x);
        xs.clone().fold(f32::MIN, f32::max) - xs.fold(f32::MAX, f32::min)
    };
    for (i, seen) in found.seen.iter().enumerate() {
        let points = cut_points(seen, flat(balls[i].p), balls[i].r);
        assert!(
            points.len() > 10 && spread(&points) > 0.2,
            "ball {i}: {points:?}"
        );
    }
    // The seam is an arc of the meeting circle's ellipse, curved on screen.
    let [((0, 1), runs)] = found.seams.as_slice() else {
        panic!("one seam expected: {:?}", found.seams);
    };
    assert!(spread(&runs.concat()) > 0.2);
}

#[test]
fn a_ball_wholly_in_front_cuts_the_one_behind_without_a_seam() {
    // Its whole front surface is outside the ball behind: its rim, not a
    // seam, is the edge between them.
    let balls = [ball([0., 0., -1.], 1.7), ball([1.2, 0., 0.8], 1.2)];
    let found = contacts(&balls, flat);
    assert!(cut(&found.seen[0]));
    assert!(whole(&found.seen[1]));
    assert!(found.seams.is_empty());
}

#[test]
fn a_seam_hidden_inside_a_third_ball_is_left_out() {
    let pair = [ball([0., 0., 0.], 1.5), ball([2., 0., 0.], 1.5)];
    let alone = contacts(&pair, flat).seams;
    let covered = contacts(&[pair[0], pair[1], ball([1., 0., 1.], 1.4)], flat).seams;
    let length = |seams: &[((usize, usize), Runs)], pair| {
        seams
            .iter()
            .filter(|(p, _)| *p == pair)
            .flat_map(|(_, runs)| runs)
            .map(|run| run.windows(2).map(|w| w[0].distance(w[1])).sum::<f32>())
            .sum::<f32>()
    };
    assert!(length(&covered, (0, 1)) < 0.5 * length(&alone, (0, 1)));
}

#[test]
fn a_fading_ball_cuts_only_fading_balls() {
    let mut balls = [ball([0., 0., 0.], 1.7), ball([1.5, 0., 0.], 1.1)];
    balls[1].fading = true;
    let found = contacts(&balls, flat);
    assert_eq!(found.seen[0], None);
    assert!(cut(&found.seen[1]));
    assert!(found.seams.is_empty());
}

#[test]
fn free_arcs_take_out_overlapping_and_wrapping_spans() {
    assert_eq!(free(&[]), [(0., TAU)]);
    let arcs = free(&[(1., 0.5), (1.4, 0.5), (TAU - 0.2, 0.4)]);
    let expected = [(1.9, TAU - 0.6), (TAU + 0.2, TAU + 0.5)];
    assert_eq!(arcs.len(), 2, "{arcs:?}");
    for ((a0, a1), (e0, e1)) in arcs.iter().zip(expected) {
        assert!((a0 - e0).abs() < 1e-5 && (a1 - e1).abs() < 1e-5, "{arcs:?}");
    }
    assert!(free(&[(0., PI)]).is_empty());
}

#[test]
fn the_shadow_follows_where_the_surface_comes_within_each_gap_of_a_neighbour() {
    let (d, ra, rb) = (2f32, 1.7f32, 1.2f32);
    let balls = [ball([0., 0., 0.], ra), ball([d, 0., 0.], rb)];
    let found = contacts(&balls, flat);
    let Some(Seen::Shown { shadow, .. }) = &found.seen[0] else {
        panic!("expected a shown ball");
    };
    assert_eq!(shadow.len(), SHADOW.len());
    let mut last = d;
    for ((gap, _), shape) in SHADOW.iter().zip(shadow) {
        // Seen edge on, the surface within `gap` of the other ball is beyond
        // the plane where the grown ball meets this one.
        let h = (d * d + ra * ra - (rb + gap).powi(2)) / (2. * d);
        let ends: Vec<Point> = shape
            .iter()
            .filter_map(|c| match c {
                PathCommand::Move(p) | PathCommand::Cubic(_, _, p) => Some(*p),
                _ => None,
            })
            .collect();
        assert_eq!(
            shape
                .iter()
                .filter(|c| matches!(c, PathCommand::Move(_)))
                .count(),
            1
        );
        let inner: Vec<_> = ends
            .iter()
            .filter(|p| p.distance(Point::default()) < ra * 0.99)
            .collect();
        assert!(inner.iter().all(|p| (p.x - h).abs() < 1e-3), "{inner:?}");
        // Its ends are on the rim, where the plane meets it.
        let rho = (ra * ra - h * h).sqrt();
        assert!(
            ends.iter()
                .any(|p| (p.x - h).abs() < 1e-3 && (p.y - rho).abs() < 1e-3)
        );
        // Each wider gap reaches farther from the seam.
        assert!(h < last);
        last = h;
    }
}
