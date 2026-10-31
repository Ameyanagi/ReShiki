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

fn recovery_app() -> App {
    let (mut app, _) = App::new();
    app.recovered.push(reshiki::recovery::Candidate {
        path: "4294967294-accessibility.json".into(),
        snapshot: reshiki::recovery::Snapshot {
            document: reshiki::document::Document::default(),
            source: None,
            saved_at: 0,
        },
    });
    app
}

#[test]
fn queued_recovery_controls_cannot_act_on_changed_checks_or_reused_rows() {
    use super::super::recovery_manager::Action as Recovery;
    let mut app = recovery_app();
    let _ = app.update(Message::Recovery(Recovery::Show));
    let request = native_request(
        &mut app,
        "recovery-draft-0",
        reshiki::accessibility::Role::Button,
    );
    let (sender, mut receiver) = mpsc::channel(1);
    enqueue_native(
        &sender,
        &app.accessibility.context_epoch,
        app.accessibility.generation,
        request.clone(),
    );
    let version = app.accessibility.context_version;
    let path = app.recovered[0].path.clone();
    let _ = app.update(Message::Recovery(Recovery::Toggle(path)));
    assert_ne!(version, app.accessibility.context_version);
    assert!(app.accessibility.tree.resolve(&request).is_none());
    assert_eq!(
        app.accessibility_action(receiver.try_recv().unwrap())
            .units(),
        0
    );

    // A later owner scan can replace row zero while the picker stays open.
    let version = app.accessibility.context_version;
    app.recovered[0].path = "4294967294-replacement.json".into();
    let _ = app.accessibility_refresh();
    assert_ne!(version, app.accessibility.context_version);
}

