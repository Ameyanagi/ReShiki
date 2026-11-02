use super::*;
use crate::app::{Job, Message, atom_labels, same_drawing};
use reshiki::{
    document::{Document, Point},
    engine::Response,
};

fn drawing(display: &str, reversed: bool) -> Document {
    let mut document = Document::default();
    let c = document.add_atom("C", Point::default());
    let f = document.add_atom("F", Point::new(28., 0.));
    let cl = document.add_atom("Cl", Point::new(-14., -24.));
    let br = document.add_atom("Br", Point::new(-14., 24.));
    document.add_bond(
        if reversed { f } else { c },
        if reversed { c } else { f },
        1,
        display,
    );
    document.add_bond(c, cl, 1, "plain");
    document.add_bond(c, br, 1, "plain");
    document.reconcile_molecule_groups();
    document
}

fn fixture_app(document: Document) -> App {
    let (mut app, _) = App::new();
    app.tab.doc = document;
    app.tab.saved = app.tab.doc.clone();
    // Keep asynchronous label refresh pending so state checks are deterministic.
    app.tab.busy = true;
    app
}

fn checked(app: &mut App, warnings: Vec<String>, available: bool) {
    let _ = app.engine_done(
        app.tab.revision,
        Job::Analyze,
        Ok(Response {
            document: Some(app.tab.doc.clone()),
            analysis: available.then(super::super::tests::completed_analysis),
            output: None,
            engine_version: "test".into(),
            warnings,
        }),
    );
}

#[test]
fn analyze_stores_notices_selects_units_and_merges_warning_summaries() {
    let mut app = fixture_app(drawing("wedge", true));
    let original = app.tab.doc.clone();
    checked(&mut app, vec!["Engine advisory".into()], true);
    assert!(!app.error, "{}", app.status);
    assert!(
        app.tab
            .stereo_notes
            .iter()
            .any(|note| note.kind == Kind::Reversed)
    );
    assert_eq!(app.tab.selected, units(&app.tab.stereo_notes));
    assert!(
        app.status
            .starts_with("Structure checked · Engine advisory · ")
    );
    assert!(app.status.contains("1 stereo bond probably reversed"));
    assert!(!app.status.contains("No chemistry errors"));
    assert!(same_drawing(&app.tab.doc, &original));
    assert!(!app.tab.history.can_undo());
    let index = app
        .tab
        .stereo_notes
        .iter()
        .position(|note| note.kind == Kind::Reversed)
        .unwrap();
    let expected = units(std::slice::from_ref(&app.tab.stereo_notes[index]));
    app.tab.selected.clear();
    app.stereo_action(Action::Select(index));
    assert_eq!(app.tab.selected, expected);
    app.stereo_action(Action::Reverse);
    assert!(
        !stereo_lint::check(&app.tab.doc, false)
            .iter()
            .any(|note| note.kind == Kind::Reversed)
    );
    assert_ne!(
        reshiki::chemistry::document::prepare(&app.tab.doc)
            .unwrap()
            .state
            .metadata
            .atoms[0]
            .chiral_tag,
        0
    );
    assert_eq!(app.tab.history.undo_frames().count(), 1);
    app.step_history(false);
    assert_eq!(app.tab.doc, original);
}

#[test]
fn analyze_reports_clean_only_without_warnings_or_stereo_notices() {
    let mut app = fixture_app(Document::default());
    app.tab.doc.add_atom("C", Point::default());
    checked(&mut app, vec![], true);
    assert!(app.tab.stereo_notes.is_empty());
    assert_eq!(app.status, "No chemistry errors found");
    checked(&mut app, vec!["Advisory".into()], true);
    assert_eq!(app.status, "Structure checked · Advisory");
    app.tab.stereo_notes.push(Note {
        kind: Kind::Reversed,
        atoms: vec![],
        bonds: vec![],
        message: "old".into(),
    });
    checked(&mut app, vec![], false);
    assert!(app.tab.stereo_notes.is_empty());
    assert_eq!(app.status, "Chemical analysis unavailable");
}

