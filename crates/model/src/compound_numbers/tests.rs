use super::*;
use crate::{document::History, editing, transaction};

fn molecule(doc: &mut Document, x: f32, y: f32) -> Vec<u64> {
    let a = doc.add_atom("C", Point::new(x, y));
    let b = doc.add_atom("C", Point::new(x + 42., y));
    doc.add_bond(a, b, 1, "plain");
    vec![a, b]
}
fn add(doc: &mut Document, atoms: &[u64]) {
    number(doc, atoms, TextFormat::default()).unwrap();
}
fn caption<'a>(doc: &'a Document, atoms: &[u64]) -> &'a Annotation {
    let id = label(doc, atoms).unwrap().annotation;
    doc.annotations.iter().find(|a| a.id == id).unwrap()
}
fn edit(doc: &mut Document, history: &mut History, change: impl FnOnce(&mut Document)) {
    let before = doc.clone();
    change(doc);
    transaction::apply(doc, history, before, false).unwrap();
}
fn typed(doc: &mut Document, atoms: &[u64], text: &str) {
    let id = label(doc, atoms).unwrap().annotation;
    doc.annotations
        .iter_mut()
        .find(|a| a.id == id)
        .unwrap()
        .text = text.into();
}

#[test]
fn adding_is_bold_below_and_idempotent_in_one_undo() {
    let mut doc = Document::default();
    let atoms = molecule(&mut doc, 0., 0.);
    let source = doc.clone();
    let mut history = History::default();
    edit(&mut doc, &mut history, |doc| add(doc, &atoms));
    let a = caption(&doc, &atoms);
    let (lo, hi) = crate::scene::selection_bounds(&doc, &atoms).unwrap();
    assert_eq!(a.text, "1");
    assert!(a.format.style.bold && !a.format.style.formula);
    assert!(a.position.y > hi.y);
    assert!((a.position.x + a.size().0 / 2. - (lo.x + hi.x) / 2.).abs() < 0.001);
    let numbered = doc.clone();
    edit(&mut doc, &mut history, |doc| add(doc, &atoms));
    assert_eq!(doc, numbered);
    assert_eq!(history.frames(), 1);
    assert!(history.undo(&mut doc));
    assert_eq!(doc, source);
    assert!(history.redo(&mut doc));
    assert_eq!(doc, numbered);
}

#[test]
fn reading_order_is_rows_then_left_to_right_independent_of_click_or_storage_order() {
    let mut doc = Document::default();
    let bottom = molecule(&mut doc, -100., 160.);
    let right = molecule(&mut doc, 200., 15.);
    let left = molecule(&mut doc, 0., 0.);
    // A slightly taller molecule in the same row should not become another row.
    doc.atom_mut(right[1]).unwrap().position.y += 35.;
    for atoms in [&bottom, &right, &left] {
        add(&mut doc, atoms);
    }
    assert_eq!(caption(&doc, &left).text, "1");
    assert_eq!(caption(&doc, &right).text, "2");
    assert_eq!(caption(&doc, &bottom).text, "3");
    let expected: Vec<_> = doc
        .compound_numbers
        .iter()
        .map(|n| (n.annotation, n.text.clone()))
        .collect();
    doc.atoms.reverse();
    doc.compound_numbers.reverse();
    renumber(&mut doc);
    for (id, text) in expected {
        assert_eq!(
            doc.annotations.iter().find(|a| a.id == id).unwrap().text,
            text
        );
    }
}

#[test]
fn tied_positions_use_atom_ids_and_tall_molecules_do_not_chain_rows() {
    let mut doc = Document::default();
    let a = molecule(&mut doc, 0., 0.);
    let b = molecule(&mut doc, 0., 0.);
    let tall = molecule(&mut doc, 200., 0.);
    doc.atom_mut(tall[1]).unwrap().position.y = 160.;
    let lower = molecule(&mut doc, -100., 160.);
    for atoms in [&lower, &tall, &b, &a] {
        add(&mut doc, atoms);
    }
    assert_eq!(caption(&doc, &a).text, "1");
    assert_eq!(caption(&doc, &b).text, "2");
    assert_eq!(caption(&doc, &tall).text, "3");
    assert_eq!(caption(&doc, &lower).text, "4");
}

