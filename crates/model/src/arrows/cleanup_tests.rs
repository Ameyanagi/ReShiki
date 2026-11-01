use super::{ArrowStyle, Head, HeadShape, Preset};
use crate::{
    arrow_anchors::{Anchor, Target},
    document::{Arrow, Point},
};

fn bow(departure: Point, arrival: Point) -> Arrow {
    let mut arrow = Arrow::new(
        17,
        Point::new(0., 0.),
        Point::new(100., 0.),
        Preset::Curved,
        ArrowStyle::default(),
    );
    arrow.cubic = Some([departure, arrival]);
    arrow
}

fn polar(angle: f64, length: f64) -> Point {
    let angle = angle.to_radians();
    Point::new((length * angle.cos()) as f32, (length * angle.sin()) as f32)
}

fn near(a: Point, b: Point) {
    assert!(a.distance(b) < 0.0001, "{a:?} != {b:?}");
}

fn unchanged(arrow: &mut Arrow, operation: fn(&mut Arrow) -> Result<bool, String>) {
    let before = serde_json::to_vec(arrow).unwrap();
    assert!(!operation(arrow).unwrap());
    assert_eq!(serde_json::to_vec(arrow).unwrap(), before);
}

#[test]
fn smooth_bow_averages_mirrored_angles_and_lengths_on_either_side() {
    for side in [-1., 1.] {
        let departure = polar(side * 35., 34.);
        let arrival = polar(side * 45., 46.);
        let mut arrow = bow(departure, Point::new(100. - arrival.x, arrival.y));
        assert!(arrow.smooth_bow().unwrap());
        let expected = polar(side * 40., 40.);
        let [a, b] = arrow.cubic.unwrap();
        near(a, expected);
        near(b, Point::new(100. - expected.x, expected.y));
        assert_eq!(arrow.control, None);
        unchanged(&mut arrow, Arrow::smooth_bow);
    }
}

#[test]
fn smooth_bow_accepts_either_control_on_the_chord() {
    for side in [-1., 1.] {
        for (departure, arrival) in [
            (Point::new(30., 0.), Point::new(70., side * 10.)),
            (Point::new(30., side * 10.), Point::new(70., 0.)),
        ] {
            let mut arrow = bow(departure, arrival);
            assert!(arrow.smooth_bow().unwrap());
            let expected = polar(
                side as f64 * 10_f64.atan2(30.).to_degrees() * 0.5,
                (30. + 30_f64.hypot(10.)) * 0.5,
            );
            let [a, b] = arrow.cubic.unwrap();
            near(a, expected);
            near(b, Point::new(100. - expected.x, expected.y));
            unchanged(&mut arrow, Arrow::smooth_bow);
        }
    }
}

#[test]
fn symmetrise_averages_departure_with_reflected_arrival() {
    let mut arrow = bow(Point::new(32., 36.), Point::new(58., 42.));
    assert!(arrow.symmetrise().unwrap());
    assert_eq!(
        arrow.cubic,
        Some([Point::new(37., 39.), Point::new(63., 39.)])
    );
    assert_eq!(arrow.control, None);
    for i in 0..=20 {
        let point = arrow.point(i as f32 / 20.);
        let opposite = arrow.point(1. - i as f32 / 20.);
        near(point, Point::new(100. - opposite.x, opposite.y));
    }
    unchanged(&mut arrow, Arrow::symmetrise);
}

#[test]
fn smooth_bow_keeps_deliberate_angles_opposite_sides_and_collapsed_controls() {
    let departure = polar(30., 35.);
    for (angle, changes) in [(49.9, true), (50.1, false)] {
        let arrival = polar(angle, 45.);
        let mut arrow = bow(departure, Point::new(100. - arrival.x, arrival.y));
        let before = arrow.clone();
        assert_eq!(arrow.smooth_bow().unwrap(), changes);
        if !changes {
            assert_eq!(arrow, before);
        }
    }
    for (a, b) in [
        (Point::new(30., 20.), Point::new(80., 40.)),
        (Point::new(30., 20.), Point::new(70., -20.)),
        (Point::new(30., 1.), Point::new(70., -1.)),
        (Point::new(30., 0.), Point::new(70., 20.)),
        (Point::new(30., 0.), Point::new(70., 0.)),
        (Point::new(0., 0.), Point::new(70., 20.)),
        (Point::new(30., 20.), Point::new(100., 0.)),
    ] {
        unchanged(&mut bow(a, b), Arrow::smooth_bow);
    }
}

#[test]
fn symmetrise_refuses_s_curves_and_large_same_side_asymmetry_without_editing() {
    for (a, b) in [
        (Point::new(30., 4.), Point::new(70., -4.)),
        (Point::new(20., 30.), Point::new(60., 85.)),
    ] {
        let mut arrow = bow(a, b);
        let before = serde_json::to_vec(&arrow).unwrap();
        assert!(arrow.symmetrise().unwrap_err().contains("asymmetric"));
        assert_eq!(serde_json::to_vec(&arrow).unwrap(), before);
    }
}

