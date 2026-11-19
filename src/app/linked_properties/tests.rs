use super::*;
use crate::app::naming::test_support::record;
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
        .find(|a| Some(a.id) == link.annotation)
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
    let id = app.tab.doc.property_captions[0].annotation.unwrap();
    app.tab.selected = vec![id];
    app.sync_typography();
    app.view_action(TwoDecimalWeight(false));
    assert_eq!(caption(&app), Some("Mol. Wt.: 46.069"));
    assert_eq!(app.tab.caption, "Mol. Wt.: 46.069");
    // One Undo step. Other edits leave the caption's precision alone.
    let _ = app.update(Message::Undo);
    assert_eq!(caption(&app), Some("Mol. Wt.: 46.07"));
    let before = app.tab.doc.clone();
    app.tab.doc.translate(&atoms, 10., 0.);
    app.changed(before);
    assert_eq!(caption(&app), Some("Mol. Wt.: 46.07"));
}

#[tokio::test]
async fn selecting_an_atom_leaves_an_imported_caption_alone() {
    use crate::app::files::{Prepared, read};
    use reshiki::engine::{LocalEngine, Request};
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/chemdraw-properties/phenol-props.cdx");
    let Some((
        _,
        Ok(Prepared::Import {
            format, contents, ..
        }),
    )) = read(path).await
    else {
        panic!("phenol-props.cdx");
    };
    let opened = LocalEngine::default()
        .request(Request::import(format, &contents))
        .await
        .unwrap()
        .document
        .unwrap();
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = opened.clone();
    app.tab.saved = opened.clone();
    // ChemDraw wrote two decimals; this setting writes three.
    assert!(!app.appearance.two_decimal_weight);
    let carbon = app.tab.doc.property_captions[0].atoms[0];
    let _ = app.update(Message::Canvas(crate::canvas::Edit::Select(vec![carbon])));
    assert_eq!(app.tab.selected, [carbon]);
    assert!(!app.tab.dirty());
    assert_eq!(
        caption(&app),
        Some(
            "Chemical formula: C6H6O\nExact mass: 94.04\nMolecular weight: 94.11\nElemental analysis: C, 76.57; H, 6.43; O, 17.00"
        )
    );
}

#[test]
fn weight_captions_in_other_tabs_follow_the_setting() {
    use crate::app::view_settings::Action::TwoDecimalWeight;
    let (mut app, _) = App::new();
    let atoms = ethanol(&mut app);
    app.appearance.two_decimal_weight = true;
    let _ = app.update(Message::ToggleProperty(atoms, Property::Weight));
    app.tab.selected = vec![app.tab.doc.property_captions[0].annotation.unwrap()];
    app.sync_typography();
    app.add_tab();
    app.inspector_open = false;
    app.view_action(TwoDecimalWeight(false));
    // The selected caption's text box follows; the front tab's inspector stays shut.
    assert!(!app.inspector_open);
    app.select_tab(0);
    assert_eq!(caption(&app), Some("Mol. Wt.: 46.069"));
    assert_eq!(app.tab.caption, "Mol. Wt.: 46.069");
    let _ = app.update(Message::Undo);
    assert_eq!(caption(&app), Some("Mol. Wt.: 46.07"));
}

fn link(app: &App) -> &linked_properties::PropertyCaption {
    &app.tab.doc.property_captions[0]
}

#[test]
fn the_automatic_name_arrives_in_the_edit_undo_step() {
    let (mut app, _) = App::new();
    let atoms = ethanol(&mut app);
    let _ = app.update(Message::ToggleProperty(atoms.clone(), Property::Name));
    // The update that showed the row started naming its structure.
    let smiles = app.tab.naming.caption_pending.clone().unwrap();
    assert_eq!(caption(&app), Some("–"));
    let frames = app.tab.history.frames();
    let revision = app.tab.revision;
    let _ = app.update(Message::Naming(Action::CaptionNamed(
        smiles,
        Box::new(Ok(Outcome::Structure(record("CCO")))),
    )));
    assert_eq!(caption(&app), Some("ethan-1-ol"));
    assert_eq!(app.tab.history.frames(), frames);
    // No new revision either, so a Cut or Paste in flight still applies.
    assert_eq!(app.tab.revision, revision);
    assert!(app.tab.naming.caption_pending.is_none());
    let _ = app.update(Message::Undo);
    assert_eq!(caption(&app), None);
    let _ = app.update(Message::Redo);
    assert_eq!(caption(&app), Some("ethan-1-ol"));
}

