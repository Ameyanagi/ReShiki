use super::*;
use crate::canvas::{Edit, Tool};
use reshiki::document::{Document, Point};
fn app() -> App {
    let (mut app, _) = App::new();
    app.tab.doc = Document::from_native_file(include_bytes!(
        "../../../../tests/fixtures/mechanism-attachments-92/before.rsk"
    ))
    .unwrap();
    app.tab.saved = app.tab.doc.clone();
    app.tab.camera.zoom = 2.5;
    let _ = app.update(Message::ArrowStyle(Preset::Curved));
    app
}
#[test]
fn mechanism_attachment_92_two_clicks_one_arrow_one_undo_and_redo() {
    let mut app = app();
    let before = app.tab.doc.clone();
    app.edit(Edit::ArrowTarget(Point::new(0., -29.166668), false));
    assert!(app.tab.arrow_source.is_some());
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.history.undo_frames().len(), 0);
    app.edit(Edit::ArrowTarget(Point::new(-36.373, -21.), false));
    assert!(app.tab.arrow_source.is_none());
    assert_eq!(app.tab.doc.arrows.len(), 1);
    assert_eq!(app.tab.history.undo_frames().len(), 1);
    assert!(app.tab.doc.arrows[0].cubic.is_some());
    assert!(app.tab.doc.arrows[0].start_anchor.is_some());
    assert!(app.tab.doc.arrows[0].end_anchor.is_some());
    let after = app.tab.doc.clone();
    if let Some(path) = std::env::var_os("RESHIKI_ATTACHMENT_EVIDENCE") {
        std::fs::write(
            std::path::PathBuf::from(path).join("app-two-click.rsk"),
            after.file_json().unwrap(),
        )
        .unwrap();
    }
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, after);
}
#[test]
fn mechanism_attachment_92_cancel_same_invalid_free_and_alt_leave_no_partial_objects() {
    let mut app = app();
    let before = app.tab.doc.clone();
    let source = Point::new(0., -29.166668);
    app.edit(Edit::ArrowTarget(source, false));
    app.edit(Edit::ArrowTarget(source, false));
    assert!(app.error);
    assert!(app.tab.arrow_source.is_some());
    assert_eq!(app.tab.doc, before);
    app.edit(Edit::ArrowTarget(Point::new(300., 300.), false));
    assert!(app.error);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Escape);
    assert!(app.tab.arrow_source.is_none());
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::ArrowStyle(Preset::Curved));
    app.edit(Edit::ArrowTarget(source, false));
    let _ = app.update(Message::Tool(Tool::Select));
    assert!(app.tab.arrow_source.is_none());
    let _ = app.update(Message::ArrowStyle(Preset::Curved));
    app.edit(Edit::ArrowTarget(source, false));
    app.add_tab();
    assert!(app.tab.arrow_source.is_none());
    app.select_tab(0);
    assert!(app.tab.arrow_source.is_none());
    app.edit(Edit::ArrowTarget(source, true));
    assert_eq!(app.tab.doc.arrows.len(), 1);
    assert!(app.tab.doc.arrows[0].start_anchor.is_none());
    app.arrow_action(Action::AttachTargets(false));
    app.edit(Edit::Click(Point::new(200., 200.)));
    assert_eq!(app.tab.doc.arrows.len(), 2);
    assert!(app.tab.doc.arrows[1].end_anchor.is_none());
}
#[test]
fn mechanism_attachment_92_detach_action_has_one_undo_and_tangent_edit_keeps_links() {
    let mut app = app();
    app.edit(Edit::ArrowTarget(Point::new(0., -29.166668), false));
    app.edit(Edit::ArrowTarget(Point::new(-36.373, -21.), false));
    let before = app.tab.doc.clone();
    let id = app.tab.doc.arrows[0].id;
    let handle = app.tab.doc.arrows[0].cubic.unwrap()[0];
    app.edit(Edit::ArrowHandle(id, 3, handle.offset(-8., -12.)));
    assert!(app.tab.doc.arrows[0].start_anchor.is_some());
    assert!(app.tab.doc.arrows[0].end_anchor.is_some());
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    app.arrow_action(Action::DetachStart);
    assert!(app.tab.doc.arrows[0].start_anchor.is_none());
    assert_eq!(app.tab.doc.arrows[0].start, before.arrows[0].start);
    assert_eq!(app.tab.doc.arrows[0].cubic, before.arrows[0].cubic);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn mechanism_attachment_92_exchange_export_receipt_exposes_link_loss() {
    let mut app = app();
    app.structure_exported(Ok(Some(crate::app::figure_export::Saved {
        path: std::path::PathBuf::from("mechanism.cdxml"),
        details: vec![reshiki::arrow_anchors::EXPORT_NOTICE.into()],
    })));
    assert!(!app.error);
    assert!(app.status.contains("mechanism.cdxml"));
    assert!(app.status.contains("editing attachment links are omitted"));
}
