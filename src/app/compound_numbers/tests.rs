use super::*;
use crate::app::{Message, inline_text};
use iced::widget::text_editor::{Action as Input, Edit};
use reshiki::document::{Document, Point};

fn app() -> (App, Vec<u64>, Vec<u64>) {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = Document::default();
    let a = app.tab.doc.add_atom("C", Point::new(0., 0.));
    let b = app.tab.doc.add_atom("O", Point::new(200., 0.));
    (app, vec![a], vec![b])
}

#[test]
fn normal_actions_add_once_and_renumber_in_one_undo_step() {
    let (mut app, a, b) = app();
    let before = app.tab.doc.clone();
    let _ = app.update(Message::NumberCompound(b.clone()));
    let _ = app.update(Message::NumberCompound(a.clone()));
    assert_eq!(compound_numbers::label(&app.tab.doc, &a).unwrap().text, "1");
    assert_eq!(compound_numbers::label(&app.tab.doc, &b).unwrap().text, "2");
    let numbered = app.tab.doc.clone();
    let _ = app.update(Message::NumberCompound(a.clone()));
    assert_eq!(app.tab.doc, numbered);
    let previous = app.tab.doc.clone();
    app.tab.doc.translate(&a, 400., 0.);
    app.changed(previous);
    let moved = app.tab.doc.clone();
    let commands = app.context_commands();
    assert!(commands.iter().any(|c| c.label == "Renumber" && c.enabled));
    let _ = app.update(Message::RenumberCompounds);
    assert_eq!(compound_numbers::label(&app.tab.doc, &b).unwrap().text, "1");
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, moved);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, numbered);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc.compound_numbers.len(), 1);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn ordinary_inline_text_edit_pins_override_without_custom_chemical_name() {
    let (mut app, a, b) = app();
    let _ = app.update(Message::NumberCompound(a.clone()));
    let _ = app.update(Message::NumberCompound(b));
    let original = app.tab.doc.clone();
    let id = compound_numbers::label(&app.tab.doc, &a)
        .unwrap()
        .annotation;
    let _ = app.update(Message::InlineText(inline_text::Action::Begin(
        Some(id),
        Point::new(0., 100.),
    )));
    let _ = app.update(Message::CaptionAction(Input::SelectAll));
    let _ = app.update(Message::CaptionAction(Input::Edit(Edit::Paste(
        "2a".to_owned().into(),
    ))));
    assert!(app.finish_inline(true));
    let link = compound_numbers::label(&app.tab.doc, &a).unwrap();
    assert_eq!(link.text, "2a");
    assert!(link.fixed);
    assert!(app.tab.doc.property_captions.is_empty());
    assert!(app.tab.doc.molecule_names.is_empty());
    let edited = app.tab.doc.clone();
    let _ = app.update(Message::RenumberCompounds);
    assert_eq!(
        compound_numbers::label(&app.tab.doc, &a).unwrap().text,
        "2a"
    );
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, edited);
}
