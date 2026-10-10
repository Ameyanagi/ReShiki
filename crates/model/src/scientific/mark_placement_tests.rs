use super::*;
use crate::{document::Document, typography::TextStyle};

fn tool(kind: SymbolKind) -> Drawing {
    Drawing {
        kind: GraphicKind::Symbol(kind),
        style: GraphicStyle::default(),
        phase: Phase::Solid,
        flipped: false,
        attach: true,
        snap_orbitals: true,
    }
}
fn mark_bounds(doc: &Document, id: u64, index: usize) -> (Point, Point) {
    let mut atom = doc.atom(id).unwrap().clone();
    atom.marks = vec![atom.marks[index].clone()];
    mark_placement::bounds(&styled_mark_parts(&atom, &doc.drawing_style))
}
fn rectangle_gap((a, b): (Point, Point), (c, d): (Point, Point)) -> f32 {
    let dx = (c.x - b.x).max(a.x - d.x).max(0.);
    let dy = (c.y - b.y).max(a.y - d.y).max(0.);
    dx.hypot(dy)
}
fn click(doc: &mut Document, id: u64, kind: SymbolKind) {
    let at = doc.atom(id).unwrap().position;
    tool(kind).place(doc, at, at, false, 10.).unwrap();
}

#[test]
fn first_click_pair_is_closer_with_positive_visible_clearance_for_actual_fonts_and_sizes() {
    for family in ["Arial", "Times New Roman", "Courier New"] {
        for size_pt in [8., 10., 18., 30.] {
            for kind in [SymbolKind::LonePair, SymbolKind::LonePairBar] {
                let mut doc = Document::default();
                let id = doc.add_atom("O", p(0., 0.));
                let atom = doc.atom_mut(id).unwrap();
                atom.label_h = 1;
                atom.text_style = Some(TextStyle {
                    family: family.into(),
                    size_pt,
                    ..Default::default()
                });
                let chemistry = doc.atom(id).unwrap().clone();
                let labels = crate::scene::atom_label_ink_boxes(&chemistry, &doc);
                assert!(labels.len() >= 2, "OH must have visible ink");
                click(&mut doc, id, kind);
                let atom = doc.atom(id).unwrap();
                assert!(chemistry.marks.is_empty());
                let allocated_id = chemistry.mark_serial.checked_add(1).unwrap();
                assert_eq!(atom.marks.len(), 1);
                assert_eq!(atom.marks[0].id, Some(allocated_id));
                assert_eq!(atom.mark_serial, allocated_id);
                let radius = atom.marks[0].offset.distance(p(0., 0.));
                assert!(
                    radius < DEFAULT.world(size_pt),
                    "{family} {size_pt}: {radius}"
                );
                let bounds = mark_bounds(&doc, id, 0);
                for &label in &labels {
                    assert!(rectangle_gap(bounds, label) > DEFAULT.world(size_pt * 0.06));
                }
                let mut actual = atom.clone();
                actual.marks.clear();
                actual.mark_serial = chemistry.mark_serial;
                assert_eq!(actual, chemistry, "annotation does not change chemistry");
                assert_eq!(
                    Document::from_json(&doc.file_json().unwrap()).unwrap(),
                    doc.current()
                );
            }
        }
    }
}

