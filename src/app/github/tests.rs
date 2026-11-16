use super::*;
use crate::app::tabs;
use reshiki::document::Document;
use reshiki::github::{
    credentials::Tokens,
    send::SendOutcome,
    workcopy::{FileState, RepoState},
};
use std::future::Future;
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
    let _ = app.github_opened(&path);
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
    assert!(app.github.dialog.is_none());
}

#[test]
fn a_sign_in_shows_the_code_and_cancel_drops_late_answers() {
    let (mut app, _) = App::new();
    app.github.account = Account::SignedOut(None);
    app.github.client_id = Some("test".into());
    // Sign in shows the code at once, without an intro.
    github(&mut app, Action::SignIn);
    assert!(matches!(app.github.account, Account::Requesting));
    assert!(matches!(app.github.dialog, Some(Dialog::SignIn)));
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
    assert_eq!(app.status, "Not sent · ring.rsk changed on GitHub");
    assert!(!app.error, "a question, not an error");
    assert_eq!(app.github.amber.as_deref(), Some(app.status.as_str()));
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
    app.github.dialog = Some(Dialog::SignIn);
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
fn history_gives_way_to_properties_on_a_drawing_not_on_github() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    let on_github = app.tab.id;
    github(&mut app, Action::ShowVersions);
    assert_eq!(app.inspector_tab, InspectorTab::Versions);
    // A new drawing on this computer has no History.
    let _ = app.update(Message::New);
    let local = app.tab.id;
    assert_ne!(local, on_github);
    assert_eq!(app.inspector_tab, InspectorTab::Properties);
    assert!(app.inspector_open);
    // Going back keeps what is showing.
    let _ = app.update(Message::Tabs(tabs::Action::Select(on_github)));
    assert_eq!(app.tab.id, on_github);
    assert_eq!(app.inspector_tab, InspectorTab::Properties);
    // So does switching to the local tab with History showing.
    github(&mut app, Action::ShowVersions);
    assert_eq!(app.inspector_tab, InspectorTab::Versions);
    let _ = app.update(Message::Tabs(tabs::Action::Select(local)));
    assert_eq!(app.tab.id, local);
    assert_eq!(app.inspector_tab, InspectorTab::Properties);
    assert_eq!(app.github.versions.target, None);
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
    // What differs is painted on both copies shown, never on the drawings.
    for shown in &compare.shown {
        let shown = shown.as_ref().unwrap();
        assert!(
            shown
                .atoms
                .iter()
                .any(|atom| atom.display.highlight.is_some())
        );
    }
    assert!(
        app.tab
            .doc
            .atoms
            .iter()
            .all(|atom| atom.display.highlight.is_none())
    );

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
    assert!(matches!(app.github.dialog, Some(Dialog::SignIn)));
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
    assert_eq!(app.status, "Kept both · yours is ring (2).rsk · sent");
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
    let shown = compare.shown[1].as_ref().unwrap();
    assert!(
        shown
            .atoms
            .iter()
            .any(|atom| atom.display.highlight.is_some())
    );
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
    // No last change is known; the drawings show.
    for name in names.iter().filter(|name| name.ends_with(".rsk")) {
        let unknown = picker::Action::Changed(generation, (*name).into(), None);
        github(app, Action::Picker(unknown));
    }
}

/// Lists the folder open in the picker with `folders` and `drawings`.
fn picker_lists(app: &mut App, folders: &[&str], drawings: &[&str]) -> u64 {
    let entry = |folder: bool| {
        move |name: &&str| gh::Entry {
            name: (*name).into(),
            sha: sha(if folder { '9' } else { '7' }),
            folder,
        }
    };
    let entries = (folders.iter().map(entry(true)))
        .chain(drawings.iter().map(entry(false)))
        .collect();
    let generation = picker_generation(app);
    let listing = picker::Action::Listing(generation, Ok((sha('8'), entries)));
    github(app, Action::Picker(listing));
    generation
}

fn picker_changed(app: &mut App, generation: u64, path: &str, date: Option<&str>) {
    let found = date.map(|date| ("@octo".to_owned(), date.to_owned()));
    let changed = picker::Action::Changed(generation, path.into(), found);
    github(app, Action::Picker(changed));
}

fn picker_names(app: &App) -> Vec<String> {
    let Some(Dialog::Picker(picker)) = &app.github.dialog else {
        panic!("no picker");
    };
    let entries = picker.entries.iter().flatten();
    entries.map(|entry| entry.name.clone()).collect()
}

fn picker_selected(app: &App) -> Option<usize> {
    match &app.github.dialog {
        Some(Dialog::Picker(picker)) => picker.selected,
        other => panic!("no picker: {other:?}"),
    }
}

#[test]
fn a_drawing_never_shows_what_another_of_its_name_was_last_changed_by() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    let other = Repo {
        id: 9,
        key: RepoKey {
            owner: "octo".into(),
            repo: "other".into(),
        },
        ..repo()
    };
    app.github.repos = vec![repo(), other.clone()];
    let picker = |action| Action::Picker(action);
    github(&mut app, picker(picker::Action::Show(Mode::Open)));
    github(&mut app, picker(picker::Action::PickRepo(repo().key)));
    let top = picker_lists(&mut app, &["sub"], &["ring.rsk"]);
    picker_changed(&mut app, top, "ring.rsk", Some("2026-03-01T00:00:00Z"));

    // Another `ring.rsk`, in a folder and in another repository, whose
    // last change GitHub does not tell: it sorts after the one known.
    github(&mut app, picker(picker::Action::Folder("sub".into())));
    let sub = picker_lists(&mut app, &[], &["a.rsk", "ring.rsk"]);
    picker_changed(&mut app, sub, "sub/a.rsk", Some("2025-01-01T00:00:00Z"));
    picker_changed(&mut app, sub, "sub/ring.rsk", None);
    assert_eq!(picker_names(&app), ["a.rsk", "ring.rsk"]);

    github(&mut app, picker(picker::Action::PickRepo(other.key)));
    let top = picker_lists(&mut app, &[], &["a.rsk", "ring.rsk"]);
    picker_changed(&mut app, top, "a.rsk", Some("2025-01-01T00:00:00Z"));
    picker_changed(&mut app, top, "ring.rsk", None);
    assert_eq!(picker_names(&app), ["a.rsk", "ring.rsk"]);
}

