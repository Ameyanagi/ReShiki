use super::*;

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

fn contacts(balls: &[Ball], project: impl Fn(V) -> Point + Sync) -> Contacts {
    super::contacts(balls, None, STEPS, project)
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
fn a_front_hydrogens_cap_fills_the_hole_in_its_carbon() {
    let balls = [ball([0., 0., 0.], 1.7), ball([0., 0., 1.09], 1.1)];
    let found = contacts(&balls, flat);
    assert!(inside(region(&found.seen[1]), Point::default()));
    assert!(!inside(region(&found.seen[0]), Point::default()));
    // The seam is a full circle, and the hydrogen shows its inside rather
    // than the thin belt outside it.
    let Meeting::Circle(meeting) = meeting(balls[1], balls[0]) else {
        panic!("the C-H spheres intersect");
    };
    for angle in sample(0., TAU, 16.) {
        let p = Point::new(
            0.9 * meeting.ring.r * angle.cos(),
            0.9 * meeting.ring.r * angle.sin(),
        );
        assert!(inside(region(&found.seen[1]), p), "missing H cap at {p:?}");
        assert!(!inside(region(&found.seen[0]), p), "C covers H at {p:?}");
    }
    assert!(matches!(found.seen[1], Some(Seen::Shown { ref rim, .. }) if rim.is_empty()));
    assert_eq!(found.seams.len(), 1);
}

#[test]
fn a_meeting_circle_on_the_rim_keeps_the_front_hemisphere() {
    // The 3-4-5 geometry puts the meeting plane exactly through the small
    // ball's centre (h=0). Its front is outside the big ball, its back inside.
    let balls = [ball([0., 0., 0.], 3.), ball([0., 0., -4.], 5.)];
    let found = contacts(&balls, flat);
    assert!(inside(region(&found.seen[0]), Point::default()));
    for angle in sample(0., TAU, 16.) {
        assert!(inside(
            region(&found.seen[0]),
            Point::new(2.9 * angle.cos(), 2.9 * angle.sin())
        ));
    }
    assert!(!inside(region(&found.seen[1]), Point::default()));
}

/// A sight ray's first intersection with a sphere, independent of the
/// meeting-circle clipping code. `p` lies in the z=0 image plane.
fn front_surface(a: Ball, p: Point, eye: Option<f32>) -> Option<V> {
    let x = p.x;
    let y = -p.y;
    if let Some(z) = eye {
        let origin = [0., 0., z];
        let direction = unit([x, y, -z]);
        let offset = sub(origin, a.p);
        let b = dot(offset, direction);
        let discriminant = b * b - dot(offset, offset) + a.r * a.r;
        (discriminant > 0.).then(|| add(origin, mul(direction, -b - discriminant.sqrt())))
    } else {
        let discriminant = a.r * a.r - (x - a.p[0]).powi(2) - (y - a.p[1]).powi(2);
        (discriminant > 0.).then(|| [x, y, a.p[2] + discriminant.sqrt()])
    }
}

fn assert_visible_surfaces(balls: &[Ball], eye: Option<f32>) {
    let project = |q: V| {
        let scale = eye.map_or(1., |z| z / (z - q[2]));
        Point::new(q[0] * scale, -q[1] * scale)
    };
    let found = super::contacts(balls, eye, STEPS, project);
    let mut visible = 0;
    for (i, &a) in balls.iter().enumerate() {
        let facing = Facing::new(a, eye);
        for gx in -48..=48 {
            for gy in -48..=48 {
                let p = Point::new(gx as f32 * 0.05, gy as f32 * 0.05);
                let Some(surface) = front_surface(a, p, eye) else {
                    continue;
                };
                // Polygonal circles approximate their analytic boundaries;
                // compare interior samples, clear of rims and seams.
                if dot(sub(surface, a.p), facing.v) - facing.s < 0.15 {
                    continue;
                }
                let clearance = balls
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| *j != i)
                    .map(|(_, b)| len(sub(surface, b.p)) - b.r)
                    .fold(f32::INFINITY, f32::min);
                if clearance.abs() < 0.02 {
                    continue;
                }
                let expected = clearance > 0.;
                let shown = match &found.seen[i] {
                    None => true,
                    Some(Seen::Hidden) => false,
                    Some(Seen::Shown { region, .. }) => inside(region, p),
                };
                assert_eq!(
                    shown, expected,
                    "ball {i}, image {p:?}, surface {surface:?}, eye {eye:?}, clearance {clearance}, balls {balls:?}"
                );
                visible += usize::from(shown);
            }
        }
    }
    assert!(visible > 1_000, "check a substantial visible region");
}

