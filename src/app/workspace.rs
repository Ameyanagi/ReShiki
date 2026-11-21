use super::{
    App, InspectorTab, Message,
    icons::{Glyph, Icon},
    shortcuts::keys,
};
use crate::canvas::layered::canvas;
use crate::canvas::{ArrowOptions, Edit, GraphicOptions, MoleculeCanvas, Tool};
use iced::widget::{
    Space, button, checkbox, column, combo_box, container, mouse_area, responsive, rich_text, row,
    scrollable, sensor, span, text, tooltip,
};
use iced::{Alignment, Border, Color, Element, Length, Theme, keyboard::Modifiers};
use reshiki::bonds::BondPreset;
use reshiki::typography::{Script, StyleChange};

#[cfg(test)]
mod cleanup_tests;
#[cfg(test)]
pub(super) mod layout_snapshots;
#[cfg(test)]
mod model_controls_qa;
#[cfg(test)]
mod model_face_qa;
#[cfg(test)]
mod selection_canvas_qa;
#[cfg(test)]
mod status_tests;
#[cfg(test)]
mod template_tests;
#[cfg(test)]
mod view_parity_tests;

/// Counts used by molecular properties and both naming preview layouts.
pub(super) fn atom_bond_counts(atoms: usize, bonds: usize) -> String {
    format!(
        "{atoms} atom{} · {bonds} bond{}",
        if atoms == 1 { "" } else { "s" },
        if bonds == 1 { "" } else { "s" }
    )
}

