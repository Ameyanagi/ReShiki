use super::{App, Message};
use iced::Task;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub enum Action {
    Enhance(bool),
    Clear,
    OriginalInk,
}

impl App {
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
        let before = self.tab.doc.clone();
        let result = match action {
            Action::Enhance(true) => reshiki::depth_appearance::enable(
                &mut self.tab.doc,
                &ids,
                reshiki::depth_appearance::DEFAULT_STRENGTH,
            ),
            Action::Enhance(false) => {
                Ok(reshiki::depth_appearance::freeze(&mut self.tab.doc, &ids))
            }
            Action::Clear => Ok(reshiki::depth_appearance::clear(&mut self.tab.doc, &ids)),
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
        self.changed(before);
        if !self.error {
            self.status = match action {
                Action::Enhance(true) => {
                    "Depth enhancement on · Rear atoms and bonds fade with depth"
                }
                Action::Enhance(false) => {
                    "Depth appearance frozen · Positions and colors remain editable"
                }
                Action::Clear => "Depth appearance cleared · Original colors restored",
                Action::OriginalInk => "Selected atoms and bonds use their original ink",
            }
            .into();
        }
        Task::none()
    }
}
