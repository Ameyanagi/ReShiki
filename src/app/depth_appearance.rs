use super::{App, Message};
use iced::Task;
use std::collections::{HashMap, HashSet};

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
    pub(super) fn rear_opacity_input(&self, ids: &[u64]) -> String {
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
    pub(super) fn rear_opacity_visible(&self, ids: &[u64]) -> bool {
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
                scope.rear_opacity < 1. && scope.atoms.iter().any(|id| ids.contains(id))
            })
    }

    /// Complete covalent components; graphics and haptic centroids are paint,
    /// not particles or chemical connectivity.
    pub(super) fn depth_ids(&self) -> Vec<u64> {
        let document = self.display_document();
        let real: HashSet<_> = document
            .atoms
            .iter()
            .filter(|atom| atom.centroid.is_empty())
            .map(|atom| atom.id)
            .collect();
        let mut ids: HashSet<_> = if self.tab.selected.is_empty() {
            real.clone()
        } else {
            document
                .expand_abbreviation_selection(&self.tab.selected)
                .into_iter()
                .filter(|id| real.contains(id))
                .collect()
        };
        let mut adjacent: HashMap<u64, Vec<u64>> = HashMap::new();
        for bond in &document.bonds {
            if matches!(bond.order, 1..=4) && real.contains(&bond.a) && real.contains(&bond.b) {
                adjacent.entry(bond.a).or_default().push(bond.b);
                adjacent.entry(bond.b).or_default().push(bond.a);
            }
        }
        let mut pending: Vec<_> = ids.iter().copied().collect();
        while let Some(id) = pending.pop() {
            for neighbor in adjacent.get(&id).into_iter().flatten() {
                if ids.insert(*neighbor) {
                    pending.push(*neighbor);
                }
            }
        }
        let mut ids: Vec<_> = ids.into_iter().collect();
        ids.sort_unstable();
        ids
    }

    pub(super) fn depth_appearance_action(&mut self, action: Action) -> Task<Message> {
        let ids = self.depth_ids();
        if ids.is_empty() {
            self.status = "Select a molecule to edit its depth appearance".into();
            self.error = true;
            return Task::none();
        }
        if let Action::RearInput(draft) = &action {
            if self.rear_draft_current(draft, &ids) {
                self.tab.rear_opacity_draft = Some(draft.clone());
            }
            return Task::none();
        }
        let rear = if matches!(action, Action::ApplyRear) {
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
            Some(value / 100.)
        } else {
            None
        };
        let before = self.tab.doc.clone();
        let result = match &action {
            Action::Enhance(true) => reshiki::depth_appearance::enable(
                &mut self.tab.doc,
                &ids,
                reshiki::depth_appearance::DEFAULT_STRENGTH,
            ),
            Action::Enhance(false) => {
                Ok(reshiki::depth_appearance::freeze(&mut self.tab.doc, &ids))
            }
            Action::Clear => Ok(reshiki::depth_appearance::clear(&mut self.tab.doc, &ids)),
            Action::ApplyRear => reshiki::depth_appearance::set_rear_opacity(
                &mut self.tab.doc,
                &ids,
                rear.unwrap_or(1.),
            ),
            Action::RearInput(_) => Ok(0),
            Action::OriginalInk => reshiki::depth_appearance::override_fade(
                &mut self.tab.doc,
                &self.tab.selected,
                Some(0.),
            ),
        };
        if let Err(error) = result {
            self.tab.doc = before;
            self.status = error;
            self.error = true;
            return Task::none();
        }
        self.tab.rear_opacity_draft = None;
        self.error = false;
        self.changed(before);
        if !self.error {
            self.status = match action {
                Action::ApplyRear => {
                    self.status = format!(
                        "Rear opacity {}% · Chemical structure and XYZ retained",
                        rear.unwrap_or(1.) * 100.
                    );
                    return Task::none();
                }
                Action::RearInput(_) => "Rear opacity input",
                Action::Enhance(true) => {
                    "Depth enhancement on · Rear atoms and bonds fade with depth"
                }
                Action::Enhance(false) => {
                    "Depth appearance frozen · Positions and colors remain editable"
                }
                Action::Clear => "Depth appearance cleared · Original ink and opacity restored",
                Action::OriginalInk => "Selected atoms and bonds use their original ink",
            }
            .into();
        }
        Task::none()
    }
}
