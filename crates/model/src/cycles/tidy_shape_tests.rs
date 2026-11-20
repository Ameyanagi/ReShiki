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
    let size = if n == 2 {
        Point::new(3000., 200.)
    } else {
        Point::new(1000., 400.)
    };
    let mut g = Graphic::dragged(
        1,
        GraphicKind::Ellipse,
        Point::new(-size.x * 0.5, -size.y * 0.5),
        Point::new(size.x * 0.5, size.y * 0.5),
        GraphicStyle {
            corner_pt: Some(24.),
            ..Default::default()
        },
        Default::default(),
        false,
    );
    if mirrored {
        g.origin.x += g.axis_x.x;
        g.axis_x.x = -g.axis_x.x;
    }
    g.cycle = Some(Box::new(Cycle {
        shape,
        counterclockwise: ccw,
        ..Default::default()
    }));
    let perimeter = cycles::outline(&g).unwrap();
    let dir = if g.cycle_counterclockwise() { -1. } else { 1. };
    let phase = perimeter
        .project(g.cycle_circle().0.offset(size.x, 0.))
        .unwrap()
        .s;
    doc.graphics.push(g);
    let mut parts = vec![];
    for i in 0..n {
        let offset = if n == 2 {
            0.
        } else {
            [0., 0.06, -0.045, 0.03][i]
        };
        let point = perimeter
            .point(phase + dir * perimeter.length() * (i as f64 / n as f64 + offset))
            .unwrap();
        let point = if n == 2 {
            point.offset(if i == 0 { 30. } else { -30. }, 5.)
        } else {
            point.offset(13. * (i + 1) as f32, -17. * (i + 1) as f32)
        };
        parts.push(vec![text(&mut doc, point, &format!("Pd{i}"))]);
    }
    let cycle = doc.graphics[0].cycle.as_mut().unwrap();
    cycle.intermediates = parts;
    cycle.steps = vec![Step::default(); n];
    let ids = doc.all_ids();
    doc.group_selection(&ids).unwrap();
    doc.validate().unwrap();
    doc
}

fn ink(doc: &Document, part: &[u64]) -> (Point, Point) {
    crate::scene::selection_bounds(doc, part).unwrap()
}

#[test]
fn noncircle_tidy_uses_ordered_equal_distance_under_both_directions_and_reflection() {
    for shape in [Shape::Ellipse, Shape::RoundedRectangle, Shape::Rectangle] {
        for mirror in [false, true] {
            for ccw in [false, true] {
                let mut doc = fixture(shape, 4, mirror, ccw);
                let original = doc.graphics[0].clone();
                let cycle = original.cycle.as_ref().unwrap();
                let perimeter = cycles::outline(&original).unwrap();
                let dir = if original.cycle_counterclockwise() {
                    -1.
                } else {
                    1.
                };
                assert!(cycles::tidy(&mut doc, 1), "{shape:?}/{mirror}/{ccw}");
                assert_eq!(
                    doc.graphics[0], original,
                    "frame, roles and settings are stored unchanged"
                );
                let positions: Vec<_> = cycle
                    .intermediates
                    .iter()
                    .map(|p| {
                        let p = perimeter.project(center_of(ink(&doc, p))).unwrap();
                        assert!(p.distance < 0.055, "center must lie on {shape:?}");
                        p.s
                    })
                    .collect();
                for i in 0..positions.len() {
                    let step = (dir * (positions[(i + 1) % positions.len()] - positions[i]))
                        .rem_euclid(perimeter.length());
                    assert!(
                        (step - perimeter.length() / positions.len() as f64).abs() < 0.12,
                        "{shape:?}: {step}"
                    );
                }
                doc.validate().unwrap();
                let once = doc.clone();
                assert!(
                    !cycles::tidy(&mut doc, 1),
                    "second Tidy must be a no-op: {shape:?}/{mirror}/{ccw}"
                );
                assert_eq!(doc, once);
            }
        }
    }
}

#[test]
fn invalid_bond_length_cannot_retain_an_inside_part() {
    let mut doc = fixture(Shape::Ellipse, 3, false, false);
    let part = doc.graphics[0].cycle.as_ref().unwrap().intermediates[0].clone();
    let center = doc.graphics[0].cycle_circle().0;
    let from = center_of(ink(&doc, &part));
    doc.translate(&part, center.x - from.x, center.y - from.y);
    assert!(cycles::holds(&doc, 1, &part));
    for bond in [f32::NAN, f32::INFINITY, 0., -1.] {
        doc.drawing_style.bond_length_world = bond;
        assert!(!cycles::holds(&doc, 1, &part), "{bond}");
    }
}

