use super::*;
use crate::app::tabs;
use reshiki::document::Document;
use reshiki::github::{
    credentials::Tokens,
    send::SendOutcome,
    workcopy::{FileState, RepoState},
};
use std::sync::Arc;

fn repo() -> Repo {
    Repo {
        id: 7,
        key: RepoKey {
            owner: "octo".into(),
            repo: "lab".into(),
        },
        private: true,
        default_branch: "main".into(),
        push: true,
    }
}

fn sha(c: char) -> String {
    c.to_string().repeat(40)
}

fn drawing(atoms: f32) -> Vec<u8> {
    reshiki::rings::Preset::Benzene
        .document(atoms, false)
        .file_json()
        .unwrap()
}

/// A GitHub at a closed port on this computer: every call fails at once
/// as offline, and nothing leaves the computer.
fn nowhere() -> config::Endpoints {
    let closed = "http://127.0.0.1:9".to_owned();
    config::Endpoints {
        api: closed.clone(),
        web: closed,
        overridden: true,
    }
}

/// An app whose GitHub folder is `dir`, signed in, with `ring.rsk` opened
/// from GitHub and in front.
fn app_in(dir: &Path) -> (App, PathBuf) {
    let (mut app, _) = App::new();
    let github = &mut app.github;
    github.root = Some(dir.to_owned());
    github.client_id = Some("test".into());
    github.ends = nowhere();
    github.account = Account::SignedIn {
        login: "octo".into(),
        remembered: true,
    };
    let tokens = Tokens {
        login: "octo".into(),
        access: "unused".into(),
        access_expires_at: None,
        refresh: None,
        refresh_expires_at: None,
    };
    github.client = Some(Client::new("test".into(), github.ends.clone(), Some(tokens)).unwrap());
    let repo = repo();
    let path = workcopy::working_path(dir, &repo.key, "ring.rsk");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let bytes = drawing(42.);
    std::fs::write(&path, &bytes).unwrap();
    let mut state = RepoState::new(&repo);
    state.files.insert(
        "ring.rsk".into(),
        FileState {
            base_commit: sha('c'),
            base_blob: Some(sha('b')),
            base_hash: workcopy::hash(&bytes),
            conflict: None,
        },
    );
    github.states.insert(repo.key.clone(), state);
    app.show_opened(
        Document::from_native_file(&bytes).unwrap(),
        Some(path.clone()),
    );
    app.github_opened(&path);
    (app, path)
}

fn unsent(app: &App) -> Vec<String> {
    app.github
        .unsent
        .get(&repo().key)
        .cloned()
        .unwrap_or_default()
}

/// A save made on this computer, as Cmd+S writes it.
fn save_here(app: &mut App, path: &Path) -> Vec<u8> {
    let bytes = drawing(50.);
    std::fs::write(path, &bytes).unwrap();
    app.tab.doc = Document::from_native_file(&bytes).unwrap();
    app.tab.mark_saved(app.tab.doc.clone());
    let _ = app.github_saved(true, Some((path.to_owned(), app.tab.doc.clone())));
    bytes
}

fn github(app: &mut App, action: Action) {
    let _ = app.update(Message::GitHub(action));
}

/// Runs `task` and what its messages start, as the app would. The test
/// GitHub is [`nowhere`], so a send fails as offline.
async fn run(app: &mut App, task: Task<Message>) {
    use iced::futures::StreamExt;
    let mut tasks = std::collections::VecDeque::from([task]);
    while let Some(task) = tasks.pop_front() {
        let Some(mut stream) = iced_runtime::task::into_stream(task) else {
            continue;
        };
        while let Some(action) = stream.next().await {
            if let iced_runtime::Action::Output(message) = action {
                tasks.push_back(app.update(message));
            }
        }
    }
}

/// `b.rsk` of the same repository, opened in a new tab in front.
fn open_other(app: &mut App, dir: &Path) -> PathBuf {
    let path = workcopy::working_path(dir, &repo().key, "b.rsk");
    let bytes = drawing(60.);
    std::fs::write(&path, &bytes).unwrap();
    let state = app.github.states.get_mut(&repo().key).unwrap();
    state.files.insert(
        "b.rsk".into(),
        FileState {
            base_commit: sha('c'),
            base_blob: Some(sha('9')),
            base_hash: workcopy::hash(&bytes),
            conflict: None,
        },
    );
    app.add_tab();
    app.show_opened(
        Document::from_native_file(&bytes).unwrap(),
        Some(path.clone()),
    );
    path
}

fn edit(app: &mut App) {
    app.tab.doc.atoms[0].position.x += 20.;
}

#[test]
fn without_a_client_id_nothing_signs_in() {
    let (mut app, _) = App::new();
    app.github.account = Account::NotConfigured;
    assert!(!app.github.configured());
    github(&mut app, Action::SignIn);
    assert!(matches!(app.github.account, Account::NotConfigured));
    github(&mut app, Action::ShowAccount);
    assert!(matches!(app.github.dialog, Some(Dialog::Account)));
}

