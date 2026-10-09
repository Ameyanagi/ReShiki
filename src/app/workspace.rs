use super::{
    App, InspectorTab, Message,
    icons::{Glyph, Icon},
    shortcuts::keys,
};
use crate::canvas::layered::canvas;
use crate::canvas::{Edit, MoleculeCanvas, Tool};
use iced::widget::{
    Space, button, checkbox, column, combo_box, container, mouse_area, responsive, rich_text, row,
    scrollable, sensor, span, text, tooltip,
};
use iced::{Alignment, Border, Color, Element, Length, Theme, keyboard::Modifiers};
use reshiki::bonds::BondPreset;
use reshiki::typography::{Script, StyleChange};

#[cfg(test)]
mod layout_snapshots;
#[cfg(test)]
mod selection_canvas_qa;
#[cfg(test)]
mod view_parity_tests;

impl App {
    fn keyboard_context_summary(&self) -> String {
        use reshiki::keyboard_drawing::Target;
        match self.tab.keyboard_drawing.target() {
            Target::Blank(_) => "Empty position".into(),
            Target::Atom(id) => self.tab.doc.atom(id).map_or_else(
                || "Empty position".into(),
                |atom| format!("{} #{id}", atom.element),
            ),
            Target::Bond(a, b) => format!("Bond {a}–{b}"),
        }
    }