impl App {
    fn keyboard_context_summary(&self) -> Option<String> {
        use reshiki::keyboard_drawing::Target;
        self.tab.keyboard_drawing.marker_point(&self.tab.doc)?;
        Some(match self.tab.keyboard_drawing.target() {
            Target::Blank(_) => "Empty position".into(),
            Target::Atom(id) => self.tab.doc.atom(id).map_or_else(
                || "Empty position".into(),
                |atom| format!("{} #{id}", atom.element),
            ),
            Target::Bond(a, b) => format!("Bond {a}–{b}"),
        })
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
        let script = self.toolbar_script();
        let toggle = |label: &'static str,
                      help: &'static str,
                      active: bool,
                      change: StyleChange|
         -> Element<'_, Message> {
            hover_hint(
                reshiki::accessibility::button(
                    format!("text-style-{}", help.to_lowercase().replace(' ', "-")),
                    help,
                    text(label).size(14),
                )
                .padding([5, 8])
                .style(control(active))
                .checked(active)
                .on_press(Message::TextStyle(change)),
                help,
                tooltip::Position::Bottom,
            )
            .into()
        };
        let mut tools = row![
            text("STYLE").size(10).style(muted_text),
            reshiki::accessibility::select(
                "text-style-font",
                "Font",
                reshiki::style::font_families().to_vec(),
                style.family.clone(),
                |family| Message::TextStyle(StyleChange::Family(family.into())),
                combo_box(
                    &self.font_options,
                    "Search fonts…",
                    Some(&style.family),
                    |family| Message::TextStyle(StyleChange::Family(family))
                )
                .id("text-style-font")
                .width(152)
                .input_style(crate::appearance::input_style)
                .menu_style(crate::appearance::dropdown_menu)
                .size(12)
                .padding(6),
            ),
            hover_hint(
                reshiki::accessibility::text_input(
                    "text-style-size",
                    "Font size in points",
                    "pt",
                    &self.tab.font_size_input
                )
                .style(crate::appearance::input_style)
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
                script == Script::Subscript,
                StyleChange::Script(if script == Script::Subscript {
                    Script::Normal
                } else {
                    Script::Subscript
                })
            ),
            toggle(
                "x²",
                "Superscript",
                script == Script::Superscript,
                StyleChange::Script(if script == Script::Superscript {
                    Script::Normal
                } else {
                    Script::Superscript
                })
            ),
            divider(),
        ]
        .spacing(4)
        .align_y(Alignment::Center);
        tools = tools.push(self.alignment_menu());
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
            if self.label_click_through() {
                // Only the drawing stays live; a click anywhere else commits.
                iced::widget::mouse_area(reshiki::accessibility::inert(element))
                    .on_press(Message::AtomText(super::atom_text::Action::Apply))
                    .into()
            } else if self.context_menu.is_some() {
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
            sensor(super::atom_text::canvas_anchor(self.with_context_menu(
                self.with_inline_text(self.with_star_prompt(
                    self.with_start(self.with_model_notice(drawing.map(Message::Canvas))),
                )),
            )))
            .on_show(Message::Viewport)
            .on_resize(Message::Viewport)
            .into(),
        );
        #[cfg(target_arch = "wasm32")]
        // The stack stays when the notice goes, so the canvas keeps its state.
        let paper: Element<'_, Message> = iced::widget::Stack::with_children([paper])
            .extend(self.recovery_notice())
            .into();
        let workspace = column![background(self.context_bar()), paper]
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
            nmr: self.nmr_canvas(),
            locant_highlight: self
                .pulse_atoms()
                .unwrap_or_else(|| self.naming_canvas_highlight()),
            optimizer: self.optimization_canvas(),
            keyboard_target: if self.keyboard_drawing_active() {
                self.tab.keyboard_drawing.point(self.display_document())
            } else {
                None
            },
            keyboard_marker: self.tab.keyboard_drawing.shown(),
            morph: self.model_morph(),
            model_generation: self.model_generation(),
            element: self.atom_tool_element(),
            atom_label: (self.element == super::palettes::R_GROUP)
                .then_some(super::palettes::R_GROUP),
            joining: self.tab.joining.as_ref().map(|s| (&s.prepared, s.anchor)),
            hidden_annotation: self.inline_label_id(),
            bond_drawing: self.tab.bond_drawing,
            chain_drawing: self.tab.chain_drawing,
            graphics: GraphicOptions {
                constrain: self.toolbar.graphic(self.tool).is_some_and(|p| p.constrain),
                arc: self.tab.arc_editor.geometry,
                style: &self.tab.graphic_style,
                point: self.tab.path_point,
                phase: self.tab.orbital_phase,
                flipped: self.tab.phase_flipped,
                attach: self.tab.attach_symbols,
                snap: self.tab.snap_orbitals,
                bracket_sides: self.tab.bracket_sides,
                superscript: self
                    .tab
                    .bracket_superscript
                    .text(&self.tab.bracket_superscript_input),
                revision: self.tab.revision,
            },
            arrows: ArrowOptions {
                preset: self.tab.arrow_style,
                style: &self.tab.arrows.style,
                source: self.tab.arrows.source(),
                attach_targets: self.tab.arrows.attach_targets,
            },
            doc: self.display_document(),
            reviewing_pair: self.tab.mapping.review_pair.is_some() && self.tool.selects(),
            selected: if self.tab.inline_text.is_some() {
                &[]
            } else {
                &self.tab.selected
            },
            tool: self.tool,
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
        .shifted(crate::canvas::State::shifts)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    /// Clean up runs at once with the current options, which its ▾ menu sets.
    fn cleanup_button(&self) -> Element<'_, Message> {
        use super::cleanup::Action;
        use reshiki::cleanup::{Orientation, Scope};
        use reshiki::style::units::{self, Unit};
        let open = self.cleanup_menu;
        let run = Message::Cleanup(Action::Begin);
        let main = icon_button(
            Icon::Cleanup,
            keyed("Clean up", &run),
            (!self.tab.busy).then_some(run.clone()),
            false,
        );
        let options = hover_hint(
            reshiki::accessibility::button(
                "header-cleanup-options",
                "Clean up options",
                container(caret(10.)).center(Length::Fill),
            )
            .width(16)
            .height(36)
            .padding(0)
            .style(control(open))
            .expanded(open)
            .on_press(Message::Cleanup(Action::Menu(!open))),
            "Clean up options",
            tooltip::Position::Bottom,
        );
        let popup = open.then(|| {
            let chosen = self.cleanup_options;
            let item = |id: &'static str, label: String, on: bool, action| {
                reshiki::accessibility::button(
                    id,
                    label.clone(),
                    row![
                        text(if on { "✓" } else { "" }).size(12).width(14),
                        text(label).size(12)
                    ]
                    .align_y(Alignment::Center),
                )
                .checked(on)
                .padding([6, 8])
                .width(Length::Fill)
                .style(control(false))
                .on_press(Message::Cleanup(action))
            };
            let orientation = |id, label: &str, mode| {
                item(
                    id,
                    label.into(),
                    chosen.orientation() == mode,
                    Action::Orientation(mode),
                )
            };
            let scope = |id, label: &str, scope| {
                item(
                    id,
                    label.into(),
                    chosen.scope == scope,
                    Action::Scope(scope),
                )
            };
            let on = self.appearance.cleanup_style_bond_length;
            let points = self.tab.doc.drawing_style.bond_length_pt;
            let length = units::format((points * 100.).round() / 100., Unit::Points);
            container(
                column![
                    container(section("Orientation")).padding([4, 8]),
                    orientation("cleanup-keep", "Keep", Orientation::Keep),
                    orientation("cleanup-free", "Free", Orientation::Free),
                    orientation("cleanup-iupac", "IUPAC fused rings", Orientation::Iupac),
                    container(section("With a selection")).padding([4, 8]),
                    scope("cleanup-atoms", "Selected atoms", Scope::SelectedAtoms),
                    scope(
                        "cleanup-molecules",
                        "Whole molecules",
                        Scope::SelectedMolecules
                    ),
                    container(section("Bond length")).padding([4, 8]),
                    item(
                        "cleanup-style-length",
                        format!("Also set bond length to the style ({length} pt)"),
                        on,
                        Action::StyleBondLength(!on),
                    ),
                    container(section("Reaction")).padding([4, 8]),
                    item(
                        "cleanup-reaction",
                        keyed("Clean up reaction", &Message::Cleanup(Action::Scheme)),
                        false,
                        Action::Scheme,
                    ),
                ]
                .spacing(1),
            )
            .width(300)
            .padding(5)
            .style(super::color_popover::surface)
            .into()
        });
        Element::new(
            super::popover::popover(
                row![main, options].align_y(Alignment::Center),
                popup,
                Message::Cleanup(Action::Menu(false)),
            )
            .align_end(),
        )
    }

    fn check_button(&self) -> Element<'_, Message> {
        use super::stereo::Action;
        let findings = self.tab.stereo_notes.len();
        let open = self.check_results && findings > 0;
        let hint = match findings {
            _ if self.tab.busy => "Checking…".to_owned(),
            0 => "Check drawing".to_owned(),
            1 => "Check drawing · 1 finding".to_owned(),
            n => format!("Check drawing · {n} findings"),
        };
        let message = match findings {
            _ if self.tab.busy => None,
            0 => Some(Message::Analyze),
            _ => Some(Message::Stereo(Action::Results(!open))),
        };
        let mut button = reshiki::accessibility::button(
            "header-check",
            hint.clone(),
            iced::widget::canvas(Glyph(Icon::Check, message.is_some()))
                .width(24)
                .height(24),
        )
        .width(36)
        .height(36)
        .padding(6)
        .style(control(open))
        .on_press_maybe(message);
        if findings > 0 {
            button = button.expanded(open);
        }
        let mut anchor = iced::widget::stack![button];
        if findings > 0 {
            anchor = anchor.push(
                container(
                    container(text(findings.to_string()).size(9).line_height(1.))
                        .padding([2, 4])
                        .style(|theme: &Theme| container::Style {
                            background: Some(
                                crate::appearance::themed(theme, Color::from_rgb8(168, 52, 47))
                                    .into(),
                            ),
                            text_color: Some(Color::WHITE),
                            border: Border {
                                color: theme.palette().background,
                                width: 1.5,
                                radius: 7.0.into(),
                            },
                            ..Default::default()
                        }),
                )
                .width(Length::Fill)
                .align_right(Length::Fill),
            );
        }
        let anchor: Element<'_, Message> = if open {
            anchor.into()
        } else {
            hover_hint(anchor, hint, tooltip::Position::Bottom).into()
        };
        let popup = open.then(|| {
            let header = row![
                section("Findings"),
                Space::new().width(Length::Fill),
                accessible_command("check-again", "Check drawing again", "Check again")
                    .padding([4, 8])
                    .style(crate::appearance::secondary)
                    .on_press_maybe((!self.tab.busy).then_some(Message::Analyze)),
            ]
            .padding(iced::Padding::ZERO.left(8).bottom(4))
            .align_y(Alignment::Center);
            // The findings scroll under the header and above their fixes,
            // which stay in reach at the minimum window size.
            let findings = scrollable(self.stereo_findings(column![].spacing(2), "check"))
                .direction(scrollable::Direction::Vertical(
                    scrollable::Scrollbar::new()
                        .width(3)
                        .scroller_width(3)
                        .spacing(3),
                ));
            let mut list =
                column![header, container(findings).max_height(FINDINGS_HEIGHT)].spacing(2);
            if let Some(fixes) = self.stereo_fixes("check") {
                list = list.push(fixes);
            }
            container(list)
                .width(320)
                .padding(8)
                .style(super::color_popover::surface)
                .into()
        });
        Element::new(
            super::popover::popover(anchor, popup, Message::Stereo(Action::Results(false)))
                .align_end(),
        )
    }

    fn command_bar(&self) -> Element<'_, Message> {
        // The browser build updates with its page and has no local assistant.
        let browser = cfg!(target_arch = "wasm32");
        let settings = Message::Settings(super::settings_page::Action::Open(None));
        let bar = row![
            hover_hint(
                reshiki::accessibility::button(
                    "header-about",
                    if browser {
                        "About ReShiki"
                    } else {
                        "About ReShiki; check for updates"
                    },
                    crate::branding::wordmark(21.0)
                )
                .padding(0)
                .style(button::text)
                .on_press(Message::Updates(super::updates::Action::Show(true))),
                if browser {
                    "About ReShiki"
                } else {
                    "About ReShiki · Check for updates"
                },
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
                keyed(super::files::SAVE_AS, &Message::SaveAs),
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
            (!browser).then(|| icon_button(
                Icon::Assistant(self.assistant.busy),
                if self.assistant.busy {
                    "Assistant · Working"
                } else {
                    "Assistant"
                },
                Some(Message::Assistant(super::assistant::Action::Open)),
                self.inspector_open && self.inspector_tab == InspectorTab::Assistant
            )),
            icon_button(
                Icon::Import,
                keyed("Import", &Message::Inspector(InspectorTab::Import)),
                Some(Message::Inspector(InspectorTab::Import)),
                self.inspector_open && self.inspector_tab == InspectorTab::Import
            ),
            self.check_button(),
            self.cleanup_button(),
            icon_button(
                Icon::Export,
                keyed("Export", &Message::Inspector(InspectorTab::Export)),
                Some(Message::Inspector(InspectorTab::Export)),
                self.inspector_open && self.inspector_tab == InspectorTab::Export
            ),
            icon_button(
                Icon::Settings,
                keyed("Settings", &settings),
                Some(settings.clone()),
                self.settings.open
            ),
            icon_button(
                Icon::Inspector,
                "Show/hide side panel",
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
                    "Select / move · Esc or toolbar"
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
            (Tool::Erase, "Eraser"),
            (Tool::Tilt, "3D tilt"),
            (Tool::Bond(1), single.as_str()),
            (Tool::Bond(2), double.as_str()),
            (Tool::Bond(3), triple.as_str()),
            (self.toolbar.bond, other_bonds.as_str()),
            (Tool::Atom, atom.as_str()),
            (self.toolbar.ring, ring.as_str()),
            (Tool::Chain(reshiki::chains::ChainMode::Straight), &chain),
            (
                Tool::Chain(reshiki::chains::ChainMode::Snaking),
                "Snaking chain",
            ),
            (Tool::Arrow, arrow.as_str()),
            (Tool::Text, "Text label · t"),
            (Tool::Graphic(G::Line), "Graphic line"),
            (Tool::Graphic(G::Arc), "Arc"),
            (Tool::Graphic(G::Curve), "Bézier curve"),
            (Tool::Graphic(G::Path), "Pen path"),
            (
                Tool::Graphic(self.toolbar.rectangle.kind),
                rectangle.as_str(),
            ),
            (Tool::Graphic(self.toolbar.ellipse.kind), ellipse.as_str()),
            (Tool::Graphic(self.toolbar.bracket.kind), brackets.as_str()),
            (self.toolbar.symbol, symbols.as_str()),
            (self.toolbar.orbital, orbitals.as_str()),
        ];
        let mut grid = column![].spacing(TOOL_GAP);
        for pair in tools.chunks(2) {
            let mut line = row![].spacing(4);
            for (tool, hint) in pair {
                line = line.push(self.tool_palette_button(*tool, hint));
            }
            grid = grid.push(line);
        }
        let mut atoms = column![section("Atoms")].spacing(ATOM_GAP);
        for line in PALETTE_ELEMENTS {
            atoms = atoms.push(
                row(line
                    .iter()
                    .map(|&symbol| self.element_palette_button(symbol).into()))
                .spacing(ATOM_GAP),
            );
        }
        // Tools and atoms scroll together, so either list can grow. The
        // area spans the panel's full width to keep its scrollbar right of
        // the atoms; the bottom padding lets the last row clear the fade.
        let tools = scrollable(
            container(
                column![grid, atoms.width(3. * ATOM_BUTTON + 2. * ATOM_GAP)]
                    .spacing(10)
                    .align_x(Alignment::Center),
            )
            .center_x(Length::Fill)
            .padding(iced::Padding::ZERO.bottom(PALETTE_FADE)),
        )
        .height(Length::Fill)
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::new()
                .width(6)
                .scroller_width(6)
                .margin(1),
        ));
        let fade = column![
            Space::new().height(Length::Fill),
            container(
                container(Space::new())
                    .width(Length::Fill)
                    .height(PALETTE_FADE)
                    .style(palette_fade)
            )
            // Keep the scrollbar clear of the fade.
            .padding([0, 8]),
        ];
        let palette = column![
            iced::widget::stack![tools, fade].height(Length::Fill),
            self.help_palette_button()
        ]
        .spacing(10)
        .align_x(Alignment::Center);
        container(palette)
            .width(PALETTE_WIDTH)
            .height(Length::Fill)
            .padding([14, 0])
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
        .width(TOOL_BUTTON)
        .height(TOOL_BUTTON);
        let item = reshiki::accessibility::button(format!("tool-{tool:?}"), hint.to_owned(), item)
            .padding(0)
            .width(TOOL_BUTTON)
            .height(TOOL_BUTTON)
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
                // The asterisk sits high and small at the label size.
                text(symbol)
                    .size(if symbol == "*" { 18 } else { 13 })
                    .center(),
            )
            .checked(self.tool == Tool::Atom && self.element == symbol)
            .width(Length::Fill)
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
            format!("Help · {}", super::help::registry::finder_keys()),
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
            Tool::Graphic(reshiki::graphics::GraphicKind::Orbital(_)) => {
                let chooser = reshiki::scientific::OrbitalKind::ALL
                    .iter()
                    .map(|kind| text_width(&kind.to_string(), 12.))
                    .fold(0., f32::max);
                // snap_orbitals_toggle: 14 px box, 8 px gap and its label.
                let label = text_width(super::graphics::SNAP_ORBITALS_LABEL, 11.);
                lead + chooser + 12. + 15. + CONTEXT_GAP + 22. + label
            }
            Tool::EditPoints
                if self.selected_path().is_some() || self.selected_node_arrow().is_some() =>
            {
                lead + ["Corner", "Smooth", "Symmetric", "Done"]
                    .map(|label| text_width(label, 12.) + 18.)
                    .into_iter()
                    .sum::<f32>()
                    + 3. * CONTEXT_GAP
            }
            _ if self.moving_bonded_selection() => lead + Self::constraints_width(),
            _ => 0.,
        }
    }

    /// A partial selection drags bonded atoms along, following Length / Angles.
    fn moving_bonded_selection(&self) -> bool {
        matches!(self.tool, Tool::Select | Tool::Lasso) && {
            // The responsive row asks this more than once. Index the expanded
            // IDs so a whole-drawing selection does not scan them per bond.
            let selected: std::collections::HashSet<_> = self
                .tab
                .doc
                .expand_abbreviation_selection(&self.tab.selected)
                .into_iter()
                .collect();
            self.tab
                .doc
                .bonds
                .iter()
                .any(|bond| selected.contains(&bond.a) != selected.contains(&bond.b))
        }
    }

    /// Context row commands, most important first: a short row folds them
    /// into ⋯ from the end. Selection commands come before global ones.
    pub(super) fn context_commands(&self) -> Vec<RowCommand> {
        let mut commands = Vec::new();
        let constraints = self.tool.bond_preset().is_some()
            || matches!(self.tool, Tool::Chain(_))
            || self.moving_bonded_selection();
        if constraints && self.bond_drawing_changed() {
            commands.push(RowCommand {
                label: "Reset",
                menu: "Reset bonds",
                message: Message::ResetBondDrawing,
                enabled: true,
            });
        }
        if !matches!(self.tool, Tool::Select | Tool::Lasso) {
            return commands;
        }
        let keyboard = self.keyboard_drawing_active();
        if keyboard {
            commands.extend(self.keyboard_drawing_commands());
        }
        if !self.tab.selected.is_empty() {
            commands.extend(self.selected_join_commands());
            commands.extend(self.selected_attach_commands());
            commands.extend(self.path_head_commands());
            commands.push(RowCommand {
                label: "Group",
                menu: "Group",
                message: Message::Group,
                enabled: self.can_group(),
            });
            if !self
                .tab
                .doc
                .outer_selected_groups(&self.tab.selected)
                .is_empty()
            {
                commands.push(RowCommand {
                    label: "Ungroup",
                    menu: "Ungroup",
                    message: Message::Ungroup,
                    enabled: true,
                });
            }
        }
        commands.extend(self.depth_commands());
        if !keyboard {
            commands.push(RowCommand {
                label: "Keyboard drawing",
                menu: "Keyboard drawing",
                message: Message::KeyboardDrawing(super::keyboard_drawing::Action::Toggle),
                enabled: !self.tab.busy,
            });
        }
        commands.push(RowCommand {
            label: "3D optimize…",
            menu: "3D optimize…",
            message: Message::Optimization(super::optimization::Action::Begin),
            enabled: !self.tab.doc.atoms.is_empty() && !self.tab.busy,
        });
        commands
    }

    /// Mark appears once an atom is the target; Connect and Coordinate once
    /// an atom is marked, and they need a different target atom.
    fn keyboard_drawing_commands(&self) -> Vec<RowCommand> {
        use super::keyboard_drawing::Action;
        use reshiki::keyboard_drawing::Target;
        let atom = match self.tab.keyboard_drawing.target() {
            Target::Atom(id) => Some(id),
            _ => None,
        };
        let marked = self.tab.keyboard_drawing.marked();
        let mut commands = Vec::new();
        if atom.is_some() {
            commands.push(RowCommand {
                label: "Mark",
                menu: "Mark atom",
                message: Message::KeyboardDrawing(Action::Mark),
                enabled: true,
            });
        }
        if marked.is_some() {
            let enabled = atom.zip(marked).is_some_and(|(a, b)| a != b);
            commands.extend([
                RowCommand {
                    label: "Connect",
                    menu: "Connect to marked atom",
                    message: Message::KeyboardDrawing(Action::Connect),
                    enabled,
                },
                RowCommand {
                    label: "Coordinate",
                    menu: "Coordinate marked donor to metal",
                    message: Message::KeyboardDrawing(Action::Coordinate),
                    enabled,
                },
            ]);
        }
        commands
    }

    /// Arrowheads for a selected open path.
    fn path_head_commands(&self) -> Vec<RowCommand> {
        use reshiki::arrows::Head;
        let Some(graphic) = self.selected_open_paths().next() else {
            return vec![];
        };
        let heads = graphic.heads.unwrap_or_default();
        [(true, heads.tail), (false, heads.head)]
            .into_iter()
            .map(|(start, current)| {
                let add = current == Head::None;
                let label = match (start, add) {
                    (true, true) => "Start head",
                    (true, false) => "No start head",
                    (false, true) => "End head",
                    (false, false) => "No end head",
                };
                let head = if add { Head::Full } else { Head::None };
                RowCommand {
                    label,
                    menu: label,
                    message: Message::Graphics(if start {
                        super::graphics::Action::Tail(head)
                    } else {
                        super::graphics::Action::Head(head)
                    }),
                    enabled: true,
                }
            })
            .collect()
    }

    /// Depth appearance of a tilted drawing.
    fn depth_commands(&self) -> Vec<RowCommand> {
        use super::depth_appearance::Action;
        let ids = self.depth_ids();
        let shown = reshiki::depth_appearance::has(&self.tab.doc, &ids);
        if !shown
            && !ids
                .iter()
                .any(|id| self.tab.doc.atom(*id).is_some_and(|a| a.depth != 0.))
        {
            return vec![];
        }
        let automatic = reshiki::depth_appearance::is_automatic_for(&self.tab.doc, &ids);
        let mut commands = vec![if automatic {
            RowCommand {
                label: "Freeze depth",
                menu: "Freeze depth appearance",
                message: Message::DepthAppearance(Action::Enhance(false)),
                enabled: true,
            }
        } else {
            RowCommand {
                label: "Enhance depth",
                menu: "Enhance depth appearance",
                message: Message::DepthAppearance(Action::Enhance(true)),
                enabled: true,
            }
        }];
        if shown {
            commands.push(RowCommand {
                label: "Clear depth",
                menu: "Clear depth appearance",
                message: Message::DepthAppearance(Action::Clear),
                enabled: true,
            });
            if !self.tab.selected.is_empty() {
                commands.push(RowCommand {
                    label: "Original ink",
                    menu: "Use original ink for selection",
                    message: Message::DepthAppearance(Action::OriginalInk),
                    enabled: true,
                });
            }
        }
        commands
    }

    pub(super) fn context_bar(&self) -> Element<'_, Message> {
        if self.tab.optimization.is_some() {
            return context_frame(
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
                        "Open 3D preview controls in Properties",
                        tooltip::Position::Bottom,
                    ),
                ]
                .spacing(CONTEXT_GAP)
                .align_y(Alignment::Center),
            );
        }
        if self.tab.joining.is_some() {
            return context_frame(self.join_bar());
        }
        context_frame(responsive(move |size| self.context_row(size.width)).height(Length::Shrink))
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
        let chips: Vec<Option<String>> = commands.iter().map(|c| c.keycap()).collect();
        let widths: Vec<f32> = commands
            .iter()
            .zip(&chips)
            .map(|(c, chip)| {
                text_width(c.label, 12.)
                    + 18.
                    + chip
                        .as_deref()
                        .map_or(0., |key| KEYCAP_GAP + keycap_width(key))
            })
            .collect();
        let select = matches!(self.tool, Tool::Select | Tool::Lasso);
        let arrange = (select && self.appearance.arrange_controls).then_some((
            super::object_toolbar::GROUP_WIDTH,
            super::object_toolbar::COMPACT_WIDTH,
        ));
        // The summary `selection_tool_options` shows: the keyboard target's
        // while its marker is shown, otherwise the selection's.
        let summary = if keyboard && let Some(summary) = self.keyboard_context_summary() {
            Some(summary)
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
        let shown = commands.len() - folded;
        let mut x = CONTEXT_PADDING + fit.fixed;
        for (index, ((c, w), chip)) in commands.iter().zip(&widths).zip(chips).enumerate() {
            if index == shown {
                break;
            }
            let mut label = row![text(c.label).size(12)]
                .spacing(KEYCAP_GAP)
                .align_y(Alignment::Center);
            if let Some(key) = chip {
                label = label.push(keycap(key, c.enabled));
            }
            row = row.push(hover_keys(
                reshiki::accessibility::button(
                    keyboard_control_id(&c.message)
                        .map_or_else(|| format!("context-command-{index}"), str::to_owned),
                    c.menu,
                    label,
                )
                .padding([7, 9])
                .style(control(false))
                .on_press_maybe(c.enabled.then(|| c.message.clone())),
                c.menu,
                super::shortcuts::label(&c.message),
                tooltip::Position::Bottom,
            ));
            x += CONTEXT_GAP + w;
        }
        if folded > 0 {
            let labels: Vec<_> = commands.iter().skip(shown).map(|c| c.menu).collect();
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
    /// name (`Tool::hint`). `summary` shows the selection summary of the
    /// selection tools.
    fn tool_options(
        &self,
        summary: bool,
    ) -> (Vec<Element<'_, Message>>, std::borrow::Cow<'static, str>) {
        let options = match self.tool {
            tool if tool.bond_preset().is_some() => {
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
                ]
            }
            // The tool name shows the mode; the palette switches it.
            Tool::Chain(mode) => self.chain_tool_options(mode),
            Tool::StretchBond { .. } => {
                vec![
                    reshiki::accessibility::button(
                        "stretch-done",
                        "Finish stretching bonds",
                        text("Done").size(12),
                    )
                    .padding([7, 9])
                    .on_press(Message::Tool(Tool::Select))
                    .style(control(false))
                    .into(),
                ]
            }
            Tool::Graphic(kind) => self.graphic_tool_options(kind),
            Tool::EditPoints => {
                let mut options = self.edit_points_node_options();
                options.push(
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
                );
                options
            }
            Tool::Template => {
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
                ]
            }
            Tool::Ring | Tool::RingPreset(_) => self.ring_tool_options(),
            Tool::Arrow => {
                vec![
                    crate::appearance::pick_list(
                        reshiki::arrows::Preset::ALL,
                        Some(self.tab.arrow_style),
                        Message::ArrowStyle,
                    )
                    .text_size(12)
                    .padding(5)
                    .into(),
                ]
            }
            Tool::Atom => {
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
                ]
            }
            Tool::Tilt => self.tilt_tool_options(),
            Tool::Select | Tool::Lasso => self.selection_tool_options(summary),
            _ => vec![],
        };
        // Ring keeps its size shortcut, drawn in the shortcut font.
        let hint = if matches!(
            self.tool,
            Tool::Ring | Tool::RingPreset(reshiki::rings::Preset::Regular)
        ) {
            format!(
                "{} · {} keeps ring size",
                self.tool.hint(),
                keys(Modifiers::SHIFT, "R")
            )
            .into()
        } else {
            self.tool.hint().into()
        };
        (options, hint)
    }

    fn chain_tool_options(&self, mode: reshiki::chains::ChainMode) -> Vec<Element<'_, Message>> {
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
        ]
    }

    fn graphic_tool_options(
        &self,
        kind: reshiki::graphics::GraphicKind,
    ) -> Vec<Element<'_, Message>> {
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
        if matches!(kind, G::Orbital(_)) {
            options.push(self.snap_orbitals_toggle());
        }
        options
    }

    fn ring_tool_options(&self) -> Vec<Element<'_, Message>> {
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
            return options;
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
        options
    }

    fn tilt_tool_options(&self) -> Vec<Element<'_, Message>> {
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
        vec![
            tilts.into(),
            hover_hint(
                command("Front bonds", depth.clone()).on_press_maybe(enabled.then_some(depth)),
                "Emphasize front bonds using the retained projection depth",
                tooltip::Position::Bottom,
            )
            .into(),
            done("Done"),
        ]
    }

    fn selection_tool_options(&self, summary: bool) -> Vec<Element<'_, Message>> {
        let mut options = Vec::new();
        if summary
            && self.keyboard_drawing_active()
            && let Some(summary) = self.keyboard_context_summary()
        {
            options.push(
                hover_hint(
                    text(summary).size(11).style(muted_text),
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
                        "Click a bond's middle to select it; Shift-click adds. {} selects the whole drawing.\nRotate: drag the round handle (Shift: 15° steps) · {}",
                        keys(Modifiers::COMMAND, "A"),
                        super::shortcuts::rotation_keys()
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
                    format!(
                        "Bonded movement follows Length / Angles · {}: free movement",
                        crate::platform::alt_key()
                    ),
                    tooltip::Position::Bottom,
                )
                .into(),
            );
        }
        options
    }

    /// One width for every tab, so the tab strip never moves under the pointer.
    pub(super) fn inspector_width(&self) -> f32 {
        if self.inspector_open {
            INSPECTOR_WIDTH
        } else {
            0.
        }
    }

    /// Records the viewport the canvas will get after the inspector changed
    /// from `old_width`, so `set_viewport` keeps the camera for that resize.
    pub(super) fn expect_inspector_width_change(&mut self, old_width: f32) {
        let new_width = self.inspector_width();
        if old_width != new_width {
            self.tab.inspector_ui.expect_viewport(iced::Size::new(
                self.viewport.width + old_width - new_width,
                self.viewport.height,
            ));
        }
    }

    /// A page change moves only the inspector's left edge. Keep the drawing
    /// anchored to the window, including when a handler already adjusted the
    /// viewport while revealing a selection or optimization controls.
    pub(super) fn preserve_inspector_page_position(
        &mut self,
        old_width: f32,
        old_viewport: iced::Size,
    ) {
        let change = old_width - self.inspector_width();
        if change == 0. {
            return;
        }
        let size = iced::Size::new(old_viewport.width + change, old_viewport.height);
        self.tab.camera.center.x +=
            (size.width - self.viewport.width) / (2. * self.tab.camera.zoom);
        self.viewport = size;
        self.tab.inspector_ui.expect_viewport(size);
    }

    /// The inspector's panel at the current tab's width.
    fn inspector_frame<'a>(
        &self,
        content: impl Into<Element<'a, Message>>,
    ) -> Element<'a, Message> {
        container(content)
            .width(self.inspector_width())
            .height(Length::Fill)
            .style(panel)
            .into()
    }

    fn inspector(&self) -> Element<'_, Message> {
        if self.inspector_tab == InspectorTab::Names {
            return self.inspector_frame(self.naming_inspector());
        }
        if self.inspector_tab == InspectorTab::Reactions {
            return self.reactions_inspector();
        }
        if self.inspector_tab == InspectorTab::DrawingStyle {
            return self.inspector_frame(self.drawing_style_panel());
        }
        if self.inspector_tab == InspectorTab::Assistant {
            return self.inspector_frame(self.assistant_panel());
        }
        let mut tabs = row![];
        for (label, tab) in INSPECTOR_TABS {
            let selected = self.inspector_tab == tab
                || (tab == InspectorTab::Properties
                    && matches!(
                        self.inspector_tab,
                        InspectorTab::Reactions
                            | InspectorTab::Labels
                            | InspectorTab::Abbreviations
                            | InspectorTab::Pages
                            | InspectorTab::DrawingStyle
                    ));
            tabs = tabs.push(
                reshiki::accessibility::button(
                    format!("inspector-tab-{tab:?}"),
                    format!("{label} tab"),
                    text(label).size(12).width(Length::Fill).center(),
                )
                .checked(selected)
                .width(Length::FillPortion(segment_portion(label)))
                .padding([4, 0])
                .style(segment(selected))
                .on_press(Message::Inspector(tab)),
            );
        }
        // A GitHub drawing's History: a clock just after the track, next in
        // focus order.
        #[cfg(not(target_arch = "wasm32"))]
        let history = self
            .github
            .locate(self.tab.path.as_deref())
            .is_some()
            .then(|| {
                let selected = self.inspector_tab == InspectorTab::Versions;
                hover_hint(
                    reshiki::accessibility::button(
                        "inspector-history",
                        "History",
                        iced::widget::canvas(Glyph(Icon::History, true))
                            .width(14)
                            .height(14),
                    )
                    .checked(selected)
                    .padding((HISTORY_BUTTON - 14.) / 2.)
                    .style(segment(selected))
                    .on_press(Message::GitHub(super::github::Action::ShowVersions)),
                    "History",
                    tooltip::Position::Bottom,
                )
            });
        #[cfg(target_arch = "wasm32")]
        let history: Option<Element<'_, Message>> = None;
        let header = row![
            container(tabs)
                .padding(TRACK_INSET)
                .width(Length::Fill)
                .style(segment_track),
            history
        ]
        .spacing(HISTORY_GAP)
        .align_y(iced::Alignment::Center);
        let body = match self.inspector_tab {
            InspectorTab::Reactions => self.reactions_panel(),
            InspectorTab::Assistant => self.assistant_panel(),
            InspectorTab::Pages => self.pages_panel(),
            InspectorTab::DrawingStyle => self.drawing_style_panel(),
            InspectorTab::ThemeGenerator => self.theme_generator_panel(),
            InspectorTab::Properties if self.tab.joining.is_some() => self.join_panel(),
            InspectorTab::Properties => self.properties_panel(),
            InspectorTab::Predict => self.nmr_inspector(),
            InspectorTab::Labels => self.atom_labels_panel(),
            InspectorTab::Abbreviations => self.abbreviations_panel(),
            InspectorTab::Templates => self.templates_panel(),
            InspectorTab::Import => self.import_panel(),
            InspectorTab::Names => self.naming_panel(),
            InspectorTab::Export => self.export_panel(),
            #[cfg(not(target_arch = "wasm32"))]
            InspectorTab::Versions => self.versions_panel(),
            // The browser has no GitHub drawings.
            #[cfg(target_arch = "wasm32")]
            InspectorTab::Versions => Space::new().into(),
        };
        let mut content = column![
            container(header).padding([6., SEGMENT_MARGIN]),
            scrollable(container(body).padding([8, 16]))
                .id("inspector-content")
                .on_scroll(|viewport| Message::InspectorScroll(viewport.absolute_offset().y))
                .height(Length::Fill)
        ];
        if self.inspector_tab == InspectorTab::Properties && self.tab.optimization.is_some() {
            content = content.push(container(self.optimization_footer()).padding([10, 16]));
            // The 3D preview is modal: Tab cycles its controls, Cancel and Apply last.
            return self.inspector_frame(reshiki::accessibility::focus_scope(content));
        }
        self.inspector_frame(content)
    }

    fn templates_panel(&self) -> Element<'_, Message> {
        use super::template_library::Action as A;
        let action = Message::Templates;
        let state = &self.templates;
        let mut body = column![section("Template library")].spacing(8);
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
                    "New template"
                } else {
                    "Edit template"
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
                        panel_action("Save", action(A::SaveDetails))
                            .style(crate::appearance::primary),
                        panel_action("Cancel", action(A::CancelDetails))
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
            body = body.push(panel_action("Undo library change", action(A::Restore)));
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
            let mut search = column![
                crate::appearance::text_input("Search names, collections…", &state.query)
                    .on_input(|s| Message::Templates(A::Search(s)))
                    .size(12)
                    .padding(8),
            ]
            .spacing(8);
            if state.collection != "All collections" && !state.query.trim().is_empty() {
                search = search.push(
                    row![
                        text(format!("In {}", state.collection))
                            .size(11)
                            .style(muted_text)
                            .width(Length::Fill),
                        command("Search all", action(A::SearchAll))
                            .style(crate::appearance::secondary),
                    ]
                    .spacing(3)
                    .align_y(Alignment::Center),
                );
            }
            body = body
                .push(
                    search.push(
                        crate::appearance::pick_list(
                            [Filter::All, Filter::Mine, Filter::Favorites],
                            Some(state.filter),
                            |f| Message::Templates(A::Filter(f)),
                        )
                        .text_size(12)
                        .padding(6)
                        .width(Length::Fill),
                    ),
                )
                .push(
                    row![
                        command("← Categories", action(A::Browse)).on_press_maybe(
                            (state.collection != "All collections"
                                || !state.query.is_empty()
                                || state.filter != Filter::All)
                                .then(|| action(A::Browse)),
                        ),
                        command("Forward →", action(A::Forward))
                            .on_press_maybe(state.can_forward().then(|| action(A::Forward))),
                    ]
                    .spacing(3),
                );
            body = body.push(
                button(text("Save selection as template").size(12))
                    .padding(7)
                    .width(Length::Fill)
                    .on_press_maybe((!self.tab.selected.is_empty()).then(|| action(A::BeginSave))),
            );
            body = body.push(
                row![
                    panel_action("Import…", action(A::Import)),
                    panel_action("Export…", action(A::Export)),
                    panel_action("Reload", action(A::Reload))
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
                locants: Some(&t.locants),
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
                    text(super::template_library::anchor_label(t, state.anchor))
                        .size(11)
                        .width(Length::Fill),
                    command("Auto", action(A::Anchor(reshiki::templates::Anchor::Auto)))
                        .style(crate::appearance::secondary)
                ]
                .align_y(Alignment::Center),
            )
            .push(
                row![
                    panel_action("Place", Message::InsertTemplate(self.template_index))
                        .style(crate::appearance::primary),
                    panel_action(
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
                    "Click again to add another copy. Esc finishes placement."
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
                        panel_action("Edit", action(A::EditDetails)),
                        panel_action("Remember anchor", action(A::RememberAnchor))
                    ]
                    .spacing(5),
                )
                .push(
                    panel_action("Replace from selection", action(A::Replace)).on_press_maybe(
                        (!self.tab.selected.is_empty()).then(|| action(A::Replace)),
                    ),
                )
                .push(panel_action("Remove template", action(A::Remove)));
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
                for name in super::template_library::categories(template) {
                    let entry = categories.entry(name).or_insert((0, template));
                    entry.0 += 1;
                }
            }
            body = body.push(horizontal_line()).push(section("Categories"));
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
                "{} · {} {}",
                if state.collection == "All collections" {
                    "Results"
                } else {
                    &state.collection
                },
                matches.len(),
                if matches.len() == 1 {
                    "template"
                } else {
                    "templates"
                }
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
        let choices: Vec<String> = reshiki::abbreviations::group_labels()
            .into_iter()
            .chain(reshiki::ligands::LABELS.iter().map(|s| (*s).into()))
            .chain(reshiki::chemistry::abbreviations::residue_labels())
            .collect();
        let findable = reshiki::chemistry::abbreviations::residue(&self.abbreviations.preset)
            .is_some()
            || reshiki::chemistry::abbreviations::lookup(&self.abbreviations.preset)
                .is_ok_and(|group| group.is_some());
        // Disabled commands say what they need.
        let item = |label, message: Message, enabled: bool, hint, reason| {
            hover_hint(
                panel_action(label, message.clone()).on_press_maybe(enabled.then_some(message)),
                if enabled { hint } else { reason },
                tooltip::Position::Top,
            )
        };
        let wait = "Wait for the current chemistry job to finish";
        let label = self.abbreviations.label.trim();
        let mut fragment_labels = column![
            crate::appearance::text_input("Label, e.g. Ar", &self.abbreviations.label)
                .on_input(move |s| action(A::Label(s)))
                .on_submit(action(A::Contract))
                .size(13),
            crate::appearance::text_input(
                "From the right (optional)",
                &self.abbreviations.reverse_label
            )
            .on_input(move |s| action(A::ReverseLabel(s)))
            .on_submit(action(A::Contract))
            .size(13),
        ]
        .spacing(6);
        for warning in self.tab.doc.contract_label_warnings(
            &self.tab.selected,
            label,
            &self.abbreviations.reverse_label,
        ) {
            fragment_labels = fragment_labels.push(text(warning).size(11).style(muted_text));
        }
        let fragment_labels = hover_hint(
            fragment_labels,
            reshiki::abbreviations::CONTRACT_LABEL_HINT,
            tooltip::Position::Top,
        );
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
            section("Common group or residue"),
            crate::appearance::pick_list(choices, Some(self.abbreviations.preset.clone()), move |s| action(A::Preset(s))).width(Length::Fill).text_size(14),
            item(
                "Find selected label",
                action(A::FindLabel),
                !self.tab.busy && !self.tab.doc.atoms.is_empty() && findable,
                "Contracts matching groups or amino-acid residues into the selected label",
                if self.tab.busy { wait } else if !findable { "Find supports catalog groups and amino-acid residues" } else { "Draw or import a molecule first" },
            ),
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
            section("Name a selected fragment"),
            fragment_labels,
            item(
                "Abbreviate selection",
                action(A::Contract),
                !self.tab.selected.is_empty() && !label.is_empty(),
                "Select a connected fragment whose outside bonds meet at most two selected atoms. A custom name does not change its chemistry.",
                if self.tab.selected.is_empty() { "Select a connected fragment first" } else { "Enter a label first" },
            ),
            item(
                "Save as abbreviation",
                action(A::Define),
                !self.tab.selected.is_empty() && !label.is_empty(),
                "Saves the selected group and its one outside bond to your abbreviations, so typing this label on any endpoint creates it.",
                if self.tab.selected.is_empty() { "Select a group with one bond to the rest of the drawing" } else { "Enter a label first" },
            ),
            horizontal_line(),
        ].spacing(10);
        if let Some(warning) = &self.abbreviations.catalog_warning {
            body = body.push(
                text(warning)
                    .size(11)
                    .style(crate::appearance::text_color(Color::from_rgb8(182, 66, 61))),
            );
        }
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
        let stereo_groups = if atoms.is_empty() {
            self.tab.doc.atom_labels.stereo_groups
        } else {
            atoms.iter().all(|a| {
                a.display
                    .stereo_group
                    .show
                    .unwrap_or(self.tab.doc.atom_labels.stereo_groups)
            })
        };
        let position = atoms
            .first()
            .map(|a| a.display.hydrogen_position)
            .filter(|p| atoms.iter().all(|a| a.display.hydrogen_position == *p));
        let notes = reshiki::atom_labels::hidden_label_notes(&self.tab.doc, &atoms);
        let mut hydrogen_control = column![
            checkbox(hydrogens)
                .label("Show implied hydrogens")
                .text_size(12)
                .on_toggle(|v| Message::Labels(A::Hydrogens(v)))
        ]
        .spacing(4);
        if let Some(note) = notes.hydrogens {
            hydrogen_control = hydrogen_control.push(text(note).size(11).style(muted_text));
        }
        let mut charge_control = column![
            checkbox(atoms.iter().all(|a| !a.display.hide_charge))
                .label("Show charge labels")
                .text_size(12)
                .on_toggle(|v| Message::Labels(A::Charges(v)))
        ]
        .spacing(4);
        if let Some(note) = notes.charges {
            charge_control = charge_control.push(text(note).size(11).style(muted_text));
        }
        let mut body = column![
            command("‹ Properties", Message::Inspector(InspectorTab::Properties)),
            section("Atom labels"),
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
            hydrogen_control,
            charge_control,
            text("Hydrogen position").size(12),
            crate::appearance::pick_list(HydrogenPosition::ALL, position, |v| Message::Labels(
                A::Position(v)
            ))
            .placeholder("Mixed")
            .text_size(12)
            .width(Length::Fill),
        ]
        .spacing(9);
        if let Some(attachment) = self.label_attachment_picker(&atoms) {
            body = body
                .push(text("Bond attachment (text labels)").size(12))
                .push(attachment);
        }
        body = body.push(
            column![
                text("Display settings keep the molecular composition intact.")
                    .size(11)
                    .style(muted_text),
                horizontal_line(),
                section("Atom numbers"),
                row![
                    crate::appearance::text_input("1, atom1, a, α…", &self.tab.labels.seed)
                        .on_input(|s| Message::Labels(A::Seed(s)))
                        .on_submit(Message::Labels(A::Number))
                        .size(12)
                        .padding(7),
                    command("Number", Message::Labels(A::Number))
                        .style(crate::appearance::secondary)
                        .on_press_maybe((!ids.is_empty()).then_some(Message::Labels(A::Number))),
                    command("IUPAC locants", Message::Labels(A::IupacLocants))
                        .style(crate::appearance::secondary)
                        .on_press_maybe(
                            (!self.tab.selected.is_empty() && !self.tab.labels.locants_running())
                                .then_some(Message::Labels(A::IupacLocants))
                        )
                ]
                .spacing(5),
                text("Sequence follows atom creation/import order.")
                    .size(11)
                    .style(muted_text),
                checkbox(self.tab.labels.ring_atoms_only)
                    .label("Ring atoms only")
                    .text_size(12)
                    .on_toggle(|value| Message::Labels(A::RingAtomsOnly(value))),
                self.locant_numbering_controls(),
                panel_action("Clear numbers", Message::Labels(A::ClearNumbers)),
            ]
            .spacing(9),
        );
        if ids.len() == 1 {
            body = body.push(text("Custom atom number").size(12)).push(
                row![
                    crate::appearance::text_input("e.g. Cα or 12a", &self.tab.labels.number)
                        .on_input(|s| Message::Labels(A::Text(s)))
                        .on_submit(Message::Labels(A::ApplyText))
                        .size(12)
                        .padding(7),
                    command("Set", Message::Labels(A::ApplyText))
                        .style(crate::appearance::secondary)
                ]
                .spacing(5),
            );
        }
        body = self.atom_label_stereo_section(body, stereo, stereo_groups);
        self.atom_label_indicator_section(body)
    }

    fn locant_numbering_controls(&self) -> Element<'_, Message> {
        use super::atom_labels::Action as A;
        if let Some(count) = self.tab.labels.locants_replacements(
            &self.tab.doc,
            self.tab.file_epoch,
            self.tab.revision,
        ) {
            return column![
                text(format!("Replace {count} existing atom numbers?")).size(12),
                row![
                    panel_action("Replace", Message::Labels(A::ReplaceLocants)),
                    panel_action("Cancel", Message::Labels(A::CancelLocants)),
                ]
                .spacing(5),
            ]
            .spacing(5)
            .into();
        }
        if self.tab.labels.locants_running() {
            return row![
                text("Generating parent locants…")
                    .size(11)
                    .style(muted_text),
                command("Cancel", Message::Labels(A::CancelLocants))
                    .style(crate::appearance::secondary),
            ]
            .spacing(5)
            .align_y(Alignment::Center)
            .into();
        }
        Space::new().height(0).into()
    }

    fn atom_label_stereo_section<'a>(
        &'a self,
        mut body: iced::widget::Column<'a, Message>,
        stereo: bool,
        stereo_groups: bool,
    ) -> iced::widget::Column<'a, Message> {
        use super::atom_labels::Action as A;
        use reshiki::atom_labels::AxialDescriptors;
        body = body
            .push(horizontal_line())
            .push(section("Stereochemistry"))
            .push(hover_hint(
                checkbox(stereo)
                    .label("Show stereodescriptors\n(R/S, E/Z…)")
                    .text_size(12)
                    .on_toggle(|v| Message::Labels(A::Stereo(v))),
                "R/S: stereocenter; swaps in the mirror image.\n\
                     r/s: pseudoasymmetric center; unchanged in the mirror image.\n\
                     E/Z: double bond; unchanged in the mirror image.\n\
                     M/P: axial or planar chirality (Ra/Sa when chosen).\n\
                     seqCis/seqTrans: enantiomorphic double bond; swaps in the mirror image.",
                tooltip::Position::Left,
            ))
            .push(hover_hint(
                checkbox(stereo_groups)
                    .label("Show stereo groups\n(abs, and1, or1)")
                    .text_size(12)
                    .on_toggle(|v| Message::Labels(A::StereoGroups(v))),
                "abs: configuration exactly as drawn.\n\
                     and1: racemic; the drawn form and its mirror image, 1:1.\n\
                     or1: one of the two forms, not known which.\n\
                     Centers with the same label flip together. Set groups in the inspector's Stereo groups section.",
                tooltip::Position::Left,
            ))
            .push(
                checkbox(self.tab.doc.atom_labels.dl_descriptors)
                    .label("Show D/L for amino acids and sugars")
                    .text_size(12)
                    .on_toggle(|v| Message::Labels(A::DlDescriptors(v))),
            )
            .push(
                checkbox(self.tab.doc.atom_labels.roman_stereo_parentheses)
                    .label("Upright stereo parentheses (whole drawing)")
                    .text_size(12)
                    .on_toggle(|v| Message::Labels(A::RomanStereoParentheses(v))),
            )
            .push(
                checkbox(self.tab.doc.atom_labels.mark_undefined_stereocentres)
                    .label("Mark undefined stereocenters")
                    .text_size(12)
                    .on_toggle_maybe((stereo && self.tab.doc.atom_labels.stereo).then_some(|v| Message::Labels(A::MarkUndefined(v)))),
            )
            .push(
                checkbox(self.tab.doc.atom_labels.undefined_xi)
                    .label("Use Ξ/ξ for undefined (IUPAC)")
                    .text_size(12)
                    .on_toggle_maybe((stereo && self.tab.doc.atom_labels.stereo && self.tab.doc.atom_labels.mark_undefined_stereocentres).then_some(|v| Message::Labels(A::UndefinedXi(v)))),
            )
            .push(hover_hint(
                checkbox(self.tab.doc.atom_labels.steroid_alpha_beta)
                    .label("Steroid α/β (3β, 5α…)")
                    .text_size(12)
                    .on_toggle_maybe(stereo.then_some(|v| Message::Labels(A::SteroidAlphaBeta(v)))),
                "α: below the ring plane, β: above it, in standard steroid orientation; other centers keep R/S",
                tooltip::Position::Left,
            ))
            .push(
                text("Labels update after chemical edits. (?) or (Ξ)/(ξ) marks undefined configuration when enabled.")
                    .size(11)
                    .style(muted_text),
            )
            .push(text("Axial descriptors").size(12))
            .push(
                crate::appearance::pick_list(
                    AxialDescriptors::ALL,
                    Some(self.tab.doc.atom_labels.axial_descriptors),
                    |value| Message::Labels(A::AxialDescriptors(value)),
                )
                .text_size(12)
                .width(Length::Fill),
            )
            .push(
                text("Applies to the whole drawing.")
                    .size(11)
                    .style(muted_text),
            );
        if let Some(notice) = &self.tab.chemistry_notice {
            let line = text(notice).size(11);
            // Generic structures wait for expansion: information, not an error.
            body = body.push(if notice == reshiki::atom_labels::refresh::GENERIC_NOTICE {
                line.style(muted_text)
            } else {
                line.style(crate::appearance::text_color(Color::from_rgb8(182, 66, 61)))
            });
        }
        body = body
            .push(
                panel_action("Check drawing", Message::Analyze)
                    .on_press_maybe((!self.tab.busy).then_some(Message::Analyze)),
            )
            .push(
                checkbox(self.appearance.iupac_style_notes)
                    .label("IUPAC style notes in Check drawing")
                    .text_size(12)
                    .on_toggle(|v| Message::Stereo(super::stereo::Action::IupacStyleNotes(v))),
            );
        self.stereo_note_lines(body, "stereo")
    }

    /// The stereo findings and their fixes; `ids` keeps the accessibility
    /// ids of the findings list apart from the panel's.
    pub(super) fn stereo_note_lines<'a>(
        &'a self,
        body: iced::widget::Column<'a, Message>,
        ids: &str,
    ) -> iced::widget::Column<'a, Message> {
        let body = self.stereo_findings(body, ids);
        match self.stereo_fixes(ids) {
            Some(fixes) => body.push(fixes),
            None => body,
        }
    }

    fn stereo_findings<'a>(
        &'a self,
        mut body: iced::widget::Column<'a, Message>,
        ids: &str,
    ) -> iced::widget::Column<'a, Message> {
        for (index, note) in self.tab.stereo_notes.iter().enumerate() {
            body = body.push(
                reshiki::accessibility::button(
                    format!("{ids}-finding-{index}"),
                    note.message.as_str(),
                    row![finding_dot(), text(&note.message).size(12)]
                        .spacing(8)
                        .align_y(Alignment::Center),
                )
                .on_press(Message::Stereo(super::stereo::Action::Select(index)))
                .width(Length::Fill)
                .padding([6, 8])
                .style(control(false)),
            );
        }
        body
    }

    /// Reverse and Redraw stereo bonds, when a finding has stereo bonds to fix.
    fn stereo_fixes<'a>(&self, ids: &str) -> Option<Element<'a, Message>> {
        use super::stereo::Action;
        use reshiki::chemistry::stereo_lint::Kind;
        let notes = &self.tab.stereo_notes;
        // Undefined stereocentres have no stereo bonds to fix.
        if !notes.iter().any(|note| note.kind != Kind::Undefined) {
            return None;
        }
        let mut actions = row![].spacing(6);
        if notes.iter().any(|note| note.kind == Kind::Reversed) {
            actions = actions.push(
                accessible_command(format!("{ids}-reverse"), "Reverse", "Reverse")
                    .on_press(Message::Stereo(Action::Reverse))
                    .style(crate::appearance::secondary),
            );
        }
        actions = actions.push(
            accessible_command(
                format!("{ids}-redraw"),
                "Redraw stereo bonds",
                "Redraw stereo bonds",
            )
            .on_press(Message::Stereo(Action::Redraw))
            .style(crate::appearance::secondary),
        );
        Some(container(actions).padding([4, 0]).into())
    }

    fn atom_label_indicator_section<'a>(
        &'a self,
        body: iced::widget::Column<'a, Message>,
    ) -> Element<'a, Message> {
        use super::atom_labels::Action as A;
        body.push(horizontal_line())
            .push(section("Indicator appearance"))
            .push(
                row![
                    crate::appearance::text_input("Size in pt", &self.tab.labels.size)
                        .on_input(|s| Message::Labels(A::Size(s)))
                        .on_submit(Message::Labels(A::ApplySize))
                        .size(12)
                        .padding(7),
                    text("pt").size(11),
                    command("Apply", Message::Labels(A::ApplySize))
                        .style(crate::appearance::secondary)
                ]
                .spacing(5)
                .align_y(Alignment::Center),
            )
            .push(panel_action(
                "Use current toolbar text style",
                Message::Labels(A::ToolbarStyle),
            ))
            .push(panel_action(
                "Position numbers & stereo labels",
                Message::Labels(A::PositionIndicators),
            ))
            .push(panel_action(
                "Restore automatic positions",
                Message::Labels(A::ResetPositions),
            ))
            .push(panel_action(
                "Use drawing label defaults",
                Message::Labels(A::ResetOverrides),
            ))
            .into()
    }

    /// Interface, canvas aids, data display and actions, set apart by wider
    /// gaps. On a narrow window whole groups wrap to a second line rather than
    /// run off the edge.
    fn view_options(&self) -> Element<'_, Message> {
        let group = |items: Vec<Element<'static, Message>>| -> Element<'static, Message> {
            iced::widget::Row::with_children(items)
                .spacing(10)
                .align_y(Alignment::Center)
                .into()
        };
        let interface = group(vec![
            text("View").size(12).style(muted_text).into(),
            text("Interface").size(12).style(muted_text).into(),
            crate::appearance::pick_list(
                crate::appearance::Mode::ALL,
                Some(self.appearance.mode),
                Message::Appearance,
            )
            .text_size(12)
            .padding(5)
            .width(132)
            .into(),
        ]);
        let canvas = group(vec![
            checkbox(self.grid)
                .label("Grid")
                .on_toggle(|_| Message::View(super::view_settings::Action::Grid))
                .size(14)
                .text_size(12)
                .into(),
            checkbox(self.guides.rulers)
                .label("Rulers")
                .on_toggle(|on| Message::View(super::view_settings::Action::Rulers(on)))
                .size(14)
                .text_size(12)
                .into(),
            checkbox(self.guides.crosshair)
                .label("Crosshair")
                .on_toggle(|on| Message::View(super::view_settings::Action::Crosshair(on)))
                .size(14)
                .text_size(12)
                .into(),
            text("Units").size(12).style(muted_text).into(),
            crate::appearance::pick_list(
                crate::canvas::guides::Unit::ALL,
                Some(self.guides.unit),
                |unit| Message::View(super::view_settings::Action::RulerUnit(unit)),
            )
            .text_size(12)
            .padding(5)
            .width(68)
            .into(),
            hover_hint(
                checkbox(self.appearance.smart_guides)
                    .label("Smart guides")
                    .on_toggle(|on| Message::View(super::view_settings::Action::SmartGuides(on)))
                    .size(14)
                    .text_size(12),
                format!(
                    "Snap dragged objects to other objects' edges, centers and equal gaps · Hold {} to move freely",
                    crate::platform::alt_key()
                ),
                tooltip::Position::Top,
            )
            .into(),
            hover_hint(
                checkbox(self.appearance.arrange_controls)
                    .label("Arrange controls")
                    .on_toggle(|visible| {
                        Message::ObjectToolbar(super::object_toolbar::Action::Visible(visible))
                    })
                    .size(14)
                    .text_size(12),
                "Align, distribute, order, flip and rotate at the right end of the Select row",
                tooltip::Position::Top,
            )
            .into(),
        ]);
        let data = group(vec![
            hover_hint(
                checkbox(self.appearance.two_decimal_weight)
                    .label("2-decimal MW")
                    .on_toggle(|on| {
                        Message::View(super::view_settings::Action::TwoDecimalWeight(on))
                    })
                    .size(14)
                    .text_size(12),
                "Mol. Wt. with 2 decimal places instead of 3; a caption can set its own",
                tooltip::Position::Top,
            )
            .into(),
            hover_hint(
                checkbox(self.appearance.name_typography)
                    .label("Chemical name typography")
                    .on_toggle(|on| Message::View(super::view_settings::Action::NameTypography(on)))
                    .size(14)
                    .text_size(12),
                "Italic descriptors, primes and superscripts in name captions",
                tooltip::Position::Top,
            )
            .into(),
        ]);
        let actions = group(vec![
            command("Page setup…", Message::Pages(super::pages::Action::Show)).into(),
            command("Done", Message::View(super::view_settings::Action::Toggle)).into(),
        ]);
        container(
            row![interface, canvas, data, actions]
                .spacing(24)
                .wrap()
                .vertical_spacing(4),
        )
        .padding([7, 14])
        .width(Length::Fill)
        .style(panel)
        .into()
    }

    fn status_bar(&self) -> Element<'_, Message> {
        let summary = self.status.lines().next().unwrap_or(&self.status);
        let error = self.error;
        // A GitHub question, such as a drawing changed there, is amber.
        #[cfg(not(target_arch = "wasm32"))]
        let amber = self.github.amber.as_deref() == Some(self.status.as_str());
        #[cfg(target_arch = "wasm32")]
        let amber = false;
        let style = move |theme: &Theme| iced::widget::text::Style {
            color: Some(if error {
                crate::appearance::readable(
                    crate::appearance::themed(theme, Color::from_rgb8(168, 52, 47)),
                    &[theme.palette().background],
                    reshiki::color_contrast::TEXT_TARGET,
                )
            } else if amber {
                crate::appearance::readable(
                    theme.palette().warning,
                    &[theme.palette().background],
                    reshiki::color_contrast::TEXT_TARGET,
                )
            } else {
                crate::appearance::muted(theme)
            }),
        };
        // One line in the room left beside the controls; the full text of
        // long or multi-line messages is a tooltip.
        let line = responsive(move |size| {
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
            let has_details = self.status.contains('\n') || text_width(summary, 11.) > size.width;
            if !has_details {
                return container(message).id("status-message").into();
            }
            // Keep the cue outside the clipped message so it remains visible
            // beside the controls.
            row![
                container(hover_hint(
                    message,
                    self.status.as_str(),
                    tooltip::Position::Top
                ))
                .id("status-message")
                .width(Length::Fill)
                .clip(true),
                hover_hint(
                    container(text("…").size(14).style(style))
                        .id("status-details")
                        .padding([0, 3]),
                    self.status.as_str(),
                    tooltip::Position::Top,
                ),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into()
        })
        .height(Length::Shrink);
        let mut status = row![].spacing(8).align_y(Alignment::Center);
        // GitHub first: its button (outside, so the controls never squeeze
        // it out), then the chip of the drawing in front.
        #[cfg(not(target_arch = "wasm32"))]
        {
            status = status.push(self.github_chip());
        }
        if self.recovery_banner_shown() {
            status = status.push(self.recovery_banner());
        }
        status = status.push(line);
        if let Some((key, progress)) = self.engine_progress() {
            status = status.push(text(progress).size(11)).push(hover_hint(
                accessible_command("engine.cancel", "Cancel chemistry operation", "Cancel")
                    .on_press(Message::CancelEngine(key)),
                "Cancel this operation · Esc",
                tooltip::Position::Top,
            ));
        }
        if self.updates.available() {
            status = status.push(
                accessible_command("status-update", "Update available", "Update available")
                    .on_press(Message::Updates(super::updates::Action::Show(true)))
                    .style(control(true)),
            );
        }
        // Only a failed recovery save shows a dot: the tab's dot already
        // says what is unsaved.
        if self.tab.autosave_status.starts_with("Recovery save failed") {
            status = status.push(hover_hint(
                container(
                    container(Space::new().width(7).height(7)).style(move |theme| {
                        container::Style {
                            background: Some(
                                crate::appearance::themed(theme, Color::from_rgb8(168, 52, 47))
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
        let controls = iced::widget::Row::with_children(self.document_settings())
            .spacing(8)
            .align_y(Alignment::Center)
            .height(32)
            .push(divider())
            .push(
                accessible_command("status-view", "Drawing view options", "View")
                    .on_press(Message::View(super::view_settings::Action::Toggle))
                    .style(control(self.view_open)),
            )
            .push(
                accessible_command("status-zoom-out", "Zoom out", "−").on_press(Message::Zoom(0.8)),
            )
            .push(
                text(format!("{:.0}%", self.tab.camera.zoom * 100.0))
                    .size(11)
                    .width(38)
                    .center(),
            )
            .push(
                accessible_command("status-zoom-in", "Zoom in", "+").on_press(Message::Zoom(1.25)),
            )
            .push(
                accessible_command("status-fit", "Fit drawing in view", "Fit")
                    .on_press(Message::Fit),
            );
        // One row at every width: the message shortens, its full text stays
        // in the tooltip, and the controls keep their place.
        #[cfg(not(target_arch = "wasm32"))]
        let content = row![self.github_button()];
        #[cfg(target_arch = "wasm32")]
        let content = row![];
        let content = content
            .push(status.width(Length::Fill).height(32))
            .push(controls)
            .spacing(8)
            .align_y(Alignment::Center);
        container(content)
            .id("status-bar")
            .padding([6, 14])
            .style(panel)
            .into()
    }

    /// Journal preset, color theme and canvas light/dark: whole-document
    /// settings, kept beside View and zoom.
    fn document_settings(&self) -> [Element<'_, Message>; 3] {
        use super::document_styles::Choice;
        use super::theme_files::Choice as ThemeChoice;
        use reshiki::document_styles::Preset;
        let current = Preset::ALL
            .into_iter()
            .find(|p| p.style() == self.tab.doc.drawing_style)
            .map(Choice::Journal)
            .unwrap_or(Choice::Custom);
        let (themes, theme) = self.theme_choices();
        let themes_only: Vec<_> = (themes.iter())
            .filter(|choice| **choice != ThemeChoice::Manage)
            .cloned()
            .collect();
        let dark = self.tab.doc.canvas_theme.is_dark();
        [
            hover_hint(
                // Manage styles… opens the inspector, so ↑ ↓ skip it.
                reshiki::accessibility::select(
                    "journal-preset",
                    "Journal preset",
                    Preset::ALL.to_vec(),
                    current.to_string(),
                    |preset| Message::QuickDrawingStyle(Choice::Journal(preset)),
                    crate::appearance::close_on_escape(
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
                    ),
                ),
                "Journal preset · Bond length, line widths and label font",
                tooltip::Position::Top,
            )
            .into(),
            hover_hint(
                // Manage themes… opens the theme editor, so ↑ ↓ skip it.
                reshiki::accessibility::select(
                    "color-theme",
                    "Color theme",
                    themes_only,
                    theme.to_string(),
                    |choice| Message::ThemeFile(super::theme_files::Action::Choose(choice)),
                    crate::appearance::close_on_escape(
                        crate::appearance::pick_list(themes, Some(theme.clone()), |choice| {
                            Message::ThemeFile(super::theme_files::Action::Choose(choice))
                        })
                        // Sizes the list for an embedded theme's name, which is not an option.
                        .placeholder(theme.to_string())
                        .text_size(11)
                        .padding([4, 8]),
                    ),
                ),
                "Color theme",
                tooltip::Position::Top,
            )
            .into(),
            hover_hint(
                reshiki::accessibility::button(
                    "canvas-theme",
                    "Dark canvas",
                    iced::widget::canvas(Glyph(if dark { Icon::Moon } else { Icon::Sun }, true))
                        .width(24)
                        .height(24),
                )
                .padding(3)
                .style(control(false))
                .checked(dark)
                .on_press(Message::CanvasTheme(self.tab.doc.canvas_theme.toggled())),
                if dark {
                    "Dark canvas · Switch to light · Remembered for new drawings"
                } else {
                    "Light canvas · Switch to dark · Remembered for new drawings"
                },
                tooltip::Position::Top,
            )
            .into(),
        ]
    }
}

const PALETTE_WIDTH: f32 = 104.;
const INSPECTOR_WIDTH: f32 = 320.;
/// The inspector's tab strip: a segmented control. Equal segments cannot fit
/// "Properties" at 12 px, so each grows with its label.
const INSPECTOR_TABS: [(&str, InspectorTab); 5] = [
    ("Properties", InspectorTab::Properties),
    ("Predict", InspectorTab::Predict),
    ("Templates", InspectorTab::Templates),
    ("Import", InspectorTab::Import),
    ("Export", InspectorTab::Export),
];
/// The tallest the Check drawing findings grow before they scroll; with the
/// header and fixes it fits under the button in the minimum 680 pt window.
const FINDINGS_HEIGHT: f32 = 360.;
/// The tab track's distance from the panel's edges, and its rim around the
/// segments.
const SEGMENT_MARGIN: f32 = 4.;
const TRACK_INSET: f32 = 2.;
/// A GitHub drawing's History clock after the track: a 14 px glyph in a
/// 20 px button, 4 px from the track.
#[cfg_attr(
    target_arch = "wasm32",
    expect(dead_code, reason = "The browser editor has no GitHub drawings")
)]
const HISTORY_BUTTON: f32 = 20.;
const HISTORY_GAP: f32 = 4.;
/// A tab segment's share of the track: its label's width plus 3 px on each
/// side, so every label keeps that margin whenever the labels fit at all.
fn segment_portion(label: &str) -> u16 {
    text_width(label, 12.).ceil() as u16 + 6
}
/// Tool buttons and the gap between their rows: a 34 px pitch.
const TOOL_BUTTON: f32 = 32.;
const TOOL_GAP: f32 = 2.;
/// The scrolled tools and atoms fade out this far above the Help button.
const PALETTE_FADE: f32 = 16.;
const ATOM_BUTTON: f32 = 28.;
const ATOM_GAP: f32 = 2.;
/// The ATOMS buttons by line: H first, then the common organic elements, as
/// in Ketcher's atom toolbar; the dummy atom and R group get their own line.
const PALETTE_ELEMENTS: [&[&str]; 4] = [
    &["H", "C", "N"],
    &["O", "S", "P"],
    &["F", "Cl", "Br"],
    &["*", super::palettes::R_GROUP],
];
const CONTEXT_PADDING: f32 = 14.;
pub(super) const CONTEXT_GAP: f32 = 10.;
const DIVIDER: f32 = 11.;
const MORE_WIDTH: f32 = 30.;
/// A context row field with its unit inside.
const UNIT_FIELD: f32 = 54.;

/// The context bar's frame, the full row width, around the tool row or the
/// bar of a modal state (3D preview, Move & attach, Coordinate in place).
fn context_frame<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content)
        .width(Length::Fill)
        .height(46)
        .padding([5., CONTEXT_PADDING])
        .center_y(46)
        .clip(true)
        .style(panel)
        .into()
}

/// A context row command that can fold into the ⋯ menu.
pub(super) struct RowCommand {
    pub label: &'static str,
    /// The full name: in the ⋯ menu, the tooltip and the accessibility tree.
    pub menu: &'static str,
    pub message: Message,
    pub enabled: bool,
}

impl RowCommand {
    /// The single key of a keyboard drawing command, shown on its button.
    fn keycap(&self) -> Option<String> {
        matches!(self.message, Message::KeyboardDrawing(_))
            .then(|| super::shortcuts::label(&self.message))
            .flatten()
    }
}

/// Space between a command's label and its keycap.
const KEYCAP_GAP: f32 = 6.;

/// Width of `keycap`: the key, its padding and its border.
fn keycap_width(key: &str) -> f32 {
    text_width(key, 11.) + 12.
}

/// A key drawn as a small outlined cap, as in “Mark [”.
fn keycap<'a>(key: String, enabled: bool) -> Element<'a, Message> {
    container(
        rich_text(super::shortcuts::spans(&key))
            .size(11)
            .line_height(1.),
    )
    .padding([1, 5])
    .style(move |theme: &Theme| {
        let ink = if enabled {
            theme.palette().text
        } else {
            crate::appearance::muted(theme)
        };
        container::Style {
            text_color: Some(ink),
            border: Border {
                color: Color { a: 0.35, ..ink },
                width: 1.,
                radius: 4.0.into(),
            },
            ..Default::default()
        }
    })
    .into()
}

/// Shared by the inline context row and its foreground overflow menu.
pub(super) fn keyboard_control_id(message: &Message) -> Option<&'static str> {
    use super::keyboard_drawing::Action;
    match message {
        Message::KeyboardDrawing(Action::Mark) => Some("keyboard-mark"),
        Message::KeyboardDrawing(Action::Connect) => Some("keyboard-connect"),
        Message::KeyboardDrawing(Action::Coordinate) => Some("keyboard-coordinate"),
        Message::Join(super::joining::Action::BeginCoordination) => Some("coordination.begin"),
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
    /// Commands folded into ⋯, from the end.
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
/// into ⋯ one by one from the end, the arrange group collapses, the selection summary
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
                .take(n - fit.folded)
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

/// A panel action: a full-width secondary button, so it reads as a control
/// rather than a label. Two side by side share the row.
pub(super) fn panel_action(label: &str, message: Message) -> button::Button<'_, Message> {
    button(text(label).size(12).width(Length::Fill).center())
        .padding([6, 8])
        .width(Length::Fill)
        .on_press(message)
        .style(crate::appearance::secondary)
}

pub(super) fn command(label: &str, message: Message) -> button::Button<'_, Message> {
    button(text(label).size(12))
        .padding([7, 9])
        .on_press(message)
        .style(control(false))
}
/// A `command` with an accessibility id and name, for controls that
/// assistive technology and tests activate by id. The caller sets its action.
pub(super) fn accessible_command<'a>(
    id: impl Into<String>,
    name: impl Into<String>,
    label: impl text::IntoFragment<'a>,
) -> reshiki::accessibility::Button<'a, Message> {
    reshiki::accessibility::button(id, name, text(label).size(12))
        .padding([7, 9])
        .style(control(false))
}
/// A checkbox that publishes its id, name and checked state; `message` is
/// the toggle, or `None` while it is unavailable.
pub(super) fn accessible_checkbox<'a>(
    id: &'static str,
    label: &'static str,
    checked: bool,
    message: Option<Message>,
) -> reshiki::accessibility::Button<'a, Message> {
    let toggle = message.clone();
    reshiki::accessibility::button(
        id,
        label,
        checkbox(checked)
            .label(label)
            .on_toggle_maybe(toggle.map(|message| move |_| message.clone()))
            .size(14)
            .text_size(12),
    )
    .checked(checked)
    .on_press_maybe(message)
    .padding(0)
    .style(|theme: &Theme, _| button::Style {
        text_color: theme.palette().text,
        ..Default::default()
    })
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
pub(super) fn icon_button(
    icon: Icon,
    hint: impl Into<std::borrow::Cow<'static, str>>,
    message: Option<Message>,
    active: bool,
) -> Element<'static, Message> {
    icon_button_at(icon, hint, message, active, tooltip::Position::Bottom, 36.)
}
/// An `icon_button` for the status bar: as high as its row, the hint above.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn status_icon_button(
    icon: Icon,
    hint: impl Into<std::borrow::Cow<'static, str>>,
    message: Option<Message>,
    active: bool,
) -> Element<'static, Message> {
    icon_button_at(icon, hint, message, active, tooltip::Position::Top, 32.)
}
fn icon_button_at(
    icon: Icon,
    hint: impl Into<std::borrow::Cow<'static, str>>,
    message: Option<Message>,
    active: bool,
    position: tooltip::Position,
    size: f32,
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
        Icon::Cloud => "status-github",
        Icon::Keyboard => "help-open",
        Icon::Settings => "header-settings",
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
        .width(size)
        .height(size)
        .padding((size - 24.) / 2.)
        .style(control(active))
        .on_press_maybe(message),
        hint,
        position,
    )
    .into()
}
/// A small heading. Callers write it in sentence case; it is drawn in capitals.
pub(super) fn section<'a>(label: &str) -> iced::widget::Text<'a> {
    text(label.to_uppercase()).size(10).style(muted_text)
}
/// The marker of a Check drawing finding, in the color of its count badge.
fn finding_dot<'a>() -> Element<'a, Message> {
    container(Space::new().width(6).height(6))
        .style(|theme: &Theme| container::Style {
            background: Some(
                crate::appearance::themed(theme, Color::from_rgb8(168, 52, 47)).into(),
            ),
            border: Border {
                radius: 3.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
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
/// The sunken track behind the inspector's tab segments.
pub(in crate::app) fn segment_track(theme: &Theme) -> container::Style {
    let dark = crate::appearance::is_dark(theme);
    container::Style {
        background: Some(
            if dark {
                Color::from_rgb8(17, 20, 25)
            } else {
                Color::from_rgb8(234, 237, 241)
            }
            .into(),
        ),
        border: Border {
            radius: 8.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}
/// A tab segment: the selected one is a raised surface on the track.
pub(in crate::app) fn segment(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let dark = crate::appearance::is_dark(theme);
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let rgb = |light: [u8; 3], night: [u8; 3]| {
            let [r, g, b] = if dark { night } else { light };
            Color::from_rgb8(r, g, b)
        };
        let background = if selected {
            Some(rgb([255, 255, 255], [44, 51, 59]))
        } else if hovered {
            Some(rgb([222, 227, 233], [32, 37, 44]))
        } else {
            None
        };
        button::Style {
            background: background.map(Into::into),
            text_color: if selected {
                theme.palette().text
            } else {
                crate::appearance::muted(theme)
            },
            border: Border {
                color: if selected {
                    rgb([214, 220, 227], [60, 70, 80])
                } else {
                    Color::TRANSPARENT
                },
                width: 1.,
                radius: 6.0.into(),
            },
            shadow: crate::appearance::surface_shadow(iced::Shadow {
                color: Color::from_rgba(0., 0., 0., if selected { 0.08 } else { 0. }),
                offset: iced::Vector::new(0., 1.),
                blur_radius: 2.,
            }),
            ..Default::default()
        }
    }
}
/// The panel color fading in toward the bottom of the scrolled tools.
fn palette_fade(theme: &Theme) -> container::Style {
    let Some(iced::Background::Color(color)) = panel(theme).background else {
        return container::Style::default();
    };
    container::Style {
        background: Some(
            iced::gradient::Linear::new(0.)
                .add_stop(0., color)
                .add_stop(1., Color { a: 0., ..color })
                .into(),
        ),
        ..Default::default()
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
        Tool::StretchBond { .. } => "Stretch bond",
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
mod inspector_motion_qa;
#[cfg(test)]
mod inspector_motion_tests;
#[cfg(test)]
mod selection_tests;
#[cfg(test)]
mod side_panel_tests;
