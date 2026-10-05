use super::workspace::{command, muted_text, section};
use super::*;
use iced::widget::{Row, column, row, text, text_editor};
use iced::{Alignment, Length};
use reshiki::abbreviations::LabelAlignment;
use reshiki::palette::{Color as Paint, Palette};
use reshiki::typography::{Script, StyleChange, TextAlign, TextFormat, TextStyle};
use std::ops::Range;

#[cfg(test)]
mod caption_controls_tests;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ColorScope {
    #[default]
    All,
    Text,
    Bonds,
    Rings,
    Highlights,
}
impl ColorScope {
    pub const ALL: [Self; 5] = [
        Self::All,
        Self::Text,
        Self::Bonds,
        Self::Rings,
        Self::Highlights,
    ];
}
impl std::fmt::Display for ColorScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::All => "All selected",
            Self::Text => "Text",
            Self::Bonds => "Bonds",
            Self::Rings => "Ring interiors",
            Self::Highlights => "Highlights",
        })
    }
}

impl App {
    fn selected_label_groups(&self) -> impl Iterator<Item = &reshiki::abbreviations::Abbreviation> {
        self.tab.doc.abbreviations.iter().filter(|group| {
            self.tab.inline_text.is_none()
                && group
                    .members
                    .iter()
                    .any(|id| self.tab.selected.contains(id))
        })
    }

    /// Outer None: no group labels. Inner None: selected groups have mixed placement.
    pub(super) fn selected_group_alignment(&self) -> Option<Option<LabelAlignment>> {
        let mut groups = self.selected_label_groups();
        let first = groups.next()?.alignment;
        Some(groups.all(|g| g.alignment == first).then_some(first))
    }

    pub(super) fn toolbar_alignment(&self) -> Option<TextAlign> {
        if self.tab.inline_text.is_some() {
            return Some(self.tab.caption_format.alignment);
        }
        let mut values = self
            .selected_label_groups()
            .map(|g| match g.alignment {
                LabelAlignment::Left => Some(TextAlign::Left),
                LabelAlignment::Center => Some(TextAlign::Center),
                LabelAlignment::Right => Some(TextAlign::Right),
                LabelAlignment::Auto | LabelAlignment::Above => None,
            })
            .chain(
                self.tab
                    .doc
                    .annotations
                    .iter()
                    .filter(|a| self.tab.selected.contains(&a.id))
                    .map(|a| Some(a.format.alignment)),
            );
        let Some(first) = values.next() else {
            return Some(self.tab.caption_format.alignment);
        };
        first.filter(|_| values.all(|value| value == first))
    }

    pub(super) fn apply_group_alignment(&mut self, alignment: LabelAlignment) {
        if self.tab.inline_text.is_some() {
            return;
        }
        let before = self.tab.doc.clone();
        for group in &mut self.tab.doc.abbreviations {
            if group
                .members
                .iter()
                .any(|id| self.tab.selected.contains(id))
            {
                group.alignment = alignment;
            }
        }
        self.changed(before);
    }

