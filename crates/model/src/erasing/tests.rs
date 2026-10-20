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

/// Ethanol with a model shown as `display`, and a separate methane.
fn modelled(display: Display) -> (Document, Vec<u64>, u64) {
    use crate::models3d::{ForceField, Model3d, Placed, set_display};
    let mut doc = Document::default();
    let c1 = doc.add_atom("C", Point::new(0., 0.));
    let c2 = doc.add_atom("C", Point::new(36., -21.));
    let o = doc.add_atom("O", Point::new(72., 0.));
    doc.add_bond(c1, c2, 1, "plain");
    doc.add_bond(c2, o, 1, "plain");
    let methane = doc.add_atom("C", Point::new(0., 200.));
    let atoms = [
        (c1, [-1.2, -0.2, 0.1]),
        (c2, [0.1, 0.5, -0.1]),
        (o, [1.2, -0.3, 0.2]),
    ]
    .map(|(id, xyz)| Placed { id, xyz })
    .to_vec();
    doc.models
        .push(Model3d::new(&doc, atoms, vec![], ForceField::Mmff94s));
    doc.require_version(crate::document::MODELS_3D);
    assert!(set_display(&mut doc, 0, display));
    (doc, vec![c1, c2, o], methane)
}

#[test]
fn stroke_across_a_3d_model_erases_its_molecule_but_not_the_hidden_drawing() {
    let (mut doc, atoms, methane) = modelled(Display::ThreeD);
    // Move the model off its hidden drawing so each can be crossed alone.
    doc.models[0].offset = Point::new(0., -300.);
    let before = doc.clone();
    stroke(&mut doc, Point::new(-10., 0.), Point::new(80., 0.), 2.);
    assert_eq!(doc, before, "the hidden 2D drawing is not erasable");

    let (balls, sticks) = crate::models3d::balls_and_sticks(&doc, &doc.models[0]).unwrap();
    let ball = balls[0].0;
    stroke(&mut doc, ball, ball, 2.);
    assert!(atoms.iter().all(|id| doc.atom(*id).is_none()));
    assert!(doc.bonds.is_empty() && doc.models.is_empty());
    assert!(doc.atom(methane).is_some());
    doc.validate().unwrap();

    // A stick between two balls counts as the model too.
    let mut doc = before;
    let (a, b, _) = sticks[0];
    let middle = Point::new((a.x + b.x) / 2., (a.y + b.y) / 2.);
    assert!(balls.iter().all(|(c, r)| c.distance(middle) > r + 0.1));
    stroke(&mut doc, middle, middle, 0.1);
    assert!(atoms.iter().all(|id| doc.atom(*id).is_none()));
}

#[test]
fn in_both_mode_the_drawing_erases_as_before_and_the_model_erases_the_molecule() {
    let (mut doc, atoms, methane) = modelled(Display::Both);
    let model = doc.clone();
    stroke(&mut doc, Point::new(0., 10.), Point::new(0., -10.), 2.);
    assert!(doc.atom(atoms[0]).is_none());
    assert!(doc.atom(atoms[1]).is_some() && doc.atom(atoms[2]).is_some());

    let mut doc = model;
    let (balls, _) = crate::models3d::balls_and_sticks(&doc, &doc.models[0]).unwrap();
    let ball = balls[2].0;
    assert!(ball.x > 72., "the model sits beside the drawing");
    stroke(&mut doc, ball, ball, 2.);
    assert!(atoms.iter().all(|id| doc.atom(*id).is_none()));
    assert!(doc.models.is_empty() && doc.atom(methane).is_some());
    doc.validate().unwrap();
}
