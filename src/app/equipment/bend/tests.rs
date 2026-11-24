use super::*;
use reshiki::equipment::{builtin, placed, solve};

fn source() -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tool = Tool::Select;
    let mm = placed::world_per_mm(&app.tab.doc, placed::DEFAULT_PT_PER_MM);
    let definition = builtin::find("reshiki/compression-tube-120-1-4").unwrap();
    app.tab.doc.graphics.push(placed::graphic(
        1,
        definition.clone(),
        Point::new(-100., 0.),
        mm,
    ));
    app.tab
        .doc
        .graphics
        .push(placed::graphic(2, definition, Point::new(100., 0.), mm));
    app.tab.doc.piece_definitions = placed::used(&app.tab.doc);
    app.tab.doc.group_selection(&[1, 2]).unwrap();
    app.tab.selected = vec![1, 2];
    app
}

fn menu(app: &mut App) -> Target {
    let graphic = app.tab.doc.graphics.iter().find(|g| g.id == 2).unwrap();
    let ports = solve::ports(graphic);
    let a = ports.iter().find(|p| p.at.joint == "a").unwrap().point;
    let b = ports.iter().find(|p| p.at.joint == "b").unwrap().point;
    let at = Point::new(
        (a[0] * 0.5 + b[0] * 0.5) as f32,
        (a[1] * 0.5 + b[1] * 0.5) as f32,
    );
    app.edit(Edit::ContextMenu {
        position: iced::Point::new(340., 230.),
        at,
        selected: vec![1, 2],
        hit: vec![2],
    });
    let (label, Action::Begin(target)) = app.tube_bend_context_command().unwrap() else {
        panic!("bend command");
    };
    assert_eq!(label, "Add Bend Point…");
    assert_eq!(target.piece, 2);
    assert_eq!(target.at, at);
    target
}

fn seat_cap(app: &mut App, end: &str) {
    let doc = &mut app.tab.doc;
    let id = doc.next_id();
    let mm = placed::world_per_mm(doc, placed::DEFAULT_PT_PER_MM);
    let cap = builtin::find("reshiki/compression-cap-1-4").unwrap();
    doc.graphics
        .push(placed::graphic(id, cap, Point::default(), mm));
    solve::connect(
        doc,
        &[id],
        &solve::JointRef {
            piece: 2,
            joint: end.into(),
        },
        &solve::JointRef {
            piece: id,
            joint: "tube".into(),
        },
    )
    .unwrap();
    doc.piece_definitions = placed::used(doc);
}

fn input_finished(token: Token, field: Field, unfocused: bool) -> Message {
    message(Action::FinishInput {
        token,
        field,
        unfocused,
    })
}

#[test]
fn context_bend_uses_the_exact_hit_and_world_click_even_in_an_expanded_group() {
    let mut app = source();
    let target = menu(&mut app);
    assert_eq!(app.context_menu.as_ref().unwrap().at, Some(target.at));
    let before = app.tab.doc.clone();
    let _ = app.update(message(Action::Begin(target)));
    let state = app.current_tube_bend().unwrap();
    assert_eq!(state.target, target);
    assert_eq!(state.model.pivot(), target.at);
    assert_eq!(state.angle, "0");
    assert!(state.notice.is_none());
    assert!(app.tube_bend_preview().is_none());
    let token = state.input_token;
    let canvas = app.tube_bend_canvas().unwrap();
    let [from, to] = canvas.leg.unwrap();
    assert!(canvas.can_rotate);
    assert_eq!(from, target.at);
    assert!(to.x > from.x && to.y == from.y);
    let _ = app.update(input_finished(token, Field::Angle, false));
    let _ = app.update(message(Action::Input(token, Field::Angle, "45".into())));
    assert!(app.tube_bend_preview().is_some());
    let _ = app.update(message(Action::Input(token, Field::Angle, "0".into())));
    let _ = app.update(input_finished(token, Field::Angle, true));
    assert!(app.tube_bend_preview().is_none());
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.history.undo_frames().len(), 0);
}

