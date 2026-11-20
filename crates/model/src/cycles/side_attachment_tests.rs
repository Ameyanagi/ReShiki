use super::{Role, Target, assign, drawn, insert};
use crate::{
    arrows::{ArrowStyle, Head, Preset},
    document::{Annotation, Arrow, Document, Point},
    graphics::Graphic,
    typography::TextFormat,
};
use std::f32::consts::TAU;

fn graphic(doc: &Document, id: u64) -> &Graphic {
    doc.graphics.iter().find(|g| g.id == id).unwrap()
}

fn text(doc: &mut Document, at: Point, label: &str) -> u64 {
    let id = doc.next_id();
    doc.annotations.push(Annotation {
        id,
        position: at,
        text: label.into(),
        format: TextFormat {
            style: doc.drawing_style.text_style(),
            ..Default::default()
        },
    });
    let (a, b) = crate::scene::selection_bounds(doc, &[id]).unwrap();
    let center = a.lerp(b, 0.5);
    doc.translate(&[id], at.x - center.x, at.y - center.y);
    id
}

fn part(doc: &mut Document, cycle: u64, role: Role, at: Point, label: &str) {
    let id = text(doc, at, label);
    let target = Target {
        cycle,
        role,
        part: vec![id],
        ink: crate::scene::selection_bounds(doc, &[id]).unwrap(),
        arc: vec![],
        tag: String::new(),
    };
    assign(doc, &target);
}

fn fixture(reflected: bool, bow: f32) -> (Document, u64) {
    let mut doc = Document::default();
    let id = insert(&mut doc, Point::default(), 400.);
    if reflected {
        let g = doc.graphics.iter_mut().find(|g| g.id == id).unwrap();
        g.origin.y += g.axis_y.y;
        g.axis_y.y = -g.axis_y.y;
    }
    for (index, degrees) in [-120_f32, 0., 120.].into_iter().enumerate() {
        let angle = degrees.to_radians();
        part(
            &mut doc,
            id,
            Role::Intermediate(index),
            Point::new(400. * angle.cos(), 400. * angle.sin()),
            "Pd",
        );
    }
    let step = &mut doc
        .graphics
        .iter_mut()
        .find(|g| g.id == id)
        .unwrap()
        .cycle
        .as_deref_mut()
        .unwrap()
        .steps[0];
    step.reversible = true;
    step.bow_pt = bow;
    doc.validate().unwrap();
    (doc, id)
}

fn lanes(doc: &Document, id: u64) -> Vec<Arrow> {
    let painted = drawn(doc, graphic(doc, id));
    painted
        .arrows
        .into_iter()
        .zip(painted.arcs)
        .filter_map(|(arrow, step)| (step == Some(0)).then_some(arrow))
        .collect()
}

fn connectors(doc: &Document, id: u64) -> Vec<Arrow> {
    let painted = drawn(doc, graphic(doc, id));
    painted
        .arrows
        .into_iter()
        .zip(painted.arcs)
        .filter_map(|(arrow, step)| step.is_none().then_some(arrow))
        .collect()
}

// Place a part on the chosen lane's side of the pair, using the painted
// circles rather than the unpainted centerline that caused the regression.
fn beside(pair: &[Arrow], lane: usize, fraction: f32, away: f32) -> Point {
    let (center, radius, first) = pair[lane].arc_circle().unwrap();
    let (_, other_radius, _) = pair[1 - lane].arc_circle().unwrap();
    let angle = (first + pair[lane].arc.unwrap() * fraction).to_radians();
    let r = radius + (radius - other_radius).signum() * away;
    center.offset(r * angle.cos(), r * angle.sin())
}

fn fraction(lane: &Arrow, point: Point) -> f32 {
    let (center, _, first) = lane.arc_circle().unwrap();
    let sweep = lane.arc.unwrap().to_radians();
    let angle = (point.y - center.y).atan2(point.x - center.x);
    (sweep.signum() * (angle - first.to_radians())).rem_euclid(TAU) / sweep.abs()
}

fn touch(connector: &Arrow, incoming: bool) -> Point {
    if incoming {
        connector.end
    } else {
        connector.start
    }
}

fn unit(point: Point) -> Point {
    let length = point.x.hypot(point.y);
    assert!(length > 0.);
    Point::new(point.x / length, point.y / length)
}

fn check_attachment(connector: &Arrow, lane: &Arrow, incoming: bool) {
    let point = touch(connector, incoming);
    let (center, radius, _) = lane.arc_circle().unwrap();
    assert!(
        (point.distance(center) - radius).abs() < 0.05,
        "connector {point:?} misses painted lane of radius {radius}"
    );
    let [a, b] = connector.cubic.unwrap();
    let derivative = if incoming {
        Point::new(point.x - b.x, point.y - b.y)
    } else {
        Point::new(a.x - point.x, a.y - point.y)
    };
    let derivative = unit(derivative);
    let radial = unit(Point::new(point.x - center.x, point.y - center.y));
    let sign = lane.arc.unwrap().signum();
    let tangent = Point::new(-sign * radial.y, sign * radial.x);
    assert!(
        derivative.x * tangent.x + derivative.y * tangent.y > 0.9999,
        "connector derivative {derivative:?} runs against lane tangent {tangent:?}"
    );
    assert_eq!(
        connector.appearance().head,
        if incoming { Head::None } else { Head::Full }
    );
}

