use super::*;
use crate::{canvas_theme::CanvasTheme, cycles::shapes::Shaft};
use kurbo::{CubicBez, ParamCurve};

fn label_box(label: &(Point, String, TextFormat)) -> (Point, Point) {
    let layout = crate::typography::layout(&label.1, &label.2);
    (label.0, label.0.offset(layout.width, layout.height))
}

fn overlaps(a: (Point, Point), b: (Point, Point)) -> bool {
    a.0.x < b.1.x && b.0.x < a.1.x && a.0.y < b.1.y && b.0.y < a.1.y
}

#[test]
fn free_noncircle_name_uses_final_true_length_midpoint_and_inside_normal() {
    for shape in [Shape::Ellipse, Shape::RoundedRectangle, Shape::Rectangle] {
        let (mut doc, _) = fixture(shape, 4, false, false);
        doc.graphics[0].cycle.as_mut().unwrap().steps[0].name = "OA".into();
        let out = cycles::drawn(&doc, graphic(&doc));
        let arrow = &out.arrows[0];
        let shaft = Shaft::from_arrow(arrow).unwrap();
        let (mid, way) = shaft.sample(shaft.length() * 0.5, false).unwrap();
        let label = &out.labels[0];
        let layout = crate::typography::layout(&label.1, &label.2);
        let normal = Point::new(way.y, -way.x);
        let reach = DEFAULT.world(cycles::GAP_PT)
            + 0.5 * (layout.width * normal.x.abs() + layout.height * normal.y.abs());
        let expected = mid.offset(
            -normal.x * reach - layout.width * 0.5,
            -normal.y * reach - layout.height * 0.5,
        );
        assert!(
            label.0.distance(expected) < 0.002,
            "{shape:?}: {:?} != {expected:?}",
            label.0
        );
        if shape == Shape::Rectangle {
            assert!(arrow.point(0.5).distance(mid) > 25.);
        }
    }
}

#[test]
fn a_free_reversible_name_stays_inside_and_clears_both_painted_lanes() {
    for shape in [Shape::Ellipse, Shape::RoundedRectangle, Shape::Rectangle] {
        let (mut doc, _) = fixture(shape, 4, false, false);
        let step = &mut doc.graphics[0].cycle.as_mut().unwrap().steps[0];
        step.name = "OA".into();
        step.reversible = true;
        let out = cycles::drawn(&doc, graphic(&doc));
        let shaft = Shaft::from_arrow(&out.arrows[0]).unwrap();
        let (p, t) = shaft.sample(shaft.length() * 0.5, false).unwrap();
        let center = center_of(label_box(&out.labels[0]));
        assert!(
            (center.x - p.x) * t.y - (center.y - p.y) * t.x < 0.,
            "{shape:?}: preserve inside preference"
        );
        assert_paint_clear(&doc, &out);
    }
}

fn assert_paint_clear(doc: &Document, out: &Drawn) {
    let parts = boxes(doc, graphic(doc).cycle.as_ref().unwrap());
    for (i, label) in out.labels.iter().enumerate() {
        let bounds = label_box(label);
        for other in out.labels.iter().skip(i + 1) {
            assert!(!overlaps(bounds, label_box(other)), "names overlap");
        }
        for part in parts.iter().flatten() {
            assert!(!overlaps(bounds, *part), "name covers a part");
        }
        // Independent dense evaluation checks every native painted cubic;
        // analytic edge-crossing/filled-containment cases have separate tests.
        let inside =
            |p: Point| bounds.0.x < p.x && p.x < bounds.1.x && bounds.0.y < p.y && p.y < bounds.1.y;
        for path in out.arrows.iter().flat_map(|a| a.paths()) {
            let mut at = Point::default();
            for c in path.commands {
                match c {
                    PathCommand::Move(p) => at = p,
                    PathCommand::Line(p) => {
                        for k in 0..=128 {
                            assert!(
                                !inside(at.lerp(p, k as f32 / 128.)),
                                "name crosses a painted line"
                            );
                        }
                        at = p;
                    }
                    PathCommand::Cubic(a, b, p) => {
                        let point = |p: Point| kurbo::Point::new(f64::from(p.x), f64::from(p.y));
                        let c = CubicBez::new(point(at), point(a), point(b), point(p));
                        for k in 0..=128 {
                            let q = c.eval(k as f64 / 128.);
                            assert!(
                                !inside(Point::new(q.x as f32, q.y as f32)),
                                "name crosses a painted cubic"
                            );
                        }
                        at = p;
                    }
                    PathCommand::Close => {}
                }
            }
        }
    }
}