#[test]
fn moving_keeps_numbers_until_one_action_renumber_and_undo_restores_all() {
    let mut doc = Document::default();
    let a = molecule(&mut doc, 0., 0.);
    let b = molecule(&mut doc, 200., 0.);
    add(&mut doc, &a);
    add(&mut doc, &b);
    let mut history = History::default();
    edit(&mut doc, &mut history, |doc| doc.translate(&a, 400., 0.));
    assert_eq!(caption(&doc, &a).text, "1");
    let moved = doc.clone();
    edit(&mut doc, &mut history, renumber);
    assert_eq!(caption(&doc, &b).text, "1");
    assert_eq!(caption(&doc, &a).text, "2");
    history.undo(&mut doc);
    assert_eq!(doc, moved);
}

#[test]
fn edited_suffix_stays_fixed_reserves_its_family_and_survives_native_undo() {
    let mut doc = Document::default();
    let a = molecule(&mut doc, 0., 0.);
    let b = molecule(&mut doc, 200., 0.);
    let c = molecule(&mut doc, 400., 0.);
    for atoms in [&a, &b, &c] {
        add(&mut doc, atoms);
    }
    let before = doc.clone();
    let mut history = History::default();
    edit(&mut doc, &mut history, |doc| typed(doc, &a, "2a"));
    assert!(label(&doc, &a).unwrap().fixed);
    assert_ne!(caption(&doc, &b).text, "2");
    edit(&mut doc, &mut history, |doc| {
        doc.translate(&a, 600., 30.);
        doc.atom_mut(a[0]).unwrap().element = "N".into();
        renumber(doc);
    });
    assert_eq!(caption(&doc, &a).text, "2a");
    assert_eq!(caption(&doc, &b).text, "1");
    assert_eq!(caption(&doc, &c).text, "3");
    let loaded = Document::from_native_file(&doc.file_json().unwrap()).unwrap();
    assert_eq!(loaded.compound_numbers, doc.compound_numbers);
    assert_eq!(caption(&loaded, &a).text, "2a");
    history.undo(&mut doc);
    history.undo(&mut doc);
    assert_eq!(doc, before);
}

#[test]
fn movement_and_manual_offset_do_not_double_translate_or_unlink() {
    let mut doc = Document::default();
    let atoms = molecule(&mut doc, 0., 0.);
    add(&mut doc, &atoms);
    let id = label(&doc, &atoms).unwrap().annotation;
    let start = caption(&doc, &atoms).position;
    let mut history = History::default();
    edit(&mut doc, &mut history, |doc| {
        doc.translate(&atoms, 40., 10.)
    });
    assert_eq!(caption(&doc, &atoms).position, start.offset(40., 10.));
    let mut all = atoms.clone();
    all.push(id);
    edit(&mut doc, &mut history, |doc| doc.translate(&all, 5., 2.));
    assert_eq!(caption(&doc, &atoms).position, start.offset(45., 12.));
    edit(&mut doc, &mut history, |doc| doc.translate(&[id], -3., 20.));
    edit(&mut doc, &mut history, |doc| doc.translate(&atoms, 10., 0.));
    assert_eq!(caption(&doc, &atoms).position, start.offset(52., 32.));
    assert!(!label(&doc, &atoms).unwrap().fixed);
}

