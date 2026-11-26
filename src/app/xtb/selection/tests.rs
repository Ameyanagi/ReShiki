use super::*;
use crate::app::{Message, xtb};
use crate::canvas::Edit;
use reshiki::document::{Annotation, Point};

#[test]
fn selection_hint_says_what_to_click() {
    if !available() {
        return;
    }
    let (mut app, _) = App::new();
    let check = |app: &App, subtitle: &str, hint: &str| {
        assert!(app.xtb_target().is_none());
        assert_eq!(
            app.xtb_selection_hint(),
            (subtitle.to_owned(), hint.to_owned())
        );
    };
    check(
        &app,
        "No molecule",
        "Draw or add a molecule to calculate it",
    );

    let a = app.tab.doc.add_atom("C", Point::new(0., 0.));
    let b = app.tab.doc.add_atom("C", Point::new(20., 0.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    let c = app.tab.doc.add_atom("O", Point::new(100., 0.));
    check(
        &app,
        "No molecule selected",
        "No molecule selected · click one to calculate it",
    );

    app.tab.selected = vec![a, b, c];
    check(
        &app,
        "2 molecules selected",
        "2 molecules selected · click one to calculate it",
    );

    let caption = app.tab.doc.next_id();
    app.tab.doc.annotations.push(Annotation {
        id: caption,
        position: Point::default(),
        text: "Sample".into(),
        format: Default::default(),
        angle: 0.,
    });
    app.tab.selected = vec![caption];
    check(
        &app,
        "No molecule selected",
        "Select a molecule to calculate it",
    );
}

#[test]
fn failed_entry_hint_clears_when_one_molecule_is_selected() {
    if !available() {
        return;
    }
    let (mut app, _) = App::new();
    let a = app.tab.doc.add_atom("C", Point::new(0., 0.));
    let b = app.tab.doc.add_atom("O", Point::new(100., 0.));
    let select = |app: &mut App, atoms| {
        let _ = app.update(Message::Canvas(Edit::Select(atoms)));
    };
    let enter = |app: &mut App| {
        let _ = app.update(Message::Xtb(xtb::Action::Entry(xtb::Entry::OptimizeGfn2)));
    };

    select(&mut app, vec![a, b]);
    enter(&mut app);
    assert_eq!(
        app.status,
        "2 molecules selected · click one to calculate it"
    );
    assert!(!app.error);
    assert!(!app.xtb_running());
    select(&mut app, vec![a]);
    assert!(app.status.is_empty());
    assert!(!app.error);

    select(&mut app, vec![a, b]);
    enter(&mut app);
    app.status = "Drawing saved".into();
    select(&mut app, vec![b]);
    assert_eq!(app.status, "Drawing saved");
}
