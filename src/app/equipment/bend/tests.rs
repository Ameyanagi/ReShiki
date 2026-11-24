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
    assert!(!state.can_apply());
    assert!(app.tube_bend_preview().is_none());
    let token = state.token;
    let canvas = app.tube_bend_canvas().unwrap();
    let [from, to] = canvas.leg.unwrap();
    assert!(canvas.can_rotate);
    assert_eq!(from, target.at);
    assert!(to.x > from.x && to.y == from.y);
    let _ = app.update(message(Action::Apply(token)));
    let _ = app.update(message(Action::Angle(token, "45".into())));
    assert!(app.current_tube_bend().unwrap().can_apply());
    let _ = app.update(message(Action::Angle(token, "0".into())));
    assert!(app.tube_bend_preview().is_none());
    assert!(!app.current_tube_bend().unwrap().can_apply());
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.history.undo_frames().len(), 0);
}

#[test]
fn invalid_bend_is_feedback_only_and_apply_records_one_undo_with_existing_seats() {
    let mut app = source();
    seat_cap(&mut app, "a");
    let target = menu(&mut app);
    let before = app.tab.doc.clone();
    let _ = app.update(message(Action::Begin(target)));
    let state = app.current_tube_bend().unwrap();
    let (token, radius) = (state.token, state.radius.clone());
    let _ = app.update(message(Action::Angle(token, "45".into())));
    let last_valid = app.tube_bend_preview().unwrap().clone();
    let _ = app.update(message(Action::Radius(token, "NaN".into())));
    assert_eq!(app.tube_bend_preview(), Some(&last_valid));
    assert!(!app.current_tube_bend().unwrap().can_apply());
    assert!(app.current_tube_bend().unwrap().notice.is_some());
    let _ = app.update(message(Action::Apply(token)));
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.history.undo_frames().len(), 0);
    let _ = app.update(message(Action::Radius(token, radius)));
    let expected = app
        .tube_bend_preview()
        .unwrap_or_else(|| panic!("{}", app.status))
        .clone();
    let _ = app.update(message(Action::Apply(token)));
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc, expected);
    assert_eq!(app.tab.doc.seats, before.seats);
    assert_eq!(app.tab.history.undo_frames().len(), 1);
    assert!(!app.tube_bend_active());
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, expected);
}

#[test]
fn bend_drag_and_numeric_input_share_the_same_preview_and_stale_actions_are_ignored() {
    let mut app = source();
    let target = menu(&mut app);
    let _ = app.update(message(Action::Begin(target)));
    let state = app.current_tube_bend().unwrap();
    let token = state.token;
    let point = state.model.pivot().offset(20., 15.);
    let _ = app.update(Message::Canvas(Edit::TubeBendDrag {
        token,
        at: Some(point),
    }));
    let angle = app.current_tube_bend().unwrap().angle.clone();
    let preview = app.tube_bend_preview().unwrap().clone();
    let _ = app.update(message(Action::Cancel(token)));
    let _ = app.update(message(Action::Begin(target)));
    let current = app.current_tube_bend().unwrap().token;
    assert_ne!(token, current);
    let _ = app.update(message(Action::Angle(current, angle)));
    assert_eq!(app.tube_bend_preview().unwrap(), &preview);
    let _ = app.update(message(Action::Apply(token)));
    assert!(app.tube_bend_active());
    app.begin_file_epoch();
    let before = app.tab.doc.clone();
    let _ = app.update(message(Action::Apply(current)));
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
    let (token, radius) = (state.token, state.radius.clone());
    let neutral = semantics(&app, &renderer);
    assert!(
        neutral
            .nodes
            .iter()
            .any(|node| node.id == "tube-bend-apply" && !node.enabled)
    );
    assert!(app.tube_bend_preview().is_none());
    capture(&mut app, &mut renderer, "02-bend-point-placed");
    let pivot = target.at;
    let _ = app.update(Message::Canvas(Edit::TubeBendDrag {
        token,
        at: Some(pivot.offset(80., 80.)),
    }));
    let valid = semantics(&app, &renderer);
    capture(&mut app, &mut renderer, "03-rotate-preview");
    assert!(valid.duplicate_ids.is_empty());
    for id in [
        "tube-bend-angle",
        "tube-bend-radius",
        "tube-bend-apply",
        "tube-bend-cancel",
    ] {
        assert!(
            valid.nodes.iter().any(|node| node.id == id && node.enabled),
            "{id}"
        );
    }
    let preview = app.tube_bend_preview().unwrap().clone();
    let _ = app.update(message(Action::Radius(token, "-1".into())));
    let invalid = semantics(&app, &renderer);
    assert!(
        invalid
            .nodes
            .iter()
            .any(|node| node.id == "tube-bend-apply" && !node.enabled)
    );
    let _ = app.update(message(Action::Radius(token, radius)));
    let _ = app.update(message(Action::Apply(token)));
    assert_eq!(app.tab.doc, preview);
    capture(&mut app, &mut renderer, "04-bend-applied");
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
            format!("VALID\n{valid:#?}\nINVALID\n{invalid:#?}\n"),
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
    capture(&mut locked, &mut renderer, "05-bend-blocked-by-fittings");
}

fn show_sections(doc: &mut Document) {
    for graphic in &mut doc.graphics {
        graphic.piece.as_mut().unwrap().params.insert(
            "view".into(),
            reshiki::equipment::eval::Value::State("section".into()),
        );
    }
}
