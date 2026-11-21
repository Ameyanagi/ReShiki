use super::*;
use crate::{chains, editing, projection};

fn owned_atom() -> (Document, u64, CellRef) {
    let mut doc = Document::default();
    let atom = doc.add_atom("C", Point::new(60., 60.));
    let mut graphic = Graphic::dragged(
        100,
        GraphicKind::Rectangle,
        Point::default(),
        Point::new(300., 200.),
        Default::default(),
        Default::default(),
        false,
    );
    graphic.kind = GraphicKind::Table;
    graphic.table = Some(Box::new(Table {
        cells: vec![
            Cell {
                id: 1,
                lo: Point::default(),
                hi: Point::new(0.5, 1.),
                members: vec![atom],
                borders: vec![],
            },
            Cell {
                id: 2,
                lo: Point::new(0.5, 0.),
                hi: Point::new(1., 1.),
                members: vec![],
                borders: vec![],
            },
        ],
    }));
    doc.graphics.push(graphic);
    doc.validate().unwrap();
    (
        doc,
        atom,
        CellRef {
            table: 100,
            cell: 1,
        },
    )
}
fn assert_owned(doc: &Document, ids: &[u64], target: CellRef) {
    doc.validate().unwrap();
    let membership = Membership::new(doc).unwrap();
    assert!(ids.iter().all(|id| membership.cell_of(*id) == Some(target)));
}

#[test]
fn mechanism_arrows_inherit_common_cell_before_creation_validation() {
    use crate::arrow_anchors::{self, Pick};
    use crate::arrows::{ArrowStyle, Preset};
    for kind in 0..3 {
        for reverse in [false, true] {
            let (mut doc, a, cell) = owned_atom();
            let b = doc.add_atom("C", Point::new(120., 60.));
            let c = doc.add_atom("N", Point::new(110., 140.));
            doc.add_bond(a, b, 1, "plain");
            doc.graphics[0].table.as_mut().unwrap().cells[0]
                .members
                .extend([b, c]);
            crate::scientific::attach(
                doc.atom_mut(a).unwrap(),
                crate::scientific::SymbolKind::LonePair,
                Point::new(0., -25.),
            )
            .unwrap();
            doc.atom_mut(a).unwrap().marks[0].id = None;
            doc.atom_mut(a).unwrap().mark_serial = 0;
            let target = match kind {
                0 => Pick::Atom(a),
                1 => Pick::Bond {
                    a,
                    b,
                    fraction: 0.5,
                },
                _ => Pick::LonePair {
                    atom: a,
                    mark: doc.atom(a).unwrap().marks[0].clone(),
                },
            };
            let other = Pick::Atom(c);
            let (source, destination) = if reverse {
                (&other, &target)
            } else {
                (&target, &other)
            };
            doc.validate().unwrap();
            let arrow = arrow_anchors::create(
                &mut doc,
                source,
                destination,
                Preset::Curved,
                ArrowStyle::preset(Preset::Curved),
            )
            .unwrap();
            assert_owned(&doc, &[a, b, c, arrow], cell);
            let made = &doc.arrows[0];
            assert!(made.start_anchor.is_some() && made.end_anchor.is_some());
            if kind == 2 {
                assert!(doc.atom(a).unwrap().marks[0].id.is_some());
            }
            let reopened = Document::from_native_file(&doc.file_json().unwrap()).unwrap();
            assert_owned(&reopened, &[a, b, c, arrow], cell);
            assert_eq!(
                reopened.arrows, doc.arrows,
                "anchors and geometry stay exact"
            );
            // Native saves raise the fixture's in-memory version to the
            // feature floor required by its table, without changing its ink.
            assert_eq!(reopened, doc.current());
        }
    }
}