#[test]
fn release_commits_once_per_drag_keeps_seats_and_rebinds_the_same_bend() {
    let mut app = source();
    seat_cap(&mut app, "a");
    let target = menu(&mut app);
    let before = app.tab.doc.clone();
    let _ = app.update(message(Action::Begin(target)));
    let token = app.current_tube_bend().unwrap().token;
    for y in [15., 30., 45.] {
        let _ = app.update(Message::Canvas(Edit::TubeBendDrag {
            token,
            at: target.at.offset(80., y),
        }));
    }
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.history.undo_frames().len(), 0);
    let _ = app.update(Message::Canvas(Edit::TubeBendFinish {
        token,
        at: Some(target.at.offset(80., 80.)),
    }));
    let first = app.tab.doc.clone();
    assert_ne!(first, before);
    assert_eq!(first.seats, before.seats);
    assert_eq!(app.tab.history.undo_frames().len(), 1);
    let state = app.current_tube_bend().unwrap();
    assert!(state.model.is_existing_bend());
    assert_eq!(state.model.pivot(), target.at);
    assert!((state.model.default_angle_deg() - 45.).abs() < 1e-8);
    let next = state.token;
    assert_ne!(token, next);
    let _ = app.update(Message::Canvas(Edit::TubeBendFinish {
        token,
        at: Some(target.at.offset(80., 30.)),
    }));
    assert_eq!(app.tab.doc, first);
    let _ = app.update(Message::Canvas(Edit::TubeBendFinish {
        token: next,
        at: Some(target.at.offset(80., -55.)),
    }));
    let second = app.tab.doc.clone();
    assert_ne!(second, first);
    let tube = second.graphics.iter().find(|g| g.id == 2).unwrap();
    let route = tube.piece.as_ref().unwrap().tube_route.as_ref().unwrap();
    assert_eq!(route.bends.len(), 1);
    assert_eq!(app.tab.history.undo_frames().len(), 2);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, first);
    assert!(!app.tube_bend_active());
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, second);
}

#[test]
fn batched_touch_motion_and_lift_commit_one_bend() {
    let mut app = source();
    let target = menu(&mut app);
    let before = app.tab.doc.clone();
    let _ = app.update(message(Action::Begin(target)));
    let token = app.current_tube_bend().unwrap().token;
    let interim = target.at.offset(80., 40.);
    let _ = app.update(Message::Canvas(Edit::Touch(vec![
        Edit::Hover(Some(interim)),
        Edit::TubeBendDrag { token, at: interim },
    ])));
    assert!(app.tube_bend_preview().is_some());
    assert_eq!(app.tab.doc, before);
    let end = target.at.offset(80., 80.);
    let _ = app.update(Message::Canvas(Edit::Touch(vec![
        Edit::TubeBendDrag { token, at: end },
        Edit::TubeBendFinish {
            token,
            at: Some(end),
        },
    ])));
    assert_eq!(app.tab.history.undo_frames().len(), 1);
    let bend = &app.current_tube_bend().unwrap().model;
    assert!((bend.default_angle_deg() - 45.).abs() < 1e-8);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn input_submit_and_blur_commit_once_and_the_current_grip_can_still_drag() {
    let mut app = source();
    let target = menu(&mut app);
    let before = app.tab.doc.clone();
    let _ = app.update(message(Action::Begin(target)));
    let state = app.current_tube_bend().unwrap();
    let (token, input_token) = (state.token, state.input_token);
    let _ = app.update(message(Action::Input(
        input_token,
        Field::Angle,
        "4".into(),
    )));
    let _ = app.update(message(Action::Input(
        input_token,
        Field::Angle,
        "45".into(),
    )));
    let preview = app.tube_bend_preview().unwrap().clone();
    assert_eq!(app.tab.doc, before);
    let _ = app.update(input_finished(input_token, Field::Angle, false));
    let _ = app.update(input_finished(input_token, Field::Angle, true));
    assert_eq!(app.tab.doc, preview);
    assert_eq!(app.tab.history.undo_frames().len(), 1);
    let state = app.current_tube_bend().unwrap();
    assert_eq!(
        state.token, token,
        "a grip pressed when the field blurs stays valid"
    );
    let input_token = state.input_token;
    let _ = app.update(message(Action::Input(
        input_token,
        Field::Radius,
        "14".into(),
    )));
    let _ = app.update(input_finished(input_token, Field::Radius, true));
    assert_eq!(app.tab.history.undo_frames().len(), 2);
    let _ = app.update(Message::Canvas(Edit::TubeBendFinish {
        token,
        at: Some(target.at.offset(45., 80.)),
    }));
    assert_eq!(app.tab.history.undo_frames().len(), 3);
}

#[test]
fn changing_fields_keeps_the_first_keystroke_and_keeps_edits_separate() {
    for blur_arrives_first in [true, false] {
        let mut app = source();
        let target = menu(&mut app);
        let _ = app.update(message(Action::Begin(target)));
        let token = app.current_tube_bend().unwrap().input_token;
        let _ = app.update(message(Action::Input(token, Field::Angle, "30".into())));
        let angle_only = app.tube_bend_preview().unwrap().clone();
        if blur_arrives_first {
            let _ = app.update(input_finished(token, Field::Angle, true));
        }
        let _ = app.update(message(Action::Input(token, Field::Radius, "18".into())));
        if !blur_arrives_first {
            let _ = app.update(input_finished(token, Field::Angle, true));
        }
        assert_eq!(app.tab.doc, angle_only);
        assert_eq!(app.current_tube_bend().unwrap().radius, "18");
        assert_eq!(app.tab.history.undo_frames().len(), 1);
        let _ = app.update(input_finished(token, Field::Radius, true));
        assert_eq!(app.tab.history.undo_frames().len(), 2);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, angle_only);
    }
}