#[test]
fn a_sign_in_shows_the_code_and_cancel_drops_late_answers() {
    let (mut app, _) = App::new();
    app.github.account = Account::SignedOut(None);
    app.github.client_id = Some("test".into());
    github(&mut app, Action::ShowAccount);
    github(&mut app, Action::SignIn);
    assert!(matches!(app.github.account, Account::Requesting));
    let generation = app.github.generation;
    let reply = br#"{"device_code":"d","user_code":"WDJB-MJHT","verification_uri":"https://evil.example/x","expires_in":900,"interval":5}"#;
    let code = auth::parse_code(reply, &app.github.ends).unwrap();
    // A verification page elsewhere is never opened.
    assert_eq!(code.verification_uri, "https://github.com/login/device");
    github(&mut app, Action::CodeReady(generation, Ok(code)));
    let Account::Waiting(code) = &app.github.account else {
        panic!("not waiting: {:?}", app.github.account);
    };
    assert_eq!(code.user_code, "WDJB-MJHT");

    // Escape cancels the sign-in, closes the dialog and drops what comes late.
    let _ = app.update(Message::Escape);
    assert!(app.github.dialog.is_none());
    assert!(matches!(app.github.account, Account::SignedOut(None)));
    github(&mut app, Action::Polled(generation, Ok(auth::Poll::Denied)));
    assert!(matches!(app.github.account, Account::SignedOut(None)));

    for (poll, note) in [
        (auth::Poll::Expired, "Code expired."),
        (auth::Poll::Denied, "Sign-in canceled."),
    ] {
        github(&mut app, Action::SignIn);
        let generation = app.github.generation;
        let code = auth::parse_code(reply, &app.github.ends).unwrap();
        github(&mut app, Action::CodeReady(generation, Ok(code)));
        github(&mut app, Action::Polled(generation, Ok(poll)));
        assert!(
            matches!(&app.github.account, Account::SignedOut(Some(shown)) if shown == note),
            "{:?}",
            app.github.account
        );
    }
}

#[test]
fn signed_in_without_repositories_asks_to_choose_them() {
    let (mut app, _) = App::new();
    app.github.client_id = Some("test".into());
    github(&mut app, Action::SignIn);
    let generation = app.github.generation;
    let account = gh::Account {
        login: "octo".into(),
        repos: Vec::new(),
    };
    github(&mut app, Action::SignedIn(generation, Ok(account)));
    assert!(matches!(&app.github.account, Account::NeedsInstall { login } if login == "octo"));
    let account = gh::Account {
        login: "octo".into(),
        repos: vec![repo()],
    };
    let generation = app.github.generation;
    github(&mut app, Action::Listed(generation, Ok(account)));
    assert!(matches!(app.github.account, Account::SignedIn { .. }));
}

#[test]
fn a_save_is_unsent_until_github_has_it() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    assert!(unsent(&app).is_empty());
    let bytes = save_here(&mut app, &path);
    assert_eq!(unsent(&app), ["ring.rsk"]);

    let outcome = SendOutcome {
        repo: repo(),
        commit: sha('d'),
        sent: vec![("ring.rsk".into(), sha('e'), workcopy::hash(&bytes))],
        conflicts: Vec::new(),
    };
    app.github.sending.insert(repo().key);
    github(
        &mut app,
        Action::Sent(repo().key, workcopy::now_id(), Ok(outcome)),
    );
    assert!(unsent(&app).is_empty());
    assert!(app.github.sending.is_empty());
    assert_eq!(app.status, "Sent to GitHub");
    let state = &app.github.states[&repo().key];
    assert_eq!(state.files["ring.rsk"].base_blob, Some(sha('e')));
}

#[test]
fn a_failed_send_keeps_the_drawing_unsent_and_says_why() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    save_here(&mut app, &path);
    github(
        &mut app,
        Action::Sent(
            repo().key,
            workcopy::now_id(),
            Err(gh::Error::RateLimited(Some(
                std::time::Duration::from_secs(200),
            ))),
        ),
    );
    assert_eq!(unsent(&app), ["ring.rsk"]);
    assert_eq!(app.status, "Not sent: Try again in 4 min");
    assert!(app.error);
    assert!(app.github.failed.contains_key(&repo().key));

    // A sign-in GitHub refuses signs out; the save stays.
    github(
        &mut app,
        Action::Sent(
            repo().key,
            workcopy::now_id(),
            Err(gh::Error::SignInExpired),
        ),
    );
    assert!(
        matches!(&app.github.account, Account::SignedOut(Some(note)) if note == "Sign in to GitHub again.")
    );
    assert_eq!(unsent(&app), ["ring.rsk"]);
}

#[test]
fn a_drawing_changed_on_github_is_recorded_as_a_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    let mine = save_here(&mut app, &path);
    let outcome = SendOutcome {
        repo: repo(),
        commit: sha('f'),
        sent: Vec::new(),
        conflicts: vec![send::Conflict {
            repo_path: "ring.rsk".into(),
            commit: sha('f'),
            blob: Some(sha('a')),
            theirs: Some(drawing(30.)),
            mine,
            names: vec!["ring.rsk".into()],
        }],
    };
    github(
        &mut app,
        Action::Sent(repo().key, workcopy::now_id(), Ok(outcome)),
    );
    assert_eq!(app.github_conflicts(&repo().key), 1);
    assert_eq!(unsent(&app), ["ring.rsk"], "nothing is overwritten");
    assert!(app.error);
    assert_eq!(app.status, "Not sent · “ring.rsk” changed on GitHub");
}

#[test]
fn closing_an_unsent_drawing_asks_and_later_closes_it() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    save_here(&mut app, &path);
    let id = app.tab.id;
    let _ = app.update(Message::Tabs(tabs::Action::Close(None)));
    assert!(matches!(app.github.closing, Some(Closing::Tab(asked, _)) if asked == id));
    assert_eq!(app.tab.path.as_deref(), Some(path.as_path()), "still open");
    github(&mut app, Action::CloseAnswer(false));
    assert!(app.strip().all(|tab| tab.id != id));
    assert!(app.github.closing.is_none());
    // Later keeps the save on this computer, unsent.
    app.github_refresh(&repo().key);
    assert_eq!(unsent(&app), ["ring.rsk"]);
}

#[test]
fn a_sent_drawing_closes_without_asking() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _path) = app_in(dir.path());
    let id = app.tab.id;
    let _ = app.update(Message::Tabs(tabs::Action::Close(None)));
    assert!(app.github.closing.is_none());
    assert!(app.strip().all(|tab| tab.id != id));
}