#[test]
fn a_structure_the_generator_refuses_is_not_asked_again() {
    let (mut app, _) = App::new();
    let atoms = ethanol(&mut app);
    let _ = app.update(Message::ToggleProperty(atoms, Property::Name));
    let smiles = app.tab.naming.caption_pending.clone().unwrap();
    let _ = app.update(Message::Naming(Action::CaptionNamed(
        smiles.clone(),
        Box::new(Err(naming::Error::Rejected("not supported".into()))),
    )));
    assert_eq!(caption(&app), Some("–"));
    assert_eq!(app.status, "No automatic name: not supported");
    assert!(app.tab.naming.caption_pending.is_none());
    assert_eq!(
        app.tab
            .naming
            .caption_failed
            .get(&smiles)
            .map(String::as_str),
        Some("not supported")
    );
    let _ = app.update(Message::Undo);
    assert!(app.tab.naming.caption_pending.is_none());
    // Showing the name again retries it.
    let atoms = app.tab.doc.atoms.iter().map(|a| a.id).collect();
    let _ = app.update(Message::ToggleProperty(atoms, Property::Name));
    assert_eq!(app.tab.naming.caption_pending, Some(smiles));
}

#[test]
fn the_name_field_and_the_canvas_edit_one_name() {
    use crate::app::inline_text;
    use iced::widget::text_editor;
    let (mut app, _) = App::new();
    let atoms = ethanol(&mut app);
    app.appearance.two_decimal_weight = true;
    let _ = app.update(Message::ToggleProperty(atoms.clone(), Property::Weight));
    // Typing in the field is a draft until Return.
    let _ = app.update(Message::Naming(Action::NameDraft(
        atoms.clone(),
        "3a".into(),
    )));
    assert_eq!(caption(&app), Some("Mol. Wt.: 46.07"));
    let _ = app.update(Message::Naming(Action::Rename(atoms.clone(), "3a".into())));
    assert_eq!(caption(&app), Some("3a\nMol. Wt.: 46.07"));
    assert!(app.tab.naming.name_draft.is_none());
    // A double-click opens the caption with the name line selected.
    let id = link(&app).annotation.unwrap();
    let _ = app.update(Message::InlineText(inline_text::Action::Begin(
        Some(id),
        Point::default(),
    )));
    assert_eq!(app.tab.caption_editor.selection().as_deref(), Some("3a"));
    let _ = app.update(Message::CaptionAction(text_editor::Action::Edit(
        text_editor::Edit::Paste(std::sync::Arc::new("Aspirin".into())),
    )));
    let _ = app.update(Message::InlineText(inline_text::Action::Finish(true)));
    // The same stored name, still linked.
    assert_eq!(caption(&app), Some("Aspirin\nMol. Wt.: 46.07"));
    assert_eq!(link(&app).custom_name, "Aspirin");
    // The typed name is bold throughout, like the name it replaced.
    let text = &app.tab.doc.annotations[0];
    assert!((0.."Aspirin".len()).all(|i| text.format.at(i).bold));
    assert!(!text.format.at("Aspirin\n".len()).bold);
    // ↺ returns to the automatic name.
    let _ = app.update(Message::Naming(Action::Rename(atoms, String::new())));
    assert_eq!(link(&app).custom_name, "");
    assert_eq!(caption(&app), Some("–\nMol. Wt.: 46.07"));
    let _ = app.update(Message::Undo);
    assert_eq!(link(&app).custom_name, "Aspirin");
    let _ = app.update(Message::Undo);
    assert_eq!(link(&app).custom_name, "3a");
}

