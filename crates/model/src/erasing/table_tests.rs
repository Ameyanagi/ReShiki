use super::*;
use crate::{
    document::Annotation,
    graphics::{
        Graphic, GraphicKind,
        table::{Cell, Table},
    },
};

fn table(members: Vec<u64>) -> Graphic {
    let mut graphic = Graphic::dragged(
        1000,
        GraphicKind::Rectangle,
        Point::default(),
        Point::new(200., 200.),
        Default::default(),
        Default::default(),
        false,
    );
    graphic.kind = GraphicKind::Table;
    graphic.table = Some(Box::new(Table {
        cells: vec![Cell {
            id: 1,
            lo: Point::default(),
            hi: Point::new(0.5, 1.),
            members,
            borders: vec![],
        }],
    }));
    graphic
}

#[test]
fn affine_stroke_clipping_keeps_crossings_and_rejects_parallel_overflow() {
    for axis_x in [Point::new(100., 20.), Point::new(-100., -20.)] {
        let frame = Frame {
            origin: Point::new(30., 40.),
            axis_x,
            axis_y: Point::new(35., 150.),
        };
        let from = frame.point(Point::new(-1., 0.5));
        let to = frame.point(Point::new(2., 0.5));
        let (a, b) = clip_segment(frame, from, to).unwrap();
        assert!(a.distance(frame.point(Point::new(0., 0.5))) < 0.001);
        assert!(b.distance(frame.point(Point::new(1., 0.5))) < 0.001);
        assert!(
            clip_segment(
                frame,
                frame.point(Point::new(1.2, -1.)),
                frame.point(Point::new(1.2, 2.)),
            )
            .is_none()
        );
        let point = frame.point(Point::new(0.25, 0.75));
        assert_eq!(clip_segment(frame, point, point), Some((point, point)));
    }
}

#[test]
fn outside_swept_strokes_preserve_owned_atoms_bonds_text_and_graphics() {
    let mut doc = Document::default();
    let a = doc.add_atom("N", Point::new(130., 50.));
    let b = doc.add_atom("O", Point::new(170., 50.));
    doc.add_bond(a, b, 1, "plain");
    doc.annotations.push(Annotation {
        id: 3,
        position: Point::new(120., 90.),
        text: "Full hidden caption".into(),
        angle: 0.,
        format: Default::default(),
    });
    let mut shape = Graphic::dragged(
        4,
        GraphicKind::Rectangle,
        Point::new(120., 120.),
        Point::new(180., 180.),
        Default::default(),
        Default::default(),
        false,
    );
    shape.style.fill = Some(crate::palette::Color::Ink);
    doc.graphics.push(shape);
    doc.graphics.push(table(vec![a, b, 3, 4]));
    doc.validate().unwrap();
    let before = doc.clone();
    stroke(&mut doc, Point::new(150., 0.), Point::new(150., 200.), 7.);
    assert_eq!(doc, before);
}

#[test]
fn one_long_stroke_crossing_the_cell_still_erases_its_visible_bond() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(20., 50.));
    let b = doc.add_atom("C", Point::new(80., 50.));
    doc.add_bond(a, b, 1, "plain");
    doc.graphics.push(table(vec![a, b]));
    stroke(&mut doc, Point::new(50., -40.), Point::new(50., 240.), 2.);
    assert!(doc.bonds.is_empty());
    assert!(doc.atom(a).is_some());
    assert!(doc.atom(b).is_some());
    assert!(doc.graphics.iter().any(|g| g.table.is_some()));
}

#[test]
fn wrapped_caption_rows_are_erasable_using_the_derived_cell_height() {
    let mut doc = Document::default();
    doc.annotations.push(Annotation {
        id: 1,
        position: Point::new(5., 5.),
        text: "CompleteUnbrokenCaption".into(),
        angle: 0.,
        format: Default::default(),
    });
    let mut graphic = table(vec![1]);
    graphic.axis_x = Point::new(60., 0.);
    let frame = graphic.table.as_ref().unwrap().cells[0]
        .frame(&graphic)
        .unwrap();
    let layout = crate::scene::table_annotation_layout(&doc.annotations[0], frame);
    let fragment = layout
        .fragments
        .iter()
        .find(|fragment| fragment.position.y > doc.annotations[0].size().1)
        .unwrap();
    let point = doc.annotations[0]
        .position
        .offset(fragment.position.x + 3., fragment.position.y + 3.);
    assert!(frame.contains(point));
    assert!(!doc.annotations[0].contains(point));
    doc.graphics.push(graphic);
    stroke(&mut doc, point, point, 1.);
    assert!(doc.annotations.is_empty());
}

#[test]
fn erasing_unowned_ink_over_owned_overflow_preserves_the_clipped_member() {
    let mut doc = Document::default();
    let hidden = doc.add_atom("N", Point::new(150., 50.));
    let visible = doc.add_atom("O", Point::new(150., 50.));
    doc.graphics.push(table(vec![hidden]));
    stroke(&mut doc, Point::new(150., 0.), Point::new(150., 100.), 4.);
    assert!(doc.atom(hidden).is_some());
    assert!(doc.atom(visible).is_none());
}

#[test]
fn an_inside_eraser_cannot_reach_fully_hidden_ink_just_beyond_the_cell_edge() {
    let mut doc = Document::default();
    doc.drawing_style.line_width_pt = 0.1;
    let a = doc.add_atom("C", Point::new(101., 40.));
    let b = doc.add_atom("C", Point::new(101., 80.));
    doc.add_bond(a, b, 1, "plain");
    doc.annotations.push(Annotation {
        id: 3,
        position: Point::new(101., 100.),
        text: "Hidden".into(),
        angle: 0.,
        format: Default::default(),
    });
    let mut shape = Graphic::dragged(
        4,
        GraphicKind::Rectangle,
        Point::new(101., 130.),
        Point::new(110., 150.),
        Default::default(),
        Default::default(),
        false,
    );
    shape.style.width_pt = 0.1;
    shape.style.fill = Some(crate::palette::Color::Ink);
    doc.graphics.push(shape);
    doc.graphics.push(table(vec![a, b, 3, 4]));
    let before = doc.clone();
    stroke(&mut doc, Point::new(99., 0.), Point::new(99., 200.), 5.);
    assert_eq!(doc, before);
}
