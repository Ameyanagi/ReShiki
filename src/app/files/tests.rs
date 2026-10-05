use super::*;
use crate::app::{App, Message, Pending, inline_text};
use reshiki::document::{Document, Point};
use rfd::MessageDialogResult as Answer;

fn prepared() -> Opened {
    let mut document = Document::default();
    document.add_atom("O", Point::default());
    Some((
        "oxygen.rsk".into(),
        Ok(Prepared::Native(Box::new(document))),
    ))
}

#[test]
fn opening_takes_an_empty_tab_and_keeps_edited_and_drafting_tabs() {
    let (mut app, _) = App::new();
    let _ = app.update(Message::FilePrepared(prepared()));
    assert!(
        app.tabs.background.is_empty(),
        "The empty startup tab is reused"
    );
    assert_eq!(app.tab.path, Some("oxygen.rsk".into()));
    for inline in [false, true] {
        let (mut app, _) = App::new();
        if inline {
            let _ = app.inline_action(inline_text::Action::Begin(None, Point::default()));
        } else {
            let before = app.tab.doc.clone();
            app.tab.doc.add_atom("N", Point::default());
            app.changed(before);
        }
        let expected = app.tab.doc.clone();
        let _ = app.update(Message::FilePrepared(prepared()));
        assert_eq!(app.tab.path, Some("oxygen.rsk".into()));
        assert!(!app.error);
        let [kept] = app.tabs.background.as_slice() else {
            panic!("One tab behind the opened file");
        };
        assert_eq!(kept.doc, expected);
        assert_eq!(kept.inline_text.is_some(), inline);
        assert!(kept.path.is_none());
    }
}

#[test]
fn open_errors_and_files_already_open_add_no_tab() {
    let (mut app, _) = App::new();
    let _ = app.update(Message::FilePrepared(prepared()));
    let _ = app.update(Message::New);
    assert_eq!(app.tabs.active, 1);
    let _ = app.update(Message::FilePrepared(Some((
        "old.rsk".into(),
        Err("Old error".into()),
    ))));
    assert!(app.error && app.status == "Old error");
    let _ = app.update(Message::FilePrepared(prepared()));
    assert_eq!(app.strip().count(), 2);
    assert_eq!(app.tabs.active, 0, "The open file's tab comes to the front");
    assert_eq!(app.status, "oxygen.rsk is already open");
}

#[test]
fn native_worker_rejects_invalid_drawing_and_preserves_import_format() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let (_, invalid) = runtime
        .block_on(prepare_contents("bad.rsk".into(), Ok(b"not JSON".to_vec())))
        .unwrap();
    assert!(invalid.unwrap_err().contains("Could not open document"));
    let (_, imported) = runtime
        .block_on(prepare_contents(
            "example.CDXML".into(),
            Ok(b"<CDXML/>".to_vec()),
        ))
        .unwrap();
    assert!(
        matches!(imported.unwrap(), Prepared::Import { format: "cdxml", contents } if contents == "<CDXML/>")
    );
}

#[test]
fn save_dialog_answers_map_to_save_discard_and_cancel() {
    for answer in [Answer::Custom(SAVE.into()), Answer::Yes] {
        assert!(matches!(save_answer(answer), Message::Save));
    }
    for answer in [Answer::Custom(DONT_SAVE.into()), Answer::No] {
        assert!(matches!(save_answer(answer), Message::Discard));
    }
    for answer in [Answer::Custom("Cancel".into()), Answer::Cancel, Answer::Ok] {
        assert!(matches!(save_answer(answer), Message::Cancel));
    }
}

