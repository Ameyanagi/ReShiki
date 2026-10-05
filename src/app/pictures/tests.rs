use super::*;
use reshiki::document::Document;

fn picture(width: u32, height: u32) -> Picture {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgba8(width, height)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    Picture::import(&bytes.into_inner()).unwrap()
}
fn ready() -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = Document::default();
    app.tab.doc.add_atom("O", Point::new(200., 0.));
    app
}
/// Inserts a selected picture, as the Import tab does.
fn insert(app: &mut App, picture: Picture) -> u64 {
    let before = app.tab.doc.clone();
    let id = app.tab.doc.next_id();
    app.tab
        .doc
        .graphics
        .push(picture.graphic(id, Point::default()));
    app.tab.selected = vec![id];
    app.changed(before);
    id
}
fn ticket(app: &mut App, target: u64) -> Ticket {
    app.tab.pictures.next += 1;
    app.tab.pictures.active = Some(app.tab.pictures.next);
    Ticket {
        serial: app.tab.pictures.next,
        epoch: app.tab.file_epoch,
        revision: app.tab.revision,
        target,
    }
}
fn finish(app: &mut App, ticket: Ticket, picture: Picture) {
    let _ = app.update(Message::Pictures(Action::Loaded(ticket, Ok(Some(picture)))));
}
#[test]
fn background_picture_replacement_keeps_the_front_and_its_focus() {
    use crate::app::tabs::{Action as Tabs, tests::Front};
    for stale in [false, true] {
        let mut app = ready();
        let target = insert(&mut app, picture(120, 80));
        app.tab.history = Default::default();
        let job = ticket(&mut app, target);
        let id = app.tab.id;
        if stale {
            let before = app.tab.doc.clone();
            app.tab.doc.add_atom("N", Point::default());
            app.changed(before);
        }
        let before = app.tab.doc.clone();
        let front = Front::new(&mut app);
        let task = app.update(Message::Tab(
            id,
            Box::new(Message::Pictures(Action::Loaded(
                job,
                Ok(Some(picture(60, 90))),
            ))),
        ));
        front.assert_unchanged(&app);
        assert!(app.tabs.background[0].pictures.active.is_none());
        if !stale {
            assert_eq!(task.units(), 0, "No inspector scroll or focus operation");
        }
        let _ = app.update(Message::Tabs(Tabs::Select(id)));
        if stale {
            assert_eq!(app.tab.doc, before);
            assert!(app.status.contains("changed"));
        } else {
            assert_ne!(app.tab.doc, before);
            assert!(app.status.contains("Picture replaced"));
            let _ = app.update(Message::Undo);
            assert_eq!(app.tab.doc, before);
            assert!(!app.tab.history.can_undo());
        }
    }
}