#[test]
fn names_follow_bow_reversible_shapes_under_reflection_skew_and_direction_changes() {
    for shape in [Shape::Ellipse, Shape::RoundedRectangle, Shape::Rectangle] {
        for mirror in [false, true] {
            for ccw in [false, true] {
                for bow in [-36., 36.] {
                    for reversible in [false, true] {
                        let (mut doc, _) = fixture(shape, 4, mirror, ccw);
                        let turn = |p: Point| Point::new(1.1 * p.x - 0.25 * p.y, 0.2 * p.x + p.y);
                        let g = &mut doc.graphics[0];
                        g.origin = turn(g.origin);
                        g.axis_x = turn(g.axis_x);
                        g.axis_y = turn(g.axis_y);
                        for a in &mut doc.annotations {
                            let (w, h) = a.size();
                            a.position = turn(a.position.offset(w * 0.5, h * 0.5))
                                .offset(-w * 0.5, -h * 0.5);
                        }
                        let cycle = doc.graphics[0].cycle.as_mut().unwrap();
                        for (i, step) in cycle.steps.iter_mut().enumerate() {
                            step.name = ["OA", "TM", "RE", "Turn"][i].into();
                            step.bow_pt = bow;
                            step.reversible = reversible;
                        }
                        cycle.label = "Pd".into();
                        doc.validate().unwrap();
                        let source = doc.file_json().unwrap();
                        let out = cycles::drawn(&doc, graphic(&doc));
                        assert_eq!(out.labels.len(), 5);
                        assert_paint_clear(&doc, &out);
                        assert_eq!(cycles::drawn(&doc, graphic(&doc)).labels, out.labels);
                        assert_eq!(doc.file_json().unwrap(), source);
                    }
                }
            }
        }
    }
}

#[test]
fn hidden_names_missing_routes_repeated_names_and_empty_center_remain_derived() {
    for shape in [Shape::Ellipse, Shape::RoundedRectangle, Shape::Rectangle] {
        for reversible in [false, true] {
            let (mut doc, _) = fixture(shape, 4, false, false);
            let cycle = doc.graphics[0].cycle.as_mut().unwrap();
            cycle.steps[0].name = "OA".into();
            cycle.steps[0].reversible = reversible;
            cycle.steps[0].bow_pt = 20.;
            let visible = cycles::drawn(&doc, graphic(&doc));
            doc.graphics[0].cycle.as_mut().unwrap().steps[0].hidden = true;
            let hidden = cycles::drawn(&doc, graphic(&doc));
            assert_eq!(visible.labels, hidden.labels);
            assert!(!hidden.arcs.contains(&Some(0)));
            doc.annotations[1].position = doc.annotations[0].position;
            let missing = cycles::drawn(&doc, graphic(&doc));
            assert_eq!(
                missing.labels.len(),
                1,
                "a stored name survives a missing route"
            );
            assert!(missing.labels[0].0.is_finite());
        }
        let (mut doc, _) = fixture(shape, 4, false, false);
        let cycle = doc.graphics[0].cycle.as_mut().unwrap();
        for step in &mut cycle.steps {
            step.name = "Pd".into();
        }
        cycle.label = "Pd".into();
        let out = cycles::drawn(&doc, graphic(&doc));
        assert_eq!(out.labels.len(), 5);
        assert_paint_clear(&doc, &out);
        let (mut doc, _) = fixture(shape, 0, false, false);
        doc.graphics[0].cycle.as_mut().unwrap().label = "Pd".into();
        assert_eq!(cycles::drawn(&doc, graphic(&doc)).labels.len(), 1);
    }
}

