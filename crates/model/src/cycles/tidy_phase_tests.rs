use super::{MAX_EVALUATIONS, MAX_POINT_EVALUATIONS, RMS_TOLERANCE, phase, solve};
use crate::{
    cycles::{Cycle, Outline, Shape, outline},
    document::Point,
    graphics::{Graphic, GraphicKind},
};

fn perimeter(shape: Shape, origin: Point, x: Point, y: Point) -> Outline {
    let mut graphic = Graphic::dragged(
        1,
        GraphicKind::Ellipse,
        Point::default(),
        Point::new(200., 40.),
        Default::default(),
        Default::default(),
        false,
    );
    (graphic.origin, graphic.axis_x, graphic.axis_y) = (origin, x, y);
    graphic.cycle = Some(Box::new(Cycle {
        shape,
        ..Default::default()
    }));
    outline(&graphic).unwrap()
}

fn cost(outline: &Outline, centers: &[Option<Point>], dir: f64, phase: f64) -> f64 {
    let mut count = 0;
    let sum: f64 = centers
        .iter()
        .enumerate()
        .filter_map(|(k, center)| {
            let center = (*center)?;
            let point = outline
                .point(phase + dir * k as f64 * outline.length() / centers.len() as f64)
                .unwrap();
            count += 1;
            Some(
                (f64::from(point.x) - f64::from(center.x)).powi(2)
                    + (f64::from(point.y) - f64::from(center.y)).powi(2),
            )
        })
        .sum();
    sum / f64::from(count)
}

fn projected_mean(outline: &Outline, centers: &[Option<Point>], dir: f64) -> f64 {
    let length = outline.length();
    let (mut sin, mut cos) = (0., 0.);
    for (k, center) in centers.iter().enumerate() {
        if let Some(center) = center {
            let seed = outline.project(*center).unwrap().s
                - dir * k as f64 * length / centers.len() as f64;
            let angle = seed / length * std::f64::consts::TAU;
            sin += angle.sin();
            cos += angle.cos();
        }
    }
    (sin.atan2(cos) / std::f64::consts::TAU * length).rem_euclid(length)
}

#[test]
fn sharp_rectangle_minimizes_ink_motion_instead_of_projected_mean() {
    let outline = perimeter(
        Shape::Rectangle,
        Point::default(),
        Point::new(200., 0.),
        Point::new(0., 40.),
    );
    let centers = [Some(Point::new(250., -100.)), Some(Point::new(130., 80.))];
    // With opposite slots, p1 = (200,40)-p0. Completing squares gives
    // the target p0=(160,-70), whose exact boundary minimum is (160,0).
    let answer = solve(&outline, &centers, 1.).unwrap();
    let actual = cost(&outline, &centers, 1., answer.phase);
    let exact = cost(&outline, &centers, 1., 160.);
    let old = cost(
        &outline,
        &centers,
        1.,
        projected_mean(&outline, &centers, 1.),
    );
    assert!(answer.certified, "gap {}", answer.rms_gap);
    assert!(actual.sqrt() - exact.sqrt() <= RMS_TOLERANCE + 0.0001);
    assert!(
        old - actual > 500.,
        "projected mean {old}, optimum {actual}"
    );
    assert!((outline.point(answer.phase).unwrap().x - 160.).abs() < 0.01);
}

#[test]
fn elongated_ellipse_searches_beyond_the_projected_mean_basin() {
    let outline = perimeter(
        Shape::Ellipse,
        Point::default(),
        Point::new(400., 0.),
        Point::new(0., 60.),
    );
    let centers = [Some(Point::new(450., -100.)), Some(Point::new(300., 80.))];
    // Central symmetry again reduces the full two-slot objective to the
    // projection of (275,-60), independently of the phase search.
    let exact = outline.project(Point::new(275., -60.)).unwrap().s;
    let answer = solve(&outline, &centers, 1.).unwrap();
    let actual = cost(&outline, &centers, 1., answer.phase);
    let optimal = cost(&outline, &centers, 1., exact);
    let old = cost(
        &outline,
        &centers,
        1.,
        projected_mean(&outline, &centers, 1.),
    );
    assert!(actual.sqrt() - optimal.sqrt() < 0.02);
    assert!(
        old - actual > 500.,
        "projected mean {old}, optimum {actual}"
    );
    assert!(answer.evaluations <= MAX_EVALUATIONS);
}

#[test]
fn reflected_full_period_seam_and_both_directions_preserve_a_tidy_arrangement() {
    for shape in [Shape::Ellipse, Shape::Rectangle] {
        let outline = perimeter(
            shape,
            Point::new(220., -40.),
            Point::new(-200., 30.),
            Point::new(16., 70.),
        );
        for dir in [-1., 1.] {
            let wanted = outline.length() - 0.017;
            let centers: Vec<_> = (0..5)
                .map(|k| {
                    (!matches!(k, 1 | 3)).then(|| {
                        outline
                            .point(wanted + dir * k as f64 * outline.length() / 5.)
                            .unwrap()
                    })
                })
                .collect();
            let first = phase(&outline, &centers, dir).unwrap();
            let second = phase(&outline, &centers, dir).unwrap();
            assert_eq!(first, second);
            assert!((0. ..outline.length()).contains(&first));
            // The caller's 0.05-world no-move policy must be able to retain
            // every occupied slot, including across the canonical seam.
            for (k, center) in centers.iter().enumerate() {
                if let Some(center) = center {
                    let actual = outline
                        .point(first + dir * k as f64 * outline.length() / 5.)
                        .unwrap();
                    assert!(actual.distance(*center) < 0.05, "{shape:?}, dir {dir}");
                }
            }
            let next_centers: Vec<_> = centers
                .iter()
                .enumerate()
                .map(|(k, center)| {
                    center.map(|_| {
                        outline
                            .point(first + dir * k as f64 * outline.length() / 5.)
                            .unwrap()
                    })
                })
                .collect();
            let next = phase(&outline, &next_centers, dir).unwrap();
            assert!(cost(&outline, &next_centers, dir, next).sqrt() < 0.005);
        }
    }
}

