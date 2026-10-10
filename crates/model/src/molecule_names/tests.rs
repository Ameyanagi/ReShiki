use super::*;
use crate::{document::History, editing, transaction};

fn ethanol() -> Document {
    Document::from_json(include_bytes!(
        "../../../../tests/fixtures/chemical-naming-rust/ethanol-rust-native.rsk"
    ))
    .unwrap()
}

fn atoms(doc: &Document) -> Vec<u64> {
    doc.atoms.iter().map(|atom| atom.id).collect()
}

fn show_ethanol(doc: &mut Document) -> u64 {
    let ids = atoms(doc);
    show(
        doc,
        &ids,
        "ethan-1-ol".into(),
        "CCO".into(),
        TextFormat::default(),
    )
    .unwrap()
}

#[test]
fn caption_is_centered_below_visual_ink_and_keeps_names_as_plain_text() {
    let mut doc = ethanol();
    let original_atoms = doc.atoms.clone();
    let original_bonds = doc.bonds.clone();
    let ids = atoms(&doc);
    let (lo, hi) = crate::scene::selection_bounds(&doc, &ids).unwrap();
    let id = show_ethanol(&mut doc);
    let caption = doc.annotations.iter().find(|a| a.id == id).unwrap();
    assert!(caption.position.y > hi.y);
    assert!((caption.position.x + caption.size().0 / 2. - (lo.x + hi.x) / 2.).abs() < 0.001);
    assert_eq!(caption.text, "ethan-1-ol");
    assert_eq!(caption.format.style.script, Script::Normal);
    assert!(!caption.format.style.formula);
    assert!(
        crate::scene::primitives(&doc)
            .iter()
            .any(|primitive| matches!(
                primitive,
                crate::scene::Primitive::Text { text, style, position, .. }
                    if text == "ethan-1-ol" && style.script == Script::Normal && position.y > hi.y
            ))
    );
    assert_eq!(doc.atoms, original_atoms);
    assert_eq!(doc.bonds, original_bonds);
}

#[test]
fn repeated_show_reuses_one_caption_and_hide_is_one_undo_step() {
    let mut doc = ethanol();
    let first = show_ethanol(&mut doc);
    let shown = doc.clone();
    assert_eq!(show_ethanol(&mut doc), first);
    assert_eq!(doc, shown);
    let before = doc.clone();
    let mut history = History::default();
    assert!(hide(&mut doc, &atoms(&before)));
    transaction::apply(&mut doc, &mut history, before.clone(), false).unwrap();
    assert!(doc.annotations.is_empty());
    assert!(doc.molecule_names.is_empty());
    let hidden = doc.clone();
    assert!(history.undo(&mut doc));
    assert_eq!(doc, before);
    assert!(history.redo(&mut doc));
    assert_eq!(doc, hidden);
}

#[test]
fn whole_molecule_copy_remaps_association_and_caption_only_copy_is_plain_text() {
    let mut source = ethanol();
    let caption = show_ethanol(&mut source);
    let selected = editing::selection(&source, &atoms(&source));
    assert_eq!(selected.annotations.len(), 1);
    assert_eq!(selected.molecule_names.len(), 1);
    let mut pasted = ethanol();
    let inserted = editing::append(&mut pasted, &selected, Point::new(240., 80.));
    assert_eq!(inserted.len(), 4);
    assert_eq!(pasted.annotations.len(), 1);
    let name = &pasted.molecule_names[0];
    assert_ne!(name.annotation, caption);
    assert!(name.atoms.iter().all(|id| inserted.contains(id)));
    assert_eq!(identity(&pasted, &name.atoms).unwrap(), "CCO");
    pasted.validate().unwrap();
    let copy = editing::selection(&source, &[caption]);
    assert!(copy.atoms.is_empty());
    assert_eq!(copy.annotations.len(), 1);
    assert!(copy.molecule_names.is_empty());
    copy.validate().unwrap();
    let partial = editing::selection(&source, &[source.atoms[0].id]);
    assert!(partial.annotations.is_empty());
    assert!(partial.molecule_names.is_empty());
}

#[test]
fn save_reopen_retains_link_and_older_drawings_need_no_metadata() {
    let mut doc = ethanol();
    assert!(doc.molecule_names.is_empty());
    assert!(
        !String::from_utf8(doc.file_json().unwrap())
            .unwrap()
            .contains("molecule_names")
    );
    show_ethanol(&mut doc);
    let bytes = doc.file_json().unwrap();
    let restored = Document::from_json(&bytes).unwrap();
    assert_eq!(restored, doc.current());
    assert_eq!(restored.molecule_names.len(), 1);
    let mut reused = restored.clone();
    show_ethanol(&mut reused);
    assert_eq!(reused.annotations.len(), 1);
    assert_eq!(reused.molecule_names.len(), 1);
}

#[test]
fn translating_molecule_follows_geometry_and_map_or_display_edits_keep_the_name() {
    let mut doc = ethanol();
    show_ethanol(&mut doc);
    let before = doc.clone();
    let at = doc.annotations[0].position;
    doc.translate(&atoms(&before), 90., 60.);
    doc.atoms[0].map_num = 47;
    doc.atoms[0].display.carbons = Some(crate::atom_labels::Carbons::All);
    doc.atoms[0].display.number = Some(crate::atom_labels::Number {
        text: "sample 9".into(),
        offset: Some(Point::new(0., 55.)),
        style: crate::atom_labels::number_style(),
    });
    let mut history = History::default();
    transaction::apply(&mut doc, &mut history, before, false).unwrap();
    assert_eq!(doc.molecule_names.len(), 1);
    assert_eq!(doc.annotations[0].text, "ethan-1-ol");
    assert!(doc.annotations[0].position.y > at.y);
    assert_eq!(identity(&doc, &atoms(&doc)).unwrap(), "CCO");
}

