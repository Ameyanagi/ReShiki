use super::*;
use moruno::typography::{Script, StyleChange, TextAlign, TextFormat, TextStyle};
use std::ops::Range;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ColorScope {
    #[default]
    All,
    Text,
    Bonds,
}
impl ColorScope {
    pub const ALL: [Self; 3] = [Self::All, Self::Text, Self::Bonds];
}
impl std::fmt::Display for ColorScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::All => "All selected",
            Self::Text => "Text",
            Self::Bonds => "Bonds",
        })
    }
}

impl App {
    pub(super) fn text_range(&self) -> Option<Range<usize>> {
        let cursor = self.caption_editor.cursor();
        let offset = |p: iced::widget::text_editor::Position| {
            self.caption
                .split_inclusive('\n')
                .take(p.line)
                .map(str::len)
                .sum::<usize>()
                + p.column
        };
        let a = offset(cursor.position).min(self.caption.len());
        let b = offset(cursor.selection?).min(self.caption.len());
        let selected = self.caption_editor.selection()?;
        if selected.is_empty() {
            return None;
        }
        let start = a.min(b);
        if self.caption.get(start..start + selected.len()) == Some(selected.as_str()) {
            return Some(start..start + selected.len());
        }
        // Word/line selections expose their anchor rather than normalized bounds.
        self.caption
            .match_indices(&selected)
            .min_by_key(|(i, _)| i.abs_diff(start))
            .map(|(i, s)| i..i + s.len())
    }
    pub(super) fn current_text_style(&self) -> &TextStyle {
        if let Some(range) = self.text_range() {
            self.caption_format.at(range.start)
        } else if self.inline_text.is_some() || self.caption_target.is_some() {
            let cursor = self.caption_editor.cursor().position;
            let offset = self
                .caption
                .split_inclusive('\n')
                .take(cursor.line)
                .map(str::len)
                .sum::<usize>()
                + cursor.column;
            self.caption_format
                .at(offset.min(self.caption.len().saturating_sub(1)))
        } else {
            &self.caption_format.style
        }
    }
    pub(super) fn current_selection_color(&self) -> Option<[u8; 3]> {
        if (self.inline_text.is_some() && self.color_scope != ColorScope::Bonds)
            || self.selected.is_empty()
            || (self.text_range().is_some()
                && self.selected.len() == 1
                && self.color_scope != ColorScope::Bonds)
        {
            return Some(self.current_text_style().color);
        }
        let mut colors = Vec::new();
        if self.color_scope != ColorScope::Bonds {
            colors.extend(
                self.doc
                    .atoms
                    .iter()
                    .filter(|a| self.selected.contains(&a.id))
                    .map(|a| a.text_style.as_ref().map_or([0; 3], |s| s.color)),
            );
            for a in self
                .doc
                .annotations
                .iter()
                .filter(|a| self.selected.contains(&a.id))
            {
                if a.text.is_empty() {
                    colors.push(a.format.style.color);
                } else {
                    colors.extend(a.text.char_indices().map(|(i, _)| a.format.at(i).color));
                }
            }
        }
        if self.color_scope != ColorScope::Text {
            colors.extend(
                self.doc
                    .bonds
                    .iter()
                    .filter(|b| self.selected.contains(&b.a) && self.selected.contains(&b.b))
                    .map(|b| b.color),
            );
        }
        if self.color_scope == ColorScope::All {
            colors.extend(
                self.doc
                    .arrows
                    .iter()
                    .filter(|a| self.selected.contains(&a.id))
                    .map(|a| a.appearance().color),
            );
            for g in self
                .doc
                .graphics
                .iter()
                .filter(|g| self.selected.contains(&g.id))
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
        self.text_color_input = self
            .current_selection_color()
            .map(|[r, g, b]| format!("#{r:02X}{g:02X}{b:02X}"))
            .unwrap_or_default();
    }
    pub(super) fn sync_typography(&mut self) {
        if self.inline_text.is_some() {
            self.sync_style_inputs();
            return;
        }
        let mut selected_atoms = self
            .doc
            .atoms
            .iter()
            .filter(|a| self.selected.contains(&a.id));
        self.labels.number = selected_atoms
            .next()
            .and_then(|a| a.display.number.as_ref())
            .map(|n| n.text.clone())
            .unwrap_or_default();
        if selected_atoms.next().is_some() {
            self.labels.number.clear();
        }

        if let Some(label) = self
            .doc
            .annotations
            .iter()
            .find(|a| self.selected.contains(&a.id))
        {
            self.caption = label.text.clone();
            self.caption_editor = iced::widget::text_editor::Content::with_text(&self.caption);
            self.caption_format = label.format.clone();
            self.caption_target = Some(label.id);
            if self.inspector_tab != InspectorTab::Templates {
                self.inspector_open = true;
                self.inspector_tab = InspectorTab::Properties;
            }
        } else {
            self.caption_target = None;
            self.caption_editor = iced::widget::text_editor::Content::with_text(&self.caption);
            if let Some(atom) = self
                .doc
                .atoms
                .iter()
                .find(|a| self.selected.contains(&a.id))
            {
                self.caption_format = TextFormat {
                    style: atom.text_style.clone().unwrap_or_default(),
                    ..Default::default()
                };
            }
        }
        self.sync_style_inputs();
    }
    pub(super) fn sync_style_inputs(&mut self) {
        let style = self.current_text_style();
        let size = style.size_pt.to_string();
        self.font_size_input = size;
        self.sync_color_input();
        self.text_width_input = self
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
        let cursor = self.caption_editor.cursor().position;
        let hint = selection.as_ref().map(|r| r.start).unwrap_or_else(|| {
            self.caption
                .split_inclusive('\n')
                .take(cursor.line)
                .map(str::len)
                .sum::<usize>()
                + cursor.column
        });
        self.caption_editor.perform(action);
        if changed {
            let text = self.caption_editor.text();
            let replaced = selection.unwrap_or_else(|| {
                if deletion && text.len() < self.caption.len() {
                    let cursor = self.caption_editor.cursor().position;
                    let after = text
                        .split_inclusive('\n')
                        .take(cursor.line)
                        .map(str::len)
                        .sum::<usize>()
                        + cursor.column;
                    let start = after.min(hint);
                    start..start + self.caption.len() - text.len()
                } else {
                    hint..hint
                }
            });
            self.caption_format.edited(&self.caption, &text, replaced);
            self.caption = text;
            if self.inline_text.is_some() {
                self.sync_style_inputs();
                return;
            }
            let before = self.doc.clone();
            if let Some(label) = self
                .doc
                .annotations
                .iter_mut()
                .find(|a| Some(a.id) == self.caption_target && self.selected.contains(&a.id))
            {
                label.text = self.caption.clone();
                label.format = self.caption_format.clone();
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
        let before = self.doc.clone();
        self.caption_format
            .apply(&self.caption, range.clone(), &change);
        if self.inline_text.is_some() {
            self.sync_style_inputs();
            return;
        }
        for label in &mut self.doc.annotations {
            if self.selected.contains(&label.id) {
                let target_range = if Some(label.id) == self.caption_target {
                    range.clone()
                } else {
                    None
                };
                label.format.apply(&label.text, target_range, &change);
            }
        }
        for atom in &mut self.doc.atoms {
            if self.selected.contains(&atom.id) {
                let style = atom.text_style.get_or_insert_with(TextStyle::default);
                change.apply(style);
                // Chemical scripts are derived from charge/isotope/H count.
                style.script = Script::Normal;
                style.formula = false;
            }
        }
        self.changed(before);
        self.sync_style_inputs();
    }
    pub(super) fn apply_selection_color(&mut self, color: [u8; 3]) {
        if self.inline_text.is_some() {
            if self.color_scope != ColorScope::Bonds {
                self.inline_checkpoint();
                self.caption_format.apply(
                    &self.caption,
                    self.text_range(),
                    &StyleChange::Color(color),
                );
                self.sync_style_inputs();
            }
            return;
        }
        let before = self.doc.clone();
        let range = self.text_range().filter(|_| {
            self.selected.len() == 1
                && self
                    .caption_target
                    .is_some_and(|id| self.selected.contains(&id))
        });
        let text_only = range.is_some() && self.color_scope != ColorScope::Bonds;
        let text = self.color_scope != ColorScope::Bonds;
        let bonds = self.color_scope != ColorScope::Text && !text_only;
        if text {
            let change = StyleChange::Color(color);
            self.caption_format
                .apply(&self.caption, range.clone(), &change);
            for label in &mut self.doc.annotations {
                if self.selected.contains(&label.id) {
                    label.format.apply(&label.text, range.clone(), &change);
                }
            }
            for atom in &mut self.doc.atoms {
                if self.selected.contains(&atom.id) && !text_only {
                    atom.text_style.get_or_insert_with(TextStyle::default).color = color;
                    atom.display.stereo.style.color = color;
                    if let Some(number) = &mut atom.display.number {
                        number.style.color = color;
                    }
                }
            }
            if !text_only {
                for bond in &mut self.doc.bonds {
                    if self.selected.contains(&bond.a) && self.selected.contains(&bond.b) {
                        bond.indicator.style.color = color;
                    }
                }
            }
        }
        if bonds {
            for bond in &mut self.doc.bonds {
                if self.selected.contains(&bond.a) && self.selected.contains(&bond.b) {
                    bond.color = color;
                }
            }
        }
        if self.color_scope == ColorScope::All && !text_only {
            for arrow in &mut self.doc.arrows {
                if self.selected.contains(&arrow.id) {
                    let mut style = arrow.appearance();
                    style.color = color;
                    arrow.style = Some(style);
                }
            }
            for graphic in &mut self.doc.graphics {
                if self.selected.contains(&graphic.id) {
                    graphic.style.stroke = color;
                    if graphic.style.fill.is_some() {
                        graphic.style.fill = Some(color);
                    }
                }
            }
        }
        let changed = before != self.doc;
        self.changed(before);
        self.sync_style_inputs();
        self.sync_graphics();
        self.sync_arrows();
        self.sync_bonds();
        self.text_color_input = format!("#{:02X}{:02X}{:02X}", color[0], color[1], color[2]);
        self.status = if text_only {
            "Text range recolored".into()
        } else if changed {
            format!(
                "Color applied to {}",
                self.color_scope.to_string().to_lowercase()
            )
        } else if self.selected.is_empty() && text {
            "Text color set · Select drawing objects to recolor them".into()
        } else {
            format!(
                "No color change · Apply to {}",
                self.color_scope.to_string().to_lowercase()
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
        update(&mut self.caption_format);
        if self.inline_text.is_some() {
            self.sync_style_inputs();
            return;
        }
        let before = self.doc.clone();
        for label in &mut self.doc.annotations {
            if self.selected.contains(&label.id) {
                update(&mut label.format);
            }
        }
        self.changed(before);
        self.sync_style_inputs();
    }
}
