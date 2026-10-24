use super::*;

#[cfg(windows)]
#[test]
fn dropdown_partial_redraws_preserve_unchanged_pixels() {
    use iced::advanced::{Layout, graphics::Viewport, mouse};
    use iced::overlay::menu;
    use resvg::tiny_skia::{Mask, Pixmap};

    let size = iced::Size::new(640., 480.);
    let bounds = iced::Rectangle::with_size(size);
    for theme in [Theme::Light, Theme::Dark] {
        for options in [
            vec![
                "JACS / ACS",
                "Nature",
                "RSC",
                "Angewandte",
                "SYNLETT / SYNTHESIS",
                "Manage styles…",
            ],
            vec![
                "Publication",
                "Presentation",
                "Pastel",
                "Jmol",
                "VESTA",
                "Manage themes…",
            ],
        ] {
            let mut renderer = iced::Renderer::Secondary(iced_tiny_skia::Renderer::new(
                iced::Font::default(),
                iced::Pixels(16.),
            ));
            let mut state = menu::State::new();
            let mut hovered = Some(3);
            let class: menu::StyleFn<'_, Theme> = Box::new(dropdown_menu);
            let mut overlay =
                menu::Menu::new(&mut state, &options, &mut hovered, |_| (), None, &class)
                    .width(240.)
                    .padding([5, 8])
                    .overlay(
                        iced::Point::new(320., 80.),
                        bounds,
                        30.,
                        iced::Length::Shrink,
                    );
            let node = overlay.as_overlay_mut().layout(&renderer, size);
            overlay.as_overlay().draw(
                &mut renderer,
                &theme,
                &iced::advanced::renderer::Style::default(),
                Layout::new(&node),
                mouse::Cursor::Unavailable,
            );
            // Like a hover redraw inside the menu: pixels outside the
            // damage must retain exactly their original color, including
            // the first row and the area around the menu border.
            let damage = iced::Rectangle {
                x: 330.,
                y: 180.,
                width: 220.,
                height: 30.,
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
                    theme.palette().background,
                );
                let original = pixels.clone();
                for _ in 0..16 {
                    renderer.draw(
                        &mut pixels.as_mut(),
                        &mut mask,
                        &viewport,
                        &[damage],
                        theme.palette().background,
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
                    "{} menu, {theme:?}, scale {scale}: pixels changed outside damage",
                    options[0]
                );
            }
        }
    }
}

#[test]
fn field_and_menu_roles_pass_on_both_interface_surfaces() {
    use iced::widget::pick_list;
    use reshiki::color_contrast::{OUTLINE_TARGET, TEXT_TARGET, contrast};
    let (mut app, _) = crate::app::App::new();
    for mode in Mode::ALL {
        for canvas in reshiki::canvas_theme::CanvasTheme::ALL {
            let _ = app.update(crate::app::Message::CanvasTheme(canvas));
            let _ = app.update(crate::app::Message::Appearance(mode));
            let theme = app.theme();
            for background in [theme.palette().background, surface(&theme, Color::WHITE)] {
                assert!(contrast(rgb(muted(&theme)), rgb(background)) >= TEXT_TARGET);
            }
            for status in [pick_list::Status::Active, pick_list::Status::Hovered] {
                let style = dropdown(&theme, status);
                let iced::Background::Color(bg) = style.background else {
                    panic!("solid field");
                };
                for ink in [style.text_color, style.placeholder_color] {
                    assert!(contrast(rgb(ink), rgb(bg)) >= TEXT_TARGET);
                }
                assert!(contrast(rgb(style.handle_color), rgb(bg)) >= 3.);
                if matches!(status, pick_list::Status::Hovered) {
                    for background in [bg, theme.palette().background] {
                        assert!(
                            contrast(rgb(style.border.color), rgb(background)) >= OUTLINE_TARGET
                        );
                    }
                }
            }
            let menu = dropdown_menu(&theme);
            let iced::Background::Color(bg) = menu.background else {
                panic!("solid menu");
            };
            let iced::Background::Color(selected) = menu.selected_background else {
                panic!("solid selection");
            };
            assert!(contrast(rgb(menu.text_color), rgb(bg)) >= TEXT_TARGET);
            assert!(contrast(rgb(menu.selected_text_color), rgb(selected)) >= TEXT_TARGET);
        }
    }
}