    pub(super) fn selection_summary(&self) -> String {
        let selected: std::collections::HashSet<_> = self.tab.selected.iter().copied().collect();
        let groups = self.tab.doc.outer_selected_groups(&self.tab.selected);
        let covered: std::collections::HashSet<_> = self
            .tab
            .doc
            .groups
            .iter()
            .filter(|g| groups.contains(&g.id))
            .flat_map(|g| &g.members)
            .collect();
        let atoms = self
            .tab
            .doc
            .atoms
            .iter()
            .filter(|a| selected.contains(&a.id))
            .count();
        let points = self
            .tab
            .doc
            .atoms
            .iter()
            .filter(|a| {
                selected.contains(&a.id) && a.element == "*" && a.display.variable.is_none()
            })
            .count();
        let bonds = self
            .tab
            .doc
            .bonds
            .iter()
            .filter(|b| selected.contains(&b.a) && selected.contains(&b.b))
            .count();
        let objects = self.tab.selected.len().saturating_sub(atoms);
        let mut parts = Vec::new();
        let mut add = |count: usize, label: &str| {
            if count > 0 {
                parts.push(format!(
                    "{count} {label}{}",
                    if count == 1 { "" } else { "s" }
                ));
            }
        };
        if !groups.is_empty() && self.tab.selected.iter().all(|id| covered.contains(id)) {
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
        if self.tab.selected.len() <= 1 {
            return false;
        }
        let selected: std::collections::HashSet<_> = self.tab.selected.iter().copied().collect();
        !self.tab.doc.groups.iter().any(|g| {
            g.members.len() == self.tab.selected.len()
                && g.members.iter().all(|id| selected.contains(id))
        })
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
            text("STYLE").size(10).style(muted_text),
            combo_box(
                &self.font_options,
                "Search fonts…",
                Some(&style.family),
                |family| Message::TextStyle(StyleChange::Family(family))
            )
            .width(152)
            .input_style(crate::appearance::input_style)
            .menu_style(crate::appearance::dropdown_menu)
            .size(12)
            .padding(6),
            hover_hint(
                crate::appearance::text_input("pt", &self.tab.font_size_input)
                    .on_input(Message::FontSize)
                    .on_submit(Message::ApplyFontSize)
                    .width(46)
                    .size(12)
                    .padding(6),
                "Font size in points · Enter to apply",
                tooltip::Position::Bottom
            ),
            text("pt").size(11).style(muted_text),
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
        let has_captions = self
            .tab
            .doc
            .annotations
            .iter()
            .any(|a| self.tab.selected.contains(&a.id));
        tools = tools.push(self.alignment_menu(group_alignment.is_some() && !has_captions));
        if let Some(alignment) = group_alignment {
            use reshiki::abbreviations::LabelAlignment;
            tools = tools.push(hover_hint(
                crate::appearance::pick_list(
                    [LabelAlignment::Auto, LabelAlignment::Above],
                    alignment.filter(|a| matches!(a, LabelAlignment::Auto | LabelAlignment::Above)),
                    Message::GroupLabelAlign,
                )
                .placeholder(if alignment.is_none() { "Mixed" } else { "Auto / above" })
                .width(112)
                .text_size(11)
                .padding(6),
                "Group labels: Automatic follows bonds; Stacked above places the nickname above its attachment. Left, Center and Right are in the alignment menu.",
                tooltip::Position::Bottom,
            ));
        }
        tools = tools.push(self.color_button());
        container(tools)
            .padding([7, 14])
            .width(Length::Fill)
            .style(panel)
            .into()
    }

    pub(super) fn workspace(&self) -> Element<'_, Message> {
        if self.inspector_tab == InspectorTab::ThemeGenerator && self.theme_library.editor.is_some()
        {
            return self.theme_generator_workspace();
        }
        let background = |element| {
            if self.context_menu.is_some() {
                reshiki::accessibility::inert(element)
            } else {
                element
            }
        };
        let mut content = column![background(self.command_bar()), background(self.style_bar())];
        let drawing: Element<'_, Edit> = self.drawing_canvas();
        // Keyed by tab, so that switching tabs ends a drag or other gesture.
        let drawing: Element<'_, Edit> = iced::widget::keyed_column([(self.tab.id, drawing)])
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
        let paper = self.with_drop_overlay(
            sensor(self.with_context_menu(self.with_inline_text(drawing.map(Message::Canvas))))
                .on_show(Message::Viewport)
                .on_resize(Message::Viewport)
                .into(),
        );
        let context: Element<'_, Message> = if let Some(preview) = &self.tab.cleanup {
            self.cleanup_bar(preview)
        } else {
            self.context_bar()
        };
        let workspace = column![background(context), paper]
            .height(Length::Fill)
            .width(Length::Fill);
        let mut body = row![background(self.tool_palette()), workspace].height(Length::Fill);
        if self.inspector_open {
            body = body.push(background(self.inspector()));
        }
        content = content.push(body);
        if self.view_open {
            content = content.push(background(self.view_options()));
        }
        content
            .push(background(self.status_bar()))
            .height(Length::Fill)
            .into()
    }

    fn drawing_canvas(&self) -> Element<'_, Edit> {
        canvas(MoleculeCanvas {
            optimizer: self.optimization_canvas(),
            keyboard_target: if self.keyboard_drawing_active() {
                self.tab
                    .keyboard_drawing
                    .marker_point(self.display_document())
            } else {
                None
            },
            element: &self.element,
            joining: self.tab.joining.as_ref().map(|s| (&s.prepared, s.anchor)),
            hidden_annotation: self.inline_label_id(),
            bond_drawing: self.tab.bond_drawing,
            chain_drawing: self.tab.chain_drawing,
            graphic_constrain: self.toolbar.graphic(self.tool).is_some_and(|p| p.constrain),
            graphic_arc: self.tab.arc_editor.geometry,
            graphic_style: &self.tab.graphic_style,
            orbital_phase: self.tab.orbital_phase,
            phase_flipped: self.tab.phase_flipped,
            attach_symbols: self.tab.attach_symbols,
            arrow_preset: self.tab.arrow_style,
            arrow_style: &self.tab.arrows.style,
            bracket_sides: self.tab.bracket_sides,
            doc: self.display_document(),
            selected: if self.tab.cleanup.is_some() || self.tab.inline_text.is_some() {
                &[]
            } else {
                &self.tab.selected
            },
            tool: if self.tab.cleanup.is_some() {
                Tool::Select
            } else {
                self.tool
            },
            camera: self.tab.camera,
            grid: self.grid,
            guides: self.guides,
            smart_guides: self.appearance.smart_guides,
            ring_size: self.ring_size,
            aromatic_ring: self.aromatic_ring,
            template_connection: self
                .tab
                .joining
                .as_ref()
                .map(|s| s.mode)
                .unwrap_or(self.templates.connection),
            template: self
                .templates
                .library
                .get(self.template_index)
                .filter(|_| self.tool == Tool::Template)
                .map(|t| (t, self.templates.anchor)),
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    fn cleanup_bar<'a>(&'a self, preview: &'a super::CleanupPreview) -> Element<'a, Message> {
        use reshiki::cleanup::Scope;
        let scopes = vec![Scope::SelectedAtoms, Scope::SelectedMolecules];
        let mut bar = column![
            row![
                text("Cleanup preview").size(13),
                crate::appearance::pick_list(scopes, Some(preview.job.options.scope), |scope| {
                    Message::Cleanup(super::cleanup::Action::Scope(scope))
                })
                .text_size(12)
                .width(160),
                checkbox(preview.original)
                    .label("Show original")
                    .text_size(12)
                    .size(14)
                    .on_toggle(
                        |original| Message::Cleanup(super::cleanup::Action::Original(original))
                    ),
                Space::new().width(Length::Fill),
                command("Cancel", Message::Cleanup(super::cleanup::Action::Cancel)),
                button(text("Apply").size(12))
                    .on_press_maybe(
                        (!self.tab.busy).then_some(Message::Cleanup(super::cleanup::Action::Apply))
                    )
                    .style(crate::appearance::primary),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            row![
                checkbox(preview.job.options.keep_orientation)
                    .label("Keep orientation")
                    .text_size(11)
                    .size(13)
                    .on_toggle(|keep| Message::Cleanup(super::cleanup::Action::Orientation(keep))),
                text(preview.job.options.scope.hint())
                    .size(11)
                    .style(muted_text),
            ]
            .spacing(16)
            .align_y(Alignment::Center),
        ]
        .spacing(6);
        for warning in &preview.warnings {
            bar = bar.push(text(warning).size(11).style(muted_text));
        }
        container(bar).padding([8, 12]).style(panel).into()
    }

    fn command_bar(&self) -> Element<'_, Message> {
        let bar = row![
            hover_hint(
                reshiki::accessibility::button(
                    "header-about",
                    "About ReShiki; check for updates",
                    crate::branding::wordmark(21.0)
                )
                .padding(0)
                .style(button::text)
                .on_press(Message::Updates(super::updates::Action::Show(true))),
                "About ReShiki · Check for updates",
                tooltip::Position::Bottom
            ),
            divider(),
            icon_button(
                Icon::New,
                keyed("New", &Message::New),
                Some(Message::New),
                false
            ),
            icon_button(
                Icon::Open,
                keyed("Open", &Message::Open),
                Some(Message::Open),
                false
            ),
            icon_button(
                Icon::Save,
                keyed("Save", &Message::Save),
                Some(Message::Save),
                false
            ),
            icon_button(
                Icon::SaveAs,
                keyed("Save as", &Message::SaveAs),
                Some(Message::SaveAs),
                false
            ),
            divider(),
            icon_button(
                Icon::Undo,
                keyed("Undo", &Message::Undo),
                self.text_history_available(false)
                    .unwrap_or_else(|| self.tab.history.can_undo())
                    .then_some(Message::Undo),
                false
            ),
            icon_button(
                Icon::Redo,
                keyed("Redo", &Message::Redo),
                self.text_history_available(true)
                    .unwrap_or_else(|| self.tab.history.can_redo())
                    .then_some(Message::Redo),
                false
            ),
            divider(),
            responsive(move |size| self.tab_strip(size.width))
                .width(Length::Fill)
                .height(Length::Shrink),
            icon_button(
                Icon::Assistant(self.assistant.busy),
                if self.assistant.busy {
                    "Assistant · Working"
                } else {
                    "Assistant"
                },
                Some(Message::Assistant(super::assistant::Action::Open)),
                self.inspector_open && self.inspector_tab == InspectorTab::Assistant
            ),
            icon_button(
                Icon::Import,
                keyed("Import", &Message::Inspector(InspectorTab::Import)),
                Some(Message::Inspector(InspectorTab::Import)),
                self.inspector_open && self.inspector_tab == InspectorTab::Import
            ),
            icon_button(
                Icon::Check,
                if self.tab.busy {
                    "Checking…"
                } else {
                    "Check drawing"
                },
                (!self.tab.busy).then_some(Message::Analyze),
                false
            ),
            icon_button(
                Icon::Cleanup,
                keyed(
                    "Clean up…",
                    &Message::Cleanup(super::cleanup::Action::Begin)
                ),
                (!self.tab.busy).then_some(Message::Cleanup(super::cleanup::Action::Begin)),
                false
            ),
            icon_button(
                Icon::Export,
                keyed("Export", &Message::Inspector(InspectorTab::Export)),
                Some(Message::Inspector(InspectorTab::Export)),
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
        let ring = if let Tool::RingPreset(preset) = self.toolbar.ring {
            super::palettes::ring_hint(
                &preset.to_string(),
                &super::palettes::Action::RingPreset(preset),
            )
        } else if self.aromatic_ring {
            "Rings · r".into()
        } else {
            super::palettes::ring_hint(
                &format!("{}-membered", self.ring_size),
                &super::palettes::Action::Ring(self.ring_size, false),
            )
        };
        let chain = format!("Straight chain · {}", keys(Modifiers::SHIFT, "X"));
        let atom = format!(
            "Atom label · {}",
            super::palettes::element_hint(&self.element)
        );
        let single = super::palettes::bond_hint(BondPreset::Single, "Single bond");
        let double = super::palettes::bond_hint(BondPreset::Double, "Double bond");
        let triple = super::palettes::bond_hint(BondPreset::Triple, "Triple bond");
        let other_bonds = self.toolbar.bond.bond_preset().map_or_else(
            || "Other bonds".into(),
            |preset| super::palettes::bond_hint(preset, preset.name()),
        );
        let arrow = format!("{} · e", self.tab.arrow_style);
        let rectangle = self.toolbar.rectangle.kind.to_string();
        let ellipse = self.toolbar.ellipse.kind.to_string();
        let brackets = super::palettes::graphic_hint(
            self.toolbar.bracket.kind,
            &self.toolbar.bracket.kind.to_string(),
        );
        let graphic_hint = |tool, fallback| match tool {
            Tool::Graphic(kind) => super::palettes::graphic_hint(kind, &kind.to_string()),
            _ => String::from(fallback),
        };
        let symbols = graphic_hint(self.toolbar.symbol, "Chemical symbols");
        let orbitals = graphic_hint(self.toolbar.orbital, "Orbitals");
        let keyboard = self.keyboard_drawing_active();
        let tools = [
            (
                Tool::Select,
                if keyboard {
                    "Select / move · Escape or toolbar"
                } else {
                    "Select / move · v"
                },
            ),
            (
                Tool::Lasso,
                if keyboard {
                    "Lasso select · Toolbar"
                } else {
                    "Lasso select · l"
                },
            ),
            (Tool::Tilt, "3D tilt"),
            (Tool::Erase, "Eraser"),
            (Tool::Atom, atom.as_str()),
            (Tool::Bond(1), single.as_str()),
            (Tool::Bond(2), double.as_str()),
            (Tool::Bond(3), triple.as_str()),
            (self.toolbar.bond, other_bonds.as_str()),
            (self.toolbar.ring, ring.as_str()),
            (Tool::Chain(reshiki::chains::ChainMode::Straight), &chain),
            (
                Tool::Chain(reshiki::chains::ChainMode::Snaking),
                "Snaking chain",
            ),
            (Tool::Arrow, arrow.as_str()),
            (Tool::Text, "Text label · t"),
            (
                Tool::Graphic(self.toolbar.rectangle.kind),
                rectangle.as_str(),
            ),
            (Tool::Graphic(self.toolbar.ellipse.kind), ellipse.as_str()),
            (Tool::Graphic(self.toolbar.bracket.kind), brackets.as_str()),
            (Tool::Graphic(G::Line), "Graphic line"),
            (Tool::Graphic(G::Curve), "Bézier curve"),
            (Tool::Graphic(G::Arc), "Arc"),
            (self.toolbar.symbol, symbols.as_str()),
            (self.toolbar.orbital, orbitals.as_str()),
        ];
        let mut palette = column![section("TOOLS")]
            .spacing(6)
            .align_x(Alignment::Center);
        for pair in tools.chunks(2) {
            let mut line = row![].spacing(4);
            for (tool, hint) in pair {
                line = line.push(self.tool_palette_button(*tool, hint));
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
                line = line.push(self.element_palette_button(symbol));
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
            self.help_palette_button()
        ]
        .spacing(8)
        .align_x(Alignment::Center);
        container(palette)
            .width(PALETTE_WIDTH)
            .height(Length::Fill)
            .padding([14, 8])
            .style(panel)
            .into()
    }

    fn tool_palette_button(&self, tool: Tool, hint: &str) -> Element<'_, Message> {
        let family = super::palettes::family(tool);
        let icon = if tool == Tool::Ring {
            Icon::Ring(self.ring_size, self.aromatic_ring)
        } else if tool == Tool::Arrow {
            Icon::Arrow(self.tab.arrow_style)
        } else {
            Icon::Tool(tool)
        };
        // Vector tools can share a renderer layer. A separate clipped
        // layer per icon adds GPU passes to every canvas redraw.
        let item = iced::widget::canvas(super::tool_button::ToolButton {
            tool,
            icon,
            active: self.tool == tool,
            opens_on_click: family == Some(super::palettes::Family::Bonds),
        })
        .width(36)
        .height(36);
        let item = reshiki::accessibility::button(format!("tool-{tool:?}"), hint.to_owned(), item)
            .padding(0)
            .width(36)
            .height(36)
            .checked(self.tool == tool)
            .style(|_, _| iced::widget::button::Style::default())
            .on_press(if family == Some(super::palettes::Family::Bonds) {
                Message::Palette(super::palettes::Action::Open(tool))
            } else {
                Message::Tool(tool)
            });
        if self.palette.is_some() {
            item.into()
        } else {
            hover_hint(item, hint.to_owned(), tooltip::Position::Right).into()
        }
    }

    fn element_palette_button(&self, symbol: &'static str) -> tooltip::Tooltip<'_, Message> {
        hover_hint(
            reshiki::accessibility::button(
                format!("element-{symbol}"),
                super::palettes::element_hint(symbol),
                text(symbol).size(13).center(),
            )
            .checked(self.tool == Tool::Atom && self.element == symbol)
            .width(36)
            .height(30)
            .on_press(Message::Element(symbol.into()))
            .style(element_control(
                self.tool == Tool::Atom && self.element == symbol,
                &self.tab.doc,
                symbol,
            )),
            super::palettes::element_hint(symbol),
            tooltip::Position::Right,
        )
    }

    fn help_palette_button(&self) -> tooltip::Tooltip<'_, Message> {
        hover_hint(
            reshiki::accessibility::button(
                "help-open",
                "Help and keyboard shortcuts",
                column![
                    iced::widget::canvas(Glyph(Icon::Keyboard, true))
                        .width(24)
                        .height(24),
                    text("Help").size(10),
                ]
                .spacing(3)
                .align_x(Alignment::Center),
            )
            .padding([5, 10])
            .on_press(Message::ToggleHelp)
            .style(control(self.help_open)),
            "Help · F1",
            tooltip::Position::Right,
        )
    }

    /// Bond length and angle constraints differ from the document's.
    fn bond_drawing_changed(&self) -> bool {
        (self.tab.bond_drawing.length - self.tab.doc.drawing_style.bond_length_world).abs() > 0.001
            || self.tab.chain_drawing.angle != 120.
            || !self.tab.bond_drawing.fixed_length
            || !self.tab.bond_drawing.fixed_angles
    }

    /// Width of `bond_constraints`: two checkboxes (13 px box, 8 px gap), the
    /// Length field, 6 px gaps and the measured labels.
    fn constraints_width() -> f32 {
        2. * 21. + UNIT_FIELD + 12. + text_width("Length", 11.) + text_width("Angles", 11.)
    }

    /// Length and Angles for drawing and bonded movement; their Reset is a
    /// row command, so that it can fold into ⋯.
    fn bond_constraints(&self) -> Element<'_, Message> {
        row![
            checkbox(self.tab.bond_drawing.fixed_length)
                .label("Length")
                .on_toggle(Message::FixedLength)
                .size(13)
                .text_size(11),
            container(unit_field(
                crate::appearance::text_input("14.4", &self.tab.drawing_length_input)
                    .on_input(Message::DrawingLength),
                "pt",
            ))
            .width(UNIT_FIELD),
            checkbox(self.tab.bond_drawing.fixed_angles)
                .label("Angles")
                .on_toggle(Message::FixedAngles)
                .size(13)
                .text_size(11),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .into()
    }

    /// Width of the tool options before the row commands, from their leading
    /// divider, for the rows that fold. The selection summary is separate.
    fn options_width(&self) -> f32 {
        let lead = 2. * CONTEXT_GAP + DIVIDER;
        match self.tool {
            tool if tool.bond_preset().is_some() => {
                let preset = BondPreset::ALL
                    .iter()
                    .map(|p| text_width(&p.to_string(), 12.))
                    .fold(0., f32::max);
                // A shrinking pick list: label, handle (text size) and padding.
                lead + preset + 12. + 15. + CONTEXT_GAP + Self::constraints_width()
            }
            Tool::Chain(mode) => {
                lead + text_width(chain_atoms_label(mode), 11.)
                    + 4.
                    + 49.
                    + CONTEXT_GAP
                    + text_width("Angle", 11.)
                    + 4.
                    + UNIT_FIELD
                    + CONTEXT_GAP
                    + Self::constraints_width()
            }
            _ if self.moving_bonded_selection() => lead + Self::constraints_width(),
            _ => 0.,
        }
    }

    /// A partial selection drags bonded atoms along, following Length / Angles.
    fn moving_bonded_selection(&self) -> bool {
        matches!(self.tool, Tool::Select | Tool::Lasso) && {
            let selected = self
                .tab
                .doc
                .expand_abbreviation_selection(&self.tab.selected);
            self.tab
                .doc
                .bonds
                .iter()
                .any(|bond| selected.contains(&bond.a) != selected.contains(&bond.b))
        }
    }

    /// Context row commands in fold order: the first ones fold into ⋯ first.
    pub(super) fn context_commands(&self) -> Vec<RowCommand> {
        let mut commands = self.standard_context_commands();
        if self.keyboard_drawing_active() {
            use super::keyboard_drawing::Action;
            use reshiki::keyboard_drawing::Target;
            let atom = match self.tab.keyboard_drawing.target() {
                Target::Atom(id) => Some(id),
                _ => None,
            };
            let marked = self.tab.keyboard_drawing.marked();
            commands.retain(|command| {
                !matches!(command.message, Message::KeyboardDrawing(Action::Toggle))
            });
            commands.extend([
                RowCommand {
                    label: "Mark [",
                    menu: "Mark atom [",
                    hint: if atom.is_some() {
                        "Mark the active atom for a connection · ["
                    } else {
                        "Choose an atom with arrows or the mouse before marking · ["
                    },
                    message: Message::KeyboardDrawing(Action::Mark),
                    enabled: atom.is_some(),
                },
                RowCommand {
                    label: "Connect ]",
                    menu: "Connect to marked atom ]",
                    hint: if marked.is_none() {
                        "Mark an atom with [ before connecting · ]"
                    } else if atom.is_none() {
                        "Choose an atom with arrows or the mouse to connect · ]"
                    } else if atom == marked {
                        "Choose a different atom to connect to the marked atom · ]"
                    } else {
                        "Connect the active atom to the marked atom · ]"
                    },
                    message: Message::KeyboardDrawing(Action::Connect),
                    enabled: atom.zip(marked).is_some_and(|(a, b)| a != b),
                },
            ]);
        }
        commands
    }

    fn standard_context_commands(&self) -> Vec<RowCommand> {
        let mut commands = Vec::new();
        let constraints = self.tool.bond_preset().is_some()
            || matches!(self.tool, Tool::Chain(_))
            || self.moving_bonded_selection();
        if constraints && self.bond_drawing_changed() {
            commands.push(RowCommand {
                label: "Reset",
                menu: "Reset bonds",
                hint: "Reset bonds · Restore this document's bond length and drawing constraints",
                message: Message::ResetBondDrawing,
                enabled: true,
            });
        }
        if matches!(self.tool, Tool::Select | Tool::Lasso) {
            commands.extend([
                RowCommand {
                    label: "3D optimize…",
                    menu: "3D optimize…",
                    hint: "Generate an optimized conformer using MMFF or UFF; preview before applying",
                    message: Message::Optimization(super::optimization::Action::Begin),
                    enabled: !self.tab.doc.atoms.is_empty() && !self.tab.busy,
                },
                RowCommand {
                    label: "Keyboard drawing",
                    menu: "Keyboard drawing (F8)",
                    hint: "Draw with digits and letters; navigate atoms and bonds with arrows (F8)",
                    message: Message::KeyboardDrawing(super::keyboard_drawing::Action::Toggle),
                    enabled: !self.tab.busy,
                },
            ]);
            let ids = self.depth_ids();
            if ids
                .iter()
                .any(|id| self.tab.doc.atom(*id).is_some_and(|a| a.depth != 0.))
                || reshiki::depth_appearance::has(&self.tab.doc, &ids)
            {
                let automatic = reshiki::depth_appearance::is_automatic_for(&self.tab.doc, &ids);
                commands.push(RowCommand {
                    label: if automatic { "Freeze depth" } else { "Enhance depth" },
                    menu: if automatic { "Freeze depth appearance" } else { "Enhance depth appearance" },
                    hint: "Rear ink fades with depth; freezing keeps positions and editable appearance",
                    message: Message::DepthAppearance(super::depth_appearance::Action::Enhance(!automatic)),
                    enabled: true,
                });
                if reshiki::depth_appearance::has(&self.tab.doc, &ids) {
                    commands.push(RowCommand {
                        label: "Clear depth",
                        menu: "Clear depth appearance",
                        hint: "Restore base colors while keeping the current 3D projection",
                        message: Message::DepthAppearance(super::depth_appearance::Action::Clear),
                        enabled: true,
                    });
                    if !self.tab.selected.is_empty() {
                        commands.push(RowCommand {
                            label: "Original ink",
                            menu: "Use original ink for selection",
                            hint: "Keep selected atoms and bonds in their editable base colors",
                            message: Message::DepthAppearance(
                                super::depth_appearance::Action::OriginalInk,
                            ),
                            enabled: true,
                        });
                    }
                }
            }
        }
        if !matches!(self.tool, Tool::Select | Tool::Lasso) || self.tab.selected.is_empty() {
            return commands;
        }
        commands.extend([
            RowCommand {
                label: "Move & attach…",
                menu: "Move & attach…",
                hint: "Join the selection to another structure at an atom or bond",
                message: Message::Join(super::joining::Action::Begin),
                enabled: self
                    .tab
                    .selected
                    .iter()
                    .any(|id| self.tab.doc.atom(*id).is_some()),
            },
            RowCommand {
                label: "Group",
                menu: "Group",
                hint: "Group",
                message: Message::Group,
                enabled: self.can_group(),
            },
        ]);
        if !self
            .tab
            .doc
            .outer_selected_groups(&self.tab.selected)
            .is_empty()
        {
            commands.push(RowCommand {
                label: "Ungroup",
                menu: "Ungroup",
                hint: "Ungroup",
                message: Message::Ungroup,
                enabled: true,
            });
        }
        commands
    }

    pub(super) fn context_bar(&self) -> Element<'_, Message> {
        if self.tab.optimization.is_some() {
            return container(
                row![
                    text(tool_name(self.tool).0).size(12),
                    text(if self.tool == Tool::Tilt {
                        "Drag to rotate the preview"
                    } else {
                        "Drag unpinned atoms · Shift-click selects"
                    })
                    .size(11)
                    .style(muted_text),
                    Space::new().width(Length::Fill),
                    hover_hint(
                        reshiki::accessibility::button(
                            "context-3d-properties",
                            "Open 3D preview controls in Properties",
                            text("3D preview →").size(12),
                        )
                        .on_press(Message::Inspector(InspectorTab::Properties))
                        .padding([7, 9])
                        .style(control(false)),
                        "Open the 3D preview controls in the right Properties panel",
                        tooltip::Position::Bottom,
                    ),
                ]
                .spacing(CONTEXT_GAP)
                .align_y(Alignment::Center),
            )
            .height(46)
            .padding([5., CONTEXT_PADDING])
            .center_y(46)
            .clip(true)
            .style(panel)
            .into();
        }
        if self.tab.joining.is_some() {
            return self.join_bar();
        }
        container(responsive(move |size| self.context_row(size.width)).height(Length::Shrink))
            .height(46)
            .padding([5., CONTEXT_PADDING])
            .center_y(46)
            .clip(true)
            .style(panel)
            .into()
    }

    /// Tool name, options and commands, plus the arrange group for selection
    /// tools. A short row folds commands into ⋯ instead of scrolling.
    fn context_row(&self, width: f32) -> Element<'_, Message> {
        let keyboard = self.keyboard_drawing_active();
        let (name, short) = if keyboard {
            ("Keyboard", "Keys")
        } else {
            tool_name(self.tool)
        };
        let commands = self.context_commands();
        let widths: Vec<f32> = commands
            .iter()
            .map(|c| text_width(c.label, 12.) + 18.)
            .collect();
        let select = matches!(self.tool, Tool::Select | Tool::Lasso);
        let arrange = (select && self.appearance.arrange_controls).then_some((
            super::object_toolbar::GROUP_WIDTH,
            super::object_toolbar::COMPACT_WIDTH,
        ));
        let summary = if keyboard {
            Some(self.keyboard_context_summary())
        } else {
            (select && !self.tab.selected.is_empty()).then(|| self.selection_summary())
        };
        let options = self.options_width();
        let fit = fit(
            width,
            &Fold {
                name: (
                    text_width(name, 12.) + if keyboard { 18. } else { 0. },
                    text_width(short, 12.) + if keyboard { 18. } else { 0. },
                ),
                options,
                summary: summary
                    .as_ref()
                    .map_or(0., |s| 2. * CONTEXT_GAP + DIVIDER + text_width(s, 11.)),
                commands: &widths,
                arrange,
            },
        );
        let (options_list, hint) = self.tool_options(fit.summary);
        // A shortened name or hidden summary stays in the name's tooltip.
        let mut about = vec![];
        if fit.short {
            about.push(name.to_owned());
        }
        if let Some(summary) = summary.filter(|_| !fit.summary) {
            about.push(summary);
        }
        about.push(hint.into_owned());
        let name: Element<'_, Message> = if keyboard {
            let active = self.tab.keyboard_drawing.active_label(&self.tab.doc);
            let marked = self
                .tab
                .keyboard_drawing
                .marked()
                .map(|id| format!(" · Marked atom {id}"))
                .unwrap_or_default();
            hover_hint(
                reshiki::accessibility::button(
                    "keyboard-done",
                    format!("Leave keyboard drawing · F8 · Active {active}{marked}"),
                    text(if fit.short { short } else { name }).size(12),
                )
                .checked(true)
                .value(active.clone())
                .padding([7, 9])
                .style(control(true))
                .on_press(Message::KeyboardDrawing(
                    super::keyboard_drawing::Action::Leave,
                )),
                format!(
                    "Keyboard drawing · {active}{marked}\n{}\nClick to turn off · F8",
                    reshiki::keyboard_drawing::State::hint()
                ),
                tooltip::Position::Bottom,
            )
            .into()
        } else {
            hover_hint(
                text(if fit.short { short } else { name })
                    .size(12)
                    .style(crate::appearance::text_color(ink())),
                about.join(" · "),
                tooltip::Position::Bottom,
            )
            .into()
        };
        let mut row = row![name].spacing(CONTEXT_GAP).align_y(Alignment::Center);
        if !options_list.is_empty() {
            row = row.push(divider());
        }
        for option in options_list {
            row = row.push(option);
        }
        let (folded, compact) = (fit.folded, fit.compact);
        let mut x = CONTEXT_PADDING + fit.fixed;
        for (index, (c, w)) in commands.iter().zip(&widths).enumerate().skip(folded) {
            row = row.push(hover_keys(
                reshiki::accessibility::button(
                    keyboard_control_id(&c.message)
                        .map_or_else(|| format!("context-command-{index}"), str::to_owned),
                    c.hint,
                    text(c.label).size(12),
                )
                .padding([7, 9])
                .style(control(false))
                .on_press_maybe(c.enabled.then(|| c.message.clone())),
                c.hint,
                super::shortcuts::label(&c.message),
                tooltip::Position::Bottom,
            ));
            x += CONTEXT_GAP + w;
        }
        if folded > 0 {
            let labels: Vec<_> = commands.iter().take(folded).map(|c| c.menu).collect();
            let page = super::context_menu::Page::More(folded);
            row = row.push(
                self.menu_anchor(
                    reshiki::accessibility::button(
                        "context-overflow",
                        format!("More: {}", labels.join(", ")),
                        iced::widget::canvas(Glyph(Icon::More, true))
                            .width(24)
                            .height(24),
                    )
                    .width(MORE_WIDTH)
                    .padding(3)
                    .style(control(false))
                    .on_press(Message::ContextMenu(
                        super::context_menu::Action::Open(page, x + CONTEXT_GAP),
                    )),
                    format!("More: {}", labels.join(", ")),
                ),
            );
        }
        if let Some((full, short)) = arrange {
            let group = if compact { short } else { full };
            row = row.push(
                container(self.arrange_group(CONTEXT_PADDING + width - group, compact))
                    .width(Length::Fill)
                    .align_right(Length::Fill),
            );
        }
        row.into()
    }

    /// Keep the tooltip wrapper stable so opening a menu preserves its button
    /// focus. The inert context bar suppresses its overlays while a menu is open.
    pub(super) fn menu_anchor<'a>(
        &self,
        anchor: impl Into<Element<'a, Message>>,
        hint: impl Into<std::borrow::Cow<'a, str>>,
    ) -> Element<'a, Message> {
        hover_hint(anchor, hint, tooltip::Position::Bottom).into()
    }

    /// Options for the current tool, and the usage hint shown on the tool
    /// name. `summary` shows the selection summary of the selection tools.
    fn tool_options(
        &self,
        summary: bool,
    ) -> (Vec<Element<'_, Message>>, std::borrow::Cow<'static, str>) {
        let (options, hint): (_, &'static str) = match self.tool {
            tool if tool.bond_preset().is_some() => (
                vec![
                    crate::appearance::pick_list(BondPreset::ALL, tool.bond_preset(), |preset| {
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
                    .padding(5)
                    .into(),
                    self.bond_constraints(),
                ],
                tool.hint(),
            ),
            // The tool name shows the mode; the palette switches it.
            Tool::Chain(mode) => self.chain_tool_options(mode),
            Tool::Graphic(kind) => self.graphic_tool_options(kind),
            Tool::EditPoints => (
                vec![
                    hover_hint(
                        reshiki::accessibility::button(
                            "edit-points-done",
                            "Finish editing points",
                            text("Done").size(12),
                        )
                        .padding([7, 9])
                        .on_press(Message::Tool(Tool::Select))
                        .style(control(false)),
                        "Return to Select · Esc",
                        tooltip::Position::Bottom,
                    )
                    .into(),
                ],
                "Drag anchors or control points · Escape finishes",
            ),
            Tool::Template => (
                vec![
                    text(
                        self.templates
                            .library
                            .get(self.template_index)
                            .map(|t| t.name.as_str())
                            .unwrap_or("Template"),
                    )
                    .size(12)
                    .into(),
                    done("Cancel"),
                ],
                "Click to place / attach · Drag to orient",
            ),
            Tool::Ring | Tool::RingPreset(_) => return self.ring_tool_options(),
            Tool::Arrow => (
                vec![
                    crate::appearance::pick_list(
                        reshiki::arrows::Preset::ALL,
                        Some(self.tab.arrow_style),
                        Message::ArrowStyle,
                    )
                    .text_size(12)
                    .padding(5)
                    .into(),
                ],
                "Click to place / change · Click again to switch · Drag to draw",
            ),
            Tool::Atom => (
                vec![
                    text(format!("Element: {}", self.element)).size(12).into(),
                    crate::appearance::text_input("Symbol: Si, Na, Fe…", &self.custom_element)
                        .on_input(Message::CustomElement)
                        .on_submit(Message::ApplyElement)
                        .size(12)
                        .padding(6)
                        .width(160)
                        .into(),
                    command("Use", Message::ApplyElement).into(),
                ],
                "Click to replace · Drag from an atom to add with a bond",
            ),
            Tool::Text => (
                vec![],
                "Click an atom to name it · Click empty space for a caption · Escape cancels",
            ),
            Tool::Tilt => self.tilt_tool_options(),
            Tool::Select | Tool::Lasso => self.selection_tool_options(summary),
            _ => (vec![], self.tool.hint()),
        };
        (options, hint.into())
    }

    fn chain_tool_options(
        &self,
        mode: reshiki::chains::ChainMode,
    ) -> (Vec<Element<'_, Message>>, &'static str) {
        (
            vec![
                hover_hint(
                    row![
                        text(chain_atoms_label(mode)).size(11),
                        crate::appearance::text_input("Auto", &self.tab.chain_atoms_input)
                            .on_input(Message::ChainAtoms)
                            .width(49)
                            .size(12)
                            .padding(5),
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center),
                    "Includes attachment atoms · Auto places 6 atoms per click",
                    tooltip::Position::Bottom,
                )
                .into(),
                row![
                    text("Angle").size(11),
                    container(unit_field(
                        crate::appearance::text_input("120", &self.tab.chain_angle_input)
                            .on_input(Message::ChainAngle),
                        "°",
                    ))
                    .width(UNIT_FIELD),
                ]
                .spacing(4)
                .align_y(Alignment::Center)
                .into(),
                self.bond_constraints(),
            ],
            "Ctrl bends · Shift flips start · Alt frees · Auto click: 6 atoms",
        )
    }

    fn graphic_tool_options(
        &self,
        kind: reshiki::graphics::GraphicKind,
    ) -> (Vec<Element<'_, Message>>, &'static str) {
        use reshiki::graphics::GraphicKind as G;
        let chooser: Element<'_, Message> = match kind {
            G::Symbol(k) => {
                crate::appearance::pick_list(reshiki::scientific::SymbolKind::ALL, Some(k), |k| {
                    Message::Graphics(super::graphics::Action::ScientificKind(G::Symbol(k)))
                })
                .text_size(12)
                .padding(5)
                .into()
            }
            G::Orbital(k) => {
                crate::appearance::pick_list(reshiki::scientific::OrbitalKind::ALL, Some(k), |k| {
                    Message::Graphics(super::graphics::Action::ScientificKind(G::Orbital(k)))
                })
                .text_size(12)
                .padding(5)
                .into()
            }
            _ => crate::appearance::pick_list(G::DRAWABLE, Some(kind), |kind| {
                Message::Tool(Tool::Graphic(kind))
            })
            .text_size(12)
            .padding(5)
            .into(),
        };
        let mut options = vec![chooser];
        if kind == G::Arc {
            options.push(self.arc_presets(false));
        }
        (
            options,
            match kind {
                G::Symbol(_) => "Click to place/attach · Drag to position · Escape cancels",
                G::Orbital(_) => "Drag from node · Click for default size · Shift snaps to 15°",
                G::Arc => "Drag an ellipse frame · Shift makes it circular · Escape cancels",
                _ => "Drag to draw · Shift constrains · Escape cancels",
            },
        )
    }

    fn ring_tool_options(&self) -> (Vec<Element<'_, Message>>, std::borrow::Cow<'static, str>) {
        use reshiki::rings::Preset;
        let preset = if let Tool::RingPreset(p) = self.tool {
            p
        } else {
            Preset::Regular
        };
        let mut options = vec![
            crate::appearance::pick_list(Preset::ALL, Some(preset), |p| {
                Message::Tool(if p == Preset::Regular {
                    Tool::Ring
                } else {
                    Tool::RingPreset(p)
                })
            })
            .text_size(12)
            .padding(5)
            .into(),
        ];
        if preset != Preset::Regular {
            let hint = if preset == Preset::Cyclopentadiene {
                "Click / drag · Alt connects · Shift swaps double bonds"
            } else {
                "Click / drag · Alt connects by a bond"
            };
            return (options, hint.into());
        }
        options.extend([
            text("Size").size(11).style(muted_text).into(),
            crate::appearance::pick_list(
                [3_u8, 4, 5, 6, 7, 8],
                Some(self.ring_size),
                Message::RingSize,
            )
            .text_size(12)
            .padding(5)
            .into(),
            // A separate label draws the shortcut in the shortcut font.
            row![
                checkbox(self.aromatic_ring)
                    .on_toggle(Message::AromaticRing)
                    .size(14),
                mouse_area(keyed_text(
                    "Aromatic",
                    Some(keys(Modifiers::SHIFT, "R")),
                    ""
                ))
                .on_press(Message::AromaticRing(!self.aromatic_ring))
                .interaction(iced::mouse::Interaction::Pointer),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into(),
        ]);
        let hint = format!(
            "Click / drag to attach · {} keeps ring size",
            keys(Modifiers::SHIFT, "R")
        );
        (options, hint.into())
    }

    fn tilt_tool_options(&self) -> (Vec<Element<'_, Message>>, &'static str) {
        let enabled = crate::canvas::tilt::available(&self.tab.doc, &self.tab.selected);
        let mut tilts = row![].spacing(4);
        for (label, transform) in [
            ("X −15°", reshiki::editing::Transform::TiltX(-15.)),
            ("X +15°", reshiki::editing::Transform::TiltX(15.)),
            ("Y −15°", reshiki::editing::Transform::TiltY(-15.)),
            ("Y +15°", reshiki::editing::Transform::TiltY(15.)),
        ] {
            tilts = tilts.push(
                command(label, Message::Transform(transform))
                    .on_press_maybe(enabled.then_some(Message::Transform(transform))),
            );
        }
        let depth = Message::InspectorAction(super::inspector::Action::DepthBonds);
        (
            vec![
                tilts.into(),
                hover_hint(
                    command("Front bonds", depth.clone()).on_press_maybe(enabled.then_some(depth)),
                    "Emphasize front bonds using the retained projection depth",
                    tooltip::Position::Bottom,
                )
                .into(),
                done("Done"),
            ],
            "Drag to tilt · Shift: 15°",
        )
    }

    fn selection_tool_options(&self, summary: bool) -> (Vec<Element<'_, Message>>, &'static str) {
        let mut options = Vec::new();
        if summary && self.keyboard_drawing_active() {
            options.push(
                hover_hint(
                    text(self.keyboard_context_summary())
                        .size(11)
                        .style(muted_text),
                    self.tab.keyboard_drawing.active_label(&self.tab.doc),
                    tooltip::Position::Bottom,
                )
                .into(),
            );
        } else if summary && !self.tab.selected.is_empty() {
            options.push(
                hover_hint(
                    text(self.selection_summary()).size(11).style(muted_text),
                    format!(
                        "Click a bond's middle to select it; Shift-click adds. {} selects the whole drawing.",
                        keys(Modifiers::COMMAND, "A")
                    ),
                    tooltip::Position::Bottom,
                )
                .into(),
            );
        }
        // Keep bonded-movement controls in this fixed-height row: a
        // second row would move the canvas between the two clicks
        // used to select a molecule.
        if self.moving_bonded_selection() {
            if !options.is_empty() {
                options.push(divider());
            }
            options.push(
                hover_hint(
                    self.bond_constraints(),
                    "Bonded movement follows Length / Angles · Option/Alt: free movement",
                    tooltip::Position::Bottom,
                )
                .into(),
            );
        }
        (
            options,
            if self.tool == Tool::Lasso {
                "Draw around objects · Shift adds · Option drag removes"
            } else {
                "Double-click selects molecule · Shift-click adds"
            },
        )
    }

    pub(super) fn inspector_width(&self) -> f32 {
        if !self.inspector_open {
            return 0.;
        }
        match self.inspector_tab {
            InspectorTab::Assistant => 380.,
            InspectorTab::Names => 340.,
            InspectorTab::DrawingStyle | InspectorTab::Reactions => 320.,
            InspectorTab::Properties | InspectorTab::Import | InspectorTab::Export => 300.,
            _ => 256.,
        }
    }

    fn inspector(&self) -> Element<'_, Message> {
        if self.inspector_tab == InspectorTab::Names {
            return container(scrollable(container(self.naming_panel()).padding([12, 16])))
                .width(self.inspector_width())
                .height(Length::Fill)
                .style(panel)
                .into();
        }
        if self.inspector_tab == InspectorTab::Reactions {
            return self.reactions_inspector();
        }
        if self.inspector_tab == InspectorTab::DrawingStyle {
            return container(self.drawing_style_panel())
                .width(self.inspector_width())
                .height(Length::Fill)
                .style(panel)
                .into();
        }
        if self.inspector_tab == InspectorTab::Assistant {
            return container(self.assistant_panel())
                .width(self.inspector_width())
                .height(Length::Fill)
                .style(panel)
                .into();
        }
        let mut tabs = row![].spacing(2);
        for (label, tab) in [
            ("Properties", InspectorTab::Properties),
            ("Templates", InspectorTab::Templates),
            ("Import", InspectorTab::Import),
            ("Export", InspectorTab::Export),
        ] {
            tabs = tabs.push(
                // Four tabs fit the narrowest (256 px) inspector.
                button(text(label).size(11))
                    .padding([7, 6])
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
            InspectorTab::ThemeGenerator => self.theme_generator_panel(),
            InspectorTab::Properties if self.tab.joining.is_some() => self.join_panel(),
            InspectorTab::Properties => self.properties_panel(),
            InspectorTab::Labels => self.atom_labels_panel(),
            InspectorTab::Abbreviations => self.abbreviations_panel(),
            InspectorTab::Templates => self.templates_panel(),
            InspectorTab::Import => column![
                self.import_panel(),
                button("Chemical names…").on_press(Message::Inspector(InspectorTab::Names)),
            ]
            .spacing(12)
            .into(),
            InspectorTab::Names => self.naming_panel(),
            InspectorTab::Export => self.export_panel(),
        };
        let mut content = column![
            container(tabs).padding([8, 8]),
            scrollable(container(body).padding([8, 16]))
                .id("inspector-content")
                .on_scroll(|viewport| Message::InspectorScroll(viewport.absolute_offset().y))
                .height(Length::Fill)
        ]
        .spacing(4);
        if self.inspector_tab == InspectorTab::Properties && self.tab.optimization.is_some() {
            content = content.push(container(self.optimization_footer()).padding([10, 16]));
        }
        container(content)
            .width(self.inspector_width())
            .height(Length::Fill)
            .style(panel)
            .into()
    }

    fn templates_panel(&self) -> Element<'_, Message> {
        use super::template_library::Action as A;
        let action = Message::Templates;
        let state = &self.templates;
        let mut body = column![section("TEMPLATE LIBRARY")].spacing(8);
        body = self.template_browse_controls(body);
        if let Some(error) = &state.notice {
            body = body.push(
                text(error)
                    .size(11)
                    .style(crate::appearance::text_color(Color::from_rgb8(182, 66, 61))),
            );
        }
        if state.editing {
            body = body
                .push(section(if state.draft.is_some() {
                    "NEW TEMPLATE"
                } else {
                    "EDIT TEMPLATE"
                }))
                .push(
                    crate::appearance::text_input("Template name", &state.name)
                        .on_input(|s| Message::Templates(A::Name(s)))
                        .size(12)
                        .padding(7),
                )
                .push(
                    crate::appearance::text_input("Collection", &state.category)
                        .on_input(|s| Message::Templates(A::Category(s)))
                        .size(12)
                        .padding(7),
                )
                .push(
                    row![
                        command("Save", action(A::SaveDetails)).style(crate::appearance::primary),
                        command("Cancel", action(A::CancelDetails))
                    ]
                    .spacing(6),
                );
        }
        if state.active
            && let Some(t) = state.library.get(self.template_index)
        {
            body = self.active_template_details(body, t);
        }
        if state.undo.is_some() {
            body = body.push(command("Undo library change", action(A::Restore)));
        }
        if !state.active && !state.editing {
            body = self.template_library_browser(body);
        }
        body.into()
    }

    fn template_browse_controls<'a>(
        &'a self,
        mut body: iced::widget::Column<'a, Message>,
    ) -> iced::widget::Column<'a, Message> {
        use super::template_library::{Action as A, Filter};
        let action = Message::Templates;
        let state = &self.templates;
        if !state.active {
            body = body.push(
                column![
                    crate::appearance::text_input("Search names, collections…", &state.query)
                        .on_input(|s| Message::Templates(A::Search(s)))
                        .size(12)
                        .padding(8),
                    crate::appearance::pick_list(
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
                    .on_press_maybe((!self.tab.selected.is_empty()).then(|| action(A::BeginSave))),
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
        body
    }

    fn active_template_details<'a>(
        &'a self,
        mut body: iced::widget::Column<'a, Message>,
        t: &'a reshiki::templates::Template,
    ) -> iced::widget::Column<'a, Message> {
        use super::template_library::Action as A;
        let action = Message::Templates;
        let state = &self.templates;
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
                crate::appearance::pick_list(
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
            .push(text(state.connection.hint()).size(11).style(muted_text))
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
                .style(muted_text),
            );
        if !t.note.is_empty() {
            body = body.push(text(&t.note).size(11).style(muted_text));
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
                            (!self.tab.selected.is_empty()).then(|| action(A::Replace)),
                        ),
                )
                .push(command("Remove template", action(A::Remove)));
        }
        body
    }

    fn template_library_browser<'a>(
        &'a self,
        mut body: iced::widget::Column<'a, Message>,
    ) -> iced::widget::Column<'a, Message> {
        use super::template_library::{Action as A, Filter};
        let action = Message::Templates;
        let state = &self.templates;
        if state.collection == "All collections"
            && state.query.trim().is_empty()
            && state.filter == Filter::All
        {
            let mut categories =
                std::collections::BTreeMap::<&str, (usize, &reshiki::templates::Template)>::new();
            for template in state.library.iter() {
                let name = super::template_library::category(template);
                let entry = categories.entry(name).or_insert((0, template));
                entry.0 += 1;
            }
            body = body
                .push(horizontal_line())
                .push(text("CATEGORIES").size(10).style(muted_text));
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
                            text(count.to_string()).size(11).style(muted_text),
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
            return body;
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
            .style(muted_text),
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
                    .style(muted_text),
            );
        }
        body
    }

    fn abbreviations_panel(&self) -> Element<'_, Message> {
        use super::abbreviations::Action as A;
        let action = Message::Abbreviations;
        let selected: Vec<_> = self
            .tab
            .doc
            .abbreviations
            .iter()
            .filter(|g| g.members.iter().any(|id| self.tab.selected.contains(id)))
            .collect();
        let choices: Vec<String> = reshiki::abbreviations::PRESETS
            .iter()
            .chain(reshiki::common_groups::LABELS.iter())
            .chain(reshiki::ligands::LABELS)
            .map(|s| (*s).into())
            .collect();
        // Disabled commands say what they need.
        let item = |label, message: Message, enabled: bool, hint, reason| {
            hover_hint(
                command(label, message.clone()).on_press_maybe(enabled.then_some(message)),
                if enabled { hint } else { reason },
                tooltip::Position::Top,
            )
        };
        let wait = "Wait for the current chemistry job to finish";
        let label = self.abbreviations.label.trim();
        let mut body = column![
            command("‹ Properties", Message::Inspector(InspectorTab::Properties)),
            text("Chemical abbreviations").size(18),
            text("Compact labels with the complete molecule inside.").size(12).style(muted_text),
            row![
                item("Expand selected", action(A::Expand), !selected.is_empty(), "Restore the atoms and bonds of the selected abbreviations", "Select an abbreviation first"),
                item("Expand all", action(A::ExpandAll), !self.tab.doc.abbreviations.is_empty(), "Restore every abbreviation in the drawing", "The drawing has no abbreviations"),
            ]
            .spacing(6),
            horizontal_line(),
            text("COMMON GROUP").size(11).style(muted_text),
            crate::appearance::pick_list(choices, Some(self.abbreviations.preset.clone()), move |s| action(A::Preset(s))).width(Length::Fill).text_size(14),
            item(
                "Replace selected endpoint",
                action(A::Replace),
                !self.tab.busy && !self.tab.selected.is_empty(),
                "Replaces one terminal atom or an existing abbreviation. Its connecting bond stays in place.",
                if self.tab.busy { wait } else { "Select one terminal atom or an existing abbreviation" },
            ),
            horizontal_line(),
            item(
                "Contract common groups",
                action(A::Find),
                !self.tab.busy && !self.tab.doc.atoms.is_empty(),
                "Contracts recognized common groups into labels",
                if self.tab.busy { wait } else { "Draw or import a molecule first" },
            ),
            text(if self.tab.selected.is_empty() { "Searches the whole drawing." } else { "Complete groups in the selection only." }).size(11).style(muted_text),
            horizontal_line(),
            text("NAME A SELECTED FRAGMENT").size(11).style(muted_text),
            crate::appearance::text_input("Label, e.g. Ar", &self.abbreviations.label).on_input(move |s| action(A::Label(s))).on_submit(action(A::Contract)).size(13),
            crate::appearance::text_input("From the right (optional)", &self.abbreviations.reverse_label).on_input(move |s| action(A::ReverseLabel(s))).on_submit(action(A::Contract)).size(13),
            item(
                "Contract selection",
                action(A::Contract),
                !self.tab.selected.is_empty() && !label.is_empty(),
                "Select a connected fragment whose outside bonds meet one selected atom. A custom name does not change its chemistry.",
                if self.tab.selected.is_empty() { "Select a connected fragment first" } else { "Enter a label first" },
            ),
            horizontal_line(),
        ].spacing(10);
        for group in selected {
            let atoms = group
                .members
                .iter()
                .filter(|id| self.tab.doc.atom(**id).is_some_and(|a| a.element != "*"))
                .count();
            body = body.push(
                text(format!(
                    "{} · {} atom{}",
                    group.label,
                    atoms,
                    if atoms == 1 { "" } else { "s" }
                ))
                .size(12)
                .style(crate::appearance::text_color(Color::from_rgb8(
                    17, 126, 108,
                ))),
            );
        }
        body.into()
    }

    fn atom_labels_panel(&self) -> Element<'_, Message> {
        use super::atom_labels::{Action as A, Scope};
        use reshiki::atom_labels::{Carbons, HydrogenPosition};
        let ids = self.label_ids();
        let atoms: Vec<_> = self
            .tab
            .doc
            .atoms
            .iter()
            .filter(|a| ids.contains(&a.id))
            .collect();
        let carbon_values: Vec<_> = atoms
            .iter()
            .map(|a| {
                a.display
                    .carbons
                    .unwrap_or(self.tab.doc.atom_labels.carbons)
            })
            .collect();
        let carbons = if atoms.is_empty() {
            Some(self.tab.doc.atom_labels.carbons)
        } else {
            carbon_values
                .first()
                .copied()
                .filter(|c| carbon_values.iter().all(|v| v == c))
        };
        let hydrogens = if atoms.is_empty() {
            self.tab.doc.atom_labels.hydrogens
        } else {
            atoms
                .iter()
                .all(|a| reshiki::atom_labels::hydrogens(a, &self.tab.doc))
        };
        let stereo = if atoms.is_empty() {
            self.tab.doc.atom_labels.stereo
        } else {
            atoms.iter().all(|a| {
                a.display
                    .stereo
                    .show
                    .unwrap_or(self.tab.doc.atom_labels.stereo)
            })
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
            crate::appearance::pick_list(
                [Scope::Drawing, Scope::Selection],
                Some(self.tab.labels.scope),
                |s| Message::Labels(A::Scope(s))
            )
            .text_size(12)
            .width(Length::Fill),
            text(format!("{} atoms in scope", ids.len()))
                .size(11)
                .style(muted_text),
            text("Carbon labels").size(12),
            crate::appearance::pick_list(Carbons::ALL, carbons, |v| Message::Labels(A::Carbons(v)))
                .placeholder("Mixed")
                .text_size(12)
                .width(Length::Fill),
            checkbox(hydrogens)
                .label("Show implied hydrogens")
                .text_size(12)
                .on_toggle(|v| Message::Labels(A::Hydrogens(v))),
            checkbox(atoms.iter().all(|a| !a.display.hide_charge))
                .label("Show charge labels")
                .text_size(12)
                .on_toggle(|v| Message::Labels(A::Charges(v))),
            text("Hydrogen position").size(12),
            crate::appearance::pick_list(HydrogenPosition::ALL, position, |v| Message::Labels(
                A::Position(v)
            ))
            .placeholder("Mixed")
            .text_size(12)
            .width(Length::Fill),
            text("Display settings keep the molecular composition intact.")
                .size(11)
                .style(muted_text),
            horizontal_line(),
            section("ATOM NUMBERS"),
            row![
                crate::appearance::text_input("1, atom1, a, α…", &self.tab.labels.seed)
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
                .style(muted_text),
            command("Clear numbers", Message::Labels(A::ClearNumbers)),
        ]
        .spacing(9);
        if ids.len() == 1 {
            body = body.push(text("Custom atom number").size(12)).push(
                row![
                    crate::appearance::text_input("e.g. Cα or 12a", &self.tab.labels.number)
                        .on_input(|s| Message::Labels(A::Text(s)))
                        .on_submit(Message::Labels(A::ApplyText))
                        .size(12)
                        .padding(7),
                    command("Set", Message::Labels(A::ApplyText))
                ]
                .spacing(5),
            );
        }
        body = self.atom_label_stereo_section(body, stereo);
        self.atom_label_indicator_section(body)
    }

    fn atom_label_stereo_section<'a>(
        &'a self,
        mut body: iced::widget::Column<'a, Message>,
        stereo: bool,
    ) -> iced::widget::Column<'a, Message> {
        use super::atom_labels::Action as A;
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
                    .style(muted_text),
            );
        if let Some(error) = &self.tab.chemistry_notice {
            body = body.push(
                text(error)
                    .size(11)
                    .style(crate::appearance::text_color(Color::from_rgb8(182, 66, 61))),
            );
        }
        body
    }

    fn atom_label_indicator_section<'a>(
        &'a self,
        body: iced::widget::Column<'a, Message>,
    ) -> Element<'a, Message> {
        use super::atom_labels::Action as A;
        body.push(horizontal_line())
            .push(section("INDICATOR APPEARANCE"))
            .push(
                row![
                    crate::appearance::text_input("Size in pt", &self.tab.labels.size)
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

    fn view_options(&self) -> Element<'_, Message> {
        container(
            row![
                text("View").size(12).style(muted_text),
                text("Interface").size(12).style(muted_text),
                crate::appearance::pick_list(
                    crate::appearance::Mode::ALL,
                    Some(self.appearance.mode),
                    Message::Appearance
                )
                .text_size(12)
                .padding(5)
                .width(132),
                hover_hint(
                    checkbox(self.appearance.arrange_controls)
                        .label("Arrange controls")
                        .on_toggle(|visible| Message::ObjectToolbar(
                            super::object_toolbar::Action::Visible(visible)
                        ))
                        .size(14)
                        .text_size(12),
                    "Align, distribute, order, flip and rotate at the right end of the Select row",
                    tooltip::Position::Top,
                ),
                checkbox(self.grid)
                    .label("Grid")
                    .on_toggle(|_| Message::View(super::view_settings::Action::Grid))
                    .size(14)
                    .text_size(12),
                hover_hint(
                    checkbox(self.appearance.smart_guides)
                        .label("Smart guides")
                        .on_toggle(|on| Message::View(super::view_settings::Action::SmartGuides(on)))
                        .size(14)
                        .text_size(12),
                    "Snap dragged objects to other objects' edges, centers and equal gaps · Hold Option/Alt to move freely",
                    tooltip::Position::Top,
                ),
                checkbox(self.guides.rulers)
                    .label("Rulers")
                    .on_toggle(|on| Message::View(super::view_settings::Action::Rulers(on)))
                    .size(14)
                    .text_size(12),
                checkbox(self.guides.crosshair)
                    .label("Crosshair")
                    .on_toggle(|on| Message::View(super::view_settings::Action::Crosshair(on)))
                    .size(14)
                    .text_size(12),
                divider(),
                text("Units").size(12).style(muted_text),
                crate::appearance::pick_list(
                    crate::canvas::guides::Unit::ALL,
                    Some(self.guides.unit),
                    |unit| Message::View(super::view_settings::Action::RulerUnit(unit))
                )
                .text_size(12)
                .padding(5)
                .width(68),
                command("Page setup…", Message::Pages(super::pages::Action::Show)),
                Space::new().width(Length::Fill),
                command("Done", Message::View(super::view_settings::Action::Toggle)),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .padding([7, 14])
        .style(panel)
        .into()
    }

    fn status_bar(&self) -> Element<'_, Message> {
        let summary = self.status.lines().next().unwrap_or(&self.status);
        let error = self.error;
        let style = move |theme: &Theme| iced::widget::text::Style {
            color: Some(if error {
                crate::appearance::readable(
                    crate::appearance::themed(theme, Color::from_rgb8(168, 52, 47)),
                    &[theme.palette().background],
                    reshiki::color_contrast::TEXT_TARGET,
                )
            } else {
                crate::appearance::muted(theme)
            }),
        };
        // Key symbols, as in "⇧⌘G ungroups", use the shortcut font.
        let message: Element<'_, Message> = if summary.contains(super::shortcuts::symbol) {
            rich_text(super::shortcuts::spans(summary))
                .size(11)
                .wrapping(text::Wrapping::None)
                .style(style)
                .into()
        } else {
            text(summary)
                .size(11)
                .wrapping(text::Wrapping::None)
                .style(style)
                .into()
        };
        // One line; the full text of long or multi-line messages is a tooltip.
        let room = if self.recovered.is_empty() {
            300.
        } else {
            160.
        };
        let message = if self.status.contains('\n') || text_width(summary, 11.) > room {
            hover_hint(message, self.status.as_str(), tooltip::Position::Top).into()
        } else {
            message
        };
        let mut status = row![].spacing(8).align_y(Alignment::Center);
        // Short enough to fit the minimum window beside Update available.
        if !self.recovered.is_empty() {
            status = status
                .push(
                    text("Recovery draft")
                        .size(11)
                        .wrapping(text::Wrapping::None),
                )
                .push(hover_hint(
                    command("Restore", Message::Restore).style(control(true)),
                    match self.recovered.len() {
                        1 => "Open the draft in a tab · Save it to keep a copy".to_owned(),
                        n => format!("Open all {n} drafts as tabs · Save them to keep copies"),
                    },
                    tooltip::Position::Top,
                ))
                .push(hover_hint(
                    command("Dismiss", Message::DismissRecovery),
                    "Hide until the next launch",
                    tooltip::Position::Top,
                ));
        }
        status = status.push(container(message).width(Length::Fill).clip(true));
        if self.updates.available() {
            status = status.push(
                command(
                    "Update available",
                    Message::Updates(super::updates::Action::Show(true)),
                )
                .style(control(true)),
            );
        }
        if !self.tab.autosave_status.is_empty() {
            let failed = self.tab.autosave_status.starts_with("Recovery save failed");
            status = status.push(hover_hint(
                container(
                    container(Space::new().width(7).height(7)).style(move |theme| {
                        container::Style {
                            background: Some(
                                crate::appearance::themed(
                                    theme,
                                    if failed {
                                        Color::from_rgb8(168, 52, 47)
                                    } else {
                                        Color::from_rgb8(86, 160, 132)
                                    },
                                )
                                .into(),
                            ),
                            border: Border {
                                radius: 4.0.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        }
                    }),
                )
                .padding(5),
                self.tab.autosave_status.as_str(),
                tooltip::Position::Top,
            ));
        }
        let status = status
            .extend(self.document_settings())
            .push(divider())
            .push(
                command("View", Message::View(super::view_settings::Action::Toggle))
                    .style(control(self.view_open)),
            )
            .push(command("−", Message::Zoom(0.8)))
            .push(
                text(format!("{:.0}%", self.tab.camera.zoom * 100.0))
                    .size(11)
                    .width(38)
                    .center(),
            )
            .push(command("+", Message::Zoom(1.25)))
            .push(command("Fit", Message::Fit));
        container(status).padding([6, 14]).style(panel).into()
    }

    /// Journal preset, color theme and canvas light/dark: whole-document
    /// settings, kept beside View and zoom.
    fn document_settings(&self) -> [Element<'_, Message>; 3] {
        use super::document_styles::Choice;
        use reshiki::document_styles::Preset;
        let current = Preset::ALL
            .into_iter()
            .find(|p| p.style() == self.tab.doc.drawing_style)
            .map(Choice::Journal)
            .unwrap_or(Choice::Custom);
        let (themes, theme) = self.theme_choices();
        let dark = self.tab.doc.canvas_theme.is_dark();
        [
            hover_hint(
                crate::appearance::pick_list(
                    Preset::ALL
                        .into_iter()
                        .map(Choice::Journal)
                        .chain([Choice::Details])
                        .collect::<Vec<_>>(),
                    Some(current),
                    Message::QuickDrawingStyle,
                )
                .text_size(11)
                .padding([4, 8]),
                "Journal preset · Bond length, line widths and label font",
                tooltip::Position::Top,
            )
            .into(),
            hover_hint(
                crate::appearance::pick_list(themes, Some(theme.clone()), |choice| {
                    Message::ThemeFile(super::theme_files::Action::Choose(choice))
                })
                // Sizes the list for an embedded theme's name, which is not an option.
                .placeholder(theme.to_string())
                .text_size(11)
                .padding([4, 8]),
                "Color theme",
                tooltip::Position::Top,
            )
            .into(),
            hover_hint(
                button(
                    iced::widget::canvas(Glyph(if dark { Icon::Moon } else { Icon::Sun }, true))
                        .width(24)
                        .height(24),
                )
                .padding(3)
                .style(control(false))
                .on_press(Message::CanvasTheme(self.tab.doc.canvas_theme.toggled())),
                if dark {
                    "Dark canvas · Switch to light"
                } else {
                    "Light canvas · Switch to dark"
                },
                tooltip::Position::Top,
            )
            .into(),
        ]
    }
}

const PALETTE_WIDTH: f32 = 104.;
const CONTEXT_PADDING: f32 = 14.;
const CONTEXT_GAP: f32 = 10.;
const DIVIDER: f32 = 11.;
const MORE_WIDTH: f32 = 30.;
/// A context row field with its unit inside.
const UNIT_FIELD: f32 = 54.;

/// A context row command that can fold into the ⋯ menu.
pub(super) struct RowCommand {
    pub label: &'static str,
    /// The label in the ⋯ menu, away from the controls it belongs to.
    pub menu: &'static str,
    /// Tooltip text, followed by the shortcut if there is one.
    pub hint: &'static str,
    pub message: Message,
    pub enabled: bool,
}

/// Shared by the inline context row and its foreground overflow menu.
pub(super) fn keyboard_control_id(message: &Message) -> Option<&'static str> {
    use super::keyboard_drawing::Action;
    match message {
        Message::KeyboardDrawing(Action::Mark) => Some("keyboard-mark"),
        Message::KeyboardDrawing(Action::Connect) => Some("keyboard-connect"),
        Message::KeyboardDrawing(Action::Leave) => Some("keyboard-done"),
        _ => None,
    }
}