#[test]
fn asynchronous_replacement_never_changes_a_newer_drawing() {
    let mut app = ready();
    let id = insert(&mut app, picture(120, 80));
    let after = app.tab.doc.clone();
    for new_file in [false, true] {
        let job = ticket(&mut app, id);
        if new_file {
            app.tab.file_epoch += 1;
        } else {
            app.tab.revision += 1;
        }
        finish(&mut app, job, picture(10, 20));
        assert_eq!(app.tab.doc, after);
        assert!(app.tab.pictures.active.is_none());
    }
    let job = ticket(&mut app, id);
    let _ = app.update(Message::InlineText(
        super::super::inline_text::Action::Begin(None, Point::default()),
    ));
    finish(&mut app, job, picture(10, 20));
    assert!(app.tab.inline_text.is_some());
    assert_eq!(app.tab.doc, after);
}
#[test]
fn replacement_retains_center_rotation_and_layer_and_undo_restores_the_pixels() {
    let mut app = ready();
    insert(&mut app, picture(120, 80));
    let _ = app.update(Message::Transform(reshiki::editing::Transform::Rotate(37.)));
    let _ = app.update(Message::Transform(
        reshiki::editing::Transform::FlipHorizontal,
    ));
    let before = app.tab.doc.clone();
    let old = &before.graphics[0];
    let center = old.origin.offset(
        (old.axis_x.x + old.axis_y.x) / 2.,
        (old.axis_x.y + old.axis_y.y) / 2.,
    );
    let job = ticket(&mut app, old.id);
    finish(&mut app, job, picture(60, 100));
    let g = &app.tab.doc.graphics[0];
    assert_eq!((g.id, g.layer), (old.id, old.layer));
    assert!(
        center.distance(g.origin.offset(
            (g.axis_x.x + g.axis_y.x) / 2.,
            (g.axis_x.y + g.axis_y.y) / 2.
        )) < 0.001
    );
    assert!((g.axis_x.y.atan2(g.axis_x.x) - old.axis_x.y.atan2(old.axis_x.x)).abs() < 0.001);
    assert!(
        (g.axis_x.distance(Point::default()) / g.axis_y.distance(Point::default()) - 0.6).abs()
            < 0.001
    );
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}
#[test]
fn numeric_sizes_keep_proportions_and_reject_invalid_input_without_edits() {
    let mut app = ready();
    insert(&mut app, picture(120, 80));
    let before = app.tab.doc.clone();
    for invalid in ["NaN", "-1", "inf", "0", "wrong"] {
        let _ = app.update(Message::Pictures(Action::Width(invalid.into())));
        let _ = app.update(Message::Pictures(Action::Resize(true)));
        assert_eq!(app.tab.doc, before);
    }
    let _ = app.update(Message::Pictures(Action::Width("60".into())));
    let _ = app.update(Message::Pictures(Action::Resize(true)));
    let g = &app.tab.doc.graphics[0];
    assert!((millimetres(g.axis_x.distance(Point::default())) - 60.).abs() < 0.001);
    assert!((millimetres(g.axis_y.distance(Point::default())) - 40.).abs() < 0.001);
    let _ = app.update(Message::Pictures(Action::Lock(false)));
    let _ = app.update(Message::Pictures(Action::Height("70".into())));
    let _ = app.update(Message::Pictures(Action::Resize(false)));
    assert_eq!(app.tab.pictures.width, "60.00");
    assert_eq!(app.tab.pictures.height, "70.00");
    let _ = app.update(Message::Pictures(Action::RestoreAspect));
    assert_eq!(app.tab.pictures.height, "40.00");
    let restored = app.tab.doc.clone();
    app.apply_graphic_style(reshiki::graphics::GraphicChange::Stroke(
        reshiki::palette::Color::Custom([255, 0, 0]),
    ));
    assert_eq!(app.tab.doc, restored);
    assert_eq!(app.tab.doc.atoms, before.atoms);
}
#[test]
fn cancellation_failure_and_old_job_completion_leave_the_drawing_untouched() {
    let mut app = ready();
    let id = insert(&mut app, picture(20, 20));
    let before = app.tab.doc.clone();
    for result in [Ok(None), Err("Broken image".into())] {
        let job = ticket(&mut app, id);
        let _ = app.update(Message::Pictures(Action::Loaded(job, result)));
        assert_eq!(app.tab.doc, before);
        assert!(app.tab.pictures.active.is_none());
    }
    let old = ticket(&mut app, id);
    let current = ticket(&mut app, id);
    finish(&mut app, old, picture(20, 20));
    assert_eq!(app.tab.pictures.active, Some(current.serial));
    assert_eq!(app.tab.doc, before);
}

/// The context row's Order menu stacks pictures; the panel has no own row.
#[test]
fn order_commands_stack_a_selected_picture() {
    use super::super::object_toolbar::Command;
    let mut app = ready();
    insert(&mut app, picture(20, 20));
    let initial = app.tab.doc.graphics[0].layer;
    for front in [false, true] {
        let command = Command::Layer(front);
        assert!(command.enabled(&app, app.alignment_count()));
        let _ = app.update(command.message());
        let layer = app.tab.doc.graphics[0].layer;
        assert!(if front {
            layer > 0
        } else {
            layer < initial.min(0)
        });
    }
    let _ = app.update(Message::Undo);
    assert!(app.tab.doc.graphics[0].layer < initial.min(0));
}
#[test]
fn canvas_handle_resizing_refreshes_the_picture_dimensions() {
    let mut app = ready();
    insert(&mut app, picture(600, 360));
    assert_eq!(app.tab.pictures.width, "50.80");
    app.edit(crate::canvas::Edit::Transform {
        ids: app.tab.selected.clone(),
        pivot: Point::default(),
        scale: 0.5,
        rotation: 32.,
    });
    assert_eq!(app.tab.pictures.width, "25.40");
    assert_eq!(app.tab.pictures.height, "15.24");
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.pictures.width, "50.80");
}
