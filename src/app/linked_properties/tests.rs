use super::*;
use crate::app::Message;
use reshiki::document::{Document, Point};

fn ethanol(app: &mut App) -> Vec<u64> {
    app.tab.busy = false;
    app.tab.doc = Document::from_json(include_bytes!(
        "../../../tests/fixtures/chemical-naming-rust/ethanol-rust-native.rsk"
    ))
    .unwrap();
    app.tab.doc.atoms.iter().map(|a| a.id).collect()
}

fn caption(app: &App) -> Option<&str> {
    let link = app.tab.doc.property_captions.first()?;
    app.tab
        .doc
        .annotations
        .iter()
        .find(|a| a.id == link.annotation)
        .map(|a| a.text.as_str())
}

#[test]
fn pins_target_one_whole_molecule() {
    let (mut app, _) = App::new();
    let atoms = ethanol(&mut app);
    assert_eq!(app.property_target(&[]), Some(atoms.clone()));
    let sodium = app.tab.doc.add_atom("Na", Point::new(200., 0.));
    // Two molecules and no selection: no single target.
    assert_eq!(app.property_target(&[]), None);
    app.tab.selected = vec![atoms[0]];
    assert_eq!(app.property_target(&[]), Some(atoms.clone()));
    app.tab.selected = vec![atoms[0], sodium];
    assert_eq!(app.property_target(&[]), None);
    assert_eq!(app.property_target(&[sodium]), Some(vec![sodium]));
}

#[test]
fn pin_shows_and_hides_a_line_with_undo() {
    let (mut app, _) = App::new();
    let atoms = ethanol(&mut app);
    app.appearance.two_decimal_weight = true;
    let _ = app.update(Message::ToggleProperty(atoms.clone(), Property::Weight));
    assert_eq!(caption(&app), Some("Mol. Wt.: 46.07"));
    let _ = app.update(Message::ToggleProperty(atoms.clone(), Property::Formula));
    assert_eq!(caption(&app), Some("Formula: C2H6O\nMol. Wt.: 46.07"));
    let _ = app.update(Message::ToggleProperty(atoms.clone(), Property::Formula));
    let _ = app.update(Message::ToggleProperty(atoms, Property::Weight));
    assert_eq!(caption(&app), None);
    assert!(app.tab.doc.annotations.is_empty());
    let _ = app.update(Message::Undo);
    assert_eq!(caption(&app), Some("Mol. Wt.: 46.07"));
}

#[test]
fn weight_captions_follow_the_two_decimal_setting() {
    use crate::app::view_settings::Action::TwoDecimalWeight;
    let (mut app, _) = App::new();
    let atoms = ethanol(&mut app);
    app.appearance.two_decimal_weight = true;
    let _ = app.update(Message::ToggleProperty(atoms.clone(), Property::Weight));
    assert_eq!(caption(&app), Some("Mol. Wt.: 46.07"));
    // With the caption selected, its text box in Properties follows too.
    let id = app.tab.doc.property_captions[0].annotation;
    app.tab.selected = vec![id];
    app.sync_typography();
    app.view_action(TwoDecimalWeight(false));
    assert_eq!(caption(&app), Some("Mol. Wt.: 46.069"));
    assert_eq!(app.tab.caption, "Mol. Wt.: 46.069");
    // One Undo step, and the next edit follows the setting again.
    let _ = app.update(Message::Undo);
    assert_eq!(caption(&app), Some("Mol. Wt.: 46.07"));
    let before = app.tab.doc.clone();
    app.tab.doc.translate(&atoms, 10., 0.);
    app.changed(before);
    assert_eq!(caption(&app), Some("Mol. Wt.: 46.069"));
}
