//! Working-copy reads and writes off the UI thread: what lands late, and
//! what waits for a write.
use super::*;

#[test]
fn the_read_begun_last_wins_and_a_write_beats_older_reads() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    let key = repo().key;
    let (synced, saved) = (workcopy::hash(&drawing(42.)), workcopy::hash(&drawing(50.)));
    let read = |hash: &String| vec![("ring.rsk".to_owned(), Some(hash.clone()))];
    // Two reads begin, the first before a save, the second after it.
    let (first, second) = (app.github.seq + 1, app.github.seq + 2);
    app.github.seq = second;
    // The later read lands first; the older one, landing late, is dropped.
    github(&mut app, Action::Scanned(key.clone(), second, read(&saved)));
    github(&mut app, Action::Scanned(key.clone(), first, read(&synced)));
    assert_eq!(unsent(&app), ["ring.rsk"]);
    // A write made here is newer than every read begun before it.
    app.github.wrote(&key, "ring.rsk", Some(synced));
    github(&mut app, Action::Scanned(key, second, read(&saved)));
    assert!(unsent(&app).is_empty());
}

#[tokio::test]
async fn a_save_reads_back_only_its_own_drawing() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    let other = open_other(&mut app, dir.path());
    read_back(&mut app);
    // Changed in another app: a save of ring.rsk does not look at it.
    std::fs::write(&other, drawing(70.)).unwrap();
    std::fs::write(&path, drawing(50.)).unwrap();
    let doc = Document::from_native_file(&drawing(50.)).unwrap();
    app.github_unread(&path);
    let task = app.github_saved(true, Some((path.clone(), doc)));
    // Counted at once, before it is read back, so the count never dips.
    assert_eq!(unsent(&app), ["ring.rsk"]);
    run(&mut app, task).await;
    assert_eq!(unsent(&app), ["ring.rsk"]);
    // Back in the window, every drawing is read again.
    let task = app.update(Message::GitHub(Action::Focused));
    run(&mut app, task).await;
    assert_eq!(unsent(&app), ["b.rsk", "ring.rsk"]);
}

#[tokio::test]
async fn opening_from_the_picker_writes_off_the_ui_thread() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    app.github.repos = vec![repo()];
    github(
        &mut app,
        Action::Picker(picker::Action::Link(
            "https://github.com/octo/lab/blob/main/new.rsk".into(),
        )),
    );
    let download = api::Download {
        commit: sha('d'),
        blob: sha('e'),
        bytes: drawing(30.),
    };
    let request = app.github.seq;
    let task = app.update(Message::GitHub(Action::Picker(picker::Action::Downloaded(
        request,
        repo(),
        "new.rsk".into(),
        false,
        Ok(download.clone()),
    ))));
    let path = workcopy::working_path(dir.path(), &repo().key, "new.rsk");
    assert!(!path.exists(), "not written by the update itself");
    assert!(file_state(&app, "new.rsk").is_none());
    assert_eq!(app.github.writes, 1);
    run(&mut app, task).await;
    // Done once `state.json` keeps it too.
    assert_eq!(app.github.writes, 0);
    let kept = workcopy::load(dir.path(), &repo().key).unwrap().unwrap();
    assert!(kept.files.contains_key("new.rsk"));
    assert_eq!(std::fs::read(&path).unwrap(), download.bytes);
    let file = file_state(&app, "new.rsk").unwrap();
    assert_eq!(
        (file.base_commit, file.base_blob),
        (sha('d'), Some(sha('e')))
    );
    assert!(app.github.dialog.is_none());
    assert_eq!(app.tab.path.as_deref(), Some(path.as_path()));
    assert!(unsent(&app).is_empty());
}

#[test]
fn a_save_that_goes_on_with_a_close_still_asks_to_send() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    edit(&mut app);
    let id = app.tab.id;
    // Close, answered Save: the save lands, then the close goes on.
    app.pending = Some(crate::app::Pending::CloseTab(id));
    std::fs::write(&path, app.tab.doc.file_json().unwrap()).unwrap();
    (app.file_io.saving, app.file_io.saving_tab) = (true, Some(id));
    let (epoch, doc) = (app.tab.file_epoch, Box::new(app.tab.doc.clone()));
    let _ = app.update(Message::Saved(epoch, doc, Ok(Some(path))));
    assert!(matches!(app.github.closing, Some(Closing::Tab(asked, _)) if asked == id));
    assert!(app.strip().any(|tab| tab.id == id), "still open");
}

#[test]
fn what_a_send_read_counts_as_read_when_it_began() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    let sent = |app: &mut App, bytes: &[u8]| {
        let outcome = SendOutcome {
            repo: repo(),
            commit: sha('d'),
            sent: vec![("ring.rsk".into(), sha('e'), workcopy::hash(bytes))],
            conflicts: Vec::new(),
        };
        github(
            app,
            Action::Sent(repo().key, workcopy::now_id(), Ok(outcome)),
        );
    };
    // Changed in another app, not read here yet; a Send reads and sends it.
    std::fs::write(&path, drawing(50.)).unwrap();
    let _ = app.github_send(repo().key, "Update ring.rsk".into(), None);
    sent(&mut app, &drawing(50.));
    assert!(unsent(&app).is_empty(), "sent as read");
    // A save read back after a Send began is newer than what it sent.
    let _ = app.github_send(repo().key, "Update ring.rsk".into(), None);
    std::fs::write(&path, drawing(60.)).unwrap();
    read_back(&mut app);
    sent(&mut app, &drawing(50.));
    assert_eq!(unsent(&app), ["ring.rsk"]);
}