#[test]
fn closing_the_window_asks_once_for_every_unsent_drawing() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    save_here(&mut app, &path);
    let window = iced::window::Id::unique();
    let _ = app.update(Message::Close(window));
    assert!(matches!(app.github.closing, Some(Closing::Window(..))));
    assert!(!app.exit.frozen());
    github(&mut app, Action::CloseAnswer(false));
    assert!(app.exit.frozen(), "Later goes on closing");
}

#[test]
fn a_send_that_leaves_a_conflict_does_not_close_the_tab() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    let mine = save_here(&mut app, &path);
    let id = app.tab.id;
    app.github.closing = Some(Closing::Tab(id, app.tab.revision));
    app.github.sending.insert(repo().key);
    let outcome = SendOutcome {
        repo: repo(),
        commit: sha('f'),
        sent: Vec::new(),
        conflicts: vec![send::Conflict {
            repo_path: "ring.rsk".into(),
            commit: sha('f'),
            blob: None,
            theirs: None,
            mine,
            names: Vec::new(),
        }],
    };
    github(
        &mut app,
        Action::Sent(repo().key, workcopy::now_id(), Ok(outcome)),
    );
    assert!(app.strip().any(|tab| tab.id == id));
    assert!(app.github.closing.is_none());
}

#[test]
fn a_dialog_holds_the_keyboard_and_escape_returns_to_the_drawing() {
    let (mut app, _) = App::new();
    github(&mut app, Action::ShowAccount);
    let tool = app.tool;
    let _ = app.update(Message::Tool(crate::canvas::Tool::Erase));
    assert_eq!(app.tool, tool, "the drawing waits behind the dialog");
    assert_eq!(
        modal_key(
            &iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
            iced::keyboard::Modifiers::empty()
        )
        .map(|message| format!("{message:?}")),
        Some(format!("{:?}", Message::GitHub(Action::Close)))
    );
    let _ = app.update(Message::Escape);
    assert!(app.github.dialog.is_none());
}

#[test]
fn send_on_a_drawing_not_on_github_chooses_where_it_goes() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    app.add_tab();
    github(&mut app, Action::ShowSend);
    assert!(matches!(&app.github.dialog, Some(Dialog::Picker(_))));
}

#[test]
fn send_lists_the_unsent_drawings_with_a_message() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    save_here(&mut app, &path);
    github(&mut app, Action::ShowSend);
    let Some(Dialog::Send(draft)) = &app.github.dialog else {
        panic!("no Send box");
    };
    assert_eq!(draft.files, ["ring.rsk"]);
    assert_eq!(draft.message, "Update ring.rsk");
}

#[test]
fn restoring_a_version_is_one_undo_step_and_a_new_save() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _path) = app_in(dir.path());
    github(&mut app, Action::ShowVersions);
    assert_eq!(app.inspector_tab, InspectorTab::Versions);
    let generation = app.github.versions.generation;
    let old = Arc::new(Document::from_native_file(&drawing(30.)).unwrap());
    let id = "1763200000000".to_owned();
    github(
        &mut app,
        Action::Versions(versions::Action::LocalLoaded(
            generation,
            Ok(vec![(id.clone(), old.clone())]),
        )),
    );
    github(
        &mut app,
        Action::Versions(versions::Action::Select(versions::Pick::Local(id))),
    );
    let before = app.tab.doc.clone();
    github(&mut app, Action::Versions(versions::Action::Restore));
    assert_ne!(app.tab.doc, before);
    assert!(app.file_io.saving, "Restore saves");
    let _ = app.update(Message::Undo);
    assert!(super::super::same_drawing(&app.tab.doc, &before));

    // Open shows the version in a new, unsaved tab.
    let tabs = app.strip().count();
    github(&mut app, Action::Versions(versions::Action::Open));
    assert_eq!(app.strip().count(), tabs + 1);
    assert!(app.tab.path.is_none());
    assert!(app.dirty(), "closing it asks");
    assert!(app.tab.name().starts_with("ring · "));
}

/// Ethanol whose Mol. Wt. caption the user placed, as stored, and the drawing
/// after it: the molecule moved and the caption put back where it was.
fn placed_caption(app: &mut App) -> (Vec<u8>, Document) {
    use reshiki::linked_properties::Property;
    fn place(app: &mut App, dx: f32, dy: f32) {
        let before = app.tab.doc.clone();
        let caption = &mut app.tab.doc.annotations[0];
        caption.position = caption.position.offset(dx, dy);
        app.changed(before);
    }
    app.tab.busy = false;
    app.tab.doc = Document::from_json(include_bytes!(
        "../../../tests/fixtures/chemical-naming-rust/ethanol-rust-native.rsk"
    ))
    .unwrap();
    let atoms = app.tab.doc.atoms.iter().map(|a| a.id).collect();
    let _ = app.update(Message::ToggleProperty(atoms, Property::Weight));
    place(app, 40., 25.);
    let stored = app.tab.doc.file_json().unwrap();
    let before = app.tab.doc.clone();
    for atom in &mut app.tab.doc.atoms {
        atom.position = atom.position.offset(30., 0.);
    }
    app.changed(before);
    place(app, -30., 0.);
    assert_eq!(app.tab.doc.property_captions.len(), 1, "still linked");
    (stored, app.tab.doc.clone())
}

#[test]
fn restoring_a_version_puts_a_placed_property_caption_back_exactly() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _path) = app_in(dir.path());
    let (stored, _) = placed_caption(&mut app);
    let stored = Arc::new(Document::from_native_file(&stored).unwrap());
    github(&mut app, Action::ShowVersions);
    let generation = app.github.versions.generation;
    let id = "1763200000000".to_owned();
    github(
        &mut app,
        Action::Versions(versions::Action::LocalLoaded(
            generation,
            Ok(vec![(id.clone(), stored.clone())]),
        )),
    );
    github(
        &mut app,
        Action::Versions(versions::Action::Select(versions::Pick::Local(id))),
    );
    assert_ne!(app.tab.doc, *stored);
    github(&mut app, Action::Versions(versions::Action::Restore));
    assert_eq!(
        app.tab.doc.annotations[0].position,
        stored.annotations[0].position
    );
    assert_eq!(app.tab.doc, *stored);
}

