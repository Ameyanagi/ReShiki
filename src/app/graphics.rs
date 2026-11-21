use super::workspace::{control, hover_hint, muted_text, section};
use super::*;
use crate::canvas::layered::canvas;
use iced::widget::{checkbox, column, container, row, text, tooltip};
use iced::{Alignment, Length};
use reshiki::graphics::GraphicKind;
use reshiki::palette::{Color as Paint, Palette, Row};
mod heads;
pub(super) mod path;
pub use heads::Field as HeadField;
mod path_cleanup;

const ATTACH_SYMBOLS_HINT: &str = concat!(
    "Attached charges and radicals update chemistry. Lone pairs annotate the atom. ",
    "H and attachment symbols use free placement."
);
pub(super) const SNAP_ORBITALS_LABEL: &str = "Snap to atoms";

#[cfg(test)]
mod tests;

#[cfg(test)]
mod path_cleanup_tests;
#[cfg(test)]
mod path_tests;

#[derive(Debug, Clone, Copy)]
pub enum Cleanup {
    Smooth,
    Symmetrise { upright: bool },
}

pub(super) const SYMMETRISE_HINT: &str =
    "Average across the best mirror axis · Shift: upright axis";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Superscript {
    #[default]
    None,
    TransitionState,
    Positive,
    Negative,
    RadicalPositive,
    RadicalNegative,
    Custom,
}

impl Superscript {
    const ALL: [Self; 7] = [
        Self::None,
        Self::TransitionState,
        Self::Positive,
        Self::Negative,
        Self::RadicalPositive,
        Self::RadicalNegative,
        Self::Custom,
    ];

    pub(super) fn of(text: Option<&str>) -> Self {
        Self::ALL
            .into_iter()
            .find(|choice| *choice != Self::Custom && choice.text("") == text)
            .unwrap_or(Self::Custom)
    }

    pub(super) fn text(self, custom: &str) -> Option<&str> {
        match self {
            Self::None => None,
            Self::TransitionState => Some("‡"),
            Self::Positive => Some("+"),
            Self::Negative => Some("−"),
            Self::RadicalPositive => Some("•+"),
            Self::RadicalNegative => Some("•−"),
            Self::Custom => (!custom.is_empty()).then_some(custom),
        }
    }
}

impl std::fmt::Display for Superscript {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::None => "None",
            Self::Custom => "Custom…",
            choice => choice.text("").unwrap_or_default(),
        })
    }
}

#[derive(Clone, Copy)]
pub(super) enum ColorField {
    Stroke,
    Fill,
}

/// The fill's gradient, as the Properties list names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GradientChoice {
    None,
    Linear,
    Radial,
}
impl GradientChoice {
    pub(super) const ALL: [Self; 3] = [Self::None, Self::Linear, Self::Radial];
    fn of(style: &reshiki::graphics::GraphicStyle) -> Self {
        match style.effects.gradient.map(|g| g.kind) {
            None => Self::None,
            Some(reshiki::graphics::GradientKind::Linear { .. }) => Self::Linear,
            Some(reshiki::graphics::GradientKind::Radial) => Self::Radial,
        }
    }
}
impl std::fmt::Display for GradientChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::None => "None",
            Self::Linear => "Linear",
            Self::Radial => "Radial",
        })
    }
}
/// The angle a new linear gradient runs at: top to bottom.
const DOWN: f32 = 90.;

/// Graphic style and scientific-symbol inputs, nested under `Message::Graphics`.
/// Applied style and superscript changes commit inline and join drafts.
#[derive(Debug, Clone)]
pub enum Action {
    Style(GraphicChange),
    Width(String),
    ApplyWidth,
    Corner(String),
    ApplyCorner,
    Opacity(String),
    ApplyOpacity,
    Gradient(GradientChoice),
    GradientFrom(String),
    ApplyGradientFrom,
    Angle(String),
    ApplyAngle,
    Stroke(String),
    ApplyStroke,
    Fill(String),
    ApplyFill,
    Head(reshiki::arrows::Head),
    Tail(reshiki::arrows::Head),
    HeadShape(reshiki::arrows::HeadShape),
    HeadNumber(HeadField, String),
    ApplyHeadNumber(HeadField),
    Sides(BracketSides),
    Superscript(Superscript),
    SuperscriptText(String),
    ApplySuperscript,
    Path(path::Action),
    Cleanup(Cleanup),
    ScientificKind(GraphicKind),
    OrbitalPhase(reshiki::scientific::Phase),
    FlipPhase(bool),
    AttachSymbols(bool),
    SnapOrbitals(bool),
}

