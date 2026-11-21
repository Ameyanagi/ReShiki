use super::*;
use crate::{
    document::{Annotation, DRAWING_ELEMENTS, History},
    editing,
};

fn fixture() -> (Document, u64, [u64; 4]) {
    let mut doc = Document::default();
    let a = doc.add_atom("N", Point::new(125., 110.));
    let b = doc.add_atom("O", Point::new(167., 110.));
    doc.add_bond(a, b, 1, "plain");
    let mut texts = Vec::new();
    for (position, text) in [
        (Point::new(125., 150.), "A full editable caption"),
        (Point::new(290., 110.), "Reference"),
    ] {
        let id = doc.next_id();
        doc.annotations.push(Annotation {
            id,
            position,
            text: text.into(),
            format: Default::default(),
            angle: 0.,
        });
        texts.push(id);
    }
    let id = doc.next_id();
    let mut g = Graphic::dragged(
        id,
        GraphicKind::Rectangle,
        Point::new(100., 80.),
        Point::new(340., 200.),
        Default::default(),
        Default::default(),
        false,
    );
    g.kind = GraphicKind::Table;
    g.table = Some(Box::new(Table {
        cells: (0..2)
            .map(|i| Cell {
                id: i + 1,
                lo: Point::new(i as f32 * 0.5, 0.),
                hi: Point::new((i + 1) as f32 * 0.5, 1.),
                members: if i == 0 {
                    vec![a, b, texts[0]]
                } else {
                    vec![texts[1]]
                },
                borders: [Side::Left, Side::Top, Side::Bottom, Side::Right]
                    .map(|side| Border {
                        side,
                        width_pt: 0.8,
                        color: Color::Ink,
                        pattern: LinePattern::Solid,
                    })
                    .to_vec(),
            })
            .collect(),
    }));
    doc.graphics.push(g);
    doc.validate().unwrap();
    (doc, id, [a, b, texts[0], texts[1]])
}

#[test]
fn table_save_reopen_preserves_graphs_ownership_text_and_order_at_v40() {
    let (mut doc, _, _) = fixture();
    let cells = &mut doc.graphics[0].table.as_mut().unwrap().cells;
    cells[0].borders[3].width_pt = 5.;
    cells[0].borders[3].color = Color::Custom([255, 0, 0]);
    cells[1].borders[0].width_pt = 1.;
    cells[1].borders[0].color = Color::Custom([0, 0, 255]);
    cells[1].borders[3].width_pt = 0.;
    assert_eq!(doc.file_version(), DRAWING_ELEMENTS);
    let back = Document::from_json(&doc.file_json().unwrap()).unwrap();
    assert_eq!(back.graphics, doc.graphics);
    assert_eq!(back.atoms, doc.atoms);
    assert_eq!(back.bonds, doc.bonds);
    assert_eq!(back.annotations, doc.annotations);
    let parts = back.graphics[0].parts();
    assert_eq!(parts.len(), 7);
    // Native painting places each cell's vertical borders before horizontal.
    assert_eq!(parts[1].commands, parts[4].commands);
    assert_eq!(parts[1].style.width_pt, 5.);
    assert_eq!(parts[4].style.width_pt, 1.);
    assert!(!parts[1].style.round_caps() && !parts[4].style.round_caps());
    assert_eq!(parts[1].style.stroke, Color::Custom([255, 0, 0]));
    assert_eq!(parts[4].style.stroke, Color::Custom([0, 0, 255]));
    let mut seen = false;
    crate::palette::for_each_color_mut(&mut doc, |color| {
        if *color == Color::Custom([255, 0, 0]) {
            seen = true;
            *color = Color::Custom([0, 255, 0]);
        }
    });
    assert!(seen);
    assert!(crate::palette::any_color(&doc, |c| c == Color::Custom([0, 255, 0])));
}

