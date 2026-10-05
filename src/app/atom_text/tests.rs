use super::*;
use crate::canvas::{Edit, Tool};
use reshiki::document::Point;

#[test]
fn accepting_an_unchanged_label_preserves_imported_atom_properties() -> Result<(), String> {
    for element in ["H", "N", "Pt"] {
        let (mut app, _) = App::new();
        let id = app.tab.doc.add_atom(element, Point::default());
        let atom = app.tab.doc.atom_mut(id).ok_or("Atom")?;
        atom.no_implicit = true;
        atom.charge = 1;
        atom.isotope = 2;
        let before = app.tab.doc.clone();
        let _ = app.update(Message::AtomText(Action::Begin(Some(id))));
        if element == "H" {
            assert_eq!(app.tab.atom_text.as_ref().ok_or("Draft")?.input, "H");
        }
        let _ = app.update(Message::AtomText(Action::Apply));
        assert_eq!(app.tab.doc, before);
    }
    Ok(())
}

#[test]
fn explicit_label_draft_and_history_retain_the_entered_count() -> Result<(), String> {
    let (mut app, _) = App::new();
    let pt = app.tab.doc.add_atom("Pt", Point::default());
    let n = app.tab.doc.add_atom("N", Point::new(42., 0.));
    app.tab.doc.add_bond(n, pt, 1, "plain");
    let _ = app.update(Message::AtomText(Action::Begin(Some(n))));
    let _ = app.update(Message::AtomText(Action::Input("NH3".into())));
    let _ = app.update(Message::AtomText(Action::Apply));
    assert_eq!(app.tab.doc.atom(n).ok_or("N")?.explicit_h, 3);
    let _ = app.update(Message::AtomText(Action::Begin(Some(n))));
    assert_eq!(app.tab.atom_text.as_ref().ok_or("Draft")?.input, "NH3");
    let _ = app.update(Message::AtomText(Action::Cancel));
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc.atom(n).ok_or("N")?.explicit_h, 0);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc.atom(n).ok_or("N")?.explicit_h, 3);
    Ok(())
}

#[test]
fn defining_and_renaming_a_group_keeps_chemistry_and_invalid_drafts() -> Result<(), String> {
    let (mut app, _) = App::new();
    // This exercises classic selection-based Enter; hybrid Enter edits
    // its explicit atom/bond hotspot instead of contracting a selection.
    let _ = app.update(Message::KeyboardDrawing(
        crate::app::keyboard_drawing::Action::Leave,
    ));
    assert!(!app.tab.keyboard_drawing.enabled());
    let n = app.tab.doc.add_atom("N", Point::default());
    let a = app.tab.doc.add_atom("C", Point::new(42., 0.));
    let b = app.tab.doc.add_atom("O", Point::new(84., 0.));
    app.tab.doc.add_bond(n, a, 1, "plain");
    app.tab.doc.add_bond(a, b, 1, "plain");
    let original = app.tab.doc.clone();
    app.tab.selected = vec![a, b];
    let _ = app.update(Message::ContextKey("Enter".into()));
    assert!(
        app.tab
            .atom_text
            .as_ref()
            .ok_or("Group draft")?
            .members
            .is_some()
    );
    let _ = app.update(Message::AtomText(Action::Input("Custom".into())));
    let _ = app.update(Message::AtomText(Action::ReverseInput("LeftCustom".into())));
    let _ = app.update(Message::AtomText(Action::Apply));
    assert!(app.tab.atom_text.is_none());
    assert_eq!(app.tab.doc.atoms, original.atoms);
    assert_eq!(app.tab.doc.bonds, original.bonds);
    assert_eq!(
        app.tab.doc.abbreviation(a).ok_or("Group")?.reverse_label,
        "LeftCustom"
    );
    let _ = app.update(Message::AtomText(Action::Begin(None)));
    let _ = app.update(Message::AtomText(Action::ReverseInput("a".repeat(33))));
    let _ = app.update(Message::AtomText(Action::Apply));
    assert!(
        app.tab
            .atom_text
            .as_ref()
            .ok_or("Invalid draft must remain")?
            .error
            .is_some()
    );
    assert_eq!(
        app.tab.doc.abbreviation(a).ok_or("Group")?.reverse_label,
        "LeftCustom"
    );
    let _ = app.update(Message::AtomText(Action::Cancel));
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc.abbreviations.len(), 1);
    Ok(())
}

