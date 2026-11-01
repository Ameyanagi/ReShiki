use super::*;
use crate::canvas::pen::Stroke;
use path::Action as PathAction;
use reshiki::graphics::{Graphic, PathNodeType};

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
fn path_heads_follow_pen_preview_and_survive_node_edits_and_closing() {
    use reshiki::arrows::Head;
    use reshiki::graphics::PathHeads;
    let mut app = fixture_app();
    let id = app.tab.selected[0];
    let heads = PathHeads {
        head: Head::Full,
        tail: Head::Left,
        ..Default::default()
    };
    app.tab.doc.graphics[0].heads = Some(heads);
    send(&mut app, PathAction::Continue);
    let before = app.tab.doc.clone();
    let stroke = Stroke {
        start: Point::new(360., 50.),
        end: Point::new(380., 80.),
        dragged: true,
        close: false,
    };
    let (preview, _, _) =
        crate::canvas::pen::apply(&before, &app.tab.selected, &app.tab.graphic_style, stroke)
            .unwrap();
    assert_eq!(preview.graphics[0].heads, Some(heads));
    assert_eq!(preview.graphics[0].parts().len(), 3);
    let _ = app.update(Message::Canvas(Edit::PenSegment(stroke)));
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc, preview);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);

    let _ = app.update(Message::Tool(Tool::EditPoints));
    let before = app.tab.doc.clone();
    let point = before.graphics[0].edit_points()[3].offset(10., -15.);
    app.edit(Edit::GraphicPoint(id, 3, point));
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc.graphics[0].heads, Some(heads));
    assert_eq!(app.tab.doc.graphics[0].parts().len(), 3);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    send(&mut app, PathAction::Close);
    assert_eq!(app.tab.doc.graphics[0].heads, Some(heads));
    assert_eq!(app.tab.doc.graphics[0].parts().len(), 1);
    send(&mut app, PathAction::Open);
    assert_eq!(app.tab.doc.graphics[0].heads, Some(heads));
    assert_eq!(app.tab.doc.graphics[0].parts().len(), 3);
}

#[test]
fn compound_path_point_edit_matches_generic_preview_and_has_one_history_step() {
    use reshiki::graphics::PathCommand;
    let mut app = fixture_app();
    let id = app.tab.selected[0];
    let points = [
        Point::new(0., 0.),
        Point::new(40., 0.),
        Point::new(40., 40.),
        Point::new(80., 0.),
        Point::new(100., 20.),
        Point::new(110., 40.),
        Point::new(140., 40.),
        Point::new(160., 20.),
    ];
    let mut graphic =
        reshiki::graphics::Graphic::pen_curve(id, points[0], points[1], Default::default());
    graphic.path = vec![
        PathCommand::Move(points[0]),
        PathCommand::Line(points[1]),
        PathCommand::Line(points[2]),
        PathCommand::Move(points[3]),
        PathCommand::Line(points[4]),
        PathCommand::Cubic(points[5], points[6], points[7]),
    ];
    graphic.node_types.clear();
    assert_eq!(graphic.path_handles(), None);
    assert_eq!(graphic.edit_points(), points);
    app.tab.doc.graphics[0] = graphic;
    let before = app.tab.doc.clone();
    let replacement = points[5].offset(-12., 17.);
    let mut preview = before.clone();
    preview.graphics[0].edit_point(5, replacement).unwrap();
    assert!(!app.tab.history.can_undo());
    app.edit(Edit::GraphicPoint(id, 5, replacement));
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc, preview);
    let mut expected = points;
    expected[5] = replacement;
    assert_eq!(app.tab.doc.graphics[0].edit_points(), expected);
    assert_eq!(app.tab.doc.graphics[1], before.graphics[1]);
    assert_eq!(app.tab.path_point, Some((id, 5)));
    assert!(app.tab.history.can_undo());
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    assert!(app.tab.history.can_redo());
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, preview);
    assert!(!app.tab.history.can_redo());
    assert_eq!(
        Document::from_native_file(&preview.file_json().unwrap()).unwrap(),
        preview.current()
    );
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
    assert_eq!(
        first.graphics[2].node_types,
        vec![PathNodeType::Corner, PathNodeType::Symmetric]
    );
    app.edit(Edit::PenSegment(Stroke {
        start: Point::new(480., 30.),
        end: Point::new(480., 30.),
        dragged: false,
        close: false,
    }));
    assert_eq!(app.tab.selected, vec![id]);
    assert_eq!(app.tab.doc.graphics.len(), 3);
    let second = app.tab.doc.clone();
    assert_eq!(
        second.graphics[2].node_types.last(),
        Some(&PathNodeType::Corner)
    );
    app.edit(Edit::PenSegment(Stroke {
        start: Point::new(560., 50.),
        end: Point::new(560., 100.),
        dragged: true,
        close: false,
    }));
    let third = app.tab.doc.clone();
    assert_eq!(
        third.graphics[2].node_types.last(),
        Some(&PathNodeType::Symmetric)
    );
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