#[test]
fn undefined_note_selects_double_bond_endpoints_and_atom_centres() {
    let mut app = fixture_app(drawing("plain", false));
    checked(&mut app, vec![], true);
    let note = app
        .tab
        .stereo_notes
        .iter()
        .find(|note| note.kind == Kind::Undefined)
        .unwrap();
    assert_eq!(note.atoms, vec![1]);
    assert!(app.status.contains("1 undefined stereocentre"));
    assert_eq!(app.tab.selected, vec![1]);
    // Selection includes both ends, even when a notice has no atom entries.
    app.tab.stereo_notes = vec![Note {
        kind: Kind::Undefined,
        atoms: vec![1],
        bonds: vec![(2, 3)],
        message: "units".into(),
    }];
    app.stereo_action(Action::Select(0));
    assert_eq!(app.tab.selected, vec![1, 2, 3]);
}

#[test]
fn note_card_hides_redraw_for_undefined_only_notices() {
    let mut app = fixture_app(Document::default());
    for (kinds, button_count) in [
        (vec![], 0),
        (vec![Kind::Undefined], 1),
        (vec![Kind::Undefined, Kind::Undefined], 2),
        (vec![Kind::NonWedgeOnly], 2),
        (vec![Kind::Undefined, Kind::NonWedgeOnly], 3),
        (vec![Kind::Reversed], 3),
    ] {
        app.tab.stereo_notes = kinds
            .iter()
            .map(|&kind| Note {
                kind,
                atoms: vec![],
                bonds: vec![],
                message: "notice".into(),
            })
            .collect();
        let view: iced::Element<'_, Message> =
            app.stereo_note_lines(iced::widget::Column::new()).into();
        assert_eq!(view.as_widget().children().len(), button_count, "{kinds:?}");
    }
}

