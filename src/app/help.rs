use super::{
    App, Message,
    shortcuts::keys,
    workspace::{control, muted_text},
};
use iced::widget::{
    Space, column, container, mouse_area, opaque, rich_text, row, scrollable, stack, text,
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
            // Keep the workspace at the same child position while Help opens
            // and closes, so its inert focus and text selection survive.
            return stack![base].into();
        }
        let base = reshiki::accessibility::inert(base);
        let drawing = drawing_shortcuts();
        let editing = editing_shortcuts();
        let context = context_shortcuts();
        let files = file_shortcuts();
        let examples = help_examples();
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
                    reshiki::accessibility::button(
                        "help-close",
                        "Close Help and shortcuts",
                        text("×").size(25).center()
                    )
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
                    reshiki::accessibility::button(
                        "help-done",
                        "Done; close Help and shortcuts",
                        text("Done").size(13)
                    )
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
    pub(super) fn toggle_help(&mut self) {
        self.help_open = !self.help_open;
        if self.help_open {
            self.palette = None;
        }
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

fn shortcut_label(message: Message) -> String {
    super::shortcuts::label(&message).unwrap_or_default()
}

fn drawing_shortcuts() -> Element<'static, Message> {
    let (command, shift, alt) = (Modifiers::COMMAND, Modifiers::SHIFT, Modifiers::ALT);
    group(
        "Drawing tools",
        &[
            ("Space / l".into(), "Select / Lasso"),
            ("x / 1".into(), "Single bond"),
            ("2 / 3 / 4".into(), "Double / Triple / Quadruple"),
            (keys(shift, "X"), "Straight chain"),
            (
                format!("{}–{}", keys(shift, "3"), keys(shift, "8")),
                "Saturated 3–8-membered ring tools",
            ),
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
                "F8".into(),
                "Keyboard drawing on/off; orange hotspot shows the active atom or bond",
            ),
            (
                "Arrows / Shift-arrows in keyboard drawing".into(),
                "Navigate atom–bond–atom / skip to the same target kind",
            ),
            (
                "[ / ] in keyboard drawing".into(),
                "Mark an atom / connect the active atom to the marked atom",
            ),
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
    )
}

fn editing_shortcuts() -> Element<'static, Message> {
    let (command, shift) = (Modifiers::COMMAND, Modifiers::SHIFT);
    group(
        "Selection & arrangement",
        &[
            (
                format!(
                    "{} / {} / {}",
                    shortcut_label(Message::Copy(true)),
                    shortcut_label(Message::Copy(false)),
                    shortcut_label(Message::Paste)
                ),
                "Cut / Copy / Paste",
            ),
            (shortcut_label(Message::SelectAll), "Select all"),
            (shortcut_label(Message::Group), "Group"),
            (shortcut_label(Message::Ungroup), "Ungroup"),
            (shortcut_label(Message::InvertSelection), "Invert selection"),
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
                format!(
                    "{} / {}",
                    shortcut_label(Message::Undo),
                    shortcut_label(Message::Redo)
                ),
                "Undo / Redo",
            ),
            (shortcut_label(Message::Delete), "Delete selection"),
            (
                keys(Modifiers::empty(), "Enter"),
                "Edit selected atom label (M, L, X, Boc…)",
            ),
        ],
    )
}

fn context_shortcuts() -> iced::widget::Column<'static, Message> {
    column![
        text("Under the pointer or keyboard hotspot (case-sensitive)").size(14),
        shortcut("1 / 2 / 3", "Bond: single / double / triple"),
        shortcut("b / w / h / y", "Bond: bold / wedge / hashed wedge / wavy"),
        shortcut(
            "d / D / B / H",
            "Bond: dashed / partial double / bold double / hashed"
        ),
        shortcut("l / c / r", "Double line: left / center / right"),
        shortcut("c n o s p f h", "Atom: C N O S P F H"),
        shortcut("b / C / B / i / L / S", "Atom: Br / Cl / B / I / Li / Si"),
        shortcut("m / e / y / P", "Atom: Me / Et / Boc / Ph"),
        shortcut("M / Z", "Atom: MgBr / N₃ (complete chemical groups)"),
        shortcut(
            "j / J on an atom",
            "Tilted Cp / arene ligand; repeat at a metal to add another"
        ),
        shortcut(
            "A / E / F / H / N / O / Q",
            "Ac / CO₂Me / CF₃ / Cbz / NO₂ / OMe / Fmoc"
        ),
        shortcut("d / + / −", "Atom: deuterium / increase / decrease charge"),
        shortcut("r / x", "Atom: variable R / X"),
        shortcut(
            "0 / 1 / 2 / 8 / z",
            "Atom: branch / chain / carbonyl / =CH₂ / alkyne"
        ),
        shortcut(
            "3 / 6 / 7 / v / u",
            "Atom: phenyl / 6-ring / 5-ring / 3-ring / 4-ring"
        ),
        shortcut("9 / K / k", "Atom: dimethyl / tert-butyl / sulfonyl"),
        shortcut(
            "v / 4–8 / a / z / 9 / 0",
            "Bond: fuse rings / benzene / diene / chairs"
        ),
        shortcut(
            &format!("g / ? / {}", keys(Modifiers::empty(), "Enter")),
            "Select / Properties / Edit atom label"
        ),
        text(
            "Uppercase means Shift-letter. Repeat 2 on a double bond to cycle its line placement."
        )
        .size(12)
        .style(muted_text),
    ]
    .spacing(8)
}

fn file_shortcuts() -> Element<'static, Message> {
    let (command, shift, alt) = (Modifiers::COMMAND, Modifiers::SHIFT, Modifiers::ALT);
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
    group(
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
                shortcut_label(Message::Optimization(super::optimization::Action::Begin)),
                "Generate an optimized 3D conformer",
            ),
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
    )
}

fn help_examples() -> iced::widget::Column<'static, Message> {
    column![
        reshiki::accessibility::button("help-examples", "Open shortcut examples", text("Open shortcut examples").size(14))
            .padding([10, 16])
            .on_press(Message::OpenShortcutExamples)
            .style(control(true)),
        text("One editable drawing with labeled examples. Opens in a tab; double-click a structure to select it, then copy and paste into your drawing. Save creates your own copy.")
            .size(12).style(muted_text),
    ].spacing(8)
}

#[cfg(test)]
mod tests;
