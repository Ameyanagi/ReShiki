use super::*;
use reshiki::abbreviations::LabelAlignment;
use reshiki::palette::{Color as Paint, Palette};
use reshiki::typography::{Script, StyleChange, TextAlign, TextFormat, TextStyle};
use std::ops::Range;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ColorScope {
    #[default]
    All,
    Text,
    Bonds,
    Rings,
}
impl ColorScope {
    pub const ALL: [Self; 4] = [Self::All, Self::Text, Self::Bonds, Self::Rings];
}
impl std::fmt::Display for ColorScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::All => "All selected",
            Self::Text => "Text",
            Self::Bonds => "Bonds",
            Self::Rings => "Ring interiors",
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
        let explicit = atom.text_style.as_ref().map_or(Paint::Ink, |s| s.color);
        if atom.display.color_override || explicit != Paint::Ink {
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
                    .filter(|a| self.tab.selected.contains(&a.id))
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
                        self.tab.selected.contains(&b.a) && self.tab.selected.contains(&b.b)
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
            .filter(|a| self.tab.selected.contains(&a.id));
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
                .find(|a| self.tab.selected.contains(&a.id))
            {
                self.tab.caption_format = TextFormat {
                    style: atom
                        .text_style
                        .clone()
                        .unwrap_or_else(|| self.tab.doc.drawing_style.text_style()),
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
        for atom in &mut self.tab.doc.atoms {
            if self.tab.selected.contains(&atom.id) {
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
    /// Keep a custom color that was just used in the drawing for the picker.
    pub(super) fn remember_custom(&mut self, color: Option<Paint>, before: &Document) {
        if let Some(Paint::Custom(rgb)) = color
            && self.tab.doc != *before
        {
            self.tab.doc.remember_color(rgb);
        }
    }
    pub(super) fn apply_selection_color(&mut self, color: Paint) {
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
            for atom in &mut self.tab.doc.atoms {
                if self.tab.selected.contains(&atom.id) && !text_only {
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
                    if self.tab.selected.contains(&bond.a) && self.tab.selected.contains(&bond.b) {
                        bond.indicator.style.color = color;
                    }
                }
            }
        }
        if bonds {
            for bond in &mut self.tab.doc.bonds {
                if self.tab.selected.contains(&bond.a) && self.tab.selected.contains(&bond.b) {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(app: &mut App, label: &str, x: f32) -> Result<u64, String> {
        let id = app.tab.doc.add_atom("C", Point::new(x, 0.));
        app.tab.doc =
            reshiki::atom_text::apply(&app.tab.doc, id, label, reshiki::atom_text::Mode::Group)?;
        Ok(id)
    }

    #[test]
    fn toolbar_aligns_selected_groups_and_captions_in_one_undo_without_changing_chemistry()
    -> Result<(), String> {
        let (mut app, _) = App::new();
        app.tab.doc = Document::default();
        let boc = group(&mut app, "Boc", 0.)?;
        let cp = group(&mut app, "Cp*", 150.)?;
        let other = group(&mut app, "OMe", 300.)?;
        let caption = app.tab.doc.next_id();
        app.tab.doc.annotations.push(Annotation {
            id: caption,
            position: Point::new(0., 200.),
            text: "Caption".into(),
            format: Default::default(),
        });
        app.tab.selected = vec![boc];
        assert_eq!(
            app.selected_group_alignment(),
            Some(Some(LabelAlignment::Auto))
        );
        assert_eq!(
            app.toolbar_alignment(),
            None,
            "Automatic must not highlight Left"
        );
        let _ = app.update(Message::TextAlign(TextAlign::Right));
        assert_eq!(app.toolbar_alignment(), Some(TextAlign::Right));
        app.tab.selected = vec![boc, cp, caption];
        assert_eq!(app.selected_group_alignment(), Some(None));
        assert_eq!(app.toolbar_alignment(), None);
        let before = app.tab.doc.clone();
        let _ = app.update(Message::TextAlign(TextAlign::Center));
        assert_eq!(app.toolbar_alignment(), Some(TextAlign::Center));
        for id in [boc, cp] {
            assert_eq!(
                app.tab.doc.abbreviation(id).ok_or("group")?.alignment,
                LabelAlignment::Center
            );
        }
        assert_eq!(
            app.tab
                .doc
                .abbreviation(other)
                .ok_or("other group")?
                .alignment,
            LabelAlignment::Auto
        );
        assert_eq!(app.tab.doc.atoms, before.atoms);
        assert_eq!(app.tab.doc.bonds, before.bonds);
        let after = app.tab.doc.clone();
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, after);

        let _ = app.update(Message::TextAlign(TextAlign::Justified));
        assert_eq!(
            app.tab.doc.abbreviation(boc).ok_or("group")?.alignment,
            LabelAlignment::Center
        );
        assert_eq!(
            app.tab
                .doc
                .annotations
                .first()
                .ok_or("caption")?
                .format
                .alignment,
            TextAlign::Justified
        );
        assert_eq!(
            app.toolbar_alignment(),
            None,
            "Mixed caption/group alignment"
        );
        let _ = app.update(Message::GroupLabelAlign(LabelAlignment::Auto));
        assert_eq!(
            app.selected_group_alignment(),
            Some(Some(LabelAlignment::Auto))
        );
        assert_eq!(
            app.tab
                .doc
                .annotations
                .first()
                .ok_or("caption")?
                .format
                .alignment,
            TextAlign::Justified
        );
        let _ = app.update(Message::GroupLabelAlign(LabelAlignment::Above));
        assert_eq!(
            app.selected_group_alignment(),
            Some(Some(LabelAlignment::Above))
        );
        assert_eq!(app.tab.doc.atoms, before.atoms);
        assert_eq!(app.tab.doc.bonds, before.bonds);
        Ok(())
    }

    #[test]
    fn group_only_alignment_does_not_change_paragraph_defaults_or_unselected_groups()
    -> Result<(), String> {
        let (mut app, _) = App::new();
        app.tab.doc = Document::default();
        let id = group(&mut app, "Boc", 0.)?;
        app.tab.selected = app.tab.doc.abbreviation(id).ok_or("group")?.members.clone();
        let format = app.tab.caption_format.clone();
        for alignment in [TextAlign::Left, TextAlign::Center, TextAlign::Right] {
            let _ = app.update(Message::TextAlign(alignment));
            assert_eq!(app.toolbar_alignment(), Some(alignment));
            assert_eq!(app.tab.caption_format, format);
        }
        let before = app.tab.doc.clone();
        let _ = app.update(Message::TextAlign(TextAlign::Justified));
        assert_eq!(app.tab.doc, before);
        let second = group(&mut app, "Cp", 200.)?;
        assert_eq!(
            app.tab
                .doc
                .abbreviation(second)
                .ok_or("new group")?
                .alignment,
            LabelAlignment::Auto
        );
        app.tab.selected.clear();
        let _ = app.update(Message::TextAlign(TextAlign::Justified));
        assert_eq!(app.tab.caption_format.alignment, TextAlign::Justified);
        assert_eq!(
            app.tab.doc.abbreviation(id).ok_or("group")?.alignment,
            LabelAlignment::Right
        );
        Ok(())
    }

    #[test]
    fn inline_caption_alignment_never_moves_group_labels_and_can_be_cancelled() -> Result<(), String>
    {
        let (mut app, _) = App::new();
        app.tab.doc = Document::default();
        let id = group(&mut app, "Boc", 0.)?;
        let caption = app.tab.doc.next_id();
        app.tab.doc.annotations.push(Annotation {
            id: caption,
            position: Point::new(0., 100.),
            text: "Caption".into(),
            format: Default::default(),
        });
        let before = app.tab.doc.clone();
        let _ = app.update(Message::InlineText(
            super::super::inline_text::Action::Begin(Some(caption), Point::default()),
        ));
        app.tab.selected = vec![id, caption];
        assert_eq!(app.selected_group_alignment(), None);
        let _ = app.update(Message::GroupLabelAlign(LabelAlignment::Above));
        let _ = app.update(Message::TextAlign(TextAlign::Right));
        assert_eq!(app.toolbar_alignment(), Some(TextAlign::Right));
        assert_eq!(
            app.tab.doc, before,
            "Inline formatting only changes the draft"
        );
        let _ = app.update(Message::InlineText(
            super::super::inline_text::Action::Finish(false),
        ));
        assert_eq!(app.tab.doc, before);
        Ok(())
    }
}