#[test]
fn text_width_changes_keep_center_and_user_style() {
    let mut doc = Document::default();
    let mut molecules = vec![];
    for i in 0..10 {
        let atoms = molecule(&mut doc, i as f32 * 100., 0.);
        add(&mut doc, &atoms);
        molecules.push(atoms);
    }
    let atoms = &molecules[8];
    let id = label(&doc, atoms).unwrap().annotation;
    let a = doc.annotations.iter_mut().find(|a| a.id == id).unwrap();
    a.format.style.italic = true;
    let center = a.position.x + a.size().0 / 2.;
    let mut history = History::default();
    edit(&mut doc, &mut history, |doc| {
        doc.translate(atoms, 1000., 0.);
        renumber(doc);
    });
    let a = caption(&doc, atoms);
    assert_eq!(a.text, "10");
    assert!(a.format.style.italic && a.format.style.bold);
    assert!((a.position.x + a.size().0 / 2. - center - 1000.).abs() < 0.001);
}

#[test]
fn whole_copy_gets_a_fresh_automatic_number_partial_or_caption_only_is_plain() {
    let mut doc = Document::default();
    let atoms = molecule(&mut doc, 0., 0.);
    add(&mut doc, &atoms);
    let id = label(&doc, &atoms).unwrap().annotation;
    let whole = editing::selection(&doc, &atoms);
    assert_eq!(whole.compound_numbers.len(), 1);
    let caption_only = editing::selection(&doc, &[id]);
    assert_eq!(caption_only.annotations.len(), 1);
    assert!(caption_only.compound_numbers.is_empty());
    let partial = editing::selection(&doc, &[atoms[0], id]);
    assert_eq!(partial.annotations.len(), 1);
    assert!(partial.compound_numbers.is_empty());
    let mut history = History::default();
    edit(&mut doc, &mut history, |doc| {
        assert!(!editing::append(doc, &whole, Point::new(200., 0.)).is_empty());
    });
    assert_eq!(doc.compound_numbers.len(), 2);
    assert_eq!(caption(&doc, &atoms).text, "1");
    assert_eq!(
        doc.compound_numbers
            .iter()
            .map(|n| n.text.as_str())
            .collect::<Vec<_>>(),
        ["1", "2"]
    );
    doc.validate().unwrap();
    history.undo(&mut doc);
    assert_eq!(doc.compound_numbers.len(), 1);
}

#[test]
fn fixed_copies_keep_intentional_duplicate_overrides() {
    let mut doc = Document::default();
    let atoms = molecule(&mut doc, 0., 0.);
    add(&mut doc, &atoms);
    let mut history = History::default();
    edit(&mut doc, &mut history, |doc| typed(doc, &atoms, "2a"));
    let whole = editing::selection(&doc, &atoms);
    edit(&mut doc, &mut history, |doc| {
        editing::append(doc, &whole, Point::new(200., 0.));
        renumber(doc);
    });
    assert_eq!(doc.compound_numbers.len(), 2);
    assert!(
        doc.compound_numbers
            .iter()
            .all(|n| n.fixed && n.text == "2a")
    );
}

#[test]
fn split_join_and_deletion_follow_instances_not_chemical_identity() {
    let mut doc = Document::default();
    let a = molecule(&mut doc, 0., 0.);
    let b = molecule(&mut doc, 200., 0.);
    add(&mut doc, &a);
    add(&mut doc, &b);
    let mut history = History::default();
    edit(&mut doc, &mut history, |doc| typed(doc, &b, "2a"));
    let joined_before = doc.clone();
    edit(&mut doc, &mut history, |doc| {
        doc.add_bond(a[1], b[0], 1, "plain")
    });
    assert_eq!(doc.compound_numbers.len(), 1);
    assert_eq!(doc.compound_numbers[0].text, "2a");
    assert_eq!(doc.compound_numbers[0].atoms.len(), 4);
    assert_eq!(doc.annotations.len(), 1);
    history.undo(&mut doc);
    assert_eq!(doc, joined_before);
    edit(&mut doc, &mut history, |doc| {
        doc.bonds.retain(|bond| bond.a != a[0] || bond.b != a[1])
    });
    let link = doc.compound_numbers.iter().find(|n| n.text == "1").unwrap();
    assert_eq!(link.atoms, [a[0]]);
    edit(&mut doc, &mut history, |doc| doc.delete(&b));
    assert!(!doc.annotations.iter().any(|a| a.text == "2a"));
    let id = doc.compound_numbers[0].annotation;
    edit(&mut doc, &mut history, |doc| doc.delete(&[id]));
    assert!(doc.compound_numbers.is_empty());
    assert!(!doc.atoms.is_empty());
}

