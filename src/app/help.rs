use super::{
    App, Message,
    shortcuts::keys,
    workspace::{control, muted_text},
};
use iced::widget::{
    Space, button, column, container, mouse_area, opaque, rich_text, row, scrollable, stack, text,
};
use iced::{Alignment, Border, Color, Element, Length, keyboard::Modifiers};

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
        let (command, shift, alt) = (Modifiers::COMMAND, Modifiers::SHIFT, Modifiers::ALT);
        let label = |message| super::shortcuts::label(&message).unwrap_or_default();
        let drawing = group(
            "Drawing tools",
            &[
                ("Space / l".into(), "Select / Lasso"),
                ("x / 1".into(), "Single bond"),
                ("2 / 3 / 4".into(), "Double / Triple / Quadruple"),
                (keys(shift, "X"), "Straight chain"),
                (
                    format!("r / {}", keys(shift, "R")),
                    "Ring / Toggle saturated–aromatic (same size)",
                ),
                (
                    format!("e / t / {}", keys(shift, "T")),
                    "Arrow / Text / Brackets",
                ),
                (
                    format!("j / {}", keys(shift, "J")),
                    "Benzene / Cyclopentadiene",
                ),
                ("F1".into(), "Keyboard shortcuts"),
                (
                    format!("{} drag", keys(alt, "")),
                    "Draw or move freely, without bond constraints or smart guides",
                ),
                (
                    format!("{}-click with a ring tool", keys(command, "")),
                    "Place the delocalized circle form",
                ),
                (
                    "a with an aromatic ring selected".into(),
                    "Toggle circle / alternating bonds",
                ),
                (
                    "Atoms: drag from an existing atom".into(),
                    "Add the chosen element with a single bond",
                ),
                ("Esc".into(), "Return to selection"),
            ],
        );
        let editing = group(
            "Selection & arrangement",
            &[
                (
                    format!(
                        "{} / {} / {}",
                        label(Message::Copy(true)),
                        label(Message::Copy(false)),
                        label(Message::Paste)
                    ),
                    "Cut / Copy / Paste",
                ),
                (label(Message::SelectAll), "Select all"),
                (label(Message::Group), "Group"),
                (label(Message::Ungroup), "Ungroup"),
                (label(Message::InvertSelection), "Invert selection"),
                (label(Message::Duplicate), "Duplicate"),
                (
                    "Drag an object".into(),
                    "Snap to other objects' edges, centers and equal gaps (smart guides)",
                ),
                (
                    format!("{} drag", keys(command, "")),
                    "Drag a copy, leaving the original in place; the copy snaps too",
                ),
                (
                    format!("{} drag", keys(command | shift, "")),
                    "Drag a copy along one axis, snapping along it",
                ),
                (
                    format!("Release {}, then Esc", keys(command, "")),
                    "Cancel a copy drag",
                ),
                (
                    format!("{} drag", keys(shift, "")),
                    "Move horizontally or vertically only, snapping along that axis",
                ),
                (
                    keys(command | shift, "Right"),
                    "Reaction arrow and molecule copy",
                ),
                (
                    "Space with Select active".into(),
                    "Select the most recently edited molecule",
                ),
                (
                    format!("{} / {}", label(Message::Undo), label(Message::Redo)),
                    "Undo / Redo",
                ),
                (label(Message::Delete), "Delete selection"),
                (
                    keys(Modifiers::empty(), "Enter"),
                    "Edit selected atom label (M, L, X, Boc…)",
                ),
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
            shortcut("j / J on an atom", "Tilted Cp / arene ligand; repeat at a metal to add another"),
            shortcut("A / E / F / H / N / O / Q", "Ac / CO₂Me / CF₃ / Cbz / NO₂ / OMe / Fmoc"),
            shortcut("d / + / −", "Atom: deuterium / increase / decrease charge"),
            shortcut("r / x", "Atom: variable R / X"),
            shortcut("0 / 1 / 2 / 8 / z", "Atom: branch / chain / carbonyl / =CH₂ / alkyne"),
            shortcut("3 / 6 / 7 / v / u", "Atom: phenyl / 6-ring / 5-ring / 3-ring / 4-ring"),
            shortcut("9 / K / k", "Atom: dimethyl / tert-butyl / sulfonyl"),
            shortcut("v / 4–8 / a / z / 9 / 0", "Bond: fuse rings / benzene / diene / chairs"),
            shortcut(&format!("g / ? / {}", keys(Modifiers::empty(), "Enter")), "Select / Properties / Edit atom label"),
            text("Uppercase means Shift-letter. Repeat 2 on a double bond to cycle its line placement.")
                .size(12).style(muted_text),
        ].spacing(8);
        // Windows also takes Alt+K alone; both zoom modifiers work on macOS.
        let aromatic = if cfg!(windows) {
            keys(alt, "K")
        } else {
            keys(command | alt, "K")
        };
        let zoom = if cfg!(target_os = "macos") {
            format!(
                "{} / {} scroll",
                keys(command, ""),
                keys(Modifiers::CTRL, "")
            )
        } else {
            format!("{} scroll", keys(command, ""))
        };
        let files = group(
            "Files",
            &[
                (
                    format!("{} / {}", keys(command, "N"), keys(command, "O")),
                    "New / Open, each in a new tab",
                ),
                (keys(command, "S"), "Save"),
                (keys(command, "W"), "Close tab"),
                (
                    format!(
                        "{} / {}",
                        keys(Modifiers::CTRL, "Tab"),
                        keys(Modifiers::CTRL | shift, "Tab")
                    ),
                    "Next / previous tab",
                ),
                (
                    format!(
                        "{}–{} / {}",
                        keys(command, "1"),
                        keys(command, "8"),
                        keys(command, "9")
                    ),
                    "Tab 1–8 / last tab",
                ),
                (
                    format!("{} / {}", keys(command, "I"), keys(command | shift, "E")),
                    "Import / Export",
                ),
                (keys(command, "P"), "Print"),
                (keys(command, "J"), "Join selected atoms / bonds"),
                (keys(command | shift, "K"), "Clean up"),
                (
                    format!("{} / {}", keys(command, "L"), keys(command, "E")),
                    "Fixed bond length / angles",
                ),
                (aromatic, "Aromatic circle / alternating bonds"),
                (keys(command, "D"), "Copy CDXML text"),
                (
                    format!(
                        "{} / {}",
                        keys(command | alt, "C"),
                        keys(command | alt, "O")
                    ),
                    "Copy SMILES / MOL",
                ),
                (
                    format!(
                        "{} arrows / {} arrows",
                        keys(alt, ""),
                        keys(alt | shift, "")
                    ),
                    "Rotate / 3D tilt selection",
                ),
                (
                    format!("Arrows / {} arrows", keys(shift, "")),
                    "Nudge 1 / 10 units, never snapping",
                ),
                ("Drag side handle".into(), "Change width or height"),
                ("Drag corner handle".into(), "Resize proportionally"),
                (
                    "Scroll / side-scroll".into(),
                    "Pan the canvas vertically / horizontally",
                ),
                (zoom, "Zoom at the pointer"),
            ],
        );
        let examples = column![
            button(text("Open shortcut examples ↗").size(14))
                .padding([10, 16])
                .on_press(Message::OpenShortcutExamples)
                .style(control(true)),
            text("One editable ReShiki file with labeled examples. Opens in a separate window; double-click a structure to select it, then copy and paste into your drawing.")
                .size(12).style(muted_text),
        ].spacing(8);
        let body = column![examples, drawing, context, editing, files]
            .spacing(24)
            .width(Length::Fill)
            .padding([0, 12]);
        let popup = container(
            column![
                row![
                    column![
                        text("Help & shortcuts").size(22),
                        text("Quick reference for drawing and editing")
                            .size(12)
                            .style(muted_text)
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
                        .style(muted_text),
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
        .style(|theme| {
            crate::appearance::container(
                theme,
                container::Style {
                    background: Some(Color::WHITE.into()),
                    border: Border {
                        radius: 14.into(),
                        width: 1.,
                        color: Color::from_rgb8(207, 216, 216),
                    },
                    shadow: crate::appearance::surface_shadow(iced::Shadow {
                        color: Color::from_rgba8(20, 40, 35, 0.18),
                        offset: iced::Vector::new(0., 8.),
                        blur_radius: 30.,
                    }),
                    ..Default::default()
                },
            )
        });
        stack![
            base,
            opaque(
                mouse_area(
                    container(Space::new())
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .style(|theme| crate::appearance::container(
                            theme,
                            container::Style {
                                background: Some(Color::from_rgba8(25, 35, 40, 0.18).into()),
                                ..Default::default()
                            }
                        ))
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

fn shortcut(keys: &str, label: &'static str) -> Element<'static, Message> {
    row![
        text(label).size(12).width(Length::Fill),
        container(rich_text(super::shortcuts::spans(keys)).size(11))
            .padding([3, 6])
            .style(|theme| crate::appearance::container(
                theme,
                container::Style {
                    background: Some(Color::from_rgb8(245, 247, 249).into()),
                    border: Border {
                        radius: 4.into(),
                        width: 1.,
                        color: Color::from_rgb8(224, 228, 233)
                    },
                    ..Default::default()
                }
            )),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

fn group(title: &'static str, shortcuts: &[(String, &'static str)]) -> Element<'static, Message> {
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
        let mut renderer = iced::Renderer::Secondary(iced_tiny_skia::Renderer::new(
            iced::Font::default(),
            iced::Pixels(16.),
        ));
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
}
