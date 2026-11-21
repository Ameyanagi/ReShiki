use super::*;
use crate::{
    cycles::{self, Cycle, Shape, Step},
    document::Annotation,
    graphics::{GraphicKind, GraphicStyle},
    typography::TextFormat,
};

fn text(doc: &mut Document, center: Point, label: &str) -> u64 {
    let id = doc.next_id();
    let mut annotation = Annotation {
        id,
        angle: 0.,
        position: Point::default(),
        text: label.into(),
        format: TextFormat {
            style: doc.drawing_style.text_style(),
            ..Default::default()
        },
    };
    let (w, h) = annotation.size();
    annotation.position = center.offset(-w * 0.5, -h * 0.5);
    doc.annotations.push(annotation);
    id
}

fn fixture(shape: Shape, n: usize, mirrored: bool, ccw: bool) -> Document {
    let mut doc = Document::default();
    let mut graphic = Graphic::dragged(
        1,
        GraphicKind::Ellipse,
        Point::new(-900., -150.),
        Point::new(900., 150.),
        GraphicStyle::default(),
        Default::default(),
        false,
    );
    if mirrored {
        graphic.origin.x += graphic.axis_x.x;
        graphic.axis_x.x = -graphic.axis_x.x;
    }
    graphic.cycle = Some(Box::new(Cycle {
        shape,
        counterclockwise: ccw,
        ..Default::default()
    }));
    doc.graphics.push(graphic);
    let g = &doc.graphics[0];
    let outline = outline(g).unwrap();
    let top = outline.project(Point::new(0., -300.)).unwrap().s;
    let dir = if g.cycle_counterclockwise() { -1. } else { 1. };
    let mut parts = vec![];
    for i in 0..n {
        let center = outline
            .point(top + dir * outline.length() * i as f64 / n as f64)
            .unwrap();
        parts.push(vec![text(&mut doc, center, "Pd")]);
    }
    let c = doc.graphics[0].cycle.as_mut().unwrap();
    c.intermediates = parts;
    c.steps = vec![Step::default(); n];
    doc.validate().unwrap();
    doc
}

fn place(doc: &mut Document, id: u64, point: Point) {
    let from = center_of(crate::scene::selection_bounds(doc, &[id]).unwrap());
    doc.translate(&[id], point.x - from.x, point.y - from.y);
}

#[test]
fn native_tip_drops_use_visible_unnamed_shafts_in_both_directions_and_reflections() {
    for shape in [Shape::Ellipse, Shape::RoundedRectangle, Shape::Rectangle] {
        for mirrored in [false, true] {
            for ccw in [false, true] {
                let mut doc = fixture(shape, 2, mirrored, ccw);
                let geometry = Geometry::new(&doc, &doc.graphics[0]).unwrap();
                let route = geometry.route(0, 0, 1).unwrap();
                let point = Shaft::from_arrow(&route.arrows[0])
                    .unwrap()
                    .sample(
                        Shaft::from_arrow(&route.arrows[0]).unwrap().length() * 0.5,
                        false,
                    )
                    .unwrap()
                    .0;
                let (center, radius) = doc.graphics[0].cycle_circle();
                assert!(point.distance(center) > radius + 4. * doc.drawing_style.bond_length_world);
                let free = text(&mut doc, point, "Ar–X");
                let target = cycles::drop_target(&doc, &[free]).unwrap();
                assert_eq!(
                    target.role,
                    Role::Intermediate(1),
                    "{shape:?}/{mirrored}/{ccw}"
                );
                assert_eq!(target.tag, "Insert between 1 and 2");
                assert!(!target.arc.is_empty());
                assert_eq!(cycles::step_at(&doc, &doc.graphics[0], point), Some(0));
                cycles::assign(&mut doc, &target);
                doc.validate().unwrap();
                assert_eq!(
                    cycles::role_of(&doc, free),
                    Some((1, Role::Intermediate(1)))
                );
            }
        }
    }
}

