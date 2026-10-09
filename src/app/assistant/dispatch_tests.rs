//! Pins which assistant actions snap the chat to its end and which return no
//! task, so the assistant_action split can be checked against the original.

use super::*;

fn fresh() -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app
}

#[test]
fn reject_snaps_the_chat_and_state_only_actions_return_no_task() {
    let mut app = fresh();
    assert!(app.assistant_action(Action::Reject).units() > 0);
    for action in [
        Action::ClearImage,
        Action::Model(None),
        Action::Menu(Some(Menu::Models)),
        Action::Effort("high".into()),
        Action::Tier("default".into()),
        Action::PreferencesSaved(Ok(())),
        Action::AutoApply(false),
        Action::AutoApply(true),
        Action::Stop,
        Action::Reset,
        Action::Poll,
    ] {
        assert_eq!(app.assistant_action(action).units(), 0);
    }
}

#[test]
fn attachment_and_connection_results_return_no_task() {
    let mut app = fresh();
    let (serial, epoch) = (app.assistant.image_serial, app.tab.file_epoch);
    for text in [None, Some("Draw water".to_string())] {
        let task = app.assistant_action(Action::TextPasted {
            serial,
            epoch,
            text,
        });
        assert_eq!(task.units(), 0);
    }
    let late = Action::TextPasted {
        serial: serial.wrapping_add(1),
        epoch,
        text: None,
    };
    assert_eq!(app.assistant_action(late).units(), 0);
    let serial = app.assistant.serial;
    let late = Action::Connected(
        serial.wrapping_add(1),
        Err(codex::ConnectionError::AccountFailed),
    );
    assert_eq!(app.assistant_action(late).units(), 0);
    assert!(!app.assistant.error);
    let current = Action::Connected(serial, Err(codex::ConnectionError::ModelsFailed));
    assert_eq!(app.assistant_action(current).units(), 0);
    assert!(app.assistant.error);
}

#[test]
fn poll_done_and_apply_snap_only_while_following_the_chat() {
    for follow in [false, true] {
        let mut app = fresh();
        app.tab
            .doc
            .add_atom("C", reshiki::document::Point::default());
        app.assistant.follow_chat = follow;
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        app.assistant.progress = Some(rx);
        tx.try_send(codex::Progress::Plan("Plan".into())).unwrap();
        assert_eq!(app.assistant_action(Action::Poll).units() > 0, follow);
        let (serial, epoch, revision) =
            (app.assistant.serial, app.tab.file_epoch, app.tab.revision);
        let late = Action::Done {
            serial: serial.wrapping_add(1),
            epoch,
            revision,
            replace: vec![],
            result: Box::new(Err("late".into())),
        };
        assert_eq!(app.assistant_action(late).units(), 0);
        let done = Action::Done {
            serial,
            epoch,
            revision,
            replace: vec![],
            result: Box::new(Err("stopped".into())),
        };
        assert_eq!(app.assistant_action(done).units() > 0, follow);
        // Apply reads follow_chat first, but its early returns stay empty.
        assert_eq!(app.assistant_action(Action::Apply).units(), 0);
        let mut fragment = Document::default();
        fragment.add_atom("O", reshiki::document::Point::default());
        app.assistant.draft = Some(Draft {
            proposal: Proposal::default(),
            fragment,
            review: assistant::review::Report::default(),
            revision,
            epoch,
            replace: vec![],
        });
        assert_eq!(app.assistant_action(Action::Apply).units() > 0, follow);
        assert!(app.assistant.completed.is_some());
    }
}

#[test]
fn request_preflight_changes_replace_stale_ready_setup_state() {
    use super::setup::Connection;
    for (event, expected) in [
        (
            codex::Progress::Catalog(codex::Account {
                connected: false,
                models: vec![],
            }),
            Connection::SignInRequired,
        ),
        (
            codex::Progress::ConnectionFailed(codex::ConnectionError::MissingInstallation),
            Connection::Failed(codex::ConnectionError::MissingInstallation),
        ),
    ] {
        let mut app = fresh();
        let before = app.tab.doc.clone();
        app.assistant.connection = Connection::Ready;
        app.assistant.account = Some(codex::Account {
            connected: true,
            models: vec![],
        });
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        app.assistant.progress = Some(rx);
        tx.try_send(event).unwrap();
        let _ = app.assistant_action(Action::Poll);
        assert_eq!(app.assistant.connection, expected);
        assert!(
            !app.assistant
                .account
                .as_ref()
                .is_some_and(|account| account.connected)
        );
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.assistant_action(Action::Send).units(), 0);
        assert!(app.assistant_action(Action::Connect).units() > 0);
    }
}
