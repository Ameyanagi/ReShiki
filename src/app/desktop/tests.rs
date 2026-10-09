use super::*;
use crate::{
    app::{
        Job,
        office::{Host, Lease},
        tabs,
    },
    desktop::Reply,
};
use reshiki::document::{Document, Point};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

fn request(app: &mut App, paths: Vec<std::path::PathBuf>) -> Arc<Reply> {
    let reply = Reply::new();
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
            kind: Job::ImportFile,
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
    alive.store(false, Ordering::Release);
    let _ = app.update(Message::Desktop(Action::Poll));
    assert_eq!(app.tab.doc, before);
    assert!(app.tab.office.is_none());
    assert!(app.tab.path.is_none());
    assert_ne!(app.tab.file_epoch, epoch);
    assert!(app.dirty());
    assert!(!lease.writable());
    assert!(app.error);
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