#[test]
fn reversible_side_connectors_touch_the_nearest_painted_lane_and_follow_its_travel() {
    for (reflected, bow) in [(false, 0.), (true, 0.), (false, 40.), (true, -20.)] {
        for lane in 0..2 {
            for incoming in [true, false] {
                let (mut doc, id) = fixture(reflected, bow);
                let pair = lanes(&doc, id);
                assert_eq!(pair.len(), 2);
                let at = beside(&pair, lane, 0.5, 2. * doc.drawing_style.bond_length_world);
                part(
                    &mut doc,
                    id,
                    if incoming {
                        Role::Reagent(0)
                    } else {
                        Role::Product(0)
                    },
                    at,
                    "ArX",
                );
                let sides = connectors(&doc, id);
                assert_eq!(sides.len(), 1);
                check_attachment(&sides[0], &pair[lane], incoming);
                doc.validate().unwrap();
            }
        }
    }
}

#[test]
fn crowded_incoming_and_outgoing_attachments_keep_projected_order_and_clearance() {
    for lane in 0..2 {
        let (mut doc, id) = fixture(false, 0.);
        let pair = lanes(&doc, id);
        let bond = doc.drawing_style.bond_length_world;
        let reagent = beside(&pair, lane, 0.4, 3. * bond);
        let product = beside(&pair, lane, 0.6, 3. * bond);
        assert!(fraction(&pair[lane], reagent) < fraction(&pair[lane], product));
        part(&mut doc, id, Role::Reagent(0), reagent, "ArX");
        part(&mut doc, id, Role::Product(0), product, "ArAr");
        let sides = connectors(&doc, id);
        assert_eq!(sides.len(), 2);
        check_attachment(&sides[0], &pair[lane], true);
        check_attachment(&sides[1], &pair[lane], false);
        let incoming = fraction(&pair[lane], sides[0].end);
        let outgoing = fraction(&pair[lane], sides[1].start);
        let (_, radius, _) = pair[lane].arc_circle().unwrap();
        let clearance = (outgoing - incoming) * radius * pair[lane].arc.unwrap().to_radians().abs();
        assert!(
            clearance >= 0.25 * bond - 0.01,
            "lane {lane}: projected order/clearance lost at {incoming}, {outgoing} ({clearance})"
        );
    }
}

#[test]
fn hiding_a_nonreversible_step_keeps_its_existing_side_connector_geometry() {
    let (mut doc, id) = fixture(false, 0.);
    let pair = lanes(&doc, id);
    let at = beside(&pair, 0, 0.4, 2. * doc.drawing_style.bond_length_world);
    part(&mut doc, id, Role::Reagent(0), at, "ArX");
    doc.graphics
        .iter_mut()
        .find(|g| g.id == id)
        .unwrap()
        .cycle
        .as_deref_mut()
        .unwrap()
        .steps[0]
        .reversible = false;
    let before = connectors(&doc, id);
    doc.graphics
        .iter_mut()
        .find(|g| g.id == id)
        .unwrap()
        .cycle
        .as_deref_mut()
        .unwrap()
        .steps[0]
        .hidden = true;
    assert_eq!(connectors(&doc, id), before);
}

#[test]
fn negative_radius_parallel_span_matches_the_painted_reverse_arrow_and_tangent() {
    let style = ArrowStyle::preset(Preset::Forward);
    let half = crate::style::DEFAULT.world(style.gap_pt.max(2.)) * 0.75;
    for sweep in [-120., 120.] {
        let arc = super::Span {
            center: Point::default(),
            radius: 2.,
            first: 30.,
            sweep,
        };
        assert!(half > arc.radius);
        let path = arc.shifted(-half).unwrap().reversed();
        let painted = super::arrow(1, arc.at(1., -half), arc.at(0., -half), -sweep, &style);
        assert!(path.radius > 0.);
        assert!(arc.shifted(-arc.radius).is_none());
        let (center, radius, first) = painted.arc_circle().unwrap();
        for t in [0., 0.4, 1.] {
            let angle = (first + painted.arc.unwrap() * t).to_radians();
            let expected = center.offset(radius * angle.cos(), radius * angle.sin());
            assert!(path.at(t, 0.).distance(expected) < 0.001);
        }
        for incoming in [true, false] {
            let connector = super::side_arrow(
                1,
                path.at(0.4, 0.),
                path.tangent(0.4),
                (Point::new(10., 10.), Point::new(30., 30.)),
                incoming,
                &style,
            );
            check_attachment(&connector, &painted, incoming);
        }
    }
}

#[test]
fn a_collapsed_reversible_step_keeps_centerline_connectors_without_phantom_lanes() {
    let (mut doc, id) = fixture(false, 0.);
    // The intermediate ink covers this tiny circle, leaving no painted step.
    super::set_radius(&mut doc, id, 2.);
    part(&mut doc, id, Role::Reagent(0), Point::new(30., 30.), "ArX");
    assert!(lanes(&doc, id).is_empty());
    let reversible = connectors(&doc, id);
    assert_eq!(reversible.len(), 1);
    doc.graphics
        .iter_mut()
        .find(|g| g.id == id)
        .unwrap()
        .cycle
        .as_deref_mut()
        .unwrap()
        .steps[0]
        .reversible = false;
    assert_eq!(connectors(&doc, id), reversible);
    doc.validate().unwrap();
}