#[test]
fn mechanism_arrows_refuse_cross_cell_targets_without_allocating_arrow_or_mark_ids() {
    use crate::arrow_anchors::{self, Pick};
    use crate::arrows::{ArrowStyle, Preset};
    for other_cell in [false, true] {
        let (mut doc, a, _) = owned_atom();
        let b = doc.add_atom("O", Point::new(240., 60.));
        if other_cell {
            doc.graphics[0].table.as_mut().unwrap().cells[1]
                .members
                .push(b);
        }
        crate::scientific::attach(
            doc.atom_mut(a).unwrap(),
            crate::scientific::SymbolKind::LonePair,
            Point::new(0., -25.),
        )
        .unwrap();
        doc.atom_mut(a).unwrap().marks[0].id = None;
        doc.atom_mut(a).unwrap().mark_serial = 0;
        let source = Pick::LonePair {
            atom: a,
            mark: doc.atom(a).unwrap().marks[0].clone(),
        };
        doc.validate().unwrap();
        let before = doc.clone();
        let error = arrow_anchors::create(
            &mut doc,
            &source,
            &Pick::Atom(b),
            Preset::Curved,
            ArrowStyle::preset(Preset::Curved),
        )
        .unwrap_err();
        assert!(error.contains("same table cell"), "{error}");
        assert_eq!(
            doc, before,
            "the arrow, cell members and legacy mark stay unchanged"
        );
    }
}

#[test]
fn converting_cell_captions_transfers_every_replacement_atom_before_deletion() {
    for label in ["N", "Et", "NC"] {
        let (mut doc, atom, target) = owned_atom();
        let text = doc.next_id();
        doc.annotations.push(crate::document::Annotation {
            id: text,
            position: Point::new(90., 70.),
            text: label.into(),
            angle: 0.,
            format: Default::default(),
        });
        doc.graphics[0].table.as_mut().unwrap().cells[0]
            .members
            .push(text);
        let before = doc.clone();
        let (converted, anchor) =
            crate::atom_text::text_to_atom_toward(&doc, text, Some(Point::new(120., 70.))).unwrap();
        assert_eq!(doc, before);
        assert!(converted.annotations.iter().all(|a| a.id != text));
        let added: Vec<_> = converted
            .atoms
            .iter()
            .filter(|a| a.id != atom)
            .map(|a| a.id)
            .collect();
        assert!(added.contains(&anchor));
        assert_owned(&converted, &added, target);
        let (joined, _) = editing::add_bonded_atom(
            &converted,
            anchor,
            converted.atom(atom).unwrap().position,
            Some(atom),
            "C",
        )
        .unwrap();
        joined.validate().unwrap();
    }
}