/// Hundredths of a point, so an imported 0.99 pt reads "0.99".
fn points(value: f32) -> String {
    ((value * 100.).round() / 100.).to_string()
}

pub(super) fn parse_color(s: &str) -> Option<[u8; 3]> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 || !s.is_ascii() {
        return None;
    }
    Some([
        u8::from_str_radix(s.get(0..2)?, 16).ok()?,
        u8::from_str_radix(s.get(2..4)?, 16).ok()?,
        u8::from_str_radix(s.get(4..6)?, 16).ok()?,
    ])
}
impl App {
    pub(super) fn selected_styled_graphics(
        &self,
    ) -> impl Iterator<Item = &reshiki::graphics::Graphic> {
        self.tab.doc.graphics.iter().filter(|g| {
            self.tab.selected.contains(&g.id)
                && g.picture.is_none()
                && g.piece.is_none()
                && g.tlc.is_none()
        })
    }

    pub(super) fn graphic_panel_visible(&self) -> bool {
        matches!(self.tool, Tool::Graphic(_)) || self.selected_styled_graphics().next().is_some()
    }

    pub(super) fn graphic_fill_controls(&self) -> bool {
        let mut selected = self.selected_styled_graphics();
        let first = selected.next();
        let kind = match self.tool {
            Tool::Graphic(kind) => kind,
            _ => first.map_or(GraphicKind::Rectangle, |g| g.kind),
        };
        let mut shapes = first.into_iter().chain(selected);
        if kind.closed() {
            !shapes.any(|g| g.cycle.is_some())
        } else {
            kind == GraphicKind::Path
                && shapes.any(|g| {
                    g.commands()
                        .iter()
                        .any(|c| matches!(c, reshiki::graphics::PathCommand::Close))
                })
        }
    }

