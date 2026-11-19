use super::{ArrowStyle, Badge, BadgeIcon, Body, Head, Preset};
use crate::{
    document::{Arrow, DRAWING_ELEMENTS, Document, Point},
    graphics::{PathCommand, flattened},
    style::DEFAULT,
};

fn arrow(body: Body) -> Arrow {
    let mut style = ArrowStyle::preset(Preset::Forward);
    style.set_body(body);
    Arrow::new(
        1,
        Point::new(0., 0.),
        Point::new(100., 0.),
        Preset::Forward,
        style,
    )
}

fn points(commands: &[PathCommand]) -> Vec<Point> {
    flattened(commands).into_iter().flatten().collect()
}

#[test]
fn block_body_is_one_filled_outline_from_tail_to_tip() {
    let a = arrow(Body::Block);
    let paths = a.paths();
    assert_eq!(paths.len(), 1);
    assert!(paths[0].filled);
    assert!(matches!(paths[0].commands.last(), Some(PathCommand::Close)));
    let s = a.appearance();
    let (w, h) = (
        DEFAULT.world(s.gap_pt) * 0.5,
        DEFAULT.world(s.head_width_pt),
    );
    let p = points(&paths[0].commands);
    assert!(p.iter().any(|q| q.distance(Point::new(100., 0.)) < 1e-3));
    let top = p.iter().map(|q| q.y).fold(f32::INFINITY, f32::min);
    let bottom = p.iter().map(|q| q.y).fold(f32::NEG_INFINITY, f32::max);
    assert!((top + h).abs() < 1e-3 && (bottom - h).abs() < 1e-3);
    assert!(
        p.iter()
            .any(|q| q.x.abs() < 1e-3 && (q.y.abs() - w).abs() < 1e-3)
    );
    // The inside of the block is part of the arrow, not only its edge.
    assert!(a.hit(Point::new(40., 0.), 0.5));
    assert!(!a.hit(Point::new(40., h * 1.5), 0.5));
}

#[test]
fn hollow_body_is_stroked_and_still_picked_inside() {
    let a = arrow(Body::Hollow);
    let paths = a.paths();
    assert_eq!(paths.len(), 1);
    assert!(!paths[0].filled && paths[0].style.fill.is_none());
    assert!(a.hit(Point::new(40., 0.), 0.5));
}

#[test]
fn gradient_and_chevron_bodies_are_filled() {
    let gradient = arrow(Body::Gradient).paths();
    assert!(gradient.iter().all(|p| p.filled && p.style.effects.shaded));
    // The scene shades it in bands, as it does a shaded shape.
    let mut doc = Document::default();
    doc.arrows.push(arrow(Body::Gradient));
    assert!(crate::scene::primitives(&doc).len() > 5);
    let chevrons = arrow(Body::Chevron).paths();
    assert!(chevrons.len() > 2, "{}", chevrons.len());
    assert!(chevrons.iter().all(|p| p.filled));
    let last = points(&chevrons.last().unwrap().commands);
    assert!(last.iter().any(|q| (q.x - 100.).abs() < 1e-2));
}

#[test]
fn a_look_never_changes_the_kind_and_double_shafts_keep_lines() {
    let mut a = arrow(Body::Block);
    assert_eq!(a.kind, "forward");
    let mut style = ArrowStyle::preset(Preset::Equilibrium);
    style.set_body(Body::Block);
    a.set_preset(Preset::Equilibrium, &style);
    assert_eq!(a.drawn_body(), Body::Line);
    assert!(a.paths().len() >= 2);
    // Switching back to a line restores line-sized heads.
    let mut line = ArrowStyle::preset(Preset::Forward);
    line.set_body(Body::Block);
    line.set_body(Body::Line);
    assert_eq!(line, ArrowStyle::preset(Preset::Forward));
}

#[test]
fn line_styles_save_without_a_body_field() {
    let text = serde_json::to_string(&ArrowStyle::default()).unwrap();
    assert!(!text.contains("body"));
    let mut doc = Document::default();
    doc.arrows.push(arrow(Body::Line));
    assert!(doc.file_version() < DRAWING_ELEMENTS);
    doc.arrows.push(arrow(Body::Chevron));
    assert_eq!(doc.file_version(), DRAWING_ELEMENTS);
    let back: Document = serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
    assert_eq!(back.arrows, doc.arrows);
}

