//! Newer on GitHub: a drawing someone changed there, found on window focus.
use super::*;

/// `ring.rsk` as @`login` changed it on GitHub: 30 atoms where it had 42,
/// asked against the base `app_in` opens it with.
fn changed_by(login: &str) -> Found {
    let theirs = drawing(30.);
    let conflict = send::Conflict {
        repo_path: "ring.rsk".into(),
        commit: sha('e'),
        blob: Some(sha('a')),
        theirs: Some(theirs),
        mine: Vec::new(),
        names: vec!["ring.rsk".into()],
    };
    let by = api::RemoteVersion {
        commit: sha('e'),
        message: "Add an ethyl group".into(),
        login: Some(login.into()),
        author: login.into(),
        date: "2026-11-16T14:02:00Z".into(),
    };
    Found {
        base: sha('b'),
        theirs: conflict,
        by: Some(by),
    }
}

fn found(app: &mut App, login: &str) {
    let generation = app.github.generation;
    github(
        app,
        Action::Newer(generation, repo().key, Ok(vec![changed_by(login)])),
    );
}

fn rows(app: &App) -> Vec<String> {
    (app.github_palette().into_iter())
        .map(|(name, ..)| name)
        .collect()
}

#[test]
fn a_drawing_changed_on_github_with_no_changes_here_waits_for_update() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    assert_eq!(app.github_chip_state(&repo().key), Chip::Sent);
    assert!(!rows(&app).contains(&"Update from GitHub".into()));
    found(&mut app, "alice");
    assert_eq!(app.github_chip_state(&repo().key), Chip::Newer);
    let tab = app.strip().find(|tab| tab.id == app.tab.id).unwrap();
    let (hint, spoken) = app
        .github_tab(tab, false)
        .map(|badge| (badge.hint, badge.spoken))
        .unwrap();
    assert_eq!(hint, "octo/lab · ring.rsk · Newer on GitHub");
    assert_eq!(spoken, "lab, newer on GitHub");
    // The palette's row, only while it applies.
    let update = (app.github_palette().into_iter())
        .find(|(name, ..)| name == "Update from GitHub")
        .unwrap();
    assert_eq!(update.1, "ring.rsk");
    assert!(matches!(update.2, Some(Action::Update)));
    // Nothing is written until Update.
    assert_eq!(std::fs::read(&path).unwrap(), drawing(42.));
    assert_eq!(
        file_state(&app, "ring.rsk").unwrap().base_blob,
        Some(sha('b'))
    );
    // A save here since: no Update, but Review, which shows the conflict.
    let mine = save_here(&mut app, &path);
    assert_eq!(app.github_chip_state(&repo().key), Chip::Review);
    assert!(!rows(&app).contains(&"Update from GitHub".into()));
    assert!(rows(&app).contains(&"Review changes on GitHub".into()));
    let _ = app.github_update();
    assert_eq!(std::fs::read(&path).unwrap(), mine);
    github(&mut app, Action::Review);
    assert_eq!(review(&app).name, "ring.rsk");
    let mark = file_state(&app, "ring.rsk").unwrap().conflict.unwrap();
    assert_eq!((mark.commit, mark.blob), (sha('e'), Some(sha('a'))));
}

