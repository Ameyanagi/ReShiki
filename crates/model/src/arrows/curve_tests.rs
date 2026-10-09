//! Retained legacy geometry from the #91 before fixture.
use crate::arrows::{ArrowStyle, Head, HeadShape, Preset};
use crate::document::Arrow;
use crate::document::{Document, Point};
use crate::graphics::PathCommand;

fn near(a: Point, b: Point) {
    assert!(a.distance(b) < 0.0001, "{a:?} != {b:?}");
}

fn cubic() -> Arrow {
    let mut arrow = Arrow::new(
        1,
        Point::new(0., 0.),
        Point::new(120., 0.),
        Preset::Curved,
        ArrowStyle::default(),
    );
    arrow.control = None;
    arrow.cubic = Some([Point::new(0., -80.), Point::new(120., 80.)]);
    arrow
}

#[test]
fn mechanism_curvature_91_legacy_fixture_retains_quadratic_render_and_native_data() {
    let bytes = include_bytes!("../../../../tests/fixtures/mechanism-curvature-91/before.rsk");
    let doc = Document::from_native_file(bytes).unwrap();
    assert_eq!(doc.arrows.len(), 2);
    for arrow in &doc.arrows {
        let control = arrow.control.unwrap();
        let midpoint = Point::new(
            (arrow.start.x + 2. * control.x + arrow.end.x) / 4.,
            (arrow.start.y + 2. * control.y + arrow.end.y) / 4.,
        );
        assert_eq!(arrow.point(0.5), midpoint);
        assert_eq!(arrow.handles()[2], midpoint);
        assert!(arrow.hit(midpoint, 2.));
        let (lo, hi) = arrow.bounds();
        assert!(lo.x <= midpoint.x && midpoint.x <= hi.x);
        assert!(lo.y <= midpoint.y && midpoint.y <= hi.y);
        let mut edited = arrow.clone();
        edited.edit_handle(2, midpoint.offset(0., -10.));
        assert_eq!((edited.start, edited.end), (arrow.start, arrow.end));
        assert_eq!(edited.point(0.5), midpoint.offset(0., -10.));
    }
    let saved = serde_json::to_vec(&doc).unwrap();
    assert_eq!(Document::from_native_file(&saved).unwrap(), doc);
}

#[test]
fn independent_handles_preserve_endpoints_opposite_tangent_and_legacy_curve() {
    let mut arrow = Arrow::new(
        1,
        Point::new(0., 0.),
        Point::new(120., 0.),
        Preset::Fishhook,
        ArrowStyle::preset(Preset::Fishhook),
    );
    arrow.control = Some(Point::new(50., -90.));
    let legacy = arrow.clone();
    let [departure, arrival] = arrow.bezier_controls().unwrap();
    assert_eq!(arrow.handles().len(), 5);
    assert_eq!(arrow.handle_at(departure, 8.), Some(3));
    assert_eq!(arrow.handle_at(arrival, 8.), Some(4));
    arrow.edit_handle(3, departure);
    assert_eq!(arrow.control, None);
    for i in 0..=20 {
        near(arrow.point(i as f32 / 20.), legacy.point(i as f32 / 20.));
    }
    let end_tangent = arrow.velocity(1.);
    arrow.edit_handle(3, Point::new(-10., -50.));
    assert_eq!((arrow.start, arrow.end), (legacy.start, legacy.end));
    assert_eq!(arrow.cubic.unwrap()[1], arrival);
    assert_eq!(arrow.velocity(1.), end_tangent);
    let start_tangent = arrow.velocity(0.);
    arrow.edit_handle(4, Point::new(135., -10.));
    assert_eq!(arrow.velocity(0.), start_tangent);
    assert_eq!(arrow.appearance(), legacy.appearance());
    arrow.edit_handle(2, Point::new(50., -25.));
    near(arrow.point(0.5), Point::new(50., -25.));
    assert_eq!((arrow.start, arrow.end), (legacy.start, legacy.end));
    arrow.validate().unwrap();
}