#[test]
fn saves_are_serial_and_cancelled_save_releases_pending_slot() {
    let (mut app, _) = App::new();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("drawing.rsk");
    app.tab.path = Some(path.clone());
    let task = app.update(Message::Save);
    assert!(task.units() > 0);
    assert!(app.file_io.saving);
    assert!(
        !path.exists(),
        "dispatch cannot perform filesystem work synchronously"
    );
    let second = app.update(Message::Save);
    assert_eq!(second.units(), 0);
    let _ = app.update(Message::Saved(
        app.tab.file_epoch,
        Box::new(app.tab.doc.clone()),
        Ok(None),
    ));
    assert!(!app.file_io.saving);
    assert!(!path.exists());
}

#[test]
fn microsoft_365_save_waits_for_receipt_and_failure_cancels_close_without_clearing_dirty() {
    let (mut app, _) = App::new();
    let path = PathBuf::from("recovery/drawing.rsk");
    app.tab.path = Some(path.clone());
    app.office_path = Some(path);
    app.office_host = "Microsoft 365";
    app.tab.doc.add_atom("O", Point::default());
    let snapshot = app.tab.doc.clone();
    let saved_before = app.tab.saved.clone();
    let _ = app.update(Message::Tabs(crate::app::tabs::Action::Close(None)));
    let task = app.update(Message::Save);
    assert!(task.units() > 0 && app.file_io.saving);
    assert!(app.status.contains("waiting for Microsoft 365"));
    assert!(app.dirty());
    let _ = app.update(Message::Saved(
        app.tab.file_epoch,
        Box::new(snapshot.clone()),
        Err("Draft saved; Microsoft 365 has not confirmed the update".into()),
    ));
    assert!(app.pending.is_none() && !app.file_io.saving);
    assert_eq!(app.tab.doc, snapshot);
    assert_eq!(app.tab.saved, saved_before);
    assert!(app.dirty() && app.error);
    assert!(!app.status.contains("Drawing updated"));
}

#[test]
fn stale_save_results_preserve_the_new_documents_pending_action() {
    for result in [
        Ok(Some("old.rsk".into())),
        Ok(None),
        Err("Old failure".into()),
    ] {
        let (mut app, _) = App::new();
        let epoch = app.tab.file_epoch;
        let snapshot = app.tab.doc.clone();
        let _ = app.update(Message::Save);
        app.reset_tab();
        app.tab.doc.add_atom("O", Point::default());
        let _ = app.update(Message::Tabs(crate::app::tabs::Action::Close(None)));
        app.status = "Current drawing".into();
        let before = app.tab.doc.clone();
        let _ = app.update(Message::Saved(epoch, Box::new(snapshot), result));
        assert!(!app.file_io.saving);
        assert!(matches!(app.pending, Some(Pending::CloseTab(_))));
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.status, "Current drawing");
        assert!(!app.error && app.tab.path.is_none());
        // The outstanding dialog still answers the new drawing's request.
        let _ = app.update(Message::Discard);
        assert!(app.tab.doc.all_ids().is_empty() && app.pending.is_none());
    }
}

#[test]
fn edits_during_a_save_reopen_the_pending_dialog() {
    let (mut app, _) = App::new();
    app.tab.doc.add_atom("O", Point::default());
    let snapshot = app.tab.doc.clone();
    let _ = app.update(Message::Tabs(crate::app::tabs::Action::Close(None)));
    let _ = app.update(Message::Save);
    let before = app.tab.doc.clone();
    app.tab.doc.add_atom("N", Point::new(80., 0.));
    app.changed(before);
    let edited = app.tab.doc.clone();
    let task = app.update(Message::Saved(
        app.tab.file_epoch,
        Box::new(snapshot.clone()),
        Ok(Some("drawing.rsk".into())),
    ));
    assert!(task.units() > 0, "Ask about the edits made during the save");
    assert!(matches!(app.pending, Some(Pending::CloseTab(_))));
    assert_eq!(app.tab.saved, snapshot);
    assert_eq!(app.tab.doc, edited);
    assert!(app.dirty());
    let _ = app.update(Message::Cancel);
    assert!(app.pending.is_none());
    assert_eq!(app.tab.doc, edited);
}
