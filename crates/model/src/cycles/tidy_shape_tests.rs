use super::*;
use crate::{
    cycles::{self, Cycle, Shape, Step},
    document::Annotation,
    graphics::{Graphic, GraphicKind, GraphicStyle},
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

fn suzuki(shape: Shape) -> Document {
    let mut doc = Document::from_json(include_bytes!(
        "../../../../tests/fixtures/cycles/tidy-before.rsk"
    ))
    .unwrap();
    let g = doc.graphics.iter_mut().find(|g| g.id == 191).unwrap();
    let center = g.cycle_circle().0;
    g.cycle.as_mut().unwrap().shape = shape;
    g.axis_x.x *= 1.3;
    g.axis_x.y *= 1.3;
    g.axis_y.x *= 0.8;
    g.axis_y.y *= 0.8;
    g.origin = center.offset(
        -(g.axis_x.x + g.axis_y.x) * 0.5,
        -(g.axis_x.y + g.axis_y.y) * 0.5,
    );
    doc
}

fn save_suzuki_qa(shape: Shape, before: &Document, after: &Document) {
    if let Some(dir) = std::env::var_os("RESHIKI_CYCLE_SHAPE_JOIN_QA_DIR") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (state, doc) in [("before", before), ("preview", after)] {
            let name = format!("noncircle-suzuki-{shape:?}-{state}");
            std::fs::write(dir.join(format!("{name}.svg")), crate::scene::svg(doc)).unwrap();
            std::fs::write(dir.join(format!("{name}.rsk")), doc.file_json().unwrap()).unwrap();
        }
        let g = after.graphics.iter().find(|g| g.id == 191).unwrap();
        std::fs::write(
            dir.join(format!("{shape:?}-drawn.txt")),
            format!("{:#?}", cycles::drawn(after, g)),
        )
        .unwrap();
    }
}

#[test]
fn actual_suzuki_tidy_keeps_reversible_contacts_in_order_on_the_painted_lane() {
    for shape in [
        Shape::Circle,
        Shape::Ellipse,
        Shape::RoundedRectangle,
        Shape::Rectangle,
    ] {
        let mut doc = suzuki(shape);
        let before = doc.clone();
        let roles = doc
            .graphics
            .iter()
            .find(|g| g.id == 191)
            .unwrap()
            .cycle
            .clone();
        assert!(cycles::tidy(&mut doc, 191));
        let g = doc.graphics.iter().find(|g| g.id == 191).unwrap();
        let cycle = g.cycle.as_ref().unwrap();
        assert_eq!(g.cycle, roles);
        let drawn = cycles::drawn(&doc, g);
        let shafts: Vec<_> = drawn
            .arrows
            .iter()
            .zip(&drawn.arcs)
            .filter(|(_, step)| **step == Some(1))
            .map(|(a, _)| super::super::shapes::Shaft::from_arrow(a).unwrap())
            .collect();
        let sides: Vec<_> = drawn
            .arrows
            .iter()
            .zip(&drawn.arcs)
            .filter(|(_, step)| step.is_none())
            .map(|(a, _)| a)
            .collect();
        assert_eq!(shafts.len(), 2);
        // The fixture stores step0's reagent, then step1's reagent/product.
        let reagent = sides[1];
        let product = sides[2];
        let part_a = center_of(ink(&doc, &cycle.steps[1].reagents[0]));
        let part_b = center_of(ink(&doc, &cycle.steps[1].products[0]));
        let lane = |point| {
            shafts
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    a.project(point)
                        .unwrap()
                        .distance
                        .total_cmp(&b.project(point).unwrap().distance)
                })
                .unwrap()
                .0
        };
        let lane_a = lane(part_a);
        let lane_b = lane(part_b);
        assert_eq!(lane_a, lane_b, "{shape:?}: same outside painted lane");
        let shaft = &shafts[lane_a];
        let a = shaft.project(reagent.end).unwrap();
        let b = shaft.project(product.start).unwrap();
        assert!(a.distance < 0.01 && b.distance < 0.01, "{shape:?}");
        let preferred_order = shaft.project(part_b).unwrap().s - shaft.project(part_a).unwrap().s;
        let actual_order = b.s - a.s;
        assert!(preferred_order * actual_order > 0., "{shape:?}");
        assert!(
            actual_order.abs() >= f64::from(doc.drawing_style.bond_length_world) - 0.02,
            "{shape:?}: joined only {} world units apart",
            actual_order.abs()
        );
        let once = doc.clone();
        assert!(!cycles::tidy(&mut doc, 191), "{shape:?}");
        assert_eq!(doc, once);
        save_suzuki_qa(shape, &before, &doc);
    }
}

#[test]
fn actual_rectangular_suzuki_keeps_the_inside_reagent_and_its_connector_clear() {
    let mut doc = suzuki(Shape::Rectangle);
    assert!(cycles::tidy(&mut doc, 191));
    let g = doc.graphics.iter().find(|g| g.id == 191).unwrap();
    let cycle = g.cycle.as_ref().unwrap();
    let bounds = ink(&doc, &cycle.steps[0].reagents[0]);
    let (lo, hi) = super::super::grown(bounds, DEFAULT.world(cycle.gap_pt));
    let perimeter = cycles::outline(g).unwrap();
    for point in [lo, Point::new(hi.x, lo.y), hi, Point::new(lo.x, hi.y)] {
        assert_eq!(perimeter.contains(point), Some(true), "{point:?}");
    }
    let drawn = cycles::drawn(&doc, g);
    let connector = drawn
        .arrows
        .iter()
        .zip(&drawn.arcs)
        .find(|(_, step)| step.is_none())
        .map(|(a, _)| a)
        .unwrap();
    for i in 0..=128 {
        let point = connector.point(i as f32 / 128.);
        assert!(
            !super::super::contains(bounds, point),
            "the reagent connector entered its molecule: {point:?}"
        );
    }
    let once = doc.clone();
    assert!(!cycles::tidy(&mut doc, 191));
    assert_eq!(doc, once);
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