#[test]
fn a_download_still_running_never_closes_a_new_folder() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    picker_at(&mut app, Mode::Open, &["other.rsk"]);
    let picker = |action| Action::Picker(action);
    github(&mut app, picker(picker::Action::Select(0)));
    github(&mut app, picker(picker::Action::Open));
    let download = app.github.seq;
    github(&mut app, picker(picker::Action::NewItem(true)));
    github(
        &mut app,
        picker(picker::Action::NewItemName("fresh".into())),
    );
    github(&mut app, picker(picker::Action::CreateItem));

    let late = api::Download {
        commit: sha('c'),
        blob: sha('b'),
        bytes: drawing(42.),
    };
    let late = picker::Action::Downloaded(download, repo(), "other.rsk".into(), false, Ok(late));
    github(&mut app, picker(late));
    let Some(Dialog::Picker(picker)) = &app.github.dialog else {
        panic!("the picker closed");
    };
    assert_eq!(picker.trail, [("fresh".to_owned(), String::new())]);
    assert!(!workcopy::working_path(dir.path(), &repo().key, "other.rsk").exists());
}

#[test]
fn the_keyboard_selects_only_the_tiles_that_show() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    app.github.repos = vec![repo()];
    let picker = |action| Action::Picker(action);
    github(&mut app, picker(picker::Action::Show(Mode::Open)));
    github(&mut app, picker(picker::Action::PickRepo(repo().key)));
    // The drawings wait for their last changes; only the folder shows.
    let listed = picker_lists(&mut app, &["sub"], &["a.rsk", "b.rsk"]);
    for _ in 0..3 {
        github(&mut app, picker(picker::Action::Move(1)));
    }
    assert_eq!(picker_selected(&app), Some(0));
    // A selection on a hidden drawing opens nothing.
    github(&mut app, picker(picker::Action::Select(1)));
    let requests = app.github.seq;
    github(&mut app, picker(picker::Action::Enter));
    assert_eq!(app.github.seq, requests);

    // The order lands: the selection stays on `a.rsk`, now after `b.rsk`.
    picker_changed(&mut app, listed, "a.rsk", Some("2025-01-01T00:00:00Z"));
    picker_changed(&mut app, listed, "b.rsk", Some("2026-01-01T00:00:00Z"));
    assert_eq!(picker_names(&app), ["sub", "b.rsk", "a.rsk"]);
    assert_eq!(picker_selected(&app), Some(2));
    github(&mut app, picker(picker::Action::Move(-1)));
    assert_eq!(picker_selected(&app), Some(1));
}

#[test]
fn a_picker_opened_before_the_saved_sign_in_loads_lists_once_it_does() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    let tokens = Tokens {
        login: "octo".into(),
        access: "unused".into(),
        access_expires_at: None,
        refresh: None,
        refresh_expires_at: None,
    };
    app.github.client = None;
    app.github.account = Account::SignedOut(None);
    let picker = |app: &App| match &app.github.dialog {
        Some(Dialog::Picker(picker)) => (picker.signed_out, picker.repos.clone()),
        other => panic!("no picker: {other:?}"),
    };
    github(&mut app, Action::Picker(picker::Action::Show(Mode::Open)));
    assert!(picker(&app).0);

    let generation = app.github.generation;
    github(&mut app, Action::Loaded(generation, Ok(Some(tokens))));
    assert_eq!(picker(&app), (false, None), "listing");
    let account = gh::Account {
        login: "octo".into(),
        repos: vec![repo()],
    };
    github(&mut app, Action::Listed(generation, Ok(account)));
    assert_eq!(picker(&app), (false, Some(vec![repo()])));
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
fn send_here_never_takes_the_name_of_a_preview_already_there() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    app.add_tab();
    picker_at(&mut app, Mode::SendTo, &["x.svg"]);
    github(
        &mut app,
        Action::Picker(picker::Action::SaveName("x".into())),
    );
    github(&mut app, Action::Picker(picker::Action::SendHere));
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
    github(&mut app, picker(picker::Action::Show(Mode::SendTo)));
    github(&mut app, picker(picker::Action::PickRepo(repo().key)));
    let failed = picker_generation(&app);
    github(
        &mut app,
        picker(picker::Action::Listing(failed, Err(gh::Error::Offline))),
    );
    // GitHub may hold `x.svg`: nothing here knows.
    github(&mut app, picker(picker::Action::SaveName("x".into())));
    github(&mut app, picker(picker::Action::SendHere));
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
    github(&mut app, picker(picker::Action::SendHere));
    assert_eq!(picker_error(&app).as_deref(), Some("Name taken"));
    github(&mut app, picker(picker::Action::SaveName("y".into())));
    github(&mut app, picker(picker::Action::SendHere));
    assert!(app.tab.path.is_some());
}

#[test]
fn nothing_is_made_in_a_folder_that_could_not_be_listed() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    app.github.repos = vec![repo()];
    let picker = |action| Action::Picker(action);
    github(&mut app, picker(picker::Action::Show(Mode::Open)));
    github(&mut app, picker(picker::Action::PickRepo(repo().key)));
    let failed = picker_generation(&app);
    github(
        &mut app,
        picker(picker::Action::Listing(failed, Err(gh::Error::Offline))),
    );
    // GitHub may hold `x.rsk` or `x.svg`: nothing here knows.
    github(&mut app, picker(picker::Action::NewItem(false)));
    github(&mut app, picker(picker::Action::NewItemName("x".into())));
    github(&mut app, picker(picker::Action::CreateItem));
    assert!(matches!(app.github.dialog, Some(Dialog::Picker(_))));
    assert!(!workcopy::working_path(dir.path(), &repo().key, "x.rsk").exists());
}

