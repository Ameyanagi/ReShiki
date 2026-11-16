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
