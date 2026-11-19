//! History: GitHub's versions listed at once, each drawing read when its row
//! shows, and kept for the session.
use super::*;

fn version(c: char) -> api::RemoteVersion {
    api::RemoteVersion {
        commit: sha(c),
        message: format!("Change {c}"),
        login: Some("alice".into()),
        author: "alice".into(),
        date: "2026-11-16T14:02:00Z".into(),
    }
}

fn listed(app: &App) -> Option<Vec<(String, bool)>> {
    app.github.versions.listed()
}

fn versions(app: &mut App, action: versions::Action) -> usize {
    app.update(Message::GitHub(Action::Versions(action)))
        .units()
}

/// `app_in`'s ring.rsk with History open, GitHub's commits `1` and `2`
/// listed.
fn listing(dir: &Path) -> App {
    let (mut app, _) = app_in(dir);
    github(&mut app, Action::ShowVersions);
    let generation = app.github.versions.generation;
    let found = Ok(vec![version('1'), version('2')]);
    // Listed, and nothing more asked: no drawing is read yet.
    let asked = versions(&mut app, versions::Action::RemoteLoaded(generation, found));
    assert_eq!(asked, 0);
    app
}

fn legacy_listing(dir: &Path) -> (App, PathBuf) {
    let raw: &[u8] = include_bytes!("../../../../docs/fixtures/qa-property-continuity/anisole.rsk");
    let doc = Document::from_native_file(raw).unwrap();
    assert_ne!(doc.file_json().unwrap().as_slice(), raw);

    let mut app = listing(dir);
    let path = app.tab.path.clone().unwrap();
    std::fs::write(&path, raw).unwrap();
    let state = app.github.states.get_mut(&repo().key).unwrap();
    let file = state.files.get_mut("ring.rsk").unwrap();
    file.base_commit = sha('1');
    file.base_hash = workcopy::hash(raw);
    app.show_opened(doc.clone(), Some(path.clone()));
    read_back(&mut app);
    assert!(unsent(&app).is_empty());

    let generation = app.github.versions.generation;
    versions(
        &mut app,
        versions::Action::Doc(
            generation,
            sha('1'),
            Ok((Arc::new(doc), workcopy::hash(raw))),
        ),
    );
    (app, path)
}

#[test]
fn restoring_the_tracked_github_version_leaves_nothing_to_send() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = legacy_listing(dir.path());
    let save = |app: &mut App| {
        let doc = app.tab.doc.clone();
        let bytes = doc.file_json().unwrap();
        std::fs::write(&path, &bytes).unwrap();
        app.tab.mark_saved(doc.clone());
        app.file_io.saving = false;
        let task = app.github_saved(true, Some((path.clone(), doc)));
        land(app, task);
        read_back(app);
        bytes
    };

    edit(&mut app);
    let moved = app.tab.doc.atoms[0].position;
    save(&mut app);
    assert_eq!(unsent(&app), ["ring.rsk"]);

    let pick = versions::Pick::Remote(sha('1'));
    versions(&mut app, versions::Action::Select(pick));
    versions(&mut app, versions::Action::Restore);
    let restored = save(&mut app);
    assert!(unsent(&app).is_empty());
    let key = repo().key;
    let saves = workcopy::snapshots(dir.path(), &key, "ring.rsk").unwrap();
    assert!(
        saves
            .into_iter()
            .any(|save| std::fs::read(save.path).unwrap() == restored)
    );
    let kept = workcopy::load(dir.path(), &key).unwrap().unwrap();
    assert_eq!(kept.files["ring.rsk"].base_hash, workcopy::hash(&restored));

    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc.atoms[0].position, moved);
    save(&mut app);
    assert_eq!(unsent(&app), ["ring.rsk"]);
}

#[test]
fn restoring_another_github_version_stays_unsent() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = legacy_listing(dir.path());
    let base = file_state(&app, "ring.rsk").unwrap().base_hash;
    let bytes = drawing(30.);
    let doc = Arc::new(Document::from_native_file(&bytes).unwrap());
    let generation = app.github.versions.generation;
    versions(
        &mut app,
        versions::Action::Doc(
            generation,
            sha('2'),
            Ok((doc.clone(), workcopy::hash(&bytes))),
        ),
    );
    let pick = versions::Pick::Remote(sha('2'));
    versions(&mut app, versions::Action::Select(pick));
    versions(&mut app, versions::Action::Restore);
    assert_eq!(&app.tab.doc, doc.as_ref());
    std::fs::write(&path, app.tab.doc.file_json().unwrap()).unwrap();
    app.tab.mark_saved(app.tab.doc.clone());
    app.file_io.saving = false;
    let task = app.github_saved(true, Some((path, app.tab.doc.clone())));
    land(&mut app, task);
    read_back(&mut app);
    assert_eq!(unsent(&app), ["ring.rsk"]);
    assert_eq!(file_state(&app, "ring.rsk").unwrap().base_hash, base);
}