#[test]
fn a_new_repository_with_a_renamed_ones_name_gets_its_own_folder() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    let old = app.github.states[&repo().key].clone();
    let folder = dir.path().join("octo/lab");
    std::fs::write(folder.join("state.json"), serde_json::to_vec(&old).unwrap()).unwrap();
    // `lab` was renamed on GitHub, and a new `lab` made.
    let new = Repo { id: 8, ..repo() };
    app.add_tab();
    app.github.repos = vec![new.clone()];
    let picker = |action| Action::Picker(action);
    github(&mut app, picker(picker::Action::Show(Mode::SendTo)));
    github(&mut app, picker(picker::Action::PickRepo(new.key.clone())));
    let generation = picker_generation(&app);
    github(
        &mut app,
        picker(picker::Action::Listing(generation, Ok((sha('8'), vec![])))),
    );
    github(&mut app, picker(picker::Action::SaveName("new".into())));
    github(&mut app, picker(picker::Action::SendHere));

    let path = app.tab.path.clone().expect("saved here");
    assert!(!path.starts_with(&folder), "{path:?}");
    assert_eq!(app.github.states[&repo().key], old);
    let (key, _) = app.github.locate(Some(&path)).unwrap();
    assert_eq!(app.github.states[&key].repo_id, 8);
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
    github(&mut app, Action::Picker(picker::Action::Select(0)));
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
    picker_changed(&mut app, generation, "ring.rsk", None);
    github(&mut app, Action::Picker(picker::Action::Select(0)));
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

#[test]
fn the_chip_says_what_is_left_to_do() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    let key = repo().key;
    assert_eq!(app.github_chip_state(&key), Chip::Sent);
    save_here(&mut app, &path);
    assert_eq!(app.github_chip_state(&key), Chip::Unsent(1));
    app.github.sending.insert(key.clone());
    assert_eq!(app.github_chip_state(&key), Chip::Sending);
    app.github.sending.clear();
    app.github.failed.insert(key.clone(), "offline".into());
    assert_eq!(app.github_chip_state(&key), Chip::NotSent);
    app.github.failed.clear();
    conflicted(&mut app, &path, Some(drawing(30.)));
    app.github.sending.clear();
    assert_eq!(app.github_chip_state(&key), Chip::Review);
}

#[test]
fn palette_rows_appear_only_when_they_apply() {
    let names = |app: &App| -> Vec<(String, bool)> {
        app.github_palette()
            .into_iter()
            .map(|(name, _, action)| (name, action.is_some()))
            .collect()
    };
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    let rows = names(&app);
    assert!(rows.contains(&("Nothing to send".into(), false)));
    assert!(rows.contains(&("History".into(), true)));
    assert!(rows.contains(&("Copy GitHub link".into(), true)));
    assert!(rows.contains(&("Sign out of GitHub".into(), true)));
    assert!(
        !rows
            .iter()
            .any(|(name, _)| name == "Review changes on GitHub")
    );

    save_here(&mut app, &path);
    let send = app
        .github_palette()
        .into_iter()
        .find(|(name, ..)| name == "Send 1 drawing");
    assert_eq!(
        send.map(|(_, detail, _)| detail).as_deref(),
        Some("ring.rsk → lab")
    );
    conflicted(&mut app, &path, Some(drawing(30.)));
    assert!(names(&app).contains(&("Review changes on GitHub".into(), true)));

    // The link names the file on the default branch, encoded, once GitHub
    // has it.
    let copy = workcopy::working_path(dir.path(), &repo().key, "ring (2).rsk");
    app.tab.path = Some(copy);
    assert_eq!(app.github_link(), None, "not on GitHub yet");
    let state = app.github.states.get_mut(&repo().key).unwrap();
    state.files.insert(
        "ring (2).rsk".into(),
        FileState {
            base_blob: Some(sha('6')),
            ..FileState::default()
        },
    );
    assert_eq!(
        app.github_link().as_deref(),
        Some("http://127.0.0.1:9/octo/lab/blob/main/ring%20%282%29.rsk")
    );
    // A drawing on this computer only: Send chooses where; no Versions.
    app.add_tab();
    let rows = names(&app);
    assert!(rows.contains(&("Send to GitHub…".into(), true)));
    assert!(!rows.iter().any(|(name, _)| name == "History"));
    app.github.account = Account::SignedOut(None);
    assert!(names(&app).contains(&("Sign in to GitHub…".into(), true)));
}

#[test]
fn repository_colors_are_kept_and_a_chosen_one_wins() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    let key = repo().key;
    let generation = app.github.generation;
    let account = gh::Account {
        login: "octo".into(),
        repos: vec![repo()],
    };
    github(&mut app, Action::Listed(generation, Ok(account)));
    let first = app.github.prefs.colors[&repo().id];
    assert_eq!(app.github.hue(&key), first);
    let other = (first + 2) % identity::HUES;
    github(&mut app, Action::Color(key.clone(), other));
    assert_eq!(app.github.hue(&key), other);
    assert!(app.tabs.color_menu.is_none());
}

#[test]
fn the_picker_lists_the_last_used_repository_first_and_opens_it() {
    let named = |name: &str, id: u64| Repo {
        id,
        key: RepoKey {
            owner: "octo".into(),
            repo: name.into(),
        },
        ..repo()
    };
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    app.github.repos = vec![named("zeta", 1), named("Alpha", 2), named("mid", 3)];
    app.github.prefs.last = Some(3);
    github(&mut app, Action::Picker(picker::Action::Show(Mode::Open)));
    let Some(Dialog::Picker(picker)) = &app.github.dialog else {
        panic!("no picker");
    };
    let order: Vec<&str> = picker
        .repos
        .iter()
        .flatten()
        .map(|repo| repo.key.repo.as_str())
        .collect();
    assert_eq!(order, ["mid", "Alpha", "zeta"]);
    assert_eq!(picker.repo.as_ref().map(|repo| repo.id), Some(3));
}