fn node_type_app() -> App {
    let mut app = fixture_app();
    let id = app.tab.selected[0];
    let mut graphic = Graphic::pen_curve(
        id,
        Point::new(-80., 0.),
        Point::default(),
        Default::default(),
    );
    graphic
        .append_pen_node(Point::new(80., 0.), Some(Point::new(100., 30.)))
        .unwrap();
    graphic
        .move_path_point_free(4, Point::new(20., -30.))
        .unwrap();
    app.tab.doc.graphics[0] = graphic;
    let _ = app.update(Message::Tool(Tool::EditPoints));
    app.tab.path_point = Some((id, 3));
    app
}

#[test]
fn edit_points_node_double_click_cycles_once_without_moving_or_extra_history() {
    use crate::canvas::{Camera, pen_tests::point_gesture};
    use iced::Vector;
    for (initial, next) in [
        (PathNodeType::Corner, PathNodeType::Smooth),
        (PathNodeType::Smooth, PathNodeType::Symmetric),
        (PathNodeType::Symmetric, PathNodeType::Corner),
    ] {
        let mut app = node_type_app();
        let id = app.tab.selected[0];
        app.tab.doc.graphics[0]
            .set_path_node_type(3, initial)
            .unwrap();
        let before = app.tab.doc.clone();
        let node = before.graphics[0].edit_points()[3];
        for click in 0..2 {
            let events = point_gesture(
                &app.tab.doc,
                id,
                Camera::default(),
                3,
                Vector::new(2., -1.),
                Vector::new(1., 0.),
            );
            assert_eq!(events.preview, app.tab.doc);
            for edit in events.before_release {
                let _ = app.update(Message::Canvas(edit));
            }
            let _ = app.update(Message::Canvas(events.release));
            if click == 0 {
                assert_eq!(app.tab.doc, before);
                assert!(!app.tab.history.can_undo());
            }
        }
        assert!(!app.error, "{}", app.status);
        assert_eq!(app.tab.doc.graphics[0].node_types[1], next);
        assert_eq!(app.tab.doc.graphics[0].edit_points()[3], node);
        let after = app.tab.doc.clone();
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, after);
    }
}

#[test]
fn edit_points_control_drags_commit_the_constrained_or_alt_preview_in_one_undo() {
    use crate::canvas::{Camera, pen_tests::point_gesture_with_modifiers};
    use iced::{Vector, keyboard::Modifiers};
    for kind in [
        PathNodeType::Corner,
        PathNodeType::Smooth,
        PathNodeType::Symmetric,
    ] {
        for modifiers in [Modifiers::empty(), Modifiers::ALT] {
            let mut app = node_type_app();
            let id = app.tab.selected[0];
            app.tab.doc.graphics[0].set_path_node_type(3, kind).unwrap();
            let before = app.tab.doc.clone();
            let events = point_gesture_with_modifiers(
                &before,
                id,
                Camera::default(),
                4,
                Vector::new(2., -1.),
                Vector::new(25., 17.),
                modifiers,
            );
            for edit in events.before_release {
                let _ = app.update(Message::Canvas(edit));
            }
            let _ = app.update(Message::Canvas(events.release));
            assert!(!app.error, "{}", app.status);
            assert_eq!(app.tab.doc, events.preview);
            let after = app.tab.doc.clone();
            let old = before.graphics[0].edit_points();
            let moved = after.graphics[0].edit_points();
            assert_ne!(moved[4], old[4]);
            if modifiers.alt() || kind == PathNodeType::Corner {
                assert_eq!(moved[2], old[2]);
                assert_eq!(after.graphics[0].node_types[1], PathNodeType::Corner);
            } else {
                assert_ne!(moved[2], old[2]);
                assert!(
                    (moved[2].distance(moved[3]) - old[2].distance(old[3])).abs() < 0.001
                        || kind == PathNodeType::Symmetric
                );
                if kind == PathNodeType::Symmetric {
                    assert!(
                        moved[2].distance(Point::new(
                            2. * moved[3].x - moved[4].x,
                            2. * moved[3].y - moved[4].y
                        )) < 0.001
                    );
                }
            }
            let _ = app.update(Message::Undo);
            assert_eq!(app.tab.doc, before);
            assert!(!app.tab.history.can_undo());
            let _ = app.update(Message::Redo);
            assert_eq!(app.tab.doc, after);
        }
    }
}

