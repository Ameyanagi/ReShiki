use super::{
    App, InspectorTab, Message,
    icons::{Glyph, Icon},
};
use crate::canvas::layered::canvas;
use crate::canvas::{Edit, MoleculeCanvas, Tool};
use iced::widget::{
    Space, button, checkbox, column, combo_box, container, pick_list, row, scrollable, sensor,
    text, text_editor, text_input, tooltip,
};
use iced::{Alignment, Border, Color, Element, Length, Theme};
use reshiki::bonds::BondPreset;
use reshiki::typography::{Script, StyleChange, TextAlign};

impl App {
    pub(super) fn selection_summary(&self) -> String {
        let groups = self.doc.outer_selected_groups(&self.selected);
        let covered: std::collections::HashSet<_> = self
            .doc
            .groups
            .iter()
            .filter(|g| groups.contains(&g.id))
            .flat_map(|g| &g.members)
            .collect();
        let atoms = self
            .doc
            .atoms
            .iter()
            .filter(|a| self.selected.contains(&a.id))
            .count();
        let points = self
            .doc
            .atoms
            .iter()
            .filter(|a| {
                self.selected.contains(&a.id) && a.element == "*" && a.display.variable.is_none()
            })
            .count();
        let bonds = self
            .doc
            .bonds
            .iter()
            .filter(|b| self.selected.contains(&b.a) && self.selected.contains(&b.b))
            .count();
        let objects = self.selected.len().saturating_sub(atoms);
        let mut parts = Vec::new();
        let mut add = |count: usize, label: &str| {
            if count > 0 {
                parts.push(format!(
                    "{count} {label}{}",
                    if count == 1 { "" } else { "s" }
                ));
            }
        };
        if !groups.is_empty() && self.selected.iter().all(|id| covered.contains(id)) {
            add(groups.len(), "group");
        } else {
            add(atoms.saturating_sub(points), "atom");
            add(points, "point");
            add(objects, "object");
        }
        add(bonds, "bond");
        if parts.is_empty() {
            "No selection".into()
        } else {
            parts.join(" · ")
        }
    }

    pub(super) fn can_group(&self) -> bool {
        self.selected.len() > 1
            && !self.doc.groups.iter().any(|g| {
                g.members.len() == self.selected.len()
                    && g.members.iter().all(|id| self.selected.contains(id))
            })
    }
    pub(super) fn graphic_panel(&self) -> Element<'_, Message> {
        use reshiki::graphics::{BracketSides, GraphicChange, GraphicKind, LinePattern};
        let selected: Vec<_> = self
            .doc
            .graphics
            .iter()
            .filter(|g| self.selected.contains(&g.id) && g.picture.is_none())
            .collect();
        let kind = match self.tool {
            Tool::Graphic(k) => k,
            _ => selected
                .first()
                .map(|g| g.kind)
                .unwrap_or(GraphicKind::Rectangle),
        };
        let swatch = |c: [u8; 3], fill: bool| {
            button(Space::new().width(19).height(19))
                .padding(3)
                .style(move |_, _| button::Style {
                    background: Some(Color::from_rgb8(c[0], c[1], c[2]).into()),
                    border: Border {
                        color: Color::from_rgb8(186, 198, 195),
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    ..Default::default()
                })
                .on_press(Message::GraphicStyle(if fill {
                    GraphicChange::Fill(Some(c))
                } else {
                    GraphicChange::Stroke(c)
                }))
        };
        let mut panel = column![
            section(if selected.is_empty() {
                "DRAWING STYLE"
            } else {
                "GRAPHIC STYLE"
            }),
            text(kind.to_string()).size(14),
            row![
                text("Line (pt)").size(11),
                text_input("0.6", &self.graphic_width_input)
                    .on_input(Message::GraphicWidth)
                    .on_submit(Message::ApplyGraphicWidth)
                    .size(12)
                    .padding(5)
                    .width(48),
                pick_list(
                    [LinePattern::Solid, LinePattern::Dashed, LinePattern::Dotted],
                    Some(self.graphic_style.pattern),
                    |p| Message::GraphicStyle(GraphicChange::Pattern(p))
                )
                .text_size(12)
                .padding(5)
            ]
            .spacing(6)
            .align_y(Alignment::Center),
            text("Stroke color").size(11).color(muted()),
            row![
                swatch([0, 0, 0], false),
                swatch([32, 80, 145], false),
                swatch([17, 126, 108], false),
                swatch([180, 50, 55], false),
                swatch([116, 65, 147], false)
            ]
            .spacing(6),
            text_input("#RRGGBB", &self.graphic_stroke_input)
                .on_input(Message::GraphicStroke)
                .on_submit(Message::ApplyGraphicStroke)
                .size(12)
                .padding(6),
        ]
        .spacing(9);
        if matches!(kind, GraphicKind::Symbol(_) | GraphicKind::Orbital(_)) {
            let mut preview = selected.first().map(|g| (*g).clone()).unwrap_or_else(|| {
                reshiki::graphics::Graphic::dragged(
                    1,
                    kind,
                    reshiki::document::Point::default(),
                    reshiki::document::Point::default(),
                    self.graphic_style.clone(),
                    self.bracket_sides,
                    false,
                )
            });
            preview.phase = self.orbital_phase;
            preview.phase_flipped = self.phase_flipped;
            panel = panel.push(container(
                canvas(crate::canvas::ScientificPreview(preview))
                    .width(Length::Fill)
                    .height(92),
            ));
        }
        match kind {
            GraphicKind::Symbol(kind) => {
                panel=panel.push(pick_list(reshiki::scientific::SymbolKind::ALL,Some(kind),|k|Message::ScientificKind(GraphicKind::Symbol(k))).text_size(12).padding(6).width(Length::Fill))
                    .push(checkbox(self.attach_symbols).label("Attach to atoms").on_toggle(Message::AttachSymbols).size(14).text_size(12))
                    .push(text("Attached charges and radicals update chemistry. Lone pairs annotate the atom. H and attachment symbols use free placement.").size(11).color(muted()));
            }
            GraphicKind::Orbital(kind) => {
                panel=panel.push(pick_list(reshiki::scientific::OrbitalKind::ALL,Some(kind),|k|Message::ScientificKind(GraphicKind::Orbital(k))).text_size(12).padding(6).width(Length::Fill))
                    .push(pick_list(reshiki::scientific::Phase::ALL,Some(self.orbital_phase),Message::OrbitalPhase).text_size(12).padding(6).width(Length::Fill))
                    .push(checkbox(self.phase_flipped).label("Reverse phases").on_toggle_maybe((!matches!(kind, reshiki::scientific::OrbitalKind::S | reshiki::scientific::OrbitalKind::Sigma | reshiki::scientific::OrbitalKind::Lobe)).then_some(Message::FlipPhase)).size(14).text_size(12))
                    .push(text("Drag from the orbital node to set direction and size. Click uses one bond length. Shift snaps to 15°. Group with a molecule to move them together.").size(11).color(muted()));
            }
            _ => {}
        }
        if kind.closed()
            || (kind == GraphicKind::Path
                && selected.iter().any(|g| {
                    g.commands()
                        .iter()
                        .any(|c| matches!(c, reshiki::graphics::PathCommand::Close))
                }))
        {
            panel = panel
                .push(
                    checkbox(self.graphic_style.fill.is_some())
                        .label("Fill shape")
                        .size(14)
                        .text_size(12)
                        .on_toggle(|v| {
                            Message::GraphicStyle(GraphicChange::Fill(v.then(|| {
                                super::graphics::parse_color(&self.graphic_fill_input)
                                    .unwrap_or([220, 239, 233])
                            })))
                        }),
                )
                .push(
                    row![
                        swatch([220, 239, 233], true),
                        swatch([221, 232, 248], true),
                        swatch([253, 239, 203], true),
                        swatch([249, 223, 225], true),
                        swatch([255, 255, 255], true)
                    ]
                    .spacing(6),
                )
                .push(
                    text_input("#RRGGBB", &self.graphic_fill_input)
                        .on_input(Message::GraphicFill)
                        .on_submit(Message::ApplyGraphicFill)
                        .size(12)
                        .padding(6),
                );
        }
        if kind.brackets() {
            panel = panel.push(
                pick_list(
                    [BracketSides::Both, BracketSides::Left, BracketSides::Right],
                    Some(self.bracket_sides),
                    Message::GraphicSides,
                )
                .text_size(12)
                .padding(6),
            );
        }
        if !selected.is_empty() {
            panel = panel.push(
                row![
                    command("Send to back", Message::GraphicLayer(false)),
                    command("Bring to front", Message::GraphicLayer(true))
                ]
                .spacing(4),
            );
        }
        if matches!(selected.as_slice(), [g] if matches!(g.kind, GraphicKind::Curve | GraphicKind::Path))
        {
            panel = panel.push(command(
                if self.tool == Tool::EditPoints {
                    "Finish editing points"
                } else {
                    "Edit curve points"
                },
                Message::Tool(if self.tool == Tool::EditPoints {
                    Tool::Select
                } else {
                    Tool::EditPoints
                }),
            ));
        }
        panel.push(text("Drag to size. Select to move, rotate or resize. Enter applies numeric and color fields.").size(11).color(muted())).into()
    }

