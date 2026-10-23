use super::*;
use crate::app::{Pending, tabs};
use reshiki::document::Point;

fn edited() -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let before = app.tab.doc.clone();
    app.tab.doc.add_atom("N", Point::default());
    app.changed(before);
    app
}

/// Close the unsaved tab in front and ask in the app, as the browser does.
fn close_asking(app: &mut App) {
    let _ = app.update(Message::Tabs(tabs::Action::Close(None)));
    assert!(matches!(app.pending, Some(Pending::CloseTab(_))));
    app.prompt = Some(Prompt::Save(app.document_name()));
}

#[test]
fn save_dialog_buttons_answer_as_the_system_dialog() {
    let save = || Prompt::Save("Untitled".into());
    assert!(matches!(save().answer(Answer::Save), Message::Save));
    assert!(matches!(save().answer(Answer::DontSave), Message::Discard));
    assert!(matches!(save().answer(Answer::Cancel), Message::Cancel));
    let notice = || Prompt::Notice {
        title: "Title".into(),
        text: "Text".into(),
        then: Box::new(Message::Restore),
    };
    for answer in [Answer::Save, Answer::DontSave, Answer::Cancel] {
        assert!(matches!(notice().answer(answer), Message::Restore));
    }
}

#[test]
fn enter_saves_and_escape_cancels() {
    let key = |named| modal_key(&Key::Named(named), Modifiers::empty());
    assert!(matches!(
        key(Named::Enter),
        Some(Message::Prompt(Answer::Save))
    ));
    assert!(matches!(
        key(Named::Escape),
        Some(Message::Prompt(Answer::Cancel))
    ));
    assert!(key(Named::Tab).is_none(), "Tab moves between the buttons");
    assert!(modal_key(&Key::Named(Named::Enter), Modifiers::COMMAND).is_none());
}

#[test]
fn dont_save_closes_the_tab_and_cancel_keeps_it() {
    let mut app = edited();
    let first = app.tab.id;
    close_asking(&mut app);
    let _ = app.update(Message::Prompt(Answer::Cancel));
    assert!(app.prompt.is_none() && app.pending.is_none());
    assert_eq!(app.tab.id, first, "Cancel keeps the drawing");
    assert!(app.dirty());

    close_asking(&mut app);
    let _ = app.update(Message::Prompt(Answer::DontSave));
    assert!(app.prompt.is_none() && app.pending.is_none());
    assert_ne!(app.tab.id, first, "Don’t Save closes the tab");
    assert!(!app.unsaved());
}

#[test]
fn save_starts_saving_the_tab_it_asked_about() {
    let mut app = edited();
    close_asking(&mut app);
    let _ = app.update(Message::Prompt(Answer::Save));
    assert!(app.prompt.is_none());
    assert!(
        matches!(app.pending, Some(Pending::CloseTab(_))),
        "The tab closes once it is saved"
    );
}

#[test]
fn an_answer_without_a_dialog_does_nothing() {
    let mut app = edited();
    let id = app.tab.id;
    let _ = app.update(Message::Prompt(Answer::DontSave));
    assert_eq!(app.tab.id, id);
    assert!(app.dirty());
}

#[test]
fn the_template_warning_waits_for_ok() {
    let mut app = edited();
    app.templates.pending_warning = Some(7);
    app.prompt = Some(Prompt::Notice {
        title: "Templates saved with a warning".into(),
        text: "Warning".into(),
        then: Box::new(Message::Templates(
            crate::app::template_library::Action::WarningAcknowledged(7),
        )),
    });
    let _ = app.update(Message::Prompt(Answer::Cancel));
    assert!(app.prompt.is_none());
    assert!(app.templates.pending_warning.is_none());
}
