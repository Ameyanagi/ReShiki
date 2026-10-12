use super::*;

#[tokio::test]
#[ignore = "Opt-in real Assistant widget-tree accessibility check"]
async fn setup_controls_expose_live_state_and_next_actions() {
    use iced::advanced::{renderer::Headless, widget::operation};
    use iced_runtime::{UserInterface, user_interface::Cache};
    use reshiki::accessibility::Collect;

    let mut renderer =
        <iced::Renderer as Headless>::new(iced::Font::default(), iced::Pixels(16.), None)
            .await
            .unwrap();
    let size = iced::Size::new(400., 850.);
    let (mut app, _) = App::new();
    for state in [
        Connection::Ready,
        Connection::SignInRequired,
        Connection::Failed(codex::ConnectionError::HandshakeFailed),
        Connection::Checking,
    ] {
        app.assistant.connection = state;
        app.assistant.busy = state == Connection::Checking;
        let mut ui = UserInterface::build(app.assistant_panel(), size, Cache::new(), &mut renderer);
        let mut collect = Collect::new(iced::Rectangle::with_size(size));
        ui.operate(&renderer, &mut operation::black_box(&mut collect));
        let snapshot = collect.snapshot();
        assert!(snapshot.duplicate_ids.is_empty());
        for id in ["assistant.setup.help", "assistant.setup.drawing"] {
            assert!(
                snapshot
                    .nodes
                    .iter()
                    .any(|node| node.id == id && node.enabled)
            );
        }
        let next = if state == Connection::Checking {
            "assistant.setup.cancel"
        } else {
            "assistant.setup.test"
        };
        assert!(
            snapshot
                .nodes
                .iter()
                .any(|node| node.id == next && node.enabled)
        );
        assert_eq!(
            snapshot
                .nodes
                .iter()
                .any(|node| node.id == "assistant.setup.example"),
            state == Connection::Ready
        );
        if state != Connection::Checking {
            assert!(
                snapshot
                    .nodes
                    .iter()
                    .find(|node| node.id == next)
                    .unwrap()
                    .value
                    .as_ref()
                    .unwrap()
                    .contains("through Codex")
            );
        }
    }
}

#[test]
fn example_is_local_and_preserves_the_drawing_until_apply() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.assistant.preferences.auto_apply = true;
    let original = app.tab.doc.clone();
    let task = app.assistant_action(Action::ImageExample);
    assert!(task.units() > 0); // Only local chat scrolling; no model request.
    assert!(!app.assistant.busy);
    assert!(app.assistant.guided_example);
    assert!(app.assistant.source_image.is_some());
    assert_eq!(app.tab.doc, original);
    let doc = reference().unwrap();
    assert_eq!(
        doc.atoms
            .iter()
            .map(|a| a.element.as_str())
            .collect::<Vec<_>>(),
        ["C", "C", "O"]
    );
    assert_eq!(doc.bonds.len(), 2);
    assert_eq!(
        doc.bonds.iter().map(|b| (b.a, b.b)).collect::<Vec<_>>(),
        [(1, 2), (2, 3)]
    );
    assert!(doc.bonds.iter().all(|b| b.order == 1));
    assert!(
        doc.atoms
            .iter()
            .all(|a| a.charge == 0 && a.stereo.is_none())
    );
    let _ = app.assistant_done(
        app.assistant.serial,
        app.tab.file_epoch,
        app.tab.revision,
        vec![],
        Box::new(Ok(reshiki::assistant::review::Outcome {
            proposal: Default::default(),
            document: doc,
            review: reshiki::assistant::review::Report {
                verified: true,
                ..Default::default()
            },
        })),
    );
    assert!(app.assistant.draft.is_some());
    assert_eq!(app.tab.doc, original);
    // Changing the next attachment must not let the preference apply the
    // already-completed guided draft from the previous source.
    let source = app.assistant.source_image.clone().unwrap();
    let _ = app.assistant_action(Action::ImageRead {
        serial: app.assistant.image_serial,
        epoch: app.tab.file_epoch,
        image_only: true,
        result: Ok(Some(source)),
    });
    assert!(!app.assistant.guided_example);
    assert!(app.assistant.requires_apply);
    assert_eq!(app.assistant_action(Action::AutoApply(true)).units(), 0);
    assert_eq!(app.tab.doc, original);
    let _ = app.assistant_action(Action::Apply);
    assert_eq!(app.tab.doc.atoms.len(), 3);
    let reopened = Document::from_json(&serde_json::to_vec(&app.tab.doc).unwrap()).unwrap();
    assert_eq!(reopened, app.tab.doc);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
}

#[test]
fn setup_help_runs_in_a_task_and_only_failure_changes_status() {
    for help in [Help::Setup, Help::Install, Help::SignIn] {
        let (mut app, _) = App::new();
        app.assistant.status = "Existing status".into();
        app.assistant.error = true;
        let task = app.assistant_action(Action::SetupHelp(help));
        assert!(task.units() > 0);
        assert_eq!(app.assistant.status, "Existing status");
        assert!(app.assistant.error);

        let task = app.assistant_action(Action::SetupHelpOpened(help, Ok(())));
        assert_eq!(task.units(), 0);
        assert_eq!(app.assistant.status, "Existing status");
        assert!(app.assistant.error);

        app.assistant.error = false;
        let task = app.assistant_action(Action::SetupHelpOpened(
            help,
            Err("Browser launch failed".into()),
        ));
        assert_eq!(task.units(), 0);
        assert_eq!(
            app.assistant.status,
            format!("Could not open the browser. Visit {}", help.url())
        );
        assert!(app.assistant.error);
    }
}