#[test]
fn display_colors_preserve_hue_and_alpha() {
    assert_eq!(color(true, Color::BLACK), Color::WHITE);
    assert_eq!(color(true, Color::WHITE), Color::BLACK);
    let red = Color::from_rgba8(180, 50, 55, 0.5);
    let shown = color(true, red);
    assert!(shown.r > shown.b && shown.b > shown.g);
    assert_eq!(shown.a, red.a);
    assert_eq!(color(false, red), red);
}

/// Escape closes an open pick list, as it does the app's own popovers, and is
/// left alone while the list is closed (#336).
#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn escape_closes_an_open_pick_list() {
    use iced::advanced::renderer::Headless;
    use iced::keyboard::{self, Key, Modifiers, key};
    use iced::{Event, Size, mouse};
    use iced_runtime::{UserInterface, user_interface::Cache};

    #[derive(Debug, Clone, PartialEq)]
    enum Msg {
        Pick(&'static str),
        Open,
        Close,
    }
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .expect("headless renderer");
    let size = Size::new(400., 300.);
    let mut cache = Cache::new();
    let mut send = |events: &[Event], cursor: mouse::Cursor| {
        // The status bar shows a hover hint with each list.
        let view = iced::widget::tooltip(
            close_on_escape(
                pick_list(["Publication", "Pastel"], Some("Publication"), Msg::Pick)
                    .on_open(Msg::Open)
                    .on_close(Msg::Close),
            ),
            "Color theme",
            iced::widget::tooltip::Position::Bottom,
        );
        let mut ui = UserInterface::build(view, size, std::mem::take(&mut cache), &mut renderer);
        let mut messages = Vec::new();
        let (_, statuses) = ui.update(
            events,
            cursor,
            &mut renderer,
            &mut iced::advanced::clipboard::Null,
            &mut messages,
        );
        // Remaining overlays are laid out again once the menu has gone.
        ui.draw(&mut renderer, &Theme::Light, &Default::default(), cursor);
        cache = ui.into_cache();
        (statuses[0], messages)
    };
    let escape = || {
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: Key::Named(key::Named::Escape),
            modified_key: Key::Named(key::Named::Escape),
            physical_key: key::Physical::Code(key::Code::Escape),
            location: keyboard::Location::Standard,
            modifiers: Modifiers::empty(),
            text: None,
            repeat: false,
        })
    };
    // Hovered throughout, so the hint shows beside the open menu.
    let at = iced::Point::new(10., 10.);
    let pointer = mouse::Cursor::Available(at);
    let moved = |position| Event::Mouse(mouse::Event::CursorMoved { position });
    let press = || Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
    let _ = send(&[moved(at)], pointer);
    let (_, opened) = send(&[press()], pointer);
    assert_eq!(opened, [Msg::Open]);
    // The open menu takes Escape, so nothing behind it acts on the key, and
    // the list closes on the redraw that follows.
    let (status, messages) = send(&[escape()], pointer);
    assert!(messages.is_empty());
    assert_eq!(status, iced::event::Status::Captured);
    let redraw = || {
        Event::Window(iced::window::Event::RedrawRequested(
            std::time::Instant::now(),
        ))
    };
    let (_, closed) = send(&[redraw()], pointer);
    assert_eq!(closed, [Msg::Close]);
    // Closed: Escape reaches the app's own shortcut.
    let (status, ignored) = send(&[escape()], pointer);
    assert!(ignored.is_empty());
    assert_eq!(status, iced::event::Status::Ignored);

    // A click on the second item picks it, as a control for the batch below.
    let pastel = iced::Point::new(10., 77.);
    let on_pastel = mouse::Cursor::Available(pastel);
    let (_, opened) = send(&[press()], pointer);
    assert_eq!(opened, [Msg::Open]);
    let (_, picked) = send(&[moved(pastel), press()], on_pastel);
    assert!(picked.contains(&Msg::Pick("Pastel")), "{picked:?}");
    // A click queued after Escape in the same batch picks nothing: the list
    // closes once, and the current choice stays.
    let _ = send(&[moved(at)], pointer);
    let (_, opened) = send(&[press()], pointer);
    assert_eq!(opened, [Msg::Open]);
    let (_, batch) = send(&[escape(), moved(pastel), press()], on_pastel);
    let (_, closed) = send(&[redraw()], pointer);
    assert_eq!([batch, closed].concat(), [Msg::Close]);
}
