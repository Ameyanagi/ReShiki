use super::*;
use crate::{document::History, editing, transaction};

/// Phenol as a Kekulé ring: C6H6O.
fn phenol(doc: &mut Document, x: f32) -> Vec<u64> {
    let ring: Vec<_> = (0..6)
        .map(|i| {
            let angle = i as f32 * std::f32::consts::PI / 3.;
            doc.add_atom("C", Point::new(x + 30. * angle.cos(), 30. * angle.sin()))
        })
        .collect();
    for i in 0..6 {
        doc.add_bond(
            ring[i],
            ring[(i + 1) % 6],
            if i % 2 == 0 { 2 } else { 1 },
            "plain",
        );
    }
    let oxygen = doc.add_atom("O", Point::new(x + 60., 0.));
    doc.add_bond(ring[0], oxygen, 1, "plain");
    let mut atoms = ring;
    atoms.push(oxygen);
    atoms
}

fn shown_doc(properties: &[Property], two: bool) -> (Document, Vec<u64>) {
    let mut doc = Document::default();
    let atoms = phenol(&mut doc, 0.);
    for &p in properties {
        toggle(&mut doc, &atoms, p, TextFormat::default(), two).unwrap();
    }
    (doc, atoms)
}

fn text(doc: &Document) -> &str {
    let id = doc.property_captions[0].annotation;
    &doc.annotations.iter().find(|a| a.id == id).unwrap().text
}

/// Apply an edit the way the editor does, with undo.
fn edit(doc: &mut Document, history: &mut History, change: impl FnOnce(&mut Document)) {
    let before = doc.clone();
    change(doc);
    transaction::apply(doc, history, before, false).unwrap();
}

#[test]
fn lines_use_short_labels_in_panel_order() {
    let (doc, _) = shown_doc(
        &[
            Property::Analysis,
            Property::Tpsa,
            Property::LogP,
            Property::ExactMass,
            Property::Weight,
            Property::Formula,
        ],
        true,
    );
    assert_eq!(
        text(&doc),
        "Formula: C6H6O\nMol. Wt.: 94.11\nExact Mass: 94.0419\ncLogP: 1.39\nTPSA: 20.23\nAnal. Calcd for C6H6O: C, 76.57; H, 6.43"
    );
    // The formula shows real subscripts: its runs use the formula style.
    let caption = &doc.annotations[0];
    let formulas: Vec<_> = caption
        .format
        .spans
        .iter()
        .filter(|s| s.style.formula)
        .map(|s| &caption.text[s.start..s.end])
        .collect();
    assert_eq!(formulas, ["C6H6O", "C6H6O"]);
    assert_eq!(caption.format.alignment, TextAlign::Center);
}

#[test]
fn weight_follows_the_two_decimal_setting() {
    assert_eq!(
        text(&shown_doc(&[Property::Weight], false).0),
        "Mol. Wt.: 94.113"
    );
    assert_eq!(
        text(&shown_doc(&[Property::Weight], true).0),
        "Mol. Wt.: 94.11"
    );
}

#[test]
fn link_survives_save_and_load_and_older_files_still_open() {
    let (doc, atoms) = shown_doc(&[Property::Weight, Property::Formula], true);
    let file = doc.file_json().unwrap();
    let loaded = Document::from_json(&file).unwrap();
    assert_eq!(loaded.version, crate::document::PROPERTY_CAPTIONS);
    assert_eq!(loaded.property_captions, doc.property_captions);
    assert_eq!(loaded.property_captions[0].atoms, atoms);
    // A version 37 file has no links and opens as before.
    let mut older = Document::default();
    phenol(&mut older, 0.);
    let file = older.file_json().unwrap();
    assert!(!String::from_utf8_lossy(&file).contains("property_captions"));
    assert!(
        Document::from_json(&file)
            .unwrap()
            .property_captions
            .is_empty()
    );
}

#[test]
fn structure_edits_update_the_text_in_the_same_undoable_step() {
    let (mut doc, atoms) = shown_doc(&[Property::Formula, Property::Weight], true);
    let mut history = History::default();
    let before_edit = doc.clone();
    let mut methyl = 0;
    edit(&mut doc, &mut history, |doc| {
        methyl = doc.add_atom("C", Point::new(-60., 0.));
        doc.add_bond(atoms[3], methyl, 1, "plain");
    });
    assert_eq!(text(&doc), "Formula: C7H8O\nMol. Wt.: 108.14");
    assert!(doc.property_captions[0].atoms.contains(&methyl));
    let edited = doc.clone();
    assert!(history.undo(&mut doc));
    assert_eq!(doc, before_edit);
    assert!(history.redo(&mut doc));
    assert_eq!(doc, edited);
}