#[test]
fn keep_githubs_shows_a_placed_property_caption_exactly() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    let (theirs, yours) = placed_caption(&mut app);
    conflicted(&mut app, &path, Some(theirs.clone()));
    app.github.sending.clear();
    app.tab.doc = yours.clone();
    app.tab.mark_saved(yours);
    github(&mut app, Action::Resolve(send::Choice::KeepTheirs));
    assert!(app.status.starts_with("Kept GitHub’s"), "{}", app.status);
    assert_eq!(app.tab.doc, Document::from_native_file(&theirs).unwrap());
    assert!(!app.dirty());
}

/// `ring.rsk` saved here as "mine" while GitHub got "theirs" (or deleted it),
/// then sent: the send leaves it out as a conflict.
fn conflicted(app: &mut App, path: &Path, theirs: Option<Vec<u8>>) -> Vec<u8> {
    let mine = save_here(app, path);
    let outcome = SendOutcome {
        repo: repo(),
        commit: sha('f'),
        sent: Vec::new(),
        conflicts: vec![send::Conflict {
            repo_path: "ring.rsk".into(),
            commit: sha('f'),
            blob: theirs.as_ref().map(|_| sha('a')),
            theirs,
            mine: mine.clone(),
            names: vec!["ring.rsk".into(), "ring.svg".into()],
        }],
    };
    app.github.sending.insert(repo().key);
    github(
        app,
        Action::Sent(repo().key, workcopy::now_id(), Ok(outcome)),
    );
    mine
}

fn review(app: &App) -> &conflict::Review {
    match &app.github.dialog {
        Some(Dialog::Conflict(review)) => review,
        other => panic!("no conflict dialog: {other:?}"),
    }
}

fn file_state(app: &App, path: &str) -> Option<FileState> {
    app.github.states[&repo().key].files.get(path).cloned()
}

fn outgoing(app: &App, dir: &Path) -> Vec<(String, Option<String>)> {
    let state = &app.github.states[&repo().key];
    send::outgoing(dir, &repo().key, state)
        .unwrap()
        .into_iter()
        .map(|file| (file.repo_path, file.base_blob))
        .collect()
}

#[test]
fn a_conflict_shows_githubs_and_yours_with_the_changes_marked() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    conflicted(&mut app, &path, Some(drawing(30.)));
    let review = review(&app);
    assert_eq!(
        (review.name.as_str(), review.copy.as_str()),
        ("ring.rsk", "ring (2).rsk")
    );
    assert_eq!((review.done, review.total), (0, 1));
    let compare = &review.compare;
    assert!(compare.left.is_ok());
    assert!(!compare.marks[0].is_empty() && !compare.marks[1].is_empty());

    // Escape decides later: nothing changes, and Review stays.
    let before = std::fs::read(&path).unwrap();
    let _ = app.update(Message::Escape);
    assert!(app.github.dialog.is_none());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(file_state(&app, "ring.rsk").unwrap().conflict.is_some());
    github(&mut app, Action::Review);
    assert!(matches!(app.github.dialog, Some(Dialog::Conflict(_))));
}

#[test]
fn keep_mine_sends_yours_over_githubs() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    let mine = conflicted(&mut app, &path, Some(drawing(30.)));
    app.github.sending.clear();
    github(&mut app, Action::Resolve(send::Choice::KeepMine));
    assert!(app.github.dialog.is_none());
    // Files: yours stays.
    assert_eq!(std::fs::read(&path).unwrap(), mine);
    // State: built on GitHub's version, still to send.
    let file = file_state(&app, "ring.rsk").unwrap();
    assert_eq!((file.base_commit, file.conflict), (sha('f'), None));
    assert_eq!(
        outgoing(&app, dir.path()),
        [("ring.rsk".into(), Some(sha('a')))]
    );
    assert!(!app.dirty());
    // Send: started again at once, and its end still says what was kept.
    assert!(app.github.sending.contains(&repo().key));
    sent(&mut app, "ring.rsk");
    assert_eq!(app.status, "Kept yours · sent");
}

#[test]
fn a_choice_whose_send_never_starts_keeps_no_outcome_for_later() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    conflicted(&mut app, &path, Some(drawing(30.)));
    app.github.sending.clear();
    app.github.account = Account::SignedOut(None);
    github(&mut app, Action::Resolve(send::Choice::KeepMine));
    assert!(matches!(app.github.dialog, Some(Dialog::Account)));
    assert!(app.github.kept.is_empty());
}

/// GitHub took `repo_path`, with nothing in conflict.
fn sent(app: &mut App, repo_path: &str) {
    let outcome = SendOutcome {
        repo: repo(),
        commit: sha('9'),
        sent: vec![(repo_path.into(), sha('8'), "hash".into())],
        conflicts: Vec::new(),
    };
    github(
        app,
        Action::Sent(repo().key, workcopy::now_id(), Ok(outcome)),
    );
}

#[test]
fn keep_githubs_shows_it_as_one_undo_step_and_keeps_yours_in_versions() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    let mine = conflicted(&mut app, &path, Some(drawing(30.)));
    app.github.sending.clear();
    let yours = app.tab.doc.clone();
    github(&mut app, Action::Resolve(send::Choice::KeepTheirs));
    assert!(app.github.dialog.is_none());
    // Files: GitHub's is here; yours is the newest version.
    assert_eq!(std::fs::read(&path).unwrap(), drawing(30.));
    let saves = workcopy::snapshots(dir.path(), &repo().key, "ring.rsk").unwrap();
    assert_eq!(std::fs::read(&saves.last().unwrap().path).unwrap(), mine);
    // State: nothing left to send, so nothing is sent.
    let file = file_state(&app, "ring.rsk").unwrap();
    assert_eq!(file.base_hash, workcopy::hash(&drawing(30.)));
    assert_eq!(file.conflict, None);
    assert!(outgoing(&app, dir.path()).is_empty());
    assert!(app.github.sending.is_empty());
    // The tab shows GitHub's, saved, and Undo brings yours back.
    let theirs = Document::from_native_file(&drawing(30.)).unwrap();
    assert!(super::super::same_drawing(&app.tab.doc, &theirs));
    assert!(!app.dirty());
    let _ = app.update(Message::Undo);
    assert!(super::super::same_drawing(&app.tab.doc, &yours));
}

