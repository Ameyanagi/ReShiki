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