#[test]
fn a_single_observed_part_keeps_its_missing_slot_offset() {
    let outline = perimeter(
        Shape::Rectangle,
        Point::default(),
        Point::new(200., 0.),
        Point::new(0., 40.),
    );
    let center = Point::new(135., -20.);
    let centers = [None, None, None, Some(center), None];
    for dir in [-1., 1.] {
        let expected = (outline.project(center).unwrap().s
            - dir * 3. * outline.length() / centers.len() as f64)
            .rem_euclid(outline.length());
        let answer = solve(&outline, &centers, dir).unwrap();
        let difference = (answer.phase - expected).abs();
        assert!(difference.min(outline.length() - difference) < 0.01);
        assert!(answer.evaluations > 0);
    }
    let exact = solve(&outline, &[Some(center)], 1.).unwrap();
    assert_eq!(exact.phase, outline.project(center).unwrap().s);
    assert!(exact.certified);
    assert_eq!(exact.evaluations, 0);
    assert!(phase(&outline, &[None; 5], 1.).is_none());
    assert!(phase(&outline, &centers, 0.).is_none());
}

#[test]
fn a_missing_slot_offset_does_not_reuse_the_projection_tie_order() {
    let outline = perimeter(
        Shape::Rectangle,
        Point::default(),
        Point::new(100., 0.),
        Point::new(0., 100.),
    );
    let centers = [None, Some(Point::new(50., 50.)), None, None];
    // Project(center) chooses s=50. Subtracting slot one's offset chooses
    // phase=350, while phases 50/150/250/350 have exactly equal cost.
    assert_eq!(phase(&outline, &centers, 1.).unwrap(), 50.);
    assert_eq!(phase(&outline, &centers, -1.).unwrap(), 50.);
}

#[test]
fn large_coordinate_rounding_does_not_get_a_false_projection_certificate() {
    let outline = perimeter(
        Shape::Rectangle,
        Point::new(100_000_000., 0.),
        Point::new(80., 80.),
        Point::new(-16., 16.),
    );
    let target = Point::new(100_000_008., 0.);
    let centers = [Some(target)];
    // The continuous projection lies on an f32 x rounding boundary. Its
    // rounded point is farther away than a point just beyond that boundary.
    let projected = outline.project(target).unwrap().s;
    let answer = solve(&outline, &centers, 1.).unwrap();
    assert!(answer.evaluations > 0);
    assert!(!answer.certified);
    assert!(answer.rms_gap > RMS_TOLERANCE);
    assert!(cost(&outline, &centers, 1., answer.phase).sqrt() < 5.);
    assert!(cost(&outline, &centers, 1., answer.phase) < cost(&outline, &centers, 1., projected));
}

#[test]
fn exact_equal_minima_choose_the_lowest_canonical_phase() {
    let outline = perimeter(
        Shape::Rectangle,
        Point::default(),
        Point::new(100., 0.),
        Point::new(0., 100.),
    );
    let centers = [Some(Point::new(50., 50.)); 2];
    // The four side midpoints are equal global minima. The first is s=50.
    assert_eq!(phase(&outline, &centers, 1.).unwrap(), 50.);
    assert_eq!(phase(&outline, &centers, -1.).unwrap(), 50.);
}

#[test]
fn dense_difficult_input_returns_a_deterministic_result_with_bounded_work() {
    let outline = perimeter(
        Shape::Ellipse,
        Point::new(100_000., -100_000.),
        Point::new(400., 0.),
        Point::new(0., 60.),
    );
    let centers: Vec<_> = (0..crate::cycles::MAX_PARTS)
        .map(|k| {
            Some(Point::new(
                100_000. + (k % 7) as f32 * 90.,
                -100_000. + (k % 11) as f32 * 20.,
            ))
        })
        .collect();
    let first = solve(&outline, &centers, 1.).unwrap();
    let second = solve(&outline, &centers, 1.).unwrap();
    assert_eq!(first.phase, second.phase);
    assert_eq!(first.certified, second.certified);
    assert_eq!(first.evaluations, second.evaluations);
    assert!(first.evaluations * centers.len() <= MAX_POINT_EVALUATIONS);
    assert!(first.evaluations <= MAX_EVALUATIONS);
    assert!((0. ..outline.length()).contains(&first.phase));
    assert!(first.rms_gap.is_finite());
    assert!(cost(&outline, &centers, 1., first.phase) <= cost(&outline, &centers, 1., 0.));
}
