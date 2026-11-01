use super::*;
use iced::advanced::{
    Layout, Shell, clipboard, layout, mouse,
    renderer::Headless,
    widget::{Tree, operation},
};
use iced::{Event, Rectangle, Size};
use reshiki::cleanup::{Options, Scope};

fn preview(scope: Scope, busy: bool) -> App {
    let (mut app, _) = App::new();
    app.recovered.clear();
    app.inspector_open = true;
    app.tab.doc = reshiki::rings::Preset::Benzene.document(42., false);
    app.tab.saved = app.tab.doc.clone();
    app.tab.selected = app.tab.doc.all_ids();
    app.tab.busy = busy;
    let mut document = app.tab.doc.clone();
    document.atoms[0].position.x += 10.;
    app.tab.cleanup = Some(super::super::CleanupPreview {
        job: super::super::cleanup::CleanupJob {
            options: Options {
                scope,
                ..Options::default()
            },
            selection: app.tab.selected.clone(),
            serial: app.tab.cleanup_serial,
            epoch: app.tab.file_epoch,
        },
        warnings: vec![],
        document,
        analysis: None,
        revision: app.tab.revision,
        epoch: app.tab.file_epoch,
        original: false,
    });
    app
}

#[tokio::test]
#[ignore = "Opt-in renderer layout and pointer checks"]
async fn cleanup_preview_actions_fit_and_click_in_narrow_and_default_workspaces() {
    let renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .unwrap();
    for (size, whole_window) in [
        (Size::new(450., 200.), false),
        (Size::new(1040., 680.), true),
        (Size::new(1280., 820.), true),
    ] {
        for scope in [Scope::SelectedAtoms, Scope::SelectedMolecules] {
            for busy in [false, true] {
                for id in ["cleanup.cancel", "cleanup.apply"] {
                    let mut app = preview(scope, busy);
                    let original = app.tab.doc.clone();
                    let cleaned = app.tab.cleanup.as_ref().unwrap().document.clone();
                    let messages = {
                        let mut view = if whole_window {
                            app.view()
                        } else {
                            app.cleanup_bar(app.tab.cleanup.as_ref().unwrap())
                        };
                        let mut tree = Tree::new(view.as_widget());
                        let node = view.as_widget_mut().layout(
                            &mut tree,
                            &renderer,
                            &layout::Limits::new(Size::ZERO, size),
                        );
                        let mut collect =
                            reshiki::accessibility::Collect::new(Rectangle::with_size(size));
                        view.as_widget_mut().operate(
                            &mut tree,
                            Layout::new(&node),
                            &renderer,
                            &mut operation::black_box(&mut collect),
                        );
                        let controls = &collect.snapshot().nodes;
                        for control_id in ["cleanup.cancel", "cleanup.apply"] {
                            let control = controls.iter().find(|n| n.id == control_id).unwrap();
                            let visible = control.visible_bounds.expect("visible cleanup action");
                            assert!(
                                (visible.width - control.bounds.width).abs() < 0.1
                                    && (visible.height - control.bounds.height).abs() < 0.1,
                                "Clipped {control_id} at {size:?}: {control:?}"
                            );
                            let (left, right) = if whole_window {
                                (PALETTE_WIDTH, size.width - app.inspector_width())
                            } else {
                                (0., size.width)
                            };
                            assert!(visible.x >= left && visible.x + visible.width <= right);
                        }
                        let control = controls.iter().find(|n| n.id == id).unwrap();
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
                        messages
                    };
                    let actions: Vec<_> = messages
                        .into_iter()
                        .filter(|message| matches!(message, Message::Cleanup(_)))
                        .collect();
                    if busy && id == "cleanup.apply" {
                        assert!(
                            actions.is_empty(),
                            "Apply stays disabled while cleanup runs"
                        );
                        continue;
                    }
                    assert!(
                        matches!(
                            actions.as_slice(),
                            [Message::Cleanup(super::super::cleanup::Action::Cancel)]
                                if id == "cleanup.cancel"
                        ) || matches!(
                            actions.as_slice(),
                            [Message::Cleanup(super::super::cleanup::Action::Apply)]
                                if id == "cleanup.apply"
                        )
                    );
                    for message in actions {
                        let _ = app.update(message);
                    }
                    assert!(app.tab.cleanup.is_none());
                    assert_eq!(
                        app.tab.doc,
                        if id == "cleanup.apply" {
                            cleaned
                        } else {
                            original
                        }
                    );
                }
            }
        }
    }
}
