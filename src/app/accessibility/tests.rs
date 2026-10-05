use super::*;

fn native_request(
    app: &mut App,
    id: &str,
    role: reshiki::accessibility::Role,
) -> accesskit::ActionRequest {
    let bounds = iced::Rectangle::new(iced::Point::new(10., 10.), iced::Size::new(80., 30.));
    let snapshot = Snapshot {
        nodes: vec![reshiki::accessibility::Node {
            id: id.into(),
            name: "Test control".into(),
            role,
            enabled: true,
            focused: true,
            checked: None,
            expanded: None,
            value: None,
            bounds,
            visible_bounds: Some(bounds),
        }],
        duplicate_ids: Vec::new(),
    };
    let tree = app
        .accessibility
        .tree
        .update(
            &snapshot,
            "ReShiki",
            iced::Rectangle::with_size(app.accessibility.viewport),
            1.,
        )
        .unwrap();
    accesskit::ActionRequest {
        action: if role == reshiki::accessibility::Role::Button {
            accesskit::Action::Click
        } else {
            accesskit::Action::SetValue
        },
        target_tree: accesskit::TreeId::ROOT,
        target_node: tree.focus,
        data: if role == reshiki::accessibility::Role::Button {
            None
        } else {
            Some(accesskit::ActionData::Value("stale caption".into()))
        },
    }
}

#[test]
fn native_callback_keeps_its_enqueue_context_across_a_document_revision() {
    let (mut app, _) = App::new();
    let _ = app.accessibility_refresh();
    let request = native_request(&mut app, "header-new", reshiki::accessibility::Role::Button);
    let (sender, mut receiver) = mpsc::channel(2);
    enqueue_native(
        &sender,
        &app.accessibility.context_epoch,
        app.accessibility.generation,
        request.clone(),
    );
    app.tab.revision += 1;
    let _ = app.accessibility_refresh();
    // Document edits keep the live control's ID. The enqueue stamp, not
    // accidental removal of the control, must reject this stale request.
    assert!(app.accessibility.tree.resolve(&request).is_some());
    assert_eq!(
        app.accessibility_action(receiver.try_recv().unwrap())
            .units(),
        0
    );
    enqueue_native(
        &sender,
        &app.accessibility.context_epoch,
        app.accessibility.generation,
        request,
    );
    assert!(
        app.accessibility_action(receiver.try_recv().unwrap())
            .units()
            > 0
    );
}

#[test]
fn cancelled_and_reopened_caption_rejects_old_native_and_dispatched_edits() {
    use super::super::inline_text::Action as Inline;
    use reshiki::document::Point;
    let (mut app, _) = App::new();
    let _ = app.update(Message::InlineText(Inline::Begin(
        None,
        Point::new(20., 30.),
    )));
    let version = app.accessibility.context_version;
    let document = (app.tab.id, app.tab.file_epoch, app.tab.revision);
    let selected = app.tab.selected.clone();
    let tool = app.tool;
    let request = native_request(
        &mut app,
        "inline-caption",
        reshiki::accessibility::Role::TextArea,
    );
    let (sender, mut receiver) = mpsc::channel(1);
    enqueue_native(
        &sender,
        &app.accessibility.context_epoch,
        app.accessibility.generation,
        request.clone(),
    );
    let _ = app.update(Message::InlineText(Inline::Finish(false)));
    assert!(
        app.accessibility
            .context
            .as_ref()
            .unwrap()
            .inline_session
            .is_none()
    );
    let _ = app.update(Message::InlineText(Inline::Begin(
        None,
        Point::new(60., 70.),
    )));
    assert_eq!(document, (app.tab.id, app.tab.file_epoch, app.tab.revision));
    assert_eq!(selected, app.tab.selected);
    assert_eq!(tool, app.tool);
    assert_ne!(version, app.accessibility.context_version);
    let reopened = native_request(
        &mut app,
        "inline-caption",
        reshiki::accessibility::Role::TextArea,
    );
    assert_ne!(request.target_node, reopened.target_node);
    assert!(app.accessibility.tree.resolve(&request).is_none());
    assert_eq!(
        app.accessibility_action(receiver.try_recv().unwrap())
            .units(),
        0
    );
    assert_eq!(
        app.accessibility_action(Action::Dispatch(
            version,
            Box::new(Message::InlineText(Inline::ReplaceText("old draft".into())))
        ))
        .units(),
        0
    );
    assert!(app.tab.caption.is_empty());

    // A new Begin may replace an empty draft in a single update, without
    // ever publishing a context in which the editor is absent.
    let version = app.accessibility.context_version;
    let _ = app.update(Message::InlineText(Inline::Begin(
        None,
        Point::new(60., 70.),
    )));
    assert_ne!(version, app.accessibility.context_version);
    assert!(app.accessibility.tree.resolve(&reopened).is_none());
}

#[test]
fn closing_during_install_waits_for_completion_before_destroying_the_host() {
    let (mut app, _) = App::new();
    let id = window::Id::unique();
    app.accessibility.window = Some(id);
    app.accessibility.installing = true;
    assert_eq!(app.accessibility_close(id).units(), 0);
    assert!(app.accessibility.closed);
    assert_eq!(app.accessibility.terminal, Some(Terminal::Close(id)));
    let generation = app.accessibility.generation;
    let task = app.accessibility_action(Action::Installed(
        generation,
        Err("injected install failure".into()),
    ));
    assert!(task.units() > 0);
    assert!(!app.accessibility.installing);
    assert!(app.accessibility.terminal.is_none());
}
#[test]
fn a_failed_install_is_not_retried_on_the_visible_fallback_window() {
    let (mut app, _) = App::new();
    app.accessibility.window = Some(window::Id::unique());
    app.accessibility.installing = true;
    let generation = app.accessibility.generation;
    let _ = app.accessibility_action(Action::Installed(
        generation,
        Err("injected install failure".into()),
    ));
    assert!(app.accessibility.install_failed);
    let sequence = app.accessibility.sequence;
    let task =
        app.accessibility_action(Action::Snapshot(generation, sequence, Snapshot::default()));
    assert_eq!(task.units(), 0);
    assert!(!app.accessibility.installing);
    assert!(app.accessibility.install_failed);
}
#[test]
fn queued_native_command_cannot_move_to_a_new_selection() {
    let (mut app, _) = App::new();
    let _ = app.accessibility_refresh();
    let version = app.accessibility.context_version;
    app.tab.selected.push(123);
    let _ = app.accessibility_refresh();
    assert_ne!(version, app.accessibility.context_version);
    let before = app.tab.id;
    assert_eq!(
        app.accessibility_action(Action::Dispatch(version, Box::new(Message::New)))
            .units(),
        0
    );
    assert_eq!(app.tab.id, before);
}
#[test]
fn updater_exit_uses_the_same_pending_install_barrier() {
    let (mut app, _) = App::new();
    app.accessibility.installing = true;
    assert_eq!(app.accessibility_exit().units(), 0);
    assert_eq!(app.accessibility.terminal, Some(Terminal::Exit));
    let generation = app.accessibility.generation;
    assert!(
        app.accessibility_action(Action::Installed(
            generation,
            Err("injected install failure".into())
        ))
        .units()
            > 0
    );
    assert!(app.accessibility.terminal.is_none());
}