#[test]
fn keep_both_saves_yours_beside_githubs_and_sends_it() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    let mine = conflicted(&mut app, &path, Some(drawing(30.)));
    app.github.sending.clear();
    github(&mut app, Action::Resolve(send::Choice::KeepBoth));
    assert!(app.github.dialog.is_none());
    // Files: GitHub's under the name, yours beside it.
    let copy = workcopy::working_path(dir.path(), &repo().key, "ring (2).rsk");
    assert_eq!(std::fs::read(&path).unwrap(), drawing(30.));
    assert_eq!(std::fs::read(&copy).unwrap(), mine);
    // State: the copy is new on GitHub; the original is GitHub's.
    assert_eq!(file_state(&app, "ring (2).rsk"), Some(FileState::default()));
    assert_eq!(file_state(&app, "ring.rsk").unwrap().conflict, None);
    assert_eq!(outgoing(&app, dir.path()), [("ring (2).rsk".into(), None)]);
    // The tab still shows yours, now as the copy.
    assert_eq!(app.tab.path.as_deref(), Some(copy.as_path()));
    assert!(!app.dirty());
    assert!(app.github.sending.contains(&repo().key));
    sent(&mut app, "ring (2).rsk");
    assert_eq!(app.status, "Kept both · yours is “ring (2).rsk” · sent");
}

#[test]
fn keep_githubs_deletion_leaves_yours_open_unsaved() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    conflicted(&mut app, &path, None);
    assert_eq!(
        review(&app).compare.left.as_ref().err().map(String::as_str),
        Some("Deleted on GitHub")
    );
    github(&mut app, Action::Resolve(send::Choice::KeepTheirs));
    assert!(!path.exists());
    assert_eq!(file_state(&app, "ring.rsk"), None);
    assert_eq!(app.tab.path, None);
    assert_eq!(app.tab.name(), "ring.rsk");
    assert!(app.dirty(), "an unsaved drawing");
}

#[tokio::test]
async fn unsaved_edits_are_saved_first_and_are_what_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    conflicted(&mut app, &path, Some(drawing(30.)));
    app.tab.doc.atoms[0].position.x += 20.;
    github(&mut app, Action::Resolve(send::Choice::KeepBoth));
    assert!(app.file_io.saving, "saves first");
    assert!(matches!(app.github.dialog, Some(Dialog::Conflict(_))));
    // The save lands, as `Saved` delivers it.
    app.file_io.saving = false;
    let edited = app.tab.doc.file_json().unwrap();
    std::fs::write(&path, &edited).unwrap();
    app.tab.mark_saved(app.tab.doc.clone());
    let saved = app.github_saved(true, Some((path.clone(), app.tab.doc.clone())));
    run(&mut app, saved).await;
    let copy = workcopy::working_path(dir.path(), &repo().key, "ring (2).rsk");
    assert_eq!(std::fs::read(&copy).unwrap(), edited);
    assert_eq!(std::fs::read(&path).unwrap(), drawing(30.));
}

#[tokio::test]
async fn a_save_still_writing_is_waited_for() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    let mine = conflicted(&mut app, &path, Some(drawing(30.)));
    app.file_io.saving = true;
    app.file_io.saving_tab = Some(app.tab.id);
    github(&mut app, Action::Resolve(send::Choice::KeepBoth));
    let copy = workcopy::working_path(dir.path(), &repo().key, "ring (2).rsk");
    assert!(!copy.exists(), "nothing is kept before the save lands");
    assert_eq!(std::fs::read(&path).unwrap(), mine);
    app.file_io.saving = false;
    let saved = app.github_saved(true, Some((path.clone(), app.tab.doc.clone())));
    run(&mut app, saved).await;
    assert_eq!(std::fs::read(&copy).unwrap(), mine);
    assert_eq!(std::fs::read(&path).unwrap(), drawing(30.));
}

#[tokio::test]
async fn a_choice_made_while_another_tab_saves_can_be_made_again() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    conflicted(&mut app, &path, Some(drawing(30.)));
    edit(&mut app);
    let other = open_other(&mut app, dir.path());
    app.file_io.saving = true;
    app.file_io.saving_tab = Some(app.tab.id);
    github(&mut app, Action::Resolve(send::Choice::KeepBoth));
    // The other tab's save lands; nothing of this drawing was saved.
    app.file_io.saving = false;
    let _ = app.github_saved(true, Some((other, app.tab.doc.clone())));
    github(&mut app, Action::Resolve(send::Choice::KeepBoth));
    assert!(app.file_io.saving, "the second choice saves first");
    app.file_io.saving = false;
    let edited = app.tab.doc.file_json().unwrap();
    std::fs::write(&path, &edited).unwrap();
    app.tab.mark_saved(app.tab.doc.clone());
    let saved = app.github_saved(true, Some((path.clone(), app.tab.doc.clone())));
    run(&mut app, saved).await;
    let copy = workcopy::working_path(dir.path(), &repo().key, "ring (2).rsk");
    assert_eq!(std::fs::read(&copy).unwrap(), edited);
}

