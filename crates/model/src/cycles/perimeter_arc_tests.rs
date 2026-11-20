use super::*;
use crate::{
    cycles::{self, Cycle, Shape, Step},
    document::Annotation,
    typography::TextFormat,
};

#[path = "perimeter_side_tests.rs"]
mod sides;

fn fixture(shape: Shape, n: usize, mirrored: bool, counterclockwise: bool) -> (Document, u64) {
    let mut doc = Document::default();
    let mut g = Graphic::dragged(
        1,
        crate::graphics::GraphicKind::Ellipse,
        Point::new(-300., -150.),
        Point::new(300., 150.),
        Default::default(),
        Default::default(),
        false,
    );
    if n > 4 {
        g.axis_x.x *= 10.;
        g.axis_y.y *= 10.;
    }
    if mirrored {
        g.origin.x += g.axis_x.x;
        g.axis_x.x = -g.axis_x.x;
    }
    g.cycle = Some(Box::new(Cycle {
        shape,
        counterclockwise,
        ..Default::default()
    }));
    doc.graphics.push(g);
    let g = &doc.graphics[0];
    let perimeter = cycles::outline(g).unwrap();
    let (center, radius) = g.cycle_circle();
    let top = perimeter
        .project(center.offset(0., -2. * radius))
        .unwrap()
        .s;
    let dir = if g.cycle_counterclockwise() { -1. } else { 1. };
    let mut parts = vec![];
    for i in 0..n {
        let center = perimeter
            .point(top + dir * perimeter.length() * i as f64 / n as f64)
            .unwrap();
        let id = doc.next_id();
        let mut text = Annotation {
            id,
            angle: 0.,
            position: Point::default(),
            text: "Pd".into(),
            format: TextFormat {
                style: doc.drawing_style.text_style(),
                ..Default::default()
            },
        };
        let (w, h) = text.size();
        text.position = center.offset(-w * 0.5, -h * 0.5);
        doc.annotations.push(text);
        parts.push(vec![id]);
    }
    let cycle = doc.graphics[0].cycle.as_mut().unwrap();
    cycle.steps = vec![Step::default(); n];
    cycle.intermediates = parts;
    doc.validate().unwrap();
    (doc, 1)
}

fn graphic(doc: &Document) -> &Graphic {
    &doc.graphics[0]
}

fn on_perimeter(perimeter: &Outline, arrow: &Arrow) {
    arrow.validate().unwrap();
    assert!(
        arrow.arc.is_none(),
        "noncircles must not claim an analytic circular sweep"
    );
    for i in 0..=12 {
        let point = arrow.point(i as f32 / 12.);
        assert!(
            perimeter.project(point).unwrap().distance < 0.01,
            "{point:?} left the perimeter"
        );
    }
}

