use super::*;
use crate::canvas::Edit;
use reshiki::document::{Document, Point};

fn ready() -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = Document::default();
    app.tab.doc.add_atom("C", Point::default());
    let source = app.tab.doc.add_atom("C", Point::new(180., 0.));
    let end = app.tab.doc.add_atom("C", Point::new(222., 0.));
    app.tab.doc.add_bond(source, end, 1, "plain");
    app.tab.selected = vec![source];
    app
}
#[test]
fn joining_uses_the_preview_and_is_one_undo_step() {
    let mut app = ready();
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Join(Action::Begin));
    assert_eq!(app.tab.doc, before);
    let state = app.tab.joining.as_ref().unwrap();
    let (expected, _) = state
        .prepared
        .place(
            Point::default(),
            None,
            10. / app.tab.camera.zoom,
            state.anchor,
            state.mode,
        )
        .unwrap();
    let _ = app.update(Message::Canvas(Edit::Template(Point::default(), None)));
    assert!(app.tab.joining.is_none());
    assert_eq!(app.tab.doc, expected);
    assert_eq!(app.tab.doc.bonds.len(), 2);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, expected);
}
#[test]
fn cancel_switching_tools_and_invalid_targets_keep_the_original() {
    let mut app = ready();
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Join(Action::Begin));
    let _ = app.update(Message::Canvas(Edit::Template(
        Point::new(500., 500.),
        None,
    )));
    assert!(app.tab.joining.is_some());
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Escape);
    assert!(app.tab.joining.is_none());
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    let _ = app.update(Message::Join(Action::Begin));
    let _ = app.update(Message::Tool(Tool::Bond(2)));
    assert!(app.tab.joining.is_none());
    assert_eq!(app.tool, Tool::Bond(2));
    assert_eq!(app.tab.doc, before);
}
#[test]
fn a_changed_document_cannot_be_overwritten_by_a_prepared_join() {
    let mut app = ready();
    let _ = app.update(Message::Join(Action::Begin));
    let before = app.tab.doc.clone();
    app.tab.doc.add_atom("O", Point::new(300., 100.));
    app.changed(before);
    let changed = app.tab.doc.clone();
    let _ = app.update(Message::Canvas(Edit::Template(Point::default(), None)));
    assert!(app.tab.joining.is_none());
    assert_eq!(app.tab.doc, changed);
    assert!(app.error);
}
#[test]
fn anchor_picker_switches_between_atom_and_bond_modes() {
    let mut app = ready();
    let _ = app.update(Message::Join(Action::Begin));
    let _ = app.update(Message::Join(Action::Anchor(Anchor::Bond(2, 3))));
    assert_eq!(app.tab.joining.as_ref().unwrap().mode, Connection::FuseBond);
    let _ = app.update(Message::Join(Action::Mode(Connection::ShareAtom)));
    assert!(matches!(
        app.tab.joining.as_ref().unwrap().anchor,
        Anchor::Atom(_)
    ));
    let _ = app.update(Message::Join(Action::Anchor(Anchor::Atom(3))));
    assert_eq!(app.tab.joining.as_ref().unwrap().anchor, Anchor::Atom(3));
}