#[test]
fn signed_out_the_picker_shows_known_repositories_and_signs_in_first() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    let _ = save_here(&mut app, &path);
    let state = app.github.states[&repo().key].clone();
    run_blocking(workcopy::persist(dir.path(), &repo().key, state, 99)).unwrap();
    app.github.account = Account::SignedOut(None);
    github(&mut app, Action::Picker(picker::Action::Show(Mode::Open)));
    let Some(Dialog::Picker(picker)) = &app.github.dialog else {
        panic!("no picker");
    };
    assert!(picker.signed_out);
    assert_eq!(picker.repos.as_ref().map(Vec::len), Some(1));
    github(&mut app, Action::Picker(picker::Action::SignIn));
    assert!(matches!(app.github.dialog, Some(Dialog::SignIn)));
    // Once GitHub approves, the dialog closes and the picker comes back.
    let generation = app.github.generation;
    let account = gh::Account {
        login: "octo".into(),
        repos: vec![repo()],
    };
    github(&mut app, Action::SignedIn(generation, Ok(account)));
    assert_eq!(app.status, "Signed in as @octo");
    assert!(matches!(app.github.dialog, Some(Dialog::Picker(_))));
}

fn run_blocking<T>(future: impl Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}

#[test]
fn a_new_drawing_is_an_empty_working_copy_with_nothing_on_github() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    picker_at(&mut app, Mode::Open, &["x.svg"]);
    let new = |app: &mut App, name: &str| {
        github(app, Action::Picker(picker::Action::NewItem(false)));
        github(
            app,
            Action::Picker(picker::Action::NewItemName(name.into())),
        );
        github(app, Action::Picker(picker::Action::CreateItem));
    };
    new(&mut app, "x");
    assert_eq!(picker_error(&app).as_deref(), Some("Name taken"));
    // A name, never a path into another folder.
    new(&mut app, "scheme/ring");
    assert_eq!(
        picker_error(&app).as_deref(),
        Some("Can't open this name here")
    );
    new(&mut app, "route");
    assert!(app.github.dialog.is_none());
    let path = workcopy::working_path(dir.path(), &repo().key, "route.rsk");
    assert!(path.exists());
    assert_eq!(file_state(&app, "route.rsk"), Some(FileState::default()));
    // It is unsent from the start, as a new file.
    app.github_refresh(&repo().key);
    assert!(unsent(&app).contains(&"route.rsk".to_owned()));
}

#[test]
fn a_new_folder_opens_at_once_and_is_empty() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    picker_at(&mut app, Mode::Open, &["ring.rsk"]);
    github(&mut app, Action::Picker(picker::Action::NewItem(true)));
    github(
        &mut app,
        Action::Picker(picker::Action::NewItemName("scheme".into())),
    );
    github(&mut app, Action::Picker(picker::Action::CreateItem));
    let Some(Dialog::Picker(picker)) = &app.github.dialog else {
        panic!("no picker");
    };
    assert_eq!(picker.trail.len(), 1);
    assert_eq!(picker.entries.as_deref(), Some(&[][..]));
    // Backspace goes back up.
    github(&mut app, Action::Picker(picker::Action::Up));
    let Some(Dialog::Picker(picker)) = &app.github.dialog else {
        panic!("no picker");
    };
    assert!(picker.trail.is_empty());
}

#[test]
fn keep_githubs_says_how_to_get_yours_back() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    conflicted(&mut app, &path, Some(drawing(30.)));
    app.github.sending.clear();
    github(&mut app, Action::Resolve(send::Choice::KeepTheirs));
    let undo = crate::app::shortcuts::label(&Message::Undo).unwrap();
    assert_eq!(
        app.status,
        format!("Kept GitHub’s · {undo} brings yours back")
    );
}

#[test]
fn versions_name_saves_by_minute_and_grey_out_default_messages() {
    // A whole minute, so the first two share it.
    let at = |seconds: i64| (1_763_199_960_000 + seconds * 1000).to_string();
    let (a, b, c) = (at(0), at(20), at(600));
    let ids = [a.as_str(), b.as_str(), c.as_str()];
    let minute = |id: &str| versions::saved(id, &ids);
    // Two saves in one minute show their seconds; another does not.
    assert_eq!(minute(&a).matches(':').count(), 2);
    assert_eq!(minute(&c).matches(':').count(), 1);
    assert!(minute(&c).starts_with("Saved "));
    assert!(versions::default_message("Update ring.rsk"));
    assert!(versions::default_message("Update a.rsk and 3 more"));
    assert!(!versions::default_message("Add an ethyl group"));
}

#[test]
fn opening_takes_over_an_untouched_untitled_tab_and_sits_with_its_repository() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    let other = workcopy::working_path(dir.path(), &repo().key, "b.rsk");
    std::fs::write(&other, drawing(60.)).unwrap();
    // ring.rsk, then a local drawing with work in it, in front.
    app.add_tab();
    app.tab.doc = Document::from_native_file(&drawing(20.)).unwrap();
    // A new tab of the repository opens after its last tab, not at the end.
    assert!(app.open_target(Some(&other)));
    assert_eq!(app.strip().count(), 3);
    assert_eq!(app.tabs.active, 1);

    // An untouched Untitled tab, even behind the front one, gives way.
    let (mut app, _) = app_in(dir.path());
    app.add_tab();
    app.select_tab(0);
    assert!(app.open_target(Some(&other)));
    assert_eq!(app.strip().count(), 2, "the Untitled tab gave way");
    assert_eq!(app.tabs.active, 1);
}