#[test]
fn right_click_finishes_a_pending_value_and_opens_the_normal_context_menu() {
    let mut app = source();
    let target = menu(&mut app);
    let _ = app.update(message(Action::Begin(target)));
    let token = app.current_tube_bend().unwrap().input_token;
    let _ = app.update(message(Action::Input(token, Field::Angle, "35".into())));
    let expected = app.tube_bend_preview().unwrap().clone();
    let _ = app.update(Message::Canvas(Edit::ContextMenu {
        position: iced::Point::new(340., 230.),
        at: target.at,
        selected: vec![2],
        hit: vec![2],
    }));
    assert_eq!(app.tab.doc, expected);
    assert_eq!(app.tab.history.undo_frames().len(), 1);
    assert!(!app.tube_bend_active());
    assert!(app.context_menu.is_some());
}

#[test]
fn invalid_fields_and_aborted_or_invalid_drags_leave_the_document_unchanged() {
    let mut app = source();
    let target = menu(&mut app);
    let before = app.tab.doc.clone();
    let _ = app.update(message(Action::Begin(target)));
    let state = app.current_tube_bend().unwrap();
    let (token, input_token) = (state.token, state.input_token);
    let _ = app.update(message(Action::Input(
        input_token,
        Field::Angle,
        "45".into(),
    )));
    let valid = app.tube_bend_preview().unwrap().clone();
    let _ = app.update(message(Action::Input(
        input_token,
        Field::Angle,
        "NaN".into(),
    )));
    assert_eq!(app.tube_bend_preview(), Some(&valid));
    let _ = app.update(input_finished(input_token, Field::Angle, false));
    let _ = app.update(input_finished(input_token, Field::Angle, true));
    assert!(app.current_tube_bend().unwrap().notice.is_some());
    assert!(app.tube_bend_preview().is_none());
    assert_eq!(app.tab.doc, before);

    let _ = app.update(Message::Canvas(Edit::TubeBendDrag {
        token,
        at: target.at.offset(80., 80.),
    }));
    assert!(app.tube_bend_preview().is_some());
    let _ = app.update(Message::Canvas(Edit::TubeBendAbort { token }));
    let _ = app.update(Message::Canvas(Edit::TubeBendFinish {
        token,
        at: Some(target.at.offset(80., 80.)),
    }));
    assert!(app.tube_bend_preview().is_none());
    assert_eq!(app.tab.doc, before);
    let token = app.current_tube_bend().unwrap().token;
    let _ = app.update(Message::Canvas(Edit::TubeBendDrag {
        token,
        at: target.at.offset(80., 80.),
    }));
    let _ = app.update(Message::Canvas(Edit::TubeBendFinish {
        token,
        at: Some(target.at.offset(-80., 0.)),
    }));
    assert!(app.current_tube_bend().unwrap().notice.is_some());
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.history.undo_frames().len(), 0);
    app.begin_file_epoch();
    let _ = app.update(Message::Canvas(Edit::TubeBendFinish {
        token,
        at: Some(target.at.offset(80., 80.)),
    }));
    assert_eq!(app.tab.doc, before);
    assert!(!app.tube_bend_active());
}

