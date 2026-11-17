//! Newer on GitHub: a drawing someone changed there, found on window focus.
use super::*;

/// `ring.rsk` as @`login` changed it on GitHub: 30 atoms where it had 42,
/// asked against the base commit `app_in` opens it with.
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
        base: sha('c'),
        theirs: conflict,
        by: Some(by),
    }
}

fn found(app: &mut App, login: &str) {
    let generation = app.github.generation;
    github(
        app,
        Action::Newer(
            generation,
            repo().key,
            Ok((repo(), vec![changed_by(login)])),
        ),
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
    landed(&mut app, Action::Update);
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
    landed(&mut app, Action::Update);
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
    landed(&mut app, Action::Update);
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

    // A send that found it in conflict since (here with GitHub at another
    // version) leaves only that conflict to review.
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    found(&mut app, "alice");
    conflicted(&mut app, &path, Some(drawing(20.)));
    app.github.sending.clear();
    let _ = app.update(Message::Escape);
    github(&mut app, Action::Review);
    let conflicts = &app.github.conflicts[&repo().key];
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].commit, sha('f'));
    assert_eq!(
        file_state(&app, "ring.rsk")
            .unwrap()
            .conflict
            .unwrap()
            .commit,
        sha('f')
    );

    // A save made elsewhere since: Update keeps it, and the chip says Review.
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    found(&mut app, "alice");
    std::fs::write(&path, drawing(50.)).unwrap();
    landed(&mut app, Action::Update);
    assert_eq!(std::fs::read(&path).unwrap(), drawing(50.));
    assert_eq!(app.github_chip_state(&repo().key), Chip::Review);

    // Sent back to the very bytes it was based on: a new base all the same.
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _path) = app_in(dir.path());
    found(&mut app, "alice");
    let state = app.github.states.get_mut(&repo().key).unwrap();
    state.files.get_mut("ring.rsk").unwrap().base_commit = sha('9');
    assert_eq!(app.github_chip_state(&repo().key), Chip::Sent);

    // Access taken away since (main's listing makes it read only): nothing
    // to update or review here, and a late reply is dropped.
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _path) = app_in(dir.path());
    found(&mut app, "alice");
    let generation = app.github.generation;
    let account = gh::Account {
        login: "octo".into(),
        repos: vec![Repo {
            push: false,
            ..repo()
        }],
    };
    github(&mut app, Action::Listed(generation, Ok(account)));
    assert!(app.github.read_only(&repo().key));
    assert!(app.github_newer(app.tab.path.as_deref()).is_none());
    assert!(!rows(&app).contains(&"Update from GitHub".into()));
    app.github.newer.clear();
    found(&mut app, "alice");
    assert!(app.github.newer.is_empty());
    // With changes here, found before the listing: no Review either.
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    save_here(&mut app, &path);
    found(&mut app, "alice");
    assert!(rows(&app).contains(&"Review changes on GitHub".into()));
    let generation = app.github.generation;
    let account = gh::Account {
        login: "octo".into(),
        repos: vec![Repo {
            push: false,
            ..repo()
        }],
    };
    github(&mut app, Action::Listed(generation, Ok(account)));
    assert!(!rows(&app).contains(&"Review changes on GitHub".into()));
    github(&mut app, Action::Review);
    assert!(app.github.dialog.is_none());

    // A Send starting after the check: no Update while it runs.
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    found(&mut app, "alice");
    app.github.sending.insert(repo().key);
    assert!(app.github_newer(app.tab.path.as_deref()).is_none());
    assert!(!rows(&app).contains(&"Update from GitHub".into()));
    landed(&mut app, Action::Update);
    assert_eq!(std::fs::read(&path).unwrap(), drawing(42.));

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
            Ok((repo(), vec![changed_by("alice")])),
        ),
    );
    assert_eq!(app.github_chip_state(&repo().key), Chip::Sent);
    // With no GitHub drawing open, nothing is asked and the window stays open.
    let (mut empty, _) = App::new();
    empty.github.client = app.github.client.clone();
    empty.github.account = app.github.account.clone();
    assert!(!asked(empty.github_recheck()));
    assert_eq!(empty.github.checked, None);
    // A check whose sign-in GitHub no longer takes signs out.
    let generation = app.github.generation;
    github(
        &mut app,
        Action::Newer(generation, repo().key, Err(gh::Error::SignInExpired)),
    );
    assert!(matches!(app.github.account, Account::SignedOut(Some(_))));
}

