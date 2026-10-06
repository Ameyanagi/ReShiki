use super::workspace::{control, hover_hint, muted_text, section};
use super::*;
use crate::canvas::layered::canvas;
use iced::widget::{checkbox, column, container, row, text, tooltip};
use iced::{Alignment, Length};
use reshiki::graphics::GraphicKind;
use reshiki::palette::{Color as Paint, Palette, Row};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy)]
pub(super) enum ColorField {
    Stroke,
    Fill,
}

/// Graphic style and scientific-symbol inputs, nested under `Message::Graphics`.
/// Only `Style` commits inline and join drafts.
#[derive(Debug, Clone)]
pub enum Action {
    Style(GraphicChange),
    Width(String),
    ApplyWidth,
    Stroke(String),
    ApplyStroke,
    Fill(String),
    ApplyFill,
    Sides(BracketSides),
    ScientificKind(GraphicKind),
    OrbitalPhase(reshiki::scientific::Phase),
    FlipPhase(bool),
    AttachSymbols(bool),
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
    pub(super) fn graphic_action(&mut self, action: Action) {
        match action {
            Action::Style(change) => self.apply_graphic_style(change),
            Action::Width(s) => self.tab.graphic_width_input = s,
            Action::ApplyWidth => self.apply_graphic_width(),
            Action::Stroke(s) => self.tab.graphic_stroke_input = s,
            Action::ApplyStroke => self.apply_graphic_color(ColorField::Stroke),
            Action::Fill(s) => self.tab.graphic_fill_input = s,
            Action::ApplyFill => self.apply_graphic_color(ColorField::Fill),
            Action::ScientificKind(kind) => self.set_scientific_kind(kind),
            Action::OrbitalPhase(phase) => self.set_orbital_phase(phase),
            Action::FlipPhase(value) => self.set_phase_flipped(value),
            Action::AttachSymbols(value) => self.tab.attach_symbols = value,
            Action::Sides(sides) => self.set_graphic_sides(sides),
        }
    }
    pub(super) fn graphic_panel(&self) -> Element<'_, Message> {
        use reshiki::graphics::{BracketSides, GraphicChange, GraphicKind, LinePattern};
        let selected: Vec<_> = self
            .tab
            .doc
            .graphics
            .iter()
            .filter(|g| self.tab.selected.contains(&g.id) && g.picture.is_none())
            .collect();
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
                "DRAWING STYLE"
            } else {
                "GRAPHIC STYLE"
            }),
            text(kind.to_string()).size(14),
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
                    [LinePattern::Solid, LinePattern::Dashed, LinePattern::Dotted],
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
        if matches!(kind, GraphicKind::Symbol(_) | GraphicKind::Orbital(_)) {
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
                panel=panel.push(crate::appearance::pick_list(reshiki::scientific::SymbolKind::ALL,Some(kind),|k| Message::Graphics(Action::ScientificKind(GraphicKind::Symbol(k)))).text_size(12).padding(6).width(Length::Fill))
                    .push(hover_hint(checkbox(self.tab.attach_symbols).label("Attach to atoms").on_toggle(|attach| Message::Graphics(Action::AttachSymbols(attach))).size(14).text_size(12), "Attached charges and radicals update chemistry. Lone pairs annotate the atom. H and attachment symbols use free placement.", tooltip::Position::Top));
            }
            GraphicKind::Orbital(kind) => {
                panel=panel.push(hover_hint(crate::appearance::pick_list(reshiki::scientific::OrbitalKind::ALL,Some(kind),|k| Message::Graphics(Action::ScientificKind(GraphicKind::Orbital(k)))).text_size(12).padding(6).width(Length::Fill), "Drag from the orbital node to set direction and size. Click uses one bond length. Shift snaps to 15°. Group with a molecule to move them together.", tooltip::Position::Top))
                    .push(crate::appearance::pick_list(reshiki::scientific::Phase::ALL,Some(self.tab.orbital_phase),|phase| Message::Graphics(Action::OrbitalPhase(phase))).text_size(12).padding(6).width(Length::Fill))
                    .push(checkbox(self.tab.phase_flipped).label("Reverse phases").on_toggle_maybe((!matches!(kind, reshiki::scientific::OrbitalKind::S | reshiki::scientific::OrbitalKind::Sigma | reshiki::scientific::OrbitalKind::Lobe)).then_some(|flipped: bool| Message::Graphics(Action::FlipPhase(flipped)))).size(14).text_size(12));
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
                .push(text("Fill color").size(11).style(muted_text))
                .push(swatches(
                    Row::Tint,
                    (
                        style.fill.is_none(),
                        Some(Message::Graphics(Action::Style(GraphicChange::Fill(None)))),
                    ),
                    |color| GraphicChange::Fill(Some(color)),
                ))
                .push(
                    crate::appearance::text_input("#RRGGBB", &self.tab.graphic_fill_input)
                        .on_input(|fill| Message::Graphics(Action::Fill(fill)))
                        .on_submit(Message::Graphics(Action::ApplyFill))
                        .size(12)
                        .padding(6),
                );
        }
        if kind == GraphicKind::Arc {
            panel = panel.push(self.arc_controls());
        }
        if kind.brackets() {
            panel = panel.push(
                crate::appearance::pick_list(
                    [BracketSides::Both, BracketSides::Left, BracketSides::Right],
                    Some(self.tab.bracket_sides),
                    |sides| Message::Graphics(Action::Sides(sides)),
                )
                .text_size(12)
                .padding(6),
            );
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
                    _ => "Enter applies typed widths and colors.",
                })
                .size(11)
                .style(muted_text),
            )
            .into()
    }

    pub(super) fn sync_graphics(&mut self) {
        self.sync_pictures();
        self.sync_arc();
        if let Some(g) = self
            .tab
            .doc
            .graphics
            .iter()
            .find(|g| self.tab.selected.contains(&g.id))
        {
            self.tab.graphic_style = g.style.clone();
            self.tab.orbital_phase = g.phase;
            self.tab.phase_flipped = g.phase_flipped;
            self.tab.bracket_sides = g.sides;
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
        self.tab.graphic_width_input = self.tab.graphic_style.width_pt.to_string();
        let palette = reshiki::palette::Palette::of(&self.tab.doc);
        let hex = |c| reshiki::palette::hex(palette.rgb(c));
        self.tab.graphic_stroke_input = hex(self.tab.graphic_style.stroke);
        if let Some(c) = self.tab.graphic_style.fill {
            self.tab.graphic_fill_input = hex(c);
        }
    }
    pub(super) fn apply_graphic_style(&mut self, change: GraphicChange) {
        change.apply(&mut self.tab.graphic_style);
        let before = self.tab.doc.clone();
        for g in self.selected_graphics_mut().filter(|g| g.picture.is_none()) {
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
    fn selected_graphics_mut(&mut self) -> impl Iterator<Item = &mut Graphic> {
        let selected = &self.tab.selected;
        self.tab
            .doc
            .graphics
            .iter_mut()
            .filter(move |g| selected.contains(&g.id))
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
}