/// Measured widths of a context row's parts.
struct Fold<'a> {
    /// The tool name and its short form.
    name: (f32, f32),
    /// Tool options from their leading divider, without the summary.
    options: f32,
    /// The selection summary with its divider and gaps, or 0.
    summary: f32,
    commands: &'a [f32],
    /// The full and collapsed arrange group.
    arrange: Option<(f32, f32)>,
}

/// How a context row fits its width.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Fit {
    /// Commands folded into ⋯, from the front.
    folded: usize,
    /// The arrange group is one Arrange ▾ button.
    compact: bool,
    /// The selection summary is shown.
    summary: bool,
    /// The tool name is its short form.
    short: bool,
    /// Width of the name, options and summary, before the commands.
    fixed: f32,
}

/// A short row gives way in this order until it fits `width`: commands fold
/// into ⋯ one by one, the arrange group collapses, the selection summary
/// hides, and the tool name shortens.
fn fit(width: f32, row: &Fold<'_>) -> Fit {
    let n = row.commands.len();
    let with = |folded, compact, summary: bool, short: bool| {
        let name = if short { row.name.1 } else { row.name.0 };
        Fit {
            folded,
            compact,
            summary,
            short,
            fixed: name + row.options + if summary { row.summary } else { 0. },
        }
    };
    let total = |fit: &Fit| {
        fit.fixed
            + row
                .commands
                .iter()
                .skip(fit.folded)
                .map(|w| CONTEXT_GAP + w)
                .sum::<f32>()
            + if fit.folded > 0 {
                CONTEXT_GAP + MORE_WIDTH
            } else {
                0.
            }
            + row.arrange.map_or(0., |(full, short)| {
                CONTEXT_GAP + if fit.compact { short } else { full }
            })
    };
    let compact = row.arrange.is_some();
    let summary = row.summary > 0.;
    let short = row.name.1 < row.name.0;
    (0..=n)
        .map(|folded| with(folded, false, true, false))
        .chain(compact.then(|| with(n, true, true, false)))
        .chain(summary.then(|| with(n, compact, false, false)))
        .chain(short.then(|| with(n, compact, !summary, true)))
        .find(|fit| total(fit) <= width)
        .unwrap_or_else(|| with(n, compact, !summary, short))
}