    pub(super) fn graphic_action(&mut self, action: Action) {
        match action {
            Action::Style(change) => self.apply_graphic_style(change),
            Action::Width(s) => self.tab.graphic_width_input = s,
            Action::ApplyWidth => self.apply_graphic_width(),
            Action::Corner(s) => self.tab.graphic_corner_input = s,
            Action::ApplyCorner => self.apply_graphic_corner(),
            Action::Opacity(s) => self.tab.graphic_opacity_input = s,
            Action::ApplyOpacity => self.apply_graphic_opacity(),
            Action::Gradient(choice) => self.set_gradient(choice),
            Action::GradientFrom(s) => self.tab.graphic_gradient_input = s,
            Action::ApplyGradientFrom => self.apply_gradient_from(),
            Action::Angle(s) => self.tab.graphic_angle_input = s,
            Action::ApplyAngle => self.apply_gradient_angle(),
            Action::Stroke(s) => self.tab.graphic_stroke_input = s,
            Action::ApplyStroke => self.apply_graphic_color(ColorField::Stroke),
            Action::Fill(s) => self.tab.graphic_fill_input = s,
            Action::ApplyFill => self.apply_graphic_color(ColorField::Fill),
            Action::Head(_)
            | Action::Tail(_)
            | Action::HeadShape(_)
            | Action::HeadNumber(..)
            | Action::ApplyHeadNumber(_) => self.path_head_action(action),
            Action::ScientificKind(kind) => self.set_scientific_kind(kind),
            Action::OrbitalPhase(phase) => self.set_orbital_phase(phase),
            Action::FlipPhase(value) => self.set_phase_flipped(value),
            Action::AttachSymbols(value) => self.tab.attach_symbols = value,
            Action::SnapOrbitals(value) => self.tab.snap_orbitals = value,
            Action::Sides(sides) => self.set_graphic_sides(sides),
            Action::Superscript(choice) => {
                self.tab.bracket_superscript = choice;
                if choice != Superscript::Custom {
                    self.apply_bracket_superscript();
                }
            }
            Action::SuperscriptText(text) => self.tab.bracket_superscript_input = text,
            Action::ApplySuperscript => self.apply_bracket_superscript(),
            Action::Path(action) => self.path_action(action),
            Action::Cleanup(command) => self.cleanup_paths(command),
        }
    }
    /// The orbital tool's one Snap to atoms option, in the context row like
    /// Ring's Aromatic; `options_width` measures its label.
    pub(super) fn snap_orbitals_toggle(&self) -> Element<'_, Message> {
        hover_hint(
            checkbox(self.tab.snap_orbitals)
                .label(SNAP_ORBITALS_LABEL)
                .on_toggle(|snap| Message::Graphics(Action::SnapOrbitals(snap)))
                .size(14)
                .text_size(11),
            format!(
                "Snap the orbital node to the nearest atom · Hold {} to place freely · Applies to new orbitals",
                crate::platform::alt_key()
            ),
            tooltip::Position::Bottom,
        )
        .into()
    }
    pub(super) fn graphic_panel(&self) -> Element<'_, Message> {
        use reshiki::graphics::{BracketSides, GraphicChange, GraphicKind, LinePattern};
        let selected: Vec<_> = self.selected_styled_graphics().collect();
        // A cycle's style colors its arrows; it has no fill.
        let cycle = selected.iter().any(|g| g.cycle.is_some());
        let kind = match self.tool {
            Tool::Graphic(k) => k,
            _ => selected
                .first()
                .map(|g| g.kind)
                .unwrap_or(GraphicKind::Rectangle),
        };
        // The same palette rows as the color popover, on the canvas color.
        let palette = Palette::of(&self.tab.doc);
        let hues = reshiki::palette::Hues::of(&self.tab.doc);
        let style = &self.tab.graphic_style;
        let swatches =
            |row: Row, first: (bool, Option<Message>), change: fn(Paint) -> GraphicChange| {
                super::color_popover::paper(
                    super::color_popover::palette_row(&palette, row, hues, 20., first, |hue| {
                        let color = Paint::Palette(hue, row);
                        let current = if row == Row::Strong {
                            style.stroke == color
                        } else {
                            style.fill == Some(color)
                        };
                        (
                            current,
                            Some(Message::Graphics(Action::Style(change(color)))),
                        )
                    }),
                    self.tab.doc.canvas_theme,
                )
                .padding([6, 6])
            };
        let mut panel = column![
            section(if selected.is_empty() {
                "Drawing style"
            } else {
                "Graphic style"
            }),
            text(if selected.iter().any(|g| g.panel.is_some()) {
                "Panel".to_owned()
            } else if cycle {
                "Cycle arrows".to_owned()
            } else if let Some(icon) = selected.first().and_then(|g| g.icon.as_ref()) {
                format!("Icon · {}", icon.name.replace('-', " "))
            } else {
                kind.to_string()
            })
            .size(14),
            row![
                text("Line (pt)").size(11),
                reshiki::accessibility::text_input(
                    "graphic-line-width",
                    "Graphic line width (pt)",
                    "0.6",
                    &self.tab.graphic_width_input,
                )
                .style(crate::appearance::input_style)
                .on_input(|width| Message::Graphics(Action::Width(width)))
                .on_submit(Message::Graphics(Action::ApplyWidth))
                .size(12)
                .padding(5)
                .width(48),
                crate::appearance::pick_list(
                    // Symbols and orbitals draw no waves.
                    if matches!(
                        kind,
                        GraphicKind::Symbol(_) | GraphicKind::Orbital(_) | GraphicKind::Icon
                    ) {
                        LinePattern::PLAIN.as_slice()
                    } else {
                        LinePattern::ALL.as_slice()
                    },
                    Some(self.tab.graphic_style.pattern),
                    |p| Message::Graphics(Action::Style(GraphicChange::Pattern(p)))
                )
                .text_size(12)
                .padding(5)
            ]
            .spacing(6)
            .align_y(Alignment::Center),
            text("Stroke color").size(11).style(muted_text),
            swatches(
                Row::Strong,
                (
                    style.stroke == Paint::Ink,
                    Some(Message::Graphics(Action::Style(GraphicChange::Stroke(
                        Paint::Ink
                    ))))
                ),
                GraphicChange::Stroke
            ),
            crate::appearance::text_input("#RRGGBB", &self.tab.graphic_stroke_input)
                .on_input(|stroke| Message::Graphics(Action::Stroke(stroke)))
                .on_submit(Message::Graphics(Action::ApplyStroke))
                .size(12)
                .padding(6),
        ]
        .spacing(9);
        if matches!(
            kind,
            GraphicKind::Symbol(_) | GraphicKind::Orbital(_) | GraphicKind::Icon
        ) {
            let mut preview = selected.first().map(|g| (*g).clone()).unwrap_or_else(|| {
                reshiki::graphics::Graphic::dragged(
                    1,
                    kind,
                    reshiki::document::Point::default(),
                    reshiki::document::Point::default(),
                    self.tab.graphic_style.clone(),
                    self.tab.bracket_sides,
                    false,
                )
            });
            preview.phase = self.tab.orbital_phase;
            preview.phase_flipped = self.tab.phase_flipped;
            panel = panel.push(container(
                canvas(crate::canvas::ScientificPreview(preview))
                    .width(Length::Fill)
                    .height(92),
            ));
        }
        match kind {
            GraphicKind::Symbol(kind) => {
                panel = panel.push(
                    crate::appearance::pick_list(
                        reshiki::scientific::SymbolKind::ALL,
                        Some(kind),
                        |k| Message::Graphics(Action::ScientificKind(GraphicKind::Symbol(k))),
                    )
                    .text_size(12)
                    .padding(6)
                    .width(Length::Fill),
                );
                // Condition icons are free shapes; they never attach.
                if !kind.is_condition() {
                    panel = panel.push(hover_hint(
                        checkbox(self.tab.attach_symbols)
                            .label("Attach to atoms")
                            .on_toggle(|attach| Message::Graphics(Action::AttachSymbols(attach)))
                            .size(14)
                            .text_size(12),
                        ATTACH_SYMBOLS_HINT,
                        tooltip::Position::Top,
                    ));
                }
            }
            GraphicKind::Orbital(kind) => {
                panel = panel
                    .push(hover_hint(
                        crate::appearance::pick_list(
                            reshiki::scientific::OrbitalKind::ALL,
                            Some(kind),
                            |k| Message::Graphics(Action::ScientificKind(GraphicKind::Orbital(k))),
                        )
                        .text_size(12)
                        .padding(6)
                        .width(Length::Fill),
                        format!(
                            "Drag from the orbital node to set direction and size. Click uses one bond length. Shift snaps to 15°. {} bypasses atom snapping. Group with a molecule to move them together.",
                            crate::platform::alt_key()
                        ),
                        tooltip::Position::Top,
                    ))
                    .push(
                        crate::appearance::pick_list(
                            reshiki::scientific::Phase::ALL,
                            Some(self.tab.orbital_phase),
                            |phase| Message::Graphics(Action::OrbitalPhase(phase)),
                        )
                        .text_size(12)
                        .padding(6)
                        .width(Length::Fill),
                    )
                    .push(
                        checkbox(self.tab.phase_flipped)
                            .label("Reverse phases")
                            .on_toggle_maybe(
                                (!matches!(
                                    kind,
                                    reshiki::scientific::OrbitalKind::S
                                        | reshiki::scientific::OrbitalKind::Sigma
                                        | reshiki::scientific::OrbitalKind::Lobe
                                ))
                                .then_some(|flipped: bool| {
                                    Message::Graphics(Action::FlipPhase(flipped))
                                }),
                            )
                            .size(14)
                            .text_size(12),
                    );
            }
            _ => {}
        }
        if self.graphic_fill_controls() {
            // A panel's fill swatches and shadow are in its Panel card.
            let panels = selected.iter().any(|g| g.panel.is_some());
            if !panels {
                panel = panel.push(text("Fill color").size(11).style(muted_text));
                panel = panel.push(swatches(
                    Row::Tint,
                    (
                        style.fill.is_none(),
                        Some(Message::Graphics(Action::Style(GraphicChange::Fill(None)))),
                    ),
                    |color| GraphicChange::Fill(Some(color)),
                ));
            }
            let mut effects = row![hover_hint(
                checkbox(style.effects.shaded)
                    .label("Shaded")
                    .on_toggle(|on| { Message::Graphics(Action::Style(GraphicChange::Shaded(on))) })
                    .size(14)
                    .text_size(12),
                "Fill from white at the upper left to the fill color, as ChemDraw shades",
                tooltip::Position::Top,
            )]
            .spacing(14);
            if !panels {
                effects = effects.push(hover_hint(
                    checkbox(style.effects.shadow_pt.is_some())
                        .label("Shadow")
                        .on_toggle(|on| {
                            Message::Graphics(Action::Style(GraphicChange::Shadow(
                                on.then_some(reshiki::graphics::Effects::SHADOW_PT),
                            )))
                        })
                        .size(14)
                        .text_size(12),
                    "A grey drop shadow below and to the right",
                    tooltip::Position::Top,
                ));
            }
            if panels {
                panel = panel.push(text("Custom fill color").size(11).style(muted_text));
            }
            panel = panel
                .push(
                    crate::appearance::text_input("#RRGGBB", &self.tab.graphic_fill_input)
                        .on_input(|fill| Message::Graphics(Action::Fill(fill)))
                        .on_submit(Message::Graphics(Action::ApplyFill))
                        .size(12)
                        .padding(6),
                )
                .push(effects)
                .push(
                    row![
                        text("Gradient").size(11),
                        crate::appearance::pick_list(
                            GradientChoice::ALL,
                            Some(GradientChoice::of(style)),
                            |choice| Message::Graphics(Action::Gradient(choice)),
                        )
                        .text_size(12)
                        .padding(5)
                        .width(Length::Fill),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                );
            if let Some(gradient) = style.effects.gradient {
                let mut controls = row![
                    text("From").size(11),
                    hover_hint(
                        reshiki::accessibility::text_input(
                            "graphic-gradient-from",
                            "Gradient start color",
                            "Paper",
                            &self.tab.graphic_gradient_input,
                        )
                        .style(crate::appearance::input_style)
                        .on_input(|value| Message::Graphics(Action::GradientFrom(value)))
                        .on_submit(Message::Graphics(Action::ApplyGradientFrom))
                        .size(12)
                        .padding(5)
                        .width(76),
                        "Paper (the canvas color) or a color · Enter applies · the gradient ends at the fill color",
                        tooltip::Position::Top,
                    ),
                ]
                .spacing(6)
                .align_y(Alignment::Center);
                if matches!(
                    gradient.kind,
                    reshiki::graphics::GradientKind::Linear { .. }
                ) {
                    controls = controls.push(text("Angle (°)").size(11)).push(hover_hint(
                        reshiki::accessibility::text_input(
                            "graphic-gradient-angle",
                            "Gradient angle (degrees)",
                            "90",
                            &self.tab.graphic_angle_input,
                        )
                        .style(crate::appearance::input_style)
                        .on_input(|value| Message::Graphics(Action::Angle(value)))
                        .on_submit(Message::Graphics(Action::ApplyAngle))
                        .size(12)
                        .padding(5)
                        .width(48),
                        "Enter applies · 0 runs left to right, 90 top to bottom",
                        tooltip::Position::Top,
                    ));
                }
                panel = panel.push(controls);
            }
        }
        panel = panel.push(
            row![
                text("Opacity (%)").size(11),
                hover_hint(
                    reshiki::accessibility::text_input(
                        "graphic-opacity",
                        "Opacity (%)",
                        "100",
                        &self.tab.graphic_opacity_input,
                    )
                    .style(crate::appearance::input_style)
                    .on_input(|value| Message::Graphics(Action::Opacity(value)))
                    .on_submit(Message::Graphics(Action::ApplyOpacity))
                    .size(12)
                    .padding(5)
                    .width(48),
                    "Enter applies · fades the fill, or the lines of a shape without one",
                    tooltip::Position::Top,
                ),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        );
        if kind == GraphicKind::RoundedRectangle {
            panel = panel.push(
                row![
                    text("Corner (pt)").size(11),
                    hover_hint(
                        reshiki::accessibility::text_input(
                            "graphic-corner-radius",
                            "Corner radius (pt)",
                            "Auto",
                            &self.tab.graphic_corner_input,
                        )
                        .style(crate::appearance::input_style)
                        .on_input(|value| Message::Graphics(Action::Corner(value)))
                        .on_submit(Message::Graphics(Action::ApplyCorner))
                        .size(12)
                        .padding(5)
                        .width(56),
                        "Enter applies · empty rounds to the shape",
                        tooltip::Position::Top,
                    ),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            );
        }
        if kind == GraphicKind::Arc {
            panel = panel.push(self.arc_controls());
        }
        if let Some(graphic) = self.selected_open_paths().next() {
            panel = panel.push(self.path_head_controls(graphic.heads.unwrap_or_default()));
        }
        if kind == GraphicKind::Path
            || matches!(selected.as_slice(),[g] if matches!(g.kind,GraphicKind::Path|GraphicKind::Curve))
        {
            panel = panel.push(self.path_controls());
        }
        if kind.brackets() || selected.iter().any(|g| g.kind.brackets()) {
            panel = panel.push(
                crate::appearance::pick_list(
                    [BracketSides::Both, BracketSides::Left, BracketSides::Right],
                    Some(self.tab.bracket_sides),
                    |sides| Message::Graphics(Action::Sides(sides)),
                )
                .text_size(12)
                .padding(6),
            );
            panel = panel
                .push(text("Superscript").size(11).style(muted_text))
                .push(
                    crate::appearance::pick_list(
                        Superscript::ALL,
                        Some(self.tab.bracket_superscript),
                        |choice| Message::Graphics(Action::Superscript(choice)),
                    )
                    .text_size(12)
                    .padding(6)
                    .width(Length::Fill),
                );
            if self.tab.bracket_superscript == Superscript::Custom {
                panel = panel.push(
                    reshiki::accessibility::text_input(
                        "bracket-superscript",
                        "Bracket superscript",
                        "Custom superscript",
                        &self.tab.bracket_superscript_input,
                    )
                    .style(crate::appearance::input_style)
                    .on_input(|text| Message::Graphics(Action::SuperscriptText(text)))
                    .on_submit(Message::Graphics(Action::ApplySuperscript))
                    .size(12)
                    .padding(6),
                );
            }
        }
        // Editing points ends with Done in the context row or Escape.
        if self.tool != Tool::EditPoints
            && matches!(selected.as_slice(), [g] if matches!(g.kind, GraphicKind::Curve | GraphicKind::Path | GraphicKind::Arc))
        {
            let (id, label) = if kind == GraphicKind::Arc {
                ("arc-edit-endpoints", "Edit arc endpoints")
            } else {
                ("curve-edit-points", "Edit curve points")
            };
            panel = panel.push(
                reshiki::accessibility::button(id, label, text(label).size(12))
                    .padding([7, 9])
                    .on_press(Message::Tool(Tool::EditPoints))
                    .style(control(false)),
            );
        }
        panel
            .push(
                text(match kind {
                    GraphicKind::Arc if selected.is_empty() => {
                        "Drag to draw · Shift makes a circle."
                    }
                    GraphicKind::Arc => "Drag the endpoints to adjust the curve.",
                    _ if kind.brackets() => "Enter applies typed numbers, colors and superscripts.",
                    _ => "Enter applies typed numbers and colors.",
                })
                .size(11)
                .style(muted_text),
            )
            .into()
    }

    pub(super) fn sync_graphics(&mut self) {
        self.sync_pictures();
        self.sync_arc();
        self.sync_tlc();
        if let Some(g) = self
            .tab
            .doc
            .graphics
            .iter()
            .find(|g| self.tab.selected.contains(&g.id))
        {
            // A condition icon's paper disc stays with the icon, not the next shape.
            let fill = self.tab.graphic_style.fill;
            self.tab.graphic_style = g.style.clone();
            if g.is_icon() {
                self.tab.graphic_style.fill = fill;
            }
            self.tab.orbital_phase = g.phase;
            self.tab.phase_flipped = g.phase_flipped;
            self.tab.bracket_sides = g.sides;
            if let Some(unit) = self
                .tab
                .doc
                .generic
                .repeats
                .iter()
                .find(|u| u.drawn_with(g.id))
            {
                self.generic.label = unit.label.clone();
                self.generic.pattern = unit.pattern;
            }
            self.show_repeat_label_offset();
            if !matches!(
                self.inspector_tab,
                InspectorTab::Templates
                    | InspectorTab::Reactions
                    | InspectorTab::Assistant
                    | InspectorTab::Pages
            ) {
                self.inspector_open = true;
                self.inspector_tab = InspectorTab::Properties;
            }
        }
        if let Some(bracket) = self
            .tab
            .doc
            .graphics
            .iter()
            .find(|g| self.tab.selected.contains(&g.id) && g.kind.brackets())
        {
            self.tab.bracket_superscript = Superscript::of(bracket.superscript.as_deref());
            self.tab.bracket_superscript_input = bracket.superscript.clone().unwrap_or_default();
        }
        self.tab.graphic_width_input = points(self.tab.graphic_style.width_pt);
        self.tab.graphic_corner_input = self
            .tab
            .graphic_style
            .corner_pt
            .map(points)
            .unwrap_or_default();
        let heads = self
            .selected_open_paths()
            .next()
            .and_then(|g| g.heads)
            .unwrap_or_default();
        self.tab.path_head_inputs = HeadField::ALL.map(|field| field.get(&heads).to_string());
        let palette = reshiki::palette::Palette::of(&self.tab.doc);
        let hex = |c| reshiki::palette::hex(palette.rgb(c));
        self.tab.graphic_stroke_input = hex(self.tab.graphic_style.stroke);
        if let Some(c) = self.tab.graphic_style.fill {
            self.tab.graphic_fill_input = hex(c);
        }
        let effects = self.tab.graphic_style.effects;
        self.tab.graphic_opacity_input = effects
            .opacity
            .map_or(100., |a| (a * 1000.).round() / 10.)
            .to_string();
        if let Some(gradient) = effects.gradient {
            self.tab.graphic_gradient_input = if gradient.from == Paint::Paper {
                "Paper".into()
            } else {
                hex(gradient.from)
            };
            if let reshiki::graphics::GradientKind::Linear { angle } = gradient.kind {
                self.tab.graphic_angle_input = points(angle);
            }
        }
        self.sync_figure();
        self.sync_cycle();
    }
    pub(super) fn apply_graphic_style(&mut self, change: GraphicChange) {
        change.apply(&mut self.tab.graphic_style);
        let before = self.tab.doc.clone();
        for g in self
            .selected_graphics_mut()
            .filter(|g| g.picture.is_none() && g.tlc.is_none())
        {
            // Symbols and orbitals draw no waves.
            if matches!(
                change,
                GraphicChange::Pattern(reshiki::graphics::LinePattern::Wavy)
            ) && matches!(
                g.kind,
                GraphicKind::Symbol(_) | GraphicKind::Orbital(_) | GraphicKind::Icon
            ) {
                continue;
            }
            // An icon keeps its disc when a selection's fill or shading changes.
            if matches!(change, GraphicChange::Fill(_) | GraphicChange::Shaded(_)) && g.is_icon() {
                continue;
            }
            // Only a closed shape has an inside for a gradient to fill.
            if matches!(
                change,
                GraphicChange::Gradient(Some(_)) | GraphicChange::GradientKind(Some(_))
            ) && !g.closed_shape()
            {
                continue;
            }
            change.apply(&mut g.style);
        }
        if let GraphicChange::Stroke(color) | GraphicChange::Fill(Some(color)) = change {
            self.remember_custom(Some(color), &before);
        }
        self.changed(before);
        self.sync_graphics();
    }

    pub(super) fn apply_graphic_width(&mut self) {
        match self.tab.graphic_width_input.parse::<f32>() {
            Ok(width) if width.is_finite() && (0.1..=12.0).contains(&width) => {
                self.apply_graphic_style(GraphicChange::Width(width))
            }
            _ => {
                self.error = true;
                self.status = "Line width must be 0.1–12 pt".into();
            }
        }
    }

    pub(super) fn apply_graphic_corner(&mut self) {
        let input = self.tab.graphic_corner_input.trim();
        match input.parse::<f32>() {
            _ if input.is_empty() => self.apply_graphic_style(GraphicChange::Corner(None)),
            Ok(radius)
                if radius.is_finite()
                    && (0. ..=reshiki::graphics::MAX_CORNER_PT).contains(&radius) =>
            {
                self.apply_graphic_style(GraphicChange::Corner(Some(radius)))
            }
            _ => {
                self.error = true;
                self.status = "Corner radius must be 0–1000 pt".into();
            }
        }
    }

    pub(super) fn apply_graphic_opacity(&mut self) {
        let input = self.tab.graphic_opacity_input.trim().trim_end_matches('%');
        match input.trim().parse::<f32>() {
            Ok(100.) => self.apply_graphic_style(GraphicChange::Opacity(None)),
            // ChemDraw's FadePercent keeps tenths of a percent.
            Ok(percent) if percent.is_finite() && (1. ..=99.).contains(&percent) => self
                .apply_graphic_style(GraphicChange::Opacity(Some(
                    (percent * 10.).round() / 1000.,
                ))),
            _ => {
                self.error = true;
                self.status = "Opacity must be 1–99%, or 100 for none".into();
            }
        }
    }

    /// Each selected shape's gradient as `choice`, keeping its own start
    /// color and angle.
    pub(super) fn set_gradient(&mut self, choice: GradientChoice) {
        use reshiki::graphics::GradientKind;
        self.apply_graphic_style(GraphicChange::GradientKind(match choice {
            GradientChoice::None => None,
            GradientChoice::Linear => Some(GradientKind::Linear { angle: DOWN }),
            GradientChoice::Radial => Some(GradientKind::Radial),
        }));
    }

    pub(super) fn apply_gradient_from(&mut self) {
        let input = self.tab.graphic_gradient_input.trim();
        let from = if input.eq_ignore_ascii_case("paper") {
            Some(Paint::Paper)
        } else {
            parse_color(input).map(Paint::Custom)
        };
        match from {
            Some(from) => self.apply_graphic_style(GraphicChange::GradientFrom(from)),
            None => {
                self.error = true;
                self.status = "Enter Paper or a six-digit hex color, such as #FFFFFF".into();
            }
        }
    }

    pub(super) fn apply_gradient_angle(&mut self) {
        match self.tab.graphic_angle_input.trim().parse::<f32>() {
            Ok(angle) if angle.is_finite() && angle.abs() <= 360. => {
                self.apply_graphic_style(GraphicChange::GradientAngle(angle))
            }
            _ => {
                self.error = true;
                self.status = "Angle must be within ±360°".into();
            }
        }
    }

    pub(super) fn apply_graphic_color(&mut self, field: ColorField) {
        let input = match field {
            ColorField::Stroke => &self.tab.graphic_stroke_input,
            ColorField::Fill => &self.tab.graphic_fill_input,
        };
        if let Some(rgb) = parse_color(input) {
            let color = Paint::Custom(rgb);
            let change = match field {
                ColorField::Stroke => GraphicChange::Stroke(color),
                ColorField::Fill => GraphicChange::Fill(Some(color)),
            };
            self.apply_graphic_style(change);
        } else {
            self.error = true;
            self.status = match field {
                ColorField::Stroke => "Enter a six-digit hex color, such as #117E6C",
                ColorField::Fill => "Enter a six-digit hex color, such as #DCEFE9",
            }
            .into();
        }
    }

    // Selection is shared; each operation retains its own picture and kind filters.
    pub(super) fn selected_open_paths(&self) -> impl Iterator<Item = &Graphic> {
        self.tab
            .doc
            .graphics
            .iter()
            .filter(|g| self.tab.selected.contains(&g.id) && is_open_path(g))
    }

    /// The selected graphics that take a graphic style: not equipment pieces.
    fn selected_graphics_mut(&mut self) -> impl Iterator<Item = &mut Graphic> {
        let selected = &self.tab.selected;
        self.tab
            .doc
            .graphics
            .iter_mut()
            .filter(move |g| selected.contains(&g.id) && g.piece.is_none())
    }

    pub(super) fn set_scientific_kind(&mut self, kind: GraphicKind) {
        self.toolbar.remember(Tool::Graphic(kind));
        let before = self.tab.doc.clone();
        for g in self.selected_graphics_mut() {
            if matches!(
                (g.kind, kind),
                (GraphicKind::Symbol(_), GraphicKind::Symbol(_))
                    | (GraphicKind::Orbital(_), GraphicKind::Orbital(_))
            ) {
                g.kind = kind;
                if let GraphicKind::Symbol(symbol) = kind {
                    g.style.fill = symbol.fill();
                }
            }
        }
        self.tool = Tool::Graphic(kind);
        self.changed(before);
    }

    pub(super) fn set_orbital_phase(&mut self, phase: reshiki::scientific::Phase) {
        self.tab.orbital_phase = phase;
        let before = self.tab.doc.clone();
        for g in self.selected_graphics_mut() {
            g.phase = phase;
        }
        self.changed(before);
    }

    pub(super) fn set_phase_flipped(&mut self, value: bool) {
        self.tab.phase_flipped = value;
        let before = self.tab.doc.clone();
        for g in self.selected_graphics_mut() {
            g.phase_flipped = value;
        }
        self.changed(before);
    }

    pub(super) fn set_graphic_sides(&mut self, sides: BracketSides) {
        let before = self.tab.doc.clone();
        self.tab.bracket_sides = sides;
        for g in self.selected_graphics_mut() {
            g.sides = sides;
        }
        self.changed(before);
    }

    fn apply_bracket_superscript(&mut self) {
        let text = self
            .tab
            .bracket_superscript
            .text(&self.tab.bracket_superscript_input)
            .map(str::to_owned);
        let before = self.tab.doc.clone();
        for graphic in self.selected_graphics_mut().filter(|g| g.kind.brackets()) {
            graphic.superscript = text.clone();
        }
        self.changed(before);
    }
}

fn is_open_path(graphic: &Graphic) -> bool {
    graphic.kind == GraphicKind::Line || graphic.path_summary().is_some_and(|path| !path.closed)
}