#[test]
fn late_conflict_details_never_reach_another_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    conflicted(&mut app, &path, Some(drawing(30.)));
    let commit = app.github.conflicts[&repo().key][0].commit.clone();
    let found = api::RemoteVersion {
        commit: commit.clone(),
        message: "Swap in a pyridine N".into(),
        login: Some("alice".into()),
        author: "Alice".into(),
        date: "2026-11-16T14:02:00Z".into(),
    };
    let meta = |commit: &str| {
        Action::ReviewMeta(
            repo().key,
            "ring.rsk".into(),
            commit.into(),
            Some(found.clone()),
        )
    };
    github(&mut app, meta("an older commit"));
    assert!(review(&app).theirs.is_none());
    github(&mut app, meta(&commit));
    assert_eq!(review(&app).theirs.as_ref(), Some(&found));
}

#[test]
fn send_on_closing_while_signed_out_signs_in_then_sends() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    save_here(&mut app, &path);
    app.github.account = Account::SignedOut(None);
    let _ = app.update(Message::Tabs(tabs::Action::Close(None)));
    github(&mut app, Action::CloseAnswer(true));
    assert!(matches!(app.github.dialog, Some(Dialog::SignIn)));
    assert!(matches!(
        app.github.after_sign_in,
        Some(Action::CloseAnswer(true))
    ));
    // Canceling the sign-in leaves the close behind; the next close asks again.
    let _ = app.update(Message::Escape);
    assert!(app.github.closing.is_none());
}

#[test]
fn an_exact_github_word_ranks_its_rows_first_in_the_palette() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    save_here(&mut app, &path);
    let first = |app: &mut App, query: &str| {
        let _ = app.update(Message::Commands(crate::app::command_palette::Action::Open));
        let _ = app.update(Message::Commands(
            crate::app::command_palette::Action::Query(query.into()),
        ));
        let name = app
            .commands
            .ranked()
            .first()
            .map(|command| command.name.clone());
        let _ = app.update(Message::Escape);
        name
    };
    assert_eq!(first(&mut app, "history").as_deref(), Some("History"));
    assert_eq!(first(&mut app, "versions").as_deref(), Some("History"));
    assert_eq!(first(&mut app, "send").as_deref(), Some("Send 1 drawing"));
}

#[test]
fn the_chip_opens_the_picker_in_the_drawings_folder() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    app.github.repos = vec![repo()];
    let walk = |app: &mut App, folder: &str, listed: Vec<gh::Entry>| {
        github(
            app,
            Action::Picker(picker::Action::ShowIn(repo().key, folder.into())),
        );
        let generation = picker_generation(app);
        github(
            app,
            Action::Picker(picker::Action::Listing(generation, Ok((sha('8'), listed)))),
        );
    };
    let folder = gh::Entry {
        name: "scheme".into(),
        sha: sha('5'),
        folder: true,
    };
    walk(&mut app, "scheme", vec![folder.clone()]);
    let Some(Dialog::Picker(picker)) = &app.github.dialog else {
        panic!("no picker");
    };
    assert_eq!(picker.trail, [("scheme".to_owned(), sha('5'))]);
    assert!(picker.entries.is_none(), "listing scheme");

    // A folder GitHub does not have yet opens as a new one.
    walk(&mut app, "drafts/new", vec![folder]);
    let Some(Dialog::Picker(picker)) = &app.github.dialog else {
        panic!("no picker");
    };
    let trail: Vec<&str> = picker.trail.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(trail, ["drafts", "new"]);
    assert_eq!(picker.entries.as_deref(), Some(&[][..]));
}

/// A GitHub drawing's History is a clock just after the inspector's tab
/// track: in the same row, next in focus order, named for assistive
/// technology, opening History and then shown as selected. A drawing on this
/// computer has no clock, and its track takes the whole row.
#[tokio::test]
#[ignore = "Opt-in real renderer layout check"]
async fn the_history_clock_follows_the_tab_track_on_github_drawings() -> Result<(), String> {
    use iced::advanced::renderer::Headless;
    use iced::advanced::widget::{Id, Operation, operation};
    use reshiki::accessibility::Node;
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .ok_or("No renderer")?;
    /// The keyboard stops, in Tab order.
    struct Stops(Vec<Option<Id>>);
    impl Operation for Stops {
        fn focusable(
            &mut self,
            id: Option<&Id>,
            _: iced::Rectangle,
            _: &mut dyn operation::Focusable,
        ) {
            self.0.push(id.cloned());
        }
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }
    }
    let size = iced::Size::new(1280., 800.);
    let mut tree = |app: &App, click: Option<iced::Point>| {
        let mut ui = iced_runtime::UserInterface::build(
            app.view(),
            size,
            iced_runtime::user_interface::Cache::new(),
            &mut renderer,
        );
        let mut messages = Vec::new();
        if let Some(point) = click {
            use iced::mouse::{Button, Event};
            ui.update(
                &[
                    iced::Event::Mouse(Event::ButtonPressed(Button::Left)),
                    iced::Event::Mouse(Event::ButtonReleased(Button::Left)),
                ],
                iced::mouse::Cursor::Available(point),
                &mut renderer,
                &mut iced::advanced::clipboard::Null,
                &mut messages,
            );
        }
        let mut collect = reshiki::accessibility::Collect::new(iced::Rectangle::with_size(size));
        ui.operate(&renderer, &mut operation::black_box(&mut collect));
        let mut stops = Stops(Vec::new());
        ui.operate(&renderer, &mut stops);
        (collect.snapshot().nodes.clone(), messages, stops.0)
    };
    // The five tabs and what follows them, in focus order.
    let header = |nodes: &[Node]| -> Vec<Node> {
        let first = nodes
            .iter()
            .position(|node| node.id == "inspector-tab-Properties")
            .expect("tab track");
        nodes[first..first + 6].to_vec()
    };
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    app.inspector_open = true;
    let (nodes, _, stops) = tree(&app, None);
    let github = header(&nodes);
    let (export, clock) = (&github[4], &github[5]);
    assert_eq!(export.id, "inspector-tab-Export");
    assert_eq!(clock.id, "inspector-history");
    assert_eq!(clock.name, "History");
    assert_eq!(clock.checked, Some(false));
    assert!(clock.bounds.x >= export.bounds.x + export.bounds.width + 4.);
    assert!(clock.bounds.x + clock.bounds.width <= size.width);
    assert!((clock.bounds.center_y() - export.bounds.center_y()).abs() < 1.);
    for tab in &github[..5] {
        let label = tab.name.trim_end_matches(" tab");
        let width = crate::app::workspace::text_width(label, 12.);
        assert!(width + 6. <= tab.bounds.width, "{label}: {:?}", tab.bounds);
    }
    // Tab moves from Export to the clock.
    let id = |id: &str| Some(Id::from(id.to_owned()));
    let after = stops
        .iter()
        .position(|stop| *stop == id("inspector-tab-Export"))
        .expect("Export");
    assert_eq!(stops[after + 1], id("inspector-history"));

    let (_, messages, _) = tree(&app, Some(clock.bounds.center()));
    assert!(!messages.is_empty());
    for message in messages {
        let _ = app.update(message);
    }
    assert_eq!(app.inspector_tab, InspectorTab::Versions);
    let (nodes, ..) = tree(&app, None);
    let clock = &header(&nodes)[5];
    assert_eq!(clock.id, "inspector-history");
    assert_eq!(clock.checked, Some(true));

    app.add_tab();
    app.inspector_tab = InspectorTab::Properties;
    let (nodes, ..) = tree(&app, None);
    assert!(!nodes.iter().any(|node| node.id == "inspector-history"));
    let local = header(&nodes);
    assert_ne!(local[5].id, "inspector-history");
    // The track takes the clock's room.
    assert!(
        local[4].bounds.x + local[4].bounds.width > github[4].bounds.x + github[4].bounds.width
    );
    Ok(())
}