#[test]
fn cubic_head_tangents_reverse_flip_straighten_and_degenerate_controls() {
    let mut arrow = cubic();
    arrow.style.as_mut().unwrap().shape = HeadShape::Open;
    arrow.style.as_mut().unwrap().tail = Head::Full;
    let paths = arrow.paths();
    for (path, direction, tip) in [
        (&paths[1], Point::new(0., -1.), arrow.end),
        (&paths[2], Point::new(0., 1.), arrow.start),
    ] {
        let [
            PathCommand::Move(a),
            PathCommand::Line(actual_tip),
            PathCommand::Line(b),
        ] = path.commands.as_slice()
        else {
            panic!("Open full head");
        };
        near(*actual_tip, tip);
        let center = Point::new((a.x + b.x) / 2., (a.y + b.y) / 2.);
        let length = crate::style::DEFAULT.world(arrow.appearance().head_length_pt);
        near(
            center,
            tip.offset(-direction.x * length, -direction.y * length),
        );
    }
    let original = arrow.clone();
    arrow.reverse();
    for i in 0..=20 {
        near(
            arrow.point(i as f32 / 20.),
            original.point(1. - i as f32 / 20.),
        );
    }
    arrow.reverse();
    assert_eq!(arrow, original);
    arrow.flip_bend();
    for i in 0..=20 {
        let p = original.point(i as f32 / 20.);
        near(arrow.point(i as f32 / 20.), Point::new(p.x, -p.y));
    }
    arrow.straighten();
    assert_eq!(arrow.cubic, None);
    near(arrow.point(0.25), Point::new(30., 0.));
    let mut collapsed = cubic();
    collapsed.cubic = Some([collapsed.start, collapsed.end]);
    for path in collapsed.paths() {
        assert!(
            path.commands
                .iter()
                .flat_map(PathCommand::points)
                .all(|p| p.x.is_finite() && p.y.is_finite())
        );
    }
}

#[test]
fn cubic_endpoints_move_adjacent_controls_and_transforms_native_copy_preserve_shape() {
    let mut arrow = cubic();
    arrow.edit_handle(0, Point::new(-10., 20.));
    assert_eq!(
        arrow.cubic,
        Some([Point::new(-10., -60.), Point::new(120., 80.)])
    );
    arrow.edit_handle(1, Point::new(140., -20.));
    assert_eq!(
        arrow.cubic,
        Some([Point::new(-10., -60.), Point::new(140., 60.)])
    );
    let mut doc = Document {
        arrows: vec![arrow.clone()],
        ..Document::default()
    };
    let copied = crate::editing::append(
        &mut doc,
        &Document {
            arrows: vec![arrow.clone()],
            ..Document::default()
        },
        Point::new(160., 10.),
    );
    assert_eq!(
        doc.arrows[1].cubic,
        Some([Point::new(150., -50.), Point::new(300., 70.)])
    );
    crate::editing::transform_about(&mut doc, &copied, Point::new(160., 10.), 2., 90.);
    for i in 0..=20 {
        let p = arrow.point(i as f32 / 20.);
        near(
            doc.arrows[1].point(i as f32 / 20.),
            Point::new(160. - 2. * p.y, 10. + 2. * p.x),
        );
    }
    let bytes = doc.file_json().unwrap();
    assert_eq!(Document::from_native_file(&bytes).unwrap(), doc.current());
    let before = doc.clone();
    doc.arrows[0].edit_handle(3, Point::new(f32::NAN, 0.));
    assert_eq!(doc, before);
    doc.arrows[0].cubic.as_mut().unwrap()[0].x = f32::INFINITY;
    assert!(doc.validate().is_err());
}

#[test]
fn cubic_half_heads_cover_shaft_caps_on_both_independent_end_tangents() {
    for head in [Head::Left, Head::Right] {
        let mut arrow = cubic();
        let style = arrow.style.as_mut().unwrap();
        style.head = head;
        style.tail = head;
        let paths = arrow.paths();
        let shaft = &paths[0];
        let radius = shaft.style.width() * 0.5;
        let start = shaft.commands[0].points()[0];
        let end = *shaft.commands.last().unwrap().points().last().unwrap();
        for (cap, head) in [(end, &paths[1]), (start, &paths[2])] {
            let polygon = &crate::graphics::flattened(&head.commands)[0];
            for i in 0..64 {
                let (sin, cos) = (i as f32 * std::f32::consts::TAU / 64.).sin_cos();
                assert!(
                    crate::selection_region::contains(
                        polygon,
                        cap.offset(cos * radius, sin * radius)
                    ),
                    "Exposed cubic shaft cap: {head:?}",
                    head = arrow.appearance().head
                );
            }
        }
    }
}

#[test]
fn collapsed_cubic_tangents_keep_the_shaft_inside_full_and_half_heads() {
    for head in [Head::Full, Head::Left, Head::Right] {
        let mut arrow = cubic();
        arrow.cubic = Some([arrow.start, arrow.end]);
        let style = arrow.style.as_mut().unwrap();
        style.head = head;
        style.tail = head;
        let paths = arrow.paths();
        let shaft = &paths[0];
        let radius = shaft.style.width() * 0.5;
        let start = shaft.commands[0].points()[0];
        let end = *shaft.commands.last().unwrap().points().last().unwrap();
        for (cap, head) in [(end, &paths[1]), (start, &paths[2])] {
            let polygon = &crate::graphics::flattened(&head.commands)[0];
            for i in 0..64 {
                let (sin, cos) = (i as f32 * std::f32::consts::TAU / 64.).sin_cos();
                assert!(crate::selection_region::contains(
                    polygon,
                    cap.offset(cos * radius, sin * radius)
                ));
            }
        }
        let (lo, hi) = arrow.bounds();
        assert!(lo.x > -15. && hi.x < 135. && lo.y > -15. && hi.y < 15.);
    }
}
