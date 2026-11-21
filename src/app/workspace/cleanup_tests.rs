use super::*;
use iced::advanced::{
    Layout, Shell, clipboard, layout, mouse,
    renderer::Headless,
    widget::{Tree, operation},
};
use iced::{Event, Rectangle, Size};

fn app(busy: bool) -> App {
    let (mut app, _) = App::new();
    app.recovered.clear();
    app.inspector_open = true;
    app.tab.doc = reshiki::rings::Preset::Benzene.document(42., false);
    app.tab.saved = app.tab.doc.clone();
    app.tab.busy = busy;
    app
}

#[tokio::test]
#[ignore = "Opt-in renderer layout and pointer checks"]
async fn cleanup_split_button_fits_and_clicks_in_default_workspaces() {
    use super::super::cleanup::Action;
    let renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .unwrap();
    for size in [Size::new(1040., 680.), Size::new(1280., 820.)] {
        for busy in [false, true] {
            for id in ["header-cleanup", "header-cleanup-options"] {
                let mut app = app(busy);
                let mut view = app.view();
                let mut tree = Tree::new(view.as_widget());
                let node = view.as_widget_mut().layout(
                    &mut tree,
                    &renderer,
                    &layout::Limits::new(Size::ZERO, size),
                );
                let mut collect = reshiki::accessibility::Collect::new(Rectangle::with_size(size));
                view.as_widget_mut().operate(
                    &mut tree,
                    Layout::new(&node),
                    &renderer,
                    &mut operation::black_box(&mut collect),
                );
                let controls = &collect.snapshot().nodes;
                let control = controls.iter().find(|n| n.id == id).unwrap();
                let visible = control.visible_bounds.expect("visible Clean up control");
                assert!(
                    (visible.width - control.bounds.width).abs() < 0.1
                        && (visible.height - control.bounds.height).abs() < 0.1,
                    "Clipped {id} at {size:?}: {control:?}"
                );
                let cursor = mouse::Cursor::Available(control.bounds.center());
                let mut messages = vec![];
                for event in [mouse::Event::ButtonPressed, mouse::Event::ButtonReleased] {
                    view.as_widget_mut().update(
                        &mut tree,
                        &Event::Mouse(event(mouse::Button::Left)),
                        Layout::new(&node),
                        cursor,
                        &renderer,
                        &mut clipboard::Null,
                        &mut Shell::new(&mut messages),
                        &Rectangle::with_size(size),
                    );
                }
                drop(view);
                let actions: Vec<_> = messages
                    .into_iter()
                    .filter(|message| matches!(message, Message::Cleanup(_)))
                    .collect();
                if id == "header-cleanup" {
                    if busy {
                        assert!(actions.is_empty(), "Clean up waits for the running job");
                    } else {
                        assert!(matches!(
                            actions.as_slice(),
                            [Message::Cleanup(Action::Begin)]
                        ));
                    }
                    continue;
                }
                // The options stay available while a cleanup runs.
                assert!(matches!(
                    actions.as_slice(),
                    [Message::Cleanup(Action::Menu(true))]
                ));
                for message in actions {
                    let _ = app.update(message);
                }
                assert!(app.cleanup_menu);
            }
        }
    }
}

#[tokio::test]
#[ignore = "Opt-in renderer cleanup menu activation and contextual enablement"]
async fn cleanup_cycle_menu_item_is_contextual_and_applies_immediately() {
    use iced_runtime::{UserInterface, user_interface::Cache};
    use reshiki::accessibility::{Activate, Collect};
    use reshiki::document::{Document, Point};

    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .unwrap();
    for size in [Size::new(1040., 680.), Size::new(1280., 820.)] {
        for context in ["none", "molecule", "empty cycle", "part", "cycle", "busy"] {
            let mut app = app(false);
            app.viewport = size;
            app.tab.doc = Document::default();
            let parts = [
                app.tab.doc.add_atom("C", Point::new(0., -200.)),
                app.tab.doc.add_atom("O", Point::new(170., 90.)),
                app.tab.doc.add_atom("N", Point::new(-230., 90.)),
            ];
            let (id, _) = reshiki::cycles::make(&mut app.tab.doc, &parts).unwrap();
            app.tab.doc.translate(&[parts[1]], -50., 20.);
            app.tab.selected = match context {
                "none" => vec![],
                "molecule" => vec![app.tab.doc.add_atom("F", Point::new(1000., 0.))],
                "empty cycle" => vec![reshiki::cycles::insert(
                    &mut app.tab.doc,
                    Point::new(1000., 0.),
                    100.,
                )],
                "part" => vec![parts[0]],
                _ => vec![id],
            };
            app.tab.busy = context == "busy";
            app.cleanup_menu = true;
            let before = app.tab.doc.clone();
            let frames = app.tab.history.undo_frames().len();
            let mut ui = UserInterface::build(app.view(), size, Cache::new(), &mut renderer);
            let mut collect = Collect::new(Rectangle::with_size(size));
            ui.operate(&renderer, &mut operation::black_box(&mut collect));
            let snapshot = collect.snapshot();
            assert!(snapshot.duplicate_ids.is_empty());
            let item = snapshot
                .nodes
                .iter()
                .find(|n| n.id == "cleanup-cycle")
                .unwrap();
            assert_eq!(item.name, "Clean up cycle");
            assert_eq!(item.enabled, context == "cycle", "{context}");
            let visible = item.visible_bounds.expect("cycle cleanup stays visible");
            assert!((visible.height - item.bounds.height).abs() < 0.1);
            let mut activate = Activate::<Message>::new("cleanup-cycle");
            ui.operate(&renderer, &mut operation::black_box(&mut activate));
            let message = activate.message().cloned();
            drop(ui);
            if context != "cycle" {
                assert!(message.is_none(), "{context}");
                continue;
            }
            let message = message.expect("selected cycle offers cleanup");
            assert!(matches!(
                message,
                Message::Cycle(super::super::cycles::Action::Tidy)
            ));
            let _ = app.update(message);
            assert!(!app.cleanup_menu);
            assert!(!app.error, "{}", app.status);
            assert_ne!(app.tab.doc, before);
            assert!(app.tab.cycle_inputs.proposal.is_none());
            assert_eq!(app.tab.history.undo_frames().len(), frames + 1);
            let _ = app.update(Message::Undo);
            assert_eq!(app.tab.doc, before);
        }
    }
}