fn alice() -> Repo {
    Repo {
        id: 42,
        key: RepoKey {
            owner: "alice".into(),
            repo: "ligands".into(),
        },
        private: false,
        default_branch: "main".into(),
        push: false,
    }
}

const LIGANDS: &str = "https://github.com/alice/ligands";

fn the_picker(app: &App) -> &picker::Picker {
    match &app.github.dialog {
        Some(Dialog::Picker(picker)) => picker,
        other => panic!("no picker: {other:?}"),
    }
}

/// A link followed in the picker, and GitHub's answer for its repository.
fn resolved(app: &mut App, link: &str, answer: Result<Repo, gh::Error>) {
    github(app, Action::Picker(picker::Action::Link(link.into())));
    let found = gh::link::parse(link, &app.github.ends.web).unwrap();
    let request = app.github.seq;
    github(
        app,
        Action::Picker(picker::Action::Resolved(request, found, answer)),
    );
}

#[test]
fn a_pasted_link_shows_one_row_and_enter_opens_its_folder() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    app.github.repos = vec![repo()];
    github(&mut app, Action::Picker(picker::Action::Show(Mode::Open)));
    let link = "https://github.com/octo/lab/tree/main/scheme/drafts";
    github(
        &mut app,
        Action::Picker(picker::Action::Search(link.into())),
    );
    let found = the_picker(&app).link.clone().unwrap();
    assert_eq!(found.place(), "lab / scheme / drafts");
    // A search that is not a link filters the repositories as before.
    github(
        &mut app,
        Action::Picker(picker::Action::Search("la".into())),
    );
    assert!(the_picker(&app).link.is_none());
    github(
        &mut app,
        Action::Picker(picker::Action::Search(link.into())),
    );
    // A repository listed here needs no request: Enter opens the folder.
    github(&mut app, Action::Picker(picker::Action::Enter));
    let picker = the_picker(&app);
    assert_eq!(picker.repo.as_ref().map(|repo| repo.id), Some(repo().id));
    assert!(picker.link.is_none());
    let generation = picker_generation(&app);
    let folder = gh::Entry {
        name: "scheme".into(),
        sha: sha('5'),
        folder: true,
    };
    github(
        &mut app,
        Action::Picker(picker::Action::Listing(
            generation,
            Ok((sha('8'), vec![folder])),
        )),
    );
    let trail: Vec<&str> = (the_picker(&app).trail.iter())
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(trail, ["scheme"]);
}

#[test]
fn a_shared_repository_stays_listed_until_removed() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    app.github.repos = vec![repo()];
    resolved(&mut app, LIGANDS, Ok(alice()));
    assert!(app.github.prefs.is_shared(alice().id));
    let picker = the_picker(&app);
    assert_eq!(picker.repo.as_ref().map(|repo| repo.id), Some(alice().id));
    let listed: Vec<u64> = picker.repos.iter().flatten().map(|r| r.id).collect();
    assert_eq!(listed, [repo().id, alice().id]);
    // Listed again in the next picker, without asking GitHub.
    github(&mut app, Action::Close);
    github(&mut app, Action::Picker(picker::Action::Show(Mode::Open)));
    let listed = the_picker(&app).repos.clone().unwrap();
    assert!(
        listed
            .iter()
            .any(|repo| repo.id == alice().id && !repo.push)
    );
    // Send never offers it.
    github(&mut app, Action::Close);
    github(&mut app, Action::Picker(picker::Action::Show(Mode::SendTo)));
    let listed = the_picker(&app).repos.clone().unwrap();
    assert!(!listed.iter().any(|repo| repo.id == alice().id));
    github(&mut app, Action::Close);
    // Right-click, Remove from list.
    github(&mut app, Action::Picker(picker::Action::Show(Mode::Open)));
    github(
        &mut app,
        Action::Picker(picker::Action::PickRepo(alice().key)),
    );
    github(
        &mut app,
        Action::Picker(picker::Action::RowMenu(Some(alice().id))),
    );
    assert_eq!(the_picker(&app).menu, Some(alice().id));
    github(&mut app, Action::Picker(picker::Action::Forget(alice().id)));
    assert!(!app.github.prefs.is_shared(alice().id));
    let picker = the_picker(&app);
    assert!(picker.repo.is_none());
    assert!(!picker.repos.iter().flatten().any(|r| r.id == alice().id));
}