#[test]
fn chemical_edit_removes_obsolete_caption_atomically_and_undo_restores_it() {
    let mut doc = ethanol();
    show_ethanol(&mut doc);
    let before = doc.clone();
    doc.atoms
        .iter_mut()
        .find(|a| a.element == "O")
        .unwrap()
        .element = "N".into();
    let mut history = History::default();
    transaction::apply(&mut doc, &mut history, before.clone(), false).unwrap();
    assert!(doc.annotations.is_empty());
    assert!(doc.molecule_names.is_empty());
    let edited = doc.clone();
    assert!(history.undo(&mut doc));
    assert_eq!(doc, before);
    assert!(history.redo(&mut doc));
    assert_eq!(doc, edited);
}

#[test]
fn user_edited_or_independently_dragged_caption_detaches_and_is_preserved() {
    for edit_text in [true, false] {
        let mut doc = ethanol();
        show_ethanol(&mut doc);
        let before = doc.clone();
        if edit_text {
            doc.annotations[0].text = "My ethanol sample".into();
        } else {
            doc.annotations[0].position = doc.annotations[0].position.offset(25., 30.);
        }
        transaction::apply(&mut doc, &mut History::default(), before, false).unwrap();
        assert_eq!(doc.annotations.len(), 1);
        assert!(doc.molecule_names.is_empty());
        let user_caption = doc.annotations[0].clone();
        let before = doc.clone();
        doc.atoms
            .iter_mut()
            .find(|a| a.element == "O")
            .unwrap()
            .element = "N".into();
        transaction::apply(&mut doc, &mut History::default(), before, false).unwrap();
        assert_eq!(doc.annotations, [user_caption]);
    }
}

#[test]
fn deleting_atoms_removes_name_but_copying_a_caption_does_not_delete_it() {
    let mut doc = ethanol();
    show_ethanol(&mut doc);
    let deleted = doc.atoms[0].id;
    doc.delete(&[deleted]);
    assert!(doc.molecule_names.is_empty());
    assert!(doc.annotations.is_empty());
    doc.validate().unwrap();
}

#[test]
fn incorrect_verified_identity_cannot_install_a_caption() {
    let mut doc = ethanol();
    let before = doc.clone();
    assert!(
        show(
            &mut doc,
            &atoms(&before),
            "ethanamine".into(),
            "CCN".into(),
            TextFormat::default()
        )
        .is_err()
    );
    assert_eq!(doc, before);
}

#[test]
fn component_bond_charge_and_unknown_stereo_edits_remove_automatic_names() {
    for case in ["join", "split", "bond", "charge", "unknown"] {
        let mut doc = ethanol();
        show_ethanol(&mut doc);
        let before = doc.clone();
        match case {
            "join" => {
                let added = doc.add_atom("C", Point::new(150., 0.));
                doc.add_bond(doc.atoms[0].id, added, 1, "plain");
            }
            "split" => {
                doc.bonds.pop();
            }
            "bond" => doc.bonds[0].order = 2,
            "charge" => doc.atoms[0].charge = 1,
            "unknown" => doc.bonds[0].display = "wavy".into(),
            _ => unreachable!(),
        }
        transaction::apply(&mut doc, &mut History::default(), before, false).unwrap();
        assert!(doc.annotations.is_empty(), "{case}");
        assert!(doc.molecule_names.is_empty(), "{case}");
    }
}

#[test]
fn atom_deletion_preserves_caption_edited_in_the_same_transaction() {
    let mut doc = ethanol();
    show_ethanol(&mut doc);
    let before = doc.clone();
    doc.annotations[0].text = "Sample label".into();
    doc.delete(&[doc.atoms[0].id]);
    transaction::apply(&mut doc, &mut History::default(), before, false).unwrap();
    assert!(doc.molecule_names.is_empty());
    assert_eq!(doc.annotations[0].text, "Sample label");
}

#[test]
fn changes_to_specified_stereo_or_unknown_stereo_remove_the_old_name() {
    let mut doc = Document::from_json(include_bytes!(
        "../../../../tests/fixtures/chemical-naming-rust/r-lactic-rust-native.rsk"
    ))
    .unwrap();
    let ids = atoms(&doc);
    show(
        &mut doc,
        &ids,
        "(2R)-2-hydroxypropanoic acid".into(),
        "C[C@@H](O)C(=O)O".into(),
        TextFormat::default(),
    )
    .unwrap();
    let before = doc.clone();
    let stereo = doc
        .atoms
        .iter_mut()
        .find_map(|atom| atom.stereo.as_mut())
        .unwrap();
    stereo.winding = if stereo.winding == "cw" { "ccw" } else { "cw" }.into();
    transaction::apply(&mut doc, &mut History::default(), before, false).unwrap();
    assert!(doc.molecule_names.is_empty());
    assert!(doc.annotations.is_empty());
}
