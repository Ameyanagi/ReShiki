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
    let doc = Arc::new(Document::from_native_file(&drawing(30.)).unwrap());
    versions(
        &mut app,
        versions::Action::Doc(generation, sha('1'), Ok(doc)),
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
    let doc = Arc::new(Document::from_native_file(&drawing(30.)).unwrap());
    versions(
        &mut app,
        versions::Action::Doc(generation, sha('1'), Ok(doc)),
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
}