#[test]
fn app_owned_recovery_completion_refreshes_the_closed_native_dialog() {
    use super::super::recovery_manager::{Action as Recovery, Outcome};
    let mut app = recovery_app();
    app.accessibility.window = Some(window::Id::unique());
    let _ = app.update(Message::Recovery(Recovery::Discard));
    // Do not poll the deletion task: this test covers the app-owned receipt,
    // while the recovery manager tests exercise real durable file removal.
    let _ = app.update(Message::Recovery(Recovery::ConfirmDiscard));
    let sequence = app.accessibility.sequence;
    let version = app.accessibility.context_version;
    let task = app.update(Message::Recovery(Recovery::Finished(1, Outcome::default())));
    assert!(!app.recovery_manager.open());
    assert_ne!(version, app.accessibility.context_version);
    if SUPPORTED {
        assert!(app.accessibility.sequence > sequence);
        assert!(
            task.units() > 0,
            "Collect a new native accessibility snapshot"
        );
    }
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

#[cfg(any(target_os = "macos", windows))]
mod hover_controls {
    use super::*;
    use crate::canvas::{Camera, Edit};
    use iced::futures::StreamExt;
    use reshiki::document::{Document, Point};
    use reshiki::keyboard_drawing::Target;

    fn app(doc: Document) -> App {
        let (mut app, _) = App::new();
        app.recovered.clear();
        app.tab.doc = doc;
        app.tab.saved = app.tab.doc.clone();
        app.tab.busy = false;
        app.tab.labels_dirty = false;
        app.sync_keyboard_drawing();
        app.accessibility.window = Some(window::Id::unique());
        let _ = app.accessibility_refresh();
        app
    }

    // Run actual returned tasks, including finishing their Widget operations.
    // Collect emits a Snapshot round trip even with no widget nodes; this test
    // needs no native OS adapter and does not publish the empty snapshot.
    async fn work(task: Task<Message>) -> (usize, usize) {
        let mut widgets = 0;
        let mut snapshots = 0;
        if let Some(mut stream) = iced_runtime::task::into_stream(task) {
            while let Some(action) = stream.next().await {
                match action {
                    iced_runtime::Action::Widget(operation) => {
                        widgets += 1;
                        let _ = operation.finish();
                    }
                    iced_runtime::Action::Output(Message::Accessibility(Action::Snapshot(..))) => {
                        snapshots += 1;
                    }
                    _ => {}
                }
            }
        }
        (widgets, snapshots)
    }

    #[tokio::test]
    async fn blank_motion_keeps_current_position_without_collect_or_snapshot_tasks() {
        let mut app = app(Document::default());
        app.tab.camera = Camera {
            center: Point::new(10., -20.),
            zoom: 2.,
        };
        let original = app.tab.doc.clone();
        let revision = app.tab.revision;
        let sequence = app.accessibility.sequence;
        let version = app.accessibility.context_version;
        for (edit, expected) in [
            (
                Edit::Hover(Some(Point::new(200., 200.))),
                Point::new(200., 200.),
            ),
            (
                Edit::PointerHover(Some(iced::Vector::new(400., 480.))),
                Point::new(210., 220.),
            ),
        ] {
            let task = app.update(Message::Canvas(edit));
            let counts = work(task).await;
            println!("blank hover widget/snapshot tasks: {counts:?}");
            assert_eq!(counts, (0, 0));
            assert_eq!(app.accessibility.sequence, sequence);
            assert_eq!(app.accessibility.context_version, version);
            assert_eq!(app.tab.keyboard_drawing.target(), Target::Blank(expected));
            assert_eq!(app.tab.hover, Some((expected, app.tab.file_epoch)));
        }
        assert_eq!(app.tab.doc, original);
        assert_eq!(app.tab.revision, revision);
        assert!(app.tab.selected.is_empty());
        assert!(!app.tab.history.can_undo());
        assert!(!app.tab.history.can_redo());
        assert_eq!(app.tab.camera.center, Point::new(10., -20.));
        assert_eq!(app.tab.camera.zoom, 2.);
    }

    #[tokio::test]
    async fn target_identity_changes_still_collect_and_unchanged_atoms_and_bonds_skip() {
        let mut doc = Document::default();
        let a = doc.add_atom("C", Point::new(0., 0.));
        let b = doc.add_atom("C", Point::new(42., 0.));
        let c = doc.add_atom("C", Point::new(84., 0.));
        doc.add_bond(a, b, 1, "plain");
        doc.add_bond(b, c, 1, "plain");
        let mut app = app(doc);
        let original = app.tab.doc.clone();
        for (point, expected, refresh) in [
            (
                Point::new(200., 200.),
                Target::Blank(Point::new(200., 200.)),
                1,
            ),
            (
                Point::new(201., 200.),
                Target::Blank(Point::new(201., 200.)),
                0,
            ),
            (Point::new(0., 0.), Target::Atom(a), 1),
            (Point::new(0.2, 0.), Target::Atom(a), 0),
            (Point::new(42., 0.), Target::Atom(b), 1),
            (Point::new(21., 0.), Target::Bond(a, b), 1),
            (Point::new(22., 0.), Target::Bond(a, b), 0),
            (Point::new(63., 0.), Target::Bond(b, c), 1),
            (
                Point::new(300., 300.),
                Target::Blank(Point::new(300., 300.)),
                1,
            ),
        ] {
            let sequence = app.accessibility.sequence;
            let counts = work(app.update(Message::Canvas(Edit::Hover(Some(point))))).await;
            assert_eq!(counts, (refresh, refresh), "{expected:?}");
            assert_eq!(app.accessibility.sequence, sequence + refresh as u64);
            assert_eq!(app.tab.keyboard_drawing.target(), expected);
        }
        assert_eq!(app.tab.doc, original);
        assert!(!app.tab.history.can_undo());
    }

    #[tokio::test]
    async fn a_mark_reconciled_by_hover_still_refreshes_the_native_controls() {
        let mut app = app(Document::default());
        app.tab.keyboard_drawing.mark(u64::MAX);
        let sequence = app.accessibility.sequence;
        assert_eq!(
            work(app.update(Message::Canvas(Edit::Hover(Some(Point::new(200., 200.)))))).await,
            (1, 1)
        );
        assert_eq!(app.accessibility.sequence, sequence + 1);
        assert_eq!(app.tab.keyboard_drawing.marked(), None);
    }

    #[tokio::test]
    async fn menu_pointer_handoff_skips_refresh_but_submenu_hover_still_collects() {
        use super::super::super::context_menu::{Action as MenuAction, Page};

        let mut app = app(Document::default());
        let _ = work(app.update(Message::ContextMenu(MenuAction::Open(Page::Main, 10.)))).await;
        let _ = work(app.update(Message::ContextMenu(MenuAction::Key(
            iced::keyboard::key::Named::Home,
        ))))
        .await;
        let sequence = app.accessibility.sequence;
        let version = app.accessibility.context_version;
        assert_eq!(
            work(app.update(Message::ContextMenu(MenuAction::Pointer))).await,
            (0, 0)
        );
        assert_eq!(app.accessibility.sequence, sequence);
        assert_eq!(app.accessibility.context_version, version);

        // The empty-selection Main menu's Copy as row opens a child; Paste
        // closes it. Both alter the native controls, unlike pointer ownership.
        for index in [4, 3] {
            let sequence = app.accessibility.sequence;
            assert_eq!(
                work(app.update(Message::ContextMenu(MenuAction::Hover(0, index)))).await,
                (1, 1)
            );
            assert_eq!(app.accessibility.sequence, sequence + 1);
        }
        assert!(app.context_menu.is_some());
        assert!(app.tab.doc.atoms.is_empty());
        assert!(!app.tab.history.can_undo());
    }

    #[tokio::test]
    async fn queued_blank_action_uses_latest_point_and_popup_boundaries_still_reject_stale_actions()
    {
        let mut app = app(Document::default());
        let _ = work(app.update(Message::Canvas(Edit::Hover(Some(Point::new(200., 200.)))))).await;
        let version = app.accessibility.context_version;
        assert_eq!(
            work(app.update(Message::Canvas(Edit::PointerHover(Some(
                iced::Vector::new(250., 230.)
            )))))
            .await,
            (0, 0)
        );
        assert_eq!(app.accessibility.context_version, version);
        let _ = app.accessibility_action(Action::Dispatch(
            version,
            Box::new(Message::ContextKey("n".into())),
        ));
        assert_eq!(app.tab.doc.atoms.len(), 1);
        assert_eq!(app.tab.doc.atoms[0].element, "N");
        assert_eq!(app.tab.doc.atoms[0].position, Point::new(250., 230.));
        assert_eq!(app.tab.history.undo_frames().count(), 1);

        let mut app = self::app(Document::default());
        let version = app.accessibility.context_version;
        let _ = app.update(Message::Commands(
            super::super::super::command_palette::Action::Open,
        ));
        assert_ne!(app.accessibility.context_version, version);
        let tab = app.tab.id;
        assert_eq!(
            app.accessibility_action(Action::Dispatch(version, Box::new(Message::New)))
                .units(),
            0
        );
        assert_eq!(app.tab.id, tab);
        let open_version = app.accessibility.context_version;
        let _ = app.update(Message::Commands(
            super::super::super::command_palette::Action::Close,
        ));
        assert_ne!(app.accessibility.context_version, open_version);
        assert_eq!(
            app.accessibility_action(Action::Dispatch(open_version, Box::new(Message::New)))
                .units(),
            0
        );
        assert_eq!(app.tab.id, tab);
    }
}