#[test]
fn actual_sharp_corners_and_arrowheads_are_insert_targets() {
    let mut doc = fixture(Shape::Rectangle, 2, true, true);
    let geometry = Geometry::new(&doc, &doc.graphics[0]).unwrap();
    let route = geometry.route(0, 0, 1).unwrap();
    let corner = route.arrows[0]
        .nodes
        .iter()
        .map(|n| n.point)
        .find(|p| (p.x.abs() - 900.).abs() < 0.01 && (p.y.abs() - 150.).abs() < 0.01)
        .unwrap();
    let tip = route.arrows[0].end;
    let free = text(&mut doc, corner, "Ar–X");
    assert_eq!(
        cycles::drop_target(&doc, &[free]).unwrap().role,
        Role::Intermediate(1)
    );
    place(&mut doc, free, tip);
    assert_eq!(
        cycles::drop_target(&doc, &[free]).unwrap().role,
        Role::Intermediate(1)
    );
}

#[test]
fn empty_native_cycle_only_accepts_its_visible_open_arrow() {
    for shape in [Shape::Ellipse, Shape::RoundedRectangle, Shape::Rectangle] {
        let mut doc = fixture(shape, 0, false, false);
        let arrow =
            perimeter_arcs::empty_arrow(&doc.graphics[0], &outline(&doc.graphics[0]).unwrap())
                .unwrap();
        let point = arrow.point(0.3);
        let free = text(&mut doc, point, "Pd");
        assert_eq!(
            cycles::drop_target(&doc, &[free]).unwrap().role,
            Role::Intermediate(0)
        );
        for point in [
            Point::new(0., 0.),
            point.offset(0., -3. * doc.drawing_style.bond_length_world),
        ] {
            place(&mut doc, free, point);
            assert!(cycles::drop_target(&doc, &[free]).is_none());
        }
    }
}

#[test]
fn old_mean_circle_zone_and_hidden_shafts_do_not_collect_loose_parts() {
    let mut doc = fixture(Shape::Ellipse, 2, false, false);
    let (center, radius) = doc.graphics[0].cycle_circle();
    let free = text(&mut doc, center.offset(0., -radius), "Ar–X");
    assert!(cycles::drop_target(&doc, &[free]).is_none());
    let route = Geometry::new(&doc, &doc.graphics[0])
        .unwrap()
        .route(0, 0, 1)
        .unwrap();
    let p = route.arrows[0].point(0.5);
    doc.graphics[0].cycle.as_mut().unwrap().steps[0].hidden = true;
    place(&mut doc, free, p);
    assert!(cycles::drop_target(&doc, &[free]).is_none());
    place(&mut doc, free, center);
    assert!(cycles::drop_target(&doc, &[free]).is_none());
}

#[test]
fn far_bowed_insert_preserves_ownership_steps_and_inherited_side_positions_after_reverse() {
    for shape in [Shape::Ellipse, Shape::RoundedRectangle, Shape::Rectangle] {
        for reversible in [false, true] {
            let mut doc = fixture(shape, 2, false, false);
            let side = text(&mut doc, Point::new(1100., -50.), "Ar–X");
            let c = doc.graphics[0].cycle.as_mut().unwrap();
            c.steps[0].name = "Oxidative addition".into();
            c.steps[0].bow_pt = 720.;
            c.steps[0].reversible = reversible;
            c.steps[0].reagents.push(vec![side]);
            let side_before = doc
                .annotations
                .iter()
                .find(|a| a.id == side)
                .unwrap()
                .clone();
            let geometry = Geometry::new(&doc, &doc.graphics[0]).unwrap();
            let route = geometry.route(0, 0, 1).unwrap();
            let point = route.arrows[0].point(0.5);
            assert!(
                geometry.outline.project(point).unwrap().distance
                    > f64::from(5. * doc.drawing_style.bond_length_world)
            );
            let free = text(&mut doc, point, "Pd insert");
            let target = cycles::drop_target(&doc, &[free]).unwrap();
            assert_eq!(target.role, Role::Intermediate(1));
            cycles::assign(&mut doc, &target);
            assert_eq!(
                doc.annotations.iter().find(|a| a.id == side).unwrap(),
                &side_before
            );
            let c = doc.graphics[0].cycle.as_mut().unwrap();
            c.steps[1].name = "Preserve the inserted outgoing step".into();
            c.steps[1].lead_pt = 13.;
            c.steps[1].trail_pt = 7.;
            c.steps[1].reversible = true;
            for reverse in [false, true] {
                if reverse {
                    cycles::reverse(doc.graphics[0].cycle.as_mut().unwrap());
                }
                let saved_steps = doc.graphics[0].cycle.as_ref().unwrap().steps.clone();
                let delta = Point::new(0.25, -0.125);
                doc.translate(&[free], delta.x, delta.y);
                assert!(
                    cycles::holds(&doc, 1, &[free]),
                    "{shape:?}/{reversible}/{reverse}"
                );
                assert!(doc.take_out(&[free], delta).is_empty());
                let saved = Document::from_json(&doc.file_json().unwrap()).unwrap();
                assert!(cycles::holds(&saved, 1, &[free]));
                assert_eq!(saved.graphics[0].cycle.as_ref().unwrap().steps, saved_steps);
            }
            place(&mut doc, free, Point::new(-2500., 0.));
            assert!(!cycles::holds(&doc, 1, &[free]));
            assert!(
                doc.take_out(&[free], Point::new(-1., 0.))
                    .contains(&crate::grouping::Container::Cycle)
            );
        }
    }
}

