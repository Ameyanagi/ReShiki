use super::*;

#[cfg(windows)]
#[test]
fn shortcuts_dialog_partial_redraws_preserve_unchanged_pixels() {
    use iced::advanced::{graphics::Viewport, layout, mouse, widget::Tree};
    use resvg::tiny_skia::{Mask, Pixmap};

    let (mut app, _) = App::new();
    app.help_open = true;
    let size = iced::Size::new(1040., 680.);
    let bounds = iced::Rectangle::with_size(size);
    let mut renderer = iced::Renderer::Secondary(iced_tiny_skia::Renderer::new(
        iced::Font::default(),
        iced::Pixels(16.),
    ));
    let mut view = app.with_help(Space::new().width(Length::Fill).height(Length::Fill).into());
    let mut tree = Tree::new(view.as_widget());
    let node = view
        .as_widget_mut()
        .layout(&mut tree, &renderer, &layout::Limits::new(size, size));
    view.as_widget_mut().update(
        &mut tree,
        &iced::Event::Window(iced::window::Event::RedrawRequested(
            std::time::Instant::now(),
        )),
        iced::advanced::Layout::new(&node),
        mouse::Cursor::Unavailable,
        &renderer,
        &mut iced::advanced::clipboard::Null,
        &mut iced::advanced::Shell::new(&mut Vec::new()),
        &bounds,
    );
    view.as_widget().draw(
        &tree,
        &mut renderer,
        &app.theme(),
        &iced::advanced::renderer::Style::default(),
        iced::advanced::Layout::new(&node),
        mouse::Cursor::Unavailable,
        &bounds,
    );

    // Simulate small hover/caret redraws inside the dialog. The compositor
    // clears only the damaged region, so drawing outside it corrupts the
    // retained pixels even though the dialog's contents have not changed.
    let damage = iced::Rectangle {
        x: 500.,
        y: 300.,
        width: 40.,
        height: 40.,
    };
    let iced::Renderer::Secondary(mut renderer) = renderer else {
        panic!("partial-redraw regression requires the software renderer");
    };
    for scale in [1., 1.25, 2.] {
        let width = (size.width * scale) as u32;
        let height = (size.height * scale) as u32;
        let viewport = Viewport::with_physical_size(iced::Size::new(width, height), scale);
        let mut pixels = Pixmap::new(width, height).unwrap();
        let mut mask = Mask::new(width, height).unwrap();
        renderer.draw(
            &mut pixels.as_mut(),
            &mut mask,
            &viewport,
            &[bounds],
            Color::WHITE,
        );
        let original = pixels.clone();
        for _ in 0..16 {
            renderer.draw(
                &mut pixels.as_mut(),
                &mut mask,
                &viewport,
                &[damage],
                Color::WHITE,
            );
        }
        let physical_damage = damage * scale;
        let changed = pixels
            .pixels()
            .iter()
            .zip(original.pixels())
            .enumerate()
            .filter(|(index, (actual, expected))| {
                let point = iced::Point::new(
                    (*index % width as usize) as f32,
                    (*index / width as usize) as f32,
                );
                !physical_damage.contains(point) && actual != expected
            })
            .count();
        assert_eq!(
            changed, 0,
            "partial redraws changed pixels outside damage at scale {scale}"
        );
    }
}

#[test]
fn help_uses_f1_and_leaves_question_mark_for_properties() {
    use iced::keyboard::{Key, Modifiers, key::Named};
    assert!(is_shortcut(&Key::Named(Named::F1), Modifiers::empty()));
    assert!(!is_shortcut(
        &Key::Character("?".into()),
        Modifiers::empty()
    ));
    assert!(!is_shortcut(&Key::Character("/".into()), Modifiers::SHIFT));
    assert!(!is_shortcut(&Key::Named(Named::F1), Modifiers::ALT));
}

#[test]
fn dismissing_shortcuts_preserves_the_drawing_tool_and_view() {
    let (mut app, _) = App::new();
    app.tool = crate::canvas::Tool::Ring;
    app.inspector_tab = crate::app::InspectorTab::Import;
    app.tab.selected = vec![
        app.tab
            .doc
            .add_atom("O", reshiki::document::Point::default()),
    ];
    app.tab.camera.zoom = 5.;
    let before = app.tab.doc.clone();
    let selected = app.tab.selected.clone();
    let _ = app.update(Message::ToggleHelp);
    assert!(app.help_open);
    assert_eq!(app.inspector_tab, crate::app::InspectorTab::Import);
    let _ = app.update(Message::Escape);
    assert!(!app.help_open);
    assert_eq!(app.tool, crate::canvas::Tool::Ring);
    assert_eq!(app.tab.selected, selected);
    assert_eq!(app.tab.camera.zoom, 5.);
    assert_eq!(app.tab.doc, before);
}
