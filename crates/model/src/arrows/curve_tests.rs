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

/// Rounded #717 square-handle offsets in screen points at 225% zoom.
fn narrow_bow() -> Arrow {
    let mut arrow = Arrow::new(
        1,
        Point::new(0., 15.),
        Point::new(0., 0.),
        Preset::Curved,
        ArrowStyle::default(),
    );
    arrow.cubic = Some([
        arrow.start.offset(22. / 2.25, -25. / 2.25),
        arrow.end.offset(22. / 2.25, 15. / 2.25),
    ]);
    arrow
}

fn assert_terminal_ink_covered(shaft: &super::ArrowPath, head: &super::ArrowPath) {
    let polygon = &crate::graphics::flattened(&head.commands)[0];
    let radius = shaft.style.width() * 0.5;
    let [PathCommand::Move(start), PathCommand::Cubic(a, b, end)] = shaft.commands.as_slice()
    else {
        panic!("One exactly trimmed cubic shaft");
    };
    for i in 0..128 {
        let (sin, cos) = (i as f32 * std::f32::consts::TAU / 128.).sin_cos();
        assert!(
            crate::selection_region::contains(polygon, end.offset(cos * radius, sin * radius)),
            "Exposed terminal cap at {end:?}"
        );
    }
    // Inspect both stroke edges for the last radius of the rendered shaft,
    // as well as its cap: a covered cap alone can hide a side spur upstream.
    for i in 0..=256 {
        let t = 1. - i as f32 / 256.;
        let u = 1. - t;
        let center = start
            .lerp(*a, t)
            .lerp(a.lerp(*b, t), t)
            .lerp(a.lerp(*b, t).lerp(b.lerp(*end, t), t), t);
        if center.distance(*end) > radius {
            break;
        }
        let velocity = Point::new(
            3. * (u * u * (a.x - start.x) + 2. * u * t * (b.x - a.x) + t * t * (end.x - b.x)),
            3. * (u * u * (a.y - start.y) + 2. * u * t * (b.y - a.y) + t * t * (end.y - b.y)),
        );
        let direction = super::unit(Point::default(), velocity);
        for side in [-radius, radius] {
            assert!(crate::selection_region::contains(
                polygon,
                center.offset(-direction.y * side, direction.x * side)
            ));
        }
    }
}

#[test]
fn mechanism_curvature_717_solid_full_notch_joins_tight_cubic_and_covers_terminal_ink() {
    let mut turning = narrow_bow();
    // Stress the same short span with a turn that used to expose the shaft
    // beside the tangent head, before the curve returned to the tip.
    turning.cubic = Some([Point::new(25., -35.), Point::new(-8., 15.)]);
    for original in [narrow_bow(), turning] {
        for reversed in [false, true] {
            let mut arrow = original.clone();
            if reversed {
                arrow.reverse();
            }
            let paths = arrow.paths();
            let [
                PathCommand::Move(tip),
                PathCommand::Line(left),
                PathCommand::Line(notch),
                PathCommand::Line(right),
                PathCommand::Close,
            ] = paths[1].commands.as_slice()
            else {
                panic!("Solid full head");
            };
            let radius = paths[0].style.width() * 0.5;
            let closest = (0..=4096)
                .map(|i| notch.distance(arrow.point(i as f32 / 4096.)))
                .fold(f32::MAX, f32::min);
            assert!(closest < radius * 0.05, "Notch beside shaft by {closest}");
            let base = left.lerp(*right, 0.5);
            near(super::unit(base, *tip), super::unit(*notch, arrow.end));
            assert_terminal_ink_covered(&paths[0], &paths[1]);
            assert_eq!(
                arrow.arrival_head_path().unwrap().commands,
                paths[1].commands
            );
        }
    }
}

#[test]
fn mechanism_curvature_717_cubic_half_head_axis_uses_its_visible_neck() {
    for side in [1., -1.] {
        let mut arrow = narrow_bow();
        let style = arrow.style.as_mut().unwrap();
        style.head = if side > 0. { Head::Left } else { Head::Right };
        let paths = arrow.paths();
        let commands = &paths[1].commands;
        let (tip, wing, outer, inner) = (
            commands[0].points()[0],
            commands[1].points()[0],
            commands[2].points()[0],
            commands[3].points()[0],
        );
        let neck = outer.lerp(inner, 0.5);
        let direction = super::unit(neck, arrow.end);
        let style = arrow.appearance();
        let radius = paths[0].style.width() * 0.5;
        let length = crate::style::DEFAULT
            .world(style.head_length_pt)
            .min(arrow.start.distance(arrow.end) * 0.4);
        let width = crate::style::DEFAULT.world(style.head_width_pt) + radius;
        near(
            wing,
            tip.offset(
                -direction.x * length + direction.y * side * width,
                -direction.y * length - direction.x * side * width,
            ),
        );
        assert!((neck.distance(arrow.end) - length * (1. - style.head_notch)).abs() < 0.0001);
        assert_terminal_ink_covered(&paths[0], &paths[1]);
        assert_eq!(
            arrow.arrival_head_path().unwrap().commands,
            paths[1].commands
        );
    }
}