#[test]
fn an_edit_after_restoring_the_tracked_version_is_unsent() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = legacy_listing(dir.path());
    let pick = versions::Pick::Remote(sha('1'));
    versions(&mut app, versions::Action::Select(pick));
    versions(&mut app, versions::Action::Restore);
    std::fs::write(&path, app.tab.doc.file_json().unwrap()).unwrap();
    app.tab.mark_saved(app.tab.doc.clone());
    app.file_io.saving = false;
    let task = app.github_saved(true, Some((path.clone(), app.tab.doc.clone())));
    land(&mut app, task);
    read_back(&mut app);
    assert!(unsent(&app).is_empty());

    edit(&mut app);
    std::fs::write(&path, app.tab.doc.file_json().unwrap()).unwrap();
    app.tab.mark_saved(app.tab.doc.clone());
    let task = app.github_saved(true, Some((path, app.tab.doc.clone())));
    land(&mut app, task);
    read_back(&mut app);
    assert_eq!(unsent(&app), ["ring.rsk"]);
}

#[test]
fn history_lists_first_and_reads_a_drawing_when_its_row_shows() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = listing(dir.path());
    assert_eq!(
        listed(&app),
        Some(vec![(sha('1'), false), (sha('2'), false)])
    );
    // Its row shows: its drawing is asked for, once.
    assert!(versions(&mut app, versions::Action::Shown(sha('1'))) > 0);
    assert_eq!(versions(&mut app, versions::Action::Shown(sha('1'))), 0);
    let generation = app.github.versions.generation;
    let bytes = drawing(30.);
    let doc = Arc::new(Document::from_native_file(&bytes).unwrap());
    versions(
        &mut app,
        versions::Action::Doc(generation, sha('1'), Ok((doc, workcopy::hash(&bytes)))),
    );
    assert_eq!(
        listed(&app),
        Some(vec![(sha('1'), true), (sha('2'), false)])
    );
}

#[test]
fn history_shows_a_drawings_versions_again_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = listing(dir.path());
    let generation = app.github.versions.generation;
    let bytes = drawing(30.);
    let doc = Arc::new(Document::from_native_file(&bytes).unwrap());
    versions(
        &mut app,
        versions::Action::Doc(generation, sha('1'), Ok((doc, workcopy::hash(&bytes)))),
    );
    // Another drawing, then this one again: what was listed and read shows
    // at once, while the list is asked again.
    open_other(&mut app, dir.path());
    let _ = app.github_follow();
    assert_eq!(listed(&app), None);
    app.select_tab(0);
    assert!(app.github_follow().units() > 0, "listed again");
    assert_eq!(
        listed(&app),
        Some(vec![(sha('1'), true), (sha('2'), false)])
    );
    // A commit made since comes first; what was read stays read.
    let generation = app.github.versions.generation;
    let found = Ok(vec![version('3'), version('1'), version('2')]);
    versions(&mut app, versions::Action::RemoteLoaded(generation, found));
    assert_eq!(
        listed(&app),
        Some(vec![(sha('3'), false), (sha('1'), true), (sha('2'), false)])
    );
    // A drawing read before is never asked again.
    assert_eq!(versions(&mut app, versions::Action::Shown(sha('1'))), 0);
}

#[test]
fn signing_out_forgets_what_history_kept() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = listing(dir.path());
    open_other(&mut app, dir.path());
    let _ = app.github_follow();
    // Signed out and in again: nothing of the last sign-in shows first.
    let client = app.github.client.clone();
    app.github_signed_out(None);
    app.github.client = client;
    app.github.account = Account::SignedIn {
        login: "octo".into(),
        remembered: true,
    };
    app.select_tab(0);
    let _ = app.github_follow();
    assert_eq!(listed(&app), None, "listed again from GitHub");

    // What shows while signed out goes too, and a late reply drops.
    let generation = app.github.versions.generation;
    app.github_signed_out(None);
    assert_eq!(listed(&app), Some(vec![]));
    let found = Ok(vec![version('1')]);
    versions(&mut app, versions::Action::RemoteLoaded(generation, found));
    assert_eq!(listed(&app), Some(vec![]));
}

#[test]
fn a_drawing_whose_row_left_the_list_is_asked_again_when_it_shows() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = listing(dir.path());
    open_other(&mut app, dir.path());
    let _ = app.github_follow();
    app.select_tab(0);
    let _ = app.github_follow();
    // The kept row 2 shows and is asked for; meanwhile GitHub's first page
    // has moved on without it.
    assert!(versions(&mut app, versions::Action::Shown(sha('2'))) > 0);
    let generation = app.github.versions.generation;
    let found = Ok(vec![version('3'), version('1')]);
    versions(&mut app, versions::Action::RemoteLoaded(generation, found));
    let bytes = drawing(30.);
    let doc = Arc::new(Document::from_native_file(&bytes).unwrap());
    versions(
        &mut app,
        versions::Action::Doc(generation, sha('2'), Ok((doc, workcopy::hash(&bytes)))),
    );
    // Load more brings it back: shown again, it is asked again.
    let found = Ok(vec![version('1'), version('2')]);
    versions(&mut app, versions::Action::RemoteLoaded(generation, found));
    assert_eq!(
        listed(&app),
        Some(vec![
            (sha('3'), false),
            (sha('1'), false),
            (sha('2'), false)
        ])
    );
    assert!(versions(&mut app, versions::Action::Shown(sha('2'))) > 0);
}