#[test]
fn table_frames_invert_reflections_without_clamping_or_moving_content() {
    let (mut doc, table, ids) = fixture();
    let before = doc.clone();
    doc.translate(&[table, ids[0]], 32., 57.);
    assert_eq!(
        doc.atom(ids[0]).unwrap().position,
        before.atom(ids[0]).unwrap().position.offset(32., 57.)
    );
    assert_eq!(
        doc.graphics[0].origin,
        before.graphics[0].origin.offset(32., 57.)
    );
    editing::transform_about(&mut doc, &[table, ids[0]], Point::default(), 1., 90.);
    editing::transform(&mut doc, &[table], editing::Transform::FlipHorizontal);
    doc.validate().unwrap();
    let g = &doc.graphics[0];
    for cell in &g.table.as_ref().unwrap().cells {
        let frame = cell.frame(g).unwrap();
        for p in [Point::new(0.2, 0.7), Point::new(-0.5, 1.4)] {
            assert!(frame.local(frame.point(p)).unwrap().distance(p) < 1e-5);
        }
        assert!(frame.contains(frame.point(Point::new(0.5, 0.5))));
        assert!(!frame.contains(frame.point(Point::new(1.1, 0.5))));
    }
    assert_eq!(g.table, before.graphics[0].table);
    assert_eq!(doc.annotations[0].text, before.annotations[0].text);
}

#[test]
fn table_rotation_uses_its_affine_center_and_keeps_owned_coordinates() {
    let (mut doc, table, ids) = fixture();
    let frame = Frame::of(&doc.graphics[0]).unwrap();
    assert_eq!(
        editing::rotation_center(&doc, &[table]),
        Some(frame.point(Point::new(0.5, 0.5)))
    );
    let local = frame.local(doc.atom(ids[0]).unwrap().position).unwrap();
    let selected = with_contents(&doc, &[table]);
    let pivot = editing::rotation_center(&doc, &selected).unwrap();
    let ownership = doc.graphics[0].table.clone();
    for _ in 0..4 {
        editing::transform(&mut doc, &[table], editing::Transform::Rotate(90.));
        doc.validate().unwrap();
        let frame = Frame::of(&doc.graphics[0]).unwrap();
        assert!(
            editing::rotation_center(&doc, &[table])
                .unwrap()
                .distance(frame.point(Point::new(0.5, 0.5)))
                < 1e-4
        );
        assert!(
            editing::rotation_center(&doc, &selected)
                .unwrap()
                .distance(pivot)
                < 1e-4
        );
        assert!(
            frame
                .local(doc.atom(ids[0]).unwrap().position)
                .unwrap()
                .distance(local)
                < 1e-5
        );
        assert_eq!(doc.graphics[0].table, ownership);
    }
}

#[test]
fn table_validation_rejects_overlap_bad_members_and_split_chemistry_atomically() {
    let (doc, table, ids) = fixture();
    let mut invalid = doc.clone();
    invalid.graphics[0].table.as_mut().unwrap().cells[1]
        .members
        .push(ids[0]);
    assert!(invalid.validate().is_err());
    invalid = doc.clone();
    invalid.graphics[0].table.as_mut().unwrap().cells[0]
        .members
        .push(9999);
    assert!(invalid.validate().is_err());
    invalid = doc.clone();
    invalid.graphics[0].table.as_mut().unwrap().cells[1].lo.x = 0.4;
    assert!(invalid.validate().is_err());
    invalid = doc.clone();
    invalid.graphics[0].axis_y = Point::default();
    assert!(invalid.validate().is_err());
    invalid = doc.clone();
    invalid.graphics[0].table.as_mut().unwrap().cells[0].borders[0].width_pt = f32::NAN;
    assert!(invalid.validate().is_err());
    invalid = doc.clone();
    assert!(relink(&mut invalid, Some(CellRef { table, cell: 2 }), &[ids[0]]).is_err());
    assert_eq!(invalid, doc);
    assert!(relink(&mut invalid, Some(CellRef { table, cell: 2 }), &[table]).is_err());
    assert_eq!(invalid, doc);
    relink(&mut invalid, Some(CellRef { table, cell: 2 }), &ids[..3]).unwrap();
    assert_eq!(invalid.atoms, doc.atoms);
    assert_eq!(invalid.annotations, doc.annotations);
    assert_eq!(
        Membership::new(&invalid).unwrap().cell_of(ids[0]),
        Some(CellRef { table, cell: 2 })
    );
    relink(&mut invalid, None, &ids[..3]).unwrap();
    assert_eq!(Membership::new(&invalid).unwrap().cell_of(ids[0]), None);
    assert_eq!(invalid.atoms, doc.atoms);
}

