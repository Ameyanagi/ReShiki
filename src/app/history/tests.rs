use crate::app::{App, Message};
use reshiki::document::{Annotation, Arrow, Document, History, Point};

/// Undo and Redo show the restored gap of the selected reaction condition,
/// and drop a typed value that was not applied.
#[test]
fn undo_and_redo_refresh_the_arrow_label_gap_input() {
    use crate::app::arrow_labels::Action;
    use reshiki::arrows::{ArrowStyle, Preset};
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
        text: "EtOH, rt".into(),
        format: Default::default(),
    });
    let _ = reshiki::transaction::reconcile(&mut doc, before).unwrap();
    app.tab.doc = doc;
    app.tab.history = History::default();
    app.set_selection(vec![2]);

    let _ = app.update(Message::ArrowLabelGap(Action::Input("3".into())));
    let _ = app.update(Message::ArrowLabelGap(Action::Apply));
    assert_eq!(app.tab.arrow_label_gap_input, "3");
    let _ = app.update(Message::ArrowLabelGap(Action::Input("9".into())));

    let _ = app.update(Message::Undo);
    assert!(app.tab.arrow_label_gap_input.is_empty());
    assert!(!app.tab.arrow_label_gap_edited);
    let _ = app.update(Message::ArrowLabelGap(Action::Input("9".into())));
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.arrow_label_gap_input, "3");
    assert!(!app.tab.arrow_label_gap_edited);
}
