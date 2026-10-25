use super::*;

#[test]
fn a_wedge_closing_edge_blocks_a_mark_even_when_its_center_is_outside_the_fill() {
    let wedge = Obstacle {
        points: vec![
            Point::new(-10., -10.),
            Point::new(10., -10.),
            Point::new(10., 10.),
        ],
        width: 0.,
        filled: true,
    };
    assert!(intersects(
        &wedge,
        (Point::new(-1., -0.5), Point::new(1., 1.5))
    ));
    assert!(!intersects(
        &wedge,
        (Point::new(-5., 2.), Point::new(-3., 4.))
    ));
    assert!(intersects(
        &wedge,
        (Point::new(3., -1.), Point::new(4., 0.))
    ));
}

#[test]
fn clicked_circled_charge_clears_nitro_bonds_neighbour_labels_and_saved_marks() {
    use crate::graphics::{GraphicKind, GraphicStyle};
    use crate::scientific::{Drawing, Phase, SymbolKind};
    let mut doc = Document::default();
    let n = doc.add_atom("N", Point::default());
    let oxygen = doc.add_atom("O", Point::new(0., -42.));
    doc.add_bond(n, oxygen, 2, "plain");
    let c = doc.add_atom("C", Point::new(-36.4, 21.));
    doc.add_bond(c, n, 1, "plain");
    let o = doc.add_atom("O", Point::new(36.4, 21.));
    doc.atom_mut(o).unwrap().charge = -1;
    doc.add_bond(n, o, 1, "plain");
    crate::scientific::attach(
        doc.atom_mut(o).unwrap(),
        SymbolKind::LonePair,
        Point::new(24., 0.),
    )
    .unwrap();
    let saved = doc.atom(o).unwrap().marks[0].clone();
    let at = doc.atom(n).unwrap().position;
    Drawing {
        kind: GraphicKind::Symbol(SymbolKind::CirclePlus),
        style: GraphicStyle::default(),
        phase: Phase::Solid,
        flipped: false,
        attach: true,
        snap_orbitals: true,
    }
    .place(&mut doc, at, at, false, 10.)
    .unwrap();
    let owner = doc.atom(n).unwrap();
    assert_eq!(doc.atom(o).unwrap().marks[0], saved);
    assert_eq!(owner.charge, 1);
    let mark = owner.marks.last().unwrap().clone();
    assert_eq!(mark.kind, MarkKind::CircledCharge);
    assert!(
        mark.offset.x.abs() > 0.1,
        "the upward double bond blocks the old default"
    );
    let mut candidate = owner.clone();
    candidate.marks = vec![mark.clone()];
    let bounds = bounds(&styled_mark_parts(&candidate, &doc.drawing_style));
    for atom in &doc.atoms {
        for label in crate::scene::atom_label_ink_boxes(atom, &doc) {
            assert!(!overlaps(bounds, label), "label {atom:?}");
        }
    }
    let mut chemistry = doc.clone();
    chemistry.atom_mut(n).unwrap().marks.pop();
    for obstacle in crate::scene::primitives(&chemistry)
        .into_iter()
        .flat_map(primitive_obstacles)
    {
        assert!(!intersects(&obstacle, expand(bounds, obstacle.width * 0.5)));
    }
    let reopened = Document::from_json(&doc.file_json().unwrap()).unwrap();
    assert_eq!(reopened.atom(n).unwrap().marks.last(), Some(&mark));
}