#[test]
fn caption_follows_its_molecule_unless_moved_with_it() {
    let (mut doc, atoms) = shown_doc(&[Property::Weight], true);
    let mut history = History::default();
    let start = doc.annotations[0].position;
    edit(&mut doc, &mut history, |doc| {
        doc.translate(&atoms, 40., 10.)
    });
    assert_eq!(doc.annotations[0].position, start.offset(40., 10.));
    let caption = doc.annotations[0].id;
    let mut all = atoms.clone();
    all.push(caption);
    edit(&mut doc, &mut history, |doc| doc.translate(&all, 5., 0.));
    assert_eq!(doc.annotations[0].position, start.offset(45., 10.));
    assert_eq!(doc.property_captions.len(), 1);
}

#[test]
fn editing_the_text_unlinks_it() {
    let (mut doc, atoms) = shown_doc(&[Property::Weight], true);
    let mut history = History::default();
    edit(&mut doc, &mut history, |doc| {
        doc.annotations[0].text = "Mol. Wt.: about 94".into()
    });
    assert!(doc.property_captions.is_empty());
    edit(&mut doc, &mut history, |doc| {
        let carbon = doc.add_atom("C", Point::new(-60., 0.));
        doc.add_bond(atoms[3], carbon, 1, "plain");
    });
    assert_eq!(doc.annotations[0].text, "Mol. Wt.: about 94");
}

#[test]
fn styling_keeps_the_link() {
    let (mut doc, _) = shown_doc(&[Property::Formula], true);
    let mut history = History::default();
    edit(&mut doc, &mut history, |doc| {
        let caption = &mut doc.annotations[0];
        let text = caption.text.clone();
        caption
            .format
            .apply(&text, None, &crate::typography::StyleChange::Size(14.));
    });
    assert_eq!(doc.property_captions.len(), 1);
    assert_eq!(doc.annotations[0].format.style.size_pt, 14.);
}

#[test]
fn deleting_the_structure_removes_the_caption() {
    let (mut doc, atoms) = shown_doc(&[Property::Weight], true);
    let mut history = History::default();
    edit(&mut doc, &mut history, |doc| doc.delete(&atoms));
    assert!(doc.annotations.is_empty());
    assert!(doc.property_captions.is_empty());
}

#[test]
fn a_salt_counter_ion_is_its_own_molecule() {
    let mut doc = Document::default();
    let anion = phenol(&mut doc, 0.);
    let oxygen = *anion.last().unwrap();
    doc.atom_mut(oxygen).unwrap().charge = -1;
    let sodium = doc.add_atom("Na", Point::new(120., 0.));
    doc.atom_mut(sodium).unwrap().charge = 1;
    let whole = crate::molecule_names::component(&doc, oxygen);
    assert_eq!(whole, anion);
    toggle(
        &mut doc,
        &whole,
        Property::Formula,
        TextFormat::default(),
        true,
    )
    .unwrap();
    assert_eq!(text(&doc), "Formula: C6H5O⁻");
    assert!(
        toggle(
            &mut doc,
            &[oxygen, sodium],
            Property::Weight,
            TextFormat::default(),
            true
        )
        .is_err()
    );
    toggle(
        &mut doc,
        &[sodium],
        Property::Formula,
        TextFormat::default(),
        true,
    )
    .unwrap();
    assert_eq!(doc.property_captions.len(), 2);
}

#[test]
fn hiding_the_last_property_removes_the_caption() {
    let (mut doc, atoms) = shown_doc(&[Property::Weight, Property::Tpsa], true);
    assert!(shown(&doc, &atoms, Property::Tpsa));
    toggle(
        &mut doc,
        &atoms,
        Property::Tpsa,
        TextFormat::default(),
        true,
    )
    .unwrap();
    assert_eq!(text(&doc), "Mol. Wt.: 94.11");
    assert!(!shown(&doc, &atoms, Property::Tpsa));
    toggle(
        &mut doc,
        &atoms,
        Property::Weight,
        TextFormat::default(),
        true,
    )
    .unwrap();
    assert!(doc.annotations.is_empty() && doc.property_captions.is_empty());
}

#[test]
fn copying_the_molecule_copies_the_link() {
    let (doc, atoms) = shown_doc(&[Property::Weight], true);
    let ids = crate::molecule_names::include_annotations(&doc, &atoms);
    assert_eq!(ids.len(), atoms.len() + 1);
    let copy = editing::selection(&doc, &ids);
    assert_eq!(copy.property_captions, doc.property_captions);
    // The caption alone is plain text.
    let caption = editing::selection(&doc, &[doc.annotations[0].id]);
    assert!(caption.property_captions.is_empty());
    assert_eq!(caption.annotations.len(), 1);
}

