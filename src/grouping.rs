//! Persistent, nested logical groups, represented as laminar sets of object IDs.
use crate::document::Document;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Group {
    pub id: u64,
    pub members: Vec<u64>,
    #[serde(default)]
    pub integral: bool,
}

impl Document {
    /// Extending a grouped molecule keeps its new atoms in the group. Connecting
    /// separately grouped molecules unites overlapping groups and keeps captions.
    pub fn reconcile_molecule_groups(&mut self) {
        if self.groups.is_empty() {
            return;
        }
        for group in &mut self.groups {
            let mut members: HashSet<_> = group.members.iter().copied().collect();
            loop {
                let count = group.members.len();
                for b in &self.bonds {
                    if members.contains(&b.a) != members.contains(&b.b) {
                        let id = if members.contains(&b.a) { b.b } else { b.a };
                        members.insert(id);
                        group.members.push(id);
                    }
                }
                if count == group.members.len() {
                    break;
                }
            }
        }
        loop {
            let conflict = self.groups.iter().enumerate().find_map(|(i, a)| {
                self.groups
                    .iter()
                    .enumerate()
                    .skip(i + 1)
                    .find_map(|(j, b)| {
                        let overlap = a.members.iter().filter(|id| b.members.contains(id)).count();
                        (overlap > 0 && overlap < a.members.len().min(b.members.len()))
                            .then_some((i, j))
                    })
            });
            let Some((i, j)) = conflict else { break };
            if j >= self.groups.len() {
                break;
            }
            let other = self.groups.remove(j);
            let Some(group) = self.groups.get_mut(i) else {
                break;
            };
            group.integral |= other.integral;
            for id in other.members {
                if !group.members.contains(&id) {
                    group.members.push(id);
                }
            }
        }
        self.prune_groups();
    }

    pub fn expand_groups(&self, ids: &[u64]) -> Vec<u64> {
        let mut selected: HashSet<_> = self
            .expand_abbreviation_selection(ids)
            .into_iter()
            .collect();
        loop {
            let count = selected.len();
            for group in &self.groups {
                if group.members.iter().any(|id| selected.contains(id)) {
                    selected.extend(&group.members);
                }
            }
            if count == selected.len() {
                break;
            }
        }
        self.all_ids()
            .into_iter()
            .filter(|id| selected.contains(id))
            .collect()
    }

    pub fn expand_integral_groups(&self, ids: &[u64]) -> Vec<u64> {
        let mut selected: HashSet<_> = self
            .expand_abbreviation_selection(ids)
            .into_iter()
            .collect();
        for group in &self.groups {
            if group.integral && group.members.iter().any(|id| selected.contains(id)) {
                selected.extend(&group.members);
            }
        }
        self.all_ids()
            .into_iter()
            .filter(|id| selected.contains(id))
            .collect()
    }

    /// Include complete molecules so the grouping boundary never cuts a bond.
    pub fn complete_selection(&self, ids: &[u64]) -> Vec<u64> {
        let mut members = self.expand_groups(ids);
        loop {
            let count = members.len();
            for b in &self.bonds {
                if members.contains(&b.a) != members.contains(&b.b) {
                    members.push(if members.contains(&b.a) { b.b } else { b.a });
                }
            }
            members = self.expand_groups(&members);
            if count == members.len() {
                break;
            }
        }
        members
    }
    pub fn group_selection(&mut self, ids: &[u64]) -> Result<Vec<u64>, String> {
        let members = self.complete_selection(ids);
        if members.len() < 2 {
            return Err("Select at least two objects or atoms to group".into());
        }
        if self.groups.iter().any(|g| {
            g.members.len() == members.len() && g.members.iter().all(|id| members.contains(id))
        }) {
            return Err("This selection is already one group".into());
        }
        self.groups.push(Group {
            id: self.next_id(),
            members: members.clone(),
            integral: false,
        });
        Ok(members)
    }

    pub fn outer_selected_groups(&self, ids: &[u64]) -> Vec<u64> {
        self.groups
            .iter()
            .filter(|g| g.members.iter().all(|id| ids.contains(id)))
            .filter(|g| {
                !self.groups.iter().any(|outer| {
                    outer.members.len() > g.members.len()
                        && g.members.iter().all(|id| outer.members.contains(id))
                        && outer.members.iter().all(|id| ids.contains(id))
                })
            })
            .map(|g| g.id)
            .collect()
    }
    pub fn ungroup_selection(&mut self, ids: &[u64]) -> bool {
        let remove = self.outer_selected_groups(ids);
        self.groups.retain(|g| !remove.contains(&g.id));
        !remove.is_empty()
    }
    pub fn prune_groups(&mut self) {
        let ids: HashSet<_> = self.all_ids().into_iter().collect();
        for group in &mut self.groups {
            group.members.retain(|id| ids.contains(id));
        }
        let mut merged: Vec<Group> = Vec::new();
        for mut group in std::mem::take(&mut self.groups) {
            let members = &mut group.members;
            members.sort_unstable();
            if members.len() < 2 {
                continue;
            }
            if let Some(existing) = merged.iter_mut().find(|g| g.members == *members) {
                existing.integral |= group.integral;
            } else {
                merged.push(group);
            }
        }
        self.groups = merged;
    }
    pub fn validate_groups(&self) -> Result<(), String> {
        let objects: HashSet<_> = self.all_ids().into_iter().collect();
        let mut ids = objects.clone();
        for g in &self.groups {
            if g.id == 0
                || g.id == u64::MAX
                || !ids.insert(g.id)
                || g.members.len() < 2
                || g.members.iter().any(|id| !objects.contains(id))
                || g.members.iter().collect::<HashSet<_>>().len() != g.members.len()
            {
                return Err("Invalid group ID or membership".into());
            }
        }
        for (i, g) in self.groups.iter().enumerate() {
            for other in self.groups.iter().skip(i.saturating_add(1)) {
                let overlap = g
                    .members
                    .iter()
                    .filter(|id| other.members.contains(id))
                    .count();
                if overlap > 0
                    && (overlap != g.members.len().min(other.members.len())
                        || (g.members.len() == other.members.len()))
                {
                    return Err("Groups must be separate or strictly nested".into());
                }
            }
        }
        Ok(())
    }
}
