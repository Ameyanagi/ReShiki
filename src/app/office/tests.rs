use super::super::{document_tab::DocumentTab, tabs};
use super::*;
use reshiki::document::{Document, Point};

fn open(app: &mut App, name: &str, host: Host, token: u8) -> (TabId, Lease) {
    let path = PathBuf::from(name);
    let lease = Lease::new([token; 16], || true);
    let binding = Binding {
        path: path.clone(),
        host,
        lease: Some(lease.clone()),
    };
    let mut doc = Document::default();
    doc.add_atom("O", Point::default());
    let _ = app.update(Message::OfficePrepared(
        binding,
        Some((path, Ok(files::Prepared::Native(Box::new(doc))))),
    ));
    assert_eq!(lease.phase(), Phase::Open);
    (app.tab.id, lease)
}

#[test]
fn two_office_tabs_keep_their_own_host_and_ordinary_duplicates_keep_binding() {
    let (mut app, _) = App::new();
    let (a, lease_a) = open(&mut app, "a.rsk", Host::Office, 1);
    let (b, lease_b) = open(&mut app, "b.rsk", Host::Microsoft365, 2);
    assert_eq!(app.strip().count(), 2);
    assert_eq!(app.tab.office.as_ref().unwrap().host, Host::Microsoft365);
    let _ = app.update(Message::Tabs(tabs::Action::Select(a)));
    assert!(app.office_document());
    assert_eq!(app.tab.office.as_ref().unwrap().host, Host::Office);
    let before = app.tab.doc.clone();
    let _ = app.update(Message::FilePrepared(Some((
        "a.rsk".into(),
        Ok(files::Prepared::Native(Box::default())),
    ))));
    assert_eq!(app.tab.doc, before);
    assert_eq!(
        app.tab
            .office
            .as_ref()
            .unwrap()
            .lease
            .as_ref()
            .unwrap()
            .token,
        lease_a.token
    );
    let _ = app.update(Message::Tabs(tabs::Action::Close(Some(a))));
    assert_eq!(lease_a.phase(), Phase::Closed);
    assert_eq!(lease_b.phase(), Phase::Open);
    assert_eq!(app.tab.id, b);
}

#[test]
fn different_session_cannot_take_an_office_path_or_dirty_ordinary_tab() {
    let (mut app, _) = App::new();
    let (_, original) = open(&mut app, "a.rsk", Host::Office, 1);
    let rejected = Lease::new([2; 16], || true);
    let binding = Binding {
        path: "a.rsk".into(),
        host: Host::LibreOffice,
        lease: Some(rejected.clone()),
    };
    let _ = app.update(Message::OfficePrepared(
        binding,
        Some(("a.rsk".into(), Ok(files::Prepared::Native(Box::default())))),
    ));
    assert_eq!(rejected.phase(), Phase::Rejected);
    assert_eq!(original.phase(), Phase::Open);
    assert_eq!(app.strip().count(), 1);
    assert_eq!(app.tab.office.as_ref().unwrap().host, Host::Office);

    let (mut ordinary, _) = App::new();
    let _ = ordinary.update(Message::FilePrepared(Some((
        "dirty.rsk".into(),
        Ok(files::Prepared::Native(Box::default())),
    ))));
    ordinary.tab.doc.add_atom("N", Point::default());
    let before = ordinary.tab.doc.clone();
    let rejected = Lease::new([3; 16], || true);
    let _ = ordinary.update(Message::OfficePrepared(
        Binding {
            path: "dirty.rsk".into(),
            host: Host::Office,
            lease: Some(rejected.clone()),
        },
        Some((
            "dirty.rsk".into(),
            Ok(files::Prepared::Native(Box::default())),
        )),
    ));
    assert_eq!(rejected.phase(), Phase::Rejected);
    assert_eq!(ordinary.tab.doc, before);
    assert!(ordinary.tab.office.is_none());
}