    pub(super) fn text_range(&self) -> Option<Range<usize>> {
        let cursor = self.tab.caption_editor.cursor();
        let offset = |p: iced::widget::text_editor::Position| {
            self.tab
                .caption
                .split_inclusive('\n')
                .take(p.line)
                .map(str::len)
                .sum::<usize>()
                + p.column
        };
        let a = offset(cursor.position).min(self.tab.caption.len());
        let b = offset(cursor.selection?).min(self.tab.caption.len());
        let selected = self.tab.caption_editor.selection()?;
        if selected.is_empty() {
            return None;
        }
        let start = a.min(b);
        if self.tab.caption.get(start..start + selected.len()) == Some(selected.as_str()) {
            return Some(start..start + selected.len());
        }
        // Word/line selections expose their anchor rather than normalized bounds.
        self.tab
            .caption
            .match_indices(&selected)
            .min_by_key(|(i, _)| i.abs_diff(start))
            .map(|(i, s)| i..i + s.len())
    }
    pub(super) fn current_text_style(&self) -> &TextStyle {
        if let Some(range) = self.text_range() {
            self.tab.caption_format.at(range.start)
        } else if self.tab.inline_text.is_some() || self.tab.caption_target.is_some() {
            let cursor = self.tab.caption_editor.cursor().position;
            let offset = self
                .tab
                .caption
                .split_inclusive('\n')
                .take(cursor.line)
                .map(str::len)
                .sum::<usize>()
                + cursor.column;
            self.tab
                .caption_format
                .at(offset.min(self.tab.caption.len().saturating_sub(1)))
        } else {
            &self.tab.caption_format.style
        }
    }
    /// An atom's color as a swatch: its explicit color, or its automatic theme
    /// ink as Ink or an exact color.
    fn atom_paint(&self, atom: &reshiki::document::Atom) -> Paint {
        let (explicit, overridden) = self.tab.doc.abbreviation(atom.id).map_or_else(
            || {
                (
                    atom.text_style.as_ref().map_or(Paint::Ink, |s| s.color),
                    atom.display.color_override,
                )
            },
            |group| {
                (
                    group.text_style(&self.tab.doc).color,
                    group.color_override(&self.tab.doc),
                )
            },
        );
        if overridden || explicit != Paint::Ink {
            return explicit;
        }
        let rgb = self
            .tab
            .doc
            .canvas_theme
            .color(reshiki::canvas_theme::atom_color(&self.tab.doc, atom));
        if rgb == self.tab.doc.canvas_theme.color([0; 3]) {
            Paint::Ink
        } else {
            Paint::Custom(rgb)
        }
    }
    /// The interior color of each selected ring; None where it has no fill.
    pub(super) fn selected_ring_fills(&self) -> Vec<Option<Paint>> {
        reshiki::ring_fills::selected_cycles(&self.tab.doc, &self.tab.selected)
            .iter()
            .map(|atoms| {
                self.tab
                    .doc
                    .ring_fills
                    .iter()
                    .find(|fill| {
                        fill.atoms.len() == atoms.len()
                            && fill.atoms.iter().all(|id| atoms.contains(id))
                    })
                    .map(|f| f.color)
            })
            .collect()
    }
    pub(super) fn current_selection_color(&self) -> Option<Paint> {
        if self.tab.color_scope == ColorScope::Highlights {
            let colors: Vec<_> =
                reshiki::highlights::selected_colors(&self.tab.doc, &self.tab.selected)
                    .into_iter()
                    .collect::<Option<_>>()?;
            return colors
                .first()
                .copied()
                .filter(|first| colors.iter().all(|c| c == first));
        }
        if self.tab.color_scope == ColorScope::Rings {
            let colors: Vec<_> = self
                .selected_ring_fills()
                .into_iter()
                .collect::<Option<_>>()?;
            return colors
                .first()
                .copied()
                .filter(|first| colors.iter().all(|c| c == first));
        }
        if (self.tab.inline_text.is_some() && self.tab.color_scope != ColorScope::Bonds)
            || self.tab.selected.is_empty()
            || (self.text_range().is_some()
                && self.tab.selected.len() == 1
                && self.tab.color_scope != ColorScope::Bonds)
        {
            return Some(self.current_text_style().color);
        }
        let mut colors = Vec::new();
        if self.tab.color_scope != ColorScope::Bonds {
            colors.extend(
                self.tab
                    .doc
                    .atoms
                    .iter()
                    .filter(|a| {
                        self.tab.selected.contains(&a.id) && self.tab.doc.atom_visible(a.id)
                    })
                    .map(|a| self.atom_paint(a)),
            );
            for a in self
                .tab
                .doc
                .annotations
                .iter()
                .filter(|a| self.tab.selected.contains(&a.id))
            {
                if a.text.is_empty() {
                    colors.push(a.format.style.color);
                } else {
                    colors.extend(a.text.char_indices().map(|(i, _)| a.format.at(i).color));
                }
            }
        }
        if self.tab.color_scope != ColorScope::Text {
            colors.extend(
                self.tab
                    .doc
                    .bonds
                    .iter()
                    .filter(|b| {
                        self.tab.selected.contains(&b.a)
                            && self.tab.selected.contains(&b.b)
                            && self.tab.doc.bond_visible(b.a, b.b)
                    })
                    .map(|b| b.color),
            );
        }
        if self.tab.color_scope == ColorScope::All {
            colors.extend(
                self.tab
                    .doc
                    .arrows
                    .iter()
                    .filter(|a| self.tab.selected.contains(&a.id))
                    .map(|a| a.appearance().color),
            );
            for g in self
                .tab
                .doc
                .graphics
                .iter()
                .filter(|g| self.tab.selected.contains(&g.id) && g.picture.is_none())
            {
                colors.push(g.style.stroke);
                colors.extend(g.style.fill);
            }
        }
        colors
            .first()
            .copied()
            .filter(|first| colors.iter().all(|c| c == first))
    }
    pub(super) fn sync_color_input(&mut self) {
        let palette = Palette::of(&self.tab.doc);
        self.tab.text_color_input = self
            .current_selection_color()
            .map(|color| reshiki::palette::hex(palette.rgb(color)))
            .unwrap_or_default();
        self.flag_color_input(false);
    }
    pub(super) fn sync_typography(&mut self) {
        if self.tab.inline_text.is_some() {
            self.sync_style_inputs();
            return;
        }
        let mut selected_atoms = self
            .tab
            .doc
            .atoms
            .iter()
            .filter(|a| self.tab.selected.contains(&a.id) && self.tab.doc.atom_visible(a.id));
        self.tab.labels.number = selected_atoms
            .next()
            .and_then(|a| a.display.number.as_ref())
            .map(|n| n.text.clone())
            .unwrap_or_default();
        if selected_atoms.next().is_some() {
            self.tab.labels.number.clear();
        }

        if let Some(label) = self
            .tab
            .doc
            .annotations
            .iter()
            .find(|a| self.tab.selected.contains(&a.id))
        {
            self.tab.caption = label.text.clone();
            self.tab.caption_editor =
                iced::widget::text_editor::Content::with_text(&self.tab.caption);
            self.tab.caption_format = label.format.clone();
            self.tab.caption_target = Some(label.id);
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
        } else {
            self.tab.caption_target = None;
            self.tab.caption_editor =
                iced::widget::text_editor::Content::with_text(&self.tab.caption);
            if let Some(atom) = self
                .tab
                .doc
                .atoms
                .iter()
                .find(|a| self.tab.selected.contains(&a.id) && self.tab.doc.atom_visible(a.id))
            {
                self.tab.caption_format = TextFormat {
                    style: self.tab.doc.abbreviation(atom.id).map_or_else(
                        || {
                            atom.text_style
                                .clone()
                                .unwrap_or_else(|| self.tab.doc.drawing_style.text_style())
                        },
                        |group| group.text_style(&self.tab.doc),
                    ),
                    ..Default::default()
                };
            }
        }
        self.sync_style_inputs();
    }
    pub(super) fn sync_style_inputs(&mut self) {
        let style = self.current_text_style();
        let size = style.size_pt.to_string();
        self.tab.font_size_input = size;
        self.sync_color_input();
        self.tab.text_width_input = self
            .tab
            .caption_format
            .width_pt
            .map(|w| w.to_string())
            .unwrap_or_default();
    }
    pub(super) fn caption_action(&mut self, action: iced::widget::text_editor::Action) {
        let changed = action.is_edit();
        if changed {
            self.inline_checkpoint();
        }
        let selection = self.text_range();
        let deletion = matches!(
            action,
            iced::widget::text_editor::Action::Edit(
                iced::widget::text_editor::Edit::Backspace
                    | iced::widget::text_editor::Edit::Delete
            )
        );
        let cursor = self.tab.caption_editor.cursor().position;
        let hint = selection.as_ref().map(|r| r.start).unwrap_or_else(|| {
            self.tab
                .caption
                .split_inclusive('\n')
                .take(cursor.line)
                .map(str::len)
                .sum::<usize>()
                + cursor.column
        });
        self.tab.caption_editor.perform(action);
        if changed {
            let text = self.tab.caption_editor.text();
            let replaced = selection.unwrap_or_else(|| {
                if deletion && text.len() < self.tab.caption.len() {
                    let cursor = self.tab.caption_editor.cursor().position;
                    let after = text
                        .split_inclusive('\n')
                        .take(cursor.line)
                        .map(str::len)
                        .sum::<usize>()
                        + cursor.column;
                    let start = after.min(hint);
                    start..start + self.tab.caption.len() - text.len()
                } else {
                    hint..hint
                }
            });
            self.tab
                .caption_format
                .edited(&self.tab.caption, &text, replaced);
            self.tab.caption = text;
            self.auto_format_caption();
            if self.tab.inline_text.is_some() {
                self.sync_style_inputs();
                return;
            }
            let before = self.tab.doc.clone();
            if let Some(label) = self.tab.doc.annotations.iter_mut().find(|a| {
                Some(a.id) == self.tab.caption_target && self.tab.selected.contains(&a.id)
            }) {
                label.text = self.tab.caption.clone();
                label.format = self.tab.caption_format.clone();
            }
            self.changed(before);
        }
        self.sync_style_inputs();
    }
    fn apply_group_text_style(&mut self, change: &StyleChange) -> std::collections::HashSet<u64> {
        let mut styles: std::collections::HashMap<_, _> = self
            .tab
            .doc
            .abbreviations
            .iter()
            .filter(|group| {
                group
                    .members
                    .iter()
                    .any(|id| self.tab.selected.contains(id))
            })
            .map(|group| {
                (
                    group.anchor,
                    (
                        group.text_style(&self.tab.doc),
                        group.color_override(&self.tab.doc),
                    ),
                )
            })
            .collect();
        let mut members = std::collections::HashSet::new();
        for group in &mut self.tab.doc.abbreviations {
            if let Some((mut style, overridden)) = styles.remove(&group.anchor) {
                change.apply(&mut style);
                style.script = Script::Normal;
                style.formula = false;
                group.label_style = Some(style);
                group.label_color_override = overridden || matches!(change, StyleChange::Color(_));
                members.extend(group.members.iter().copied());
            }
        }
        members
    }

