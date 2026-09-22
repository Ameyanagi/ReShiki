use super::{
    App, Message, platform_shortcut,
    workspace::{control, muted},
};
use iced::widget::{
    Space, button, column, container, mouse_area, opaque, row, scrollable, stack, text,
};
use iced::{Alignment, Border, Color, Element, Length};

pub(super) fn is_shortcut(key: &iced::keyboard::Key, modifiers: iced::keyboard::Modifiers) -> bool {
    !modifiers.command()
        && !modifiers.control()
        && !modifiers.alt()
        && matches!(key, iced::keyboard::Key::Character(c) if c == "?" || (c == "/" && modifiers.shift()))
}

impl App {
    pub(super) fn with_help<'a>(&'a self, base: Element<'a, Message>) -> Element<'a, Message> {
        if !self.help_open {
            return base;
        }
        let drawing = group(
            "Drawing tools",
            &[
                ("V / L", "Select / Lasso"),
                ("B / 1", "Single bond"),
                ("2 / 3 / 4", "Double / Triple / Quadruple"),
                ("X", "Straight chain"),
                ("Shift X", "Snaking chain"),
                ("R / Shift R", "Ring / Aromatic ring"),
                ("A / T / E", "Arrow / Text / Eraser"),
                ("Alt drag", "Draw bonds freely"),
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
                (platform_shortcut("⌘ D", "Ctrl D"), "Duplicate"),
                (
                    platform_shortcut("⌘ Z / ⇧ ⌘ Z", "Ctrl Z / Ctrl Shift Z"),
                    "Undo / Redo",
                ),
                ("Delete", "Delete selection"),
            ],
        );
        let context = column![
            text("Edit under the pointer").size(14),
            text("Hover an atom to change its element:")
                .size(12)
                .color(muted()),
            shortcut("C N O S P F H", "Element symbol"),
            shortcut("S / D / T", "Single / Double / Triple bond"),
            text("Hover or select a bond. Repeat D to shift the double bond lines.")
                .size(12)
                .color(muted()),
            shortcut("A", "Aromatic circle / Alternating bonds"),
            text("Select an aromatic ring first.")
                .size(12)
                .color(muted()),
        ]
        .spacing(8);
        let files = group(
            "Files",
            &[
                (
                    platform_shortcut("⌘ N / ⌘ O", "Ctrl N / Ctrl O"),
                    "New / Open",
                ),
                (platform_shortcut("⌘ S", "Ctrl S"), "Save"),
                (
                    platform_shortcut("⌘ I / ⌘ E", "Ctrl I / Ctrl E"),
                    "Import / Export",
                ),
                (platform_shortcut("⌘ P", "Ctrl P"), "Print"),
            ],
        );
        let body = row![
            column![drawing, context].spacing(22).width(Length::Fill),
            column![editing, files].spacing(22).width(Length::Fill),
        ]
        .spacing(30);
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
                scrollable(body).height(Length::Shrink),
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
        .max_height(650)
        .style(|_| container::Style {
            background: Some(Color::WHITE.into()),
            border: Border {
                radius: 14.into(),
                width: 1.,
                color: Color::from_rgb8(207, 216, 216),
            },
            shadow: iced::Shadow {
                color: Color::from_rgba8(20, 40, 35, 0.18),
                offset: iced::Vector::new(0., 8.),
                blur_radius: 30.,
            },
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
    #[test]
    fn question_mark_accepts_the_native_unmodified_slash_key() {
        use iced::keyboard::{Key, Modifiers};
        let slash = Key::Character("/".into());
        assert!(is_shortcut(&slash, Modifiers::SHIFT));
        assert!(is_shortcut(&Key::Character("?".into()), Modifiers::empty()));
        assert!(!is_shortcut(&slash, Modifiers::empty()));
        assert!(!is_shortcut(&slash, Modifiers::SHIFT | Modifiers::ALT));
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
