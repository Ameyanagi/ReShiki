//! The PDF task must not be pollable until its own acknowledgment redraw.
use super::*;
use iced::futures::{StreamExt, poll};
use std::task::Poll;

fn ready() -> App {
    let (mut app, _) = App::new();
    app.recovered.clear();
    app.tab.busy = false;
    app.tab.labels_dirty = false;
    let atom = app.tab.doc.add_atom("O", Point::default());
    app.tab.selected = vec![atom];
    app.tab.saved = app.tab.doc.clone();
    app
}

fn request(app: &mut App) -> Task<Message> {
    if CopyFormat::Pdf.available() {
        app.copy_as(CopyFormat::Pdf)
    } else {
        // Headless Linux may have no native transport. Exercise the same
        // post-preflight state machine without weakening the public refusal.
        let key = app.start_copy_as(CopyFormat::Pdf);
        let snapshot = reshiki::clipboard::selection_or_drawing(&app.tab.doc, &app.tab.selected);
        app.queue_copy_as(key, snapshot, false)
    }
}

fn arm(app: &mut App) -> (u64, window::Id, Instant) {
    let task = request(app);
    assert!(task.units() > 0); // A window query, never PDF preparation.
    let serial = app.pdf_copy.pending.as_ref().unwrap().serial;
    let window = window::Id::unique();
    assert!(
        app.copy_frame(FrameAction::Bound(serial, Some(window)))
            .units()
            > 0
    );
    (
        serial,
        window,
        app.pdf_copy.pending.as_ref().unwrap().armed_at,
    )
}

fn next_frame(app: &App) -> FrameAction {
    let pending = app.pdf_copy.pending.as_ref().unwrap();
    FrameAction::Redrawn(
        pending.serial,
        pending.window.unwrap(),
        pending.armed_at + std::time::Duration::from_millis(1),
    )
}

#[tokio::test]
async fn routed_pdf_task_cannot_prepare_while_other_batched_tasks_complete() {
    let mut app = ready();
    if !CopyFormat::Pdf.available() {
        let before = app.tab.doc.clone();
        assert_eq!(app.copy_as(CopyFormat::Pdf).units(), 0);
        assert!(app.error && app.status.contains("use Export"));
        assert_eq!(app.tab.doc, before);
        assert!(app.pdf_copy.pending.is_none());
        assert!(!app.clipboard_working());
        return;
    }
    let owner = window::Id::unique();
    let _ = app.accessibility_action(super::super::accessibility::Action::Window(
        owner,
        window::Event::Opened {
            position: None,
            size: iced::Size::new(1280., 820.),
        },
    ));
    // App::update includes idle autosave's empty SelectAll and accessibility
    // operations. A one-shot yield in the PDF future is not a frame barrier.
    let task = app.update(Message::CopyAs(CopyFormat::Pdf));
    let mut stream = iced_runtime::task::into_stream(task).unwrap();
    let mut redraws = 0;
    for _ in 0..32 {
        match poll!(stream.next()) {
            Poll::Ready(Some(iced_runtime::Action::Window(
                iced_runtime::window::Action::RedrawAll,
            ))) => redraws += 1,
            Poll::Ready(Some(iced_runtime::Action::Output(message))) => {
                assert!(!matches!(message, Message::CopyAsPrepared(..)));
                if let Message::Tab(_, message) = message {
                    assert!(!matches!(*message, Message::CopyAsPrepared(..)));
                }
            }
            Poll::Ready(Some(_)) | Poll::Pending => tokio::task::yield_now().await,
            Poll::Ready(None) => break,
        }
    }
    assert_eq!(redraws, 1);
    assert!(app.pdf_copy.pending.is_some());
    assert!(app.clipboard_working());
    assert_eq!(app.status, "Preparing PDF for the clipboard…");
}

