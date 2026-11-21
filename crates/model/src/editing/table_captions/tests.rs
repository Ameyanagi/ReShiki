use crate::{
    document::{Annotation, Document, Point},
    editing::{self, Transform},
    graphics::{
        Graphic, GraphicKind,
        table::{Cell, Frame, Membership, Table},
    },
    scene::Primitive,
};

// Native QA geometry: three columns and two rows, with the existing Entry and
// Solvent captions inserted near the upper-left corner of their cells.
fn drawing() -> Document {
    let mut doc = Document::default();
    let mut g = Graphic::dragged(
        1,
        GraphicKind::Rectangle,
        Point::new(-262., -190.),
        Point::new(253., 100.),
        Default::default(),
        Default::default(),
        false,
    );
    g.kind = GraphicKind::Table;
    g.table = Some(Box::new(Table {
        cells: (0..6)
            .map(|i| Cell {
                id: i + 1,
                lo: Point::new((i % 3) as f32 / 3., (i / 3) as f32 / 2.),
                hi: Point::new((i % 3 + 1) as f32 / 3., (i / 3 + 1) as f32 / 2.),
                members: if i < 3 {
                    vec![u64::from(i + 2)]
                } else {
                    vec![]
                },
                borders: vec![],
            })
            .collect(),
    }));
    for (i, text) in ["Entry", "Solvent", "Wrapped text keeps every word"]
        .into_iter()
        .enumerate()
    {
        let frame = g.table.as_ref().unwrap().cells[i].frame(&g).unwrap();
        doc.annotations.push(Annotation {
            id: i as u64 + 2,
            position: frame.point(Point::new(0.05, 0.08)),
            text: text.into(),
            format: Default::default(),
            angle: 0.,
        });
    }
    doc.graphics.push(g);
    doc.validate().unwrap();
    doc
}

fn frame(doc: &Document, cell: usize) -> Frame {
    let g = &doc.graphics[0];
    g.table.as_ref().unwrap().cells[cell].frame(g).unwrap()
}

fn ink(parts: &[Primitive], convert: &dyn Fn(Point) -> Point) -> Vec<[Point; 4]> {
    let mut boxes = vec![];
    for part in parts {
        match part {
            Primitive::Text {
                position,
                text,
                size,
                style,
                ..
            } => {
                for (lo, hi) in crate::style::text_ink_boxes(text, *size, style) {
                    boxes.push(
                        [lo, Point::new(hi.x, lo.y), hi, Point::new(lo.x, hi.y)]
                            .map(|p| convert(position.offset(p.x, p.y))),
                    );
                }
            }
            Primitive::Rotated {
                pivot,
                angle,
                primitives,
            } => {
                boxes.extend(ink(primitives, &|p| convert(p.turned(*pivot, *angle))));
            }
            _ => {}
        }
    }
    boxes
}

fn cell_ink(doc: &Document, cell: usize) -> Vec<[Point; 4]> {
    let scene = crate::scene::primitives(doc);
    let parts = scene
        .iter()
        .filter_map(|p| match p {
            Primitive::Clip { primitives, .. } => Some(primitives),
            _ => None,
        })
        .nth(cell)
        .unwrap();
    ink(parts, &|p| p)
}

fn centre(boxes: &[[Point; 4]]) -> Point {
    let mut lo = Point::new(f32::INFINITY, f32::INFINITY);
    let mut hi = Point::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
    for p in boxes.iter().flatten() {
        lo.x = lo.x.min(p.x);
        lo.y = lo.y.min(p.y);
        hi.x = hi.x.max(p.x);
        hi.y = hi.y.max(p.y);
    }
    Point::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.)
}

fn assert_visible(doc: &Document) {
    for i in 0..3 {
        let boxes = cell_ink(doc, i);
        assert!(!boxes.is_empty());
        assert_eq!(
            boxes.len(),
            doc.annotations[i]
                .text
                .chars()
                .filter(|c| !c.is_whitespace())
                .count(),
            "The derived layout must retain every glyph"
        );
        assert!(
            boxes.iter().flatten().all(|p| {
                let p = frame(doc, i).local(*p).unwrap();
                (-0.0001..=1.0001).contains(&p.x) && (-0.0001..=1.0001).contains(&p.y)
            }),
            "Caption {i} crosses its affine cell clip"
        );
    }
}

#[test]
fn existing_short_and_wrapped_cell_ink_survives_a_half_turn() {
    let mut doc = drawing();
    assert_visible(&doc);
    let before = doc.clone();
    let pivot = frame(&doc, 0).point(Point::new(1.5, 1.));
    let centres: Vec<_> = (0..2).map(|i| centre(&cell_ink(&doc, i))).collect();
    editing::transform(&mut doc, &[1], Transform::Rotate(180.));
    assert_visible(&doc);
    for (i, old) in centres.iter().enumerate() {
        assert!(centre(&cell_ink(&doc, i)).distance(old.turned(pivot, 180.)) < 0.001);
    }
    for (a, old) in doc.annotations.iter().zip(&before.annotations) {
        assert_eq!(a.angle, 0.);
        assert_eq!(a.format, old.format);
        assert_eq!(a.text, old.text);
        assert_eq!(
            Membership::new(&doc).unwrap().cell_of(a.id),
            Membership::new(&before).unwrap().cell_of(a.id)
        );
    }
    assert!(frame(&doc, 0).point(Point::new(1.5, 1.)).distance(pivot) < 0.001);
    doc.validate().unwrap();
}