#[test]
fn symmetrise_tolerances_follow_chord_length() {
    for scale in [0.01, 1., 100.] {
        for (a, b, accepted) in [
            (Point::new(30., 0.5), Point::new(70., -0.5), true),
            (Point::new(30., 1.1), Point::new(70., -1.1), false),
            (Point::new(20., 30.), Point::new(60.1, 30.), true),
            (Point::new(20., 30.), Point::new(59.9, 30.), false),
        ] {
            let mut arrow = bow(a, b);
            arrow.map_points(|p| Point::new(p.x * scale, p.y * scale));
            let before = arrow.clone();
            if accepted {
                assert!(arrow.symmetrise().unwrap());
            } else {
                assert!(arrow.symmetrise().is_err());
                assert_eq!(arrow, before);
            }
        }
    }
}

#[test]
fn cleanup_preserves_endpoints_anchors_kind_heads_and_style() {
    for operation in [Arrow::smooth_bow, Arrow::symmetrise] {
        for preset in [Preset::Curved, Preset::Fishhook, Preset::Forward] {
            let mut arrow = bow(Point::new(30., 0.), Point::new(70., 10.));
            arrow.kind = preset.kind().into();
            arrow.start_anchor = Some(Anchor {
                target: Target::Atom { atom: 3 },
                offset: Point::new(1., 2.),
                direction: Point::new(0., 1.),
                gap: 1.5,
            });
            arrow.end_anchor = Some(Anchor {
                target: Target::Bond {
                    a: 4,
                    b: 5,
                    fraction: 0.3,
                },
                offset: Point::new(-3., 2.),
                direction: Point::new(1., 0.),
                gap: 2.5,
            });
            let style = arrow.style.as_mut().unwrap();
            style.head = Head::Right;
            style.tail = Head::Left;
            style.shape = HeadShape::Hollow;
            style.width_pt = 1.7;
            let before = arrow.clone();
            assert!(operation(&mut arrow).unwrap());
            assert_eq!(arrow.start, before.start);
            assert_eq!(arrow.end, before.end);
            assert_eq!(arrow.start_anchor, before.start_anchor);
            assert_eq!(arrow.end_anchor, before.end_anchor);
            assert_eq!(arrow.kind, before.kind);
            assert_eq!(arrow.style, before.style);
            assert_eq!(arrow.id, before.id);
            arrow.validate().unwrap();
        }
    }
}

#[test]
fn legacy_bow_cleanup_materializes_cubic_only_when_geometry_changes() {
    for operation in [Arrow::smooth_bow, Arrow::symmetrise] {
        let mut arrow = bow(Point::default(), Point::default());
        arrow.cubic = None;
        arrow.control = Some(Point::new(44., 45.));
        arrow.style = None;
        let before = arrow.clone();
        assert!(operation(&mut arrow).unwrap());
        assert!(arrow.cubic.is_some());
        assert_eq!(arrow.control, None);
        assert_eq!(arrow.style, before.style);
        unchanged(&mut arrow, operation);
    }
}

#[test]
fn cleanup_noops_leave_straight_bent_and_symmetric_encodings_bytewise_unchanged() {
    let mut legacy = Arrow::new(
        1,
        Point::new(5., 10.),
        Point::new(105., 10.),
        Preset::Curved,
        ArrowStyle::default(),
    );
    legacy.style = None;
    let mut explicit = legacy.clone();
    explicit.control = Some(Point::new(55., -30.));
    let mut straight = legacy.clone();
    straight.kind = Preset::Forward.kind().into();
    let mut bent = explicit.clone();
    bent.kind = Preset::Bent.kind().into();
    let mut collapsed = bow(Point::new(0., 10.), Point::new(0., 20.));
    collapsed.end = collapsed.start;
    let symmetric = bow(Point::new(30., -40.), Point::new(70., -40.));
    for mut arrow in [legacy, explicit, straight, bent, collapsed, symmetric] {
        unchanged(&mut arrow, Arrow::smooth_bow);
        unchanged(&mut arrow, Arrow::symmetrise);
    }
}

#[test]
fn cleanup_uses_the_rotated_chord_and_is_idempotent() {
    for operation in [Arrow::smooth_bow, Arrow::symmetrise] {
        for degrees in [0_f64, 31., 90., 177.] {
            let (sin, cos) = degrees.to_radians().sin_cos();
            let transform = |p: Point| {
                Point::new(
                    (13. + f64::from(p.x) * cos - f64::from(p.y) * sin) as f32,
                    (-7. + f64::from(p.x) * sin + f64::from(p.y) * cos) as f32,
                )
            };
            let mut arrow = bow(Point::new(32., 36.), Point::new(58., 42.));
            let mut expected = arrow.clone();
            assert!(operation(&mut expected).unwrap());
            expected.map_points(transform);
            arrow.map_points(transform);
            let endpoints = (arrow.start, arrow.end);
            assert!(operation(&mut arrow).unwrap());
            for (actual, expected) in arrow
                .cubic
                .unwrap()
                .into_iter()
                .zip(expected.cubic.unwrap())
            {
                near(actual, expected);
            }
            assert_eq!((arrow.start, arrow.end), endpoints);
            unchanged(&mut arrow, operation);
        }
    }
}