#[test]
fn foreign_stale_and_pre_request_frames_leave_the_pending_pdf_intact() {
    let mut app = ready();
    let (serial, owner, at) = arm(&mut app);
    for action in [
        FrameAction::Redrawn(
            serial,
            window::Id::unique(),
            at + std::time::Duration::from_secs(1),
        ),
        FrameAction::Redrawn(
            serial.wrapping_sub(1),
            owner,
            at + std::time::Duration::from_secs(1),
        ),
        FrameAction::Redrawn(serial, owner, at),
        FrameAction::Redrawn(serial, owner, at - std::time::Duration::from_millis(1)),
        FrameAction::Closed(serial, window::Id::unique()),
        FrameAction::Closed(serial.wrapping_sub(1), owner),
        FrameAction::Bound(serial, None),
    ] {
        assert_eq!(app.copy_frame(action).units(), 0);
        let pending = app.pdf_copy.pending.as_ref().unwrap();
        assert_eq!(
            (pending.serial, pending.window, pending.armed_at),
            (serial, Some(owner), at)
        );
        assert!(app.clipboard_working());
    }
    let action = next_frame(&app);
    assert!(app.copy_frame(action.clone()).units() > 0);
    assert!(app.pdf_copy.pending.is_none());
    assert_eq!(app.copy_frame(action).units(), 0);
    assert!(app.clipboard_working()); // Preparation/publication still own it.
}

#[tokio::test]
async fn raw_subscription_preserves_foreign_and_owner_window_identities() {
    use iced_runtime::futures::subscription::{self, Event};
    let mut app = ready();
    let (serial, owner, at) = arm(&mut app);
    let foreign = window::Id::unique();
    let at = at + std::time::Duration::from_millis(1);
    let mut recipes = subscription::into_recipes(app.pdf_copy_subscription()).into_iter();
    let recipe = recipes.next().unwrap();
    assert!(recipes.next().is_none());
    let events = [foreign, owner].map(|window| Event::Interaction {
        window,
        event: iced::Event::Window(window::Event::RedrawRequested(at)),
        status: iced::event::Status::Ignored,
    });
    let mut stream = recipe.stream(Box::pin(iced::futures::stream::iter(events)));
    for actual in [foreign, owner] {
        let Some(Message::CopyFrame(FrameAction::Redrawn(found_serial, found_window, found_at))) =
            stream.next().await
        else {
            panic!("Expected a real redraw subscription message");
        };
        assert_eq!((found_serial, found_window, found_at), (serial, actual, at));
        let task = app.update(Message::CopyFrame(FrameAction::Redrawn(
            found_serial,
            found_window,
            found_at,
        )));
        if actual == foreign {
            assert_eq!(task.units(), 0);
            assert!(app.pdf_copy.pending.is_some());
        } else {
            assert!(task.units() > 0);
            assert!(app.pdf_copy.pending.is_none());
        }
    }
    assert!(stream.next().await.is_none());
    assert!(subscription::into_recipes(app.pdf_copy_subscription()).is_empty());
}

#[test]
fn missing_or_closed_window_cancels_only_its_request() {
    for closed in [false, true] {
        let mut app = ready();
        let _ = request(&mut app);
        let serial = app.pdf_copy.pending.as_ref().unwrap().serial;
        assert_eq!(
            app.copy_frame(FrameAction::Bound(serial.wrapping_sub(1), None))
                .units(),
            0
        );
        assert!(app.pdf_copy.pending.is_some());
        let action = if closed {
            let owner = window::Id::unique();
            let _ = app.copy_frame(FrameAction::Bound(serial, Some(owner)));
            FrameAction::Closed(serial, owner)
        } else {
            FrameAction::Bound(serial, None)
        };
        assert_eq!(app.copy_frame(action).units(), 0);
        assert!(app.pdf_copy.pending.is_none());
        assert!(!app.clipboard_working());
        assert!(app.error && app.status.contains("Clipboard unchanged"));
    }
}