#[test]
fn a_choice_never_waits_for_a_closing_tabs_save() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    conflicted(&mut app, &path, Some(drawing(30.)));
    edit(&mut app);
    open_other(&mut app, dir.path());
    // The other tab is closing and saving first.
    app.pending = Some(crate::app::Pending::CloseTab(app.tab.id));
    app.file_io.saving = true;
    app.file_io.saving_tab = Some(app.tab.id);
    github(&mut app, Action::Resolve(send::Choice::KeepBoth));
    assert!(app.github.resolving.is_none(), "it can be made again");
}

#[test]
fn several_conflicts_are_shown_one_after_another() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    let other = workcopy::working_path(dir.path(), &repo().key, "b.rsk");
    std::fs::write(&other, drawing(60.)).unwrap();
    let mine = save_here(&mut app, &path);
    let conflict = |repo_path: &str, mine: Vec<u8>| send::Conflict {
        repo_path: repo_path.into(),
        commit: sha('f'),
        blob: Some(sha('a')),
        theirs: Some(drawing(30.)),
        mine,
        names: Vec::new(),
    };
    app.github
        .states
        .get_mut(&repo().key)
        .unwrap()
        .files
        .insert(
            "b.rsk".into(),
            FileState {
                base_commit: sha('c'),
                base_blob: Some(sha('b')),
                base_hash: String::new(),
                conflict: None,
            },
        );
    let outcome = SendOutcome {
        repo: repo(),
        commit: sha('f'),
        sent: Vec::new(),
        conflicts: vec![conflict("b.rsk", drawing(60.)), conflict("ring.rsk", mine)],
    };
    github(
        &mut app,
        Action::Sent(repo().key, workcopy::now_id(), Ok(outcome)),
    );
    assert_eq!(review(&app).name, "b.rsk");
    assert_eq!((review(&app).done, review(&app).total), (0, 2));
    github(&mut app, Action::Resolve(send::Choice::KeepTheirs));
    assert_eq!(review(&app).name, "ring.rsk");
    assert_eq!((review(&app).done, review(&app).total), (1, 2));
    assert!(
        app.github.sending.is_empty(),
        "sends once every conflict is settled"
    );
    github(&mut app, Action::Resolve(send::Choice::KeepMine));
    assert!(app.github.dialog.is_none());
    assert!(app.github.sending.contains(&repo().key));
}

#[test]
fn review_after_a_restart_asks_github_again() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    conflicted(&mut app, &path, Some(drawing(30.)));
    let _ = app.update(Message::Escape);
    app.github.conflicts.clear();
    app.github.sending.clear();
    github(&mut app, Action::Review);
    assert!(app.github.sending.contains(&repo().key));
}

#[test]
fn a_version_compares_with_the_drawing_now() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _path) = app_in(dir.path());
    github(&mut app, Action::ShowVersions);
    let generation = app.github.versions.generation;
    let old = Arc::new(Document::from_native_file(&drawing(30.)).unwrap());
    let id = "1763200000000".to_owned();
    github(
        &mut app,
        Action::Versions(versions::Action::LocalLoaded(
            generation,
            Ok(vec![(id.clone(), old.clone())]),
        )),
    );
    github(
        &mut app,
        Action::Versions(versions::Action::Select(versions::Pick::Local(id))),
    );
    github(&mut app, Action::Versions(versions::Action::Compare));
    let Some(Dialog::Compare(compare, _)) = &app.github.dialog else {
        panic!("no compare");
    };
    assert_eq!(compare.left.as_ref().ok(), Some(&*old));
    assert_eq!(compare.right, app.tab.doc);
    assert!(!compare.marks[1].is_empty());
    let _ = app.update(Message::Escape);
    assert!(app.github.dialog.is_none());
}

#[tokio::test]
async fn send_saves_every_open_drawing_of_the_repository_first() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, ring) = app_in(dir.path());
    edit(&mut app);
    let other = open_other(&mut app, dir.path());
    edit(&mut app);
    github(&mut app, Action::ShowSend);
    let Some(Dialog::Send(draft)) = &app.github.dialog else {
        panic!("no Send box");
    };
    assert_eq!(draft.files, ["ring.rsk", "b.rsk"], "the tab behind too");

    let send = app.update(Message::GitHub(Action::Send));
    run(&mut app, send).await;
    assert!(app.strip().all(|tab| !app.edited(tab)), "both saved");
    for (path, name) in [(&ring, "ring.rsk"), (&other, "b.rsk")] {
        let tab = app
            .strip()
            .find(|tab| tab.path.as_ref() == Some(path))
            .unwrap();
        let saved = std::fs::read(path).unwrap();
        let doc = Document::from_native_file(&saved).unwrap();
        assert!(super::super::same_drawing(&doc, &tab.doc));
        // Each save is a version, the one behind the front tab too.
        let kept = workcopy::snapshots(dir.path(), &repo().key, name).unwrap();
        assert_eq!(std::fs::read(&kept.last().unwrap().path).unwrap(), saved);
    }
    // Then one send of both, which fails here as offline.
    assert!(app.github.failed.contains_key(&repo().key));
    assert_eq!(unsent(&app), ["b.rsk", "ring.rsk"]);
}

#[tokio::test]
async fn a_save_for_send_lands_in_its_tab_after_switching_tabs() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, ring) = app_in(dir.path());
    edit(&mut app);
    let id = app.tab.id;
    github(&mut app, Action::ShowSend);
    let send = app.update(Message::GitHub(Action::Send));
    app.add_tab();
    run(&mut app, send).await;
    let tab = app.strip().find(|tab| tab.id == id).unwrap();
    assert!(!app.edited(tab), "marked saved in its own tab");
    let saved = Document::from_native_file(&std::fs::read(&ring).unwrap()).unwrap();
    assert!(super::super::same_drawing(&saved, &tab.doc));
    assert_eq!(
        workcopy::snapshots(dir.path(), &repo().key, "ring.rsk")
            .unwrap()
            .len(),
        1
    );
    assert!(app.github.failed.contains_key(&repo().key), "and sent");
}