    fn style_bar(&self) -> Element<'_, Message> {
        let style = self.current_text_style();
        let toggle = |label: &'static str,
                      help: &'static str,
                      active: bool,
                      change: StyleChange|
         -> Element<'_, Message> {
            hover_hint(
                button(text(label).size(14))
                    .padding([5, 8])
                    .style(control(active))
                    .on_press(Message::TextStyle(change)),
                help,
                tooltip::Position::Bottom,
            )
            .into()
        };
        let mut tools = row![
            text("STYLE").size(10).color(muted()),
            combo_box(
                &self.font_options,
                "Search fonts…",
                Some(&style.family),
                |family| Message::TextStyle(StyleChange::Family(family))
            )
            .width(152)
            .size(12)
            .padding(6),
            hover_hint(
                text_input("pt", &self.font_size_input)
                    .on_input(Message::FontSize)
                    .on_submit(Message::ApplyFontSize)
                    .width(46)
                    .size(12)
                    .padding(6),
                "Font size in points · Enter to apply",
                tooltip::Position::Bottom
            ),
            text("pt").size(11).color(muted()),
            divider(),
            toggle("B", "Bold", style.bold, StyleChange::Bold(!style.bold)),
            toggle(
                "I",
                "Italic",
                style.italic,
                StyleChange::Italic(!style.italic)
            ),
            toggle(
                "U",
                "Underline",
                style.underline,
                StyleChange::Underline(!style.underline)
            ),
            divider(),
            toggle(
                "CH₂",
                "Chemical formula formatting",
                style.formula,
                StyleChange::Formula(!style.formula)
            ),
            toggle(
                "x₂",
                "Subscript",
                style.script == Script::Subscript,
                StyleChange::Script(if style.script == Script::Subscript {
                    Script::Normal
                } else {
                    Script::Subscript
                })
            ),
            toggle(
                "x²",
                "Superscript",
                style.script == Script::Superscript,
                StyleChange::Script(if style.script == Script::Superscript {
                    Script::Normal
                } else {
                    Script::Superscript
                })
            ),
            divider(),
        ]
        .spacing(4)
        .align_y(Alignment::Center);
        let group_alignment = self.selected_group_alignment();
        let active_alignment = self.toolbar_alignment();
        let has_captions = self
            .doc
            .annotations
            .iter()
            .any(|a| self.selected.contains(&a.id));
        for (label, align) in [
            ("Left", TextAlign::Left),
            ("Center", TextAlign::Center),
            ("Right", TextAlign::Right),
            ("Justify", TextAlign::Justified),
        ] {
            if align == TextAlign::Justified && group_alignment.is_some() && !has_captions {
                continue;
            }
            tools = tools.push(hover_hint(
                button(
                    iced::widget::canvas(Glyph(Icon::TextAlign(align), true))
                        .width(24)
                        .height(24),
                )
                .padding(6)
                .style(control(active_alignment == Some(align)))
                .on_press(Message::TextAlign(align)),
                label,
                tooltip::Position::Bottom,
            ));
        }
        if let Some(alignment) = group_alignment {
            use reshiki::abbreviations::LabelAlignment;
            tools = tools.push(hover_hint(
                pick_list(
                    [LabelAlignment::Auto, LabelAlignment::Above],
                    alignment.filter(|a| matches!(a, LabelAlignment::Auto | LabelAlignment::Above)),
                    Message::GroupLabelAlign,
                )
                .placeholder(if alignment.is_none() { "Mixed" } else { "Auto / above" })
                .width(112)
                .text_size(11)
                .padding(6),
                "Group labels: Automatic follows bonds; Stacked above places the nickname above its attachment. Left, Center and Right use the adjacent buttons.",
                tooltip::Position::Bottom,
            ));
        }
        tools = tools
            .push(divider())
            .push(text("Color").size(11).color(muted()))
            .push(
                pick_list(
                    super::typography::ColorScope::ALL,
                    Some(self.color_scope),
                    Message::ColorScope,
                )
                .width(112)
                .text_size(11)
                .padding(6),
            );
        for (name, c) in [
            ("Black", [0, 0, 0]),
            ("Blue", [32, 80, 145]),
            ("Teal", [17, 126, 108]),
            ("Red", [180, 50, 55]),
            ("Purple", [116, 65, 147]),
        ] {
            let active = self.current_selection_color() == Some(c);
            tools = tools.push(hover_hint(
                button(Space::new().width(12).height(12))
                    .padding(4)
                    .style(move |_, _| button::Style {
                        background: Some(Color::from_rgb8(c[0], c[1], c[2]).into()),
                        border: Border {
                            color: if active {
                                Color::from_rgb8(132, 166, 159)
                            } else {
                                Color::WHITE
                            },
                            width: if active { 3.0 } else { 1.0 },
                            radius: 5.0.into(),
                        },
                        ..Default::default()
                    })
                    .on_press(Message::TextStyle(StyleChange::Color(c))),
                format!("{name} · Apply to {}", self.color_scope),
                tooltip::Position::Bottom,
            ));
        }
        tools = tools.push(hover_hint(
            text_input("Mixed / hex", &self.text_color_input)
                .on_input(Message::TextColor)
                .on_submit(Message::ApplyTextColor)
                .width(76)
                .size(11)
                .padding(6),
            format!("Custom color · Enter to apply to {}", self.color_scope),
            tooltip::Position::Bottom,
        ));
        tools = tools.push(command(
            "Atoms…",
            Message::InspectorAction(super::inspector::Action::OpenAtomColors),
        ));
        container(tools)
            .padding([7, 14])
            .width(Length::Fill)
            .style(panel)
            .into()
    }

    pub(super) fn text_panel(&self) -> Element<'_, Message> {
        if self.inline_text.is_some() {
            return column![
                section("EDITING ON CANVAS"),
                text("Select text in the canvas editor, then use the Style toolbar to format it.")
                    .size(12)
                    .color(muted()),
                row![
                    command(
                        "Cancel",
                        Message::InlineText(super::inline_text::Action::Finish(false))
                    ),
                    command(
                        "Done",
                        Message::InlineText(super::inline_text::Action::Finish(true))
                    )
                ]
                .spacing(8),
                row![
                    text("Line spacing").size(11).width(Length::Fill),
                    pick_list(
                        [1.0_f32, 1.2, 1.5, 2.0],
                        Some(self.caption_format.line_spacing),
                        Message::TextSpacing
                    )
                    .text_size(12)
                    .padding(5)
                ],
                row![
                    text("Wrap width (pt)").size(11).width(Length::Fill),
                    text_input("Auto", &self.text_width_input)
                        .on_input(Message::TextWidth)
                        .on_submit(Message::ApplyTextWidth)
                        .size(12)
                        .width(72)
                        .padding(6)
                ]
            ]
            .spacing(10)
            .into();
        }
        if self.tool == Tool::Text {
            return column![
                section("TEXT LABELS"),
                text(
                    "Click the canvas to type a new label, or click an existing label to edit it."
                )
                .size(12)
                .color(muted()),
                text("Use the Style toolbar for fonts, colors and chemical formulas.")
                    .size(11)
                    .color(muted()),
            ]
            .spacing(9)
            .into();
        }
        let selected = self
            .caption_target
            .is_some_and(|id| self.selected.contains(&id));
        column![
            section(if selected { "EDIT TEXT" } else { "NEW TEXT" }),
            text(if selected {
                "Changes appear on the drawing as you type."
            } else {
                "Write a label, choose its style, then click to place."
            })
            .size(11)
            .color(muted()),
            text_editor(&self.caption_editor)
                .on_action(Message::CaptionAction)
                .placeholder("Reaction conditions, labels, notes…")
                .height(116)
                .size(14)
                .padding(10)
                .key_binding(|key| {
                    if matches!(key.status, text_editor::Status::Focused { .. })
                        && key.modifiers.command()
                        && let iced::keyboard::Key::Character(c) = &key.key
                    {
                        let style = self.current_text_style();
                        let message = match c.as_str() {
                            "z" => Some(if key.modifiers.shift() {
                                Message::Redo
                            } else {
                                Message::Undo
                            }),
                            "b" => Some(Message::TextStyle(StyleChange::Bold(!style.bold))),
                            "i" => Some(Message::TextStyle(StyleChange::Italic(!style.italic))),
                            "u" => {
                                Some(Message::TextStyle(StyleChange::Underline(!style.underline)))
                            }
                            _ => None,
                        };
                        message
                            .map(text_editor::Binding::Custom)
                            .or_else(|| text_editor::Binding::from_key_press(key))
                    } else {
                        text_editor::Binding::from_key_press(key)
                    }
                }),
            text("Select part of the text to format it with the Style toolbar.")
                .size(11)
                .color(muted()),
            row![
                text("Line spacing").size(11).width(Length::Fill),
                pick_list(
                    [1.0_f32, 1.2, 1.5, 2.0],
                    Some(self.caption_format.line_spacing),
                    Message::TextSpacing
                )
                .text_size(12)
                .padding(5)
            ]
            .align_y(Alignment::Center),
            row![
                text("Wrap width (pt)").size(11).width(Length::Fill),
                text_input("Auto", &self.text_width_input)
                    .on_input(Message::TextWidth)
                    .on_submit(Message::ApplyTextWidth)
                    .size(12)
                    .width(72)
                    .padding(6)
            ]
            .align_y(Alignment::Center),
        ]
        .spacing(9)
        .into()
    }

    pub(super) fn workspace(&self) -> Element<'_, Message> {
        let mut content = column![self.command_bar(), self.style_bar()];
        if self.import_open {
            content = content.push(self.import_drawer());
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
            joining: self.joining.as_ref().map(|s| &s.prepared),
            hidden_annotation: self.inline_label_id(),
            bond_drawing: self.bond_drawing,
            chain_drawing: self.chain_drawing,
            graphic_constrain: self.toolbar.graphic(self.tool).is_some_and(|p| p.constrain),
            graphic_style: &self.graphic_style,
            orbital_phase: self.orbital_phase,
            phase_flipped: self.phase_flipped,
            attach_symbols: self.attach_symbols,
            arrow_preset: self.arrow_style,
            arrow_style: &self.arrows.style,
            bracket_sides: self.bracket_sides,
            doc: self.display_document(),
            selected: if self.cleanup.is_some() || self.inline_text.is_some() {
                &[]
            } else {
                &self.selected
            },
            tool: if self.cleanup.is_some() {
                Tool::Select
            } else {
                self.tool
            },
            camera: self.camera,
            grid: self.grid,
            guides: self.guides,
            ring_size: self.ring_size,
            aromatic_ring: self.aromatic_ring,
            template_connection: self
                .joining
                .as_ref()
                .map(|s| s.mode)
                .unwrap_or(self.templates.connection),
            template: self
                .joining
                .as_ref()
                .map(|s| (&s.prepared.fragment, s.anchor))
                .or_else(|| {
                    self.templates
                        .library
                        .get(self.template_index)
                        .filter(|_| self.tool == Tool::Template)
                        .map(|t| (&t.document, self.templates.anchor))
                }),
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .into();
        let paper =
            sensor(self.with_context_menu(self.with_inline_text(drawing.map(Message::Canvas))))
                .on_show(Message::Viewport)
                .on_resize(Message::Viewport);
        let context: Element<'_, Message> = if let Some(preview) = &self.cleanup {
            use reshiki::cleanup::Scope;
            let scopes = vec![Scope::SelectedAtoms, Scope::SelectedMolecules];
            let mut bar = column![
                row![
                    text("Cleanup preview").size(13),
                    pick_list(
                        scopes,
                        Some(preview.job.options.scope),
                        Message::CleanupScope
                    )
                    .text_size(12)
                    .width(160),
                    checkbox(preview.original)
                        .label("Show original")
                        .text_size(12)
                        .size(14)
                        .on_toggle(Message::CleanupOriginal),
                    Space::new().width(Length::Fill),
                    command("Cancel", Message::CancelCleanup),
                    button(text("Apply").size(12))
                        .on_press_maybe((!self.busy).then_some(Message::ApplyCleanup))
                        .style(button::primary),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
                row![
                    checkbox(preview.job.options.keep_orientation)
                        .label("Keep orientation")
                        .text_size(11)
                        .size(13)
                        .on_toggle(Message::CleanupOrientation),
                    text(preview.job.options.scope.hint())
                        .size(11)
                        .color(muted()),
                ]
                .spacing(16)
                .align_y(Alignment::Center),
            ]
            .spacing(6);
            for warning in &preview.warnings {
                bar = bar.push(text(warning).size(11).color(muted()));
            }
            container(bar).padding([8, 12]).style(panel).into()
        } else {
            self.context_bar()
        };
        let workspace = column![
            context,
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
        content = content.push(body);
        if self.view_open {
            content = content.push(self.view_options());
        }
        content.push(self.status_bar()).height(Length::Fill).into()
    }

    fn command_bar(&self) -> Element<'_, Message> {
        let title = self
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".into());
        let bar = row![
            hover_hint(
                button(crate::branding::wordmark(21.0))
                    .padding(0)
                    .style(button::text)
                    .on_press(Message::Updates(super::updates::Action::Show(true))),
                "About ReShiki · Check for updates",
                tooltip::Position::Bottom
            ),
            divider(),
            icon_button(
                Icon::New,
                super::platform_shortcut("New · ⌘N", "New · Ctrl+N"),
                Some(Message::New),
                false
            ),
            icon_button(
                Icon::Open,
                super::platform_shortcut("Open · ⌘O", "Open · Ctrl+O"),
                Some(Message::Open),
                false
            ),
            icon_button(
                Icon::Save,
                super::platform_shortcut("Save · ⌘S", "Save · Ctrl+S"),
                Some(Message::Save),
                false
            ),
            command("Save as", Message::SaveAs),
            divider(),
            icon_button(
                Icon::Undo,
                super::platform_shortcut("Undo · ⌘Z", "Undo · Ctrl+Z"),
                self.text_history_available(false)
                    .unwrap_or_else(|| self.history.can_undo())
                    .then_some(Message::Undo),
                false
            ),
            icon_button(
                Icon::Redo,
                super::platform_shortcut("Redo · ⇧⌘Z", "Redo · Ctrl+Shift+Z"),
                self.text_history_available(true)
                    .unwrap_or_else(|| self.history.can_redo())
                    .then_some(Message::Redo),
                false
            ),
            Space::new().width(8),
            column![
                text(title).size(12),
                text(if self.dirty() {
                    "Edited"
                } else if self.office_document() {
                    "Office drawing · Ctrl+S updates Office"
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
            command(
                if self.assistant.busy {
                    "● Assistant · Working"
                } else {
                    "Assistant"
                },
                Message::Assistant(super::assistant::Action::Open)
            ),
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
            command("Clean up…", Message::Clean)
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
        use reshiki::graphics::GraphicKind as G;
        let tools = [
            (Tool::Select, "Select / move · V"),
            (Tool::Lasso, "Lasso select · L"),
            (
                Tool::Tilt,
                "3D tilt · Drag a ring or selection · Shift snaps to 15°",
            ),
            (Tool::Erase, "Eraser · E · Drag to erase"),
            (Tool::Atom, "Atom label · C, N, O…"),
            (Tool::Bond(1), "Single bond · B / 1"),
            (Tool::Bond(2), "Double bond · 2"),
            (Tool::Bond(3), "Triple bond · 3"),
            (self.toolbar.bond, "Other bonds"),
            (self.toolbar.ring, "Rings · R / Aromatic · Shift+R"),
            (
                Tool::Chain(reshiki::chains::ChainMode::Straight),
                "Straight chain · X",
            ),
            (
                Tool::Chain(reshiki::chains::ChainMode::Snaking),
                "Snaking chain · Shift+X",
            ),
            (Tool::Arrow, "Reaction & electron-flow arrows · A"),
            (Tool::Text, "Text label · T"),
            (Tool::Graphic(self.toolbar.rectangle.kind), "Rectangles"),
            (
                Tool::Graphic(self.toolbar.ellipse.kind),
                "Ellipses / circles",
            ),
            (
                Tool::Graphic(self.toolbar.bracket.kind),
                "Brackets / parentheses / braces",
            ),
            (Tool::Graphic(G::Line), "Graphic line"),
            (Tool::Graphic(G::Curve), "Bézier curve"),
            (Tool::Graphic(G::Arc), "Arc"),
            (self.toolbar.symbol, "Chemical symbols"),
            (self.toolbar.orbital, "Orbitals"),
        ];
        let mut palette = column![section("TOOLS")]
            .spacing(6)
            .align_x(Alignment::Center);
        for pair in tools.chunks(2) {
            let mut line = row![].spacing(4);
            for (tool, hint) in pair {
                let family = super::palettes::family(*tool);
                let icon = if *tool == Tool::Ring {
                    Icon::Ring(self.ring_size, self.aromatic_ring)
                } else if *tool == Tool::Arrow {
                    Icon::Arrow(self.arrow_style)
                } else {
                    Icon::Tool(*tool)
                };
                // Vector tools can share a renderer layer. A separate clipped
                // layer per icon adds GPU passes to every canvas redraw.
                let item = iced::widget::canvas(super::tool_button::ToolButton {
                    tool: *tool,
                    icon,
                    active: self.tool == *tool,
                    opens_on_click: family == Some(super::palettes::Family::Bonds),
                })
                .width(36)
                .height(36);
                let hint = if family == Some(super::palettes::Family::Bonds) {
                    format!("{hint} · Click for styles")
                } else if family.is_some() {
                    format!("{hint} · Hold or click the corner for options")
                } else {
                    (*hint).to_owned()
                };
                let item: Element<'_, Message> = if self.palette.is_some() {
                    item.into()
                } else {
                    hover_hint(item, hint, tooltip::Position::Right).into()
                };
                line = line.push(item);
            }
            palette = palette.push(line);
        }
        palette = palette.push(Space::new().height(10)).push(section("ATOMS"));
        for pair in [
            ["C", "N"],
            ["O", "S"],
            ["P", "F"],
            ["Cl", "Br"],
            ["Fe", "*"],
        ] {
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
        let palette = column![
            scrollable(palette)
                .height(Length::Fill)
                .direction(scrollable::Direction::Vertical(
                    scrollable::Scrollbar::new()
                        .width(3)
                        .scroller_width(3)
                        .spacing(3)
                )),
            hover_hint(
                button(
                    column![
                        iced::widget::canvas(Glyph(Icon::Keyboard, true))
                            .width(24)
                            .height(24),
                        text("Shortcuts").size(10),
                    ]
                    .spacing(3)
                    .align_x(Alignment::Center)
                )
                .padding([5, 10])
                .on_press(Message::ToggleHelp)
                .style(control(self.help_open)),
                "Keyboard shortcuts (?)",
                tooltip::Position::Right,
            )
        ]
        .spacing(8)
        .align_x(Alignment::Center);
        container(palette)
            .width(104)
            .height(Length::Fill)
            .padding([14, 8])
            .style(panel)
            .into()
    }

    fn bond_constraints(&self) -> Element<'_, Message> {
        row![
            checkbox(self.bond_drawing.fixed_length)
                .label("Length")
                .on_toggle(Message::FixedLength)
                .size(13)
                .text_size(11),
            text_input("14.4", &self.drawing_length_input)
                .on_input(Message::DrawingLength)
                .width(48)
                .size(12)
                .padding(5),
            text("pt").size(11).color(muted()),
            checkbox(self.bond_drawing.fixed_angles)
                .label("Angles")
                .on_toggle(Message::FixedAngles)
                .size(13)
                .text_size(11),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .into()
    }

    fn context_bar(&self) -> Element<'_, Message> {
        if self.joining.is_some() {
            return self.join_bar();
        }
        let mut options = row![text(tool_name(self.tool)).size(12).color(ink()), divider()]
            .spacing(10)
            .align_y(Alignment::Center);
        match self.tool {
            tool if tool.bond_preset().is_some() => {
                options = options
                    .push(
                        pick_list(BondPreset::ALL, tool.bond_preset(), |preset| {
                            Message::Tool(match preset {
                                BondPreset::Single => Tool::Bond(1),
                                BondPreset::Double => Tool::Bond(2),
                                BondPreset::Triple => Tool::Bond(3),
                                BondPreset::Wedge => Tool::Wedge,
                                BondPreset::HashedWedge => Tool::Hash,
                                BondPreset::Wavy => Tool::Wavy,
                                other => Tool::StyledBond(other),
                            })
                        })
                        .text_size(12)
                        .padding(5),
                    )
                    .push(self.bond_constraints());
            }
            Tool::Chain(mode) => {
                options = options
                    .push(
                        pick_list(
                            [
                                reshiki::chains::ChainMode::Straight,
                                reshiki::chains::ChainMode::Snaking,
                            ],
                            Some(mode),
                            |m| Message::Tool(Tool::Chain(m)),
                        )
                        .text_size(12)
                        .padding(5),
                    )
                    .push(
                        text(if mode == reshiki::chains::ChainMode::Snaking {
                            "Max atoms"
                        } else {
                            "Atoms"
                        })
                        .size(11),
                    )
                    .push(
                        text_input("Auto", &self.chain_atoms_input)
                            .on_input(Message::ChainAtoms)
                            .width(49)
                            .size(12)
                            .padding(5),
                    )
                    .push(text("Angle").size(11))
                    .push(
                        text_input("120", &self.chain_angle_input)
                            .on_input(Message::ChainAngle)
                            .width(44)
                            .size(12)
                            .padding(5),
                    )
                    .push(text("°").size(11))
                    .push(text("Includes attachment atoms").size(10).color(muted()));
            }
            Tool::Graphic(kind) => {
                let chooser: Element<'_, Message> = match kind {
                    reshiki::graphics::GraphicKind::Symbol(k) => {
                        pick_list(reshiki::scientific::SymbolKind::ALL, Some(k), |k| {
                            Message::ScientificKind(reshiki::graphics::GraphicKind::Symbol(k))
                        })
                        .text_size(12)
                        .padding(5)
                        .into()
                    }
                    reshiki::graphics::GraphicKind::Orbital(k) => {
                        pick_list(reshiki::scientific::OrbitalKind::ALL, Some(k), |k| {
                            Message::ScientificKind(reshiki::graphics::GraphicKind::Orbital(k))
                        })
                        .text_size(12)
                        .padding(5)
                        .into()
                    }
                    _ => pick_list(
                        reshiki::graphics::GraphicKind::DRAWABLE,
                        Some(kind),
                        |kind| Message::Tool(Tool::Graphic(kind)),
                    )
                    .text_size(12)
                    .padding(5)
                    .into(),
                };
                options = options.push(chooser).push(
                    text(match kind {
                        reshiki::graphics::GraphicKind::Symbol(_) => {
                            "Click to place/attach · Drag to position · Escape cancels"
                        }
                        reshiki::graphics::GraphicKind::Orbital(_) => {
                            "Drag from node · Click for default size · Shift snaps to 15°"
                        }
                        _ => "Drag to draw · Shift constrains · Escape cancels",
                    })
                    .size(11)
                    .color(muted()),
                );
            }
            Tool::EditPoints => {
                options = options
                    .push(text("Drag anchors or control points").size(11))
                    .push(command("Done", Message::Tool(Tool::Select)));
            }
            Tool::Template => {
                options = options
                    .push(
                        text(
                            self.templates
                                .library
                                .get(self.template_index)
                                .map(|t| t.name.as_str())
                                .unwrap_or("Template"),
                        )
                        .size(12),
                    )
                    .push(
                        text("Click to place / attach · Drag to orient")
                            .size(11)
                            .color(muted()),
                    )
                    .push(command("Cancel", Message::Tool(Tool::Select)));
            }
            Tool::Ring | Tool::RingPreset(_) => {
                let preset = if let Tool::RingPreset(p) = self.tool {
                    p
                } else {
                    reshiki::rings::Preset::Regular
                };
                options = options.push(
                    pick_list(reshiki::rings::Preset::ALL, Some(preset), |p| {
                        Message::Tool(if p == reshiki::rings::Preset::Regular {
                            Tool::Ring
                        } else {
                            Tool::RingPreset(p)
                        })
                    })
                    .text_size(12)
                    .padding(5),
                );
                if preset == reshiki::rings::Preset::Regular {
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
                                .label("Aromatic · Shift+R")
                                .on_toggle(Message::AromaticRing)
                                .size(14)
                                .text_size(12),
                        )
                        .push(
                            text("Click / drag to attach · Shift+R keeps ring size")
                                .size(11)
                                .color(muted()),
                        );
                } else {
                    options = options.push(
                        text(if preset == reshiki::rings::Preset::Cyclopentadiene {
                            "Click / drag · Alt connects · Shift swaps double bonds"
                        } else {
                            "Click / drag · Alt connects by a bond"
                        })
                        .size(11)
                        .color(muted()),
                    );
                }
            }
            Tool::Arrow => {
                options = options
                    .push(
                        pick_list(
                            reshiki::arrows::Preset::ALL,
                            Some(self.arrow_style),
                            Message::ArrowStyle,
                        )
                        .text_size(12)
                        .padding(5),
                    )
                    .push(
                        text("Click to place / change · Click again to switch · Drag to draw")
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
                options = options.push(
                    text("Click an atom to name it · Click empty space for a caption · Escape cancels")
                        .size(11)
                        .color(muted()),
                );
            }
            Tool::Tilt => {
                options = options.spacing(4);
                let enabled = crate::canvas::tilt::available(&self.doc, &self.selected);
                for (label, transform) in [
                    ("X −15°", reshiki::editing::Transform::TiltX(-15.)),
                    ("X +15°", reshiki::editing::Transform::TiltX(15.)),
                    ("Y −15°", reshiki::editing::Transform::TiltY(-15.)),
                    ("Y +15°", reshiki::editing::Transform::TiltY(15.)),
                ] {
                    options = options.push(
                        command(label, Message::Transform(transform))
                            .on_press_maybe(enabled.then_some(Message::Transform(transform))),
                    );
                }
                options = options
                    .push(hover_hint(
                        command(
                            "Front bonds",
                            Message::InspectorAction(super::inspector::Action::DepthBonds),
                        )
                        .on_press_maybe(enabled.then_some(
                            Message::InspectorAction(super::inspector::Action::DepthBonds),
                        )),
                        "Emphasize front bonds using the retained projection depth",
                        tooltip::Position::Bottom,
                    ))
                    .push(text("Drag to tilt · Shift: 15°").size(11).color(muted()))
                    .push(command("Done", Message::Tool(Tool::Select)));
            }
            Tool::Select | Tool::Lasso if !self.selected.is_empty() => {
                options = options
                    .push(hover_hint(
                        text(self.selection_summary()).size(11).color(muted()),
                        "Click a bond's middle to select it; Shift-click adds. Cmd/Ctrl+A selects the whole drawing.",
                        tooltip::Position::Bottom,
                    ))
                    .push(command("Cut", Message::Copy(true)))
                    .push(command("Copy", Message::Copy(false)))
                    .push(command("Paste", Message::Paste))
                    .push(command("Duplicate", Message::Duplicate))
                    .push(command("Move & attach…", Message::Join(super::joining::Action::Begin))
                        .on_press_maybe(self.selected.iter().any(|id| self.doc.atom(*id).is_some()).then_some(Message::Join(super::joining::Action::Begin))))
                    .push(
                        command("Group", Message::Group)
                            .on_press_maybe(self.can_group().then_some(Message::Group)),
                    );
                if !self.doc.outer_selected_groups(&self.selected).is_empty() {
                    options = options.push(command("Ungroup", Message::Ungroup));
                }
            }
            Tool::Select | Tool::Lasso => {
                options = options.push(
                    text(if self.tool == Tool::Lasso {
                        "Draw around objects · Shift adds · Option drag removes"
                    } else {
                        "Double-click selects molecule · Shift-click adds"
                    })
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
        options = options.push(hover_hint(
            button(text(&self.doc.drawing_style.name).size(10))
                .padding([5, 8])
                .style(control(self.inspector_tab == InspectorTab::DrawingStyle))
                .on_press(Message::DrawingStyle(super::document_styles::Action::Open)),
            "Edit document fonts, bond dimensions and publication style",
            tooltip::Position::Bottom,
        ));
        let moving_bonded_selection = matches!(self.tool, Tool::Select | Tool::Lasso) && {
            let selected = self.doc.expand_abbreviation_selection(&self.selected);
            self.doc
                .bonds
                .iter()
                .any(|bond| selected.contains(&bond.a) != selected.contains(&bond.b))
        };
        if (matches!(self.tool, Tool::Chain(_))
            || self.tool.bond_preset().is_some()
            || moving_bonded_selection)
            && ((self.bond_drawing.length - self.doc.drawing_style.bond_length_world).abs() > 0.001
                || self.chain_drawing.angle != 120.
                || !self.bond_drawing.fixed_length
                || !self.bond_drawing.fixed_angles)
        {
            options = options.push(hover_hint(
                command("Reset bonds", Message::ResetBondDrawing),
                "Restore this document's bond length and drawing constraints",
                tooltip::Position::Bottom,
            ));
        }
        if matches!(self.tool, Tool::Chain(_)) || moving_bonded_selection {
            return container(
                column![
                    options,
                    row![
                        self.bond_constraints(),
                        text(if moving_bonded_selection {
                            "Bonded movement follows Length / Angles · Option/Alt: free movement"
                        } else {
                            "Ctrl bends · Shift flips start · Alt frees · Auto click: 6 atoms"
                        })
                        .size(10)
                        .color(muted()),
                    ]
                    .spacing(14)
                    .align_y(Alignment::Center)
                ]
                .spacing(7),
            )
            .padding([7, 14])
            .style(panel)
            .into();
        }
        container(options)
            .height(46)
            .padding([5, 14])
            .center_y(46)
            .style(panel)
            .into()
    }

    fn inspector(&self) -> Element<'_, Message> {
        if self.inspector_tab == InspectorTab::Reactions {
            return self.reactions_inspector();
        }
        if self.inspector_tab == InspectorTab::DrawingStyle {
            return container(self.drawing_style_panel())
                .width(320)
                .height(Length::Fill)
                .style(panel)
                .into();
        }
        if self.inspector_tab == InspectorTab::Assistant {
            return container(self.assistant_panel())
                .width(380)
                .height(Length::Fill)
                .style(panel)
                .into();
        }
        let mut tabs = row![].spacing(2);
        for (label, tab) in [
            ("Properties", InspectorTab::Properties),
            ("Templates", InspectorTab::Templates),
            ("Export", InspectorTab::Export),
        ] {
            tabs = tabs.push(
                button(text(label).size(11))
                    .padding([7, 8])
                    .style(control(
                        self.inspector_tab == tab
                            || (tab == InspectorTab::Properties
                                && matches!(
                                    self.inspector_tab,
                                    InspectorTab::Reactions
                                        | InspectorTab::Labels
                                        | InspectorTab::Abbreviations
                                        | InspectorTab::Pages
                                        | InspectorTab::DrawingStyle
                                )),
                    ))
                    .on_press(Message::Inspector(tab)),
            );
        }
        let body = match self.inspector_tab {
            InspectorTab::Reactions => self.reactions_panel(),
            InspectorTab::Assistant => self.assistant_panel(),
            InspectorTab::Pages => self.pages_panel(),
            InspectorTab::DrawingStyle => self.drawing_style_panel(),
            InspectorTab::Properties if self.joining.is_some() => self.join_panel(),
            InspectorTab::Properties => self.properties_panel(),
            InspectorTab::Labels => self.atom_labels_panel(),
            InspectorTab::Abbreviations => self.abbreviations_panel(),
            InspectorTab::Templates => self.templates_panel(),
            InspectorTab::Export => self.export_panel(),
        };
        container(
            column![
                container(tabs).padding([8, 8]),
                scrollable(container(body).padding([8, 16]))
                    .id("inspector-content")
                    .on_scroll(|viewport| Message::InspectorScroll(viewport.absolute_offset().y))
                    .height(Length::Fill)
            ]
            .spacing(4),
        )
        .width(
            if matches!(
                self.inspector_tab,
                InspectorTab::DrawingStyle | InspectorTab::Reactions
            ) {
                320
            } else if matches!(
                self.inspector_tab,
                InspectorTab::Properties | InspectorTab::Export
            ) {
                300
            } else {
                256
            },
        )
        .height(Length::Fill)
        .style(panel)
        .into()
    }

    fn templates_panel(&self) -> Element<'_, Message> {
        use super::template_library::{Action as A, Filter};
        let action = Message::Templates;
        let state = &self.templates;
        let mut body = column![section("TEMPLATE LIBRARY")].spacing(8);
        if !state.active {
            body = body.push(
                column![
                    text_input("Search names, collections…", &state.query)
                        .on_input(|s| Message::Templates(A::Search(s)))
                        .size(12)
                        .padding(8),
                    pick_list(
                        [Filter::All, Filter::Mine, Filter::Favorites],
                        Some(state.filter),
                        |f| Message::Templates(A::Filter(f))
                    )
                    .text_size(12)
                    .padding(6)
                    .width(Length::Fill),
                ]
                .spacing(8),
            );
            if state.collection != "All collections"
                || !state.query.is_empty()
                || state.filter != Filter::All
            {
                body = body.push(command("← Categories", action(A::Browse)));
            }
            if state.can_forward() {
                body = body.push(command("Forward →", action(A::Forward)));
            }
            body = body.push(
                button(text("Save selection as template").size(12))
                    .padding(7)
                    .width(Length::Fill)
                    .on_press_maybe((!self.selected.is_empty()).then(|| action(A::BeginSave))),
            );
            body = body.push(
                row![
                    command("Import…", action(A::Import)),
                    command("Export…", action(A::Export)),
                    command("Reload", action(A::Reload))
                ]
                .spacing(3),
            );
        } else {
            body = body.push(command("← Browse templates", action(A::Browse)));
        }
        if let Some(error) = &state.notice {
            body = body.push(text(error).size(11).color(Color::from_rgb8(182, 66, 61)));
        }
        if state.editing {
            body = body
                .push(section(if state.draft.is_some() {
                    "NEW TEMPLATE"
                } else {
                    "EDIT TEMPLATE"
                }))
                .push(
                    text_input("Template name", &state.name)
                        .on_input(|s| Message::Templates(A::Name(s)))
                        .size(12)
                        .padding(7),
                )
                .push(
                    text_input("Collection", &state.category)
                        .on_input(|s| Message::Templates(A::Category(s)))
                        .size(12)
                        .padding(7),
                )
                .push(
                    row![
                        command("Save", action(A::SaveDetails)).style(button::primary),
                        command("Cancel", action(A::CancelDetails))
                    ]
                    .spacing(6),
                );
        }
        if state.active
            && let Some(t) = state.library.get(self.template_index)
        {
            let preview: Element<'_, reshiki::templates::Anchor> =
                canvas(crate::canvas::TemplateAnchorPreview {
                    document: &t.document,
                    anchor: state.anchor,
                })
                .width(Length::Fill)
                .height(145)
                .into();
            body = body
                .push(horizontal_line())
                .push(text(&t.name).size(14))
                .push(
                    pick_list(
                        [
                            reshiki::templates::Connection::Connect,
                            reshiki::templates::Connection::ShareAtom,
                            reshiki::templates::Connection::FuseBond,
                        ],
                        Some(state.connection),
                        |mode| Message::Templates(A::Connection(mode)),
                    )
                    .text_size(12)
                    .width(Length::Fill),
                )
                .push(preview.map(|a| Message::Templates(A::Anchor(a))))
                .push(text(state.connection.hint()).size(11).color(muted()))
                .push(
                    row![
                        text(state.anchor.to_string()).size(11).width(Length::Fill),
                        command("Auto", action(A::Anchor(reshiki::templates::Anchor::Auto)))
                    ]
                    .align_y(Alignment::Center),
                )
                .push(
                    row![
                        command("Place", Message::InsertTemplate(self.template_index)),
                        command(
                            if state.library.favorite(&t.id) {
                                "★ Saved"
                            } else {
                                "☆ Favorite"
                            },
                            action(A::Favorite(self.template_index))
                        )
                    ]
                    .spacing(6),
                )
                .push(
                    checkbox(state.repeat)
                        .label("Keep placing")
                        .text_size(12)
                        .size(14)
                        .on_toggle(|v| Message::Templates(A::Repeat(v))),
                )
                .push(
                    text(if state.repeat {
                        "Click again to add another copy. Escape finishes placement."
                    } else {
                        "Returns to Select after one placement."
                    })
                    .size(11)
                    .color(muted()),
                );
            if !t.note.is_empty() {
                body = body.push(text(&t.note).size(11).color(muted()));
            }
            if self.template_index >= reshiki::templates::LIBRARY.len() {
                body = body
                    .push(
                        row![
                            command("Edit", action(A::EditDetails)),
                            command("Remember anchor", action(A::RememberAnchor))
                        ]
                        .spacing(5),
                    )
                    .push(
                        button(text("Replace from selection").size(11))
                            .padding(6)
                            .on_press_maybe(
                                (!self.selected.is_empty()).then(|| action(A::Replace)),
                            ),
                    )
                    .push(command("Remove template", action(A::Remove)));
            }
        }
        if state.undo.is_some() {
            body = body.push(command("Undo library change", action(A::Restore)));
        }
        if !state.active && !state.editing {
            if state.collection == "All collections"
                && state.query.trim().is_empty()
                && state.filter == Filter::All
            {
                let mut categories = std::collections::BTreeMap::<
                    &str,
                    (usize, &reshiki::templates::Template),
                >::new();
                for template in state.library.iter() {
                    let name = super::template_library::category(template);
                    let entry = categories.entry(name).or_insert((0, template));
                    entry.0 += 1;
                }
                body = body
                    .push(horizontal_line())
                    .push(text("CATEGORIES").size(10).color(muted()));
                for (name, (count, sample)) in categories {
                    let preview: Element<'_, Message> =
                        canvas(crate::canvas::TemplateThumbnail(&sample.document))
                            .width(26)
                            .height(26)
                            .into();
                    body = body.push(
                        button(
                            row![
                                preview,
                                text(name).size(12).width(Length::Fill),
                                text(count.to_string()).size(11).color(muted()),
                                text("›").size(15)
                            ]
                            .spacing(7)
                            .align_y(Alignment::Center),
                        )
                        .padding([3, 6])
                        .width(Length::Fill)
                        .style(button::text)
                        .on_press(action(A::Collection(name.into()))),
                    );
                }
                return body.into();
            }
            let mut matches: Vec<_> = state
                .library
                .iter()
                .enumerate()
                .filter(|(i, t)| state.matches(*i, t))
                .collect();
            matches.sort_by_key(|(_, t)| state.search_rank(t));
            let empty = matches.is_empty();
            body = body.push(horizontal_line()).push(
                text(format!(
                    "{} · {} templates",
                    if state.collection == "All collections" {
                        "Results"
                    } else {
                        &state.collection
                    },
                    matches.len()
                ))
                .size(11)
                .color(muted()),
            );
            for tiles in matches.chunks(4) {
                let mut line = row![].spacing(4);
                for (index, template) in tiles {
                    let preview: Element<'_, Message> =
                        canvas(crate::canvas::TemplateThumbnail(&template.document))
                            .width(44)
                            .height(44)
                            .into();
                    line = line.push(hover_hint(
                        button(preview)
                            .padding(4)
                            .width(52)
                            .height(52)
                            .style(control(state.active && self.template_index == *index))
                            .on_press(Message::InsertTemplate(*index)),
                        template.name.as_str(),
                        tooltip::Position::Left,
                    ));
                }
                body = body.push(line);
            }
            if empty {
                body = body.push(
                    text("No matching templates. Try another search or save a selection.")
                        .size(12)
                        .color(muted()),
                );
            }
        }
        body.into()
    }

    fn abbreviations_panel(&self) -> Element<'_, Message> {
        use super::abbreviations::Action as A;
        let action = Message::Abbreviations;
        let selected: Vec<_> = self
            .doc
            .abbreviations
            .iter()
            .filter(|g| g.members.iter().any(|id| self.selected.contains(id)))
            .collect();
        let choices: Vec<String> = reshiki::abbreviations::PRESETS
            .iter()
            .chain(reshiki::ligands::LABELS)
            .map(|s| (*s).into())
            .collect();
        let mut body = column![
            command("‹ Properties", Message::Inspector(InspectorTab::Properties)),
            text("Chemical abbreviations").size(18),
            text("Compact labels with the complete molecule inside.").size(12).color(muted()),
            row![command("Expand selected", action(A::Expand)).on_press_maybe((!selected.is_empty()).then_some(action(A::Expand))), command("Expand all", action(A::ExpandAll)).on_press_maybe((!self.doc.abbreviations.is_empty()).then_some(action(A::ExpandAll)))].spacing(6),
            horizontal_line(),
            text("COMMON GROUP").size(11).color(muted()),
            pick_list(choices, Some(self.abbreviations.preset.clone()), move |s| action(A::Preset(s))).width(Length::Fill).text_size(14),
            command("Replace selected endpoint", action(A::Replace)).on_press_maybe((!self.busy && !self.selected.is_empty()).then_some(action(A::Replace))),
            text("Select one terminal atom or an existing abbreviation. Its connecting bond stays in place.").size(11).color(muted()),
            horizontal_line(),
            command("Contract common groups", action(A::Find)).on_press_maybe((!self.busy && !self.doc.atoms.is_empty()).then_some(action(A::Find))),
            text(if self.selected.is_empty() { "Searches the whole drawing." } else { "Only complete groups within the selection are contracted." }).size(11).color(muted()),
            horizontal_line(),
            text("NAME A SELECTED FRAGMENT").size(11).color(muted()),
            text_input("Label, e.g. Ar", &self.abbreviations.label).on_input(move |s| action(A::Label(s))).on_submit(action(A::Contract)).size(13),
            text_input("From the right (optional)", &self.abbreviations.reverse_label).on_input(move |s| action(A::ReverseLabel(s))).on_submit(action(A::Contract)).size(13),
            command("Contract selection", action(A::Contract)).on_press_maybe((!self.selected.is_empty() && !self.abbreviations.label.trim().is_empty()).then_some(action(A::Contract))),
            text("Select a connected fragment with at most one outside bond. A custom name does not change its chemistry.").size(11).color(muted()),
            horizontal_line(),
        ].spacing(10);
        for group in selected {
            let atoms = group
                .members
                .iter()
                .filter(|id| self.doc.atom(**id).is_some_and(|a| a.element != "*"))
                .count();
            body = body.push(
                text(format!(
                    "{} · {} atom{}",
                    group.label,
                    atoms,
                    if atoms == 1 { "" } else { "s" }
                ))
                .size(12)
                .color(Color::from_rgb8(17, 126, 108)),
            );
        }
        body.into()
    }

    fn atom_labels_panel(&self) -> Element<'_, Message> {
        use super::atom_labels::{Action as A, Scope};
        use reshiki::atom_labels::{Carbons, HydrogenPosition};
        let ids = self.label_ids();
        let atoms: Vec<_> = self
            .doc
            .atoms
            .iter()
            .filter(|a| ids.contains(&a.id))
            .collect();
        let carbon_values: Vec<_> = atoms
            .iter()
            .map(|a| a.display.carbons.unwrap_or(self.doc.atom_labels.carbons))
            .collect();
        let carbons = if atoms.is_empty() {
            Some(self.doc.atom_labels.carbons)
        } else {
            carbon_values
                .first()
                .copied()
                .filter(|c| carbon_values.iter().all(|v| v == c))
        };
        let hydrogens = if atoms.is_empty() {
            self.doc.atom_labels.hydrogens
        } else {
            atoms
                .iter()
                .all(|a| reshiki::atom_labels::hydrogens(a, &self.doc))
        };
        let stereo = if atoms.is_empty() {
            self.doc.atom_labels.stereo
        } else {
            atoms
                .iter()
                .all(|a| a.display.stereo.show.unwrap_or(self.doc.atom_labels.stereo))
        };
        let position = atoms
            .first()
            .map(|a| a.display.hydrogen_position)
            .filter(|p| atoms.iter().all(|a| a.display.hydrogen_position == *p));
        let mut body = column![
            command(
                "← Structure properties",
                Message::Inspector(InspectorTab::Properties)
            ),
            section("ATOM LABELS"),
            pick_list(
                [Scope::Drawing, Scope::Selection],
                Some(self.labels.scope),
                |s| Message::Labels(A::Scope(s))
            )
            .text_size(12)
            .width(Length::Fill),
            text(format!("{} atoms in scope", ids.len()))
                .size(11)
                .color(muted()),
            text("Carbon labels").size(12),
            pick_list(Carbons::ALL, carbons, |v| Message::Labels(A::Carbons(v)))
                .placeholder("Mixed")
                .text_size(12)
                .width(Length::Fill),
            checkbox(hydrogens)
                .label("Show implied hydrogens")
                .text_size(12)
                .on_toggle(|v| Message::Labels(A::Hydrogens(v))),
            text("Hydrogen position").size(12),
            pick_list(HydrogenPosition::ALL, position, |v| Message::Labels(
                A::Position(v)
            ))
            .placeholder("Mixed")
            .text_size(12)
            .width(Length::Fill),
            text("Display settings keep the molecular composition intact.")
                .size(11)
                .color(muted()),
            horizontal_line(),
            section("ATOM NUMBERS"),
            row![
                text_input("1, atom1, a, α…", &self.labels.seed)
                    .on_input(|s| Message::Labels(A::Seed(s)))
                    .on_submit(Message::Labels(A::Number))
                    .size(12)
                    .padding(7),
                command("Number", Message::Labels(A::Number))
                    .on_press_maybe((!ids.is_empty()).then_some(Message::Labels(A::Number)))
            ]
            .spacing(5),
            text("Sequence follows atom creation/import order.")
                .size(11)
                .color(muted()),
            command("Clear numbers", Message::Labels(A::ClearNumbers)),
        ]
        .spacing(9);
        if ids.len() == 1 {
            body = body.push(text("Custom atom number").size(12)).push(
                row![
                    text_input("e.g. Cα or 12a", &self.labels.number)
                        .on_input(|s| Message::Labels(A::Text(s)))
                        .on_submit(Message::Labels(A::ApplyText))
                        .size(12)
                        .padding(7),
                    command("Set", Message::Labels(A::ApplyText))
                ]
                .spacing(5),
            );
        }
        body = body
            .push(horizontal_line())
            .push(section("STEREOCHEMISTRY"))
            .push(
                checkbox(stereo)
                    .label("Show R/S and E/Z labels")
                    .text_size(12)
                    .on_toggle(|v| Message::Labels(A::Stereo(v))),
            )
            .push(
                text("Labels update after chemical edits. Unassigned centers have no R/S label.")
                    .size(11)
                    .color(muted()),
            );
        if let Some(error) = &self.chemistry_notice {
            body = body.push(text(error).size(11).color(Color::from_rgb8(182, 66, 61)));
        }
        body.push(horizontal_line())
            .push(section("INDICATOR APPEARANCE"))
            .push(
                row![
                    text_input("Size in pt", &self.labels.size)
                        .on_input(|s| Message::Labels(A::Size(s)))
                        .on_submit(Message::Labels(A::ApplySize))
                        .size(12)
                        .padding(7),
                    text("pt").size(11),
                    command("Apply", Message::Labels(A::ApplySize))
                ]
                .spacing(5)
                .align_y(Alignment::Center),
            )
            .push(command(
                "Use current toolbar text style",
                Message::Labels(A::ToolbarStyle),
            ))
            .push(command(
                "Position numbers & stereo labels",
                Message::Labels(A::PositionIndicators),
            ))
            .push(command(
                "Restore automatic positions",
                Message::Labels(A::ResetPositions),
            ))
            .push(command(
                "Use drawing label defaults",
                Message::Labels(A::ResetOverrides),
            ))
            .into()
    }

    fn import_drawer(&self) -> Element<'_, Message> {
        container(
            column![
                row![
                    text("Import").size(13),
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
                    text_input(
                        "SMILES, reaction SMILES, RXN, InChI, MOL or CDXML",
                        &self.smiles
                    )
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
                    command(
                        "Picture…",
                        Message::Pictures(super::pictures::Action::Import)
                    )
                    .on_press_maybe(
                        self.pictures
                            .active
                            .is_none()
                            .then_some(Message::Pictures(super::pictures::Action::Import))
                    ),
                    text("PNG · JPEG · TIFF · WebP").size(11).color(muted()),
                    command("Paste picture", Message::PastePicture).on_press_maybe(
                        (reshiki::clipboard::available() && !self.clipboard_busy)
                            .then_some(Message::PastePicture)
                    )
                ]
                .spacing(6)
                .align_y(Alignment::Center),
                row![
                    text("Insert example").size(11).color(muted()),
                    command("Ethanol", Message::Example("CCO")),
                    command("Benzene", Message::Example("c1ccccc1")),
                    command("Aspirin", Message::Example("CC(=O)Oc1ccccc1C(=O)O")),
                    command("Caffeine", Message::Example("Cn1c(=O)c2c(ncn2C)n(C)c1=O")),
                    text("Drag to position · Delete or Undo to remove")
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

    fn view_options(&self) -> Element<'_, Message> {
        container(
            row![
                text("View").size(12).color(muted()),
                checkbox(self.grid)
                    .label("Grid")
                    .on_toggle(|_| Message::Grid)
                    .size(14)
                    .text_size(12),
                checkbox(self.guides.rulers)
                    .label("Rulers")
                    .on_toggle(Message::Rulers)
                    .size(14)
                    .text_size(12),
                checkbox(self.guides.crosshair)
                    .label("Crosshair")
                    .on_toggle(Message::Crosshair)
                    .size(14)
                    .text_size(12),
                divider(),
                text("Units").size(12).color(muted()),
                pick_list(
                    crate::canvas::guides::Unit::ALL,
                    Some(self.guides.unit),
                    Message::RulerUnit
                )
                .text_size(12)
                .padding(5)
                .width(68),
                command("Page setup…", Message::Pages(super::pages::Action::Show)),
                Space::new().width(Length::Fill),
                command("Done", Message::ToggleView),
            ]
            .spacing(18)
            .align_y(Alignment::Center),
        )
        .padding([7, 14])
        .style(panel)
        .into()
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
            command(
                if self.updates.available() {
                    "Update available"
                } else {
                    concat!("v", env!("CARGO_PKG_VERSION"))
                },
                Message::Updates(super::updates::Action::Show(true))
            ),
            text(&self.autosave_status).size(10).color(muted()),
            text(self.selection_summary()).size(11).color(muted()),
            divider(),
            command("View", Message::ToggleView).style(control(self.view_open)),
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

// Keep every hover label readable and visually consistent. Menus have their
// own overlays and do not use this wrapper.
pub(super) fn hover_hint<'a>(
    content: impl Into<Element<'a, Message>>,
    label: impl Into<std::borrow::Cow<'a, str>>,
    position: tooltip::Position,
) -> tooltip::Tooltip<'a, Message> {
    tooltip(
        content,
        container(text(label.into()).size(12))
            .max_width(300)
            .padding([7, 10])
            .style(tip),
        position,
    )
    .padding(0)
    .gap(7)
    .delay(std::time::Duration::from_millis(500))
    .snap_within_viewport(true)
}

pub(super) fn command(label: &str, message: Message) -> button::Button<'_, Message> {
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
    icon_button_at(icon, hint, message, active, tooltip::Position::Bottom)
}
fn icon_button_at(
    icon: Icon,
    hint: &'static str,
    message: Option<Message>,
    active: bool,
    position: tooltip::Position,
) -> Element<'static, Message> {
    hover_hint(
        button(
            iced::widget::canvas(Glyph(icon, message.is_some()))
                .width(24)
                .height(24),
        )
        .width(36)
        .height(36)
        .padding(6)
        .style(control(active))
        .on_press_maybe(message),
        hint,
        position,
    )
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
            iced::widget::canvas(Glyph(icon, true)).width(24).height(24),
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
pub(super) fn section(label: &str) -> iced::widget::Text<'_> {
    text(label).size(10).color(muted())
}
fn ink() -> Color {
    Color::from_rgb8(37, 43, 51)
}
pub(super) fn muted() -> Color {
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
pub(super) fn horizontal_line() -> Element<'static, Message> {
    container(
        container(Space::new().height(1).width(Length::Fill)).style(|_| container::Style {
            background: Some(Color::from_rgb8(230, 233, 237).into()),
            ..Default::default()
        }),
    )
    .padding([7, 0])
    .into()
}
pub(super) fn control(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
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
pub(super) fn panel(_: &Theme) -> container::Style {
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
        // Tiny Skia's shadow pass does not clip to the damaged region. During
        // partial redraws it darkens unchanged canvas pixels and is expensive
        // for this large surface; the sheet border already separates it.
        shadow: if cfg!(windows) {
            iced::Shadow::default()
        } else {
            iced::Shadow {
                color: Color::from_rgba8(35, 45, 57, 0.08),
                offset: iced::Vector::new(0., 2.),
                blur_radius: 8.,
            }
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
        Tool::Lasso => "Lasso select",
        Tool::Tilt => "3D tilt",
        Tool::Atom => "Atom label",
        Tool::Bond(1) => "Single bond",
        Tool::Chain(_) => "Chain",
        Tool::Bond(2) => "Double bond",
        Tool::Bond(_) => "Triple bond",
        Tool::StyledBond(preset) => preset.name(),
        Tool::Wedge => "Solid wedge",
        Tool::Hash => "Hashed wedge",
        Tool::Wavy => "Wavy bond",
        Tool::Ring | Tool::RingPreset(_) => "Ring",
        Tool::Template => "Template",
        Tool::Arrow => "Reaction arrow",
        Tool::Text => "Text label",
        Tool::Erase => "Eraser",
        Tool::Graphic(_) => "Drawing object",
        Tool::EditPoints => "Edit points",
    }
}