#[tokio::test]
#[ignore = "Opt-in native widget snapshot; set RESHIKI_TUBE_BEND_EVIDENCE"]
async fn native_bend_controls_snapshot_and_exercised_documents() {
    use iced::advanced::{Layout, layout, renderer::Headless, widget::Tree};
    use iced::{Rectangle, Size};

    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .expect("headless native renderer");
    let semantics = |app: &App, renderer: &iced::Renderer| {
        let viewport = Rectangle::with_size(Size::new(1280., 900.));
        let mut view = app.view();
        let mut tree = Tree::new(view.as_widget());
        let node = view.as_widget_mut().layout(
            &mut tree,
            renderer,
            &layout::Limits::new(viewport.size(), viewport.size()),
        );
        let mut collect = reshiki::accessibility::Collect::new(viewport);
        view.as_widget_mut().operate(
            &mut tree,
            Layout::new(&node),
            renderer,
            &mut iced::advanced::widget::operation::black_box(&mut collect),
        );
        collect.snapshot().clone()
    };
    let mut app = source();
    app.appearance.mode = crate::appearance::Mode::Light;
    app.tab.doc.groups.clear();
    app.tab.doc.graphics.retain(|g| g.id == 2);
    let tube = app.tab.doc.graphics.first_mut().unwrap();
    tube.axis_x = Point::new(4., 0.);
    tube.axis_y = Point::new(0., 4.);
    app.tab.selected = vec![2];
    seat_cap(&mut app, "a");
    show_sections(&mut app.tab.doc);
    app.tab.camera.center = Point::new(238., 78.);
    app.tab.camera.zoom = 1.1;
    let directory = std::env::var_os("RESHIKI_TUBE_BEND_EVIDENCE").map(std::path::PathBuf::from);
    let capture = |app: &mut App, renderer: &mut iced::Renderer, name: &str| {
        if let Some(dir) = &directory {
            std::fs::create_dir_all(dir).unwrap();
            crate::app::workspace::layout_snapshots::snapshot(
                app,
                renderer,
                Size::new(1280., 820.),
                &dir.join(format!("{name}.png")),
            );
        }
    };
    let target = menu(&mut app);
    capture(&mut app, &mut renderer, "01-add-bend-point-menu");
    let before = app.tab.doc.clone();
    let _ = app.update(message(Action::Begin(target)));
    let state = app.current_tube_bend().unwrap();
    let token = state.token;
    let neutral = semantics(&app, &renderer);
    assert!(
        !neutral
            .nodes
            .iter()
            .any(|node| matches!(node.id.as_str(), "tube-bend-apply" | "tube-bend-cancel"))
    );
    assert!(app.tube_bend_preview().is_none());
    capture(&mut app, &mut renderer, "02-bend-point-placed");
    let pivot = target.at;
    let _ = app.update(Message::Canvas(Edit::TubeBendDrag {
        token,
        at: pivot.offset(80., 80.),
    }));
    let valid = semantics(&app, &renderer);
    capture(&mut app, &mut renderer, "03-rotate-preview");
    assert!(valid.duplicate_ids.is_empty());
    for id in ["tube-bend-angle", "tube-bend-radius"] {
        assert!(
            valid.nodes.iter().any(|node| node.id == id && node.enabled),
            "{id}"
        );
    }
    let preview = app.tube_bend_preview().unwrap().clone();
    let _ = app.update(Message::Canvas(Edit::TubeBendFinish { token, at: None }));
    assert_eq!(app.tab.doc, preview);
    assert_eq!(app.tab.history.undo_frames().len(), 1);
    capture(&mut app, &mut renderer, "04-bend-released");
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    capture(&mut app, &mut renderer, "05-bend-undone");
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, preview);
    if let Some(directory) = &directory {
        for (stage, doc) in [
            ("before", &before),
            ("preview", &preview),
            ("accepted", &app.tab.doc),
        ] {
            std::fs::write(
                directory.join(format!("tube-bend-{stage}.rsk")),
                doc.file_json().unwrap(),
            )
            .unwrap();
        }
        std::fs::write(
            directory.join("tube-bend-controls.txt"),
            format!("NEUTRAL\n{neutral:#?}\nROTATING\n{valid:#?}\n"),
        )
        .unwrap();
    }
    let mut locked = app;
    locked.tab.doc = before;
    seat_cap(&mut locked, "b");
    show_sections(&mut locked.tab.doc);
    let target = menu(&mut locked);
    let _ = locked.update(message(Action::Begin(target)));
    assert!(locked.status.contains("Both tube ends are seated"));
    assert!(!locked.tube_bend_canvas().unwrap().can_rotate);
    capture(&mut locked, &mut renderer, "06-bend-blocked-by-fittings");
}

fn show_sections(doc: &mut Document) {
    for graphic in &mut doc.graphics {
        graphic.piece.as_mut().unwrap().params.insert(
            "view".into(),
            reshiki::equipment::eval::Value::State("section".into()),
        );
    }
}
