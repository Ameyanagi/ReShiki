use super::{Annotation, Document, Point, text_angle};
use crate::typography::TextFormat;

fn caption(angle: f32) -> Annotation {
    Annotation {
        id: 1,
        position: Point::new(10., 20.),
        text: "hν, 25 °C".into(),
        format: TextFormat::default(),
        angle,
    }
}

fn near(a: Point, b: Point) -> bool {
    a.distance(b) < 1e-3
}

#[test]
fn a_turned_box_keeps_its_centre_and_size() {
    let upright = caption(0.);
    let turned = caption(90.);
    let (w, h) = upright.size();
    assert!(near(turned.centre(), upright.centre()));
    let [a, b, _, d] = turned.corners();
    // A quarter turn clockwise: the top edge now runs downward.
    assert!(near(b, a.offset(0., w)));
    assert!(near(d, a.offset(-h, 0.)));
    let (lo, hi) = turned.bounds();
    assert!((hi.x - lo.x - h).abs() < 1e-3 && (hi.y - lo.y - w).abs() < 1e-3);
}

#[test]
fn hits_follow_the_turned_box() {
    let turned = caption(90.);
    let (w, h) = turned.size();
    let centre = turned.centre();
    assert!(turned.contains(centre));
    // Along the turned text, past the upright box's height.
    assert!(turned.contains(centre.offset(0., w / 2. - 0.5)));
    // The upright box's far right end is empty once the text turns.
    assert!(w / 2. - 0.5 > h / 2.);
    assert!(!turned.contains(centre.offset(w / 2. - 0.5, 0.)));
}

#[test]
fn text_angle_keeps_mirrored_text_readable() {
    let x = Point::new(1., 0.);
    let y = Point::new(0., 1.);
    assert_eq!(text_angle(x, y), 0.);
    assert!((text_angle(y, Point::new(-1., 0.)) - 90.).abs() < 1e-3);
    // Flips leave upright text upright, not upside down.
    assert_eq!(text_angle(Point::new(-1., 0.), y), 0.);
    assert_eq!(text_angle(x, Point::new(0., -1.)), 0.);
    // A full turn is upright again.
    let (s, c) = 360f32.to_radians().sin_cos();
    assert_eq!(text_angle(Point::new(c, s), Point::new(-s, c)), 0.);
}

#[test]
fn turned_captions_turn_with_the_drawing_and_upright_ones_stay_upright() {
    let mut doc = Document::default();
    doc.annotations.push(caption(30.));
    doc.annotations.push(Annotation {
        id: 2,
        ..caption(0.)
    });
    // With something besides text in the selection.
    let atom = doc.add_atom("C", Point::new(100., 100.));
    let centre = doc.annotations[0].centre();
    crate::editing::transform_about(&mut doc, &[1, 2, atom], Point::default(), 1., 90.);
    let turned = &doc.annotations[0];
    assert!((turned.angle - 120.).abs() < 1e-3);
    assert!(near(turned.centre(), centre.turned(Point::default(), 90.)));
    assert_eq!(doc.annotations[1].angle, 0.);

    // A mirror reflects the turn and keeps the text readable.
    crate::editing::transform(
        &mut doc,
        &[1, atom],
        crate::editing::Transform::FlipHorizontal,
    );
    assert!((doc.annotations[0].angle - 60.).abs() < 1e-3);
}

#[test]
fn turned_text_saves_as_drawing_elements_and_draws_turned() {
    let mut doc = Document::default();
    doc.annotations.push(caption(0.));
    let upright = doc.file_version();
    doc.annotations[0].angle = 45.;
    assert!(doc.file_version() > upright);
    assert_eq!(doc.file_version(), super::DRAWING_ELEMENTS);

    let json = serde_json::to_string(&doc.annotations[0]).unwrap();
    assert!(json.contains("\"angle\":45"));
    let back: Annotation = serde_json::from_str(&json).unwrap();
    assert_eq!(back.angle, 45.);
    let json = serde_json::to_string(&caption(0.)).unwrap();
    assert!(!json.contains("angle"), "upright text writes no angle");

    let scene = crate::scene::primitives(&doc);
    let Some(crate::scene::Primitive::Rotated {
        pivot,
        angle,
        primitives,
    }) = scene.last()
    else {
        panic!("turned text draws as one turned group");
    };
    assert_eq!(*angle, 45.);
    assert!(near(*pivot, doc.annotations[0].centre()));
    assert!(!primitives.is_empty());
    let svg = crate::scene::svg(&doc);
    assert!(svg.contains("rotate(45 "), "{svg}");
}

#[test]
fn text_selected_alone_turns_in_place() {
    let mut doc = Document::default();
    doc.annotations.push(caption(0.));
    let centre = doc.annotations[0].centre();
    crate::editing::transform(&mut doc, &[1], crate::editing::Transform::Rotate(90.));
    assert!((doc.annotations[0].angle - 90.).abs() < 1e-3);
    assert!(near(doc.annotations[0].centre(), centre));
}

#[test]
fn a_non_finite_turn_is_refused() {
    let mut doc = Document::default();
    doc.annotations.push(caption(f32::NAN));
    assert!(doc.validate().is_err());
    doc.annotations[0].angle = 30.;
    assert!(doc.validate().is_ok());
}
