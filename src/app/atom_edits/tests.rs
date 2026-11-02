use super::*;
use crate::app::Message;
use reshiki::{
    document::{Document, Point},
    scientific::{SymbolKind, attach},
};

#[test]
fn isotope_quick_picks_apply_immediately_and_are_undoable() {
    for (element, mass_number) in [
        ("H", 2),
        ("H", 3),
        ("C", 13),
        ("C", 14),
        ("N", 15),
        ("O", 18),
        ("F", 18),
    ] {
        let (mut app, _) = App::new();
        app.tab.doc = Document::default();
        app.tab.busy = false;
        let id = app.tab.doc.add_atom(element, Point::default());
        app.tab.selected = vec![id];

        let _ = app.update(Message::SetIsotope(mass_number));
        assert!(!app.error, "{}", app.status);
        assert_eq!(app.tab.doc.atom(id).unwrap().isotope, mass_number);
        assert_eq!(app.tab.isotope, mass_number.to_string());

        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc.atom(id).unwrap().isotope, 0);
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc.atom(id).unwrap().isotope, mass_number);
    }
}

#[test]
fn unknown_isotope_applies_with_an_informational_superscript_warning() {
    let (mut app, _) = App::new();
    app.tab.doc = Document::default();
    app.tab.busy = false;
    let id = app.tab.doc.add_atom("C", Point::default());
    app.tab.selected = vec![id];
    app.tab.isotope = "2C".into();
    app.error = true;

    let _ = app.update(Message::ApplyIsotope);
    assert_eq!(app.tab.doc.atom(id).unwrap().isotope, 2);
    assert!(!app.error, "{}", app.status);
    assert!(app.status.contains("²C is not a known nuclide"));

    app.tab.isotope = "0".into();
    let _ = app.update(Message::ApplyIsotope);
    assert_eq!(app.tab.doc.atom(id).unwrap().isotope, 0);
    assert!(!app.error, "{}", app.status);
}

#[test]
fn isotope_suffix_mismatch_leaves_the_entire_mixed_selection_unchanged() {
    let (mut app, _) = App::new();
    app.tab.doc = Document::default();
    app.tab.busy = false;
    let carbon = app.tab.doc.add_atom("C", Point::default());
    let nitrogen = app.tab.doc.add_atom("N", Point::new(42., 0.));
    app.tab.selected = vec![carbon, nitrogen];
    app.tab.isotope = "¹³C".into();
    let before = app.tab.doc.clone();

    let _ = app.update(Message::ApplyIsotope);
    assert!(app.error);
    assert!(app.status.contains("selected element N"));
    assert_eq!(app.tab.doc, before);
}

#[test]
fn isotope_quick_picks_require_expanding_a_selected_abbreviation() {
    let (mut app, _) = App::new();
    app.tab.doc = Document::default();
    app.tab.busy = false;
    let id = app.tab.doc.add_atom("C", Point::default());
    app.tab.doc.contract(&[id], "Me", "Me").unwrap();
    app.tab.selected = vec![id];
    let before = app.tab.doc.clone();

    let _ = app.update(Message::SetIsotope(13));
    assert!(app.error);
    assert!(app.status.contains("Expand the selected abbreviation"));
    assert_eq!(app.tab.doc, before);
}

#[test]
fn spin_changes_apply_to_selected_diradicals_and_each_toggle_has_one_undo_step() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let a = app.tab.doc.add_atom("C", Point::default());
    let b = app.tab.doc.add_atom("C", Point::new(100., 0.));
    let outside = app.tab.doc.add_atom("C", Point::new(200., 0.));
    for atom in &mut app.tab.doc.atoms {
        atom.radical_electrons = 2;
    }
    app.tab.saved = app.tab.doc.clone();
    assert!(!app.dirty());
    app.tab.selected = vec![a, b];
    let _ = app.update(Message::AtomSinglet(true));
    assert!(app.tab.doc.atom(a).unwrap().singlet);
    assert!(app.tab.doc.atom(b).unwrap().singlet);
    assert!(!app.tab.doc.atom(outside).unwrap().singlet);
    assert!(app.dirty());
    assert_eq!(app.tab.history.undo_frames().count(), 1);
    let _ = app.update(Message::AtomSinglet(true));
    assert_eq!(app.tab.history.undo_frames().count(), 1);

    let _ = app.update(Message::AtomSinglet(false));
    assert!(app.tab.doc.atoms.iter().all(|atom| !atom.singlet));
    assert_eq!(app.tab.history.undo_frames().count(), 2);
    let _ = app.update(Message::Undo);
    assert!(app.tab.doc.atom(a).unwrap().singlet);
    assert!(app.tab.doc.atom(b).unwrap().singlet);
    let _ = app.update(Message::Undo);
    assert!(app.tab.doc.atoms.iter().all(|atom| !atom.singlet));
    assert!(!app.tab.history.can_undo());
    assert!(!app.dirty());
    let _ = app.update(Message::Redo);
    assert!(app.tab.doc.atom(a).unwrap().singlet);
    assert!(app.tab.doc.atom(b).unwrap().singlet);

    app.tab.selected = vec![outside];
    app.tab.doc.atom_mut(outside).unwrap().radical_electrons = 1;
    let frames = app.tab.history.undo_frames().count();
    let _ = app.update(Message::AtomSinglet(true));
    assert!(!app.tab.doc.atom(outside).unwrap().singlet);
    assert_eq!(app.tab.history.undo_frames().count(), frames);
}

#[test]
fn changing_radical_count_clears_singlet_except_when_retaining_two_electrons() {
    for count in [0, 1, 2] {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        let id = app.tab.doc.add_atom("C", Point::default());
        let atom = app.tab.doc.atom_mut(id).unwrap();
        atom.radical_electrons = 2;
        atom.singlet = true;
        app.tab.selected = vec![id];
        let _ = app.update(Message::AtomRadical(count));
        let atom = app.tab.doc.atom(id).unwrap();
        assert_eq!(atom.radical_electrons, count);
        assert_eq!(atom.singlet, count == 2);
        if count != 2 {
            assert_eq!(app.tab.history.undo_frames().count(), 1);
            let _ = app.update(Message::Undo);
            let atom = app.tab.doc.atom(id).unwrap();
            assert_eq!(atom.radical_electrons, 2);
            assert!(atom.singlet);
        }
    }
}

#[test]
fn removing_a_radical_mark_clears_singlet_and_undo_restores_both() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let id = app.tab.doc.add_atom("C", Point::default());
    let atom = app.tab.doc.atom_mut(id).unwrap();
    attach(atom, SymbolKind::Diradical, Point::new(0., -10.)).unwrap();
    atom.singlet = true;
    let _ = app.update(Message::RemoveMark(id, 0));
    let atom = app.tab.doc.atom(id).unwrap();
    assert_eq!(atom.radical_electrons, 0);
    assert!(!atom.singlet);
    assert!(atom.marks.is_empty());
    assert_eq!(app.tab.history.undo_frames().count(), 1);
    let _ = app.update(Message::Undo);
    let atom = app.tab.doc.atom(id).unwrap();
    assert_eq!(atom.radical_electrons, 2);
    assert!(atom.singlet);
    assert_eq!(atom.marks.len(), 1);
}
