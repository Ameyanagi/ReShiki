use super::*;
use crate::graphics::{BracketSides, GraphicKind, GraphicStyle};
use crate::typography::{TextAlign, TextFormat};

const SHAPE: u64 = 1;

fn shape(kind: GraphicKind, from: Point, to: Point) -> Document {
    let mut doc = Document::default();
    doc.graphics.push(Graphic::dragged(
        SHAPE,
        kind,
        from,
        to,
        GraphicStyle::default(),
        BracketSides::default(),
        false,
    ));
    doc
}

/// Commit an edit as the editor does.
fn edit(doc: &mut Document, change: impl FnOnce(&mut Document)) {
    let before = doc.clone();
    change(doc);
    let _ = crate::transaction::reconcile(doc, before).unwrap();
}

/// Type `text` into the shape, as a double-click and typing do.
fn type_into(doc: &mut Document, text: &str) -> u64 {
    let id = doc.next_id();
    edit(doc, |doc| {
        let g = doc.graphics[0].clone();
        let format = TextFormat {
            alignment: TextAlign::Center,
            width_pt: Some(width_pt(&g)),
            ..Default::default()
        };
        doc.annotations.push(Annotation {
            angle: 0.,
            id,
            position: start(&g, &format),
            text: text.into(),
            format,
        });
        doc.graphics[0].text = Some(id);
    });
    id
}

fn text(doc: &Document, id: u64) -> &Annotation {
    doc.annotations.iter().find(|a| a.id == id).unwrap()
}

fn centre_of(a: &Annotation) -> Point {
    let (w, h) = a.size();
    a.position.offset(w / 2., h / 2.)
}

