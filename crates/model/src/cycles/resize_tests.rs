use super::*;
use crate::{
    cycles::{Cycle, Shape, Step, set_radius},
    document::Annotation,
    graphics::{Graphic, GraphicKind},
    typography::TextFormat,
};

fn fixture(shape: Shape, degrees: f32, mirrored: bool, skew: bool, ccw: bool) -> Document {
    let center = Point::new(120., -80.);
    let (s, c) = degrees.to_radians().sin_cos();
    let turn = |p: Point| Point::new(c * p.x - s * p.y, s * p.x + c * p.y);
    let x = turn(Point::new(if mirrored { -600. } else { 600. }, 0.));
    let y = turn(Point::new(if skew { 90. } else { 0. }, 200.));
    let mut g = Graphic::dragged(
        1,
        GraphicKind::Ellipse,
        Point::default(),
        Point::new(600., 200.),
        Default::default(),
        Default::default(),
        false,
    );
    g.axis_x = x;
    g.axis_y = y;
    g.origin = center.offset(-0.5 * (x.x + y.x), -0.5 * (x.y + y.y));
    g.cycle = Some(Box::new(Cycle {
        shape,
        counterclockwise: ccw,
        ..Default::default()
    }));
    let mut doc = Document::default();
    doc.graphics.push(g);
    doc.validate().unwrap();
    doc
}

fn norm(p: Point) -> f64 {
    f64::from(p.x).hypot(f64::from(p.y))
}
fn dot(a: Point, b: Point) -> f64 {
    f64::from(a.x) * f64::from(b.x) + f64::from(a.y) * f64::from(b.y)
}
fn near(a: Point, b: Point) {
    assert!(
        norm(Point::new(a.x - b.x, a.y - b.y)) < 0.003,
        "{a:?} != {b:?}"
    );
}
fn number(a: f64, b: f64) {
    assert!((a - b).abs() < 0.003 + b.abs() * 0.00001, "{a} != {b}");
}
fn snapshot(doc: &Document) -> serde_json::Value {
    serde_json::to_value(doc).unwrap()
}

fn projected(perimeter: &crate::cycles::Outline, axis: Point) -> [f64; 2] {
    use crate::graphics::PathCommand;
    let normal = Point::new(-axis.y, axis.x);
    let p = |p: Point| (dot(p, axis), dot(p, normal));
    let mut path = kurbo::BezPath::new();
    for c in perimeter.subpath(0., perimeter.length()).unwrap() {
        match c {
            PathCommand::Move(a) => path.move_to(p(a)),
            PathCommand::Line(a) => path.line_to(p(a)),
            PathCommand::Cubic(a, b, c) => path.curve_to(p(a), p(b), p(c)),
            PathCommand::Close => path.close_path(),
        }
    }
    let bounds = kurbo::Shape::bounding_box(&path);
    [bounds.width(), bounds.height()]
}

