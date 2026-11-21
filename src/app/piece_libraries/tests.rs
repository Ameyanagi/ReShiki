use super::*;

fn piece(id: &str) -> Arc<Definition> {
    let mut definition = (*builtin::catalogue()[0]).clone();
    definition.id = id.into();
    Arc::new(definition)
}

fn folder(piece: Arc<Definition>) -> Folder {
    Folder {
        pieces: vec![piece],
        problems: vec![],
    }
}

/// Tasks are deliberately not polled: tests deliver owned worker receipts in
/// a controlled order, without reading user preferences or any shared folder.
fn request(app: &mut App, roots: &[&str], notice: Option<NoticeKind>) -> u64 {
    let _ = app.reload_libraries(roots.iter().map(PathBuf::from).collect(), notice);
    app.equipment.libraries.loading.serial()
}

fn lab(serial: u64, root: &str) -> Job {
    Job {
        serial,
        target: Target::Lab(root.into()),
    }
}

#[test]
fn builtins_are_indexed_without_waiting_for_folder_scans() {
    let libraries = Libraries::load();
    assert_eq!(libraries.entries.len(), builtin::catalogue().len());
    assert!(
        libraries
            .entries
            .iter()
            .all(|e| e.source == Source::BuiltIn)
    );
    assert!(!libraries.loading.pending());
}

#[test]
fn local_results_publish_without_waiting_for_a_linked_share() {
    let (mut app, _) = App::new();
    let serial = request(&mut app, &["/slow/share"], None);
    let mine = piece("mine/flask");
    let _ = app.libraries_loaded(
        Job {
            serial,
            target: Target::Mine,
        },
        Ok(Some(folder(mine.clone()))),
    );
    assert!(
        app.equipment
            .libraries
            .entries
            .iter()
            .any(|e| e.key == key(&mine))
    );
    assert!(!app.equipment.libraries.loading.loading(&Target::Mine));
    assert!(
        app.equipment
            .libraries
            .loading
            .loading(&Target::Lab("/slow/share".into()))
    );
}

#[test]
fn failed_installer_handoff_leaves_linked_folders_able_to_reload() {
    let (mut app, _) = App::new();
    let root = "/lab";
    let target = Target::Lab(root.into());
    let serial = request(&mut app, &[root], Some(NoticeKind::Linked(root.into())));
    app.updates.restarting = true;
    let _ = app.restart_after_recovery();
    assert!(app.exit.frozen() && !app.exit.committed());
    let exit_status = app.status.clone();
    let _ = app.update(Message::Equipment(Action::LibrariesLoaded(
        lab(serial, root),
        Ok(Some(folder(piece("lab/first")))),
    )));
    assert!(!app.equipment.libraries.loading.loading(&target));
    assert_eq!(app.status, exit_status);

    let _ = app.update(Message::Updates(super::super::updates::Action::Restarted(
        Err("Installer failed".into()),
    )));
    assert!(!app.exit.frozen());
    let serial = request(&mut app, &[root], None);
    let updated = piece("lab/updated");
    let _ = app.update(Message::Equipment(Action::LibrariesLoaded(
        lab(serial, root),
        Ok(Some(folder(updated.clone()))),
    )));
    assert!(!app.equipment.libraries.loading.loading(&target));
    assert!(
        app.equipment
            .libraries
            .entries
            .iter()
            .any(|entry| entry.key == key(&updated))
    );
}

#[test]
fn detached_folder_entries_and_notices_disappear_before_its_old_scan_finishes() {
    let (mut app, _) = App::new();
    let old = request(&mut app, &["/old/lab"], None);
    let definition = piece("old/flask");
    app.equipment.libraries.labs[0].1 = folder(definition.clone());
    app.equipment.libraries.labs[0]
        .1
        .problems
        .push(library::Problem {
            path: "/old/lab/missing.reshiki-piece".into(),
            error: "Old missing-file notice".into(),
        });
    app.equipment.libraries.index();
    app.equipment.armed = Some(definition.clone());
    app.equipment.shelf = Some(Source::Lab(0));

    request(&mut app, &["/new/lab"], None);
    assert_eq!(app.lab_roots(), [PathBuf::from("/new/lab")]);
    assert!(app.equipment.shelf.is_none());
    assert!(app.equipment.armed.is_none());
    assert!(
        !app.equipment
            .libraries
            .entries
            .iter()
            .any(|e| e.key == key(&definition))
    );
    app.status = "A newer action".into();
    let _ = app.libraries_loaded(lab(old, "/old/lab"), Err("Old scan failed".into()));
    assert_eq!(app.status, "A newer action");
    assert!(!app.error);
    assert_eq!(app.lab_roots(), [PathBuf::from("/new/lab")]);
    assert!(app.equipment.libraries.labs[0].1.problems.is_empty());
}