#[test]
fn update_is_one_undo_step_to_githubs_drawing_exactly() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    let yours = app.tab.doc.clone();
    found(&mut app, "alice");
    github(&mut app, Action::Update);
    // The tab shows GitHub's, exactly as stored, and saved.
    let theirs = Document::from_native_file(&drawing(30.)).unwrap();
    assert_eq!(app.tab.doc, theirs);
    assert!(!app.dirty());
    assert_eq!(app.status, "Updated · @alice: Add an ethyl group");
    assert!(!app.error);
    // GitHub's is the working copy and its base: nothing to send.
    assert_eq!(std::fs::read(&path).unwrap(), drawing(30.));
    let file = file_state(&app, "ring.rsk").unwrap();
    assert_eq!(
        (file.base_commit, file.base_blob, file.base_hash),
        (sha('e'), Some(sha('a')), workcopy::hash(&drawing(30.)))
    );
    assert_eq!(app.github_chip_state(&repo().key), Chip::Sent);
    assert!(!rows(&app).contains(&"Update from GitHub".into()));
    // A late reply asked against the old base, or one kept from before:
    // the drawing has moved on, so no Update.
    found(&mut app, "alice");
    assert!(app.github.newer.is_empty(), "asked against the old base");
    app.github
        .newer
        .insert((repo().key, "ring.rsk".into()), changed_by("alice"));
    assert_eq!(app.github_chip_state(&repo().key), Chip::Sent);
    // One Undo brings yours back.
    let _ = app.update(Message::Undo);
    assert!(super::super::super::same_drawing(&app.tab.doc, &yours));
}

#[test]
fn your_own_commit_from_another_computer_is_newer_too_and_says_you() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _path) = app_in(dir.path());
    found(&mut app, "octo");
    assert_eq!(app.github_chip_state(&repo().key), Chip::Newer);
    github(&mut app, Action::Update);
    assert_eq!(app.status, "Updated · You: Add an ethyl group");
}

#[test]
fn with_changes_here_it_becomes_the_conflict_review() {
    // Saved here, not sent.
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    let mine = save_here(&mut app, &path);
    found(&mut app, "alice");
    assert_eq!(app.github_chip_state(&repo().key), Chip::Review);
    assert!(!rows(&app).contains(&"Update from GitHub".into()));
    assert!(rows(&app).contains(&"Review changes on GitHub".into()));
    let mark = file_state(&app, "ring.rsk").unwrap().conflict.unwrap();
    assert_eq!((mark.commit, mark.blob), (sha('e'), Some(sha('a'))));
    github(&mut app, Action::Review);
    let review = review(&app);
    assert_eq!(
        (review.name.as_str(), review.copy.as_str()),
        ("ring.rsk", "ring (2).rsk")
    );
    assert_eq!(app.github.conflicts[&repo().key][0].mine, mine);
    // Nothing is written until a choice.
    assert_eq!(std::fs::read(&path).unwrap(), mine);

    // Edited, not saved.
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _path) = app_in(dir.path());
    edit(&mut app);
    found(&mut app, "alice");
    assert_eq!(app.github_chip_state(&repo().key), Chip::Review);
    // A second check finds it again, and asks nothing new.
    found(&mut app, "alice");
    assert_eq!(app.github.conflicts[&repo().key].len(), 1);

    // While the repository sends, the send finds out instead.
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _path) = app_in(dir.path());
    app.github.sending.insert(repo().key);
    found(&mut app, "alice");
    assert!(app.github.newer.is_empty());
    assert!(!app.github.conflicts.contains_key(&repo().key));
}

#[test]
fn the_open_drawings_are_checked_at_most_every_five_minutes() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _path) = app_in(dir.path());
    let asked = |task: Task<Message>| iced_runtime::task::into_stream(task).is_some();
    assert!(asked(app.github_recheck()));
    let checked = app.github.checked;
    assert!(checked.is_some());
    // Not again within the window.
    assert!(!asked(app.github_recheck()));
    assert_eq!(app.github.checked, checked);
    // A reply asked under an earlier sign-in changes nothing.
    let generation = app.github.generation;
    github(
        &mut app,
        Action::Newer(
            generation.wrapping_sub(1),
            repo().key,
            Ok(vec![changed_by("alice")]),
        ),
    );
    assert_eq!(app.github_chip_state(&repo().key), Chip::Sent);
    // With no GitHub drawing open, nothing is asked and the window stays open.
    let (mut empty, _) = App::new();
    empty.github.client = app.github.client.clone();
    empty.github.account = app.github.account.clone();
    assert!(!asked(empty.github_recheck()));
    assert_eq!(empty.github.checked, None);
}
