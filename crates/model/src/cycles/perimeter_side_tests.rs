use super::*;
use crate::arrows::Head;
use kurbo::{CubicBez, ParamCurve, ParamCurveDeriv, ParamCurveNearest};

fn painted_curves(arrow: &Arrow) -> Vec<CubicBez> {
    let point = |p: Point| kurbo::Point::new(f64::from(p.x), f64::from(p.y));
    let mut curves = vec![];
    let mut at = Point::default();
    for path in arrow.paths().into_iter().filter(|p| !p.filled) {
        for command in path.commands {
            match command {
                PathCommand::Move(p) => at = p,
                PathCommand::Line(p) => {
                    curves.push(CubicBez::new(
                        point(at),
                        point(at.lerp(p, 1. / 3.)),
                        point(at.lerp(p, 2. / 3.)),
                        point(p),
                    ));
                    at = p;
                }
                PathCommand::Cubic(a, b, p) => {
                    curves.push(CubicBez::new(point(at), point(a), point(b), point(p)));
                    at = p;
                }
                PathCommand::Close => panic!("open shaft expected"),
            }
        }
    }
    curves
}

fn closest(arrow: &Arrow, p: Point) -> (f64, Point) {
    painted_curves(arrow)
        .into_iter()
        .map(|c| {
            let p = kurbo::Point::new(f64::from(p.x), f64::from(p.y));
            let algebraic = c.nearest(p, 1e-8);
            // Float-rounded degree-elevated lines can make the algebraic
            // nearest roots ill-conditioned. Independently bracket a sampled
            // minimum and compare measured distances without widening limits.
            let distance = |t| c.eval(t).distance_squared(p);
            let sample = (0..=128_usize)
                .min_by(|a, b| distance(*a as f64 / 128.).total_cmp(&distance(*b as f64 / 128.)))
                .unwrap();
            let mut lo = sample.saturating_sub(1) as f64 / 128.;
            let mut hi = (sample + 1).min(128) as f64 / 128.;
            for _ in 0..60 {
                let a = lo + (hi - lo) / 3.;
                let b = hi - (hi - lo) / 3.;
                if distance(a) <= distance(b) {
                    hi = b;
                } else {
                    lo = a;
                }
            }
            let t = [0., 1., algebraic.t, (lo + hi) * 0.5]
                .into_iter()
                .min_by(|a, b| distance(*a).total_cmp(&distance(*b)))
                .unwrap();
            let v = c.deriv().eval(t).to_vec2();
            let len = v.hypot();
            (
                distance(t).sqrt(),
                Point::new((v.x / len) as f32, (v.y / len) as f32),
            )
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .unwrap()
}

fn add_side(doc: &mut Document, center: Point, incoming: bool) -> u64 {
    let id = doc.next_id();
    let mut a = doc.annotations[0].clone();
    a.id = id;
    a.text = if incoming { "Ar–X" } else { "X−" }.into();
    let (w, h) = a.size();
    a.position = center.offset(-w * 0.5, -h * 0.5);
    doc.annotations.push(a);
    let step = &mut doc.graphics[0].cycle.as_mut().unwrap().steps[0];
    if incoming {
        step.reagents.push(vec![id]);
    } else {
        step.products.push(vec![id]);
    }
    id
}

#[test]
fn side_contacts_and_tangents_follow_final_painted_noncircle_paths() {
    for shape in [Shape::Ellipse, Shape::RoundedRectangle, Shape::Rectangle] {
        for mirrored in [false, true] {
            for counterclockwise in [false, true] {
                for bow in [-36., 0., 36.] {
                    for reversible in [false, true] {
                        for inside in [false, true] {
                            let (mut doc, _) = fixture(shape, 4, mirrored, counterclockwise);
                            let transform =
                                |p: Point| Point::new(0.9 * p.x - 0.3 * p.y, 0.4 * p.x + 1.1 * p.y);
                            let g = &mut doc.graphics[0];
                            g.origin = transform(g.origin);
                            g.axis_x = transform(g.axis_x);
                            g.axis_y = transform(g.axis_y);
                            for a in &mut doc.annotations {
                                let (w, h) = a.size();
                                a.position = transform(a.position.offset(w * 0.5, h * 0.5))
                                    .offset(-w * 0.5, -h * 0.5);
                            }
                            let step = &mut doc.graphics[0].cycle.as_mut().unwrap().steps[0];
                            step.bow_pt = bow;
                            step.reversible = reversible;
                            let before = cycles::drawn(&doc, graphic(&doc));
                            let lanes: Vec<_> = before
                                .arrows
                                .iter()
                                .zip(&before.arcs)
                                .filter(|(_, s)| **s == Some(0))
                                .map(|(a, _)| a.clone())
                                .collect();
                            let lane = &lanes[usize::from(inside && reversible)];
                            let dir = if graphic(&doc).cycle_counterclockwise() {
                                -1.
                            } else {
                                1.
                            } * if inside && reversible { -1. } else { 1. };
                            for (t, incoming) in [(0.25, true), (0.7, false)] {
                                let p = lane.point(t);
                                let a = lane.point(t - 0.001);
                                let b = lane.point(t + 0.001);
                                let len = a.distance(b);
                                let out = if inside { -100. } else { 100. };
                                add_side(
                                    &mut doc,
                                    p.offset(
                                        dir * (b.y - a.y) / len * out,
                                        -dir * (b.x - a.x) / len * out,
                                    ),
                                    incoming,
                                );
                            }
                            doc.validate().unwrap();
                            let saved = doc.file_json().unwrap();
                            let drawn = cycles::drawn(&doc, graphic(&doc));
                            let sides: Vec<_> = drawn
                                .arrows
                                .iter()
                                .zip(&drawn.arcs)
                                .filter(|(_, s)| s.is_none())
                                .map(|(a, _)| a)
                                .collect();
                            assert_eq!(sides.len(), 2);
                            for (i, (side, incoming)) in
                                sides.into_iter().zip([true, false]).enumerate()
                            {
                                let a = &doc.annotations[4 + i];
                                let (w, h) = a.size();
                                let center = a.position.offset(w * 0.5, h * 0.5);
                                let lane = lanes
                                    .iter()
                                    .min_by(|a, b| {
                                        closest(a, center).0.total_cmp(&closest(b, center).0)
                                    })
                                    .unwrap();
                                let touch = if incoming { side.end } else { side.start };
                                let (distance, tangent) = closest(lane, touch);
                                assert!(
                                    distance < 0.002,
                                    "{shape:?} mirror={mirrored} ccw={counterclockwise} bow={bow} rev={reversible} inside={inside}: {distance}; center={center:?} touch={touch:?}; oracle={:?} resolved={:?} contact={:?} curves={:?}",
                                    lanes
                                        .iter()
                                        .map(|a| closest(a, center).0)
                                        .collect::<Vec<_>>(),
                                    lanes
                                        .iter()
                                        .map(|a| cycles::shapes::Shaft::from_arrow(a)
                                            .unwrap()
                                            .project(center)
                                            .unwrap())
                                        .collect::<Vec<_>>(),
                                    lanes
                                        .iter()
                                        .map(|a| closest(a, touch).0)
                                        .collect::<Vec<_>>(),
                                    lanes.iter().map(painted_curves).collect::<Vec<_>>()
                                );
                                let controls = side.cubic.unwrap();
                                let v = if incoming {
                                    Point::new(touch.x - controls[1].x, touch.y - controls[1].y)
                                } else {
                                    Point::new(controls[0].x - touch.x, controls[0].y - touch.y)
                                };
                                let dot = (v.x * tangent.x + v.y * tangent.y)
                                    / v.distance(Point::default());
                                assert!(dot > 0.9999, "incorrect side tangent: {dot}");
                                assert_eq!(
                                    side.appearance().head,
                                    if incoming { Head::None } else { Head::Full }
                                );
                                side.validate().unwrap();
                            }
                            assert_eq!(doc.file_json().unwrap(), saved);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn hidden_steps_keep_virtual_contacts_and_materialization_preserves_sides_and_roles() {
    for shape in [Shape::Ellipse, Shape::RoundedRectangle, Shape::Rectangle] {
        for reversible in [false, true] {
            let (mut doc, _) = fixture(shape, 4, false, false);
            let step = &mut doc.graphics[0].cycle.as_mut().unwrap().steps[0];
            step.bow_pt = 36.;
            step.reversible = reversible;
            let reagent = add_side(&mut doc, Point::new(450., -50.), true);
            let product = add_side(&mut doc, Point::new(350., 100.), false);
            let group_id = doc.next_id();
            doc.groups.push(crate::grouping::Group {
                id: group_id,
                members: std::iter::once(1)
                    .chain(doc.annotations.iter().map(|a| a.id))
                    .collect(),
                integral: false,
                layout: None,
            });
            let visible = cycles::drawn(&doc, graphic(&doc));
            doc.graphics[0].cycle.as_mut().unwrap().steps[0].hidden = true;
            doc.validate().unwrap();
            let reopened = Document::from_json(&doc.file_json().unwrap()).unwrap();
            assert_eq!(reopened.graphics[0].cycle, doc.graphics[0].cycle);
            let hidden = cycles::drawn(&doc, graphic(&doc));
            assert!(!hidden.arcs.contains(&Some(0)));
            let sides = |d: &Drawn| {
                d.arrows
                    .iter()
                    .zip(&d.arcs)
                    .filter(|(_, s)| s.is_none())
                    .map(|(a, _)| a.clone())
                    .collect::<Vec<_>>()
            };
            assert_eq!(sides(&hidden), sides(&visible));
            assert_eq!(
                sides(&hidden),
                sides(&cycles::drawn(&reopened, graphic(&reopened)))
            );
            let source = doc.file_json().unwrap();
            let (materialized, written) = cycles::materialized(&doc);
            assert_eq!(written[0].arcs.len(), 3);
            assert!(written[0].arcs.iter().all(|(s, _)| *s != 0));
            assert_eq!(
                materialized.graphics.len(),
                1,
                "headless incoming path stays editable"
            );
            assert_eq!(
                materialized.arrows.len(),
                4,
                "three step arrows plus headed product"
            );
            assert_eq!(
                materialized.graphics[0].commands(),
                sides(&hidden)[0].paths()[0].commands
            );
            let expected_product = &sides(&hidden)[1];
            let product_arrow = materialized
                .arrows
                .iter()
                .find(|a| !written[0].arcs.iter().any(|(_, id)| *id == a.id))
                .unwrap();
            assert_eq!(product_arrow.start, expected_product.start);
            assert_eq!(product_arrow.end, expected_product.end);
            assert_eq!(product_arrow.cubic, expected_product.cubic);
            let group = materialized
                .groups
                .iter()
                .find(|g| g.id == group_id)
                .unwrap();
            assert!(group.members.contains(&reagent) && group.members.contains(&product));
            assert!(written[0].drawn.iter().all(|id| group.members.contains(id)));
            let back = Document::from_json(&materialized.file_json().unwrap()).unwrap();
            assert_eq!(back.graphics, materialized.graphics);
            assert_eq!(back.arrows, materialized.arrows);
            assert_eq!(doc.file_json().unwrap(), source);
        }
    }
}

#[test]
fn inkless_parts_consume_their_ordinal_and_unusable_steps_preserve_membership() {
    let (mut doc, _) = fixture(Shape::Rectangle, 4, false, false);
    add_side(&mut doc, Point::new(450., -50.), true);
    add_side(&mut doc, Point::new(350., 100.), false);
    let expected = cycles::drawn(&doc, graphic(&doc));
    doc.graphics[0].cycle.as_mut().unwrap().steps[0]
        .reagents
        .insert(1, vec![]);
    doc.validate().unwrap();
    assert!(crate::scene::selection_bounds(&doc, &[]).is_none());
    let actual = cycles::drawn(&doc, graphic(&doc));
    assert_eq!(actual.arrows, expected.arrows);
    assert_eq!(actual.arcs, expected.arcs);
    // Distinct parts at the same position provide no usable step route.
    doc.annotations[1].position = doc.annotations[0].position;
    let saved = doc.file_json().unwrap();
    let actual = cycles::drawn(&doc, graphic(&doc));
    assert!(!actual.arcs.contains(&Some(0)));
    assert!(!actual.arcs.contains(&None));
    assert_eq!(doc.file_json().unwrap(), saved);
}

#[test]
fn circle_side_arrows_remain_exactly_legacy_and_lone_noncircle_sides_stay_finite() {
    for shape in [
        Shape::Circle,
        Shape::Ellipse,
        Shape::RoundedRectangle,
        Shape::Rectangle,
    ] {
        for n in [1, 4] {
            let (mut doc, _) = fixture(shape, n, true, false);
            let step = &mut doc.graphics[0].cycle.as_mut().unwrap().steps[0];
            step.bow_pt = 36.;
            step.reversible = true;
            add_side(&mut doc, Point::new(450., -50.), true);
            add_side(&mut doc, Point::new(350., 100.), false);
            let actual = cycles::drawn(&doc, graphic(&doc));
            if shape == Shape::Circle {
                let legacy = cycles::drawn_circle(&doc, graphic(&doc));
                assert_eq!(actual.arrows, legacy.arrows);
                assert_eq!(actual.arcs, legacy.arcs);
                assert_eq!(actual.labels, legacy.labels);
            }
            assert_eq!(actual.arcs.iter().filter(|s| s.is_none()).count(), 2);
            for arrow in actual.arrows {
                arrow.validate().unwrap();
                assert!(
                    arrow
                        .paths()
                        .iter()
                        .flat_map(|p| &p.commands)
                        .flat_map(|c| c.points())
                        .all(|p| p.is_finite())
                );
            }
        }
    }
}
