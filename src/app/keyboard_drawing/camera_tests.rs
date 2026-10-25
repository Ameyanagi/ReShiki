use super::*;
use crate::canvas::{Edit, Navigation};
use iced::Vector;

fn pan(x: f32, y: f32, offset: Vector) -> Edit {
    Edit::Navigate(Navigation::Pan(Vector::new(x, y)), offset)
}

fn zoom(factor: f32, offset: Vector) -> Edit {
    Edit::Navigate(Navigation::Zoom(factor), offset)
}

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
    let _ = app.update(Message::Canvas(pan(-42., 0., Vector::ZERO)));
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
    let _ = app.update(Message::Canvas(zoom(26., Vector::new(0.48, 0.))));
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
        let _ = app.update(Message::Canvas(pan(-200., 100., Vector::ZERO)));
        let _ = app.update(Message::Canvas(zoom(2., Vector::ZERO)));
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
    let _ = app.update(Message::Canvas(Edit::Pan(-42., 0.)));
    assert_eq!(app.tab.keyboard_drawing.target(), Target::Atom(a));
    let _ = app.update(Message::Canvas(Edit::Hover(None)));
    let _ = app.update(Message::Canvas(zoom(2., Vector::new(200., 200.))));
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
    let _ = app.update(Message::Canvas(pan(-42., 0., Vector::ZERO)));
    assert_eq!(app.tab.keyboard_drawing.target(), Target::Atom(b));
    // The still pointer is now over world42; a genuine move left42 screen
    // points reaches world0 again and must not look like a duplicate hover.
    let _ = app.update(Message::Canvas(Edit::Hover(Some(pointer))));
    assert_eq!(app.tab.keyboard_drawing.target(), Target::Atom(a));
    // Its successful handoff also makes the following wheel pointer-owned.
    let _ = app.update(Message::Canvas(pan(-42., 0., Vector::new(-42., 0.))));
    assert_eq!(app.tab.keyboard_drawing.target(), Target::Atom(b));
    assert_eq!(app.tab.doc, document);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn two_pan_packets_from_one_view_resolve_through_the_current_camera() {
    let (mut app, _, b) = chain();
    let c = app.tab.doc.add_atom("C", Point::new(84., 0.));
    app.tab.doc.add_bond(b, c, 1, "plain");
    let _ = app.update(Message::Canvas(Edit::Hover(Some(Point::default()))));
    let document = app.tab.doc.clone();
    for _ in 0..2 {
        let _ = app.update(Message::Canvas(pan(-42., 0., Vector::ZERO)));
    }
    assert_eq!(app.tab.camera.center, Point::new(84., 0.));
    assert_eq!(app.tab.keyboard_drawing.target(), Target::Atom(c));
    assert_eq!(app.tab.doc, document);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn mixed_wheel_batches_keep_screen_offsets_at_nonunit_zoom_for_both_owners() {
    for (initial_zoom, factor, pan_first, expected_center, expected_point) in [
        (0.5, 2., true, Point::new(26., 16.), Point::new(126., -24.)),
        (0.5, 2., false, Point::new(68., -2.), Point::new(168., -42.)),
        (2., 0.5, true, Point::new(-61., 49.), Point::new(39., 9.)),
        (2., 0.5, false, Point::new(-82., 58.), Point::new(18., 18.)),
    ] {
        for pointer_owned in [false, true] {
            let (mut app, _) = App::new();
            app.tab.doc = Document::default();
            app.tab.busy = false;
            app.tab.labels_dirty = false;
            app.tab.camera = crate::canvas::Camera {
                center: Point::new(10., 20.),
                zoom: initial_zoom,
            };
            app.sync_keyboard_drawing();
            let offset = Vector::new(100., -40.);
            let start = app
                .tab
                .camera
                .center
                .offset(offset.x / initial_zoom, offset.y / initial_zoom);
            let _ = app.update(Message::Canvas(Edit::Hover(Some(start))));
            if !pointer_owned {
                app.tab
                    .keyboard_drawing
                    .set_target(Target::Blank(start), &app.tab.doc);
            }
            let edits = if pan_first {
                [pan(42., -18., offset), zoom(factor, offset)]
            } else {
                [zoom(factor, offset), pan(42., -18., offset)]
            };
            let document = app.tab.doc.clone();
            let revision = app.tab.revision;
            for edit in edits {
                let _ = app.update(Message::Canvas(edit));
            }
            assert!(app.tab.camera.center.distance(expected_center) < 0.001);
            assert_eq!(app.tab.camera.zoom, 1.);
            assert_eq!(
                app.tab.keyboard_drawing.target(),
                Target::Blank(if pointer_owned { expected_point } else { start })
            );
            assert_eq!(app.tab.doc, document);
            assert_eq!(app.tab.revision, revision);
            assert!(app.tab.selected.is_empty());
            assert!(!app.tab.history.can_undo());
        }
    }
}

#[test]
fn wheel_then_real_motion_in_one_old_view_reprojects_and_reclaims_keyboard_ownership() {
    for initial_zoom in [0.5, 2.] {
        for zooming in [false, true] {
            for keyboard_owned in [false, true] {
                let (mut app, _, b) = chain();
                app.tab.camera = crate::canvas::Camera {
                    center: Point::new(10., 20.),
                    zoom: initial_zoom,
                };
                let offset = Vector::new(100., -40.);
                let moved_offset = Vector::new(110., -34.);
                let start = app.tab.camera.pointer(offset);
                let _ = app.update(Message::Canvas(Edit::PointerHover(Some(offset))));
                if keyboard_owned {
                    app.tab
                        .keyboard_drawing
                        .set_target(Target::Atom(b), &app.tab.doc);
                }
                let document = app.tab.doc.clone();
                let selection = app.tab.selected.clone();
                let revision = app.tab.revision;
                let wheel = if zooming {
                    zoom(2., offset)
                } else {
                    pan(42., -18., offset)
                };
                // These packets are independent of the old view camera, as the
                // real canvas input regression verifies with one immutable view.
                let motion = Edit::PointerHover(Some(moved_offset));
                let _ = app.update(Message::Canvas(wheel));
                if keyboard_owned {
                    assert_eq!(app.tab.keyboard_drawing.target(), Target::Atom(b));
                }
                let current_point = if zooming {
                    Point::new(start.x + 5. / initial_zoom, start.y + 3. / initial_zoom)
                } else {
                    Point::new(10. + 68. / initial_zoom, 20. - 16. / initial_zoom)
                };
                let _ = app.update(Message::Canvas(motion));
                assert_eq!(
                    app.tab.keyboard_drawing.target(),
                    Target::Blank(current_point)
                );
                assert_eq!(app.tab.hover, Some((current_point, app.tab.file_epoch)));
                // A subsequent wheel now follows the reclaimed pointer owner.
                let _ = app.update(Message::Canvas(pan(-42., 0., moved_offset)));
                assert_eq!(
                    app.tab.keyboard_drawing.target(),
                    Target::Blank(current_point.offset(42. / app.tab.camera.zoom, 0.))
                );
                assert_eq!(app.tab.doc, document);
                assert_eq!(app.tab.selected, selection);
                assert_eq!(app.tab.revision, revision);
                assert!(!app.tab.history.can_undo());
            }
        }
    }
}
