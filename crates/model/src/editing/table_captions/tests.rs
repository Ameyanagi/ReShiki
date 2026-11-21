use crate::{
    document::{Annotation, Document, Point},
    editing::{self, Transform},
    graphics::{
        Graphic, GraphicKind,
        table::{Cell, Frame, Membership, Table},
    },
    scene::Primitive,
    typography::TextAlign,
};

#[test]
fn source_inset_stays_finite_inside_extremely_narrow_cell_axes() {
    for (width, height) in [(0.01, 100_000.), (100_000., 0.01)] {
        let frame = Frame {
            origin: Point::default(),
            axis_x: Point::new(width, 0.),
            axis_y: Point::new(0., height),
        };
        let position = frame.point(Point::new(0.25, 0.25));
        let caption = super::Caption {
            frame,
            ink: super::Ink {
                position,
                centre: Point::new(width * 0.1, height * 0.1),
                pivot: Point::default(),
                angle: 0.,
                visible: 1,
                total: 1,
                bounds: (Point::default(), Point::new(width * 0.2, height * 0.2)),
            },
        };
        let a = Annotation {
            id: 1,
            position,
            text: "X".into(),
            format: Default::default(),
            angle: 0.,
        };
        for reflected in [false, true] {
            let frame = if reflected {
                Frame {
                    origin: Point::new(width, height),
                    axis_x: Point::new(-width, 0.),
                    axis_y: Point::new(0., -height),
                }
            } else {
                frame
            };
            let target = frame.point(Point::new(0.5, 0.5));
            let placed = caption.source_inset(&a, frame, target).unwrap();
            assert!(placed.is_finite());
            assert!(super::in_cell(frame, placed));
            assert!(
                caption
                    .source_inset(&a, frame, Point::new(f32::NAN, 0.))
                    .is_none()
            );
        }
    }
}

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

fn left(frame: Frame) -> f32 {
    frame
        .corners()
        .iter()
        .map(|p| p.x)
        .fold(f32::INFINITY, f32::min)
}

#[test]
fn auto_right_and_center_keep_semantic_insets_and_complete_glyphs_when_centres_are_unattainable() {
    for alignment in [TextAlign::Right, TextAlign::Center] {
        for transform in [
            Transform::Rotate(180.),
            Transform::FlipHorizontal,
            Transform::FlipVertical,
        ] {
            let mut doc = drawing();
            for a in &mut doc.annotations {
                a.format.alignment = alignment;
            }
            assert_visible(&doc);
            let before = doc.clone();
            editing::transform(&mut doc, &[1], transform);
            assert_visible(&doc);
            for i in 0..3 {
                assert_eq!(doc.annotations[i].format, before.annotations[i].format);
                assert_eq!(doc.annotations[i].angle, 0.);
                let old_inset = before.annotations[i].position.x - left(frame(&before, i));
                let inset = doc.annotations[i].position.x - left(frame(&doc, i));
                assert!((inset - old_inset).abs() < 0.001);
                let old_centre = centre(&cell_ink(&before, i)).x - left(frame(&before, i));
                let new_centre = centre(&cell_ink(&doc, i)).x - left(frame(&doc, i));
                assert!((new_centre - old_centre).abs() < 0.001);
            }
            let after = doc.clone();
            let mut history = crate::document::History::default();
            assert!(history.commit(before.clone(), &doc));
            assert!(history.undo(&mut doc));
            assert_eq!(doc, before);
            assert!(history.redo(&mut doc));
            assert_eq!(doc, after);
            // Keeping the source Auto width also retains the original wraps
            // and makes a second reflection/half-turn restore the raw inset.
            editing::transform(&mut doc, &[1], transform);
            assert_visible(&doc);
            for (a, old) in doc.annotations.iter().zip(&before.annotations) {
                assert!(a.position.distance(old.position) < 0.001);
                assert_eq!(a.format, old.format);
            }
        }
    }
}

#[test]
fn aligned_auto_wrap_survives_a_mirrored_font_bearing_at_the_cell_edge() {
    for alignment in [TextAlign::Right, TextAlign::Center] {
        let mut doc = drawing();
        for i in 0..3 {
            doc.annotations[i].format.alignment = alignment;
            let raw = doc.annotations[i].position;
            let bottom = frame(&doc, i)
                .corners()
                .iter()
                .map(|p| p.y)
                .fold(f32::NEG_INFINITY, f32::max);
            let ink_bottom = cell_ink(&doc, i)
                .iter()
                .flatten()
                .map(|p| p.y - raw.y)
                .fold(f32::NEG_INFINITY, f32::max);
            doc.annotations[i].position.y = bottom - ink_bottom - 0.1;
        }
        assert_visible(&doc);
        let formats: Vec<_> = doc.annotations.iter().map(|a| a.format.clone()).collect();
        editing::transform(&mut doc, &[1], Transform::Rotate(180.));
        assert_visible(&doc);
        for (i, a) in doc.annotations.iter().enumerate() {
            let p = frame(&doc, i).local(a.position).unwrap();
            assert!((0. ..1.).contains(&p.x) && (0. ..1.).contains(&p.y));
            assert_eq!(a.format, formats[i]);
        }
    }
}