fn chain_atoms_label(mode: reshiki::chains::ChainMode) -> &'static str {
    if mode == reshiki::chains::ChainMode::Snaking {
        "Max atoms"
    } else {
        "Atoms"
    }
}

/// Returns to Select, like Escape.
fn done(label: &str) -> Element<'_, Message> {
    hover_hint(
        command(label, Message::Tool(Tool::Select)),
        "Return to Select · Esc",
        tooltip::Position::Bottom,
    )
    .into()
}

/// Width of one line of interface text, measured like the text widget does.
pub(crate) fn text_width(label: &str, size: f32) -> f32 {
    font_width(
        label,
        size,
        iced::Font::with_name(reshiki::style::ui_font_family()),
    )
}

/// Width of one line of text in `font`.
pub(super) fn font_width(label: &str, size: f32, font: iced::Font) -> f32 {
    use iced::advanced::text::Paragraph as _;
    iced::advanced::graphics::text::Paragraph::with_text(iced::advanced::Text {
        content: label,
        bounds: iced::Size::INFINITE,
        size: iced::Pixels(size),
        line_height: text::LineHeight::default(),
        font,
        align_x: text::Alignment::Default,
        align_y: iced::alignment::Vertical::Top,
        shaping: text::Shaping::default(),
        wrapping: text::Wrapping::None,
    })
    .min_width()
}