    pub(super) fn apply_text_style(&mut self, change: StyleChange) {
        if let StyleChange::Color(color) = change {
            self.apply_selection_color(color);
            return;
        }
        let range = self.text_range();
        self.inline_checkpoint();
        if matches!(change, StyleChange::Formula(_) | StyleChange::Script(_)) {
            self.manual_caption_format();
        }
        let before = self.tab.doc.clone();
        self.tab
            .caption_format
            .apply(&self.tab.caption, range.clone(), &change);
        if self.tab.inline_text.is_some() {
            self.sync_style_inputs();
            return;
        }
        for label in &mut self.tab.doc.annotations {
            if self.tab.selected.contains(&label.id) {
                let target_range = if Some(label.id) == self.tab.caption_target {
                    range.clone()
                } else {
                    None
                };
                label.format.apply(&label.text, target_range, &change);
            }
        }
        let group_members = self.apply_group_text_style(&change);
        for atom in &mut self.tab.doc.atoms {
            if self.tab.selected.contains(&atom.id) && !group_members.contains(&atom.id) {
                let style = atom
                    .text_style
                    .get_or_insert_with(|| self.tab.doc.drawing_style.text_style());
                change.apply(style);
                // Chemical scripts are derived from charge/isotope/H count.
                style.script = Script::Normal;
                style.formula = false;
            }
        }
        self.changed(before);
        self.sync_style_inputs();
    }
    pub(super) fn apply_ring_color(&mut self, color: Option<Paint>) {
        let before = self.tab.doc.clone();
        let count = reshiki::ring_fills::apply(&mut self.tab.doc, &self.tab.selected, color);
        self.remember_custom(color, &before);
        self.changed(before);
        self.sync_color_input();
        self.status = if count == 0 {
            "Select every atom of a ring to change its interior color".into()
        } else {
            format!(
                "{} {count} ring interior(s)",
                if color.is_some() {
                    "Colored"
                } else {
                    "Cleared"
                }
            )
        };
    }
    pub(super) fn apply_highlight_color(&mut self, color: Option<Paint>) {
        let before = self.tab.doc.clone();
        let count = reshiki::highlights::apply(&mut self.tab.doc, &self.tab.selected, color);
        self.remember_custom(color, &before);
        self.changed(before);
        self.sync_color_input();
        self.status = if count == 0 {
            "Select atoms or bonds to change their highlights".into()
        } else if color.is_some() {
            "Highlights applied".into()
        } else {
            "Highlights cleared".into()
        };
    }
    /// Keep a custom color that was just used in the drawing for the picker.
    pub(super) fn remember_custom(&mut self, color: Option<Paint>, before: &Document) {
        if let Some(Paint::Custom(rgb)) = color
            && self.tab.doc != *before
        {
            self.tab.doc.remember_color(rgb);
        }
    }
    pub(super) fn apply_selection_color(&mut self, color: Paint) {
        if self.tab.color_scope == ColorScope::Highlights {
            self.apply_highlight_color(Some(color));
            return;
        }
        if self.tab.color_scope == ColorScope::Rings {
            self.apply_ring_color(Some(color));
            return;
        }
        if self.tab.inline_text.is_some() {
            if self.tab.color_scope != ColorScope::Bonds {
                self.inline_checkpoint();
                self.tab.caption_format.apply(
                    &self.tab.caption,
                    self.text_range(),
                    &StyleChange::Color(color),
                );
                self.sync_style_inputs();
            }
            return;
        }
        let before = self.tab.doc.clone();
        let range = self.text_range().filter(|_| {
            self.tab.selected.len() == 1
                && self
                    .tab
                    .caption_target
                    .is_some_and(|id| self.tab.selected.contains(&id))
        });
        let text_only = range.is_some() && self.tab.color_scope != ColorScope::Bonds;
        let text = self.tab.color_scope != ColorScope::Bonds;
        let bonds = self.tab.color_scope != ColorScope::Text && !text_only;
        let visible_bonds: std::collections::HashSet<_> = self
            .tab
            .doc
            .bonds
            .iter()
            .filter(|bond| self.tab.doc.bond_visible(bond.a, bond.b))
            .map(|bond| (bond.a, bond.b))
            .collect();
        if text {
            let change = StyleChange::Color(color);
            self.tab
                .caption_format
                .apply(&self.tab.caption, range.clone(), &change);
            for label in &mut self.tab.doc.annotations {
                if self.tab.selected.contains(&label.id) {
                    label.format.apply(&label.text, range.clone(), &change);
                }
            }
            let group_members = if text_only {
                std::collections::HashSet::new()
            } else {
                self.apply_group_text_style(&change)
            };
            for atom in &mut self.tab.doc.atoms {
                if self.tab.selected.contains(&atom.id)
                    && !text_only
                    && !group_members.contains(&atom.id)
                {
                    atom.display.color_override = true;
                    atom.display.hydrogen_color = None;
                    atom.text_style
                        .get_or_insert_with(|| self.tab.doc.drawing_style.text_style())
                        .color = color;
                    atom.display.stereo.style.color = color;
                    if let Some(number) = &mut atom.display.number {
                        number.style.color = color;
                    }
                }
            }
            if !text_only {
                for bond in &mut self.tab.doc.bonds {
                    if self.tab.selected.contains(&bond.a)
                        && self.tab.selected.contains(&bond.b)
                        && visible_bonds.contains(&(bond.a, bond.b))
                    {
                        bond.indicator.style.color = color;
                    }
                }
            }
        }
        if bonds {
            for bond in &mut self.tab.doc.bonds {
                if self.tab.selected.contains(&bond.a)
                    && self.tab.selected.contains(&bond.b)
                    && visible_bonds.contains(&(bond.a, bond.b))
                {
                    bond.color = color;
                }
            }
        }
        if self.tab.color_scope == ColorScope::All && !text_only {
            for arrow in &mut self.tab.doc.arrows {
                if self.tab.selected.contains(&arrow.id) {
                    let mut style = arrow.appearance();
                    style.color = color;
                    arrow.style = Some(style);
                }
            }
            for graphic in &mut self.tab.doc.graphics {
                if self.tab.selected.contains(&graphic.id) && graphic.picture.is_none() {
                    graphic.style.stroke = color;
                    if graphic.style.fill.is_some() {
                        graphic.style.fill = Some(color);
                    }
                }
            }
        }
        let changed = before != self.tab.doc;
        self.remember_custom(Some(color), &before);
        self.changed(before);
        self.sync_style_inputs();
        self.sync_graphics();
        self.sync_arrows();
        self.sync_bonds();
        self.tab.text_color_input = reshiki::palette::hex(Palette::of(&self.tab.doc).rgb(color));
        self.status = if text_only {
            "Text range recolored".into()
        } else if changed {
            format!(
                "Color applied to {}",
                self.tab.color_scope.to_string().to_lowercase()
            )
        } else if self.tab.selected.is_empty() && text {
            "Text color set · Select drawing objects to recolor them".into()
        } else {
            format!(
                "No color change · Apply to {}",
                self.tab.color_scope.to_string().to_lowercase()
            )
        };
    }
    pub(super) fn text_panel(&self) -> Element<'_, Message> {
        if self.tab.inline_text.is_some() {
            return column![
                section("EDITING ON CANVAS"),
                text("Select text in the canvas editor, then use the Style toolbar to format it.")
                    .size(12)
                    .style(muted_text),
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
                self.caption_spacing_control(),
                self.caption_width_control()
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
                .style(muted_text),
                text("Use the Style toolbar for fonts, colors and chemical formulas.")
                    .size(11)
                    .style(muted_text),
            ]
            .spacing(9)
            .into();
        }
        let selected = self
            .tab
            .caption_target
            .is_some_and(|id| self.tab.selected.contains(&id));
        column![
            section(if selected { "EDIT TEXT" } else { "NEW TEXT" }),
            text(if selected {
                "Changes appear on the drawing as you type."
            } else {
                "Write a label, choose its style, then click to place."
            })
            .size(11)
            .style(muted_text),
            text_editor(&self.tab.caption_editor)
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
                .style(muted_text),
            self.caption_spacing_control().align_y(Alignment::Center),
            self.caption_width_control().align_y(Alignment::Center),
        ]
        .spacing(9)
        .into()
    }

    fn caption_spacing_control(&self) -> Row<'_, Message> {
        row![
            text("Line spacing").size(11).width(Length::Fill),
            crate::appearance::pick_list(
                [1.0_f32, 1.2, 1.5, 2.0],
                Some(self.tab.caption_format.line_spacing),
                Message::TextSpacing
            )
            .text_size(12)
            .padding(5)
        ]
    }

    fn caption_width_control(&self) -> Row<'_, Message> {
        row![
            text("Wrap width (pt)").size(11).width(Length::Fill),
            crate::appearance::text_input("Auto", &self.tab.text_width_input)
                .on_input(Message::TextWidth)
                .on_submit(Message::ApplyTextWidth)
                .size(12)
                .width(72)
                .padding(6)
        ]
    }

    pub(super) fn apply_paragraph(
        &mut self,
        alignment: Option<TextAlign>,
        spacing: Option<f32>,
        width: Option<Option<f32>>,
    ) {
        self.inline_checkpoint();
        let update = |format: &mut TextFormat| {
            if let Some(value) = alignment {
                format.alignment = value;
            }
            if let Some(value) = spacing {
                format.line_spacing = value;
            }
            if let Some(value) = width {
                format.width_pt = value;
            }
        };
        if self.tab.inline_text.is_some()
            || self.selected_group_alignment().is_none()
            || self
                .tab
                .doc
                .annotations
                .iter()
                .any(|a| self.tab.selected.contains(&a.id))
        {
            update(&mut self.tab.caption_format);
        }
        if self.tab.inline_text.is_some() {
            self.sync_style_inputs();
            return;
        }
        let before = self.tab.doc.clone();
        if let Some(group_alignment) = alignment.and_then(|value| match value {
            TextAlign::Left => Some(LabelAlignment::Left),
            TextAlign::Center => Some(LabelAlignment::Center),
            TextAlign::Right => Some(LabelAlignment::Right),
            TextAlign::Justified => None,
        }) {
            for group in &mut self.tab.doc.abbreviations {
                if group
                    .members
                    .iter()
                    .any(|id| self.tab.selected.contains(id))
                {
                    group.alignment = group_alignment;
                }
            }
        }
        for label in &mut self.tab.doc.annotations {
            if self.tab.selected.contains(&label.id) {
                update(&mut label.format);
            }
        }
        self.changed(before);
        self.sync_style_inputs();
    }
    pub(super) fn apply_font_size_input(&mut self) {
        match self.tab.font_size_input.parse::<f32>() {
            Ok(size) if size.is_finite() && (4.0..=144.0).contains(&size) => {
                self.apply_text_style(reshiki::typography::StyleChange::Size(size))
            }
            _ => {
                self.error = true;
                self.status = "Enter a font size from 4 to 144 pt".into();
            }
        }
    }
    pub(super) fn set_color_scope(&mut self, scope: ColorScope) {
        self.tab.color_scope = scope;
        self.sync_color_input();
        if scope == typography::ColorScope::Rings {
            self.status = "Ring interiors · Select a ring, then choose a Tint color".into();
        } else if scope == typography::ColorScope::Highlights {
            self.status = "Highlights · Select atoms or bonds, then choose a Tint color".into();
        }
    }
    pub(super) fn set_text_color_input(&mut self, value: String) {
        self.tab.text_color_input = value;
        self.flag_color_input(false);
    }
    pub(super) fn apply_text_color_input(&mut self) {
        if let Some(rgb) = reshiki::palette::parse_color(&self.tab.text_color_input) {
            // Typed colors are exact on both canvases.
            let color = reshiki::palette::Color::Custom(rgb);
            if self.tab.color_scope == typography::ColorScope::Rings {
                self.apply_ring_color(Some(color));
            } else {
                self.apply_text_style(reshiki::typography::StyleChange::Color(color));
            }
        } else {
            self.error = true;
            self.status = super::color_popover::HINT.into();
            self.flag_color_input(true);
        }
    }
    pub(super) fn apply_text_width_input(&mut self) {
        let width = self.tab.text_width_input.trim();
        if width.is_empty() {
            self.apply_paragraph(None, None, Some(None));
        } else if let Ok(width) = width.parse::<f32>()
            && width.is_finite()
            && (10.0..=2000.0).contains(&width)
        {
            self.apply_paragraph(None, None, Some(Some(width)));
        } else {
            self.error = true;
            self.status = "Text width must be 10–2000 pt, or blank for automatic width".into();
        }
    }
}

#[cfg(test)]
mod tests;