fn near(a: Point, b: Point) -> bool {
    (a.x - b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3
}

#[test]
fn typed_text_is_centred_and_wrapped_inside_the_shape() {
    let mut doc = shape(
        GraphicKind::Rectangle,
        Point::new(0., 0.),
        Point::new(200., 80.),
    );
    let id = type_into(&mut doc, "hν");
    let a = text(&doc, id);
    assert!(near(centre_of(a), Point::new(100., 40.)), "{a:?}");
    let pt = crate::style::DEFAULT.points_per_world();
    assert!((a.format.width_pt.unwrap() - 200. * 0.85 * pt).abs() < 1e-3);
    // A long text wraps to the shape's inner width.
    edit(&mut doc, |doc| {
        doc.annotations[0].text = "light and heat under reflux overnight, then cooled on ice, filtered and washed with cold ethanol".into();
    });
    let a = text(&doc, id);
    assert!(a.size().0 <= 200. * 0.85 + 1e-3);
    assert!(a.size().1 > text(&shape_with("x"), 2).size().1);
    assert!(near(centre_of(a), Point::new(100., 40.)));
    assert_eq!(doc.current().version, crate::document::DRAWING_ELEMENTS);
}

fn shape_with(text: &str) -> Document {
    let mut doc = shape(
        GraphicKind::Rectangle,
        Point::new(0., 0.),
        Point::new(200., 80.),
    );
    type_into(&mut doc, text);
    doc
}

#[test]
fn text_follows_its_shape_when_it_moves_or_resizes() {
    let mut doc = shape(
        GraphicKind::Ellipse,
        Point::new(0., 0.),
        Point::new(60., 60.),
    );
    let id = type_into(&mut doc, "💡");
    edit(&mut doc, |doc| doc.translate(&[SHAPE], 100., 50.));
    assert!(near(centre_of(text(&doc, id)), Point::new(130., 80.)));
    edit(&mut doc, |doc| {
        doc.graphics[0].axis_x = Point::new(120., 0.);
    });
    let a = text(&doc, id);
    assert!(near(centre_of(a), Point::new(160., 80.)));
    let pt = crate::style::DEFAULT.points_per_world();
    assert!((a.format.width_pt.unwrap() - 120. * 0.65 * pt).abs() < 1e-3);
}

#[test]
fn dragging_the_text_drags_its_shape() {
    let mut doc = shape(
        GraphicKind::RoundedRectangle,
        Point::new(0., 0.),
        Point::new(80., 40.),
    );
    let id = type_into(&mut doc, "Base");
    edit(&mut doc, |doc| doc.translate(&[id], 10., -5.));
    assert!(near(doc.graphics[0].origin, Point::new(10., -5.)));
    assert!(near(centre_of(text(&doc, id)), Point::new(50., 15.)));
}

#[test]
fn a_rotated_shape_wraps_text_to_its_horizontal_chord() {
    // A 80 × 40 rectangle turned a quarter: 40 wide on the page.
    let mut doc = shape(
        GraphicKind::Rectangle,
        Point::new(0., 0.),
        Point::new(80., 40.),
    );
    doc.graphics[0].axis_x = Point::new(0., 80.);
    doc.graphics[0].axis_y = Point::new(-40., 0.);
    let (centre, width) = inner(&doc.graphics[0]);
    assert!(near(centre, Point::new(-20., 40.)));
    assert!((width - 40. * 0.85).abs() < 1e-3);
}

#[test]
fn deleting_a_shape_takes_its_text_and_deleting_the_text_frees_the_shape() {
    let mut doc = shape_with("hν");
    let ids = with_texts(&doc, &[SHAPE]);
    doc.delete(&ids);
    assert!(doc.annotations.is_empty() && doc.graphics.is_empty());
    let mut doc = shape_with("hν");
    let id = doc.graphics[0].text.unwrap();
    doc.delete(&[id]);
    assert_eq!(doc.graphics[0].text, None);
    assert!(doc.validate().is_ok());
}

#[test]
fn copies_keep_the_text_in_its_shape() {
    let doc = shape_with("Adenine");
    let part = crate::editing::selection(&doc, &[SHAPE]);
    assert_eq!(part.annotations.len(), 1);
    let mut pasted = doc.clone();
    let ids = crate::editing::append(&mut pasted, &part, Point::new(200., 0.));
    let copy = pasted
        .graphics
        .iter()
        .find(|g| ids.contains(&g.id))
        .unwrap();
    let held = copy.text.unwrap();
    assert!(ids.contains(&held) && Some(held) != doc.graphics[0].text);
    assert!(pasted.validate().is_ok());
    // Saved and reopened, the link survives.
    let json = serde_json::to_string(&pasted.current()).unwrap();
    let reopened: Document = serde_json::from_str(&json).unwrap();
    assert_eq!(reopened.graphics, pasted.graphics);
}

#[test]
fn only_plain_closed_shapes_hold_text_once() {
    let mut doc = shape_with("A");
    let id = doc.graphics[0].text.unwrap();
    let mut line = doc.clone();
    line.graphics[0].kind = GraphicKind::Line;
    assert!(validate(&line).is_err());
    let mut twice = doc.clone();
    let mut other = twice.graphics[0].clone();
    other.id = 9;
    twice.graphics.push(other);
    assert!(validate(&twice).is_err());
    // Turning the rectangle into a figure panel keeps the text as a caption.
    edit(&mut doc, |doc| {
        doc.graphics[0].panel = Some(Default::default());
    });
    assert_eq!(doc.graphics[0].text, None);
    assert!(doc.annotations.iter().any(|a| a.id == id));
}

#[test]
fn copying_the_text_alone_copies_plain_text() {
    let doc = shape_with("Base");
    let id = doc.graphics[0].text.unwrap();
    let part = crate::editing::selection(&doc, &[id]);
    assert_eq!(part.annotations.len(), 1);
    assert!(part.graphics.is_empty());
}

#[test]
fn turning_or_aligning_a_shape_keeps_its_text_centred() {
    let mut doc = shape_with("hν");
    let id = doc.graphics[0].text.unwrap();
    let ids = [SHAPE, id];
    let start = doc.graphics[0].clone();
    for _ in 0..2 {
        edit(&mut doc, |doc| {
            let pivot = crate::editing::rotation_center(doc, &ids).unwrap();
            crate::editing::transform_about(doc, &ids, pivot, 1., 180.);
        });
    }
    let (centre, _) = inner(&doc.graphics[0]);
    assert!(near(centre, inner(&start).0), "{centre:?}");
    assert!(near(centre_of(text(&doc, id)), centre));
}

#[test]
fn an_arrow_never_claims_text_in_a_shape() {
    let mut doc = shape(
        GraphicKind::Rectangle,
        Point::new(20., -28.),
        Point::new(100., 0.),
    );
    doc.arrows.push(crate::document::Arrow::new(
        50,
        Point::new(0., 0.),
        Point::new(120., 0.),
        crate::arrows::Preset::Forward,
        crate::arrows::ArrowStyle::preset(crate::arrows::Preset::Forward),
    ));
    let id = type_into(&mut doc, "A");
    assert_eq!(doc.graphics[0].text, Some(id));
    assert!(doc.arrow_labels.is_empty());
}

#[test]
fn a_small_shape_still_takes_text() {
    let mut doc = shape(
        GraphicKind::Ellipse,
        Point::new(0., 0.),
        Point::new(20., 20.),
    );
    let id = type_into(&mut doc, "+");
    assert_eq!(text(&doc, id).format.width_pt, Some(10.));
    assert!(near(centre_of(text(&doc, id)), Point::new(10., 10.)));
}
