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
        let perimeter = outline(&doc.graphics[0]).unwrap();
        let on = perimeter.project(point).unwrap();
        let tangent = perimeter.tangents(on.s).unwrap().outgoing;
        let away = point.offset(
            tangent.y * 3. * doc.drawing_style.bond_length_world,
            -tangent.x * 3. * doc.drawing_style.bond_length_world,
        );
        for point in [Point::new(0., 0.), away] {
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
fn side_roles_follow_reverse_lane_travel_once_and_keep_the_actual_central_core_empty() {
    for shape in [Shape::Ellipse, Shape::RoundedRectangle, Shape::Rectangle] {
        for mirrored in [false, true] {
            let mut doc = fixture(shape, 2, mirrored, false);
            doc.graphics[0].cycle.as_mut().unwrap().steps[0].reversible = true;
            let geometry = Geometry::new(&doc, &doc.graphics[0]).unwrap();
            let route = geometry.route(0, 0, 1).unwrap();
            let reverse = Shaft::from_arrow(&route.arrows[1]).unwrap();
            let dir = geometry.dir as f32;
            let bond = doc.drawing_style.bond_length_world;
            let probes: Vec<_> = [0.2, 0.8]
                .into_iter()
                .map(|t| {
                    let (on, tangent) = reverse.sample(reverse.length() * t, false).unwrap();
                    // Reverse travel's right normal points to the actual inside.
                    on.offset(dir * tangent.y * 2. * bond, -dir * tangent.x * 2. * bond)
                })
                .collect();
            let free = text(&mut doc, probes[0], "Ar–X");
            for (point, expected) in probes.into_iter().zip([Role::Product(0), Role::Reagent(0)]) {
                place(&mut doc, free, point);
                assert_eq!(
                    cycles::drop_target(&doc, &[free]).unwrap().role,
                    expected,
                    "{shape:?}/{mirrored}"
                );
            }
            place(&mut doc, free, Point::new(0., 20.));
            assert!(cycles::drop_target(&doc, &[free]).is_none());
        }
    }
}

#[test]
fn bridge_full_lap_depends_on_endpoint_identity_not_coincident_projections() {
    let mut doc = fixture(Shape::Rectangle, 3, false, false);
    let first = doc.graphics[0].cycle.as_ref().unwrap().intermediates[0][0];
    let last = doc.graphics[0].cycle.as_ref().unwrap().intermediates[2][0];
    let at = center_of(crate::scene::selection_bounds(&doc, &[first]).unwrap());
    place(&mut doc, last, at);
    let geometry = Geometry::new(&doc, &doc.graphics[0]).unwrap();
    assert!(geometry.route(0, 0, 2).is_none());
    assert!(geometry.route(0, 0, 0).is_some());
    let mut doc = fixture(Shape::Ellipse, 1, false, false);
    doc.graphics[0].cycle.as_mut().unwrap().steps[0].bow_pt = 720.;
    let route = Geometry::new(&doc, &doc.graphics[0])
        .unwrap()
        .route(0, 0, 0)
        .unwrap();
    let point = route.arrows[0].point(0.5);
    let free = text(&mut doc, point, "Pd insert");
    let target = cycles::drop_target(&doc, &[free]).unwrap();
    assert_eq!(target.role, Role::Intermediate(1));
    cycles::assign(&mut doc, &target);
    assert!(cycles::holds(&doc, 1, &[free]));
    cycles::reverse(doc.graphics[0].cycle.as_mut().unwrap());
    assert!(cycles::holds(&doc, 1, &[free]));
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
fn a_background_blocked_bow_does_not_mask_a_painted_foreground_target() {
    let mut doc = fixture(Shape::Rectangle, 4, false, false);
    doc.graphics[0].cycle.as_mut().unwrap().steps[0].bow_pt = -720.;
    doc.graphics[0].layer = -10;
    let geometry = Geometry::new(&doc, &doc.graphics[0]).unwrap();
    let route = geometry.route(0, 0, 1).unwrap();
    let point = (1..100)
        .map(|i| route.arrows[0].point(i as f32 / 100.))
        .find(|p| !geometry.splittable(&route, *p))
        .unwrap();
    let foreground = cycles::insert(&mut doc, point.offset(-100., 0.), 100.);
    let g = doc.graphics.last_mut().unwrap();
    g.layer = 10;
    g.cycle.as_mut().unwrap().shape = Shape::Rectangle;
    doc.graphics.swap(0, 1); // The background was appended later in the document.
    let free = text(&mut doc, point, "Pd");
    let target = cycles::drop_target(&doc, &[free]).unwrap();
    assert_eq!(target.cycle, foreground);
    assert_eq!(target.role, Role::Intermediate(0));
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

#[test]
fn finite_large_native_frames_do_not_depend_on_the_overflowing_legacy_radius_seed() {
    let mut doc = fixture(Shape::Ellipse, 0, false, false);
    let g = &mut doc.graphics[0];
    g.origin = Point::default();
    g.axis_x = Point::new(1e20, 0.);
    g.axis_y = Point::new(0., 1e20);
    let center = g.cycle_circle().0;
    assert!(g.cycle_circle().1.is_infinite());
    let perimeter = outline(g).unwrap();
    assert!(perimeter_arcs::empty_arrow(g, &perimeter).is_some());
    let part = text(&mut doc, center, "Pd");
    let c = doc.graphics[0].cycle.as_mut().unwrap();
    c.intermediates.push(vec![part]);
    c.steps.push(Step::default());
    doc.validate().unwrap();
    assert!(cycles::holds(&doc, 1, &[part]));
}

#[test]
fn remote_zero_bow_reversible_corner_keeps_the_inserted_member_after_nudge_and_reverse() {
    for mirrored in [false, true] {
        let mut doc = fixture(Shape::Rectangle, 2, false, false);
        let g = &mut doc.graphics[0];
        g.origin = Point::new(if mirrored { 1000. } else { 0. }, 0.);
        g.axis_x = Point::new(if mirrored { -1000. } else { 1000. }, 0.);
        g.axis_y = Point::new(if mirrored { -1000. } else { 1000. }, 10.);
        let c = g.cycle.as_mut().unwrap();
        c.steps[0].reversible = true;
        let [first, last] = [c.intermediates[0][0], c.intermediates[1][0]];
        place(&mut doc, first, Point::new(500., 5.));
        place(&mut doc, last, Point::new(500., 0.));
        let geometry = Geometry::new(&doc, &doc.graphics[0]).unwrap();
        let route = geometry.route(0, 0, 1).unwrap();
        let point = route.arrows[0]
            .nodes
            .iter()
            .map(|node| node.point)
            .max_by(|a, b| {
                geometry
                    .outline
                    .project(*a)
                    .unwrap()
                    .distance
                    .total_cmp(&geometry.outline.project(*b).unwrap().distance)
            })
            .unwrap();
        assert!(
            geometry.outline.project(point).unwrap().distance
                > f64::from(4. * doc.drawing_style.bond_length_world),
            "mirror={mirrored} p={point:?} projection={:?} reach={} nodes={:?}",
            geometry.outline.project(point),
            4. * doc.drawing_style.bond_length_world,
            route.arrows[0].nodes,
        );
        let free = text(&mut doc, point, "Pd insert");
        let target = cycles::drop_target(&doc, &[free]).unwrap();
        assert_eq!(target.role, Role::Intermediate(1));
        cycles::assign(&mut doc, &target);
        let delta = Point::new(0.125, -0.0625);
        doc.translate(&[free], delta.x, delta.y);
        assert!(doc.take_out(&[free], delta).is_empty());
        assert!(cycles::holds(&doc, 1, &[free]));
        cycles::reverse(doc.graphics[0].cycle.as_mut().unwrap());
        assert!(cycles::holds(&doc, 1, &[free]));
        doc.validate().unwrap();
    }
}