#[test]
fn joining_two_captioned_molecules_keeps_one_caption() {
    let mut doc = Document::default();
    let first = phenol(&mut doc, 0.);
    let second = phenol(&mut doc, 200.);
    for atoms in [&first, &second] {
        toggle(
            &mut doc,
            atoms,
            Property::Weight,
            TextFormat::default(),
            true,
        )
        .unwrap();
    }
    let mut history = History::default();
    edit(&mut doc, &mut history, |doc| {
        doc.add_bond(first[6], second[3], 1, "plain")
    });
    assert_eq!(doc.property_captions.len(), 1);
    assert_eq!(doc.annotations.len(), 1);
    assert_eq!(text(&doc), "Mol. Wt.: 186.21");
}

#[test]
fn merging_atoms_updates_the_caption() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(0., 0.));
    let b = doc.add_atom("C", Point::new(40., 0.));
    let c = doc.add_atom("C", Point::new(80., 0.));
    doc.add_bond(a, b, 1, "plain");
    doc.add_bond(b, c, 1, "plain");
    toggle(
        &mut doc,
        &[a, b, c],
        Property::Formula,
        TextFormat::default(),
        true,
    )
    .unwrap();
    let (merged, _) = crate::joining::merge_atoms(&doc, &[a, b, c]).unwrap();
    assert_eq!(merged.property_captions[0].atoms.len(), 1);
    assert_eq!(text(&merged), "Formula: CH4");
}

#[test]
fn merging_into_another_atom_keeps_the_caption() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(0., 0.));
    let b = doc.add_atom("C", Point::new(40., 0.));
    let c = doc.add_atom("C", Point::new(80., 0.));
    toggle(
        &mut doc,
        &[b],
        Property::Formula,
        TextFormat::default(),
        true,
    )
    .unwrap();
    let (merged, survivors) = crate::joining::merge_atoms(&doc, &[a, b, c]).unwrap();
    assert_ne!(survivors, [b]);
    assert_eq!(merged.property_captions[0].atoms, survivors);
    assert_eq!(text(&merged), "Formula: CH4");
}

#[test]
fn a_split_molecule_keeps_its_caption_on_the_largest_piece() {
    let mut doc = Document::default();
    let atoms = phenol(&mut doc, 0.);
    let methyl = doc.add_atom("C", Point::new(-60., 0.));
    doc.add_bond(atoms[3], methyl, 1, "plain");
    let mut whole = atoms.clone();
    whole.push(methyl);
    toggle(
        &mut doc,
        &whole,
        Property::Formula,
        TextFormat::default(),
        true,
    )
    .unwrap();
    let mut history = History::default();
    edit(&mut doc, &mut history, |doc| {
        doc.bonds.retain(|b| b.b != methyl);
    });
    assert_eq!(doc.property_captions.len(), 1);
    assert_eq!(doc.property_captions[0].atoms, atoms);
    assert_eq!(text(&doc), "Formula: C6H6O");
    assert!(shown(&doc, &atoms, Property::Formula));
    // The other piece gets its own caption; the first one stays.
    toggle(
        &mut doc,
        &[methyl],
        Property::Weight,
        TextFormat::default(),
        true,
    )
    .unwrap();
    assert_eq!(doc.property_captions.len(), 2);
    assert!(shown(&doc, &atoms, Property::Formula));
    assert!(shown(&doc, &[methyl], Property::Weight));
}

#[test]
fn toggle_takes_one_molecule() {
    let mut doc = Document::default();
    let first = phenol(&mut doc, 0.);
    let second = phenol(&mut doc, 200.);
    let both: Vec<_> = first.iter().chain(&second).copied().collect();
    assert!(
        toggle(
            &mut doc,
            &both,
            Property::Weight,
            TextFormat::default(),
            true
        )
        .is_err()
    );
}

#[test]
fn styling_part_of_the_caption_keeps_it() {
    let (mut doc, _) = shown_doc(&[Property::Weight], true);
    let mut history = History::default();
    let number = "Mol. Wt.: ".len().."Mol. Wt.: 94.11".len();
    edit(&mut doc, &mut history, |doc| {
        let caption = &mut doc.annotations[0];
        let text = caption.text.clone();
        caption.format.apply(
            &text,
            Some(number.clone()),
            &crate::typography::StyleChange::Bold(true),
        );
    });
    assert_eq!(doc.property_captions.len(), 1);
    let caption = &doc.annotations[0];
    assert!(caption.format.at(number.start).bold);
    assert!(!caption.format.at(0).bold);
}

#[test]
fn analysis_without_values_uses_its_caption_label() {
    let (text, _) = write(None, &[Property::Analysis], true);
    assert_eq!(text, "Anal. Calcd: –");
}
