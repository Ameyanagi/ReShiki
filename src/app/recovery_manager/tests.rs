use super::*;
use reshiki::{
    document::{Document, Point},
    recovery::{Recovery, Snapshot},
};

fn seeded() -> (tempfile::TempDir, Recovery, App) {
    let directory = tempfile::Builder::new()
        .prefix("reshiki-recovery-manager-test-")
        .tempdir()
        .unwrap();
    let store = Recovery::in_directory(directory.path()).unwrap();
    for (name, element, saved_at) in [("older.rsk", "O", 100), ("newest.rsk", "N", 200)] {
        let mut document = Document::default();
        document.add_atom(element, Point::default());
        let snapshot = Snapshot {
            document,
            source: Some(name.into()),
            saved_at,
        };
        std::fs::write(
            directory.path().join(format!("4294967294-{name}.json")),
            serde_json::to_vec(&snapshot).unwrap(),
        )
        .unwrap();
    }
    let (mut app, _) = App::new();
    app.tab.busy = false;
    // App::new disables recovery under cfg(test). Read actual valid files;
    // an empty startup state would not exercise the acceptance requirement.
    app.recovered = store.candidates();
    assert_eq!(app.recovered.len(), 2);
    (directory, store, app)
}

fn act(app: &mut App, action: Action) -> Task<Message> {
    app.update(Message::Recovery(action))
}

#[test]
fn review_lists_real_drafts_and_restores_only_checked_drawings() {
    let (_directory, store, mut app) = seeded();
    assert_eq!(title(&app.recovered[0]), "newest.rsk");
    let _ = act(&mut app, Action::Show);
    assert!(app.recovery_manager.selected.is_empty());
    let selected = app.recovered[1].path.clone();
    let _ = act(&mut app, Action::Toggle(selected.clone()));
    let _ = act(&mut app, Action::RestoreSelected);
    assert_eq!(app.strip().count(), 1);
    assert_eq!(app.tab.doc.atoms[0].element, "O");
    assert_eq!(app.recovered.len(), 1);
    assert_eq!(title(&app.recovered[0]), "newest.rsk");
    assert!(!app.recovery_manager.open());
    assert!(
        selected.exists(),
        "Original stays until the restored tab is durably autosaved"
    );
    assert_eq!(store.candidates().len(), 2);
}

#[test]
fn dismiss_and_cancel_keep_valid_draft_files() {
    let (_directory, store, mut app) = seeded();
    let _ = act(&mut app, Action::Discard);
    assert_eq!(
        store.candidates().len(),
        2,
        "Opening confirmation changes no files"
    );
    let _ = act(&mut app, Action::Close);
    assert_eq!(store.candidates().len(), 2);
    let _ = app.update(Message::DismissRecovery);
    assert!(app.recovered.is_empty());
    assert_eq!(store.candidates().len(), 2, "Dismiss affects memory only");
}

#[tokio::test]
async fn confirmed_discard_removes_real_files_and_a_fresh_scan_is_empty() {
    use iced::futures::StreamExt;
    let (_directory, store, mut app) = seeded();
    let paths: Vec<_> = app
        .recovered
        .iter()
        .map(|candidate| candidate.path.clone())
        .collect();
    let _ = act(&mut app, Action::Discard);
    let task = act(&mut app, Action::ConfirmDiscard);
    assert_eq!(
        app.recovered.len(),
        2,
        "Keep offers until durable completion"
    );
    assert!(app.recovery_manager.pending.is_some());
    let mut stream = iced_runtime::task::into_stream(task).unwrap();
    let mut completions = 0;
    while let Some(action) = stream.next().await {
        if let iced_runtime::Action::Output(message) = action {
            assert!(
                matches!(message, Message::Recovery(Action::Finished(..))),
                "Completion is app-owned, not tied to a tab"
            );
            let _ = app.update(message);
            completions += 1;
        }
    }
    assert_eq!(completions, 1);
    assert!(!app.error, "{}", app.status);
    assert!(app.recovered.is_empty());
    assert!(!app.recovery_manager.open());
    assert!(paths.iter().all(|path| !path.exists()));
    assert!(store.candidates().is_empty());
    assert!(app.status.contains("Trash"));
}

#[test]
fn failed_durable_deletion_retains_offers_and_accepts_a_retry() {
    let (_directory, store, mut app) = seeded();
    let _ = act(&mut app, Action::Discard);
    let _ = act(&mut app, Action::ConfirmDiscard);
    let serial = app.recovery_manager.pending.unwrap();
    let _ = act(
        &mut app,
        Action::Finished(
            serial,
            Outcome {
                errors: vec!["IndexedDB transaction aborted".into()],
                ..Outcome::default()
            },
        ),
    );
    assert_eq!(app.recovered.len(), 2);
    assert_eq!(store.candidates().len(), 2);
    assert!(app.error && app.status.contains("Try again"));
    assert!(app.recovery_manager.open());
    assert!(app.recovery_manager.pending.is_none());
    let _ = act(&mut app, Action::Discard);
    let _ = act(&mut app, Action::ConfirmDiscard);
    assert!(app.recovery_manager.pending.is_some());
    assert_ne!(app.recovery_manager.pending, Some(serial));
}

#[test]
fn restore_all_remains_available_and_modal_blocks_drawing_commands() {
    let (_directory, _store, mut app) = seeded();
    let _ = act(&mut app, Action::Show);
    let _ = app.update(Message::New);
    assert_eq!(app.strip().count(), 1);
    let _ = act(&mut app, Action::RestoreAll);
    assert_eq!(app.strip().count(), 2);
    assert!(app.recovered.is_empty());
    assert_eq!(app.tab.doc.atoms[0].element, "N");
    assert!(!app.recovery_manager.open());
}

#[test]
fn a_recovery_dialog_keeps_background_update_receipts_and_save_questions_working() {
    let (_directory, _store, mut app) = seeded();
    let _ = act(&mut app, Action::Show);
    app.updates.channel = reshiki::updates::Channel::Stable;
    let _ = app.update(Message::Updates(crate::app::updates::Action::Checked(
        0,
        Ok(reshiki::updates::Release {
            version: "999.0.0".into(),
        }),
    )));
    assert!(
        app.updates.available(),
        "The already-started update receipt is applied while draft review is open"
    );
    app.prompt = Some(crate::app::prompt::Prompt::Save("Untitled".into()));
    let _ = app.update(Message::Prompt(crate::app::prompt::Answer::Cancel));
    assert!(app.prompt.is_none());
    assert!(
        app.recovery_manager.open(),
        "Cancelling close leaves draft review open"
    );
}

#[test]
fn titles_dates_and_ages_are_bounded_and_do_not_expose_source_directories() {
    let (_directory, _store, mut app) = seeded();
    app.recovered[0].snapshot.source = Some("/synthetic-account/private/aspirin.rsk".into());
    assert_eq!(title(&app.recovered[0]), "aspirin.rsk");
    assert_eq!(date(0), "1970-01-01 00:00 UTC");
    assert_eq!(date(u64::MAX), "Unknown date");
    assert_eq!(age(100, 99), "just now");
    assert_eq!(age(0, 86400 * 4), "4 d ago");
}