#[tokio::test]
#[ignore = "Opt-in real Edit points context-row accessibility regression"]
async fn edit_points_context_buttons_set_the_controls_owner_type_with_one_undo() {
    use iced::advanced::{renderer::Headless, widget::operation};
    use iced_runtime::{UserInterface, user_interface::Cache};
    use reshiki::accessibility::{Activate, Collect};
    let size = iced::Size::new(700., 80.);
    let mut renderer =
        <iced::Renderer as Headless>::new(iced::Font::default(), iced::Pixels(16.), None)
            .await
            .unwrap();
    let mut app = node_type_app();
    let id = app.tab.selected[0];
    app.tab.path_point = None;
    {
        let mut ui = UserInterface::build(
            Element::from(iced::widget::row(app.edit_points_node_options())),
            size,
            Cache::new(),
            &mut renderer,
        );
        let mut collect = Collect::new(iced::Rectangle::with_size(size));
        ui.operate(&renderer, &mut operation::black_box(&mut collect));
        assert_eq!(collect.snapshot().nodes.len(), 3);
        assert!(collect.snapshot().nodes.iter().all(|node| !node.enabled));
    }
    app.tab.path_point = Some((id, 4));
    for (id, kind) in [
        ("edit-points-smooth", PathNodeType::Smooth),
        ("edit-points-symmetric", PathNodeType::Symmetric),
        ("edit-points-corner", PathNodeType::Corner),
    ] {
        let before = app.tab.doc.clone();
        let message = {
            let mut ui = UserInterface::build(
                Element::from(iced::widget::row(app.edit_points_node_options())),
                size,
                Cache::new(),
                &mut renderer,
            );
            let mut activate = Activate::<Message>::new(id);
            ui.operate(&renderer, &mut operation::black_box(&mut activate));
            activate.message().cloned().unwrap()
        };
        let _ = app.update(message);
        assert!(!app.error, "{}", app.status);
        assert_eq!(app.tab.doc.graphics[0].node_types[1], kind);
        let after = app.tab.doc.clone();
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, after);
        // Start the next independent command without earlier history entries.
        app.tab.history = Default::default();
    }
    app.tab.selected.clear();
    assert!(app.edit_points_node_options().is_empty());
}

#[test]
fn edit_points_node_type_actions_set_the_control_owner_with_one_undo() {
    for kind in [
        PathNodeType::Corner,
        PathNodeType::Smooth,
        PathNodeType::Symmetric,
    ] {
        let mut app = node_type_app();
        let id = app.tab.selected[0];
        if kind == PathNodeType::Corner {
            app.tab.doc.graphics[0]
                .set_path_node_type(3, PathNodeType::Symmetric)
                .unwrap();
        }
        app.tab.path_point = Some((id, 4));
        let before = app.tab.doc.clone();
        send(&mut app, PathAction::NodeType(kind));
        assert_eq!(app.tab.doc.graphics[0].node_types[1], kind);
        assert_eq!(app.tab.path_point, Some((id, 4)));
        let after = app.tab.doc.clone();
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, after);
    }
}

#[test]
fn edit_points_node_clicks_require_the_same_node_within_the_double_click_interval() {
    use std::time::Duration;
    let mut app = node_type_app();
    let id = app.tab.selected[0];
    let before = app.tab.doc.clone();
    let now = iced::time::Instant::now();
    app.path_node_click(id, 0, now);
    app.path_node_click(id, 3, now + Duration::from_millis(100));
    app.path_node_click(id, 3, now + Duration::from_millis(600));
    app.edit(Edit::Select(vec![]));
    app.edit(Edit::Select(vec![id]));
    app.path_node_click(id, 3, now + Duration::from_millis(650));
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
}
