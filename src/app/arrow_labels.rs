//! Contextual controls for attached reaction-condition clearances.
use super::{App, Message};
use iced::widget::{column, row, text, tooltip};
use iced::{Alignment, Element, Length};

#[derive(Debug, Clone)]
pub enum Action {
    Input(String),
    Apply,
    Auto,
    Detach,
}

impl App {
    fn selected_arrow_labels(&self) -> Vec<u64> {
        self.tab
            .doc
            .arrow_labels
            .iter()
            .filter(|label| self.tab.selected.contains(&label.annotation))
            .map(|label| label.annotation)
            .collect()
    }

    pub(super) fn sync_arrow_label_gap_input(&mut self) {
        self.tab.arrow_label_gap_edited = false;
        let values: Vec<_> = self
            .tab
            .doc
            .arrow_labels
            .iter()
            .filter(|label| self.tab.selected.contains(&label.annotation))
            .map(|label| label.gap)
            .collect();
        self.tab.arrow_label_gap_input = match values.first().copied().flatten() {
            Some(value) if values.iter().all(|v| *v == Some(value)) => {
                let value = format!("{:.3}", f64::from(value) * self.guides.unit.per_world());
                value.trim_end_matches('0').trim_end_matches('.').to_owned()
            }
            _ => String::new(),
        };
    }

    pub(super) fn arrow_label_gap_action(&mut self, action: Action) {
        if let Action::Input(value) = action {
            self.tab.arrow_label_gap_input = value;
            self.tab.arrow_label_gap_edited = true;
            return;
        }
        if matches!(action, Action::Apply) && !self.tab.arrow_label_gap_edited {
            return;
        }
        let ids = self.selected_arrow_labels();
        if ids.is_empty() {
            return;
        }
        let value = match action {
            Action::Apply if !self.tab.arrow_label_gap_input.trim().is_empty() => {
                let value = self
                    .tab
                    .arrow_label_gap_input
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .map(|v| (v / self.guides.unit.per_world()) as f32)
                    .filter(|v| v.is_finite() && *v >= 0.);
                let Some(value) = value else {
                    self.error = true;
                    self.status = format!(
                        "Enter a finite, nonnegative arrow gap in {}",
                        self.guides.unit
                    );
                    return;
                };
                Some(value)
            }
            _ => None,
        };
        let before = self.tab.doc.clone();
        self.error = false;
        if matches!(action, Action::Detach) {
            self.tab
                .doc
                .arrow_labels
                .retain(|label| !ids.contains(&label.annotation));
        } else if let Err(error) = reshiki::arrow_labels::set_gap(&mut self.tab.doc, &ids, value) {
            self.status = error;
            self.error = true;
            return;
        }
        self.changed(before);
        self.sync_arrow_label_gap_input();
        if !self.error {
            self.status = if matches!(action, Action::Detach) {
                "Text detached from arrow".into()
            } else if value.is_none() {
                "Arrow label gap · Auto".into()
            } else {
                format!(
                    "Arrow label gap · {}",
                    self.guides.unit.format(value.unwrap_or_default())
                )
            };
        }
    }

    pub(super) fn arrow_label_gap_panel(&self) -> Option<Element<'_, Message>> {
        use super::inspector::card;
        use super::workspace::{accessible_command, hover_hint, muted_text};
        if self.tab.inline_text.is_some() || self.selected_arrow_labels().is_empty() {
            return None;
        }
        let unit = self.guides.unit;
        let values: Vec<_> = self
            .tab
            .doc
            .arrow_labels
            .iter()
            .filter(|label| self.tab.selected.contains(&label.annotation))
            .map(|label| label.gap)
            .collect();
        let mixed = values
            .first()
            .is_some_and(|first| values.iter().any(|value| value != first));
        let input = reshiki::accessibility::text_input(
            "arrow-label-gap",
            format!("Arrow label gap ({unit})"),
            if mixed { "Mixed" } else { "Auto" },
            &self.tab.arrow_label_gap_input,
        )
        .style(crate::appearance::input_style)
        .size(12)
        .padding(5)
        .width(86)
        .on_input(|v| Message::ArrowLabelGap(Action::Input(v)))
        .on_submit(Message::ArrowLabelGap(Action::Apply));
        let controls = column![
            text("ARROW LABEL SPACING").size(10).style(muted_text),
            row![
                text(format!("Arrow gap ({unit})"))
                    .size(11)
                    .width(Length::Fill),
                hover_hint(input, "Enter applies", tooltip::Position::Top)
            ]
            .spacing(6)
            .align_y(Alignment::Center),
            row![
                accessible_command("arrow-label-gap-auto", "Automatic arrow label gap", "Auto")
                    .on_press(Message::ArrowLabelGap(Action::Auto)),
                accessible_command("arrow-label-detach", "Detach text from arrow", "Detach")
                    .on_press(Message::ArrowLabelGap(Action::Detach))
            ]
            .spacing(6),
            text("Drag away from the shaft to adjust · Drag sideways to detach")
                .size(11)
                .style(muted_text),
        ]
        .spacing(8);
        Some(card(iced::widget::container(controls).padding(12)))
    }
}

#[cfg(test)]
mod tests;