/// The app keeps a sign-in only once it accepts it; storing it is
/// `Client::remember`, tested with the in-memory store in the library. These
/// app tests never run a task that could reach this computer's real store.
#[test]
fn a_canceled_sign_in_is_never_kept() {
    let (mut app, _) = App::new();
    app.github.client_id = Some("test".into());
    app.github.ends = nowhere();
    let tokens = Tokens {
        login: String::new(),
        access: "unused".into(),
        access_expires_at: None,
        refresh: None,
        refresh_expires_at: None,
    };
    let reply = br#"{"device_code":"d","user_code":"WDJB-MJHT","verification_uri":"https://github.com/login/device","expires_in":900,"interval":5}"#;
    let account = gh::Account {
        login: "octo".into(),
        repos: vec![repo()],
    };
    for cancel in [true, false] {
        github(&mut app, Action::ShowAccount);
        github(&mut app, Action::SignIn);
        let generation = app.github.generation;
        let code = auth::parse_code(reply, &app.github.ends).unwrap();
        github(&mut app, Action::CodeReady(generation, Ok(code)));
        github(
            &mut app,
            Action::Polled(generation, Ok(auth::Poll::Done(tokens.clone()))),
        );
        if cancel {
            let _ = app.update(Message::Escape);
        }
        let remember = app.github_action(Action::SignedIn(generation, Ok(account.clone())));
        // Not run: it would write to the real credential store.
        let remembers = iced_runtime::task::into_stream(remember).is_some();
        if cancel {
            assert!(matches!(app.github.account, Account::SignedOut(None)));
            assert!(!remembers, "nothing is kept");
        } else {
            assert!(matches!(app.github.account, Account::SignedIn { .. }));
            assert!(remembers);
            github(&mut app, Action::Remembered(generation, false));
            assert!(matches!(
                app.github.account,
                Account::SignedIn {
                    remembered: false,
                    ..
                }
            ));
        }
    }
}

/// Closes the tab in front without the GitHub question.
fn close_front(app: &mut App) {
    let path = app.tab.path.clone();
    app.github.later.insert(app.tab.id);
    let _ = app.update(Message::Tabs(tabs::Action::Close(None)));
    assert!(
        app.strip()
            .all(|tab| tab.path.is_none() || tab.path != path)
    );
}

/// The picker on `repo()`, its top folder listed with `names`.
fn picker_at(app: &mut App, mode: Mode, names: &[&str]) {
    app.github.repos = vec![repo()];
    github(app, Action::Picker(picker::Action::Show(mode)));
    github(app, Action::Picker(picker::Action::PickRepo(repo().key)));
    let entries = names
        .iter()
        .map(|name| gh::Entry {
            name: (*name).into(),
            sha: sha('7'),
            folder: false,
        })
        .collect();
    let generation = picker_generation(app);
    github(
        app,
        Action::Picker(picker::Action::Listing(generation, Ok((sha('8'), entries)))),
    );
}

fn picker_generation(app: &App) -> u64 {
    match &app.github.dialog {
        Some(Dialog::Picker(picker)) => picker.generation,
        other => panic!("no picker: {other:?}"),
    }
}

fn picker_error(app: &App) -> Option<String> {
    match &app.github.dialog {
        Some(Dialog::Picker(picker)) => picker.error.clone(),
        other => panic!("no picker: {other:?}"),
    }
}

#[test]
fn a_closed_pickers_late_reply_never_reaches_a_new_one() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    app.github.repos = vec![repo()];
    github(&mut app, Action::Picker(picker::Action::Show(Mode::Open)));
    github(
        &mut app,
        Action::Picker(picker::Action::PickRepo(repo().key)),
    );
    let late = picker_generation(&app);
    github(&mut app, Action::Close);
    github(&mut app, Action::Picker(picker::Action::Show(Mode::Open)));
    github(
        &mut app,
        Action::Picker(picker::Action::PickRepo(repo().key)),
    );
    assert_ne!(picker_generation(&app), late);
    let entry = gh::Entry {
        name: "late.rsk".into(),
        sha: sha('7'),
        folder: false,
    };
    github(
        &mut app,
        Action::Picker(picker::Action::Listing(late, Ok((sha('8'), vec![entry])))),
    );
    let Some(Dialog::Picker(picker)) = &app.github.dialog else {
        panic!("no picker");
    };
    assert!(picker.entries.is_none(), "still listing");
}

#[test]
fn save_here_never_takes_the_name_of_a_preview_already_there() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    app.add_tab();
    picker_at(&mut app, Mode::SaveHere, &["x.svg"]);
    github(
        &mut app,
        Action::Picker(picker::Action::SaveName("x".into())),
    );
    github(&mut app, Action::Picker(picker::Action::SaveHere));
    assert_eq!(picker_error(&app).as_deref(), Some("Name taken"));
    assert_eq!(app.tab.path, None);
}

#[test]
fn save_here_waits_for_a_folder_that_could_not_be_listed() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    app.add_tab();
    app.github.repos = vec![repo()];
    let picker = |action| Action::Picker(action);
    github(&mut app, picker(picker::Action::Show(Mode::SaveHere)));
    github(&mut app, picker(picker::Action::PickRepo(repo().key)));
    let failed = picker_generation(&app);
    github(
        &mut app,
        picker(picker::Action::Listing(failed, Err(gh::Error::Offline))),
    );
    // GitHub may hold `x.svg`: nothing here knows.
    github(&mut app, picker(picker::Action::SaveName("x".into())));
    github(&mut app, picker(picker::Action::SaveHere));
    assert_eq!(app.tab.path, None);
    assert_eq!(
        picker_error(&app),
        Some(gh::Error::Offline.to_string()),
        "the listing's error stays"
    );

    // Listed again, the names are known.
    github(&mut app, picker(picker::Action::PickRepo(repo().key)));
    let entry = gh::Entry {
        name: "x.svg".into(),
        sha: sha('7'),
        folder: false,
    };
    let listed = picker_generation(&app);
    github(
        &mut app,
        picker(picker::Action::Listing(listed, Ok((sha('8'), vec![entry])))),
    );
    github(&mut app, picker(picker::Action::SaveHere));
    assert_eq!(picker_error(&app).as_deref(), Some("Name taken"));
    github(&mut app, picker(picker::Action::SaveName("y".into())));
    github(&mut app, picker(picker::Action::SaveHere));
    assert!(app.tab.path.is_some());
}