#[test]
fn resizing_preserves_smallest_turn_and_projected_dimensions_across_quarter_turns() {
    for shape in [Shape::Ellipse, Shape::RoundedRectangle, Shape::Rectangle] {
        for degrees in [44.9, 90.1, 179.8] {
            for mirror in [false, true] {
                for skew in [false, true] {
                    for ccw in [false, true] {
                        for ratio in [0.6, 1.7] {
                            let mut doc = fixture(shape, degrees, mirror, skew, ccw);
                            let before = doc.graphics[0].clone();
                            let (center, radius) = before.cycle_circle();
                            let perimeter = outline(&before).unwrap();
                            set_radius(&mut doc, 1, radius * ratio);
                            let after = &doc.graphics[0];
                            near(after.cycle_circle().0, center);
                            number(f64::from(after.cycle_circle().1), f64::from(radius * ratio));
                            assert_eq!(after.cycle, before.cycle);
                            assert_eq!(after.style, before.style);
                            assert_eq!(
                                after.cycle_counterclockwise(),
                                before.cycle_counterclockwise()
                            );
                            assert_eq!(
                                after.frame_determinant().is_sign_negative(),
                                before.frame_determinant().is_sign_negative()
                            );
                            for (a, b) in
                                [(before.axis_x, after.axis_x), (before.axis_y, after.axis_y)]
                            {
                                let cross = f64::from(a.x) * f64::from(b.y)
                                    - f64::from(a.y) * f64::from(b.x);
                                let turn = cross.atan2(dot(a, b)).to_degrees();
                                assert!(
                                    turn.abs() < 0.0001,
                                    "{shape:?} at {degrees}° changed by {turn}°"
                                );
                                number(norm(b), norm(a) * f64::from(ratio));
                            }
                            // Project onto both retained local axes and world axes. This
                            // catches long/short-axis swaps hidden by a world AABB.
                            let length = norm(before.axis_x) as f32;
                            let u = Point::new(before.axis_x.x / length, before.axis_x.y / length);
                            for axis in [
                                u,
                                Point::new(-u.y, u.x),
                                Point::new(1., 0.),
                                Point::new(0., 1.),
                            ] {
                                let dimension = |g: &Graphic| {
                                    dot(g.axis_x, axis).abs() + dot(g.axis_y, axis).abs()
                                };
                                number(dimension(after), dimension(&before) * f64::from(ratio));
                                for (a, b) in projected(&outline(after).unwrap(), axis)
                                    .into_iter()
                                    .zip(projected(&perimeter, axis))
                                {
                                    number(a, b * f64::from(ratio));
                                }
                            }
                            let next = outline(after).unwrap();
                            number(next.length(), perimeter.length() * f64::from(ratio));
                            for t in [0., 0.07, 0.25, 0.51, 0.85, 0.99] {
                                let p = perimeter.point(perimeter.length() * t).unwrap();
                                near(
                                    next.point(next.length() * t).unwrap(),
                                    center
                                        .offset((p.x - center.x) * ratio, (p.y - center.y) * ratio),
                                );
                            }
                            doc.validate().unwrap();
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn a_shorter_first_axis_is_not_swapped_with_the_longer_second_axis() {
    for shape in [Shape::Ellipse, Shape::RoundedRectangle, Shape::Rectangle] {
        let mut doc = fixture(shape, 90.1, true, false, false);
        let g = &mut doc.graphics[0];
        std::mem::swap(&mut g.axis_x, &mut g.axis_y);
        let before = doc.graphics[0].clone();
        let radius = before.cycle_circle().1;
        set_radius(&mut doc, 1, radius * 1.25);
        assert!(norm(doc.graphics[0].axis_x) < norm(doc.graphics[0].axis_y));
        near(
            doc.graphics[0].axis_x,
            Point::new(before.axis_x.x * 1.25, before.axis_x.y * 1.25),
        );
        near(
            doc.graphics[0].axis_y,
            Point::new(before.axis_y.x * 1.25, before.axis_y.y * 1.25),
        );
        for _ in 0..8 {
            set_radius(&mut doc, 1, radius * 0.8);
            set_radius(&mut doc, 1, radius);
        }
        near(doc.graphics[0].axis_x, before.axis_x);
        near(doc.graphics[0].axis_y, before.axis_y);
        near(doc.graphics[0].origin, before.origin);
    }
}

#[test]
fn a_large_origin_does_not_amplify_center_rounding_during_resize() {
    for degrees in [0_f32, 37.] {
        for mirror in [false, true] {
            let mut doc = fixture(Shape::Ellipse, degrees, mirror, false, false);
            let (s, c) = degrees.to_radians().sin_cos();
            doc.graphics[0].origin = Point::new(1e8, 1e8);
            doc.graphics[0].axis_x = Point::new(601. * c, 601. * s);
            if mirror {
                doc.graphics[0].axis_x.x *= -1.;
                doc.graphics[0].axis_x.y *= -1.;
            }
            doc.graphics[0].axis_y = Point::new(-201. * s, 201. * c);
            let before = doc.graphics[0].cycle_circle().0;
            doc.validate().unwrap();
            set_radius(&mut doc, 1, 2000.);
            let after = doc.graphics[0].cycle_circle().0;
            for (old, new) in [(before.x, after.x), (before.y, after.y)] {
                let ulp = f32::from_bits(old.to_bits() + 1) - old;
                assert!(
                    (new - old).abs() <= ulp,
                    "center drift {before:?} → {after:?} exceeds coordinate ULP {ulp}"
                );
            }
            number(f64::from(doc.graphics[0].cycle_circle().1), 2000.);
        }
    }
}

fn text(doc: &mut Document, center: Point, value: &str) -> u64 {
    let id = doc.next_id();
    let mut a = Annotation {
        id,
        angle: 0.,
        position: Point::default(),
        text: value.into(),
        format: TextFormat {
            style: doc.drawing_style.text_style(),
            ..Default::default()
        },
    };
    let (w, h) = a.size();
    a.position = center.offset(-w * 0.5, -h * 0.5);
    doc.annotations.push(a);
    id
}

#[test]
fn part_centers_slide_but_molecules_text_and_linked_caption_offsets_keep_their_sizes() {
    for shape in [Shape::Ellipse, Shape::RoundedRectangle, Shape::Rectangle] {
        let mut doc = fixture(shape, 37., true, true, true);
        let a = doc.add_atom("C", Point::new(30., -190.));
        let b = doc.add_atom("C", Point::new(72., -180.));
        doc.add_bond(a, b, 1, "plain");
        let atoms = vec![a, b];
        let identity = crate::molecule_names::identity(&doc, &atoms).unwrap();
        let name = crate::molecule_names::show(
            &mut doc,
            &atoms,
            "ethane".into(),
            identity,
            Default::default(),
            false,
        )
        .unwrap();
        crate::linked_properties::toggle(
            &mut doc,
            &atoms,
            crate::linked_properties::Property::Formula,
            Default::default(),
            false,
        )
        .unwrap();
        let property = doc.property_captions[0].annotation;
        let intermediate = text(&mut doc, Point::new(390., 30.), "Pd");
        let reagent = text(&mut doc, Point::new(490., -70.), "Ar–X");
        let product = text(&mut doc, Point::new(310., 170.), "X−");
        let unrelated = text(&mut doc, Point::new(1300., -80.), "Unrelated");
        let cycle = doc.graphics[0].cycle.as_mut().unwrap();
        cycle.intermediates = vec![atoms.clone(), vec![intermediate]];
        cycle.steps = vec![Step::default(); 2];
        cycle.steps[0].reagents.push(vec![reagent]);
        cycle.steps[0].products.push(vec![product]);
        cycle.steps[0].bow_pt = 12.;
        cycle.steps[0].lead_pt = 7.;
        cycle.steps[0].name = "OA".into();
        doc.validate().unwrap();
        let old = doc.clone();
        let center = doc.graphics[0].cycle_circle().0;
        let radius = doc.graphics[0].cycle_circle().1;
        let ratio = 1.4;
        let parts = vec![
            atoms.clone(),
            vec![intermediate],
            vec![reagent],
            vec![product],
        ];
        let bounds = crate::scene::selections_bounds(&doc, &parts);
        set_radius(&mut doc, 1, radius * ratio);
        for (ids, bounds) in parts.iter().zip(bounds) {
            let p = center_of(bounds.unwrap());
            let expected = center.offset((p.x - center.x) * ratio, (p.y - center.y) * ratio);
            near(
                center_of(crate::scene::selection_bounds(&doc, ids).unwrap()),
                expected,
            );
        }
        let delta = Point::new(
            doc.atom(a).unwrap().position.x - old.atom(a).unwrap().position.x,
            doc.atom(a).unwrap().position.y - old.atom(a).unwrap().position.y,
        );
        for id in [name, property] {
            let before = old.annotations.iter().find(|a| a.id == id).unwrap();
            let after = doc.annotations.iter().find(|a| a.id == id).unwrap();
            near(after.position, before.position.offset(delta.x, delta.y));
            assert_eq!(after.format, before.format);
            assert_eq!(after.size(), before.size());
        }
        number(
            norm(Point::new(
                doc.atom(b).unwrap().position.x - doc.atom(a).unwrap().position.x,
                doc.atom(b).unwrap().position.y - doc.atom(a).unwrap().position.y,
            )),
            norm(Point::new(
                old.atom(b).unwrap().position.x - old.atom(a).unwrap().position.x,
                old.atom(b).unwrap().position.y - old.atom(a).unwrap().position.y,
            )),
        );
        assert_eq!(
            doc.annotations.iter().find(|a| a.id == unrelated),
            old.annotations.iter().find(|a| a.id == unrelated)
        );
        assert_eq!(doc.graphics[0].cycle, old.graphics[0].cycle);
        assert_eq!(doc.graphics[0].style, old.graphics[0].style);
        let out = crate::cycles::drawn(&doc, &doc.graphics[0]);
        assert!(out.arrows.iter().all(|a| {
            a.paths()
                .iter()
                .flat_map(|p| &p.commands)
                .flat_map(|c| c.points())
                .all(|p| p.is_finite())
        }));
        let back = Document::from_json(&doc.file_json().unwrap()).unwrap();
        assert_eq!(back.file_json().unwrap(), doc.file_json().unwrap());
        let same = snapshot(&doc);
        let radius = doc.graphics[0].cycle_circle().1;
        set_radius(&mut doc, 1, radius);
        assert_eq!(snapshot(&doc), same);
    }
}

#[test]
fn explicit_corner_radius_stays_absolute_while_auto_corners_follow_the_frame() {
    let mut doc = fixture(Shape::RoundedRectangle, 31., false, false, false);
    doc.graphics[0].style.corner_pt = Some(12.);
    let original = doc.graphics[0].corner_radius();
    let radius = doc.graphics[0].cycle_circle().1;
    set_radius(&mut doc, 1, radius * 2.);
    number(
        f64::from(doc.graphics[0].corner_radius()),
        f64::from(original),
    );
    set_radius(&mut doc, 1, radius * 0.1);
    assert!(doc.graphics[0].corner_radius() < original);
    set_radius(&mut doc, 1, radius);
    number(
        f64::from(doc.graphics[0].corner_radius()),
        f64::from(original),
    );
    assert_eq!(doc.graphics[0].style.corner_pt, Some(12.));
}

#[test]
fn invalid_requests_and_unrepresentable_later_parts_leave_the_whole_document_unchanged() {
    for shape in [Shape::Ellipse, Shape::RoundedRectangle, Shape::Rectangle] {
        for radius in [
            0.,
            -1.,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::MAX,
            f32::from_bits(1),
        ] {
            let mut doc = fixture(shape, 70., true, true, false);
            text(&mut doc, Point::new(300., 100.), "Pd");
            doc.graphics[0].cycle.as_mut().unwrap().intermediates = vec![vec![2]];
            doc.graphics[0].cycle.as_mut().unwrap().steps = vec![Step::default()];
            let before = snapshot(&doc);
            set_radius(&mut doc, 1, radius);
            assert_eq!(snapshot(&doc), before, "{shape:?}, {radius}");
        }
        let mut doc = fixture(shape, 0., false, false, false);
        let first = text(&mut doc, Point::new(300., 100.), "Pd");
        let later = text(&mut doc, Point::new(3e38, 0.), "X");
        let c = doc.graphics[0].cycle.as_mut().unwrap();
        c.intermediates = vec![vec![first]];
        c.steps = vec![Step::default()];
        c.steps[0].reagents.push(vec![later]);
        doc.validate().unwrap();
        let before = snapshot(&doc);
        let radius = doc.graphics[0].cycle_circle().1;
        set_radius(&mut doc, 1, radius * 1.5);
        assert_eq!(
            snapshot(&doc),
            before,
            "a bad later part must not move the first one"
        );
    }
    // Both axes and the mean radius can remain finite while the renderer's
    // f32 determinant underflows to -0 and reverses a mirrored cycle's way.
    let mut doc = fixture(Shape::Ellipse, 0., true, false, false);
    doc.graphics[0].origin = Point::default();
    doc.graphics[0].axis_x = Point::new(-1e-18, 0.);
    doc.graphics[0].axis_y = Point::new(0., 1e-21);
    doc.validate().unwrap();
    assert!(doc.graphics[0].cycle_counterclockwise());
    let before = snapshot(&doc);
    set_radius(&mut doc, 1, 1e-22);
    assert_eq!(
        snapshot(&doc),
        before,
        "reject a size that loses the retained direction"
    );
}

#[test]
fn tiny_valid_frames_can_be_rescued_and_same_radius_is_an_exact_noop() {
    let mut doc = fixture(Shape::Ellipse, 0., false, false, false);
    doc.graphics[0].origin = Point::default();
    doc.graphics[0].axis_x = Point::new(6e-30, 0.);
    doc.graphics[0].axis_y = Point::new(0., 2e-30);
    assert_eq!(doc.graphics[0].cycle_circle().1, 0.);
    assert!(outline(&doc.graphics[0]).is_some());
    set_radius(&mut doc, 1, 200.);
    number(f64::from(doc.graphics[0].cycle_circle().1), 200.);
    doc.validate().unwrap();
    let original = snapshot(&doc);
    set_radius(&mut doc, 1, 200.);
    assert_eq!(snapshot(&doc), original);
}

#[test]
fn collapsed_molecule_members_move_together_without_resizing_internal_bonds() {
    let mut doc = fixture(Shape::Ellipse, 35., false, false, false);
    let a = doc.add_atom("C", Point::new(300., 0.));
    let b = doc.add_atom("C", Point::new(342., 0.));
    doc.add_bond(a, b, 1, "plain");
    doc.contract(&[a, b], "Et", "").unwrap();
    let c = doc.graphics[0].cycle.as_mut().unwrap();
    c.intermediates = vec![vec![a, b]];
    c.steps = vec![Step::default()];
    let radius = doc.graphics[0].cycle_circle().1;
    let old = doc.clone();
    set_radius(&mut doc, 1, radius * 1.5);
    let delta = Point::new(
        doc.atom(a).unwrap().position.x - old.atom(a).unwrap().position.x,
        doc.atom(a).unwrap().position.y - old.atom(a).unwrap().position.y,
    );
    near(
        doc.atom(b).unwrap().position,
        old.atom(b).unwrap().position.offset(delta.x, delta.y),
    );
    assert!(norm(delta) > 20.);
    assert_eq!(doc.abbreviations, old.abbreviations);
    doc.validate().unwrap();
}

#[test]
fn ordinary_circle_resize_keeps_the_legacy_axis_reset_and_reflection() {
    for mirror in [false, true] {
        let mut doc = fixture(Shape::Circle, 47., mirror, true, true);
        let center = doc.graphics[0].cycle_circle().0;
        set_radius(&mut doc, 1, 400.);
        let g = &doc.graphics[0];
        assert_eq!(
            g.origin,
            center.offset(-400., if mirror { 400. } else { -400. })
        );
        assert_eq!(g.axis_x, Point::new(800., 0.));
        assert_eq!(g.axis_y, Point::new(0., if mirror { -800. } else { 800. }));
        assert_eq!(g.cycle_counterclockwise(), !mirror);
        number(f64::from(g.cycle_circle().1), 400.);
    }
}
