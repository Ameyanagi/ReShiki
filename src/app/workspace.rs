use super::{
    App, InspectorTab, Message,
    icons::{Glyph, Icon},
};
use crate::canvas::{Edit, MoleculeCanvas, Tool};
use iced::widget::{
    Space, button, canvas, checkbox, column, container, pick_list, row, scrollable, sensor, text,
    text_input, tooltip,
};
use iced::{Alignment, Border, Color, Element, Length, Theme};
use moruno::editing::{Arrange, Transform};

impl App {
    pub(super) fn workspace(&self) -> Element<'_, Message> {
        let mut content = column![self.command_bar()];
        if self.import_open {
            content = content.push(self.import_drawer());
        }
        if self.help_open {
            content = content.push(self.shortcut_drawer());
        }
        if !self.recovered.is_empty() {
            content = content.push(
                container(
                    row![
                        text(format!(
                            "{} recovery draft(s) available",
                            self.recovered.len()
                        ))
                        .size(12),
                        Space::new().width(Length::Fill),
                        command("Restore latest", Message::Restore),
                        command("Later", Message::DismissRecovery)
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .padding([8, 16])
                .style(panel),
            );
        }
        if self.pending.is_some() {
            content = content.push(
                container(
                    row![
                        text("This drawing has unsaved changes.").size(12),
                        Space::new().width(Length::Fill),
                        command("Save", Message::Save).style(button::primary),
                        command("Discard & continue", Message::Discard).style(button::danger),
                        command("Cancel", Message::Cancel)
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .padding([8, 16])
                .style(panel),
            );
        }
        let drawing: Element<'_, Edit> = canvas(MoleculeCanvas {
            doc: &self.doc,
            selected: &self.selected,
            tool: self.tool,
            camera: self.camera,
            grid: self.grid,
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .into();
        let paper = sensor(drawing.map(Message::Canvas))
            .on_show(Message::Viewport)
            .on_resize(Message::Viewport);
        let workspace = column![
            self.context_bar(),
            container(
                container(paper)
                    .style(sheet)
                    .width(Length::Fill)
                    .height(Length::Fill)
            )
            .padding(18)
            .width(Length::Fill)
            .height(Length::Fill)
        ]
        .height(Length::Fill)
        .width(Length::Fill);
        let mut body = row![self.tool_palette(), workspace].height(Length::Fill);
        if self.inspector_open {
            body = body.push(self.inspector());
        }
        content
            .push(body)
            .push(self.status_bar())
            .height(Length::Fill)
            .into()
    }

    fn command_bar(&self) -> Element<'_, Message> {
        let title = self
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".into());
        let bar = row![
            text("moruno").size(21).color(Color::from_rgb8(22, 91, 81)),
            divider(),
            icon_button(Icon::New, "New · ⌘N", Some(Message::New), false),
            icon_button(Icon::Open, "Open · ⌘O", Some(Message::Open), false),
            icon_button(Icon::Save, "Save · ⌘S", Some(Message::Save), false),
            command("Save as", Message::SaveAs),
            divider(),
            icon_button(
                Icon::Undo,
                "Undo · ⌘Z",
                self.history.can_undo().then_some(Message::Undo),
                false
            ),
            icon_button(
                Icon::Redo,
                "Redo · ⇧⌘Z",
                self.history.can_redo().then_some(Message::Redo),
                false
            ),
            Space::new().width(8),
            column![
                text(title).size(12),
                text(if self.dirty() {
                    "Edited"
                } else {
                    if self.path.is_some() {
                        "All changes saved"
                    } else {
                        "Not saved to file"
                    }
                })
                .size(10)
                .color(muted())
            ]
            .spacing(2)
            .width(Length::Fill),
            action(
                Icon::Import,
                "Import",
                Message::ToggleImport,
                self.import_open
            ),
            command(
                if self.busy { "Checking…" } else { "Check" },
                Message::Analyze
            )
            .on_press_maybe((!self.busy).then_some(Message::Analyze)),
            command("Clean up", Message::Clean)
                .on_press_maybe((!self.busy).then_some(Message::Clean)),
            action(
                Icon::Export,
                "Export",
                Message::Inspector(InspectorTab::Export),
                self.inspector_open && self.inspector_tab == InspectorTab::Export
            ),
            icon_button(
                Icon::Inspector,
                "Show or hide inspector",
                Some(Message::ToggleInspector),
                self.inspector_open
            )
        ]
        .spacing(4)
        .align_y(Alignment::Center);
        container(bar).padding([9, 14]).style(panel).into()
    }

    fn tool_palette(&self) -> Element<'_, Message> {
        let tools = [
            (Tool::Select, "Select / move · V"),
            (Tool::Atom, "Atom label · C, N, O…"),
            (Tool::Bond(1), "Single bond · B / 1"),
            (Tool::Bond(2), "Double bond · 2"),
            (Tool::Bond(3), "Triple bond · 3"),
            (Tool::Wavy, "Wavy bond"),
            (Tool::Wedge, "Solid wedge"),
            (Tool::Hash, "Hashed wedge"),
            (Tool::Ring, "Ring · R"),
            (Tool::Arrow, "Reaction arrow · A"),
            (Tool::Text, "Text label · T"),
            (Tool::Erase, "Eraser · E"),
        ];
        let mut palette = column![section("TOOLS")]
            .spacing(6)
            .align_x(Alignment::Center);
        for pair in tools.chunks(2) {
            let mut line = row![].spacing(4);
            for (tool, hint) in pair {
                line = line.push(icon_button(
                    Icon::Tool(*tool),
                    hint,
                    Some(Message::Tool(*tool)),
                    self.tool == *tool,
                ));
            }
            palette = palette.push(line);
        }
        palette = palette.push(Space::new().height(10)).push(section("ATOMS"));
        for pair in [["C", "N"], ["O", "S"], ["P", "F"], ["Cl", "Br"]] {
            let mut line = row![].spacing(4);
            for symbol in pair {
                line = line.push(
                    button(text(symbol).size(13).center())
                        .width(36)
                        .height(30)
                        .on_press(Message::Element(symbol.into()))
                        .style(control(self.tool == Tool::Atom && self.element == symbol)),
                );
            }
            palette = palette.push(line);
        }
        palette = palette
            .push(Space::new().height(Length::Fill))
            .push(tooltip(
                command("?", Message::ToggleHelp).width(36),
                "Keyboard shortcuts",
                tooltip::Position::Right,
            ));
        container(palette)
            .width(96)
            .height(Length::Fill)
            .padding([14, 8])
            .style(panel)
            .into()
    }

    fn context_bar(&self) -> Element<'_, Message> {
        let mut options = row![text(tool_name(self.tool)).size(12).color(ink()), divider()]
            .spacing(10)
            .align_y(Alignment::Center);
        match self.tool {
            Tool::Ring => {
                options = options
                    .push(text("Size").size(11).color(muted()))
                    .push(
                        pick_list(
                            [3_u8, 4, 5, 6, 7, 8],
                            Some(self.ring_size),
                            Message::RingSize,
                        )
                        .text_size(12)
                        .padding(5),
                    )
                    .push(
                        checkbox(self.aromatic_ring)
                            .label("Aromatic")
                            .on_toggle(Message::AromaticRing)
                            .size(14)
                            .text_size(12),
                    )
                    .push(text("Click a bond to fuse").size(11).color(muted()));
            }
            Tool::Arrow => {
                options = options
                    .push(
                        pick_list(
                            [
                                "Forward",
                                "Equilibrium",
                                "Resonance",
                                "Retrosynthesis",
                                "Curved",
                            ],
                            Some(self.arrow_style),
                            Message::ArrowStyle,
                        )
                        .text_size(12)
                        .padding(5),
                    )
                    .push(
                        text("Drag to set direction and length")
                            .size(11)
                            .color(muted()),
                    );
            }
            Tool::Atom => {
                options = options
                    .push(text(format!("Element: {}", self.element)).size(12))
                    .push(
                        text_input("Symbol: Si, Na, Fe…", &self.custom_element)
                            .on_input(Message::CustomElement)
                            .on_submit(Message::ApplyElement)
                            .size(12)
                            .padding(6)
                            .width(160),
                    )
                    .push(command("Use", Message::ApplyElement));
            }
            Tool::Text => {
                options = options
                    .push(
                        text_input("Annotation · \\n for a new line", &self.caption)
                            .on_input(Message::Caption)
                            .size(12)
                            .padding(6)
                            .width(Length::Fill),
                    )
                    .push(
                        command("Update selected", Message::UpdateLabel).on_press_maybe(
                            self.doc
                                .annotations
                                .iter()
                                .any(|a| self.selected.contains(&a.id))
                                .then_some(Message::UpdateLabel),
                        ),
                    );
            }
            Tool::Select if !self.selected.is_empty() => {
                options = options
                    .push(
                        text(format!("{} selected", self.selected.len()))
                            .size(11)
                            .color(muted()),
                    )
                    .push(command("Cut", Message::Copy(true)))
                    .push(command("Copy", Message::Copy(false)))
                    .push(command("Paste", Message::Paste))
                    .push(command("Duplicate", Message::Duplicate));
            }
            Tool::Select => {
                options = options.push(
                    text("Double-click an atom to select its molecule")
                        .size(11)
                        .color(muted()),
                );
                options = options.push(command("Paste", Message::Paste));
            }
            _ => {
                options = options.push(text(self.tool.hint()).size(11).color(muted()));
            }
        }
        if self.tool != Tool::Text {
            options = options.push(Space::new().width(Length::Fill));
        }
        options = options.push(
            container(
                text("JACS / ACS")
                    .size(10)
                    .color(Color::from_rgb8(28, 109, 91)),
            )
            .padding([5, 8])
            .style(badge),
        );
        container(options)
            .height(46)
            .padding([5, 14])
            .center_y(46)
            .style(panel)
            .into()
    }