// Keep every hover label readable and visually consistent. Menus have their
// own overlays and do not use this wrapper.
pub(super) fn hover_hint<'a>(
    content: impl Into<Element<'a, Message>>,
    label: impl Into<std::borrow::Cow<'a, str>>,
    position: tooltip::Position,
) -> tooltip::Tooltip<'a, Message> {
    let label = label.into();
    // Key symbols, as in "Redo · ⇧⌘Z", use the shortcut font.
    let label: Element<'a, Message> = if label.contains(super::shortcuts::symbol) {
        rich_text(super::shortcuts::spans(&label)).size(12).into()
    } else {
        // File paths in hints can hold words wider than the tooltip.
        text(label)
            .size(12)
            .wrapping(text::Wrapping::WordOrGlyph)
            .into()
    };
    hint_tooltip(content, label, position)
}

/// A hover hint whose first line ends with a shortcut, drawn in the
/// shortcut font, so a long hint cannot wrap the shortcut away from its name.
pub(super) fn hover_keys<'a>(
    content: impl Into<Element<'a, Message>>,
    hint: &str,
    keys: Option<String>,
    position: tooltip::Position,
) -> tooltip::Tooltip<'a, Message> {
    let (title, detail) = hint.split_once('\n').unwrap_or((hint, ""));
    hint_tooltip(content, keyed_text(title, keys, detail), position)
}