#[test]
fn repeated_clicks_avoid_hydrogens_isotopes_charges_bonds_and_existing_marks() {
    for (element, hydrogens, charge, isotope) in [
        ("O", 1, 0, 18),
        ("N", 2, 0, 15),
        ("O", 2, 1, 18),
        ("N", 3, 1, 15),
    ] {
        for bond_style in ["plain", "wedge", "hash"] {
            let mut doc = Document::default();
            let c = doc.add_atom("C", p(0., -42.));
            let id = doc.add_atom(element, p(0., 0.));
            doc.add_bond(c, id, 1, bond_style);
            let atom = doc.atom_mut(id).unwrap();
            atom.label_h = hydrogens;
            atom.charge = charge;
            atom.isotope = isotope;
            attach(atom, SymbolKind::LonePair, p(-15., 0.)).unwrap();
            let old_mark = atom.marks[0].clone();
            let bonds = doc.bonds.clone();
            for i in 1..7 {
                let old_marks = doc.atom(id).unwrap().marks.clone();
                click(&mut doc, id, SymbolKind::LonePair);
                assert_eq!(&doc.atom(id).unwrap().marks[..i], &old_marks);
                let bounds = mark_bounds(&doc, id, i);
                for other in 0..i {
                    assert!(rectangle_gap(bounds, mark_bounds(&doc, id, other)) > 0.2);
                }
                for atom in &doc.atoms {
                    for label in crate::scene::atom_label_ink_boxes(atom, &doc) {
                        assert!(rectangle_gap(bounds, label) > 0.2);
                    }
                }
                // The top bond passes through x=0. A mark on its route must
                // be above its remote endpoint rather than on the bond ink.
                if bounds.0.x <= 0. && bounds.1.x >= 0. {
                    assert!(bounds.0.y > 0. || bounds.1.y < -42.);
                }
            }
            let atom = doc.atom(id).unwrap();
            assert_eq!(atom.marks[0], old_mark);
            assert_eq!(atom.charge, charge);
            assert_eq!(atom.label_h, hydrogens);
            assert_eq!(doc.bonds, bonds);
            doc.validate().unwrap();
        }
    }
}

#[test]
fn exact_dragged_and_saved_offsets_are_preserved_after_label_and_style_changes() {
    let mut doc = Document::default();
    let id = doc.add_atom("O", p(10., 20.));
    let start = p(10., 20.);
    let end = p(18., 5.);
    tool(SymbolKind::LonePair)
        .place(&mut doc, start, end, false, 10.)
        .unwrap();
    assert_eq!(doc.atom(id).unwrap().marks[0].offset, p(8., -15.));
    assert_eq!(doc.atom(id).unwrap().marks[0].angle, 0.);
    let saved_mark = doc.atom(id).unwrap().marks[0].clone();
    doc.atom_mut(id).unwrap().label_h = 2;
    doc.drawing_style.font_size_pt = 18.;
    let _ = crate::scene::primitives(&doc);
    assert_eq!(doc.atom(id).unwrap().marks[0], saved_mark);
    click(&mut doc, id, SymbolKind::LonePairBar);
    assert_eq!(doc.atom(id).unwrap().marks[0], saved_mark);
    let reopened = Document::from_json(&doc.file_json().unwrap()).unwrap();
    assert_eq!(
        reopened.atom(id).unwrap().marks,
        doc.atom(id).unwrap().marks
    );
}

#[test]
fn charge_and_radical_click_defaults_keep_their_existing_placement() {
    for kind in [SymbolKind::Plus, SymbolKind::Radical] {
        let mut doc = Document::default();
        let id = doc.add_atom("N", p(0., 0.));
        click(&mut doc, id, kind);
        assert!(
            doc.atom(id).unwrap().marks[0]
                .offset
                .distance(p(0., -DEFAULT.font_size()))
                < 0.001
        );
        assert_eq!(doc.atom(id).unwrap().marks[0].angle, 0.);
    }
}

#[test]
fn a_hydrogen_above_or_beside_the_owner_does_not_pull_a_new_pair_away_from_the_heteroatom() {
    use crate::atom_labels::HydrogenPosition;
    for position in [
        HydrogenPosition::Above,
        HydrogenPosition::Below,
        HydrogenPosition::Left,
        HydrogenPosition::Right,
    ] {
        for kind in [SymbolKind::LonePair, SymbolKind::LonePairBar] {
            let mut doc = Document::default();
            let id = doc.add_atom("O", p(0., 0.));
            let atom = doc.atom_mut(id).unwrap();
            atom.label_h = 1;
            atom.display.hydrogen_position = position;
            click(&mut doc, id, kind);
            assert!(
                doc.atom(id).unwrap().marks[0].offset.distance(p(0., 0.))
                    < doc.drawing_style.font_size()
            );
            let bounds = mark_bounds(&doc, id, 0);
            for label in crate::scene::atom_label_ink_boxes(doc.atom(id).unwrap(), &doc) {
                assert!(rectangle_gap(bounds, label) > DEFAULT.world(0.6));
            }
        }
    }
}