#[test]
fn stale_or_closed_origin_drops_the_snapshot_before_preparation() {
    for stale in ["selection", "revision", "replace", "close"] {
        let mut app = ready();
        arm(&mut app);
        let action = next_frame(&app);
        match stale {
            "selection" => app.tab.selected.clear(),
            "revision" => app.tab.revision += 1,
            "replace" => app.reset_tab(),
            _ => {
                let _ = app.close_active_tab();
            }
        }
        let document = app.tab.doc.clone();
        app.status = "Current drawing".into();
        assert_eq!(app.update(Message::CopyFrame(action)).units(), 0);
        assert!(app.pdf_copy.pending.is_none());
        assert!(!app.clipboard_working());
        assert_eq!(app.tab.doc, document);
        if matches!(stale, "replace" | "close") {
            assert_eq!(app.status, "Current drawing");
        } else {
            assert!(app.status.contains("Clipboard unchanged"));
        }
    }
}

#[tokio::test]
async fn delayed_window_binding_remains_untagged_after_its_origin_closes() {
    let mut app = ready();
    let origin = app.tab.id;
    let task = super::super::tagged(request(&mut app), origin);
    let mut stream = iced_runtime::task::into_stream(task).unwrap();
    let Some(iced_runtime::Action::Window(iced_runtime::window::Action::GetLatest(sender))) =
        stream.next().await
    else {
        panic!("Expected window lookup before preparing PDF");
    };
    let _ = app.close_active_tab();
    let owner = window::Id::unique();
    sender.send(Some(owner)).unwrap();
    let Some(iced_runtime::Action::Output(message @ Message::CopyFrame(_))) = stream.next().await
    else {
        panic!("Window binding must not acquire the closed document's tag");
    };
    assert!(app.update(message).units() > 0); // Explicit redraw after binding.
    let action = next_frame(&app);
    assert_eq!(app.update(Message::CopyFrame(action)).units(), 0);
    assert!(app.pdf_copy.pending.is_none());
    assert!(!app.clipboard_working());
}

#[tokio::test]
async fn acknowledged_pdf_prepares_once_and_keeps_its_origin_across_tab_switch() {
    use crate::app::tabs::tests::Front;
    let mut app = ready();
    arm(&mut app);
    let origin = app.tab.id;
    let action = next_frame(&app);
    let front = Front::new(&mut app);
    let task = app.update(Message::CopyFrame(action));
    front.assert_unchanged(&app);
    assert!(app.pdf_copy.pending.is_none());
    let mut stream = iced_runtime::task::into_stream(task).unwrap();
    let Some(iced_runtime::Action::Output(Message::Tab(id, message))) = stream.next().await else {
        panic!("Expected the original tab's prepared PDF");
    };
    assert_eq!(id, origin);
    let Message::CopyAsPrepared(key, result) = *message else {
        panic!("Expected PDF preparation");
    };
    assert_eq!(result.unwrap().format, CopyFormat::Pdf);
    assert!(stream.next().await.is_none());
    // Stop before touching the clipboard, and exercise the normal origin result route.
    let _ = app.update(Message::Tab(
        origin,
        Box::new(Message::CopyAsPrepared(
            key,
            Box::new(Err("QA stop before publication".into())),
        )),
    ));
    front.assert_unchanged(&app);
    assert!(!app.clipboard_working());
}

#[test]
fn duplicate_copy_keeps_the_original_pending_snapshot_and_serial() {
    let mut app = ready();
    let (serial, owner, at) = arm(&mut app);
    let snapshot = app.pdf_copy.pending.as_ref().unwrap().snapshot.clone();
    for format in [CopyFormat::Pdf, CopyFormat::Svg, CopyFormat::Smiles] {
        assert_eq!(app.copy_as(format).units(), 0);
        let pending = app.pdf_copy.pending.as_ref().unwrap();
        assert_eq!(
            (pending.serial, pending.window, pending.armed_at),
            (serial, Some(owner), at)
        );
        assert_eq!(pending.snapshot, snapshot);
        assert!(app.clipboard_working());
    }
}