/// `title · keys` with the shortcut in the shortcut font, then `detail` on
/// the next line if it is not empty.
pub(super) fn keyed_text<'a>(
    title: &str,
    keys: Option<String>,
    detail: &str,
) -> Element<'a, Message> {
    let mut spans: Vec<text::Span<'a>> = vec![span(title.to_owned())];
    if let Some(keys) = keys {
        spans.push(span(" · "));
        spans.extend(super::shortcuts::spans(&keys));
    }
    if !detail.is_empty() {
        spans.push(span(format!("\n{detail}")));
    }
    rich_text(spans).size(12).into()
}

fn hint_tooltip<'a>(
    content: impl Into<Element<'a, Message>>,
    label: impl Into<Element<'a, Message>>,
    position: tooltip::Position,
) -> tooltip::Tooltip<'a, Message> {
    tooltip(
        content,
        container(label).max_width(300).padding([7, 10]).style(tip),
        position,
    )
    .padding(0)
    .gap(7)
    .delay(std::time::Duration::from_millis(500))
    .snap_within_viewport(true)
}

/// A numeric input with its unit inside the right edge of the field.
pub(super) fn unit_field<'a>(
    input: iced::widget::TextInput<'a, Message>,
    unit: &'a str,
) -> Element<'a, Message> {
    iced::widget::stack![
        input
            .size(12)
            .padding(iced::Padding {
                top: 5.,
                right: 18.,
                bottom: 5.,
                left: 5.,
            })
            .width(Length::Fill),
        // The degree sign is too small to see at the other units' size.
        container(
            text(unit)
                .size(if unit == "°" { 16 } else { 11 })
                .style(muted_text)
        )
        .padding([0, 4])
        .align_right(Length::Fill)
        .center_y(Length::Fill),
    ]
    .width(Length::Fill)
    .into()
}