#[test]
fn explicit_width_right_and_center_use_fully_visible_exact_centres_when_attainable() {
    for alignment in [TextAlign::Right, TextAlign::Center] {
        let mut doc = drawing();
        for a in &mut doc.annotations {
            a.format.alignment = alignment;
            a.format.width_pt = Some(55.);
        }
        assert_visible(&doc);
        let before = doc.clone();
        let centres: Vec<_> = (0..3).map(|i| centre(&cell_ink(&doc, i))).collect();
        let pivot = editing::rotation_center(&doc, &[1]).unwrap();
        editing::transform(&mut doc, &[1], Transform::Rotate(180.));
        assert_visible(&doc);
        for (i, old) in centres.iter().enumerate() {
            assert!(centre(&cell_ink(&doc, i)).distance(old.turned(pivot, 180.)) < 0.001);
            assert_eq!(doc.annotations[i].format, before.annotations[i].format);
        }
    }
}

#[test]
fn partial_overflow_is_never_captured_and_keeps_the_identical_raw_transform() {
    use std::collections::HashSet;
    for angle in [0., 15.] {
        for transform in [
            Transform::Rotate(180.),
            Transform::FlipHorizontal,
            Transform::FlipVertical,
        ] {
            let mut doc = drawing();
            doc.annotations[0].text = "Entry\nEntry".into();
            doc.annotations[0].position = frame(&doc, 0).point(Point::new(0.05, 0.7));
            doc.annotations[0].angle = angle;
            let cells = crate::scene::table::Layout::new(&doc);
            let old_ink = super::Ink::new(&cells, &doc.annotations[0]).unwrap();
            assert!(old_ink.visible > 0 && old_ink.visible < old_ink.total);
            let ids: HashSet<_> = crate::graphics::table::with_contents(&doc, &[1])
                .into_iter()
                .collect();
            let captured = super::Positions::new(&doc, &ids);
            assert!(captured.get(2).is_none());
            assert!(captured.get(3).is_some());
            let old = doc.annotations[0].clone();
            let references = crate::graphics::table::reference_ids(&doc, &[1]);
            let pivot = if matches!(transform, Transform::Rotate(_)) {
                editing::rotation_center(&doc, &[1]).unwrap()
            } else {
                editing::center(&doc, &references)
            };
            let vector = |p: Point| match transform {
                Transform::FlipHorizontal => Point::new(-p.x, p.y),
                Transform::FlipVertical => Point::new(p.x, -p.y),
                _ => {
                    let (s, c) = 180f32.to_radians().sin_cos();
                    Point::new(p.x * c - p.y * s, p.x * s + p.y * c)
                }
            };
            let convert = |p: Point| {
                let p = vector(Point::new(p.x - pivot.x, p.y - pivot.y));
                pivot.offset(p.x, p.y)
            };
            let mut expected = old.clone();
            if angle == 0. {
                expected.position = convert(old.position);
            } else {
                let (w, h) = old.size();
                let centre = convert(old.position.offset(w / 2., h / 2.));
                let (s, c) = angle.to_radians().sin_cos();
                expected.angle = crate::document::text_angle(
                    vector(Point::new(c, s)),
                    vector(Point::new(-s, c)),
                );
                expected.position = centre.offset(-w / 2., -h / 2.);
            }
            editing::transform(&mut doc, &[1], transform);
            assert_eq!(doc.annotations[0], expected);
        }
    }
}

#[test]
fn empty_or_noncaptured_selections_need_no_post_transform_cell_layout() {
    use std::collections::HashSet;
    let mut doc = drawing();
    assert!(super::Positions::new(&doc, &HashSet::from([2])).is_empty());
    for a in &mut doc.annotations {
        a.text = " \n ".into();
    }
    let selected = HashSet::from([1, 2, 3, 4]);
    assert!(super::Positions::new(&doc, &selected).is_empty());
    doc.graphics.clear();
    assert!(super::Positions::new(&doc, &selected).is_empty());
}