#[test]
fn add_public_repository_takes_a_name_or_a_link() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    github(&mut app, Action::Picker(picker::Action::Show(Mode::Open)));
    github(&mut app, Action::Picker(picker::Action::Add));
    github(
        &mut app,
        Action::Picker(picker::Action::AddName("ligands".into())),
    );
    github(&mut app, Action::Picker(picker::Action::AddRepo));
    assert_eq!(
        picker_error(&app).as_deref(),
        Some("Type owner/repo or a GitHub link")
    );
    github(&mut app, Action::Picker(picker::Action::Add));
    github(
        &mut app,
        Action::Picker(picker::Action::AddName("alice/ligands".into())),
    );
    github(&mut app, Action::Picker(picker::Action::AddRepo));
    let found = gh::link::parse(LIGANDS, "").unwrap();
    let request = app.github.seq;
    github(
        &mut app,
        Action::Picker(picker::Action::Resolved(request, found, Ok(alice()))),
    );
    assert!(app.github.prefs.is_shared(alice().id));
    assert_eq!(picker_error(&app), None);
}

/// Signed out, with nothing kept here but `alice/ligands`, shared.
fn signed_out(dir: &Path) -> App {
    let (mut app, _) = App::new();
    app.github.root = Some(dir.to_owned());
    app.github.client_id = Some("test".into());
    app.github.ends = nowhere();
    app.github.account = Account::SignedOut(None);
    app.github.prefs.share(&alice());
    app
}

#[test]
fn signed_out_public_repositories_list_with_names_only() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = signed_out(dir.path());
    github(&mut app, Action::Picker(picker::Action::Show(Mode::Open)));
    assert!(the_picker(&app).signed_out);
    github(
        &mut app,
        Action::Picker(picker::Action::PickRepo(alice().key)),
    );
    let generation = picker_generation(&app);
    let entries = ["index.rsk", "furan.rsk"]
        .map(|name| gh::Entry {
            name: name.into(),
            sha: sha('7'),
            folder: false,
        })
        .to_vec();
    let seq = app.github.seq;
    github(
        &mut app,
        Action::Picker(picker::Action::Listing(generation, Ok((sha('8'), entries)))),
    );
    // Names only: nobody is asked who changed them, nor for previews.
    assert_eq!(app.github.seq, seq);
    assert_eq!(picker_names(&app), ["furan.rsk", "index.rsk"]);
}

#[test]
fn signed_out_the_hourly_limit_asks_to_sign_in() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = signed_out(dir.path());
    app.github.prefs.shared.clear();
    resolved(&mut app, LIGANDS, Err(gh::Error::RateLimited(None)));
    let picker = the_picker(&app);
    assert!(picker.limited && picker.error.is_none());
    assert_eq!(
        picker::LIMITED,
        "GitHub's limit for signed-out use is reached. Sign in to continue."
    );
    github(&mut app, Action::Picker(picker::Action::SignIn));
    assert!(matches!(app.github.dialog, Some(Dialog::SignIn)));
}

#[test]
fn a_repository_github_will_not_show_cannot_be_opened_and_is_not_kept() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = signed_out(dir.path());
    app.github.prefs.shared.clear();
    resolved(
        &mut app,
        "https://github.com/alice/notes/blob/main/a.rsk",
        Err(gh::Error::NotFound),
    );
    let picker = the_picker(&app);
    assert_eq!(
        picker.cant_open.as_ref().map(|key| key.repo.as_str()),
        Some("notes")
    );
    assert!(picker.link.is_none());
    assert!(app.github.prefs.shared.is_empty());
    let key = picker.cant_open.clone().unwrap();
    assert_eq!(
        app.github.request(&key),
        "Please add ReShiki to alice/notes so I can send drawings: \
         http://127.0.0.1:9/apps/reshiki/installations/new"
    );
    github(&mut app, Action::CopyRequest(key));
    assert_eq!(app.status, "Request copied · Send it to the owner");
}

/// `ring.rsk` of `repo()` is read only: someone else's repository.
fn read_only_app(dir: &Path) -> (App, PathBuf) {
    let (mut app, path) = app_in(dir);
    app.github.states.get_mut(&repo().key).unwrap().read_only = true;
    (app, path)
}

#[test]
fn a_read_only_drawing_saves_here_and_offers_a_copy() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = read_only_app(dir.path());
    save_here(&mut app, &path);
    // Saved on this computer: a change, never asked to send.
    assert_eq!(unsent(&app), ["ring.rsk"]);
    assert!(!app.github_anything_unsent());
    assert!(app.github_before_close(app.tab.id).is_none());
    let (send, detail, action) = app.github_send_row();
    assert_eq!(
        (send.as_str(), detail.as_str()),
        ("Save a copy…", "Read only")
    );
    assert!(matches!(action, Some(Action::SaveCopy)));
    // The chip's menu, then Save a copy: the Send picker, name filled in.
    github(&mut app, Action::ChipMenu(true));
    assert!(app.github.chip_menu);
    github(&mut app, Action::ShowSend);
    assert!(!app.github.chip_menu);
    let picker = the_picker(&app);
    assert!(picker.copy);
    assert_eq!(picker.mode, Mode::SendTo);
    // Or this computer.
    github(&mut app, Action::Picker(picker::Action::ThisComputer));
    assert!(app.github.dialog.is_none());
    assert!(matches!(
        app.prompt,
        Some(crate::app::prompt::Prompt::SaveAs)
    ));
}