    fn inspector(&self) -> Element<'_, Message> {
        let mut tabs = row![].spacing(2);
        for (label, tab) in [
            ("Properties", InspectorTab::Properties),
            ("Templates", InspectorTab::Templates),
            ("Export", InspectorTab::Export),
        ] {
            tabs = tabs.push(
                button(text(label).size(11))
                    .padding([7, 8])
                    .style(control(self.inspector_tab == tab))
                    .on_press(Message::Inspector(tab)),
            );
        }
        let body = match self.inspector_tab {
            InspectorTab::Properties => self.properties_panel(),
            InspectorTab::Templates => self.templates_panel(),
            InspectorTab::Export => self.export_panel(),
        };
        container(
            column![
                container(tabs).padding([8, 8]),
                scrollable(container(body).padding([8, 16])).height(Length::Fill)
            ]
            .spacing(4),
        )
        .width(256)
        .height(Length::Fill)
        .style(panel)
        .into()
    }

    fn properties_panel(&self) -> Element<'_, Message> {
        let mut body = column![
            section("STRUCTURE"),
            text(format!(
                "{} atoms · {} bonds",
                self.doc.atoms.len(),
                self.doc.bonds.len()
            ))
            .size(12)
            .color(muted())
        ]
        .spacing(10);
        if let Some(a) = &self.analysis {
            body = body.push(text(&a.formula).size(25)).push(horizontal_line());
            for (label, value) in [
                ("Weight (g/mol)", format!("{:.3}", a.mass)),
                ("Exact mass (Da)", format!("{:.5}", a.exact_mass)),
                ("cLogP", format!("{:.2}", a.logp)),
                ("TPSA (Å²)", format!("{:.2}", a.tpsa)),
                ("H-bond donors", a.donors.to_string()),
                ("H-bond acceptors", a.acceptors.to_string()),
                ("Rings", a.rings.to_string()),
            ] {
                body = body.push(
                    row![
                        text(label).size(11).color(muted()),
                        Space::new().width(Length::Fill),
                        text(value).size(12)
                    ]
                    .align_y(Alignment::Center),
                );
            }
            body = body
                .push(Space::new().height(4))
                .push(section("CANONICAL SMILES"))
                .push(text(&a.smiles).size(11))
                .push(command("Copy SMILES", Message::CopySmiles));
        } else {
            body = body
                .push(
                    text("Check the structure to refresh its formula and properties.")
                        .size(12)
                        .color(muted()),
                )
                .push(
                    command("Check structure", Message::Analyze)
                        .on_press_maybe((!self.busy).then_some(Message::Analyze)),
                );
        }
        if !self.selected.is_empty() {
            body = body
                .push(horizontal_line())
                .push(section("SELECTION"))
                .push(self.selection_panel());
        }
        body.push(horizontal_line())
            .push(section("PUBLICATION STYLE"))
            .push(text("JACS / ACS").size(13))
            .push(
                text("Arial 10 pt\nBonds 14.4 pt · Lines 0.6 pt\nBlack artwork · PNG 1200 dpi")
                    .size(11)
                    .color(muted()),
            )
            .into()
    }

    fn selection_panel(&self) -> Element<'_, Message> {
        let has_atoms = self.doc.atoms.iter().any(|a| self.selected.contains(&a.id));
        let mut body = column![
            row![
                command("↶ 30°", Message::Transform(Transform::Rotate(-30.0))).width(Length::Fill),
                command("↷ 30°", Message::Transform(Transform::Rotate(30.0))).width(Length::Fill)
            ]
            .spacing(6),
            row![
                command("Flip H", Message::Transform(Transform::FlipHorizontal))
                    .width(Length::Fill),
                command("Flip V", Message::Transform(Transform::FlipVertical)).width(Length::Fill)
            ]
            .spacing(6),
            row![
                command("Align X", Message::Arrange(Arrange::AlignHorizontal)).width(Length::Fill),
                command("Align Y", Message::Arrange(Arrange::AlignVertical)).width(Length::Fill)
            ]
            .spacing(6),
            row![
                command(
                    "Distribute X",
                    Message::Arrange(Arrange::DistributeHorizontal)
                )
                .width(Length::Fill),
                command(
                    "Distribute Y",
                    Message::Arrange(Arrange::DistributeVertical)
                )
                .width(Length::Fill)
            ]
            .spacing(6),
        ]
        .spacing(6);
        if has_atoms {
            body = body
                .push(
                    row![
                        text("Charge").size(12).width(Length::Fill),
                        command("−", Message::Charge(-1)),
                        command("+", Message::Charge(1))
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                )
                .push(
                    row![
                        text_input("Isotope mass", &self.isotope)
                            .on_input(Message::Isotope)
                            .on_submit(Message::ApplyIsotope)
                            .size(12)
                            .padding(7),
                        command("Set", Message::ApplyIsotope)
                    ]
                    .spacing(6),
                )
                .push(command("Reverse bonds", Message::ReverseBonds));
        }
        if self
            .doc
            .annotations
            .iter()
            .any(|a| self.selected.contains(&a.id))
        {
            body = body
                .push(
                    text_input("Annotation", &self.caption)
                        .on_input(Message::Caption)
                        .on_submit(Message::UpdateLabel)
                        .size(12)
                        .padding(7),
                )
                .push(command("Update label", Message::UpdateLabel));
        }
        body.push(command("Delete selection", Message::Delete).style(button::danger))
            .into()
    }

    fn templates_panel(&self) -> Element<'_, Message> {
        let mut body = column![
            section("MOLECULE LIBRARY"),
            text("Insert a fragment, then position it on the canvas.")
                .size(12)
                .color(muted())
        ]
        .spacing(10);
        for (group, templates) in [
            (
                "RINGS",
                &[
                    ("Benzene", "c1ccccc1"),
                    ("Pyridine", "c1ccncc1"),
                    ("Pyrrole", "c1cc[nH]c1"),
                    ("Furan", "c1ccoc1"),
                    ("Thiophene", "c1ccsc1"),
                    ("Cyclopentane", "C1CCCC1"),
                    ("Cyclohexane", "C1CCCCC1"),
                    ("Naphthalene", "c1ccc2ccccc2c1"),
                ][..],
            ),
            (
                "SMALL MOLECULES",
                &[
                    ("Acetaldehyde", "CC=O"),
                    ("Acetic acid", "CC(=O)O"),
                    ("Methylamine", "CN"),
                    ("Methanol", "CO"),
                ][..],
            ),
        ] {
            body = body.push(Space::new().height(4)).push(section(group));
            for (name, smiles) in templates {
                body = body.push(
                    button(
                        row![
                            text(*name).size(12).width(Length::Fill),
                            text("+").size(16).color(muted())
                        ]
                        .align_y(Alignment::Center),
                    )
                    .padding([8, 10])
                    .width(Length::Fill)
                    .style(control(false))
                    .on_press_maybe((!self.busy).then_some(Message::InsertTemplate(smiles))),
                );
            }
        }
        body.into()
    }

    fn export_panel(&self) -> Element<'_, Message> {
        let mut body = column![
            section("DRAWING"),
            text("JACS / ACS · physical publication size")
                .size(11)
                .color(muted())
        ]
        .spacing(10);
        for (label, format) in [
            ("PDF · vector", "pdf"),
            ("SVG · editable vector", "svg"),
            ("PNG · 1200 dpi", "png"),
        ] {
            body = body.push(
                command(label, Message::Export(format))
                    .on_press_maybe((!self.busy).then_some(Message::Export(format)))
                    .width(Length::Fill),
            );
        }
        body = body.push(horizontal_line()).push(section("CHEMICAL DATA"));
        for (label, format) in [
            ("MOL structure", "mol"),
            ("SMILES text", "smiles"),
            ("InChI identifier", "inchi"),
            ("CDXML · basic drawing", "cdxml"),
        ] {
            body = body.push(
                command(label, Message::Export(format))
                    .on_press_maybe((!self.busy).then_some(Message::Export(format)))
                    .width(Length::Fill),
            );
        }
        body.push(
            text("Save as .moruno to retain the complete editable drawing.")
                .size(11)
                .color(muted()),
        )
        .push(command("Save native document", Message::SaveAs))
        .into()
    }

    fn import_drawer(&self) -> Element<'_, Message> {
        container(
            column![
                row![
                    text("Import structure").size(13),
                    Space::new().width(Length::Fill),
                    icon_button(
                        Icon::Close,
                        "Close import",
                        Some(Message::ToggleImport),
                        false
                    )
                ]
                .align_y(Alignment::Center),
                row![
                    text_input("SMILES, InChI, MOL or CDXML", &self.smiles)
                        .on_input(Message::Smiles)
                        .on_submit(Message::InsertInput)
                        .size(13)
                        .padding(9),
                    command("Insert", Message::InsertInput)
                        .style(button::primary)
                        .on_press_maybe((!self.busy).then_some(Message::InsertInput)),
                    command("Replace drawing", Message::Import)
                        .on_press_maybe((!self.busy).then_some(Message::Import))
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                row![
                    text("Try").size(11).color(muted()),
                    command("Ethanol", Message::Example("CCO")),
                    command("Benzene", Message::Example("c1ccccc1")),
                    command("Aspirin", Message::Example("CC(=O)Oc1ccccc1C(=O)O")),
                    command("Caffeine", Message::Example("Cn1c(=O)c2c(ncn2C)n(C)c1=O")),
                    text("Examples replace the drawing; Undo restores it.")
                        .size(11)
                        .color(muted())
                ]
                .spacing(6)
                .align_y(Alignment::Center)
            ]
            .spacing(8),
        )
        .padding([10, 18])
        .style(panel)
        .into()
    }

    fn shortcut_drawer(&self) -> Element<'_, Message> {
        container(row![
            column![text("Draw without leaving the canvas").size(13),text("V Select   B / 1 Bond   2 Double   3 Triple   R Ring   A Arrow   T Text   E Erase").size(12),text("C / N / O / S / P / F Atom   ⌘I Import   ⌘E Export   ⌘D Duplicate   Esc Select").size(12)].spacing(5),
            Space::new().width(Length::Fill),icon_button(Icon::Close,"Close shortcuts",Some(Message::ToggleHelp),false)
        ].align_y(Alignment::Center)).padding([12,18]).style(panel).into()
    }

    fn status_bar(&self) -> Element<'_, Message> {
        let left = column![text(&self.status).size(11).color(if self.error {
            Color::from_rgb8(168, 52, 47)
        } else {
            muted()
        })]
        .width(Length::Fill);
        let status = row![
            left,
            text(&self.autosave_status).size(10).color(muted()),
            text(format!("{} selected", self.selected.len()))
                .size(11)
                .color(muted()),
            divider(),
            checkbox(self.grid)
                .label("Grid")
                .on_toggle(|_| Message::Grid)
                .size(13)
                .text_size(11),
            command("−", Message::Zoom(0.8)),
            text(format!("{:.0}%", self.camera.zoom * 100.0))
                .size(11)
                .width(38)
                .center(),
            command("+", Message::Zoom(1.25)),
            command("Fit", Message::Fit)
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        container(status).padding([6, 14]).style(panel).into()
    }
}

