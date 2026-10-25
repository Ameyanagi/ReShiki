//! Cancellation uses the real abortable task stream; no timing hooks or sleeps.
use super::*;
use iced::futures::{StreamExt, future, poll};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

fn ready() -> App {
    let (mut app, _) = App::new();
    app.cancel_engine_job();
    app.tab.busy = false;
    app.tab.doc = Document::default();
    app.tab.history = Default::default();
    app.tab.selected.clear();
    app.tab.labels_dirty = false;
    app
}

fn pending(app: &mut App) -> (Key, Task<Message>) {
    let task = app.start_engine_job(Job::Insert, future::pending());
    (app.tab.engine_job.as_ref().unwrap().key, task)
}

fn imported() -> Result<Response, String> {
    let mut document = Document::default();
    document.add_atom("O", Point::default());
    Ok(Response {
        document: Some(document),
        analysis: None,
        output: None,
        engine_version: "test".into(),
        warnings: vec![],
    })
}

fn finished(key: Key, result: Result<Response, String>) -> Message {
    Message::EngineFinished {
        key,
        result: Box::new(result),
    }
}

#[tokio::test]
async fn cancel_drops_the_active_future_and_preserves_the_drawing() {
    struct Dropped(Arc<AtomicBool>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    let mut app = ready();
    let before = app.tab.doc.clone();
    let atom = app.tab.doc.add_atom("C", Point::new(20., 30.));
    app.changed(before);
    app.tab.selected = vec![atom];
    app.tab.camera.center = Point::new(125., 75.);
    app.tab.camera.zoom = 1.75;
    let drawing = app.tab.doc.clone();
    let selection = app.tab.selected.clone();
    let camera = app.tab.camera;
    let frames = app.tab.history.frames();
    let revision = app.tab.revision;
    let dropped = Arc::new(AtomicBool::new(false));
    let guard = Dropped(dropped.clone());
    let task = app.start_engine_job(Job::Import, async move {
        let _guard = guard;
        future::pending().await
    });
    let mut stream = iced_runtime::task::into_stream(task).unwrap();
    assert!(poll!(stream.next()).is_pending());
    assert!(!dropped.load(Ordering::SeqCst));
    let _ = app.update(Message::CancelEngine);
    assert!(
        stream.next().await.is_none(),
        "Aborted tasks cannot publish a completion"
    );
    // Like the iced executor, retire the stream once it reports completion.
    drop(stream);
    assert!(
        dropped.load(Ordering::SeqCst),
        "Cancel must drop the active future"
    );
    assert_eq!(app.tab.doc, drawing);
    assert_eq!(app.tab.selected, selection);
    assert_eq!(app.tab.camera.center, camera.center);
    assert_eq!(app.tab.camera.zoom, camera.zoom);
    assert_eq!(app.tab.history.frames(), frames);
    assert_eq!(app.tab.revision, revision);
    assert!(!app.tab.busy);
    assert!(app.tab.engine_job.is_none());
    assert_eq!(app.status, "Operation cancelled");
}

#[test]
fn cancelled_receipts_cannot_apply_or_clear_a_newer_job() {
    let mut app = ready();
    let (old, _old_task) = pending(&mut app);
    app.cancel_engine_job();
    let (new, _new_task) = pending(&mut app);
    assert_ne!(old, new);
    for result in [imported(), Err("Late failure".into())] {
        let _ = app.update(finished(old, result));
        assert!(app.tab.doc.atoms.is_empty());
        assert!(app.tab.busy);
        assert_eq!(app.tab.engine_job.as_ref().unwrap().key, new);
        assert_eq!(app.status, "Working…");
        assert!(!app.error);
    }
    let _ = app.update(finished(new, imported()));
    assert_eq!(app.tab.doc.atoms.len(), 1);
    assert!(!app.tab.busy);
    assert!(app.tab.engine_job.is_none());
    // A duplicated receipt must also leave the next request alone.
    let (next, _task) = pending(&mut app);
    let _ = app.update(finished(new, Err("Duplicate failure".into())));
    assert_eq!(app.tab.engine_job.as_ref().unwrap().key, next);
    assert!(app.tab.busy);
}

#[test]
fn errors_and_success_both_release_controls_for_retry() {
    let mut app = ready();
    let (first, _task) = pending(&mut app);
    let _ = app.update(finished(
        first,
        Err("Could not parse this structure".into()),
    ));
    assert!(app.error);
    assert!(!app.tab.busy);
    assert!(app.tab.engine_job.is_none());
    let (second, _task) = pending(&mut app);
    assert!(!app.error);
    let _ = app.update(finished(second, imported()));
    assert!(!app.error);
    assert!(!app.tab.busy);
    let (_, task) = pending(&mut app);
    assert!(task.units() > 0);
}

#[test]
fn progress_appears_after_one_second_and_escape_respects_modal_priority() {
    let mut app = ready();
    let _ = pending(&mut app);
    assert!(app.engine_progress().is_none());
    app.tab.engine_job.as_mut().unwrap().started -= PROGRESS_DELAY;
    assert_eq!(app.engine_progress().as_deref(), Some("Working · 1 s"));
    app.help_open = true;
    let _ = app.update(Message::Escape);
    assert!(!app.help_open);
    assert!(app.tab.engine_job.is_some(), "Help owns the first Escape");
    app.tool = Tool::Atom;
    let _ = app.update(Message::Escape);
    assert!(app.tab.engine_job.is_none());
    assert_eq!(
        app.tool,
        Tool::Atom,
        "Cancel precedes the ordinary tool reset"
    );
    let _ = app.update(Message::Escape);
    assert_eq!(app.tool, Tool::Select);
    assert!(app.engine_progress().is_none());
}

#[test]
fn cancel_remains_available_while_rerunning_a_cleanup_preview() {
    use super::super::cleanup;
    let mut app = ready();
    let atom = app.tab.doc.add_atom("C", Point::default());
    app.tab.selected = vec![atom];
    let drawing = app.tab.doc.clone();
    let _ = app.cleanup_action(cleanup::Action::Begin);
    let key = app.tab.engine_job.as_ref().unwrap().key;
    let _ = app.update(finished(
        key,
        Ok(Response {
            document: Some(drawing.clone()),
            analysis: None,
            output: None,
            engine_version: "test".into(),
            warnings: vec![],
        }),
    ));
    assert!(app.tab.cleanup.is_some());
    let _ = app.update(Message::Cleanup(cleanup::Action::Orientation(false)));
    let rerun = app.tab.engine_job.as_ref().unwrap().key;
    assert!(app.tab.busy);
    let _ = app.update(Message::CancelEngine);
    assert!(app.tab.engine_job.is_none());
    assert!(!app.tab.busy);
    assert!(
        app.tab.cleanup.is_some(),
        "The prior preview remains available"
    );
    let _ = app.update(finished(rerun, imported()));
    assert_eq!(app.tab.doc, drawing);
    assert_eq!(app.tab.cleanup.as_ref().unwrap().document, drawing);
    assert!(!app.tab.busy);
}

#[test]
fn completion_is_classified_and_keeps_its_background_tab() {
    use super::super::{results, tabs};
    let mut app = ready();
    let owner = app.tab.id;
    let (key, _task) = pending(&mut app);
    let message = finished(key, imported());
    assert_eq!(
        results::async_result(&message),
        Some(results::Owner::Document)
    );
    for modal in [
        results::Modal::AtomText,
        results::Modal::Preview,
        results::Modal::Cleanup,
        results::Modal::Optimization,
        results::Modal::UpdatesDialog,
    ] {
        assert!(results::admitted(modal, &message));
    }
    app.add_tab();
    let (front, _task) = pending(&mut app);
    let _ = app.update(Message::Tab(owner, Box::new(message)));
    assert!(app.tab.doc.atoms.is_empty());
    assert!(app.tab.busy);
    assert_eq!(app.tab.engine_job.as_ref().unwrap().key, front);
    let original = app
        .tabs
        .background
        .iter()
        .find(|tab| tab.id == owner)
        .unwrap();
    assert_eq!(original.doc.atoms.len(), 1);
    assert!(!original.busy);
    assert!(original.engine_job.is_none());
    let _ = app.update(Message::Tabs(tabs::Action::Select(owner)));
    assert_eq!(app.tab.id, owner);
    assert!(!app.tab.busy);
}

#[test]
fn reset_file_epoch_and_closed_tabs_discard_old_receipts() {
    use super::super::tabs;
    for reset in [false, true] {
        let mut app = ready();
        let (key, _task) = pending(&mut app);
        if reset {
            app.reset_tab();
        } else {
            app.begin_file_epoch();
        }
        assert!(!app.tab.busy);
        assert!(app.tab.engine_job.is_none());
        let (new, _task) = pending(&mut app);
        let _ = app.update(finished(key, imported()));
        assert!(app.tab.doc.atoms.is_empty());
        assert_eq!(app.tab.engine_job.as_ref().unwrap().key, new);
        assert!(app.tab.busy);
    }
    let mut app = ready();
    let old_tab = app.tab.id;
    let (key, _task) = pending(&mut app);
    app.add_tab();
    let _ = app.update(Message::Tabs(tabs::Action::Close(Some(old_tab))));
    assert!(app.tab_index(old_tab).is_none());
    let (new, _task) = pending(&mut app);
    let _ = app.update(Message::Tab(old_tab, Box::new(finished(key, imported()))));
    assert!(app.tab.doc.atoms.is_empty());
    assert_eq!(app.tab.engine_job.as_ref().unwrap().key, new);
    assert!(app.tab.busy);
}

#[tokio::test]
async fn real_native_import_aborts_without_publishing_or_changing_the_drawing() {
    let mut app = ready();
    let atom = app.tab.doc.add_atom("N", Point::new(20., 30.));
    app.tab.selected = vec![atom];
    let drawing = app.tab.doc.clone();
    let selected = app.tab.selected.clone();
    let task = app.run(
        Request::import_smiles(&format!("{}O", "N[C@@H](C)C(=O)".repeat(200))),
        Job::Insert,
    );
    let mut stream = iced_runtime::task::into_stream(task).unwrap();
    // Native parsing and layout yield to their worker, so the application can cancel.
    assert!(poll!(stream.next()).is_pending());
    let key = app.tab.engine_job.as_ref().unwrap().key;
    let _ = app.update(Message::CancelEngine);
    assert!(stream.next().await.is_none());
    drop(stream);
    let _ = app.update(finished(key, imported()));
    assert_eq!(app.tab.doc, drawing);
    assert_eq!(app.tab.selected, selected);
    assert_eq!(app.tab.history.frames(), 0);
    assert!(!app.tab.busy);
    assert!(!app.error);
}

#[tokio::test]
#[ignore = "Opt-in real renderer verifies the delayed native Cancel control"]
async fn delayed_cancel_control_is_accessible_and_activates_cancellation() -> Result<(), String> {
    use iced::advanced::{
        renderer::Headless,
        widget::{Operation, operation},
    };
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .ok_or("No renderer")?;
    let mut app = ready();
    let _ = pending(&mut app);
    let size = iced::Size::new(1040., 680.);
    for shown in [false, true] {
        if shown {
            app.tab.engine_job.as_mut().unwrap().started -= PROGRESS_DELAY;
        }
        let mut ui = iced_runtime::UserInterface::build(
            app.view(),
            size,
            iced_runtime::user_interface::Cache::new(),
            &mut renderer,
        );
        let mut collect = reshiki::accessibility::Collect::new(iced::Rectangle::with_size(size));
        ui.operate(&renderer, &mut operation::black_box(&mut collect));
        let button = collect
            .snapshot()
            .nodes
            .iter()
            .find(|node| node.id == "engine.cancel");
        assert_eq!(button.is_some(), shown);
        if let Some(button) = button {
            assert!(button.enabled);
            assert_eq!(button.name, "Cancel chemistry operation");
            assert!(button.visible_bounds.is_some());
            let mut activate = reshiki::accessibility::Activate::<Message>::new("engine.cancel");
            ui.operate(&renderer, &mut operation::black_box(&mut activate));
            match activate.finish() {
                operation::Outcome::Some(message @ Message::CancelEngine) => {
                    drop(ui);
                    let _ = app.update(message);
                    assert!(!app.tab.busy);
                    assert!(app.tab.engine_job.is_none());
                }
                _ => return Err("Cancel control did not activate cancellation".into()),
            }
        }
    }
    Ok(())
}