#[test]
fn horizontal_and_vertical_flips_keep_existing_cell_glyphs_inside_the_clip() {
    for transform in [Transform::FlipHorizontal, Transform::FlipVertical] {
        let mut doc = drawing();
        let formats: Vec<_> = doc.annotations.iter().map(|a| a.format.clone()).collect();
        editing::transform(&mut doc, &[1], transform);
        assert_visible(&doc);
        for (a, format) in doc.annotations.iter().zip(formats) {
            assert_eq!(a.angle, 0.);
            assert_eq!(a.format, format);
        }
        doc.validate().unwrap();
    }
}

#[test]
fn explicit_pivot_half_turn_preserves_wrapped_width_and_painted_centre() {
    let mut doc = drawing();
    doc.annotations[2].format.width_pt = Some(55.);
    assert_visible(&doc);
    let before = doc.clone();
    let old = centre(&cell_ink(&doc, 2));
    let pivot = Point::new(30., -40.);
    editing::transform_about(&mut doc, &[1], pivot, 1., 180.);
    assert_visible(&doc);
    assert!(centre(&cell_ink(&doc, 2)).distance(old.turned(pivot, 180.)) < 0.001);
    assert_eq!(doc.annotations[2].format, before.annotations[2].format);
}

#[test]
fn two_half_turns_restore_existing_cell_caption_ink_without_pivot_drift() {
    let mut doc = drawing();
    let before = doc.clone();
    let centres: Vec<_> = (0..3).map(|i| centre(&cell_ink(&doc, i))).collect();
    for _ in 0..2 {
        editing::transform(&mut doc, &[1], Transform::Rotate(180.));
        assert_visible(&doc);
    }
    for (i, a) in doc.annotations.iter().enumerate() {
        assert!(centre(&cell_ink(&doc, i)).distance(centres[i]) < 0.001);
        assert!(a.position.distance(before.annotations[i].position) < 0.001);
        assert_eq!(a.format, before.annotations[i].format);
    }
}

#[test]
fn transforming_owned_text_independently_keeps_the_ordinary_text_convention() {
    for transform in [Transform::Rotate(180.), Transform::FlipHorizontal] {
        let mut doc = drawing();
        let frame_before = doc.graphics[0].clone();
        let mut ordinary = doc.clone();
        ordinary.graphics.clear();
        editing::transform(&mut doc, &[2], transform);
        editing::transform(&mut ordinary, &[2], transform);
        assert_eq!(doc.annotations, ordinary.annotations);
        assert_eq!(doc.graphics[0], frame_before);
        assert_eq!(Membership::new(&doc).unwrap().cell_of(2).unwrap().table, 1);
    }
}

#[test]
fn mixed_free_text_keeps_its_insertion_point_and_unselected_text_is_unchanged() {
    let mut doc = drawing();
    let mut free = doc.annotations[0].clone();
    free.id = 20;
    free.position = Point::new(400., -200.);
    let mut unselected = free.clone();
    unselected.id = 21;
    doc.annotations.extend([free.clone(), unselected.clone()]);
    let pivot = Point::new(10., 20.);
    editing::transform_about(&mut doc, &[1, 20], pivot, 1., 180.);
    assert!(
        doc.annotations[3]
            .position
            .distance(free.position.turned(pivot, 180.))
            < 0.001
    );
    assert_eq!(doc.annotations[3].angle, free.angle);
    assert_eq!(doc.annotations[3].format, free.format);
    assert_eq!(doc.annotations[4], unselected);
    assert_visible(&doc);
}

#[test]
fn raw_owned_overflow_keeps_its_existing_insertion_point_transform() {
    for transform in [
        Transform::Rotate(180.),
        Transform::FlipHorizontal,
        Transform::FlipVertical,
    ] {
        let mut doc = drawing();
        doc.annotations[0].position = Point::new(5000., -3000.);
        let old = doc.annotations[0].clone();
        let pivot = Frame::of(&doc.graphics[0])
            .unwrap()
            .point(Point::new(0.5, 0.5));
        let expected = match transform {
            Transform::FlipHorizontal => Point::new(2. * pivot.x - old.position.x, old.position.y),
            Transform::FlipVertical => Point::new(old.position.x, 2. * pivot.y - old.position.y),
            _ => old.position.turned(pivot, 180.),
        };
        editing::transform(&mut doc, &[1], transform);
        assert!(doc.annotations[0].position.distance(expected) < 0.001);
        assert_eq!(doc.annotations[0].format, old.format);
        assert_eq!(doc.annotations[0].angle, old.angle);
        assert_eq!(
            Membership::new(&doc)
                .unwrap()
                .cell_of(old.id)
                .unwrap()
                .table,
            1
        );
    }
}
