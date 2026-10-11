use super::*;
use crate::app::arrows;

#[test]
fn mechanism_independent_tangents_undo_redo_and_head_changes_preserve_curve() {
    use reshiki::arrows::{Head, Preset};
    let (mut app, _) = App::new();
    app.tab.doc = Document::from_native_file(include_bytes!(
        "../../../tests/fixtures/mechanism-curvature-91/before.rsk"
    ))
    .unwrap();
    app.tab.selected = vec![101];
    let original = app.tab.doc.clone();
    app.edit(Edit::ArrowHandle(101, 3, Point::new(42., -55.)));
    let departure = app.tab.doc.clone();
    assert_eq!(
        (departure.arrows[0].start, departure.arrows[0].end),
        (original.arrows[0].start, original.arrows[0].end)
    );
    app.edit(Edit::ArrowHandle(101, 4, Point::new(139., -55.)));
    let arrival = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, departure);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    let _ = app.update(Message::Redo);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, arrival);
    app.arrow_action(arrows::Action::Head(Head::Right));
    assert_eq!(app.tab.doc.arrows[0].cubic, arrival.arrows[0].cubic);
    let _ = app.update(Message::ArrowStyle(Preset::Fishhook));
    assert_eq!(app.tab.doc.arrows[0].cubic, arrival.arrows[0].cubic);
    assert_eq!(app.tab.doc.bonds, original.bonds);
    assert_eq!(app.tab.doc.arrows[1], original.arrows[1]);
    let bytes = app.tab.doc.file_json().unwrap();
    assert_eq!(
        Document::from_native_file(&bytes).unwrap(),
        app.tab.doc.current()
    );
}
