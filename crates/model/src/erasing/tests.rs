use super::*;
#[test]
fn fast_stroke_cuts_both_ring_edges_without_deleting_off_path_atoms() {
    let mut doc = Document::default();
    crate::editing::ring(&mut doc, Point::default(), 6, false, 0.);
    let atoms = doc.atoms.clone();
    stroke(&mut doc, Point::new(0., 100.), Point::new(0., -100.), 2.);
    assert_eq!(doc.atoms, atoms);
    assert_eq!(doc.bonds.len(), 4);
    doc.validate().unwrap();
}
#[test]
fn stroke_removes_crossed_atoms_and_objects_but_preserves_nearby_geometry() {
    use crate::{
        arrows::{ArrowStyle, Preset},
        document::{Annotation, Arrow},
        graphics::{BracketSides, Graphic, GraphicKind, GraphicStyle},
    };
    let mut doc = Document::default();
    let crossed = doc.add_atom("N", Point::new(0., 0.));
    let untouched = doc.add_atom("O", Point::new(30., 0.));
    doc.add_bond(crossed, untouched, 1, "plain");
    doc.arrows.push(Arrow::new(
        10,
        Point::new(-40., 30.),
        Point::new(40., 30.),
        Preset::Forward,
        ArrowStyle::default(),
    ));
    doc.annotations.push(Annotation {
        id: 11,
        position: Point::new(-10., 50.),
        text: "label".into(),
        format: Default::default(),
    });
    doc.graphics.push(Graphic::dragged(
        12,
        GraphicKind::Rectangle,
        Point::new(-20., 80.),
        Point::new(20., 120.),
        GraphicStyle::default(),
        BracketSides::Both,
        false,
    ));
    stroke(&mut doc, Point::new(0., -30.), Point::new(0., 150.), 2.);
    assert!(doc.atom(crossed).is_none());
    assert!(doc.atom(untouched).is_some());
    assert!(
        doc.bonds.is_empty()
            && doc.arrows.is_empty()
            && doc.annotations.is_empty()
            && doc.graphics.is_empty()
    );
    doc.validate().unwrap();
}
#[test]
fn parallel_and_degenerate_strokes_do_not_erase_distant_segments() {
    assert!(!crossed(
        Point::new(0., 0.),
        Point::new(0., 0.),
        Point::new(30., 0.),
        Point::new(60., 0.),
        3.
    ));
    assert!(!crossed(
        Point::new(0., 0.),
        Point::new(60., 0.),
        Point::new(0., 10.),
        Point::new(60., 10.),
        3.
    ));
    assert!(crossed(
        Point::new(0., 0.),
        Point::new(60., 0.),
        Point::new(30., -50.),
        Point::new(30., 50.),
        0.
    ));
}
