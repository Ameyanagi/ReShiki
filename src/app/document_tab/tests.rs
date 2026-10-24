use super::*;
use crate::{app::App, app::Message, canvas::Tool};

#[test]
fn pending_per_file_work_prevents_reuse_until_it_is_cleared() {
    use crate::app::{naming, nmr, reaction_mapping};
    use reshiki::document::Arrow;
    use reshiki::nmr::Nucleus;
    for feature in ["naming", "mapping", "NMR"] {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        assert!(app.tab.reusable());
        let task = match feature {
            "naming" => app.update(Message::Naming(naming::Action::Generate)),
            "mapping" => {
                let arrow = app.tab.doc.next_id();
                app.tab.doc.arrows.push(Arrow::new(
                    arrow,
                    Point::default(),
                    Point::new(42., 0.),
                    Default::default(),
                    Default::default(),
                ));
                app.update(Message::Mapping(reaction_mapping::Action::Auto))
            }
            "NMR" => {
                app.tab.doc.add_atom("C", Point::default());
                app.update(Message::Nmr(nmr::Action::Predict(Nucleus::H1)))
            }
            _ => unreachable!(),
        };
        assert!(task.units() > 0, "{feature}");
        // Isolate the pending-work guard from document content without invalidating its job.
        app.tab.doc = Default::default();
        app.tab.saved = Default::default();
        assert!(!app.tab.reusable(), "{feature}");
        match feature {
            "naming" => app.tab.naming.invalidate_name(),
            "mapping" => app.tab.mapping.clear_pending(),
            "NMR" => app.tab.nmr.clear_pending(),
            _ => unreachable!(),
        }
        assert!(app.tab.reusable(), "{feature}");
    }
}

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

#[test]
fn unsaved_changes_follow_save_edit_undo_and_redo() {
    let (mut app, _) = App::new();
    let mut doc = Document::default();
    doc.add_atom("C", Point::default());
    app.show_opened(doc, Some("saved.rsk".into()));
    assert!(!app.dirty());
    assert_eq!(app.title(), "saved.rsk — ReShiki");

    let before = app.tab.doc.clone();
    app.tab.doc.add_atom("O", Point::new(42., 0.));
    app.changed(before);
    assert!(app.dirty());
    assert_eq!(app.title(), "saved.rsk • — ReShiki");

    // Undoing back to the saved drawing makes it clean again.
    let _ = app.update(Message::Undo);
    assert!(!app.dirty());
    assert_eq!(app.title(), "saved.rsk — ReShiki");
    let _ = app.update(Message::Redo);
    assert!(app.dirty());

    // A save that finishes at the same revision makes it clean; Undo, dirty.
    let revision = app.tab.revision;
    let snapshot = Box::new(app.tab.doc.clone());
    let _ = app.file_saved(app.tab.file_epoch, snapshot, Ok(Some("saved.rsk".into())));
    assert_eq!(app.tab.revision, revision);
    assert!(!app.dirty());
    assert_eq!(app.title(), "saved.rsk — ReShiki");
    let _ = app.update(Message::Undo);
    assert!(app.dirty());
}
