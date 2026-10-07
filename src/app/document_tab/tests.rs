use super::*;
use crate::{app::App, app::Message, canvas::Tool};

#[test]
fn a_replaced_tab_keeps_its_document_selection_view_and_undo() {
    let (mut app, _) = App::new();
    let before = app.tab.doc.clone();
    let atom = app.tab.doc.add_atom("N", Point::default());
    app.changed(before);
    app.tab.selected = vec![atom];
    app.tab.camera.zoom = 2.0;
    app.tool = Tool::Erase;

    let first = std::mem::replace(&mut app.tab, DocumentTab::new(None));
    assert!(app.tab.doc.all_ids().is_empty());
    let before = app.tab.doc.clone();
    app.tab.doc.add_atom("O", Point::default());
    app.changed(before);
    let second = std::mem::replace(&mut app.tab, first);

    assert_eq!(app.tab.selected, vec![atom]);
    assert_eq!(app.tab.camera.zoom, 2.0);
    assert_eq!(app.tool, Tool::Erase, "The tool is app-wide");
    let _ = app.update(Message::Undo);
    assert!(app.tab.doc.all_ids().is_empty(), "Undo edits its own tab");
    assert_eq!(second.doc.atoms.len(), 1);
    assert_eq!(second.doc.atoms[0].element, "O");
}