fn command(label: &str, message: Message) -> button::Button<'_, Message> {
    button(text(label).size(12))
        .padding([7, 9])
        .on_press(message)
        .style(control(false))
}
fn icon_button(
    icon: Icon,
    hint: &'static str,
    message: Option<Message>,
    active: bool,
) -> Element<'static, Message> {
    tooltip(
        button(canvas(Glyph(icon, message.is_some())).width(24).height(24))
            .width(36)
            .height(36)
            .padding(6)
            .style(control(active))
            .on_press_maybe(message),
        container(text(hint).size(12)).padding([7, 10]).style(tip),
        tooltip::Position::Bottom,
    )
    .delay(std::time::Duration::from_millis(400))
    .gap(6)
    .into()
}
fn action(
    icon: Icon,
    label: &'static str,
    message: Message,
    active: bool,
) -> Element<'static, Message> {
    button(
        row![
            canvas(Glyph(icon, true)).width(24).height(24),
            text(label).size(12)
        ]
        .spacing(5)
        .align_y(Alignment::Center),
    )
    .on_press(message)
    .style(control(active))
    .padding([6, 9])
    .into()
}
fn section(label: &str) -> iced::widget::Text<'_> {
    text(label).size(10).color(muted())
}
fn ink() -> Color {
    Color::from_rgb8(37, 43, 51)
}
fn muted() -> Color {
    Color::from_rgb8(107, 116, 127)
}
fn divider() -> Element<'static, Message> {
    container(
        container(Space::new().width(1).height(20)).style(|_| container::Style {
            background: Some(Color::from_rgb8(223, 227, 232).into()),
            ..Default::default()
        }),
    )
    .padding([0, 5])
    .into()
}
fn horizontal_line() -> Element<'static, Message> {
    container(
        container(Space::new().height(1).width(Length::Fill)).style(|_| container::Style {
            background: Some(Color::from_rgb8(230, 233, 237).into()),
            ..Default::default()
        }),
    )
    .padding([7, 0])
    .into()
}
fn control(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let disabled = matches!(status, button::Status::Disabled);
        button::Style {
            background: Some(
                if active {
                    Color::from_rgb8(222, 240, 234)
                } else if hovered {
                    Color::from_rgb8(233, 237, 241)
                } else {
                    Color::TRANSPARENT
                }
                .into(),
            ),
            text_color: if disabled {
                Color::from_rgb8(183, 189, 195)
            } else if active {
                Color::from_rgb8(15, 103, 85)
            } else {
                ink()
            },
            border: Border {
                color: if active {
                    Color::from_rgb8(113, 181, 161)
                } else {
                    Color::TRANSPARENT
                },
                width: 1.,
                radius: 5.0.into(),
            },
            ..Default::default()
        }
    }
}
fn panel(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Color::from_rgb8(250, 251, 252).into()),
        border: Border {
            color: Color::from_rgb8(224, 228, 233),
            width: 1.,
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}
fn sheet(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Color::WHITE.into()),
        border: Border {
            color: Color::from_rgb8(215, 220, 226),
            width: 1.,
            radius: 1.0.into(),
        },
        shadow: iced::Shadow {
            color: Color::from_rgba8(35, 45, 57, 0.08),
            offset: iced::Vector::new(0., 2.),
            blur_radius: 8.,
        },
        ..Default::default()
    }
}
fn badge(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Color::from_rgb8(234, 245, 240).into()),
        border: Border {
            radius: 4.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}
fn tip(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Color::from_rgb8(40, 48, 57).into()),
        text_color: Some(Color::WHITE),
        border: Border {
            radius: 5.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}
fn tool_name(tool: Tool) -> &'static str {
    match tool {
        Tool::Select => "Select / move",
        Tool::Atom => "Atom label",
        Tool::Bond(1) => "Single bond",
        Tool::Bond(2) => "Double bond",
        Tool::Bond(_) => "Triple bond",
        Tool::Wedge => "Solid wedge",
        Tool::Hash => "Hashed wedge",
        Tool::Wavy => "Wavy bond",
        Tool::Ring => "Ring",
        Tool::Arrow => "Reaction arrow",
        Tool::Text => "Text label",
        Tool::Erase => "Eraser",
    }
}
