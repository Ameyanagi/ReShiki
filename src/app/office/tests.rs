use super::super::{document_tab::DocumentTab, tabs};
use super::*;
use crate::session::{Host, Lease};
use reshiki::document::{Document, Point};
use std::{
    path::PathBuf,
    sync::{Arc, atomic::Ordering},
};

#[test]
fn binding_admission_preserves_standalone_and_watched_lease_rules() {
    let standalone = Binding::standalone("standalone.rsk".into(), Host::Office);
    assert!(standalone.preparable());
    assert!(!standalone.has_token([1; 16]));
    standalone.reject();
    assert!(standalone.preparable());

    let alive = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let watched = Arc::clone(&alive);
    let lease = Lease::new([1; 16], move || watched.load(Ordering::Acquire));
    let binding = Binding {
        path: "watched.rsk".into(),
        host: Host::Office,
        lease: Some(lease.clone()),
    };
    assert!(binding.has_token([1; 16]));
    assert!(!binding.has_token([2; 16]));
    assert!(binding.preparable());
    alive.store(false, Ordering::Release);
    assert!(!binding.preparable());
    alive.store(true, Ordering::Release);
    binding.reject();
    assert_eq!(lease.phase(), Phase::Rejected);
    assert!(!binding.preparable());
    binding.reject();
    assert_eq!(lease.phase(), Phase::Rejected);

    let lease = Lease::new([2; 16], || true);
    let binding = Binding {
        path: "open.rsk".into(),
        host: Host::LibreOffice,
        lease: Some(lease.clone()),
    };
    assert!(lease.transition(Phase::Prepared, Phase::Open));
    assert!(!binding.preparable());
    binding.reject();
    assert_eq!(lease.phase(), Phase::Open);
}

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
fn accessibility_bypasses_the_routing_tail_but_office_prepared_runs_it() {
    let (mut app, _) = App::new();
    let (id, lease) = open(&mut app, "closing.rsk", Host::Office, 1);
    app.office.closing.push(id);
    let _ = app.update(Message::Accessibility(
        super::super::accessibility::Action::Refresh,
    ));
    assert_eq!(app.office.closing, [id]);
    assert!(app.tab_index(id).is_some());
    assert_eq!(lease.phase(), Phase::Open);

    let _ = app.update(Message::OfficePrepared(
        Binding::standalone("cancelled.rsk".into(), Host::LibreOffice),
        None,
    ));
    assert!(app.office.closing.is_empty());
    assert!(app.tab_index(id).is_none());
    assert_eq!(lease.phase(), Phase::Closed);
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

#[test]
fn bound_office_generic_expansion_delivers_after_tab_switch_without_rebinding_or_replay() {
    use crate::app::generic::Action as GenericAction;
    use reshiki::generic::{Limits, expand_prepared, prepare_expansion_input};

    let history = |tab: &DocumentTab| {
        (
            tab.history.undo_frames().cloned().collect::<Vec<_>>(),
            tab.history.redo_frames().cloned().collect::<Vec<_>>(),
        )
    };
    let (mut app, _) = App::new();
    let path = PathBuf::from("generic-office.rsk");
    let (source_id, lease) = open(&mut app, "generic-office.rsk", Host::Office, 9);
    let before = app.tab.doc.clone();
    app.tab.doc.atom_mut(1).unwrap().element = "C".into();
    let list = app.tab.doc.add_atom("*", Point::new(42., 0.));
    app.tab.doc.add_bond(1, list, 1, "plain");
    app.tab.doc = reshiki::atom_text::apply(
        &app.tab.doc,
        list,
        "[Cl,Br]",
        reshiki::atom_text::Mode::Auto,
    )
    .unwrap();
    app.changed(before);
    let before = app.tab.doc.clone();
    let note = app.tab.doc.next_id();
    app.tab.doc.annotations.push(reshiki::document::Annotation {
        id: note,
        position: Point::new(0., 100.),
        text: "source-only redo".into(),
        format: Default::default(),
    });
    app.changed(before);
    let _ = app.update(Message::Undo);
    assert!(app.tab.history.can_undo() && app.tab.history.can_redo());
    app.tab.camera.center = Point::new(123., -45.);
    app.tab.camera.zoom = 1.75;
    app.tab.fit_to_view = false;
    let source = app.tab.doc.clone();
    let source_history = history(&app.tab);
    let source_camera = app.tab.camera;
    let source_epoch = app.tab.file_epoch;
    let source_revision = app.tab.revision;
    let prepared =
        expand_prepared(prepare_expansion_input(&source, &[], Limits::default()).unwrap()).unwrap();
    let delivered = GenericAction::Expanded {
        source: source_id,
        epoch: source_epoch,
        result: Box::new(Ok(prepared)),
    };
    // Exercise the real request bookkeeping. Its Task is not polled here;
    // model-prepared delivery is sent through the same originating-tab tag.
    let _ = app.update(Message::Generic(GenericAction::Expand));
    assert!(!app.error, "{}", app.status);
    let _ = app.update(Message::New);
    let foreground = app.tab.id;
    assert_ne!(foreground, source_id);
    let before = app.tab.doc.clone();
    app.tab.doc.add_atom("F", Point::new(400., 100.));
    app.changed(before);
    app.tab.camera.center = Point::new(-210., 30.);
    app.tab.camera.zoom = 0.75;
    app.tab.fit_to_view = false;
    let foreground_doc = app.tab.doc.clone();
    let foreground_history = history(&app.tab);
    let foreground_camera = app.tab.camera;
    assert_eq!(app.strip().count(), 2);

    // A stale completion must neither create a tab nor consume the valid
    // prepared capsule that this request will deliver next.
    let GenericAction::Expanded { result, .. } = delivered.clone() else {
        unreachable!()
    };
    let _ = app.update(Message::Tab(
        source_id,
        Box::new(Message::Generic(GenericAction::Expanded {
            source: source_id,
            epoch: source_epoch.wrapping_add(1),
            result,
        })),
    ));
    assert_eq!(app.strip().count(), 2);
    assert_eq!(app.tab.id, foreground);
    assert_eq!(app.tab.doc, foreground_doc);
    assert_eq!(history(&app.tab), foreground_history);

    let _ = app.update(Message::Tab(
        source_id,
        Box::new(Message::Generic(delivered.clone())),
    ));
    assert!(!app.error, "{}", app.status);
    let output_id = app.tab.id;
    assert_ne!(output_id, source_id);
    assert_ne!(output_id, foreground);
    assert_eq!(app.strip().count(), 3);
    assert_eq!(app.strip().filter(|tab| tab.office.is_some()).count(), 1);
    assert!(app.tab.office.is_none() && app.tab.path.is_none());
    assert!(app.drawing_save_target(false).0.is_none());
    assert!(app.tab.doc.generic.is_empty());
    assert_eq!(app.tab.doc.atoms.len(), 4);
    assert_eq!(app.tab.doc.bonds.len(), 2);
    assert_eq!(
        app.tab
            .doc
            .atoms
            .iter()
            .filter(|a| a.element == "Cl")
            .count(),
        1
    );
    assert_eq!(
        app.tab
            .doc
            .atoms
            .iter()
            .filter(|a| a.element == "Br")
            .count(),
        1
    );
    let output = app.tab.doc.clone();
    assert_eq!(app.tab.history.frames(), 1);
    let _ = app.update(Message::Undo);
    assert!(app.tab.doc.object_ids().next().is_none());
    assert!(!app.tab.history.can_undo() && app.tab.history.can_redo());
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, output);
    assert!(app.tab.history.can_undo() && !app.tab.history.can_redo());
    let output_history = history(&app.tab);

    // Ordinary duplicated delivery is ignored after pending is cleared.
    let _ = app.update(Message::Tab(
        source_id,
        Box::new(Message::Generic(delivered.clone())),
    ));
    assert_eq!(app.strip().count(), 3);
    assert_eq!(app.tab.id, output_id);
    assert_eq!(app.tab.doc, output);
    assert_eq!(history(&app.tab), output_history);
    // Re-arm via the real command on the same initiating tab: an old cloned
    // capability must still fail before add_tab, even with a matching key.
    let _ = app.update(Message::Tabs(tabs::Action::Select(source_id)));
    assert_eq!(app.tab.doc, source);
    assert_eq!(history(&app.tab), source_history);
    assert_eq!(
        (app.tab.camera.center, app.tab.camera.zoom),
        (source_camera.center, source_camera.zoom)
    );
    let _ = app.update(Message::Generic(GenericAction::Expand));
    let _ = app.update(Message::Tabs(tabs::Action::Select(output_id)));
    let _ = app.update(Message::Tab(
        source_id,
        Box::new(Message::Generic(delivered)),
    ));
    assert!(app.error);
    assert!(app.status.contains("already consumed"));
    assert_eq!(app.strip().count(), 3);
    assert_eq!(app.tab.id, output_id);
    assert_eq!(app.tab.doc, output);
    assert_eq!(history(&app.tab), output_history);

    // Start a real host Save, switch again, then route its completion to its
    // origin. The Task is dropped, so this regression writes no host file.
    let _ = app.update(Message::Tabs(tabs::Action::Select(source_id)));
    assert_eq!(app.drawing_save_target(false).0, Some(path.clone()));
    let _ = app.update(Message::Save);
    assert!(app.file_io.saving);
    assert_eq!(app.file_io.saving_tab, Some(source_id));
    let _ = app.update(Message::Tabs(tabs::Action::Select(output_id)));
    let _ = app.update(Message::Tab(
        source_id,
        Box::new(Message::Saved(
            source_epoch,
            Box::new(source.clone()),
            Ok(Some(path.clone())),
        )),
    ));
    assert!(!app.file_io.saving);
    assert_eq!(app.tab.id, output_id);
    assert_eq!(app.tab.doc, output);
    assert_eq!(history(&app.tab), output_history);
    assert!(app.tab.office.is_none() && app.tab.path.is_none());
    let parked_source = app
        .tabs
        .background
        .iter()
        .find(|tab| tab.id == source_id)
        .unwrap();
    assert_eq!(parked_source.doc, source);
    assert_eq!(parked_source.saved, source);
    assert_eq!(history(parked_source), source_history);
    assert_eq!(parked_source.file_epoch, source_epoch);
    assert_eq!(parked_source.revision, source_revision);
    assert_eq!(
        (parked_source.camera.center, parked_source.camera.zoom),
        (source_camera.center, source_camera.zoom)
    );
    assert_eq!(parked_source.path, Some(path.clone()));
    let binding = parked_source.office.as_ref().unwrap();
    assert_eq!(binding.path, path);
    assert_eq!(binding.host, Host::Office);
    assert_eq!(binding.lease.as_ref().unwrap().token, lease.token);
    assert_eq!(lease.phase(), Phase::Open);
    let parked_foreground = app
        .tabs
        .background
        .iter()
        .find(|tab| tab.id == foreground)
        .unwrap();
    assert_eq!(parked_foreground.doc, foreground_doc);
    assert_eq!(history(parked_foreground), foreground_history);
    assert_eq!(
        (
            parked_foreground.camera.center,
            parked_foreground.camera.zoom
        ),
        (foreground_camera.center, foreground_camera.zoom)
    );
    assert_eq!(app.strip().count(), 3);
}
