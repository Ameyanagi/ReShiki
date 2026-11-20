use super::*;
use crate::{
    arrows::{ArrowStyle, Head, Preset},
    cycles::{Cycle, Shape as CycleShape},
    document::Arrow,
    graphics::{GraphicKind, PathCommand::*},
};

#[test]
fn sharp_corner_box_support_clears_both_incident_lines() {
    let frame = Frame {
        point: Point::default(),
        incoming: Point::new(0., -1.),
        outgoing: Point::new(1., 0.),
        inward: [0., 0.],
    };
    let offset = frame.offsets(160., 40., 10., true)[0];
    assert_eq!(offset, Point::new(90., -30.));
    let center = Point::new(-offset.x, -offset.y);
    let b = (center.offset(-80., -20.), center.offset(80., 20.));
    assert_eq!(b.1.x, -10.);
    assert_eq!(b.0.y, 10.);
    let reversible = Frame {
        inward: [8., 12.],
        ..frame
    };
    assert_eq!(
        reversible.offsets(160., 40., 10., true)[0],
        Point::new(102., -38.)
    );
    assert_eq!(reversible.offsets(160., 40., 10., false)[0], offset);
    let folded = Frame {
        outgoing: Point::new(0., 1.),
        ..frame
    };
    assert!(
        folded
            .offsets(160., 40., 10., true)
            .iter()
            .all(|p| p.is_finite() && p.distance(Point::default()) < 100.)
    );
}

#[test]
fn collision_checks_actual_curves_stroke_width_and_filled_containment() {
    let p = Point::new;
    let curve = Ink::new(
        &[
            Move(p(0., 0.)),
            Cubic(p(0., 100.), p(100., 100.), p(100., 0.)),
        ],
        0.,
        false,
    );
    assert!(
        !curve.hits((p(45., 15.), p(55., 25.)), 0.),
        "empty area inside a control box is usable"
    );
    assert!(curve.hits((p(49., 74.), p(51., 76.)), 0.));
    let flat = [Move(p(0., 0.)), Cubic(p(20., 0.), p(80., 0.), p(100., 0.))];
    assert!(
        Ink::new(&flat, 0., false).hits((p(40., 0.), p(60., 10.)), 0.),
        "coincident cubic edge"
    );
    let line = [Move(p(0., 0.)), Line(p(100., 0.))];
    assert!(!Ink::new(&line, 0., false).hits((p(40., 1.), p(60., 2.)), 0.));
    assert!(Ink::new(&line, 4., false).hits((p(40., 1.), p(60., 2.)), 0.));
    let head = Ink::new(
        &[
            Move(p(0., 0.)),
            Line(p(100., 0.)),
            Line(p(50., 100.)),
            Close,
        ],
        0.,
        true,
    );
    assert!(
        head.hits((p(45., 40.), p(55., 50.)), 0.),
        "box fully inside a head"
    );
    let c = kurbo::CubicBez::new((0., 0.), (0., 10000.), (10000., 0.), (10000., 0.));
    let at = c.eval(0.013579);
    let tight = (
        p(at.x as f32 - 0.001, at.y as f32 - 0.001),
        p(at.x as f32 + 0.001, at.y as f32 + 0.001),
    );
    assert!(
        Ink::new(
            &[
                Move(p(0., 0.)),
                Cubic(p(0., 10000.), p(10000., 0.), p(10000., 0.))
            ],
            0.,
            false
        )
        .hits(tight, 0.)
    );
}

fn layout_fixture() -> (Document, Graphic, Outline, Arrow) {
    let doc = Document::default();
    let mut g = Graphic::dragged(
        1,
        GraphicKind::Rectangle,
        Point::default(),
        Point::new(1000., 400.),
        Default::default(),
        Default::default(),
        false,
    );
    g.cycle = Some(std::boxed::Box::new(Cycle {
        shape: CycleShape::Rectangle,
        ..Default::default()
    }));
    let perimeter = crate::cycles::outline(&g).unwrap();
    let arrow = Arrow::new(
        1,
        Point::default(),
        Point::new(1000., 0.),
        Preset::Forward,
        ArrowStyle {
            head: Head::None,
            ..ArrowStyle::DEFAULT
        },
    );
    (doc, g, perimeter, arrow)
}

fn target(a: &Arrow) -> Name {
    Name {
        text: "OA".into(),
        shaft: Shaft::from_arrow(a),
        reverse: None,
        fallback: 0.,
    }
}

fn label_box(out: &Drawn, i: usize) -> Box {
    let (p, t, f) = &out.labels[i];
    let l = crate::typography::layout(t, f);
    (*p, p.offset(l.width, l.height))
}

