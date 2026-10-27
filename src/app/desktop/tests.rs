use super::*;
use crate::{
    app::{Job, naming, tabs},
    desktop::Admission,
    session::{Host, Lease},
};
use reshiki::document::{Document, Point};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

fn request(app: &mut App, paths: Vec<std::path::PathBuf>) -> Arc<Admission> {
    let reply = Admission::new();
    let _ = app.update(Message::Desktop(Action::Request(Event::Open(
        paths,
        reply.clone(),
    ))));
    reply
}

#[test]
fn admission_waits_for_ready_and_blocks_exit_until_the_owned_read_finishes() {
    let (mut app, _) = App::new();
    assert_eq!(request(&mut app, vec![]).get(Duration::ZERO), Status::Busy);
    let _ = app.update(Message::Desktop(Action::Ready));
    assert_eq!(
        request(&mut app, vec!["pending.rsk".into()]).get(Duration::ZERO),
        Status::Accepted
    );
    assert!(app.desktop.opening);
    assert_eq!(request(&mut app, vec![]).get(Duration::ZERO), Status::Busy);
    let _ = app.update(Message::Close(iced::window::Id::unique()));
    assert!(!app.exit.frozen());
    assert!(app.pending.is_none());
    let id = app.tab.id;
    let _ = app.update(Message::Tabs(tabs::Action::Close(Some(id))));
    assert_eq!(app.tab.id, id);
    let _ = app.update(Message::Desktop(Action::Finished));
    assert!(!app.desktop.opening);
    app.pending = Some(super::super::Pending::CloseTab(id));
    assert_eq!(request(&mut app, vec![]).get(Duration::ZERO), Status::Busy);
}

#[test]
fn admitted_foreign_import_holds_the_close_barrier_until_engine_result() {
    for fail in [false, true] {
        let (mut app, _) = App::new();
        let _ = app.update(Message::Desktop(Action::Ready));
        assert_eq!(
            request(&mut app, vec!["delayed.mol".into()]).get(Duration::ZERO),
            Status::Accepted
        );
        let _ = app.update(Message::FilePrepared(Some((
            "delayed.mol".into(),
            Ok(files::Prepared::Import {
                format: "mol",
                contents: "delayed test payload".into(),
            }),
        ))));
        let id = app.tab.id;
        let revision = app.tab.revision;
        assert!(app.tab.busy);
        let _ = app.update(Message::Desktop(Action::Finished));
        assert!(
            app.desktop.opening,
            "read completion is not import completion"
        );
        let _ = app.update(Message::Close(iced::window::Id::unique()));
        assert!(!app.exit.frozen());
        let _ = app.update(Message::Tabs(tabs::Action::Close(Some(id))));
        assert_eq!(app.tab.id, id);
        let result = if fail {
            Err("controlled import failure".into())
        } else {
            let mut doc = Document::default();
            doc.add_atom("N", Point::default());
            Ok(reshiki::engine::Response {
                document: Some(doc),
                analysis: None,
                output: None,
                engine_version: "controlled-delay-test".into(),
                warnings: vec![],
            })
        };
        let _ = app.update(Message::EngineDone {
            revision,
            kind: Job::ImportFile("delayed.mol".into()),
            result: Box::new(result),
        });
        assert!(!app.desktop.opening);
        assert_eq!(app.tab.doc.atoms.len(), usize::from(!fail));
        if fail {
            assert!(app.error);
        }
    }
}

#[test]
fn lost_proxy_revokes_host_target_but_keeps_the_drawing_and_styles() {
    let (mut app, _) = App::new();
    let alive = Arc::new(AtomicBool::new(true));
    let check = alive.clone();
    let lease = Lease::new([3; 16], move || check.load(Ordering::Acquire));
    let binding = Binding {
        path: "office.rsk".into(),
        host: Host::Microsoft365,
        lease: Some(lease.clone()),
    };
    let mut doc = Document::default();
    doc.add_atom("O", Point::default());
    let _ = app.update(Message::OfficePrepared(
        binding,
        Some((
            "office.rsk".into(),
            Ok(files::Prepared::Native(Box::new(doc))),
        )),
    ));
    let before = app.tab.doc.clone();
    let epoch = app.tab.file_epoch;
    let _ = app.naming_action(naming::Action::Name("ethanol".into()));
    assert!(app.naming_action(naming::Action::Resolve).units() > 0);
    assert!(app.tab.naming.is_pending());
    alive.store(false, Ordering::Release);
    let _ = app.update(Message::Desktop(Action::Poll));
    assert_eq!(app.tab.doc, before);
    assert!(app.tab.office.is_none());
    assert!(app.tab.path.is_none());
    assert_ne!(app.tab.file_epoch, epoch);
    assert!(app.dirty());
    assert!(!lease.writable());
    assert!(app.error);
    assert!(!app.tab.naming.is_pending());
    assert!(app.naming_action(naming::Action::Generate).units() > 0);
    assert!(app.tab.naming.is_pending());
}

