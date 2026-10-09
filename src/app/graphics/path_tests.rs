use super::*;
use crate::canvas::pen::Stroke;
use path::Action as PathAction;

fn send(app: &mut App, action: PathAction) {
    let _ = app.update(Message::Graphics(Action::Path(action)));
    assert!(!app.error, "{}", app.status);
}
fn fixture_app() -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = Document::from_native_file(include_bytes!(
        "../../../tests/fixtures/tunable-pen-lines-67/before.rsk"
    ))
    .unwrap();
    app.tab.saved = app.tab.doc.clone();
    app.tab.selected = vec![app.tab.doc.graphics[0].id];
    app.sync_graphics();
    app
}

#[test]
fn pen_each_completed_segment_is_one_undo_step_and_preserves_chemistry() {
    let mut app = fixture_app();
    let chemistry = app.tab.doc.add_atom("O", Point::new(400., 0.));
    send(&mut app, PathAction::New);
    let before = app.tab.doc.clone();
    app.edit(Edit::PenSegment(Stroke {
        start: Point::new(320., 30.),
        end: Point::new(400., 50.),
        dragged: true,
        close: false,
    }));
    assert_eq!(app.tool, Tool::Graphic(GraphicKind::Path));
    assert_eq!(app.tab.doc.graphics.len(), 3);
    let id = app.tab.selected[0];
    let first = app.tab.doc.clone();
    app.edit(Edit::PenSegment(Stroke {
        start: Point::new(480., 30.),
        end: Point::new(480., 30.),
        dragged: false,
        close: false,
    }));
    assert_eq!(app.tab.selected, vec![id]);
    assert_eq!(app.tab.doc.graphics.len(), 3);
    let second = app.tab.doc.clone();
    app.edit(Edit::PenSegment(Stroke {
        start: Point::new(560., 50.),
        end: Point::new(560., 100.),
        dragged: true,
        close: false,
    }));
    let third = app.tab.doc.clone();
    for expected in [&second, &first, &before] {
        let _ = app.update(Message::Undo);
        assert_eq!(&app.tab.doc, expected);
    }
    for expected in [&first, &second, &third] {
        let _ = app.update(Message::Redo);
        assert_eq!(&app.tab.doc, expected);
    }
    assert_eq!(app.tab.doc.atom(chemistry), before.atom(chemistry));
    assert_eq!(app.tab.doc.graphics[..2], before.graphics);
    send(&mut app, PathAction::Finish);
    assert_eq!(app.tool, Tool::Select);
    assert_eq!(app.tab.doc, third);
    assert_eq!(
        Document::from_native_file(&third.file_json().unwrap()).unwrap(),
        third.current()
    );
}

#[test]
fn pen_node_actions_have_one_history_step_and_selection_does_not_edit() {
    for action in [
        PathAction::Insert,
        PathAction::Delete,
        PathAction::Straight,
        PathAction::Close,
    ] {
        let mut app = fixture_app();
        send(&mut app, PathAction::Node(3));
        let before = app.tab.doc.clone();
        assert!(!app.tab.history.can_undo());
        send(&mut app, action);
        let after = app.tab.doc.clone();
        assert_ne!(after, before);
        assert_eq!(after.graphics[1], before.graphics[1]);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, after);
    }
    let mut app = fixture_app();
    let id = app.tab.selected[0];
    let before = app.tab.doc.clone();
    let point = before.graphics[0].edit_points()[3];
    app.edit(Edit::GraphicPoint(id, 3, point));
    assert_eq!(app.tab.path_point, Some((id, 3)));
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn pen_point_selection_is_ephemeral_and_kept_per_document_tab() {
    let mut app = fixture_app();
    let first = app.tab.id;
    let id = app.tab.selected[0];
    send(&mut app, PathAction::Node(3));
    let _ = app.update(Message::New);
    assert_ne!(app.tab.id, first);
    assert_eq!(app.tab.path_point, None);
    let _ = app.update(Message::Tabs(super::super::tabs::Action::Select(first)));
    assert_eq!(app.tab.path_point, Some((id, 3)));
    let native = String::from_utf8(app.tab.doc.file_json().unwrap()).unwrap();
    assert!(!native.contains("path_point"));
}

#[test]
fn pen_edge_on_point_drag_explains_rejection_without_edit_or_history() {
    let mut app = fixture_app();
    app.tab.doc.graphics[0].axis_y = Point::default();
    app.tab.doc.graphics[0].depth = [10., 0., 1.];
    let before = app.tab.doc.clone();
    let id = app.tab.selected[0];
    let point = before.graphics[0].edit_points()[3].offset(10., 10.);
    app.edit(Edit::GraphicPoint(id, 3, point));
    assert!(app.error);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    let explanation = app.status.clone();
    let _ = app.update(Message::Graphics(Action::Path(PathAction::Node(3))));
    let _ = app.update(Message::Graphics(Action::Path(PathAction::Insert)));
    assert!(app.error);
    assert_eq!(app.status, explanation);
    assert!(explanation.contains("Rotate this edge-on projected path"));
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
}