#[test]
fn numbers_coexist_with_and_follow_property_captions() {
    use crate::linked_properties::{self, Property};
    let mut doc = Document::default();
    let atoms = molecule(&mut doc, 0., 0.);
    add(&mut doc, &atoms);
    let mut history = History::default();
    edit(&mut doc, &mut history, |doc| {
        linked_properties::toggle(doc, &atoms, Property::Formula, TextFormat::default(), false)
            .unwrap()
    });
    let property = doc.property_captions[0].annotation.unwrap();
    let (_, bottom) = crate::scene::selection_bounds(&doc, &[property]).unwrap();
    assert!(caption(&doc, &atoms).position.y > bottom.y);
    let start = caption(&doc, &atoms).position;
    edit(&mut doc, &mut history, |doc| {
        doc.translate(&atoms, 20., 10.)
    });
    assert_eq!(caption(&doc, &atoms).position, start.offset(20., 10.));
    assert_eq!(doc.property_captions.len(), 1);
    assert_eq!(doc.compound_numbers.len(), 1);
}

#[test]
fn format_40_keeps_visible_text_without_optional_links_and_old_files_open() {
    let mut doc = Document::default();
    let atoms = molecule(&mut doc, 0., 0.);
    let old_file = doc.file_json().unwrap();
    let old = Document::from_native_file(&old_file).unwrap();
    assert!(old.compound_numbers.is_empty());
    assert!(
        !String::from_utf8(old_file)
            .unwrap()
            .contains("compound_numbers")
    );
    add(&mut doc, &atoms);
    let file = doc.file_json().unwrap();
    let loaded = Document::from_native_file(&file).unwrap();
    assert_eq!(loaded.version, 40);
    assert_eq!(loaded.compound_numbers, doc.compound_numbers);
    // Model the format-40 reader ignoring the new optional field: no visible
    // text or typography depends on it. The exact reader also ignores unknown fields.
    let mut flat: serde_json::Value = serde_json::from_slice(&file).unwrap();
    flat.as_object_mut().unwrap().remove("compound_numbers");
    flat["future_optional_link"] = serde_json::json!({"ignored": true});
    let flattened = Document::from_native_file(&serde_json::to_vec(&flat).unwrap()).unwrap();
    assert_eq!(flattened.annotations, doc.annotations);
    assert_eq!(flattened.atoms, doc.atoms);
    assert!(flattened.compound_numbers.is_empty());
    let again = Document::from_native_file(&flattened.file_json().unwrap()).unwrap();
    assert_eq!(again.annotations, doc.annotations);
    assert!(again.compound_numbers.is_empty());
}

#[test]
fn atom_only_preview_follows_move_and_resize_without_pinning_or_renumbering() {
    let mut doc = Document::default();
    let atoms = molecule(&mut doc, 0., 0.);
    add(&mut doc, &atoms);
    let before = doc.clone();
    let start = caption(&doc, &atoms).position;
    doc.translate(&atoms, 60., 20.);
    preview_layout(&mut doc, &before);
    assert_eq!(caption(&doc, &atoms).position, start.offset(60., 20.));
    assert_eq!(doc.compound_numbers, before.compound_numbers);
    let mut committed = before.clone();
    committed.translate(&atoms, 60., 20.);
    reconcile(&mut committed, &before);
    assert_eq!(doc, committed);
    // A label explicitly included in the drag must move only once.
    let mut doc = before.clone();
    let mut all = atoms.clone();
    all.push(label(&doc, &atoms).unwrap().annotation);
    doc.translate(&all, 20., 10.);
    preview_layout(&mut doc, &before);
    assert_eq!(caption(&doc, &atoms).position, start.offset(20., 10.));
    // Transform geometry without selecting the number.
    let mut doc = before.clone();
    doc.atom_mut(atoms[1]).unwrap().position = Point::new(84., 42.);
    preview_layout(&mut doc, &before);
    assert_eq!(caption(&doc, &atoms).position, start.offset(21., 42.));
    assert_eq!(doc.compound_numbers, before.compound_numbers);
}

