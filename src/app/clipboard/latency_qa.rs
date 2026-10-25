//! Opt-in phase measurements; native recordings establish presented-frame timing.
use super::*;
use iced::advanced::{Layout, Shell, layout, mouse, renderer, renderer::Headless, widget::Tree};
use iced::futures::{StreamExt, poll};
use iced::{Rectangle, Size};
use std::time::Instant;

fn elapsed(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.
}

fn settle(app: &mut App, renderer: &mut iced::Renderer, tree: &mut Tree, size: Size) {
    let theme = app.theme();
    let viewport = Rectangle::with_size(size);
    let mut messages = vec![];
    let mut view = app.view();
    tree.diff(view.as_widget());
    let node = view
        .as_widget_mut()
        .layout(tree, renderer, &layout::Limits::new(size, size));
    view.as_widget_mut().update(
        tree,
        &iced::Event::Window(iced::window::Event::RedrawRequested(Instant::now())),
        Layout::new(&node),
        mouse::Cursor::Unavailable,
        renderer,
        &mut iced::advanced::clipboard::Null,
        &mut Shell::new(&mut messages),
        &viewport,
    );
    iced::advanced::Renderer::reset(renderer, viewport);
    view.as_widget().draw(
        tree,
        renderer,
        &theme,
        &renderer::Style {
            text_color: theme.palette().text,
        },
        Layout::new(&node),
        mouse::Cursor::Unavailable,
        &viewport,
    );
    drop(view);
    for message in messages {
        let _ = app.update(message);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "Release-mode exact-gallery copy preparation and acknowledgment phase profile"]
async fn gallery_pdf_copy_phase_profile() {
    let path = std::env::var_os("RESHIKI_COPY_FEEDBACK_QA_FIXTURE").expect("Fixture required");
    let source = Document::from_json(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(source.atoms.len(), 7728);
    assert_eq!(source.bonds.len(), 6864);
    let selected = source.all_ids();
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .unwrap();
    let size = Size::new(1600., 968.);
    let viewport = Rectangle::with_size(size);
    let mut tree = Tree::empty();
    for trial in 0..3 {
        let start = Instant::now();
        let cloned = source.clone();
        let clone_ms = elapsed(start);
        std::hint::black_box(cloned);
        let start = Instant::now();
        let snapshot = reshiki::clipboard::selection_or_drawing(&source, &selected);
        let selection_ms = elapsed(start);
        let start = Instant::now();
        assert!(CopyFormat::Pdf.unavailable_reason(&snapshot).is_none());
        let availability_ms = elapsed(start);
        let start = Instant::now();
        snapshot.validate().unwrap();
        let validation_ms = elapsed(start);
        drop(snapshot);

        let (mut app, _) = App::new();
        app.recovered.clear();
        app.tab.busy = false;
        app.tab.labels_dirty = false;
        app.tab.doc = source.clone();
        app.tab.saved = source.clone();
        app.tab.camera.zoom = 1.;
        app.tab.camera.center = source.atoms[0].position;
        app.tab.fit_to_view = false;
        app.viewport = size;
        app.inspector_open = true;
        app.inspector_tab = super::super::InspectorTab::Properties;
        app.appearance.mode = crate::appearance::Mode::Light;
        app.status = "Document opened".into();
        // Real users have already selected the drawing and opened its menu.
        app.set_selection(selected.clone());
        app.sync_keyboard_drawing();
        app.sync_numeric_transforms();
        for _ in 0..2 {
            settle(&mut app, &mut renderer, &mut tree, size);
        }

        // Give the real routed task a window, then drain its immediately
        // available effects just as iced_winit does. No PDF future exists yet.
        let window = iced::window::Id::unique();
        let _ = app.accessibility_action(super::super::accessibility::Action::Window(
            window,
            iced::window::Event::Opened {
                position: None,
                size,
            },
        ));
        let start = Instant::now();
        let routed = app.update(Message::CopyAs(CopyFormat::Pdf));
        let routed_update_ms = elapsed(start);
        assert!(app.copy_as_busy && app.tab.clipboard_busy);
        assert_eq!(app.status, "Preparing PDF for the clipboard…");
        let mut stream = iced_runtime::task::into_stream(routed).unwrap();
        let start = Instant::now();
        for _ in 0..32 {
            match poll!(stream.next()) {
                std::task::Poll::Ready(Some(iced_runtime::Action::Output(message))) => {
                    assert!(!matches!(message, Message::CopyAsPrepared(..)));
                    if let Message::Tab(_, message) = message {
                        assert!(!matches!(*message, Message::CopyAsPrepared(..)));
                    }
                }
                std::task::Poll::Ready(Some(_)) | std::task::Poll::Pending => {
                    tokio::task::yield_now().await;
                }
                std::task::Poll::Ready(None) => break,
            }
        }
        let pre_frame_task_poll_ms = elapsed(start);
        drop(stream);
        assert!(app.pdf_copy.pending.is_some());
        let start = Instant::now();
        std::hint::black_box(app.selected_label_alignment());
        let label_alignment_ms = elapsed(start);
        let start = Instant::now();
        std::hint::black_box(app.toolbar_alignment());
        let toolbar_alignment_ms = elapsed(start);

        // Time each widget phase independently. This does not claim that a
        // compositor or display has presented the resulting frame.
        let theme = app.theme();
        let start = Instant::now();
        let mut view = app.view();
        let view_ms = elapsed(start);
        let start = Instant::now();
        tree.diff(view.as_widget());
        let tree_ms = elapsed(start);
        let start = Instant::now();
        let node =
            view.as_widget_mut()
                .layout(&mut tree, &renderer, &layout::Limits::new(size, size));
        let layout_ms = elapsed(start);
        let start = Instant::now();
        iced::advanced::Renderer::reset(&mut renderer, viewport);
        view.as_widget().draw(
            &tree,
            &mut renderer,
            &theme,
            &renderer::Style {
                text_color: theme.palette().text,
            },
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            &viewport,
        );
        let draw_ms = elapsed(start);
        drop(view);
        let pending = app.pdf_copy.pending.as_ref().unwrap();
        let task = app.update(Message::CopyFrame(FrameAction::Redrawn(
            pending.serial,
            window,
            iced::time::Instant::now(),
        )));
        let mut stream = iced_runtime::task::into_stream(task).unwrap();
        let Some(iced_runtime::Action::Output(Message::Tab(_, message))) = stream.next().await
        else {
            panic!("Expected the owning tab's preparation receipt");
        };
        let Message::CopyAsPrepared(_, result) = *message else {
            panic!("Expected preparation receipt, without clipboard publication");
        };
        let prepared = result.unwrap();
        assert_eq!(prepared.format, CopyFormat::Pdf);
        println!(
            "{}",
            serde_json::json!({
                "trial": trial, "atoms": source.atoms.len(), "selected_ids": selected.len(),
                "clone_ms": clone_ms, "selection_ms": selection_ms,
                "availability_ms": availability_ms,
            "validation_ms": validation_ms,
                "routed_update_ms": routed_update_ms,
                "pre_frame_task_poll_ms": pre_frame_task_poll_ms, "view_ms": view_ms,
            "label_alignment_ms": label_alignment_ms,
            "toolbar_alignment_ms": toolbar_alignment_ms,
                "tree_ms": tree_ms, "layout_ms": layout_ms, "draw_ms": draw_ms,
                "renderer": renderer.name(),
            })
        );
    }
}