#[test]
fn a_check_keeps_githubs_names_and_default_branch_for_history() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _path) = app_in(dir.path());
    let now = Repo {
        key: RepoKey {
            owner: "octo".into(),
            repo: "lab-renamed".into(),
        },
        default_branch: "trunk".into(),
        ..repo()
    };
    let generation = app.github.generation;
    github(
        &mut app,
        Action::Newer(generation, repo().key, Ok((now, vec![changed_by("alice")]))),
    );
    let state = &app.github.states[&repo().key];
    assert_eq!(
        (state.repo.as_str(), state.branch.as_str()),
        ("lab-renamed", "trunk")
    );
    assert_eq!(app.github_chip_state(&repo().key), Chip::Newer);
}

#[test]
fn update_never_writes_over_a_change_made_in_another_app() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    found(&mut app, "alice");
    // Saved by another app since the drawing was last read here.
    std::fs::write(&path, drawing(60.)).unwrap();
    landed(&mut app, Action::Update);
    assert_eq!(std::fs::read(&path).unwrap(), drawing(60.));
    assert_eq!(app.status, "Couldn't update: It changed on this computer");
    assert!(app.error);
    // That change is read now: GitHub's version waits for Review.
    assert_eq!(app.github_chip_state(&repo().key), Chip::Review);
    assert_eq!(
        file_state(&app, "ring.rsk").unwrap().base_blob,
        Some(sha('b'))
    );
}

#[tokio::test]
async fn a_send_and_closing_the_window_wait_for_updates_write() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    // b.rsk is saved here, unsent; ring.rsk in front changed on GitHub.
    let other = open_other(&mut app, dir.path());
    std::fs::write(&other, drawing(70.)).unwrap();
    read_back(&mut app);
    app.select_tab(0);
    found(&mut app, "alice");
    let update = app.update(Message::GitHub(Action::Update));
    assert_eq!(
        std::fs::read(&path).unwrap(),
        drawing(42.),
        "not written yet"
    );
    // Send waits: nothing is read until the write lands.
    let send = app.github_send(repo().key, "Update b.rsk".into(), None);
    assert!(app.github.sending.contains(&repo().key));
    assert_eq!(app.github.deferred.len(), 1);
    // So does closing the window.
    let window = iced::window::Id::unique();
    let _ = app.update(Message::Close(window));
    assert!(app.github.quit_after.is_some());
    assert!(app.github.closing.is_none(), "nothing asked yet");
    run(&mut app, Task::batch([update, send])).await;
    assert_eq!(std::fs::read(&path).unwrap(), drawing(30.));
    assert!(app.github.deferred.is_empty() && app.github.quit_after.is_none());
    // The send ran once the write landed (and failed: this GitHub is offline).
    assert!(app.github.failed.contains_key(&repo().key));
    assert!(app.github.sending.is_empty());
}

#[test]
fn an_edit_made_while_update_writes_stays_unsaved() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _path) = app_in(dir.path());
    found(&mut app, "alice");
    let update = app.update(Message::GitHub(Action::Update));
    edit(&mut app);
    app.tab.revision += 1;
    land(&mut app, update);
    let theirs = Document::from_native_file(&drawing(30.)).unwrap();
    assert_eq!(app.tab.doc, theirs);
    assert!(app.dirty(), "the edit is one Undo away, and closing asks");
}
