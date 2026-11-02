use super::*;
use crate::app::Message;
use reshiki::document::{Document, Point};

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