#[test]
fn inside_outside_and_alternate_search_reserve_prior_names_and_center_label() {
    let (doc, mut g, perimeter, arrow) = layout_fixture();
    let fresh = || Drawn {
        arrows: vec![arrow.clone()],
        arcs: vec![Some(0)],
        ..Default::default()
    };
    let mut base = fresh();
    place(&doc, &g, &perimeter, vec![target(&arrow)], &[], &mut base);
    let inside = label_box(&base, 0);
    assert!(inside.0.y > 0.);
    let mut blocked = fresh();
    place(
        &doc,
        &g,
        &perimeter,
        vec![target(&arrow)],
        &[Some(inside)],
        &mut blocked,
    );
    let outside = label_box(&blocked, 0);
    assert!(outside.1.y < 0.);
    let mut alternate = fresh();
    place(
        &doc,
        &g,
        &perimeter,
        vec![target(&arrow)],
        &[Some(inside), Some(outside)],
        &mut alternate,
    );
    assert!(crate::cycles::center_of(label_box(&alternate, 0)).x < 400.);
    let mut repeated = fresh();
    place(
        &doc,
        &g,
        &perimeter,
        vec![target(&arrow), target(&arrow)],
        &[],
        &mut repeated,
    );
    assert_eq!(repeated.labels.len(), 2);
    assert_eq!(
        overlap(label_box(&repeated, 0), label_box(&repeated, 1)),
        0.
    );
    g.cycle.as_mut().unwrap().label = "OA".into();
    let center = crate::cycles::center_of(inside);
    g.axis_y.y = center.y * 2.;
    let mut centered = fresh();
    place(
        &doc,
        &g,
        &perimeter,
        vec![target(&arrow)],
        &[],
        &mut centered,
    );
    assert_eq!(centered.labels.len(), 2);
    assert!(label_box(&centered, 0).1.y < 0.);
    assert_eq!(
        overlap(label_box(&centered, 0), label_box(&centered, 1)),
        0.
    );
}

#[test]
fn an_unusable_forward_lane_does_not_promote_the_reverse_lane_as_forward() {
    let (_, _, perimeter, arrow) = layout_fixture();
    let name = Name::new("OA".into(), vec![None, Shaft::from_arrow(&arrow)], 50.);
    let frame = Frame::at(&name, &perimeter, 0.5, -1.).unwrap();
    assert_eq!(frame.point, perimeter.point(50.).unwrap());
    assert_eq!(frame.outgoing, Point::new(0., -1.));
    assert_eq!(frame.inward, [0., 0.]);
}

#[test]
fn full_placement_of_a_wide_multiline_name_clears_an_exact_sharp_corner() {
    use crate::{document::ArrowNode, graphics::PathNodeType};
    let (doc, g, perimeter, mut arrow) = layout_fixture();
    arrow.end = Point::new(300., 300.);
    arrow.cubic = Some([Point::new(100., 0.), Point::new(300., 200.)]);
    arrow.nodes.push(ArrowNode {
        point: Point::new(300., 0.),
        controls: [Point::new(200., 0.), Point::new(300., 100.)],
        kind: PathNodeType::Corner,
    });
    let mut name = target(&arrow);
    name.text = "Wide box\nmultiline".into();
    let mut out = Drawn {
        arrows: vec![arrow],
        arcs: vec![Some(0)],
        ..Default::default()
    };
    place(&doc, &g, &perimeter, vec![name], &[], &mut out);
    let b = label_box(&out, 0);
    let gap = DEFAULT.world(crate::cycles::GAP_PT);
    assert!((b.1.x - (300. - gap)).abs() < 0.001);
    assert!((b.0.y - gap).abs() < 0.001);
    assert!(out.labels[0].1.contains('\n'));
}

#[test]
fn an_actual_side_arrow_blocks_the_preferred_name_without_a_part_box() {
    let (doc, g, perimeter, arrow) = layout_fixture();
    let mut base = Drawn {
        arrows: vec![arrow.clone()],
        arcs: vec![Some(0)],
        ..Default::default()
    };
    place(&doc, &g, &perimeter, vec![target(&arrow)], &[], &mut base);
    let preferred = label_box(&base, 0);
    let side = crate::cycles::side_arrow(
        1,
        Point::new(700., 0.),
        Point::new(1., 0.),
        (Point::new(295., 95.), Point::new(305., 105.)),
        true,
        &ArrowStyle::DEFAULT,
    );
    assert!(
        (0..=128).any(|i| {
            let p = side.point(i as f32 / 128.);
            preferred.0.x < p.x && p.x < preferred.1.x && preferred.0.y < p.y && p.y < preferred.1.y
        }),
        "the fixture's connector must cross the old preferred box"
    );
    let mut out = Drawn {
        arrows: vec![arrow.clone(), side],
        arcs: vec![Some(0), None],
        ..Default::default()
    };
    place(&doc, &g, &perimeter, vec![target(&arrow)], &[], &mut out);
    assert!(
        label_box(&out, 0).1.y < 0.,
        "choose the clear outside candidate"
    );
}
