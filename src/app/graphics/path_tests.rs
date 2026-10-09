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
fn pen_continue_from_select_and_edit_points_appends_to_the_same_path_with_one_undo() {
    for tool in [Tool::Select, Tool::EditPoints] {
        let mut app = fixture_app();
        app.tab.doc.add_atom("O", Point::new(500., 20.));
        let id = app.tab.selected[0];
        let _ = app.update(Message::Tool(tool));
        send(&mut app, PathAction::Node(3));
        let point = app.tab.path_point;
        let before = app.tab.doc.clone();
        let tab = app.tab.id;
        let tabs = app.tabs.background.len();
        send(&mut app, PathAction::Continue);
        assert_eq!(app.tool, Tool::Graphic(GraphicKind::Path));
        assert_eq!(app.tab.selected, vec![id]);
        assert_eq!(app.tab.path_point, point);
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
        let _ = app.update(Message::Canvas(Edit::PenSegment(Stroke {
            start: Point::new(360., 50.),
            end: Point::new(360., 50.),
            dragged: false,
            close: false,
        })));
        assert!(!app.error, "{}", app.status);
        let after = app.tab.doc.clone();
        assert_eq!(after.graphics.len(), before.graphics.len());
        assert_eq!(after.graphics[0].id, id);
        assert_eq!(
            after.graphics[0].path.len(),
            before.graphics[0].path.len() + 1
        );
        assert_eq!(after.graphics[1], before.graphics[1]);
        assert_eq!(after.atoms, before.atoms);
        assert_eq!(after.bonds, before.bonds);
        assert_eq!(app.tab.id, tab);
        assert_eq!(app.tabs.background.len(), tabs);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, after);
    }
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

#[test]
fn pen_offset_click_is_selection_only_and_offset_drag_has_one_exact_history_step() {
    use crate::canvas::{Camera, pen_tests::point_gesture};
    use iced::Vector;
    for (single, tilted) in [(false, false), (true, false), (false, true)] {
        for index in [1, 3] {
            let mut app = fixture_app();
            let id = app.tab.selected[0];
            if single {
                app.tab.doc.graphics[0] = reshiki::graphics::Graphic::pen_curve(
                    id,
                    Point::new(-80., -10.),
                    Point::new(80., 40.),
                    Default::default(),
                );
            }
            if tilted {
                let graphic = &mut app.tab.doc.graphics[0];
                graphic.origin = Point::new(-17., 23.);
                graphic.axis_x = Point::new(0.8, 0.3);
                graphic.axis_y = Point::new(-0.2, 0.7);
                graphic.depth = [12., 0.4, -0.2];
            }
            let a = app.tab.doc.add_atom("C", Point::new(500., 20.));
            let b = app.tab.doc.add_atom("O", Point::new(528., 20.));
            app.tab.doc.add_bond(a, b, 1, "plain");
            let _ = app.update(Message::Tool(Tool::EditPoints));
            let before = app.tab.doc.clone();
            let native = before.file_json().unwrap();
            let revision = app.tab.revision;
            let camera = Camera {
                center: Point::new(111.13, 28.7),
                zoom: 1.48,
            };
            let grab = Vector::new(2.25, -1.75);
            let click = point_gesture(&before, id, camera, index, grab, Vector::ZERO);
            assert_eq!(click.pressed_preview, before);
            assert_eq!(click.preview, before);
            for edit in click.before_release {
                let _ = app.update(Message::Canvas(edit));
            }
            let _ = app.update(Message::Canvas(click.release));
            assert!(!app.error, "{}", app.status);
            assert_eq!(app.tab.path_point, Some((id, index)));
            assert_eq!(app.tab.selected, vec![id]);
            assert_eq!(app.tab.doc, before);
            assert_eq!(app.tab.doc.file_json().unwrap(), native);
            assert_eq!(app.tab.revision, revision);
            assert!(!app.tab.history.can_undo());
            assert!(!app.tab.history.can_redo());

            let motion = Vector::new(37., -23.);
            let drag = point_gesture(&before, id, camera, index, grab, motion);
            assert_eq!(drag.pressed_preview, before);
            assert_ne!(drag.preview, before);
            for edit in drag.before_release {
                let _ = app.update(Message::Canvas(edit));
            }
            let _ = app.update(Message::Canvas(drag.release));
            assert!(!app.error, "{}", app.status);
            let after = app.tab.doc.clone();
            assert_eq!(after, drag.preview);
            assert_eq!(after.atoms, before.atoms);
            assert_eq!(after.bonds, before.bonds);
            assert_eq!(after.graphics[1], before.graphics[1]);
            let graphic = &after.graphics[0];
            assert_eq!(graphic.origin, before.graphics[0].origin);
            assert_eq!(graphic.axis_x, before.graphics[0].axis_x);
            assert_eq!(graphic.axis_y, before.graphics[0].axis_y);
            assert_eq!(graphic.depth, before.graphics[0].depth);
            let carried: &[usize] = match (index, single) {
                (3, true) => &[2, 3],
                (3, false) => &[2, 3, 4],
                _ => &[1],
            };
            let old = before.graphics[0].edit_points();
            let moved = graphic.edit_points();
            for (i, (old, moved)) in old.into_iter().zip(moved).enumerate() {
                let expected = if carried.contains(&i) {
                    old.offset(motion.x / camera.zoom, motion.y / camera.zoom)
                } else {
                    old
                };
                assert!(
                    moved.distance(expected) < 0.0001,
                    "point {i}: {moved:?} / {expected:?}"
                );
            }
            let _ = app.update(Message::Undo);
            assert_eq!(app.tab.doc, before);
            assert_eq!(app.tab.doc.file_json().unwrap(), native);
            assert!(!app.tab.history.can_undo());
            let _ = app.update(Message::Redo);
            assert_eq!(app.tab.doc, after);
        }
    }
}

#[test]
fn pen_edge_on_offset_click_selects_without_error_and_only_a_drag_is_rejected() {
    use crate::canvas::{Camera, pen_tests::point_gesture};
    use iced::Vector;
    let mut app = fixture_app();
    let id = app.tab.selected[0];
    app.tab.doc.graphics[0] = reshiki::graphics::Graphic::pen_curve(
        id,
        Point::new(-80., -10.),
        Point::new(80., 40.),
        Default::default(),
    );
    app.tab.doc.graphics[0].axis_y = Point::default();
    app.tab.doc.graphics[0].depth = [12., 0., 1.];
    let _ = app.update(Message::Tool(Tool::EditPoints));
    let before = app.tab.doc.clone();
    let camera = Camera {
        zoom: 1.48,
        ..Default::default()
    };
    for motion in [Vector::ZERO, Vector::new(37., -23.)] {
        let events = point_gesture(&before, id, camera, 3, Vector::new(2.25, -1.75), motion);
        assert_eq!(events.pressed_preview, before);
        assert_eq!(events.preview, before);
        for edit in events.before_release {
            let _ = app.update(Message::Canvas(edit));
        }
        let _ = app.update(Message::Canvas(events.release));
        assert_eq!(app.tab.path_point, Some((id, 3)));
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
        assert_eq!(app.error, motion != Vector::ZERO);
        if app.error {
            assert!(app.status.contains("Rotate this edge-on projected path"));
        }
    }
}