#[test]
fn inward_bow_cannot_split_outside_its_directed_native_interval_or_fall_through() {
    let mut doc = fixture(Shape::Rectangle, 4, false, false);
    doc.graphics[0].cycle.as_mut().unwrap().steps[0].bow_pt = -720.;
    let geometry = Geometry::new(&doc, &doc.graphics[0]).unwrap();
    let route = geometry.route(0, 0, 1).unwrap();
    let point = (1..100)
        .map(|i| route.arrows[0].point(i as f32 / 100.))
        .find(|p| !geometry.splittable(&route, *p))
        .expect("strong inward Bow must cross native donor interval");
    let mut behind = doc.graphics[0].clone();
    behind.id = 100;
    doc.graphics.insert(0, behind);
    let free = text(&mut doc, point, "Pd");
    assert!(cycles::drop_target(&doc, &[free]).is_none());
    assert_eq!(
        doc.graphics[1].cycle.as_ref().unwrap().intermediates.len(),
        4
    );
}

#[test]
fn hidden_step_decorations_keep_their_owner_and_malformed_frames_miss() {
    let mut doc = fixture(Shape::Rectangle, 4, false, false);
    let side = text(&mut doc, Point::new(1100., -100.), "Ar–X");
    let c = doc.graphics[0].cycle.as_mut().unwrap();
    c.steps[0].hidden = true;
    c.steps[0].name = "Oxidative addition".into();
    c.steps[0].reagents.push(vec![side]);
    let mut drawn = Drawn::default();
    let mut owners = perimeter_arcs::Owners::default();
    perimeter_arcs::resolve_owned(
        &doc,
        &doc.graphics[0],
        &outline(&doc.graphics[0]).unwrap(),
        &mut drawn,
        &mut owners,
        |_, _| {},
    );
    let (at, text, format) = &drawn.labels[0];
    let layout = crate::typography::layout(text, format);
    assert_eq!(
        cycles::step_at(
            &doc,
            &doc.graphics[0],
            at.offset(layout.width * 0.5, layout.height * 0.5)
        ),
        Some(0)
    );
    let arrow = drawn
        .arrows
        .iter()
        .zip(&drawn.arcs)
        .find(|(_, s)| s.is_none())
        .unwrap()
        .0;
    assert_eq!(
        cycles::step_at(&doc, &doc.graphics[0], arrow.point(0.5)),
        Some(0)
    );
    let free = text_at_origin(&mut doc);
    for axis in [Point::default(), Point::new(f32::NAN, 0.)] {
        doc.graphics[0].axis_x = axis;
        assert!(cycles::drop_target(&doc, &[free]).is_none());
        assert!(!cycles::holds(&doc, 1, &[side]));
    }
}

fn text_at_origin(doc: &mut Document) -> u64 {
    text(doc, Point::default(), "loose")
}