#[test]
fn stale_error_cannot_finish_a_relinked_folders_new_request() {
    let (mut app, _) = App::new();
    let old = request(&mut app, &["/lab"], Some(NoticeKind::Linked("/lab".into())));
    request(&mut app, &[], None);
    let current = request(&mut app, &["/lab"], Some(NoticeKind::Linked("/lab".into())));
    let progress = app.status.clone();
    let next = app.libraries_loaded(lab(old, "/lab"), Err("Old share is offline".into()));
    assert!(
        iced_runtime::task::into_stream(next).is_some(),
        "the latest scan starts next"
    );
    let _ = app.libraries_loaded(lab(old, "/lab"), Err("Duplicate old error".into()));
    assert_eq!(app.status, progress);
    assert!(
        app.equipment
            .libraries
            .loading
            .loading(&Target::Lab("/lab".into()))
    );
    assert!(app.equipment.libraries.labs[0].1.problems.is_empty());

    let _ = app.libraries_loaded(lab(current, "/lab"), Ok(Some(folder(piece("lab/new")))));
    assert_eq!(app.status, "Linked lab · 1 piece");
    assert!(!app.error);
    assert!(
        !app.equipment
            .libraries
            .loading
            .loading(&Target::Lab("/lab".into()))
    );
}

#[test]
fn completions_reconcile_the_current_shelf_and_armed_piece() {
    let (mut app, _) = App::new();
    let serial = request(&mut app, &["/first", "/second"], None);
    app.equipment.shelf = Some(Source::Lab(1));
    let armed = builtin::catalogue()[1].clone();
    app.equipment.armed = Some(armed.clone());
    let _ = app.libraries_loaded(lab(serial, "/first"), Ok(Some(folder(piece("lab/first")))));
    assert_eq!(app.equipment.shelf, Some(Source::Lab(1)));
    assert_eq!(app.equipment.armed.as_deref(), Some(&*armed));

    request(&mut app, &["/second", "/first"], None);
    assert_eq!(app.equipment.shelf, Some(Source::Lab(0)));
    // A later user choice must not be replaced with the shelf shown at dispatch.
    app.equipment.shelf = Some(Source::BuiltIn);
    let _ = app.libraries_loaded(
        lab(serial, "/second"),
        Ok(Some(folder(piece("old/second")))),
    );
    assert_eq!(app.equipment.shelf, Some(Source::BuiltIn));
    assert_eq!(app.equipment.armed.as_deref(), Some(&*armed));
}

#[test]
fn completion_never_overwrites_an_intervening_status_or_error() {
    for error in [false, true] {
        let (mut app, _) = App::new();
        let serial = request(&mut app, &["/lab"], Some(NoticeKind::Linked("/lab".into())));
        app.status = "A later result".into();
        app.error = error;
        let _ = app.libraries_loaded(lab(serial, "/lab"), Ok(Some(Folder::default())));
        assert_eq!(app.status, "A later result");
        assert_eq!(app.error, error);
        assert!(app.equipment.libraries.notice.is_none());
    }
}

#[test]
fn current_missing_folder_problem_is_replaced_by_the_next_successful_scan() {
    let (mut app, _) = App::new();
    let serial = request(&mut app, &["/lab"], None);
    let _ = app.libraries_loaded(lab(serial, "/lab"), Err("Controlled worker failure".into()));
    assert_eq!(
        app.equipment.libraries.labs[0].1.problems,
        [library::Problem {
            path: "/lab".into(),
            error: "Controlled worker failure".into(),
        }]
    );
    let serial = request(&mut app, &["/lab"], None);
    let _ = app.libraries_loaded(lab(serial, "/lab"), Ok(Some(Folder::default())));
    assert!(app.equipment.libraries.labs[0].1.problems.is_empty());
}

#[test]
fn library_receipts_are_app_owned_and_admitted_without_committing_text_drafts() {
    let message = Message::Equipment(Action::LibrariesLoaded(lab(1, "/lab"), Ok(None)));
    assert!(matches!(
        super::super::results::async_result(&message),
        Some(super::super::results::Owner::App)
    ));
    for modal in [
        super::super::results::Modal::AtomText,
        super::super::results::Modal::Preview,
        super::super::results::Modal::Optimization,
        super::super::results::Modal::UpdatesDialog,
    ] {
        assert!(super::super::results::admitted(modal, &message));
    }
    assert!(!super::super::inline_text::commits_draft(&message));
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn native_scan_runs_on_a_detached_worker_and_reports_panics() {
    use iced::futures::StreamExt;
    let caller = std::thread::current().id();
    let task = scan_with(lab(1, "/unused"), move |_| {
        assert_ne!(std::thread::current().id(), caller);
        Some(Folder::default())
    });
    let mut stream = iced_runtime::task::into_stream(task).unwrap();
    assert!(matches!(
        stream.next().await,
        Some(iced_runtime::Action::Output(Message::Equipment(
            Action::LibrariesLoaded(_, Ok(Some(_)))
        )))
    ));

    let task = scan_with(lab(2, "/unused"), |_| panic!("Controlled scan panic"));
    let mut stream = iced_runtime::task::into_stream(task).unwrap();
    let Some(iced_runtime::Action::Output(Message::Equipment(Action::LibrariesLoaded(
        _,
        Err(error),
    )))) = stream.next().await
    else {
        panic!("the worker failure is returned")
    };
    assert!(error.contains("Controlled scan panic"), "{error}");
}