#[test]
fn table_copy_paste_remaps_contents_and_child_only_copy_is_ordinary() {
    let (mut doc, table, ids) = fixture();
    let outside = doc.add_atom("C", Point::new(500., 100.));
    let copy = editing::selection(&doc, &[table]);
    copy.validate().unwrap();
    assert!(copy.atom(outside).is_none());
    assert_eq!(copy.atoms.len(), 2);
    assert_eq!(copy.annotations.len(), 2);
    let child = editing::selection(&doc, &[ids[0]]);
    child.validate().unwrap();
    assert!(child.graphics.is_empty());
    assert_eq!(child.atoms.len(), 1);
    assert!(child.bonds.is_empty());
    assert!(Membership::new(&child).unwrap().owners.is_empty());
    let source_ids: HashSet<_> = doc.object_ids().collect();
    let added = editing::append_preserving_layout(&mut doc, &copy, Point::new(40., 80.));
    assert_eq!(added.len(), copy.all_ids().len());
    assert!(added.iter().all(|id| !source_ids.contains(id)));
    doc.validate().unwrap();
    let pasted = doc
        .graphics
        .iter()
        .find(|g| g.table.is_some() && g.id != table)
        .unwrap();
    let owner = Membership::new(&doc).unwrap();
    for cell in &pasted.table.as_ref().unwrap().cells {
        for &id in &cell.members {
            assert_eq!(
                owner.cell_of(id),
                Some(CellRef {
                    table: pasted.id,
                    cell: cell.id
                })
            );
        }
    }
    assert_eq!(pasted.origin, copy.graphics[0].origin.offset(40., 80.));
}

#[test]
fn structural_table_delete_and_undo_keep_unrelated_objects_and_prune_children() {
    let (mut doc, table, ids) = fixture();
    let outside = doc.add_atom("C", Point::new(500., 100.));
    let before = doc.clone();
    doc.delete(&[table, ids[0]]);
    assert_eq!(doc.all_ids(), vec![outside]);
    doc.validate().unwrap();
    let mut history = History::default();
    assert!(history.commit(before.clone(), &doc));
    assert!(history.undo(&mut doc));
    assert_eq!(doc, before);
    doc.delete(&[ids[0]]);
    doc.validate().unwrap();
    assert!(doc.graphics[0].table.is_some());
    assert!(
        !doc.graphics[0].table.as_ref().unwrap().cells[0]
            .members
            .contains(&ids[0])
    );
    assert!(doc.atom(ids[1]).is_some());
}

#[test]
fn native_materialization_keeps_cycle_arrow_and_apparatus_outputs_in_the_cell() {
    let (mut doc, table, _) = fixture();
    let cycle = crate::cycles::insert(&mut doc, Point::new(160., 140.), 25.);
    relink(&mut doc, Some(CellRef { table, cell: 1 }), &[cycle]).unwrap();
    let (cycles, _) = crate::cycles::materialized(&doc);
    cycles.validate().unwrap();
    assert!(cycles.graphics.iter().all(|g| g.cycle.is_none()));
    assert!(
        cycles
            .arrows
            .iter()
            .all(|arrow| Membership::new(&cycles).unwrap().cell_of(arrow.id)
                == Some(CellRef { table, cell: 1 }))
    );
    let mut doc = cycles.into_owned();
    let id = doc.next_id();
    let mut style = crate::arrows::ArrowStyle::default();
    style.set_body(crate::arrows::Body::Block);
    doc.arrows.push(crate::document::Arrow::new(
        id,
        Point::new(130., 120.),
        Point::new(190., 120.),
        crate::arrows::Preset::Forward,
        style,
    ));
    relink(&mut doc, Some(CellRef { table, cell: 1 }), &[id]).unwrap();
    // Straight block arrows retain their native ChemDraw identity.
    let lowered = crate::arrows::lowered(&doc);
    lowered.validate().unwrap();
    assert_eq!(
        lowered.arrows.iter().find(|a| a.id == id),
        doc.arrows.iter().find(|a| a.id == id)
    );
    assert_eq!(
        Membership::new(&lowered).unwrap().cell_of(id),
        Some(CellRef { table, cell: 1 })
    );
    // Chevron arrows really materialize, and every output inherits the cell.
    doc.arrows
        .iter_mut()
        .find(|a| a.id == id)
        .unwrap()
        .style
        .as_mut()
        .unwrap()
        .set_body(crate::arrows::Body::Chevron);
    let original_ids: std::collections::HashSet<_> = doc.object_ids().collect();
    let lowered = crate::arrows::lowered(&doc);
    lowered.validate().unwrap();
    assert!(!lowered.arrows.iter().any(|a| a.id == id));
    let outputs: Vec<_> = lowered
        .object_ids()
        .filter(|id| !original_ids.contains(id))
        .collect();
    assert!(!outputs.is_empty());
    let ownership = Membership::new(&lowered).unwrap();
    assert!(
        outputs
            .into_iter()
            .all(|id| ownership.cell_of(id) == Some(CellRef { table, cell: 1 }))
    );
    let mut doc = lowered.into_owned();
    let flask = crate::equipment::builtin::find("reshiki/rbf-100-15").unwrap();
    let id = doc.next_id();
    doc.graphics.push(crate::equipment::placed::graphic(
        id,
        flask,
        Point::new(280., 150.),
        1.,
    ));
    relink(&mut doc, Some(CellRef { table, cell: 2 }), &[id]).unwrap();
    let materialized = crate::equipment::placed::materialized(&doc);
    materialized.validate().unwrap();
    assert!(materialized.graphics.iter().all(|g| g.piece.is_none()));
    let cells = &materialized
        .graphics
        .iter()
        .find(|g| g.id == table)
        .unwrap()
        .table
        .as_ref()
        .unwrap()
        .cells;
    assert!(cells[1].members.len() > 1);
    let ownership = Membership::new(&materialized).unwrap();
    assert!(
        cells[1]
            .members
            .iter()
            .all(|&id| ownership.cell_of(id) == Some(CellRef { table, cell: 2 }))
    );
}