#[test]
fn background_proxy_loss_clears_naming_without_changing_the_front_tab() {
    let (mut app, _) = App::new();
    let alive = Arc::new(AtomicBool::new(true));
    let check = alive.clone();
    let lease = Lease::new([4; 16], move || check.load(Ordering::Acquire));
    let binding = Binding {
        path: "background-office.rsk".into(),
        host: Host::Microsoft365,
        lease: Some(lease.clone()),
    };
    let mut doc = Document::default();
    doc.add_atom("O", Point::default());
    let _ = app.update(Message::OfficePrepared(
        binding,
        Some((
            "background-office.rsk".into(),
            Ok(files::Prepared::Native(Box::new(doc))),
        )),
    ));
    let (id, epoch, before) = (app.tab.id, app.tab.file_epoch, app.tab.doc.clone());
    let _ = app.naming_action(naming::Action::Name("ethanol".into()));
    assert!(app.naming_action(naming::Action::Resolve).units() > 0);
    assert!(app.tab.naming.is_pending());
    let front = tabs::tests::Front::new(&mut app);

    alive.store(false, Ordering::Release);
    // Isolate background lease handling from the update wrapper's front label scheduling.
    let _ = app.desktop_action(Action::Poll);
    front.assert_unchanged(&app);
    let source = app.strip().find(|tab| tab.id == id).unwrap();
    assert_eq!(source.doc, before);
    assert!(source.office.is_none());
    assert!(source.path.is_none());
    assert_ne!(source.file_epoch, epoch);
    assert!(source.dirty());
    assert!(source.error);
    assert_eq!(source.status, LOST_PROXY_STATUS);
    assert!(!source.naming.is_pending());
    assert!(!lease.writable());
    let units = app
        .in_tab(id, |app| {
            app.naming_action(naming::Action::Generate).units()
        })
        .unwrap();
    assert!(units > 0);
    assert!(
        app.strip()
            .find(|tab| tab.id == id)
            .unwrap()
            .naming
            .is_pending()
    );
    front.assert_unchanged(&app);
}

#[test]
fn lease_liveness_polling_exists_only_while_an_office_tab_owns_a_proxy() {
    let (mut app, _) = App::new();
    let count = |app: &App| iced::advanced::subscription::into_recipes(subscription(app)).len();
    assert_eq!(count(&app), 1);
    let lease = Lease::new([6; 16], || true);
    lease.transition(Phase::Prepared, Phase::Open);
    app.tab.office = Some(Binding {
        path: "host.rsk".into(),
        host: Host::Office,
        lease: Some(lease.clone()),
    });
    assert_eq!(count(&app), 2);
    app.detach_office();
    assert_eq!(lease.phase(), Phase::Closed);
    assert_eq!(count(&app), 1);
}

#[tokio::test]
async fn desktop_open_task_emits_untagged_read_and_finished_after_the_front_tab_changes() {
    use iced::futures::StreamExt;

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("missing.rsk");
    let (mut app, _) = App::new();
    let _ = app.update(Message::Desktop(Action::Ready));
    let original = app.tab.id;
    let reply = Admission::new();
    let task = app.update(Message::Desktop(Action::Request(Event::Open(
        vec![path.clone()],
        reply.clone(),
    ))));
    assert_eq!(reply.get(Duration::ZERO), Status::Accepted);
    app.add_tab();
    let front = app.tab.id;
    assert_ne!(front, original);

    let mut stream = iced_runtime::task::into_stream(task).expect("Desktop starts a read task");
    let mut opened = false;
    loop {
        let action = stream
            .next()
            .await
            .expect("The read returns FilePrepared followed by Finished");
        // Dropping window actions does not execute the activation request.
        let iced_runtime::Action::Output(message) = action else {
            continue;
        };
        match message {
            Message::FilePrepared(Some((read, result))) => {
                assert!(!opened);
                assert_eq!(read, path);
                assert!(result.is_err());
                opened = true;
            }
            Message::Desktop(Action::Finished) => {
                assert!(opened, "Finished follows the owned read completion");
                break;
            }
            Message::Tab(..) => panic!("Desktop tasks must remain untagged"),
            _ => panic!("The open task returns only FilePrepared and Finished"),
        }
    }
    assert_eq!(app.tab.id, front);
}

#[test]
fn desktop_ready_leaves_the_queued_office_close_for_a_routed_completion() {
    let (mut app, _) = App::new();
    let id = app.tab.id;
    let lease = Lease::new([7; 16], || true);
    assert!(lease.transition(Phase::Prepared, Phase::Open));
    app.tab.office = Some(Binding {
        path: "closing.rsk".into(),
        host: Host::Office,
        lease: Some(lease.clone()),
    });
    app.office.closing.push(id);

    let _ = app.update(Message::Desktop(Action::Ready));
    assert!(app.desktop.ready);
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
fn desktop_prepare_rejects_missing_or_invalid_leases_before_not_ready_busy() {
    let (mut app, _) = App::new();
    assert!(!app.desktop.ready);
    let prepare = |app: &mut App, binding: Binding| {
        let reply = Admission::new();
        let _ = app.update(Message::Desktop(Action::Request(Event::Prepare(
            binding,
            reply.clone(),
        ))));
        reply.get(Duration::ZERO)
    };
    let watched = |lease| Binding {
        path: "watched.rsk".into(),
        host: Host::Office,
        lease: Some(lease),
    };
    let standalone = Binding::standalone("standalone.rsk".into(), Host::Office);
    assert!(standalone.preparable());
    assert_eq!(prepare(&mut app, standalone), Status::Rejected);
    assert_eq!(
        prepare(&mut app, watched(Lease::new([8; 16], || true))),
        Status::Busy
    );
    assert_eq!(
        prepare(&mut app, watched(Lease::new([9; 16], || false))),
        Status::Rejected
    );
    let rejected = Lease::new([10; 16], || true);
    assert!(rejected.transition(Phase::Prepared, Phase::Rejected));
    assert_eq!(prepare(&mut app, watched(rejected)), Status::Rejected);
    assert!(!app.desktop.ready);
}