#[test]
fn label_dialog_preserves_toolbar_alignment_and_chemical_graph() -> Result<(), String> {
    for label in ["Boc", "Cp*"] {
        let (mut app, _) = App::new();
        let id = app.tab.doc.add_atom("C", Point::default());
        app.tab.doc = reshiki::atom_text::apply(&app.tab.doc, id, label, Mode::Group)?;
        app.tab.selected = vec![id];
        let before = app.tab.doc.clone();
        for alignment in LabelAlignment::ALL {
            let _ = app.update(Message::GroupLabelAlign(alignment));
            let _ = app.update(Message::AtomText(Action::Begin(None)));
            let _ = app.update(Message::AtomText(Action::Apply));
            assert_eq!(
                app.tab.doc.abbreviation(id).ok_or("group")?.alignment,
                alignment
            );
            assert_eq!(app.tab.doc.atoms, before.atoms);
            assert_eq!(app.tab.doc.bonds, before.bonds);
        }
        let _ = app.update(Message::Undo);
        assert_eq!(
            app.tab.doc.abbreviation(id).ok_or("group")?.alignment,
            LabelAlignment::Right
        );
        let _ = app.update(Message::Redo);
        assert_eq!(
            app.tab.doc.abbreviation(id).ok_or("group")?.alignment,
            LabelAlignment::Above
        );
        assert_eq!(app.tab.doc.atoms, before.atoms);
    }
    Ok(())
}

#[test]
fn collapsed_haptic_groups_can_be_edited_and_undone() -> Result<(), String> {
    let (mut app, _) = App::new();
    let end = app.tab.doc.add_atom("C", Point::default());
    app.tab.doc = reshiki::atom_text::apply(&app.tab.doc, end, "Cp*", Mode::Auto)?;
    app.tab.selected = app
        .tab
        .doc
        .abbreviation(end)
        .ok_or("Missing group")?
        .members
        .clone();
    let before = app.tab.doc.clone();
    let _ = app.update(Message::AtomText(Action::Begin(None)));
    assert_eq!(
        app.tab.atom_text.as_ref().ok_or("Missing editor")?.input,
        "Cp*"
    );
    let _ = app.update(Message::AtomText(Action::Input("Cp".into())));
    let _ = app.update(Message::AtomText(Action::Apply));
    assert!(app.tab.atom_text.is_none());
    assert_eq!(
        app.tab
            .doc
            .atoms
            .iter()
            .filter(|a| a.element == "C")
            .count(),
        5
    );
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(
        app.tab
            .doc
            .atoms
            .iter()
            .filter(|a| a.element == "C")
            .count(),
        5
    );
    Ok(())
}

#[test]
fn atom_text_entry_preserves_bonds_is_undoable_and_cancel_is_nonmutating() -> Result<(), String> {
    let (mut app, _) = App::new();
    let a = app.tab.doc.add_atom("C", Point::new(0., 0.));
    let b = app.tab.doc.add_atom("N", Point::new(80., 0.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    app.tab.selected = vec![a];
    let before = app.tab.doc.clone();
    let _ = app.update(Message::AtomText(Action::Begin(None)));
    let _ = app.update(Message::AtomText(Action::Input("M".into())));
    let _ = app.update(Message::Delete);
    let _ = app.update(Message::New);
    assert_eq!(
        app.tab.doc, before,
        "Shortcuts cannot mutate the drawing behind the editor"
    );
    let _ = app.update(Message::AtomText(Action::Apply));
    assert_eq!(
        app.tab
            .doc
            .atom(a)
            .ok_or("Missing atom")?
            .display
            .variable
            .as_deref(),
        Some("M")
    );
    assert_eq!(app.tab.doc.bonds, before.bonds);
    let named = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, named);
    app.tool = Tool::Text;
    let _ = app.update(Message::Canvas(Edit::Click(Point::new(0., 0.))));
    assert!(app.tab.atom_text.is_some());
    assert!(app.tab.inline_text.is_none());
    let _ = app.update(Message::AtomText(Action::Input("L".into())));
    let _ = app.update(Message::Escape);
    assert_eq!(app.tab.doc, named);
    assert!(app.tab.atom_text.is_none());
    let _ = app.update(Message::Canvas(Edit::Click(Point::new(300., 300.))));
    assert!(
        app.tab.inline_text.is_some(),
        "Empty space still creates a caption"
    );
    Ok(())
}

#[test]
fn label_errors_and_stale_documents_retain_the_draft() -> Result<(), String> {
    let (mut app, _) = App::new();
    let a = app.tab.doc.add_atom("C", Point::default());
    app.tab.selected = vec![a];
    let _ = app.update(Message::AtomText(Action::Begin(None)));
    let before = app.tab.doc.clone();
    let _ = app.update(Message::AtomText(Action::Input("".into())));
    let _ = app.update(Message::AtomText(Action::Apply));
    assert!(
        app.tab
            .atom_text
            .as_ref()
            .ok_or("Missing draft")?
            .error
            .is_some()
    );
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::AtomText(Action::Input("X".into())));
    app.tab.file_epoch += 1;
    let _ = app.update(Message::AtomText(Action::Apply));
    assert!(
        app.tab
            .atom_text
            .as_ref()
            .ok_or("Missing draft")?
            .error
            .is_some()
    );
    assert_eq!(app.tab.doc, before);
    Ok(())
}