#[test]
fn elongated_outline_tips_stay_members_after_tidy_and_a_small_move() {
    for shape in [Shape::Ellipse, Shape::RoundedRectangle, Shape::Rectangle] {
        let mut doc = fixture(shape, 2, false, false);
        assert!(cycles::tidy(&mut doc, 1));
        let (center, radius) = doc.graphics[0].cycle_circle();
        let parts = doc.graphics[0]
            .cycle
            .as_ref()
            .unwrap()
            .intermediates
            .clone();
        assert!(
            parts
                .iter()
                .any(|p| center_of(ink(&doc, p)).distance(center)
                    > radius + 4. * doc.drawing_style.bond_length_world)
        );
        for part in parts {
            assert!(
                cycles::holds(&doc, 1, &part),
                "{shape:?}: the actual tip is held"
            );
            let delta = Point::new(0.01, -0.01);
            doc.translate(&part, delta.x, delta.y);
            assert!(doc.take_out(&part, delta).is_empty());
            assert!(cycles::role_of(&doc, part[0]).is_some());
        }
    }
}

#[test]
fn roles_keep_their_actual_outline_side_with_bow_reversible_and_hidden_steps() {
    for shape in [Shape::Ellipse, Shape::RoundedRectangle, Shape::Rectangle] {
        for (bow, hidden) in [(0., false), (48., false), (-48., true)] {
            let mut doc = fixture(shape, 3, true, true);
            let g = &doc.graphics[0];
            let perimeter = cycles::outline(g).unwrap();
            let center = g.cycle_circle().0;
            let on = perimeter.project(center.offset(2000., 0.)).unwrap().point;
            let outside = text(&mut doc, on.offset(100., 0.), "ArX");
            let inside = text(&mut doc, center, "Base");
            let cycle = doc.graphics[0].cycle.as_mut().unwrap();
            cycle.steps[0].reagents.push(vec![outside]);
            cycle.steps[0].products.push(vec![inside]);
            cycle.steps[0].name = "Oxidative addition".into();
            cycle.steps[0].bow_pt = bow;
            cycle.steps[0].reversible = true;
            cycle.steps[0].hidden = hidden;
            doc.groups.clear();
            let ids = doc.all_ids();
            doc.group_selection(&ids).unwrap();
            let roles = doc.graphics[0].cycle.clone();
            assert!(cycles::tidy(&mut doc, 1));
            assert_eq!(
                perimeter.contains(center_of(ink(&doc, &[outside]))),
                Some(false)
            );
            assert_eq!(
                perimeter.contains(center_of(ink(&doc, &[inside]))),
                Some(true)
            );
            assert!(cycles::holds(&doc, 1, &[outside]));
            assert!(cycles::holds(&doc, 1, &[inside]));
            assert_eq!(doc.graphics[0].cycle, roles);
            let once = doc.clone();
            assert!(
                !cycles::tidy(&mut doc, 1),
                "{shape:?}/{bow}/{hidden}: must be stable"
            );
            assert_eq!(doc, once);
            let source = doc.file_json().unwrap();
            let back = Document::from_json(&source).unwrap();
            assert_eq!(back.file_json().unwrap(), source);
            doc.validate().unwrap();
        }
    }
}

#[test]
fn translated_molecule_names_and_property_captions_keep_their_links() {
    for property in [false, true] {
        let mut doc = fixture(Shape::Ellipse, 3, false, false);
        let a = doc.add_atom("C", Point::new(1200., 0.));
        let b = doc.add_atom("C", Point::new(1242., 0.));
        doc.add_bond(a, b, 1, "plain");
        let atoms = vec![a, b];
        let caption = if property {
            crate::linked_properties::toggle(
                &mut doc,
                &atoms,
                crate::linked_properties::Property::Formula,
                TextFormat::default(),
                false,
            )
            .unwrap();
            doc.property_captions[0].annotation
        } else {
            let identity = crate::molecule_names::identity(&doc, &atoms).unwrap();
            crate::molecule_names::show(
                &mut doc,
                &atoms,
                "ethane".into(),
                identity,
                TextFormat::default(),
                false,
            )
            .unwrap()
        };
        doc.graphics[0].cycle.as_mut().unwrap().steps[0]
            .reagents
            .push(atoms.clone());
        doc.groups.clear();
        let ids = doc.all_ids();
        doc.group_selection(&ids).unwrap();
        let before = doc.clone();
        let label = doc
            .annotations
            .iter()
            .find(|t| t.id == caption)
            .unwrap()
            .position;
        assert!(cycles::tidy(&mut doc, 1));
        let moved = doc.atom(a).unwrap().position;
        let old = before.atom(a).unwrap().position;
        let delta = Point::new(moved.x - old.x, moved.y - old.y);
        let at = doc
            .annotations
            .iter()
            .find(|t| t.id == caption)
            .unwrap()
            .position;
        assert!(at.distance(label.offset(delta.x, delta.y)) < 0.002);
        let _ = crate::transaction::reconcile(&mut doc, before).unwrap();
        assert!(crate::molecule_names::include_annotations(&doc, &atoms).contains(&caption));
        assert!(!cycles::tidy(&mut doc, 1));
        doc.validate().unwrap();
    }
}
