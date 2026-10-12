use super::{App, Message, inspector::Section, workspace::muted_text};
use iced::widget::{column, row, text};
use iced::{Alignment, Element, Length, Task};
use reshiki::document::Document;
use std::collections::HashSet;

const REAR_OPACITY_HINT: &str =
    "100% keeps all ink. 0% hides occluded rear cage ink. Exposed outlines stay solid.";

#[derive(Debug, Clone)]
pub enum Action {
    Enhance(bool),
    Clear,
    OriginalInk,
    RearInput(Draft),
    ApplyRear,
}

#[derive(Debug, Clone)]
pub struct Draft {
    pub ids: Vec<u64>,
    pub revision: u64,
    pub file_epoch: u64,
    pub value: String,
}

impl App {
    fn rear_draft_current(&self, draft: &Draft, ids: &[u64]) -> bool {
        draft.ids == ids
            && draft.revision == self.tab.revision
            && draft.file_epoch == self.tab.file_epoch
    }
    fn rear_opacity_text(&self, ids: &[u64]) -> String {
        if let Some(draft) = self
            .tab
            .rear_opacity_draft
            .as_ref()
            .filter(|draft| self.rear_draft_current(draft, ids))
        {
            return draft.value.clone();
        }
        reshiki::depth_appearance::rear_opacity(&self.tab.doc, ids)
            .map(|alpha| format!("{}", alpha * 100.))
            .unwrap_or_default()
    }
    fn rear_opacity_visible(&self, ids: &[u64]) -> bool {
        let ids: HashSet<_> = ids.iter().copied().collect();
        self.tab
            .doc
            .atoms
            .iter()
            .any(|a| ids.contains(&a.id) && a.depth.abs() > 0.001)
            || self
                .tab
                .doc
                .bonds
                .iter()
                .any(|b| b.projection && ids.contains(&b.a) && ids.contains(&b.b))
            || self.tab.doc.depth_appearance.iter().any(|scope| {
                scope.has_rear_opacity() && scope.atoms.iter().any(|id| ids.contains(id))
            })
    }

    /// Complete covalent components; graphics and haptic centroids are paint,
    /// not particles or chemical connectivity.
    pub(super) fn depth_ids(&self) -> Vec<u64> {
        let document = self.display_document();
        let seeds = if self.tab.selected.is_empty() {
            document.atoms.iter().map(|atom| atom.id).collect()
        } else {
            document.expand_abbreviation_selection(&self.tab.selected)
        };
        reshiki::depth_appearance::covalent_component_ids(document, &seeds)
    }

    pub(super) fn rear_opacity_section(&self, ids: Vec<u64>) -> Option<Element<'_, Message>> {
        if !self.rear_opacity_visible(&ids) {
            return None;
        }
        Some(self.inspector_section_lazy(
            Section::ProjectionAppearance,
            "3D appearance",
            "",
            true,
            || {
                let value = self.rear_opacity_text(&ids);
                let revision = self.tab.revision;
                let file_epoch = self.tab.file_epoch;
                let apply = Message::DepthAppearance(Action::ApplyRear);
                column![
                    text("Rear opacity (%)").size(12),
                    row![
                        reshiki::accessibility::text_input(
                            "projection-rear-opacity",
                            "Rear opacity percent",
                            "Mixed",
                            &value
                        )
                        .style(crate::appearance::input_style)
                        .on_input(move |value| {
                            Message::DepthAppearance(Action::RearInput(Draft {
                                ids: ids.clone(),
                                revision,
                                file_epoch,
                                value,
                            }))
                        })
                        .on_submit(apply.clone())
                        .width(Length::Fixed(80.))
                        .size(12),
                        reshiki::accessibility::button(
                            "projection-rear-opacity-apply",
                            "Apply rear opacity",
                            text("Apply").size(12)
                        )
                        .padding([7, 9])
                        .style(super::workspace::control(false))
                        .on_press(apply),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    text(REAR_OPACITY_HINT).size(11).style(muted_text),
                ]
                .spacing(6)
            },
        ))
    }

    pub(super) fn depth_appearance_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::RearInput(draft) => self.rear_opacity_input(draft),
            Action::ApplyRear => self.apply_rear_opacity(),
            Action::Enhance(true) => self.edit_depth_appearance(
                "Depth enhancement on · Rear atoms and bonds fade with depth",
                |app, ids| {
                    reshiki::depth_appearance::enable(
                        &mut app.tab.doc,
                        ids,
                        reshiki::depth_appearance::DEFAULT_STRENGTH,
                    )
                },
            ),
            Action::Enhance(false) => self.edit_depth_appearance(
                "Depth appearance frozen · Positions and colors remain editable",
                |app, ids| Ok(reshiki::depth_appearance::freeze(&mut app.tab.doc, ids)),
            ),
            Action::Clear => self.edit_depth_appearance(
                "Depth appearance cleared · Original ink and opacity restored",
                |app, ids| Ok(reshiki::depth_appearance::clear(&mut app.tab.doc, ids)),
            ),
            Action::OriginalInk => self.edit_depth_appearance(
                "Selected atoms and bonds use their original ink",
                |app, _| {
                    reshiki::depth_appearance::override_fade(
                        &mut app.tab.doc,
                        &app.tab.selected,
                        Some(0.),
                    )
                },
            ),
        }
    }

    fn depth_targets(&mut self) -> Option<Vec<u64>> {
        let ids = self.depth_ids();
        if ids.is_empty() {
            self.status = "Select a molecule to edit its depth appearance".into();
            self.error = true;
            None
        } else {
            Some(ids)
        }
    }

    fn rear_opacity_input(&mut self, draft: Draft) -> Task<Message> {
        let Some(ids) = self.depth_targets() else {
            return Task::none();
        };
        if self.rear_draft_current(&draft, &ids) {
            self.tab.rear_opacity_draft = Some(draft);
        }
        Task::none()
    }

    fn apply_rear_opacity(&mut self) -> Task<Message> {
        let Some(ids) = self.depth_targets() else {
            return Task::none();
        };
        let Some(draft) = self
            .tab
            .rear_opacity_draft
            .as_ref()
            .filter(|draft| self.rear_draft_current(draft, &ids))
        else {
            return Task::none();
        };
        let value = draft
            .value
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite() && (0. ..=100.).contains(v));
        let Some(value) = value else {
            self.status = "Rear opacity must be between 0% and 100%".into();
            self.error = true;
            return Task::none();
        };
        let rear = value / 100.;
        let before = self.tab.doc.clone();
        let result = reshiki::depth_appearance::set_rear_opacity(&mut self.tab.doc, &ids, rear);
        if self.commit_depth_edit(before, result) {
            self.status = format!(
                "Rear opacity {}% · Chemical structure and XYZ retained",
                rear * 100.
            );
        }
        Task::none()
    }

    fn edit_depth_appearance(
        &mut self,
        status: &'static str,
        edit: impl FnOnce(&mut Self, &[u64]) -> Result<usize, String>,
    ) -> Task<Message> {
        let Some(ids) = self.depth_targets() else {
            return Task::none();
        };
        let before = self.tab.doc.clone();
        let result = edit(self, &ids);
        if self.commit_depth_edit(before, result) {
            self.status = status.into();
        }
        Task::none()
    }

    fn commit_depth_edit(&mut self, before: Document, result: Result<usize, String>) -> bool {
        if let Err(error) = result {
            self.tab.doc = before;
            self.status = error;
            self.error = true;
            return false;
        }
        self.tab.rear_opacity_draft = None;
        self.error = false;
        self.changed(before);
        !self.error
    }
}

#[cfg(test)]
mod tests;