#[test]
fn background_close_waits_for_pending_save_then_completes_only_that_session() {
    let (mut app, _) = App::new();
    let (a, lease_a) = open(&mut app, "a.rsk", Host::Office, 1);
    let epoch = app.tab.file_epoch;
    let snapshot = app.tab.doc.clone();
    app.file_io.saving = true;
    app.file_io.saving_tab = Some(a);
    let (b, lease_b) = open(&mut app, "b.rsk", Host::LibreOffice, 2);
    let _ = app.update(Message::Tabs(tabs::Action::Close(Some(a))));
    assert!(app.tab_index(a).is_some());
    assert_eq!(lease_a.phase(), Phase::Open);
    let _ = app.update(Message::Tab(
        a,
        Box::new(Message::Saved(
            epoch,
            Box::new(snapshot),
            Ok(Some("a.rsk".into())),
        )),
    ));
    assert!(app.tab_index(a).is_none());
    assert_eq!(lease_a.phase(), Phase::Closed);
    assert_eq!(lease_b.phase(), Phase::Open);
    assert_eq!(app.tab.id, b);
}

#[test]
fn failed_or_cancelled_save_keeps_session_and_cancels_queued_close() {
    for result in [Err("Host did not acknowledge the update".into()), Ok(None)] {
        let (mut app, _) = App::new();
        let (id, lease) = open(&mut app, "a.rsk", Host::Microsoft365, 1);
        let epoch = app.tab.file_epoch;
        let snapshot = app.tab.doc.clone();
        app.file_io.saving = true;
        app.file_io.saving_tab = Some(id);
        let _ = app.update(Message::Tabs(tabs::Action::Close(Some(id))));
        let _ = app.update(Message::Saved(epoch, Box::new(snapshot), result));
        assert!(app.tab_index(id).is_some());
        assert_eq!(lease.phase(), Phase::Open);
        assert!(app.office.closing.is_empty());
    }
}

#[test]
fn save_as_detaches_only_after_successful_write_to_a_different_file() {
    let (mut app, _) = App::new();
    let (_, lease) = open(&mut app, "a.rsk", Host::Office, 1);
    let epoch = app.tab.file_epoch;
    let snapshot = app.tab.doc.clone();
    let _ = app.update(Message::Saved(epoch, Box::new(snapshot.clone()), Ok(None)));
    assert_eq!(lease.phase(), Phase::Open);
    assert!(app.tab.office.is_some());
    let _ = app.update(Message::Saved(
        epoch,
        Box::new(snapshot),
        Ok(Some("ordinary.rsk".into())),
    ));
    assert_eq!(lease.phase(), Phase::Closed);
    assert!(app.tab.office.is_none());
    assert_eq!(app.tab.path, Some("ordinary.rsk".into()));
}

#[test]
fn cancelled_window_close_finishes_none_and_committed_exit_finishes_all() {
    let (mut app, _) = App::new();
    let (_, a) = open(&mut app, "a.rsk", Host::Office, 1);
    let (_, b) = open(&mut app, "b.rsk", Host::Microsoft365, 2);
    app.tab.doc.add_atom("N", Point::default());
    let _ = app.update(Message::Close(iced::window::Id::unique()));
    let _ = app.update(Message::Cancel);
    assert_eq!([a.phase(), b.phase()], [Phase::Open, Phase::Open]);
    app.commit_exit();
    assert_eq!([a.phase(), b.phase()], [Phase::Closed, Phase::Closed]);
}

#[test]
fn replacing_an_office_drawing_keeps_the_session_on_its_original_tab() {
    let (mut app, _) = App::new();
    let (original, lease) = open(&mut app, "a.rsk", Host::Office, 1);
    app.reset_tab();
    assert_ne!(app.tab.id, original);
    assert_eq!(lease.phase(), Phase::Open);
    assert!(app.tab.office.is_none());
    assert!(
        app.tabs
            .background
            .iter()
            .any(|tab: &DocumentTab| tab.id == original && tab.office.is_some())
    );
}
