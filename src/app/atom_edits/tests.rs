use super::*;
use crate::app::Message;
use reshiki::{
    document::Point,
    scientific::{SymbolKind, attach},
};

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