#[test]
fn badge_sits_in_a_gap_of_the_shaft_and_names_its_condition() {
    let mut a = arrow(Body::Line);
    let plain = a.paths().len();
    a.badge = Some(Badge::new(BadgeIcon::Light));
    let paths = a.paths();
    // Two shaft pieces, the head, the disc and the bulb strokes.
    assert!(paths.len() > plain + 2);
    let frame = a.badge_frame().unwrap();
    assert!(frame.center.distance(Point::new(50., 0.)) < 1e-3);
    for path in &paths[..2] {
        assert!(
            points(&path.commands)
                .iter()
                .all(|q| q.distance(frame.center) >= frame.radius - 1e-2),
            "the shaft must stop at the disc"
        );
    }
    assert_eq!(a.badge.as_ref().unwrap().condition(), "hν");
    assert!(a.badge_text().is_none());
    a.badge = Some(Badge::new(BadgeIcon::Metal));
    assert_eq!(a.badge_text().unwrap().1, "Pd");
    assert_eq!(a.badge.as_ref().unwrap().condition(), "Pd");
    a.badge.as_mut().unwrap().text.clear();
    assert!(a.validate().is_err());
}

#[test]
fn badge_on_a_block_body_sits_beside_it() {
    let mut a = arrow(Body::Block);
    a.badge = Some(Badge::new(BadgeIcon::Electrode));
    let frame = a.badge_frame().unwrap();
    assert!(frame.gap.is_none());
    assert!(frame.center.y < -DEFAULT.world(a.appearance().head_width_pt));
    assert_eq!(a.badge_text().unwrap().1, "e");
}

#[test]
fn badge_keeps_a_straight_forward_arrow_a_reaction_arrow() {
    let mut a = arrow(Body::Line);
    a.badge = Some(Badge::new(BadgeIcon::Heat));
    assert_eq!(a.kind, "forward");
    assert!(a.control.is_none() && a.cubic.is_none());
    assert_eq!(a.appearance().head, Head::Full);
}

fn arc(sweep: f32) -> Arrow {
    let mut a = arrow(Body::Line);
    a.set_arc(sweep);
    a
}

#[test]
fn arc_runs_clockwise_through_its_middle_on_one_circle() {
    let a = arc(180.);
    assert!(a.point(0.5).distance(Point::new(50., -50.)) < 0.05);
    let (center, radius, _) = a.arc_circle().unwrap();
    assert!(center.distance(Point::new(50., 0.)) < 1e-3 && (radius - 50.).abs() < 1e-3);
    for i in 0..=20 {
        let p = a.point(i as f32 / 20.);
        assert!((p.distance(center) - radius).abs() < 0.05, "{p:?}");
    }
    let wide = arc(270.);
    assert_eq!(wide.nodes.len(), 2);
    assert!(wide.validate().is_ok());
    assert!(arc(-90.).point(0.5).y > 0.);
}

#[test]
fn arc_handles_keep_it_an_arc() {
    let mut a = arc(90.);
    assert_eq!(a.handles().len(), 3);
    assert!(a.node_handles().is_empty() && a.has_bend_handle());
    a.edit_handle(2, Point::new(50., 30.));
    assert!(
        a.arc.unwrap() < 0.,
        "a bend below makes it run the other way"
    );
    assert!(a.point(0.5).distance(Point::new(50., 30.)) < 0.1);
    a.edit_handle(1, Point::new(120., 0.));
    assert_eq!(a.end, Point::new(120., 0.));
    assert!(a.is_arc());
}

#[test]
fn reverse_flip_and_transforms_keep_the_same_circle() {
    let mut a = arc(120.);
    let middle = a.point(0.5);
    a.reverse();
    assert_eq!(a.arc, Some(-120.));
    assert!(a.point(0.5).distance(middle) < 0.05);
    a.flip_bend();
    assert_eq!(a.arc, Some(120.));
    let mut turned = arc(120.);
    turned.map_points(|p| Point::new(-p.y, p.x));
    assert!((turned.arc.unwrap() - 120.).abs() < 0.01);
    let mut mirrored = arc(120.);
    mirrored.map_points(|p| Point::new(p.x, -p.y));
    assert!((mirrored.arc.unwrap() + 120.).abs() < 0.01);
}