pub(super) fn accessible_unit_field<'a>(
    input: reshiki::accessibility::TextInput<'a, Message>,
    unit: &'a str,
) -> Element<'a, Message> {
    iced::widget::stack![
        input
            .size(12)
            .padding(iced::Padding {
                top: 5.,
                right: 18.,
                bottom: 5.,
                left: 5.,
            })
            .width(Length::Fill),
        // The degree sign is too small to see at the other units' size.
        container(
            text(unit)
                .size(if unit == "°" { 16 } else { 11 })
                .style(muted_text)
        )
        .padding([0, 4])
        .align_right(Length::Fill)
        .center_y(Length::Fill),
    ]
    .width(Length::Fill)
    .into()
}

pub(super) fn command(label: &str, message: Message) -> button::Button<'_, Message> {
    button(text(label).size(12))
        .padding([7, 9])
        .on_press(message)
        .style(control(false))
}
/// `title · keys` with the shortcut of `message`, for plain-text hints.
pub(super) fn keyed(title: &str, message: &Message) -> String {
    match super::shortcuts::label(message) {
        Some(keys) => format!("{title} · {keys}"),
        None => title.to_owned(),
    }
}
/// A `command` labeled `label · keys` with the command's shortcut, in the
/// notation of the menus.
pub(super) fn keyed_command<'a>(label: &str, message: Message) -> button::Button<'a, Message> {
    button(keyed_text(label, super::shortcuts::label(&message), ""))
        .padding([7, 9])
        .on_press(message)
        .style(control(false))
}
fn icon_button(
    icon: Icon,
    hint: impl Into<std::borrow::Cow<'static, str>>,
    message: Option<Message>,
    active: bool,
) -> Element<'static, Message> {
    icon_button_at(icon, hint, message, active, tooltip::Position::Bottom)
}
fn icon_button_at(
    icon: Icon,
    hint: impl Into<std::borrow::Cow<'static, str>>,
    message: Option<Message>,
    active: bool,
    position: tooltip::Position,
) -> Element<'static, Message> {
    let hint = hint.into();
    let id = match icon {
        Icon::New => "header-new",
        Icon::Open => "header-open",
        Icon::Save => "header-save",
        Icon::SaveAs => "header-save-as",
        Icon::Assistant(_) => "header-assistant",
        Icon::Check => "header-check",
        Icon::Cleanup => "header-cleanup",
        Icon::Undo => "header-undo",
        Icon::Redo => "header-redo",
        Icon::Import => "header-import",
        Icon::Export => "header-export",
        Icon::Inspector => "header-inspector",
        Icon::Keyboard => "help-open",
        _ => "header-other",
    };
    hover_hint(
        reshiki::accessibility::button(
            id,
            hint.to_string(),
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
pub(super) fn section(label: &str) -> iced::widget::Text<'_> {
    text(label).size(10).style(muted_text)
}
fn ink() -> Color {
    Color::from_rgb8(37, 43, 51)
}
pub(super) fn muted_text(theme: &Theme) -> iced::widget::text::Style {
    iced::widget::text::Style {
        color: Some(crate::appearance::muted(theme)),
    }
}
pub(super) fn muted() -> Color {
    Color::from_rgb8(107, 116, 127)
}
/// The dropdown arrow used by pick lists, for buttons that open menus.
pub(super) fn caret(size: f32) -> iced::widget::Text<'static> {
    use iced::advanced::text::Renderer as _;
    text(iced::Renderer::ARROW_DOWN_ICON.to_string())
        .font(iced::Renderer::ICON_FONT)
        .size(size)
        .shaping(text::Shaping::Basic)
}
pub(super) fn divider() -> Element<'static, Message> {
    container(container(Space::new().width(1).height(20)).style(|theme| {
        crate::appearance::container(
            theme,
            container::Style {
                background: Some(Color::from_rgb8(223, 227, 232).into()),
                ..Default::default()
            },
        )
    }))
    .padding([0, 5])
    .into()
}
pub(super) fn horizontal_line() -> Element<'static, Message> {
    container(
        container(Space::new().height(1).width(Length::Fill)).style(|theme| {
            crate::appearance::container(
                theme,
                container::Style {
                    background: Some(Color::from_rgb8(230, 233, 237).into()),
                    ..Default::default()
                },
            )
        }),
    )
    .padding([7, 0])
    .into()
}
/// Theme colors belong on the tile; symbols retain strong interface contrast.
/// Use the interface's palette lightness when canvas and chrome modes differ.
pub(super) fn element_control<'a>(
    active: bool,
    doc: &'a reshiki::document::Document,
    symbol: &'a str,
) -> impl Fn(&Theme, button::Status) -> button::Style + 'a {
    move |theme, status| {
        use reshiki::canvas_theme::CanvasTheme;
        let mut style = control(active)(theme, status);
        if active {
            style.border.width = 2.;
        }
        let mode = if crate::appearance::is_dark(theme) {
            CanvasTheme::Dark
        } else {
            CanvasTheme::Light
        };
        if let Some(rgb) = reshiki::canvas_theme::element_swatch(doc, symbol, mode)
            .filter(|_| status != button::Status::Disabled)
        {
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            let background = crate::appearance::from_rgb(reshiki::color_contrast::tile(
                rgb,
                mode.is_dark(),
                active,
                hovered,
            ));
            style.background = Some(background.into());
            if active {
                style.border.color = crate::appearance::focus_border(theme, background);
            }
            style.text_color = theme.palette().text;
        }
        style
    }
}

