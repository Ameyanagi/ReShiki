use super::*;
use crate::canvas::Edit;

fn chain() -> (App, u64, u64) {
    let (mut app, _) = App::new();
    app.tab.doc = Document::default();
    app.tab.busy = false;
    app.tab.labels_dirty = false;
    let a = app.tab.doc.add_atom("C", Point::default());
    let b = app.tab.doc.add_atom("C", Point::new(42., 0.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    app.tab.saved = app.tab.doc.clone();
    app.sync_keyboard_drawing();
    (app, a, b)
}

#[test]
fn wheel_pan_retargets_hover_and_keeps_document_selection_and_history_unchanged() {
    let (mut app, a, b) = chain();
    let _ = app.update(Message::Canvas(Edit::Hover(Some(Point::default()))));
    assert_eq!(app.tab.keyboard_drawing.target(), Target::Atom(a));
    let document = app.tab.doc.clone();
    let selection = app.tab.selected.clone();
    let revision = app.tab.revision;
    let undo = app.tab.history.undo_frames().count();
    let _ = app.update(Message::Canvas(Edit::Pan(-42., 0., Some(Point::default()))));
    assert_eq!(app.tab.keyboard_drawing.target(), Target::Atom(b));
    assert_eq!(
        app.tab.hover,
        Some((Point::new(42., 0.), app.tab.file_epoch))
    );
    assert_eq!(app.tab.doc, document);
    assert_eq!(app.tab.selected, selection);
    assert_eq!(app.tab.revision, revision);
    assert_eq!(app.tab.history.undo_frames().count(), undo);
    assert!(!app.tab.history.can_redo());
}

#[test]
fn wheel_zoom_retests_the_same_world_point_at_the_new_pick_radius() {
    let (mut app, a, b) = chain();
    app.tab.camera.zoom = 0.04;
    let pointer = Point::new(12., 0.);
    let _ = app.update(Message::Canvas(Edit::Hover(Some(pointer))));
    assert_eq!(app.tab.keyboard_drawing.target(), Target::Atom(a));
    let document = app.tab.doc.clone();
    let _ = app.update(Message::Canvas(Edit::Zoom(26., pointer)));
    assert_eq!(app.tab.keyboard_drawing.target(), Target::Bond(a, b));
    assert_eq!(app.tab.hover, Some((pointer, app.tab.file_epoch)));
    assert_eq!(app.tab.doc, document);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn wheel_navigation_cannot_take_a_keyboard_or_edit_owned_target() {
    for edited in [false, true] {
        let (mut app, a, b) = chain();
        let pointer = Point::default();
        let _ = app.update(Message::Canvas(Edit::Hover(Some(pointer))));
        if edited {
            // Editing a hovered atom owns that target even if its ID stays the same.
            let _ = app.update(Message::ContextKey("n".into()));
            assert_eq!(app.tab.doc.atom(a).unwrap().element, "N");
        } else {
            let _ = app.update(Message::KeyboardDrawing(Action::Navigate(
                Direction::Right,
                true,
            )));
            assert_eq!(app.tab.keyboard_drawing.target(), Target::Atom(b));
        }
        let target = app.tab.keyboard_drawing.target();
        let document = app.tab.doc.clone();
        let _ = app.update(Message::Canvas(Edit::Pan(-200., 100., Some(pointer))));
        let _ = app.update(Message::Canvas(Edit::Zoom(2., pointer)));
        assert_eq!(app.tab.keyboard_drawing.target(), target);
        assert_eq!(app.tab.doc, document);
        // Actual subsequent motion gives ownership back to the pointer.
        let moved = Point::new(500., 500.);
        let _ = app.update(Message::Canvas(Edit::Hover(Some(moved))));
        assert_eq!(app.tab.keyboard_drawing.target(), Target::Blank(moved));
    }
}

#[test]
fn pan_without_a_pointer_and_pointer_leave_do_not_retarget() {
    let (mut app, a, _) = chain();
    let _ = app.update(Message::Canvas(Edit::Hover(Some(Point::default()))));
    let _ = app.update(Message::Canvas(Edit::Pan(-42., 0., None)));
    assert_eq!(app.tab.keyboard_drawing.target(), Target::Atom(a));
    let _ = app.update(Message::Canvas(Edit::Hover(None)));
    let _ = app.update(Message::Canvas(Edit::Zoom(2., Point::new(200., 200.))));
    assert_eq!(app.tab.keyboard_drawing.target(), Target::Atom(a));
}

#[test]
fn real_motion_back_to_the_previous_world_point_reclaims_keyboard_owned_target_after_pan() {
    let (mut app, a, b) = chain();
    let pointer = Point::default();
    let _ = app.update(Message::Canvas(Edit::Hover(Some(pointer))));
    let _ = app.update(Message::KeyboardDrawing(Action::Navigate(
        Direction::Right,
        true,
    )));
    assert_eq!(app.tab.keyboard_drawing.target(), Target::Atom(b));
    let document = app.tab.doc.clone();
    let _ = app.update(Message::Canvas(Edit::Pan(-42., 0., Some(pointer))));
    assert_eq!(app.tab.keyboard_drawing.target(), Target::Atom(b));
    // The still pointer is now over world42; a genuine move left42 screen
    // points reaches world0 again and must not look like a duplicate hover.
    let _ = app.update(Message::Canvas(Edit::Hover(Some(pointer))));
    assert_eq!(app.tab.keyboard_drawing.target(), Target::Atom(a));
    // Its successful handoff also makes the following wheel pointer-owned.
    let _ = app.update(Message::Canvas(Edit::Pan(-42., 0., Some(pointer))));
    assert_eq!(app.tab.keyboard_drawing.target(), Target::Atom(b));
    assert_eq!(app.tab.doc, document);
    assert!(!app.tab.history.can_undo());
}