#[test]
fn reversing_named_steps_keeps_their_names_and_reversing_twice_restores_layout() {
    for shape in [Shape::Ellipse, Shape::RoundedRectangle, Shape::Rectangle] {
        let (mut doc, _) = fixture(shape, 4, true, false);
        let cycle = doc.graphics[0].cycle.as_mut().unwrap();
        for (i, step) in cycle.steps.iter_mut().enumerate() {
            step.name = ["OA", "TM", "RE", "Turn"][i].into();
            step.bow_pt = if i % 2 == 0 { 20. } else { -20. };
            step.reversible = true;
        }
        let original = cycles::drawn(&doc, graphic(&doc));
        let data = doc.graphics[0].cycle.clone();
        cycles::reverse(doc.graphics[0].cycle.as_mut().unwrap());
        let reversed = cycles::drawn(&doc, graphic(&doc));
        let mut before: Vec<_> = original.labels.iter().map(|(_, t, _)| t.clone()).collect();
        let mut after: Vec<_> = reversed.labels.iter().map(|(_, t, _)| t.clone()).collect();
        before.sort();
        after.sort();
        assert_eq!(before, after);
        assert_paint_clear(&doc, &reversed);
        cycles::reverse(doc.graphics[0].cycle.as_mut().unwrap());
        assert_eq!(doc.graphics[0].cycle, data);
        assert_eq!(cycles::drawn(&doc, graphic(&doc)).labels, original.labels);
    }
}

fn without_colors(svg: &str) -> String {
    let mut rest = svg;
    let mut out = String::new();
    while let Some((before, after)) = rest.split_once("rgb(") {
        out.push_str(before);
        out.push_str("rgb(INK)");
        rest = after.split_once(')').unwrap().1;
    }
    out.push_str(rest);
    out
}

#[test]
fn name_geometry_and_wrapping_survive_native_materialization_reopen_and_light_dark_svg() {
    for shape in [
        Shape::Circle,
        Shape::Ellipse,
        Shape::RoundedRectangle,
        Shape::Rectangle,
    ] {
        let (mut doc, _) = fixture(shape, 4, false, false);
        let cycle = doc.graphics[0].cycle.as_mut().unwrap();
        cycle.steps[0].name = "Oxidative addition α–Pd".into();
        cycle.steps[0].reversible = true;
        cycle.steps[0].bow_pt = 36.;
        cycle.steps[1].name = "酸化的付加\nβ".into();
        cycle.steps[2].name = "Pd".into();
        cycle.label = "Pd".into();
        let out = cycles::drawn(&doc, graphic(&doc));
        assert_eq!(out.labels.len(), 4);
        assert!(out.labels[0].1.contains('\n'));
        if shape == Shape::Circle {
            assert_eq!(out.labels, cycles::drawn_circle(&doc, graphic(&doc)).labels);
        }
        let reopened = Document::from_json(&doc.file_json().unwrap()).unwrap();
        assert_eq!(
            cycles::drawn(&reopened, graphic(&reopened)).labels,
            out.labels
        );
        let (native, written) = cycles::materialized(&doc);
        let labels: Vec<_> = native
            .annotations
            .iter()
            .filter(|a| written[0].drawn.contains(&a.id))
            .map(|a| (a.position, a.text.clone(), a.format.clone()))
            .collect();
        assert_eq!(labels, out.labels);
        let back = Document::from_json(&native.file_json().unwrap()).unwrap();
        assert_eq!(back.annotations, native.annotations);
        let light = without_colors(&crate::scene::svg_with_background(&doc));
        doc.canvas_theme = CanvasTheme::Dark;
        assert_eq!(cycles::drawn(&doc, graphic(&doc)).labels, out.labels);
        assert_eq!(
            without_colors(&crate::scene::svg_with_background(&doc)),
            light
        );
    }
}

#[test]
#[ignore = "local named cycle rendering benchmark"]
fn benchmark_named_shapes() {
    use std::{hint::black_box, time::Instant};
    for n in [4, 64] {
        for shape in [
            Shape::Circle,
            Shape::Ellipse,
            Shape::RoundedRectangle,
            Shape::Rectangle,
        ] {
            let (mut doc, _) = fixture(shape, n, false, false);
            for step in &mut doc.graphics[0].cycle.as_mut().unwrap().steps {
                step.name = "OA".into();
                step.bow_pt = 20.;
                step.reversible = true;
            }
            let start = Instant::now();
            for _ in 0..20 {
                let out = black_box(cycles::drawn(black_box(&doc), black_box(graphic(&doc))));
                assert_eq!(out.labels.len(), n);
            }
            println!(
                "{shape:?}, {n} named bowed reversible steps: {:.3} ms/render",
                start.elapsed().as_secs_f64() * 1000. / 20.
            );
        }
    }
}