#[test]
fn atom_merge_remaps_removed_instances_and_retains_fixed_number() {
    let mut doc = Document::default();
    let a = molecule(&mut doc, 0., 0.);
    let b = molecule(&mut doc, 200., 0.);
    let c = molecule(&mut doc, 400., 0.);
    for atoms in [&a, &b, &c] {
        add(&mut doc, atoms);
    }
    let mut history = History::default();
    edit(&mut doc, &mut history, |doc| typed(doc, &c, "3a"));
    // Preserve an explicit caption offset relative to the winning molecule.
    let id = label(&doc, &c).unwrap().annotation;
    edit(&mut doc, &mut history, |doc| doc.translate(&[id], 7., 9.));
    let old_anchor = anchor(&doc, &c).unwrap();
    let old_position = caption(&doc, &c).position;
    let before = doc.clone();
    let (merged, _) = crate::joining::merge_atoms(&doc, &[a[0], b[0], c[0]]).unwrap();
    edit(&mut doc, &mut history, |doc| *doc = merged);
    assert_eq!(doc.compound_numbers.len(), 1);
    assert_eq!(doc.compound_numbers[0].text, "3a");
    assert_eq!(doc.compound_numbers[0].atoms.len(), 4);
    assert!(!doc.compound_numbers[0].atoms.contains(&b[0]));
    assert!(!doc.compound_numbers[0].atoms.contains(&c[0]));
    let new_anchor = anchor(&doc, &doc.compound_numbers[0].atoms).unwrap();
    let expected = old_position.offset(new_anchor.x - old_anchor.x, new_anchor.y - old_anchor.y);
    let actual = doc
        .annotations
        .iter()
        .find(|a| a.id == id)
        .unwrap()
        .position;
    assert!(
        actual.distance(expected) < 0.001,
        "{actual:?} != {expected:?}"
    );
    doc.validate().unwrap();
    history.undo(&mut doc);
    assert_eq!(doc, before);
}

#[test]
fn sharing_a_numbered_single_atom_keeps_incoming_fixed_label_offset() {
    use crate::templates::{Anchor, Connection};
    let mut doc = Document::default();
    let host = molecule(&mut doc, 0., 0.);
    let source = doc.add_atom("C", Point::new(200., 0.));
    add(&mut doc, &host);
    add(&mut doc, &[source]);
    let mut history = History::default();
    edit(&mut doc, &mut history, |doc| typed(doc, &[source], "2a"));
    let old = caption(&doc, &[source]);
    let old_anchor = anchor(&doc, &[source]).unwrap();
    let offset = Point::new(
        old.position.x + old.size().0 / 2. - old_anchor.x,
        old.position.y - old_anchor.y,
    );
    let prepared = crate::joining::Prepared::new(&doc, &[source]).unwrap();
    let (joined, _) = prepared
        .place(
            Point::default(),
            None,
            5.,
            Anchor::Atom(source),
            Connection::ShareAtom,
        )
        .unwrap();
    assert_eq!(joined.compound_numbers.len(), 1);
    let link = &joined.compound_numbers[0];
    assert!(link.fixed);
    assert_eq!(link.text, "2a");
    let at = anchor(&joined, &link.atoms).unwrap();
    let actual = joined
        .annotations
        .iter()
        .find(|a| a.id == link.annotation)
        .unwrap();
    assert!((actual.position.x + actual.size().0 / 2. - at.x - offset.x).abs() < 0.001);
    assert!((actual.position.y - at.y - offset.y).abs() < 0.001);
    joined.validate().unwrap();
}
