use super::*;
use crate::canvas::guides::Unit;
use reshiki::{
    arrows::{ArrowStyle, Preset},
    document::{Annotation, Arrow, Document, History, Point},
};

fn fixture() -> App {
    let (mut app, _) = App::new();
    let mut doc = Document::default();
    doc.arrows.push(Arrow::new(
        1,
        Point::new(0., 0.),
        Point::new(160., 0.),
        Preset::Forward,
        ArrowStyle::default(),
    ));
    let before = doc.clone();
    doc.annotations.push(Annotation {
        id: 2,
        position: Point::new(20., -40.),
        text: "H2, Pd/C\nEtOH, rt".into(),
        format: Default::default(),
    });
    let _ = reshiki::transaction::reconcile(&mut doc, before).unwrap();
    assert_eq!(doc.arrow_labels.len(), 1);
    app.tab.doc = doc;
    app.tab.history = History::default();
    app.set_selection(vec![2]);
    app
}

#[test]
fn arrow_gap_control_applies_ruler_units_and_undo_redo_as_one_edit() {
    let mut app = fixture();
    let before = app.tab.doc.clone();
    let _ = app.update(Message::ArrowLabelGap(Action::Input("3".into())));
    assert_eq!(app.tab.doc, before, "typing is not a document edit");
    let _ = app.update(Message::ArrowLabelGap(Action::Apply));
    let after = app.tab.doc.clone();
    let expected = (3. / Unit::Millimetres.per_world()) as f32;
    assert!((after.arrow_labels[0].gap.unwrap() - expected).abs() < 1e-4);
    let dimension = reshiki::arrow_labels::gap_geometry(&after, 2).unwrap();
    assert!((dimension.from.distance(dimension.to) - expected).abs() < 1e-3);
    assert_eq!(app.tab.arrow_label_gap_input, "3");
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    assert!(app.tab.arrow_label_gap_input.is_empty());
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, after);
    let _ = app.update(Message::ArrowLabelGap(Action::Auto));
    assert_eq!(app.tab.doc.arrow_labels[0].gap, None);
    let native: serde_json::Value =
        serde_json::from_slice(&app.tab.doc.file_json().unwrap()).unwrap();
    assert_eq!(native["version"], 30);
    assert!(native["arrow_labels"][0].get("gap").is_none());
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, after);
}

#[test]
fn arrow_gap_invalid_input_and_unchanged_enter_keep_redo() {
    for invalid in ["-1", "NaN", "inf", "1e300", "conditions"] {
        let mut app = fixture();
        let _ = app.update(Message::ArrowLabelGap(Action::Input("2.5".into())));
        let _ = app.update(Message::ArrowLabelGap(Action::Apply));
        let after = app.tab.doc.clone();
        let _ = app.update(Message::Undo);
        let before = app.tab.doc.clone();
        let _ = app.update(Message::ArrowLabelGap(Action::Apply));
        assert_eq!(app.tab.doc, before);
        let _ = app.update(Message::ArrowLabelGap(Action::Input(invalid.into())));
        let _ = app.update(Message::ArrowLabelGap(Action::Apply));
        assert_eq!(app.tab.doc, before, "{invalid}");
        assert!(app.error);
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, after, "invalid input must retain redo");
    }
}

#[test]
fn arrow_gap_unit_change_is_display_only_and_detach_is_undoable() {
    let mut app = fixture();
    let _ = app.update(Message::ArrowLabelGap(Action::Input("4".into())));
    let _ = app.update(Message::ArrowLabelGap(Action::Apply));
    let attached = app.tab.doc.clone();
    let _ = app.update(Message::View(
        super::super::view_settings::Action::RulerUnit(Unit::Points),
    ));
    assert_eq!(app.tab.doc, attached);
    let shown: f32 = app.tab.arrow_label_gap_input.parse().unwrap();
    assert!((shown - (4. * 72. / 25.4)).abs() < 0.001);
    assert!(app.arrow_label_gap_panel().is_some());
    let _ = app.update(Message::ArrowLabelGap(Action::Detach));
    assert!(app.tab.doc.arrow_labels.is_empty());
    assert_eq!(app.tab.doc.annotations, attached.annotations);
    assert!(app.arrow_label_gap_panel().is_none());
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, attached);
}