#[test]
fn reverse_fixes_all_reversed_notes_exactly_in_one_undo_step() {
    let mut document = Document::default();
    for position in [
        Point::default(),
        Point::new(42., 0.),
        Point::new(42., 42.),
        Point::new(0., 42.),
    ] {
        document.add_atom("C", position);
    }
    document.add_bond(1, 2, 1, "wedge");
    document.add_bond(3, 4, 1, "hash");
    document.add_bond(2, 3, 1, "plain");
    document.reconcile_molecule_groups();
    let mut app = fixture_app(document);
    for bonds in [vec![(1, 2)], vec![(3, 4)]] {
        app.tab.stereo_notes.push(Note {
            kind: Kind::Reversed,
            atoms: vec![],
            bonds,
            message: "reverse".into(),
        });
    }
    app.tab.stereo_notes.push(Note {
        kind: Kind::NoEffect,
        atoms: vec![],
        bonds: vec![(2, 3)],
        message: "leave".into(),
    });
    let original = app.tab.doc.clone();
    app.stereo_action(Action::Reverse);
    assert_eq!((app.tab.doc.bonds[0].a, app.tab.doc.bonds[0].b), (2, 1));
    assert_eq!((app.tab.doc.bonds[1].a, app.tab.doc.bonds[1].b), (4, 3));
    assert_eq!(app.tab.doc.bonds[2], original.bonds[2]);
    assert_eq!(app.tab.history.undo_frames().count(), 1);
    assert!(app.tab.stereo_notes.is_empty());
    assert!(app.status.starts_with("Reversed 2 stereo bonds"));
    app.step_history(false);
    assert_eq!(app.tab.doc, original);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn notices_clear_after_position_edits_history_file_epoch_and_tab_changes() {
    let mut app = fixture_app(drawing("plain", false));
    checked(&mut app, vec![], true);
    assert!(!app.tab.stereo_notes.is_empty());
    let notes = app.tab.stereo_notes.clone();
    let before = app.tab.doc.clone();
    app.tab.doc.atoms[0].position.x += 2.;
    app.changed(before);
    assert!(app.tab.stereo_notes.is_empty());
    app.tab.stereo_notes = notes.clone();
    app.step_history(false);
    assert!(app.tab.stereo_notes.is_empty());
    app.tab.stereo_notes = notes.clone();
    app.begin_file_epoch();
    assert!(app.tab.stereo_notes.is_empty());
    app.tab.stereo_notes = notes;
    app.add_tab();
    app.select_tab(0);
    assert!(app.tab.stereo_notes.is_empty());
}

#[test]
fn redraw_is_undoable_and_reports_processed_centres() {
    let mut app = fixture_app(drawing("hollow_wedge", false));
    let original = app.tab.doc.clone();
    let configuration: Vec<_> = reshiki::chemistry::document::prepare(&original)
        .unwrap()
        .state
        .metadata
        .atoms
        .into_iter()
        .map(|atom| atom.chiral_tag)
        .collect();
    app.stereo_action(Action::Redraw);
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.status, "Redrew stereo bonds at 1 stereocentre");
    assert!(
        app.tab
            .doc
            .bonds
            .iter()
            .any(|bond| matches!(bond.display.as_str(), "wedge" | "hash"))
    );
    assert_eq!(
        reshiki::chemistry::document::prepare(&app.tab.doc)
            .unwrap()
            .state
            .metadata
            .atoms
            .into_iter()
            .map(|atom| atom.chiral_tag)
            .collect::<Vec<_>>(),
        configuration
    );
    assert_eq!(
        app.tab
            .doc
            .atoms
            .iter()
            .map(|atom| atom.position)
            .collect::<Vec<_>>(),
        original
            .atoms
            .iter()
            .map(|atom| atom.position)
            .collect::<Vec<_>>()
    );
    assert_eq!(app.tab.history.undo_frames().count(), 1);
    app.step_history(false);
    assert_eq!(app.tab.doc, original);
    let mut achiral = fixture_app(Document::default());
    achiral.stereo_action(Action::Redraw);
    assert_eq!(achiral.status, "No stereocentres to redraw");
}

#[test]
fn check_keeps_undefined_markers_and_computed_flags_do_not_dirty_the_drawing() {
    let mut app = fixture_app(drawing("plain", false));
    app.tab.doc.atom_labels.stereo = true;
    app.tab.doc.atom_labels.mark_undefined_stereocentres = true;
    app.tab.saved = app.tab.doc.clone();
    checked(&mut app, vec![], true);
    assert!(app.tab.doc.atoms[0].undefined_stereo);
    assert!(!app.dirty());
    app.tab.doc.bonds[0].undefined_stereo = true;
    assert!(!app.dirty());
    app.label_action(atom_labels::Action::MarkUndefined(false));
    assert!(!app.tab.doc.atom_labels.mark_undefined_stereocentres);
    assert!(app.tab.labels_dirty);
    // The refresh for the disabled option clears the markers.
    reshiki::atom_labels::refresh::refresh_undefined_stereo(&mut app.tab.doc);
    assert!(!app.tab.doc.atoms[0].undefined_stereo);
    app.step_history(false);
    assert!(app.tab.doc.atom_labels.mark_undefined_stereocentres);
    assert!(app.tab.doc.atoms[0].undefined_stereo);
}

#[test]
fn style_preference_is_opt_in_and_invalidates_cached_notices() {
    let mut app = fixture_app(drawing("wedge", true));
    assert!(!app.appearance.iupac_style_notes);
    checked(&mut app, vec![], true);
    assert!(!app.tab.stereo_notes.is_empty());
    app.stereo_action(Action::IupacStyleNotes(true));
    assert!(app.appearance.iupac_style_notes);
    assert!(app.tab.stereo_notes.is_empty());
}