#[test]
fn near_axis_hydrogen_caps_keep_every_visible_surface_in_both_projections() {
    for eye in [None, Some(8.)] {
        for degrees in [0_f32, 5., 11., 11.7, 11.9, 15., 30., 90., 180.] {
            for azimuth in [0_f32, 45., 140.] {
                let angle = degrees.to_radians();
                let phi = azimuth.to_radians();
                let centre = [-0.4, 0.3, 0.1];
                let h = add(
                    centre,
                    [
                        1.09 * angle.sin() * phi.cos(),
                        1.09 * angle.sin() * phi.sin(),
                        1.09 * angle.cos(),
                    ],
                );
                let pair = [ball(centre, 1.7), ball(h, 1.1)];
                assert_visible_surfaces(&pair, eye);
                // A third intersecting sphere also cuts the cap, so the
                // complement's hole must compose correctly with other cuts.
                assert_visible_surfaces(&[pair[0], pair[1], ball([0.6, 0.6, 1.4], 0.85)], eye);
            }
        }
    }
}

#[test]
fn turning_contacts_keep_seam_ends_in_both_projections() {
    for eye in [None, Some(8.)] {
        let project = |q: V| {
            let scale = eye.map_or(1., |z| z / (z - q[2]));
            Point::new(q[0] * scale, -q[1] * scale)
        };
        for degrees in [0_f32, 5., 11.9, 30., 90.] {
            let angle = degrees.to_radians();
            let pair = [
                ball([0., 0., 0.], 1.7),
                ball([1.09 * angle.sin(), 0., 1.09 * angle.cos()], 1.1),
            ];
            let triple = [pair[0], pair[1], ball([0.6, 0.6, 1.4], 0.85)];
            for balls in [&pair[..], &triple[..]] {
                let still = super::contacts(balls, eye, STEPS, project);
                let turning = super::contacts(balls, eye, TURNING_STEPS, project);
                for (a, b) in still.seen.iter().zip(&turning.seen) {
                    assert_eq!(
                        matches!(a, Some(Seen::Shown { .. })),
                        matches!(b, Some(Seen::Shown { .. }))
                    );
                }
                assert!(!still.seams.is_empty());
                assert_eq!(still.seams.len(), turning.seams.len());
                let mut fewer = false;
                for ((pair, runs), (other_pair, other_runs)) in
                    still.seams.iter().zip(&turning.seams)
                {
                    assert_eq!(pair, other_pair);
                    assert_eq!(runs.len(), other_runs.len());
                    for (run, other) in runs.iter().zip(other_runs) {
                        assert!(other.len() <= run.len());
                        fewer |= other.len() < run.len();
                        assert!(run[0].distance(other[0]) < 1e-5);
                        assert!(run.last().unwrap().distance(*other.last().unwrap()) < 1e-5);
                    }
                }
                assert!(fewer);
            }
        }
    }
}

#[test]
fn a_ball_touching_nothing_has_no_contact() {
    let found = contacts(&[ball([0., 0., 0.], 1.7), ball([4., 0., 0.], 1.2)], flat);
    assert_eq!(found.seen, [None, None]);
    // Nor do balls that nearly touch: there is no contact shadow.
    let near = contacts(&[ball([0., 0., 0.], 1.5), ball([3.2, 0., 0.], 1.5)], flat);
    assert_eq!(near.seen, [None, None]);
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

/// Chunks and reused buffers keep the exact contours in ball order.
#[test]
fn many_balls_keep_the_serial_contact_order() {
    for degrees in [0_f32, 11.9, 30., 90.] {
        let (s, c) = degrees.to_radians().sin_cos();
        let balls: Vec<_> = (0..97)
            .map(|i| {
                let [x, y, z] = [i % 6, i / 6 % 4, i / 24].map(|q| q as f32 * 1.6 - 3.);
                Ball {
                    p: [c * x + s * z, y, c * z - s * x],
                    r: [1.7, 1.2, 1.5][i % 3],
                    fading: i % 5 == 0,
                }
            })
            .collect();
        for eye in [None, Some(20.)] {
            let project = |q: V| {
                let scale = eye.map_or(1., |z| z / (z - q[2]));
                Point::new(17. + q[0] * scale, 29. - q[1] * scale)
            };
            let near = neighbours(&balls);
            for steps in [STEPS, TURNING_STEPS] {
                let found = super::contacts(&balls, eye, steps, project);
                let serial: Vec<_> = balls
                    .iter()
                    .zip(&near)
                    .map(|(&a, near)| {
                        (!near.is_empty()).then(|| {
                            seen(
                                a,
                                &Facing::new(a, eye),
                                near,
                                &balls,
                                steps,
                                &project,
                                &mut FloatOverlay::new_empty(
                                    Default::default(),
                                    Default::default(),
                                    0,
                                ),
                            )
                        })
                    })
                    .collect();
                assert_eq!(found.seen, serial, "{degrees}, {eye:?}, {steps}");
            }
        }
    }
}