#[test]
fn access_given_later_ends_read_only_and_changes_count_as_unsent() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = read_only_app(dir.path());
    save_here(&mut app, &path);
    // On window focus, at most every five minutes.
    let _ = app.github_recheck();
    let checked = app.github.checked;
    assert!(checked.is_some());
    let _ = app.github_recheck();
    assert_eq!(app.github.checked, checked);
    // Still no access: nothing changes.
    let no_push = Repo {
        push: false,
        ..repo()
    };
    github(&mut app, Action::Access(repo().key, Ok(no_push)));
    assert!(app.github.read_only(&repo().key));
    github(&mut app, Action::Access(repo().key, Ok(repo())));
    assert!(!app.github.read_only(&repo().key));
    assert!(app.github_anything_unsent());
    assert_eq!(app.github_send_row().0, "Send 1 drawing");
    // A sign-in that lists it with access does the same.
    app.github.states.get_mut(&repo().key).unwrap().read_only = true;
    let generation = app.github.generation;
    let account = gh::Account {
        login: "octo".into(),
        repos: vec![repo()],
    };
    github(&mut app, Action::Listed(generation, Ok(account)));
    assert!(!app.github.read_only(&repo().key));
}

#[test]
fn a_branch_link_opens_an_old_version_kept_apart_and_never_sent() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, working) = app_in(dir.path());
    app.github.repos = vec![repo()];
    github(
        &mut app,
        Action::Picker(picker::Action::Link(
            "https://github.com/octo/lab/blob/draft/ring.rsk".into(),
        )),
    );
    let old = api::Download {
        commit: sha('d'),
        blob: sha('e'),
        bytes: drawing(30.),
    };
    let request = app.github.seq;
    github(
        &mut app,
        Action::Picker(picker::Action::Downloaded(
            request,
            repo(),
            "ring.rsk".into(),
            true,
            Ok(old.clone()),
        )),
    );
    assert!(app.github.dialog.is_none());
    let path = workcopy::old_path(dir.path(), &repo().key, &old.commit, "ring.rsk");
    assert_eq!(std::fs::read(&path).unwrap(), old.bytes);
    // The working copy is untouched.
    assert_eq!(std::fs::read(&working).unwrap(), drawing(42.));
    app.add_tab();
    app.show_opened(
        Document::from_native_file(&old.bytes).unwrap(),
        Some(path.clone()),
    );
    assert!(app.github.old(app.tab.path.as_deref()).is_some());
    // Edits stay here; nothing is sent; Cmd+S saves a copy.
    edit(&mut app);
    assert_eq!(app.github_title().as_deref(), Some("lab"));
    let spoken = app.github_tab(&app.tab, false).unwrap().spoken;
    assert!(spoken.ends_with("old version"));
    assert_eq!(
        app.github_link().as_deref(),
        Some("http://127.0.0.1:9/octo/lab/blob/main/ring.rsk")
    );
    assert!(!app.github_anything_unsent());
    assert_eq!(app.github_send_row().0, "Save a copy…");
    let _ = app.update(Message::Save);
    assert!(the_picker(&app).copy);
    github(&mut app, Action::Close);
    // Open latest: the working copy, already open, comes to the front.
    github(&mut app, Action::OpenLatest);
    assert!(app.github.dialog.is_none());
}

#[test]
fn authors_read_you_for_the_signed_in_user() {
    use super::versions::who;
    assert_eq!(who(Some("octo"), "Octo", Some("octo")), "You");
    assert_eq!(who(Some("alice"), "Alice", Some("octo")), "@alice");
    assert_eq!(who(Some("alice"), "Alice", None), "@alice");
    assert_eq!(who(None, "Alice Smith", Some("octo")), "Alice Smith");
}

#[test]
fn a_tabs_menu_copies_its_github_link() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    let id = app.tab.id;
    app.add_tab();
    app.tabs.color_menu = Some(id);
    github(&mut app, Action::CopyTabLink(id));
    assert_eq!(app.status, "Link copied");
    assert!(app.tabs.color_menu.is_none());
}

#[test]
fn the_palette_lists_known_drawings_and_opens_a_link() {
    let dir = tempfile::tempdir().unwrap();
    let (app, path) = app_in(dir.path());
    let state = app.github.states[&repo().key].clone();
    run_blocking(workcopy::persist(dir.path(), &repo().key, state, 99)).unwrap();
    let rows = app.github_palette();
    let row = |name: &str| rows.iter().find(|(row, ..)| row == name);
    let (_, detail, action) = row("Open from GitHub: lab/ring").unwrap();
    assert_eq!(detail, "octo/lab · ring.rsk");
    assert!(matches!(action, Some(Action::OpenKnown(known)) if *known == path));
    let (_, detail, action) = row("Open from GitHub: lab").unwrap();
    assert_eq!(detail, "octo/lab");
    assert!(matches!(
        action,
        Some(Action::Picker(picker::Action::ShowIn(..)))
    ));
    let link = "https://github.com/alice/ligands/blob/main/esters/route.rsk";
    let (name, detail, _) = app.github_link_row(link).unwrap();
    assert_eq!(name, "Open from GitHub: ligands/esters/route");
    assert_eq!(detail, "alice/ligands · main");
    assert!(app.github_link_row("benzene").is_none());
}

#[test]
fn a_link_pasted_or_dropped_on_the_window_opens() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    github(&mut app, Action::Pasted(Some("CCO".into())));
    assert!(app.github.dialog.is_none());
    github(&mut app, Action::Pasted(Some(LIGANDS.into())));
    assert_eq!(the_picker(&app).search, LIGANDS);
    github(&mut app, Action::Close);
    let webloc = dir.path().join("ligands.webloc");
    std::fs::write(
        &webloc,
        format!("<plist><dict><key>URL</key><string>{LIGANDS}</string></dict></plist>"),
    )
    .unwrap();
    assert!(app.github_dropped(&[webloc]).is_some());
    assert_eq!(the_picker(&app).search, LIGANDS);
    // Any other file is imported as before.
    let other = dir.path().join("ring.mol");
    std::fs::write(&other, "").unwrap();
    assert!(app.github_dropped(&[other]).is_none());
}