#[test]
fn a_custom_name_stays_through_a_structure_edit_with_a_note() {
    let (mut app, _) = App::new();
    let atoms = ethanol(&mut app);
    app.appearance.two_decimal_weight = true;
    let _ = app.update(Message::Naming(Action::Rename(atoms.clone(), "3a".into())));
    assert_eq!(caption(&app), Some("3a"));
    assert!(!link(&app).renamed_structure_changed());
    let before = app.tab.doc.clone();
    let carbon = app.tab.doc.add_atom("C", Point::new(-60., 0.));
    let bonded = atoms
        .iter()
        .copied()
        .find(|id| app.tab.doc.atom(*id).is_some_and(|a| a.element == "C"))
        .unwrap();
    app.tab.doc.add_bond(bonded, carbon, 1, "plain");
    app.changed(before);
    assert_eq!(caption(&app), Some("3a"));
    assert!(link(&app).renamed_structure_changed());
    let _ = app.update(Message::Undo);
    assert!(!link(&app).renamed_structure_changed());
}

#[test]
fn undo_and_redo_reuse_a_generated_name() {
    let (mut app, _) = App::new();
    let atoms = ethanol(&mut app);
    let _ = app.update(Message::ToggleProperty(atoms, Property::Name));
    let smiles = app.tab.naming.caption_pending.clone().unwrap();
    // An unrelated edit while the generator runs.
    let before = app.tab.doc.clone();
    app.tab.doc.add_atom("Ne", Point::new(300., 0.));
    app.changed(before);
    let _ = app.update(Message::Naming(Action::CaptionNamed(
        smiles,
        Box::new(Ok(Outcome::Structure(record("CCO")))),
    )));
    assert_eq!(caption(&app), Some("ethan-1-ol"));
    // Undo brings back the drawing from before the name arrived; it is
    // named again at once, without asking the generator.
    let _ = app.update(Message::Undo);
    assert_eq!(caption(&app), Some("ethan-1-ol"));
    assert!(app.tab.naming.caption_pending.is_none());
}

#[test]
fn mass_decimals_change_one_caption_in_one_undo_step() {
    use crate::app::view_settings::Action::TwoDecimalWeight;
    let (mut app, _) = App::new();
    let atoms = ethanol(&mut app);
    app.appearance.two_decimal_weight = true;
    let _ = app.update(Message::ShowMasses(atoms.clone()));
    let masses = app.tab.doc.clone();
    // A caption whose Mol. Wt. differs from Settings keeps it when Exact
    // Mass changes.
    app.appearance.two_decimal_weight = false;
    let five = Message::PropertyPlaces(atoms.clone(), Property::ExactMass, 5);
    let _ = app.update(five);
    assert_eq!(
        caption(&app),
        Some("Formula: C2H6O\nMol. Wt.: 46.07\nExact Mass: 46.04186")
    );
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, masses);
    app.appearance.two_decimal_weight = true;
    // A caption's own Mol. Wt. places win over the Settings choice, and
    // choosing them again changes nothing.
    let one = Message::PropertyPlaces(atoms.clone(), Property::Weight, 1);
    let _ = app.update(one.clone());
    let chosen = app.tab.doc.clone();
    app.view_action(TwoDecimalWeight(false));
    assert_eq!(app.tab.doc, chosen);
    let _ = app.update(one);
    assert_eq!(app.tab.doc, chosen);
    assert_eq!(
        caption(&app),
        Some("Formula: C2H6O\nMol. Wt.: 46.1\nExact Mass: 46.0419")
    );
    // Choosing the Settings default again follows Settings from then on.
    let _ = app.update(Message::PropertyPlaces(atoms, Property::Weight, 3));
    assert_eq!(app.tab.doc.property_captions[0].weight_places, None);
    assert_eq!(
        caption(&app),
        Some("Formula: C2H6O\nMol. Wt.: 46.069\nExact Mass: 46.0419")
    );
    app.view_action(TwoDecimalWeight(true));
    assert_eq!(
        caption(&app),
        Some("Formula: C2H6O\nMol. Wt.: 46.07\nExact Mass: 46.0419")
    );
}