#[test]
fn mechanism_curvature_717_gentle_full_head_stays_within_one_tenth_point() {
    let mut arrow = cubic();
    arrow.cubic = Some([Point::new(40., -4.), Point::new(80., -4.)]);
    let style = arrow.appearance();
    let mut tangent_head = Vec::new();
    arrow.head_path(
        &style,
        arrow.end,
        arrow.shaft_tangent(1., false),
        style.head,
        &mut |commands, _, _| tangent_head = commands,
    );
    let paths = arrow.paths();
    let tolerance = crate::style::DEFAULT.world(0.1);
    for (old, current) in tangent_head
        .iter()
        .flat_map(PathCommand::points)
        .zip(paths[1].commands.iter().flat_map(PathCommand::points))
    {
        assert!(old.distance(current) < tolerance);
    }
    assert_terminal_ink_covered(&paths[0], &paths[1]);
}

#[test]
fn preset_transitions_preserve_only_matching_or_mechanism_curve_geometry() {
    let start = Point::new(2., -4.);
    let end = Point::new(122., 16.);
    let control = Point::new(48., -52.);
    let controls = [Point::new(8., -72.), Point::new(116., 84.)];
    let compatible = [
        (Preset::Curved, Preset::Fishhook),
        (Preset::Fishhook, Preset::Curved),
    ];
    for is_cubic in [false, true] {
        for &from in Preset::ALL {
            for &to in Preset::ALL {
                let mut arrow = Arrow::new(1, start, end, from, ArrowStyle::preset(from));
                if is_cubic {
                    arrow.cubic = Some(controls);
                } else {
                    arrow.control = Some(control);
                }
                let mut style = ArrowStyle::preset(to);
                style.width_pt = 1.3;
                arrow.set_preset(to, &style);
                let keeps_geometry = from == to || compatible.contains(&(from, to));
                assert_eq!(
                    arrow.control,
                    (keeps_geometry && !is_cubic).then_some(control),
                    "quadratic {from:?} -> {to:?}, cubic={is_cubic}"
                );
                assert_eq!(
                    arrow.cubic,
                    (keeps_geometry && is_cubic).then_some(controls),
                    "cubic {from:?} -> {to:?}, cubic={is_cubic}"
                );
                assert_eq!((arrow.start, arrow.end), (start, end));
                assert_eq!(arrow.kind, to.kind());
                assert_eq!(arrow.style, Some(style));
            }
        }
    }
}

#[test]
fn setting_a_preset_clears_unknown_saved_kind_geometry() {
    for is_cubic in [false, true] {
        for &preset in Preset::ALL {
            let mut arrow = cubic();
            arrow.kind = "unknown_saved_kind".into();
            if !is_cubic {
                arrow.cubic = None;
                arrow.control = Some(Point::new(50., -90.));
            }
            let style = ArrowStyle::preset(preset);
            arrow.set_preset(preset, &style);
            assert_eq!(arrow.control, None, "{preset:?}, cubic={is_cubic}");
            assert_eq!(arrow.cubic, None, "{preset:?}, cubic={is_cubic}");
            assert_eq!(arrow.kind, preset.kind());
            assert_eq!(arrow.style, Some(style));
        }
    }
}

#[test]
fn typed_handles_keep_visible_order_and_first_nearest_tie() {
    use crate::arrows::ArrowHandle;

    let start = Point::new(0., 0.);
    let end = Point::new(12., 0.);
    for preset in [Preset::Forward, Preset::Curved] {
        let mut arrow = Arrow::new(1, start, end, preset, ArrowStyle::preset(preset));
        let mut expected = vec![
            (ArrowHandle::Start, start),
            (ArrowHandle::End, end),
            (ArrowHandle::Bend, Point::new(6., 0.)),
        ];
        if preset == Preset::Curved {
            arrow.cubic = Some([start, end]);
            expected.extend([(ArrowHandle::Departure, start), (ArrowHandle::Arrival, end)]);
        }
        let handles = arrow.handles();
        let copied = handles;
        assert_eq!(copied.len(), expected.len());
        assert_eq!(handles.typed().collect::<Vec<_>>(), expected);
        assert_eq!(arrow.handle_at(start, 1.), Some(0));
        assert_eq!(arrow.handle_at(end, 1.), Some(1));
        assert_eq!(arrow.handle_at(Point::new(3., 0.), 3.1), Some(0));
        assert_eq!(arrow.handle_at(Point::new(3., 0.), 3.), None);
        for (index, (handle, _)) in expected.into_iter().enumerate() {
            assert_eq!(handle.index(), index);
            assert_eq!(ArrowHandle::from_index(index), Some(handle));
            assert_eq!(handle.is_endpoint(), index < 2);
            assert_eq!(handle.is_direction(), index >= 3);
        }
    }
    assert_eq!(ArrowHandle::from_index(5), None);
    assert_eq!(ArrowHandle::from_index(usize::MAX), None);
}