#[test]
fn editing_points_or_straightening_releases_the_arc() {
    let mut a = arc(180.);
    a.insert_node(a.point(0.25)).unwrap();
    assert!(!a.is_arc() && a.nodes.len() == 2);
    let mut b = arc(180.);
    b.straighten();
    assert!(!b.is_arc() && b.nodes.is_empty());
    let mut c = arc(180.);
    c.set_preset(
        Preset::Equilibrium,
        &ArrowStyle::preset(Preset::Equilibrium),
    );
    assert_eq!(c.arc, Some(180.));
    assert!(c.validate().is_ok());
    c.set_preset(Preset::Bent, &ArrowStyle::preset(Preset::Bent));
    assert!(!c.is_arc());
}

#[test]
fn arcs_save_as_version_forty_and_reopen_unchanged() {
    let mut doc = Document::default();
    doc.arrows.push(arc(270.));
    assert_eq!(doc.file_version(), DRAWING_ELEMENTS);
    doc.validate().unwrap();
    let back: Document = serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
    assert_eq!(back.arrows, doc.arrows);
}

#[test]
fn double_headed_and_headless_blocks_keep_both_ends_broad() {
    let mut a = arrow(Body::Block);
    let mut style = a.appearance();
    style.tail = Head::Full;
    a.style = Some(style.clone());
    let p = points(&a.paths()[0].commands);
    assert!(p.iter().any(|q| q.distance(Point::new(0., 0.)) < 1e-3));
    assert!(p.iter().any(|q| q.distance(Point::new(100., 0.)) < 1e-3));
    style.head = Head::None;
    style.tail = Head::None;
    a.style = Some(style.clone());
    let p = points(&a.paths()[0].commands);
    let w = DEFAULT.world(style.gap_pt) * 0.5;
    assert!(p.iter().all(|q| (q.y.abs() - w).abs() < 1e-3));
}

#[test]
fn degenerate_edits_leave_an_arc_whole() {
    let mut a = arc(180.);
    let before = a.clone();
    a.edit_handle(1, a.start);
    assert_eq!(a, before, "an end dropped on the other end stays put");
    assert!(a.delete_node(super::FIRST_NODE_HANDLE).is_err());
    let mut huge = arrow(Body::Chevron);
    huge.end = Point::new(1e20, 0.);
    let _ = huge.paths();
}

#[test]
fn exchange_copy_keeps_nested_groups_valid() {
    let mut doc = Document::default();
    let mut a = arrow(Body::Line);
    a.badge = Some(Badge::new(BadgeIcon::Heat));
    doc.arrows.push(a);
    for (id, y) in [(2, -30.), (5, 30.)] {
        doc.annotations.push(crate::document::Annotation {
            id,
            position: Point::new(0., y),
            text: "heat".into(),
            format: Default::default(),
        });
    }
    doc.groups.push(crate::grouping::Group {
        id: 3,
        members: vec![1, 2],
        integral: false,
    });
    doc.groups.push(crate::grouping::Group {
        id: 4,
        members: vec![1, 2, 5],
        integral: false,
    });
    doc.validate().unwrap();
    super::lowered(&doc).validate().unwrap();
}

#[test]
fn a_double_headed_chevron_points_both_ways() {
    let mut a = arrow(Body::Chevron);
    let mut style = a.appearance();
    style.tail = Head::Full;
    a.style = Some(style);
    let chevrons = a.paths();
    // The tip of each chevron is its third point.
    let tip = |i: usize| points(&chevrons[i].commands)[2].x;
    let back = |i: usize| points(&chevrons[i].commands)[0].x;
    assert!(tip(0) < back(0), "the first chevron points to the start");
    let last = chevrons.len() - 1;
    assert!(tip(last) > back(last), "the last chevron points to the end");
}
