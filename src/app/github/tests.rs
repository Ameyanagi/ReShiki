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

/// An app whose GitHub folder is `dir`, signed in, with `ring.rsk` opened
/// from GitHub and in front.
fn app_in(dir: &Path) -> (App, PathBuf) {
    let (mut app, _) = App::new();
    let github = &mut app.github;
    github.root = Some(dir.to_owned());
    github.client_id = Some("test".into());
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
    let _ = app.github_saved(Some(path.to_owned()));
    bytes
}

fn github(app: &mut App, action: Action) {
    let _ = app.update(Message::GitHub(action));
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
    github(&mut app, Action::SignedIn(generation, Ok((account, true))));
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
    assert!(app.tab.name().starts_with("ring · "));
}