#[test]
fn ordinary_noncircle_shafts_follow_the_perimeter_and_clear_the_ink() {
    for shape in [Shape::Ellipse, Shape::RoundedRectangle, Shape::Rectangle] {
        let (doc, _) = fixture(shape, 4, false, false);
        let g = graphic(&doc);
        let perimeter = cycles::outline(g).unwrap();
        let legacy = cycles::drawn_circle(&doc, g);
        assert!(
            legacy
                .arrows
                .iter()
                .any(|a| [0., 0.25, 0.5, 0.75, 1.].into_iter().any(|t| perimeter
                    .project(a.point(t))
                    .unwrap()
                    .distance
                    > 5.)),
            "legacy Circle must reproduce the wrong outline for {shape:?}"
        );
        let drawn = cycles::drawn(&doc, g);
        assert_eq!(drawn.arrows.len(), 4, "{shape:?}");
        assert_eq!(drawn.arcs, [Some(0), Some(1), Some(2), Some(3)]);
        let ink = boxes(&doc, g.cycle.as_ref().unwrap());
        for arrow in &drawn.arrows {
            on_perimeter(&perimeter, arrow);
            for end in [arrow.start, arrow.end] {
                for ink in ink.iter().flatten() {
                    assert!(
                        !contains(
                            grown(*ink, DEFAULT.world(super::super::GAP_PT) * 0.999),
                            end
                        ),
                        "{shape:?}: {end:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn rectangle_keeps_exact_right_angle_nodes_in_native_arrows() {
    let (doc, _) = fixture(Shape::Rectangle, 4, false, false);
    let drawn = cycles::drawn(&doc, graphic(&doc));
    let corners = [
        Point::new(300., -150.),
        Point::new(300., 150.),
        Point::new(-300., 150.),
        Point::new(-300., -150.),
    ];
    for (arrow, corner) in drawn.arrows.iter().zip(corners) {
        assert_eq!(arrow.nodes.len(), 1);
        let node = arrow.nodes[0];
        assert_eq!(node.point, corner);
        assert_eq!(node.kind, PathNodeType::Corner);
        let a = Point::new(
            node.point.x - node.controls[0].x,
            node.point.y - node.controls[0].y,
        );
        let b = Point::new(
            node.controls[1].x - node.point.x,
            node.controls[1].y - node.point.y,
        );
        assert_eq!(a.x * b.x + a.y * b.y, 0.);
        assert_eq!(arrow.point(0.5), corner);
    }
    // A shallow affine corner is still a deliberate corner, not a Smooth
    // node inferred from nearly parallel controls.
    let (mut doc, _) = fixture(Shape::Rectangle, 0, false, false);
    doc.graphics[0].axis_y = Point::new(1000., 1.);
    let drawn = cycles::drawn(&doc, graphic(&doc));
    assert!(
        drawn.arrows[0]
            .nodes
            .iter()
            .all(|n| n.kind == PathNodeType::Corner)
    );
}

#[test]
fn reflected_and_counterclockwise_cycles_follow_their_step_order_once() {
    for shape in [Shape::Ellipse, Shape::RoundedRectangle, Shape::Rectangle] {
        for mirrored in [false, true] {
            for counterclockwise in [false, true] {
                let (doc, _) = fixture(shape, 4, mirrored, counterclockwise);
                let g = graphic(&doc);
                let perimeter = cycles::outline(g).unwrap();
                let dir = if g.cycle_counterclockwise() { -1. } else { 1. };
                for arrow in cycles::drawn(&doc, g).arrows {
                    on_perimeter(&perimeter, &arrow);
                    let mut previous = perimeter.project(arrow.start).unwrap().s;
                    let mut walked = 0.;
                    for i in 1..=24 {
                        let next = perimeter.project(arrow.point(i as f32 / 24.)).unwrap().s;
                        let advance = (dir * (next - previous) + perimeter.length() * 0.5)
                            .rem_euclid(perimeter.length())
                            - perimeter.length() * 0.5;
                        assert!(
                            advance >= -0.01,
                            "{shape:?}, mirror={mirrored}, ccw={counterclockwise}: reversed twice"
                        );
                        walked += advance;
                        previous = next;
                    }
                    assert!(walked > 0. && walked < perimeter.length() * 0.5);
                }
            }
        }
    }
}

#[test]
fn empty_and_lone_intermediate_cycles_remain_open_and_finite() {
    for shape in [Shape::Ellipse, Shape::RoundedRectangle, Shape::Rectangle] {
        for n in [0, 1] {
            let (doc, _) = fixture(shape, n, false, false);
            let g = graphic(&doc);
            let drawn = cycles::drawn(&doc, g);
            assert_eq!(drawn.arrows.len(), 1, "{shape:?}, n={n}");
            assert_ne!(drawn.arrows[0].start, drawn.arrows[0].end);
            on_perimeter(&cycles::outline(g).unwrap(), &drawn.arrows[0]);
        }
    }
    let (mut doc, _) = fixture(Shape::Rectangle, 0, false, false);
    let g = &mut doc.graphics[0];
    g.origin = Point::default();
    g.axis_x = Point::new(0.0001, 0.);
    g.axis_y = Point::new(0., 0.0002);
    assert_eq!(cycles::drawn(&doc, graphic(&doc)).arrows.len(), 1);
}

#[test]
fn lead_trail_and_hidden_settings_keep_their_meaning() {
    let (mut doc, _) = fixture(Shape::Rectangle, 4, false, false);
    let before = cycles::drawn(&doc, graphic(&doc));
    {
        let cycle = doc.graphics[0].cycle.as_mut().unwrap();
        cycle.steps[0].lead_pt = 15.;
        cycle.steps[0].trail_pt = 10.;
        cycle.steps[2].hidden = true;
    }
    let after = cycles::drawn(&doc, graphic(&doc));
    assert_eq!(after.arcs, [Some(0), Some(1), Some(3)]);
    assert!(after.arrows[0].start.distance(before.arrows[0].start) > 10.);
    assert!(after.arrows[0].end.distance(before.arrows[0].end) > 10.);
    assert_eq!(after.arrows[1], before.arrows[1]);
    assert_eq!(after.arrows[2], before.arrows[3]);
    for arrow in &after.arrows {
        on_perimeter(&cycles::outline(graphic(&doc)).unwrap(), arrow);
    }
}

#[test]
fn circle_is_exactly_the_legacy_analytic_path_including_other_features() {
    for n in [0, 1, 4] {
        let (mut doc, _) = fixture(Shape::Circle, n, true, false);
        if n > 1 {
            let cycle = doc.graphics[0].cycle.as_mut().unwrap();
            cycle.steps[0].reversible = true;
            cycle.steps[1].bow_pt = 120.;
            cycle.steps[1].name = "Oxidative addition".into();
            cycle.label = "Pd cycle".into();
        }
        let g = graphic(&doc);
        let actual = cycles::drawn(&doc, g);
        let legacy = cycles::drawn_circle(&doc, g);
        assert_eq!(actual.arrows, legacy.arrows);
        assert_eq!(actual.arcs, legacy.arcs);
        assert_eq!(actual.labels, legacy.labels);
        assert!(actual.arrows.iter().all(|a| a.arc.is_some()));
    }
}

#[test]
fn converted_bow_reversible_and_side_arrows_preserve_names() {
    let (mut doc, _) = fixture(Shape::Ellipse, 4, false, false);
    let side = doc.next_id();
    let mut reagent = doc.annotations[0].clone();
    reagent.id = side;
    reagent.position = Point::new(450., 200.);
    reagent.text = "Ar–X".into();
    doc.annotations.push(reagent);
    {
        let cycle = doc.graphics[0].cycle.as_mut().unwrap();
        cycle.steps[0].reversible = true;
        cycle.steps[1].bow_pt = 40.;
        cycle.steps[1].name = "Oxidative addition".into();
        cycle.steps[2].reagents.push(vec![side]);
    }
    doc.validate().unwrap();
    let g = graphic(&doc);
    let old = cycles::drawn_circle(&doc, g);
    let actual = cycles::drawn(&doc, g);
    let retained = |drawn: &Drawn| {
        drawn
            .arrows
            .iter()
            .zip(&drawn.arcs)
            .filter(|(_, step)| step.is_none())
            .map(|(a, s)| (a.clone(), *s))
            .collect::<Vec<_>>()
    };
    assert_ne!(retained(&actual), retained(&old));
    assert_eq!(retained(&actual).len(), 1);
    assert_eq!(actual.labels, old.labels);
    assert_eq!(actual.arcs.iter().filter(|s| **s == Some(0)).count(), 2);
    assert!(
        actual
            .arrows
            .iter()
            .zip(actual.arcs)
            .filter(|(_, s)| s.is_some())
            .all(|(a, _)| a.arc.is_none())
    );
}

#[test]
fn bow_and_reversible_lanes_survive_direction_reflection_rotation_and_materialization() {
    use crate::arrows::Head;
    for shape in [Shape::Ellipse, Shape::RoundedRectangle, Shape::Rectangle] {
        for mirrored in [false, true] {
            for counterclockwise in [false, true] {
                let (mut doc, _) = fixture(shape, 4, mirrored, counterclockwise);
                let rotate = |p: Point| {
                    let (s, c) = 0.43_f32.sin_cos();
                    Point::new(c * p.x - s * p.y, s * p.x + c * p.y)
                };
                let g = &mut doc.graphics[0];
                g.origin = rotate(g.origin);
                g.axis_x = rotate(g.axis_x);
                g.axis_y = rotate(g.axis_y);
                for a in &mut doc.annotations {
                    let (w, h) = a.size();
                    a.position =
                        rotate(a.position.offset(w * 0.5, h * 0.5)).offset(-w * 0.5, -h * 0.5);
                }
                let original = cycles::drawn(&doc, graphic(&doc));
                for (i, s) in doc.graphics[0]
                    .cycle
                    .as_mut()
                    .unwrap()
                    .steps
                    .iter_mut()
                    .enumerate()
                {
                    s.bow_pt = if i % 2 == 0 { 36. } else { -36. };
                }
                let bowed = cycles::drawn(&doc, graphic(&doc));
                assert_eq!(original.arcs, bowed.arcs);
                for (plain, bent) in original.arrows.iter().zip(&bowed.arrows) {
                    assert_eq!(plain.start, bent.start);
                    assert_eq!(plain.end, bent.end);
                    assert_ne!(plain.point(0.5), bent.point(0.5));
                }
                for s in &mut doc.graphics[0].cycle.as_mut().unwrap().steps {
                    s.reversible = true;
                }
                doc.validate().unwrap();
                let g = graphic(&doc);
                let drawn = cycles::drawn(&doc, g);
                assert_eq!(
                    drawn.arrows.len(),
                    8,
                    "{shape:?}, {mirrored}, {counterclockwise}"
                );
                assert_eq!(drawn.arcs.len(), 8);
                let dir = if g.cycle_counterclockwise() { -1. } else { 1. };
                let half = DEFAULT.world(ArrowStyle::DEFAULT.gap_pt.max(2.)) * 0.75;
                for (s, center) in bowed.arrows.iter().enumerate() {
                    let lanes: Vec<_> = drawn
                        .arrows
                        .iter()
                        .zip(&drawn.arcs)
                        .filter(|(_, step)| **step == Some(s))
                        .map(|(a, _)| a)
                        .collect();
                    assert_eq!(lanes.len(), 2);
                    let controls = center.cubic.unwrap();
                    let normal = |a: Point, b: Point| {
                        let (x, y) = (b.x - a.x, b.y - a.y);
                        let len = x.hypot(y);
                        Point::new(dir * y / len, -dir * x / len)
                    };
                    let first = normal(center.start, controls[0]);
                    let last = normal(controls[1], center.end);
                    let near =
                        |a: Point, b: Point| assert!(a.distance(b) < 0.0001, "{a:?} != {b:?}");
                    near(
                        lanes[0].start,
                        center.start.offset(half * first.x, half * first.y),
                    );
                    near(
                        lanes[0].end,
                        center.end.offset(half * last.x, half * last.y),
                    );
                    near(
                        lanes[1].start,
                        center.end.offset(-half * last.x, -half * last.y),
                    );
                    near(
                        lanes[1].end,
                        center.start.offset(-half * first.x, -half * first.y),
                    );
                    for lane in lanes {
                        lane.validate().unwrap();
                        assert!(lane.arc.is_none());
                        assert_eq!(
                            lane.appearance().head,
                            if dir > 0. { Head::Left } else { Head::Right }
                        );
                        assert_eq!(lane.appearance().tail, Head::None);
                        let paths = lane.paths();
                        assert!(paths.iter().any(|p| !p.filled && p.commands.len() > 1));
                        assert!(paths.iter().any(|p| p.filled));
                        assert!(cycles::hit(&doc, g, lane.point(0.5), 1.));
                    }
                }
                let (materialized, written) = cycles::materialized(&doc);
                assert_eq!(written[0].arcs.len(), 8);
                assert_eq!(materialized.arrows.len(), 8);
                let reopened = Document::from_json(&materialized.file_json().unwrap()).unwrap();
                assert_eq!(reopened.arrows, materialized.arrows);
                for (stored, rendered) in materialized.arrows.iter().zip(drawn.arrows) {
                    assert_eq!(stored.nodes, rendered.nodes);
                    assert_eq!(stored.cubic, rendered.cubic);
                    assert_eq!(stored.appearance(), rendered.appearance());
                }
            }
        }
    }
}

#[test]
fn materialized_arrows_keep_nodes_and_hit_tests_reach_wide_noncircles() {
    let (doc, _) = fixture(Shape::Rectangle, 4, false, false);
    let drawn = cycles::drawn(&doc, graphic(&doc));
    let (copy, written) = cycles::materialized(&doc);
    assert_eq!(written[0].arcs.len(), 4);
    assert_eq!(copy.arrows.len(), 4);
    for (arrow, original) in copy.arrows.iter().zip(drawn.arrows) {
        assert_eq!(arrow.nodes, original.nodes);
        assert_eq!(arrow.cubic, original.cubic);
        assert!(arrow.arc.is_none());
    }
    let back = Document::from_json(&copy.file_json().unwrap()).unwrap();
    assert_eq!(back.arrows, copy.arrows);
    assert!(doc.arrows.is_empty());

    let (mut doc, _) = fixture(Shape::Ellipse, 0, false, false);
    doc.graphics[0].origin = Point::new(-1000., -30.);
    doc.graphics[0].axis_x = Point::new(2000., 0.);
    doc.graphics[0].axis_y = Point::new(0., 60.);
    let g = graphic(&doc);
    assert!(cycles::hit(&doc, g, Point::new(1000., 0.), 1.));

    let (mut doc, _) = fixture(Shape::Ellipse, 4, false, false);
    doc.graphics[0].origin = Point::new(-2000., -30.);
    doc.graphics[0].axis_x = Point::new(4000., 0.);
    doc.graphics[0].axis_y = Point::new(0., 60.);
    doc.graphics[0].cycle.as_mut().unwrap().steps[0].reversible = true;
    let g = graphic(&doc);
    let drawn = cycles::drawn(&doc, g);
    let legacy = drawn
        .arrows
        .iter()
        .zip(&drawn.arcs)
        .find(|(_, step)| **step == Some(0))
        .unwrap()
        .0;
    assert!(cycles::hit(&doc, g, legacy.point(0.5), 1.));
}

#[test]
fn invalid_non_circle_frame_does_not_fall_back_to_a_circle() {
    let (mut doc, _) = fixture(Shape::Rectangle, 0, false, false);
    doc.graphics[0].axis_y = Point::default();
    assert!(cycles::drawn(&doc, graphic(&doc)).arrows.is_empty());
}

#[test]
fn coincident_intermediates_do_not_turn_a_zero_length_step_into_a_loop() {
    let (mut doc, _) = fixture(Shape::Rectangle, 4, false, false);
    doc.annotations[1].position = doc.annotations[0].position;
    let drawn = cycles::drawn(&doc, graphic(&doc));
    assert!(!drawn.arcs.contains(&Some(0)));
    assert_eq!(drawn.arcs.len(), drawn.arrows.len());
}

#[test]
#[ignore = "local cycle rendering benchmark"]
fn benchmark_perimeter_step_rendering() {
    use std::{hint::black_box, time::Instant};
    for n in [4, 64] {
        for shape in [
            Shape::Circle,
            Shape::Ellipse,
            Shape::RoundedRectangle,
            Shape::Rectangle,
        ] {
            let (doc, _) = fixture(shape, n, false, false);
            let start = Instant::now();
            let mut arrows = 0;
            for _ in 0..20 {
                arrows += black_box(cycles::drawn(black_box(&doc), black_box(graphic(&doc))))
                    .arrows
                    .len();
            }
            println!(
                "{shape:?}, {n} steps: {:.3} ms/render, {arrows} arrows/20 renders",
                start.elapsed().as_secs_f64() * 1000. / 20.
            );
            if n == 64 && shape == Shape::Ellipse {
                let perimeter = cycles::outline(graphic(&doc)).unwrap();
                let bounds = boxes(&doc, graphic(&doc).cycle.as_ref().unwrap());
                let start = Instant::now();
                for _ in 0..20 {
                    for b in bounds.iter().flatten() {
                        black_box(perimeter.project(center_of(*b)));
                    }
                }
                println!(
                    "  all anchor projections: {:.3} ms/render",
                    start.elapsed().as_secs_f64() * 1000. / 20.
                );
                let anchors: Vec<_> = bounds
                    .iter()
                    .map(|b| perimeter.project(center_of(b.unwrap())).unwrap().s)
                    .collect();
                let start = Instant::now();
                for _ in 0..20 {
                    for (&anchor, b) in anchors.iter().zip(&bounds) {
                        for dir in [-1., 1.] {
                            black_box(clear(
                                &perimeter,
                                anchor,
                                dir,
                                grown(b.unwrap(), DEFAULT.world(super::super::GAP_PT)),
                                perimeter.length() / 128.,
                            ));
                        }
                    }
                }
                println!(
                    "  both clearances: {:.3} ms/render",
                    start.elapsed().as_secs_f64() * 1000. / 20.
                );
            }
        }
    }
}

#[test]
#[ignore = "local bowed reversible cycle rendering benchmark"]
fn benchmark_bowed_reversible_step_rendering() {
    use std::{hint::black_box, time::Instant};
    for n in [4, 64] {
        for shape in [
            Shape::Circle,
            Shape::Ellipse,
            Shape::RoundedRectangle,
            Shape::Rectangle,
        ] {
            let (mut doc, _) = fixture(shape, n, false, false);
            for (i, step) in doc.graphics[0]
                .cycle
                .as_mut()
                .unwrap()
                .steps
                .iter_mut()
                .enumerate()
            {
                step.reversible = true;
                step.bow_pt = if i % 2 == 0 { 36. } else { -36. };
            }
            let start = Instant::now();
            let mut arrows = 0;
            for _ in 0..20 {
                arrows += black_box(cycles::drawn(black_box(&doc), black_box(graphic(&doc))))
                    .arrows
                    .len();
            }
            assert_eq!(arrows, n * 2 * 20);
            println!(
                "{shape:?}, {n} bowed reversible steps: {:.3} ms/render",
                start.elapsed().as_secs_f64() * 1000. / 20.
            );
        }
    }
}