#[test]
fn explicit_atom_merges_relink_within_one_cell_and_refuse_cross_cell_identity_merges() {
    let (mut doc, table, _) = fixture();
    let atoms: Vec<_> = (0..3)
        .map(|i| doc.add_atom("C", Point::new(130. + i as f32 * 5., 180.)))
        .collect();
    relink(&mut doc, Some(CellRef { table, cell: 1 }), &atoms).unwrap();
    let (merged, selected) = crate::joining::merge_atoms(&doc, &atoms).unwrap();
    merged.validate().unwrap();
    assert_eq!(selected, vec![atoms[0]]);
    assert_eq!(
        Membership::new(&merged).unwrap().cell_of(atoms[0]),
        Some(CellRef { table, cell: 1 })
    );
    assert!(merged.atom(atoms[1]).is_none());
    assert!(merged.atom(atoms[2]).is_none());
    relink(&mut doc, Some(CellRef { table, cell: 2 }), &[atoms[2]]).unwrap();
    let before = doc.clone();
    assert!(crate::joining::merge_atoms(&doc, &atoms).is_err());
    assert_eq!(doc, before);
}

#[test]
fn move_and_attach_preserves_original_cell_membership_after_temporary_id_remapping() {
    use crate::templates::{Anchor, Connection};
    for mode in [Connection::Connect, Connection::ShareAtom] {
        let (mut doc, table, _) = fixture();
        let a = doc.add_atom("C", Point::new(140., 180.));
        let b = doc.add_atom("C", Point::new(180., 180.));
        relink(&mut doc, Some(CellRef { table, cell: 1 }), &[a, b]).unwrap();
        let before = doc.clone();
        let source = crate::joining::Prepared::with_mode(&doc, &[a], mode).unwrap();
        let (joined, _) = source
            .place(Point::new(180., 180.), None, 4., Anchor::Atom(a), mode)
            .unwrap();
        joined.validate().unwrap();
        assert_eq!(
            Membership::new(&joined).unwrap().cell_of(b),
            Some(CellRef { table, cell: 1 })
        );
        if mode == Connection::Connect {
            assert_eq!(
                Membership::new(&joined).unwrap().cell_of(a),
                Some(CellRef { table, cell: 1 })
            );
            assert!(
                joined
                    .bonds
                    .iter()
                    .any(|bond| bond.a == a && bond.b == b || bond.a == b && bond.b == a)
            );
        } else {
            assert!(joined.atom(a).is_none());
        }
        assert_eq!(doc, before);
        relink(&mut doc, Some(CellRef { table, cell: 2 }), &[b]).unwrap();
        let source = crate::joining::Prepared::with_mode(&doc, &[a], mode).unwrap();
        assert!(
            source
                .place(Point::new(180., 180.), None, 4., Anchor::Atom(a), mode)
                .is_err()
        );
    }
}
