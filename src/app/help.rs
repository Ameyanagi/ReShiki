use super::{
    App, Message, platform_shortcut,
    workspace::{control, muted},
};
use iced::widget::{
    Space, button, column, container, mouse_area, opaque, row, scrollable, stack, text,
};
use iced::{Alignment, Border, Color, Element, Length};

pub(super) fn is_shortcut(key: &iced::keyboard::Key, modifiers: iced::keyboard::Modifiers) -> bool {
    modifiers.is_empty()
        && matches!(
            key,
            iced::keyboard::Key::Named(iced::keyboard::key::Named::F1)
        )
}

impl App {
    pub(super) fn with_help<'a>(&'a self, base: Element<'a, Message>) -> Element<'a, Message> {
        if !self.help_open {
            return base;
        }
        let drawing = group(
            "Drawing tools",
            &[
                ("Space / l", "Select / Lasso"),
                ("x / 1", "Single bond"),
                ("2 / 3 / 4", "Double / Triple / Quadruple"),
                ("Shift X", "Straight chain"),
                (
                    "r / Shift R",
                    "Ring / Toggle saturated–aromatic (same size)",
                ),
                ("e / t / Shift T", "Arrow / Text / Brackets"),
                ("j / Shift J", "Benzene / Cyclopentadiene"),
                ("F1", "Keyboard shortcuts"),
                ("Option / Alt drag", "Draw or move bonded atoms freely"),
                ("Esc", "Return to selection"),
            ],
        );
        let editing = group(
            "Selection & arrangement",
            &[
                (platform_shortcut("⌘ G", "Ctrl G"), "Group"),
                (platform_shortcut("⇧ ⌘ G", "Ctrl Shift G"), "Ungroup"),
                (
                    platform_shortcut("⇧ ⌘ A", "Ctrl Shift A"),
                    "Invert selection",
                ),
                (platform_shortcut("⇧ ⌘ D", "Ctrl Shift D"), "Duplicate"),
                (
                    platform_shortcut("⌘ Z / ⇧ ⌘ Z", "Ctrl Z / Ctrl Shift Z"),
                    "Undo / Redo",
                ),
                ("Delete", "Delete selection"),
                ("Enter", "Edit selected atom label (M, L, X, Boc…)"),
            ],
        );
        let context = column![
            text("Under the pointer (case-sensitive)").size(14),
            shortcut("1 / 2 / 3", "Bond: single / double / triple"),
            shortcut("b / w / h / y", "Bond: bold / wedge / hashed wedge / wavy"),
            shortcut("d / D / B / H", "Bond: dashed / partial double / bold double / hashed"),
            shortcut("l / c / r", "Double line: left / center / right"),
            shortcut("c n o s p f h", "Atom: C N O S P F H"),
            shortcut("b / C / B / i / L / S", "Atom: Br / Cl / B / I / Li / Si"),
            shortcut("m / e / y / P", "Atom: Me / Et / Boc / Ph"),
            shortcut("M / Z", "Atom: MgBr / N₃ (complete chemical groups)"),
            shortcut("j / J on an atom", "Cp / arene pi ligand; repeat at a metal for another ligand"),
            shortcut("A / E / F / H / N / O / Q", "Ac / CO₂Me / CF₃ / Cbz / NO₂ / OMe / Fmoc"),
            shortcut("d / + / −", "Atom: deuterium / increase / decrease charge"),
            shortcut("r / x", "Atom: variable R / X"),
            shortcut("0 / 1 / 2 / 8 / z", "Atom: branch / chain / carbonyl / =CH₂ / alkyne"),
            shortcut("3 / 6 / 7 / v / u", "Atom: phenyl / 6-ring / 5-ring / 3-ring / 4-ring"),
            shortcut("9 / K / k", "Atom: dimethyl / tert-butyl / sulfonyl"),
            shortcut("v / 4–8 / a / z / 9 / 0", "Bond: fuse rings / benzene / diene / chairs"),
            shortcut("g / ? / Enter", "Select / Properties / Edit atom label"),
            text("Uppercase means Shift-letter. Repeat 2 on a double bond to cycle its line placement.")
                .size(12).color(muted()),
        ].spacing(8);
        let files = group(
            "Files",
            &[
                (
                    platform_shortcut("⌘ N / ⌘ O", "Ctrl N / Ctrl O"),
                    "New / Open",
                ),
                (platform_shortcut("⌘ S", "Ctrl S"), "Save"),
                (
                    platform_shortcut("⌘ I / ⇧ ⌘ E", "Ctrl I / Ctrl Shift E"),
                    "Import / Export",
                ),
                (platform_shortcut("⌘ P", "Ctrl P"), "Print"),
                (
                    platform_shortcut("⌘ J", "Ctrl J"),
                    "Join selected atoms / bonds",
                ),
                (platform_shortcut("⇧ ⌘ K", "Ctrl Shift K"), "Clean up"),
                (
                    platform_shortcut("⌘ L / ⌘ E", "Ctrl L / Ctrl E"),
                    "Fixed bond length / angles",
                ),
                (
                    platform_shortcut("⌥ ⌘ K", "Alt K"),
                    "Aromatic circle / alternating bonds",
                ),
                (platform_shortcut("⌘ D", "Ctrl D"), "Copy CDXML text"),
                (
                    platform_shortcut("⌥ ⌘ C / ⌥ ⌘ O", "Ctrl Alt C / Ctrl Alt O"),
                    "Copy SMILES / MOL",
                ),
                (
                    "Alt arrows / Shift Alt arrows",
                    "Rotate / 3D tilt selection",
                ),
                ("Arrows / Shift arrows", "Nudge 1 / 10 units"),
                ("Drag side handle", "Change width or height"),
                ("Drag corner handle", "Resize proportionally"),
            ],
        );
        let body = column![drawing, context, editing, files]
            .spacing(24)
            .width(Length::Fill)
            .padding([0, 12]);
        let popup = container(
            column![
                row![
                    column![
                        text("Keyboard shortcuts").size(22),
                        text("Quick reference for drawing and editing")
                            .size(12)
                            .color(muted())
                    ]
                    .spacing(5),
                    Space::new().width(Length::Fill),
                    button(text("×").size(25).center())
                        .width(32)
                        .height(32)
                        .padding(0)
                        .on_press(Message::ToggleHelp)
                        .style(control(false)),
                ]
                .align_y(Alignment::Center)
                .spacing(12),
                scrollable(body).height(Length::Fill),
                row![
                    text("Hold a tool or click its corner for more options.")
                        .size(12)
                        .color(muted()),
                    Space::new().width(Length::Fill),
                    button(text("Done").size(13))
                        .padding([7, 18])
                        .on_press(Message::ToggleHelp)
                        .style(control(true))
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            ]
            .spacing(22),
        )
        .padding(24)
        .width(720)
        .height(650)
        .max_height(650)
        .style(|_| container::Style {
            background: Some(Color::WHITE.into()),
            border: Border {
                radius: 14.into(),
                width: 1.,
                color: Color::from_rgb8(207, 216, 216),
            },
            shadow: super::workspace::surface_shadow(iced::Shadow {
                color: Color::from_rgba8(20, 40, 35, 0.18),
                offset: iced::Vector::new(0., 8.),
                blur_radius: 30.,
            }),
            ..Default::default()
        });
        stack![
            base,
            opaque(
                mouse_area(
                    container(Space::new())
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .style(|_| container::Style {
                            background: Some(Color::from_rgba8(25, 35, 40, 0.18).into()),
                            ..Default::default()
                        })
                )
                .on_press(Message::ToggleHelp)
            ),
            container(opaque(popup))
                .padding(24)
                .center_x(Length::Fill)
                .center_y(Length::Fill),
        ]
        .into()
    }
}

fn shortcut(keys: &'static str, label: &'static str) -> Element<'static, Message> {
    row![
        text(label).size(12).width(Length::Fill),
        container(text(keys).size(11))
            .padding([3, 6])
            .style(|_| container::Style {
                background: Some(Color::from_rgb8(245, 247, 249).into()),
                border: Border {
                    radius: 4.into(),
                    width: 1.,
                    color: Color::from_rgb8(224, 228, 233)
                },
                ..Default::default()
            }),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

fn group(
    title: &'static str,
    shortcuts: &[(&'static str, &'static str)],
) -> Element<'static, Message> {
    let mut body = column![text(title).size(14)].spacing(8);
    for (keys, label) in shortcuts {
        body = body.push(shortcut(keys, label));
    }
    body.into()
}

#[cfg(test)]
mod tests {
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
        let mut renderer = iced::Renderer::new(iced::Font::default(), iced::Pixels(16.));
        let mut view = app.with_help(Space::new().width(Length::Fill).height(Length::Fill).into());
        let mut tree = Tree::new(view.as_widget());
        let node =
            view.as_widget_mut()
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
        app.import_open = true;
        app.selected = vec![app.doc.add_atom("O", reshiki::document::Point::default())];
        app.camera.zoom = 5.;
        let before = app.doc.clone();
        let selected = app.selected.clone();
        let _ = app.update(Message::ToggleHelp);
        assert!(app.help_open);
        assert!(app.import_open);
        let _ = app.update(Message::Escape);
        assert!(!app.help_open);
        assert_eq!(app.tool, crate::canvas::Tool::Ring);
        assert_eq!(app.selected, selected);
        assert_eq!(app.camera.zoom, 5.);
        assert_eq!(app.doc, before);
    }
}