#[test]
fn a_working_copy_unlike_its_base_is_never_downloaded_over() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    let mine = save_here(&mut app, &path);
    // A state.json from a newer ReShiki: the state cannot be read.
    app.github.states.clear();
    let folder = path.parent().unwrap().parent().unwrap();
    std::fs::write(folder.join("state.json"), r#"{"version":99}"#).unwrap();
    close_front(&mut app);

    picker_at(&mut app, Mode::Open, &["ring.rsk"]);
    github(&mut app, Action::Picker(picker::Action::Open));
    assert!(app.github.dialog.is_none(), "opened, not downloaded");
    assert_eq!(app.status, "Opened your unsent version");
    assert_eq!(std::fs::read(&path).unwrap(), mine);
}

#[test]
fn a_renamed_repository_keeps_its_folder_here() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    save_here(&mut app, &path);
    // Versions listed under the old names, then hidden.
    github(&mut app, Action::ShowVersions);
    app.inspector_open = false;
    close_front(&mut app);
    let renamed = Repo {
        key: RepoKey {
            owner: "octo".into(),
            repo: "lab2".into(),
        },
        ..repo()
    };
    app.github.repos = vec![renamed.clone()];
    github(&mut app, Action::Picker(picker::Action::Show(Mode::Open)));
    github(
        &mut app,
        Action::Picker(picker::Action::PickRepo(renamed.key.clone())),
    );
    let entry = gh::Entry {
        name: "ring.rsk".into(),
        sha: sha('7'),
        folder: false,
    };
    let generation = picker_generation(&app);
    github(
        &mut app,
        Action::Picker(picker::Action::Listing(
            generation,
            Ok((sha('8'), vec![entry])),
        )),
    );
    github(&mut app, Action::Picker(picker::Action::Open));
    assert!(app.github.dialog.is_none(), "the unsent drawing opens");
    assert!(!workcopy::working_path(dir.path(), &renamed.key, "ring.rsk").exists());

    // Versions asks GitHub under the names it has now.
    app.show_opened(
        Document::from_native_file(&std::fs::read(&path).unwrap()).unwrap(),
        Some(path.clone()),
    );
    github(&mut app, Action::ShowVersions);
    assert_eq!(app.github.versions.named, Some(renamed.key));
}

#[test]
fn a_conflict_saved_back_to_its_base_is_cleared_by_review() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    conflicted(&mut app, &path, Some(drawing(30.)));
    let _ = app.update(Message::Escape);
    app.github.conflicts.clear();
    app.github.sending.clear();
    std::fs::write(&path, drawing(42.)).unwrap();
    github(&mut app, Action::Review);
    assert_eq!(app.github_conflicts(&repo().key), 0);
    assert!(app.github.sending.is_empty(), "nothing to send");
}

#[test]
fn restore_waits_for_a_save_in_progress() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _path) = app_in(dir.path());
    github(&mut app, Action::ShowVersions);
    let generation = app.github.versions.generation;
    let old = Arc::new(Document::from_native_file(&drawing(30.)).unwrap());
    let id = "1763200000000".to_owned();
    github(
        &mut app,
        Action::Versions(versions::Action::LocalLoaded(
            generation,
            Ok(vec![(id.clone(), old)]),
        )),
    );
    github(
        &mut app,
        Action::Versions(versions::Action::Select(versions::Pick::Local(id))),
    );
    app.file_io.saving = true;
    let before = app.tab.doc.clone();
    github(&mut app, Action::Versions(versions::Action::Restore));
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.status, "Wait for the save to finish");
}

#[test]
fn versions_list_again_after_a_send_and_a_sign_in() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    let bytes = save_here(&mut app, &path);
    github(&mut app, Action::ShowVersions);
    let generation = app.github.versions.generation;
    let outcome = SendOutcome {
        repo: repo(),
        commit: sha('d'),
        sent: vec![("ring.rsk".into(), sha('e'), workcopy::hash(&bytes))],
        conflicts: Vec::new(),
    };
    github(
        &mut app,
        Action::Sent(repo().key, workcopy::now_id(), Ok(outcome)),
    );
    assert!(app.github.versions.generation > generation);

    let generation = app.github.versions.generation;
    let next = app.github.generation;
    let account = gh::Account {
        login: "octo".into(),
        repos: vec![repo()],
    };
    github(&mut app, Action::SignedIn(next, Ok(account)));
    assert!(app.github.versions.generation > generation);
}

#[test]
fn closing_again_waits_for_the_send_it_started() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    save_here(&mut app, &path);
    let window = iced::window::Id::unique();
    let _ = app.update(Message::Close(window));
    github(&mut app, Action::CloseAnswer(true));
    assert!(app.github.sending.contains(&repo().key));
    let _ = app.update(Message::Close(window));
    assert!(!app.exit.frozen(), "still sending");
}

#[tokio::test]
async fn a_send_waiting_for_a_closed_tabs_save_goes_on() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    save_here(&mut app, &path);
    app.github.after_save = Some((repo().key, "Update ring.rsk".into(), Vec::new()));
    app.file_io.saving = true;
    let closed = Message::Saved(0, Box::default(), Ok(None));
    let task = app.update(Message::Tab(TabId(u64::MAX), Box::new(closed)));
    run(&mut app, task).await;
    assert!(app.github.after_save.is_none());
    assert!(app.github.failed.contains_key(&repo().key), "sent");
}