pub(super) fn control(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let disabled = matches!(status, button::Status::Disabled);
        if crate::appearance::is_dark(theme) && !disabled {
            return button::Style {
                background: Some(
                    if active {
                        Color::from_rgb8(29, 64, 55)
                    } else if hovered {
                        Color::from_rgb8(38, 46, 52)
                    } else {
                        Color::TRANSPARENT
                    }
                    .into(),
                ),
                text_color: if active {
                    Color::from_rgb8(162, 230, 207)
                } else {
                    theme.palette().text
                },
                border: Border {
                    color: if active {
                        Color::from_rgb8(82, 193, 163)
                    } else {
                        Color::TRANSPARENT
                    },
                    width: 1.,
                    radius: 6.0.into(),
                },
                ..Default::default()
            };
        }
        crate::appearance::button(
            theme,
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
                        crate::appearance::focus_border(theme, Color::from_rgb8(222, 240, 234))
                    } else {
                        Color::TRANSPARENT
                    },
                    width: 1.,
                    radius: 5.0.into(),
                },
                ..Default::default()
            },
        )
    }
}
pub(super) fn panel(theme: &Theme) -> container::Style {
    crate::appearance::container(
        theme,
        container::Style {
            background: Some(Color::from_rgb8(250, 251, 252).into()),
            border: Border {
                color: Color::from_rgb8(224, 228, 233),
                width: 1.,
                radius: 0.0.into(),
            },
            ..Default::default()
        },
    )
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
/// The context row's tool name and the short form a short row uses.
fn tool_name(tool: Tool) -> (&'static str, &'static str) {
    let name = match tool {
        Tool::Select => return ("Select / move", "Select"),
        Tool::Lasso => return ("Lasso select", "Lasso"),
        Tool::Chain(reshiki::chains::ChainMode::Straight) => return ("Straight chain", "Chain"),
        // "Max atoms" beside it still marks the snaking mode.
        Tool::Chain(_) => return ("Snaking chain", "Chain"),
        Tool::Tilt => "3D tilt",
        Tool::Atom => "Atom label",
        // The bond pick list beside it names the preset.
        Tool::Bond(_) | Tool::StyledBond(_) | Tool::Wedge | Tool::Hash | Tool::Wavy => "Bond",
        Tool::Ring | Tool::RingPreset(_) => "Ring",
        Tool::Template => "Template",
        Tool::Arrow => "Reaction arrow",
        Tool::Text => "Text label",
        Tool::Erase => "Eraser",
        Tool::Graphic(_) => "Drawing object",
        Tool::EditPoints => "Edit points",
    };
    (name, name)
}

#[cfg(test)]
mod selection_tests;