#[test]
fn typed_arrow_labels_inherit_cells_but_existing_cross_cell_text_does_not() {
    use crate::document::{Annotation, Arrow};
    for existing in [false, true] {
        for other_cell in [false, true] {
            let (mut doc, _, target) = owned_atom();
            let arrow = doc.next_id();
            doc.arrows.push(Arrow::new(
                arrow,
                Point::new(20., 130.),
                Point::new(130., 130.),
                Default::default(),
                Default::default(),
            ));
            doc.graphics[0].table.as_mut().unwrap().cells[0]
                .members
                .push(arrow);
            let text = doc.next_id();
            let mut caption = Annotation {
                id: text,
                position: Point::new(60., 20.),
                text: "heat".into(),
                angle: 0.,
                format: Default::default(),
            };
            if existing || other_cell {
                doc.annotations.push(caption.clone());
            }
            if other_cell {
                doc.graphics[0].table.as_mut().unwrap().cells[1]
                    .members
                    .push(text);
            }
            let before = doc.clone();
            let (w, h) = caption.size();
            caption.position = Point::new(75. - w * 0.5, 118. - h * 0.5);
            if existing || other_cell {
                *doc.annotations.iter_mut().find(|a| a.id == text).unwrap() = caption;
            } else {
                doc.annotations.push(caption);
            }
            crate::transaction::reconcile(&mut doc, before).unwrap();
            doc.validate().unwrap();
            if !existing && !other_cell {
                assert_eq!(doc.arrow_labels[0].annotation, text);
                assert_owned(&doc, &[arrow, text], target);
            } else {
                assert!(
                    doc.arrow_labels.is_empty(),
                    "existing text never changes cells automatically"
                );
                assert_eq!(
                    Membership::new(&doc).unwrap().cell_of(text),
                    other_cell.then_some(CellRef {
                        table: 100,
                        cell: 2
                    })
                );
            }
        }
    }
}
#[test]
fn ordinary_bond_atom_chain_and_ring_growth_inherit_only_the_attachment_cell() {
    let (doc, atom, target) = owned_atom();
    let before = doc.clone();
    let (grown, end) =
        editing::add_bonded_atom(&doc, atom, Point::new(100., 60.), None, "N").unwrap();
    assert_owned(&grown, &[atom, end], target);
    assert_eq!(
        grown.atom(atom).unwrap().position,
        doc.atom(atom).unwrap().position
    );
    let (grown, ids) = chains::place(
        &doc,
        &[
            Point::new(60., 60.),
            Point::new(90., 60.),
            Point::new(105., 86.),
        ],
        Some(atom),
        None,
        2.,
    )
    .unwrap();
    assert_owned(&grown, &ids, target);
    let (grown, ids) =
        editing::ring_placement(&doc, Point::new(60., 60.), 5, false, 5., None).unwrap();
    assert_owned(&grown, &ids, target);
    assert_eq!(doc, before);
}
#[test]
fn explicit_existing_endpoints_in_different_cells_still_refuse_atom_and_chain_growth() {
    let (mut doc, atom, _) = owned_atom();
    let other = doc.add_atom("O", Point::new(240., 60.));
    doc.graphics[0].table.as_mut().unwrap().cells[1]
        .members
        .push(other);
    doc.validate().unwrap();
    let before = doc.clone();
    assert!(
        editing::add_bonded_atom(&doc, atom, Point::new(240., 60.), Some(other), "C")
            .unwrap_err()
            .contains("same table cell")
    );
    assert!(
        chains::place(
            &doc,
            &[Point::new(60., 60.), Point::new(240., 60.)],
            Some(atom),
            Some(other),
            2.
        )
        .unwrap_err()
        .contains("same table cell")
    );
    assert_eq!(doc, before);
}
#[test]
fn aromatic_plane_growth_keeps_the_substituent_in_the_ring_cell() {
    let (mut doc, _, target) = owned_atom();
    let (ring, ids) = editing::ring_placement(
        &Document::default(),
        Point::new(60., 60.),
        6,
        true,
        0.,
        None,
    )
    .unwrap();
    doc.atoms.clear();
    doc.graphics[0].table.as_mut().unwrap().cells[0]
        .members
        .clear();
    let mapped = editing::append(&mut doc, &ring, Point::default());
    relink(&mut doc, Some(target), &mapped).unwrap();
    let start = mapped[0];
    let end = projection::growth::Plane::at(&doc, start)
        .unwrap()
        .outward(42.)
        .unwrap();
    let (grown, added) =
        projection::growth::place(&doc, start, end, "C", crate::bonds::BondPreset::Single).unwrap();
    assert_owned(&grown, &[start, added], target);
    // The test ring contains real aromatic chemistry, rather than an empty frame.
    assert_eq!(ids.len(), 6);
}

#[test]
fn first_linked_name_and_formula_captions_inherit_the_molecules_cell() {
    let (mut doc, atom, target) = owned_atom();
    let identity = crate::molecule_names::identity(&doc, &[atom]).unwrap();
    let caption = crate::molecule_names::show(
        &mut doc,
        &[atom],
        "Methane".into(),
        identity,
        Default::default(),
        false,
    )
    .unwrap();
    assert_owned(&doc, &[atom, caption], target);
    crate::linked_properties::toggle(
        &mut doc,
        &[atom],
        crate::linked_properties::Property::Formula,
        Default::default(),
        false,
    )
    .unwrap();
    let formula = doc.property_captions[0].annotation;
    assert_owned(&doc, &[atom, caption, formula], target);
    let back = Document::from_json(&doc.file_json().unwrap()).unwrap();
    assert_eq!(back.version, doc.file_version());
    let mut expected = doc.clone();
    expected.version = doc.file_version();
    assert_eq!(expected, back);
}
