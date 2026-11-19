//! Persistent, nested logical groups, represented as laminar sets of object IDs.
use crate::document::{Document, Point};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Group {
    pub id: u64,
    pub members: Vec<u64>,
    #[serde(default)]
    pub integral: bool,
    /// A lasting row, column or grid of the group's units.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<crate::layouts::Layout>,
}

/// An object a dragged piece can leave.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Container {
    Cycle,
    Panel,
    Group,
}

impl Container {
    /// The objects a piece left, innermost first, as "the cycle and the panel".
    pub fn names(left: &[Self]) -> Option<String> {
        let names: Vec<&str> = left
            .iter()
            .map(|c| match c {
                Self::Cycle => "the cycle",
                Self::Panel => "the panel",
                Self::Group => "the group",
            })
            .collect();
        (!names.is_empty()).then(|| names.join(" and "))
    }
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
            group.layout = group.layout.take().or(other.layout);
            for id in other.members {
                if !group.members.contains(&id) {
                    group.members.push(id);
                }
            }
        }
        self.prune_groups();
    }

    /// Expand abbreviations, then every group and equipment assembly with a
    /// selected member until nothing changes. The result is in [`Document::all_ids`] order.
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
            // Seated pieces are picked as one assembly, like a group.
            for seat in &self.seats {
                if [Some(seat.a.piece), Some(seat.b.piece), seat.clip]
                    .iter()
                    .flatten()
                    .any(|id| selected.contains(id))
                {
                    selected.extend([seat.a.piece, seat.b.piece].into_iter().chain(seat.clip));
                }
            }
            if count == selected.len() {
                break;
            }
        }
        let ids: Vec<_> = self
            .all_ids()
            .into_iter()
            .filter(|id| selected.contains(id))
            .collect();
        crate::molecule_names::include_annotations(self, &ids)
    }

    /// What a click on `picked` selects. Outside the selection it is the
    /// whole object: group, cycle, panel or assembly. Inside a selected
    /// object it is the next piece in, under the pointer: a nested group, an
    /// assembly, a cycle part or a molecule, then an atom or bond. Another
    /// piece of the same object is picked at the object's top level.
    pub fn click_selection(&self, selected: &[u64], picked: &[u64]) -> Vec<u64> {
        let whole = self.expand_groups(picked);
        let set = |ids: &[u64]| ids.iter().copied().collect::<HashSet<u64>>();
        let (selection, outer) = (set(selected), set(&whole));
        if selection.is_empty() || !selection.is_subset(&outer) {
            return whole;
        }
        let bound = if set(picked).is_subset(&selection) {
            selection
        } else {
            outer
        };
        let base = crate::cycles::whole_parts(self, &self.expand_integral_groups(picked));
        let molecule: Vec<u64> = crate::reactions::molecules(self, &base)
            .into_iter()
            .flatten()
            .chain(base.iter().copied())
            .collect();
        let mut levels = vec![
            self.expand_integral_groups(picked),
            crate::cycles::whole_parts(self, &self.expand_integral_groups(&molecule)),
            self.expand_integral_groups(&crate::equipment::solve::closure(self, &base)),
            base.clone(),
        ];
        levels.extend(
            self.groups
                .iter()
                .filter(|g| base.iter().all(|id| g.members.contains(id)))
                .map(|g| self.expand_integral_groups(&g.members)),
        );
        let level = levels
            .into_iter()
            .map(|l| set(&l))
            .filter(|l| l.len() < bound.len() && l.is_subset(&bound))
            .max_by_key(HashSet::len)
            .unwrap_or(bound);
        self.all_ids()
            .into_iter()
            .filter(|id| level.contains(id))
            .collect()
    }

    /// Take `moved`, one piece just moved by `delta`, out of each object
    /// around it that it now lies outside, from the inside out: a catalytic
    /// cycle once it is farther out than a reagent's place, a panel once it
    /// is not wholly inside the frame, any other group once its middle is
    /// outside the box the group had before the move. The first object still
    /// around it keeps it, and so does everything around that. A piece that
    /// is part of a molecule or cuts across groups stays. Returns what it
    /// left, innermost first.
    pub fn take_out(&mut self, moved: &[u64], delta: Point) -> Vec<Container> {
        // Linked names and captions follow their molecule without being members.
        let grouped: HashSet<u64> = self
            .groups
            .iter()
            .flat_map(|g| g.members.iter().copied())
            .collect();
        let linked: HashSet<u64> = (self.molecule_names.iter().map(|n| n.annotation))
            .chain(self.property_captions.iter().map(|c| c.annotation))
            .collect();
        let set: HashSet<u64> = moved
            .iter()
            .copied()
            .filter(|id| grouped.contains(id) || !linked.contains(id))
            .collect();
        let mut around: Vec<&Group> = self
            .groups
            .iter()
            .filter(|g| g.members.len() > set.len() && set.iter().all(|id| g.members.contains(id)))
            .collect();
        around.sort_by_key(|g| g.members.len());
        let cuts = |g: &Group| {
            let inside = g.members.iter().filter(|id| set.contains(id)).count();
            inside > 0 && inside < set.len() && inside < g.members.len()
        };
        if set.is_empty()
            || around.is_empty()
            || self.groups.iter().any(cuts)
            || self
                .bonds
                .iter()
                .any(|b| set.contains(&b.a) != set.contains(&b.b))
        {
            return vec![];
        }
        let Some(ink) = crate::scene::selection_bounds(self, moved) else {
            return vec![];
        };
        // The smallest group holding a cycle or panel is the one it runs or frames.
        let own = |graphic: u64| {
            self.groups
                .iter()
                .filter(|g| g.members.contains(&graphic))
                .min_by_key(|g| g.members.len())
                .map(|g| g.id)
        };
        let cycle = moved
            .iter()
            .find_map(|id| crate::cycles::role_of(self, *id))
            .map(|(cycle, _)| cycle)
            .filter(|cycle| !set.contains(cycle));
        let (mut groups, mut left) = (vec![], vec![]);
        for g in around {
            let owns = |id: u64| g.members.contains(&id) && own(id) == Some(g.id);
            // A cycle's circle never leaves the group of its own parts.
            if (self.graphics.iter())
                .any(|c| c.cycle.is_some() && set.contains(&c.id) && owns(c.id))
            {
                break;
            }
            let runs = cycle.is_some_and(owns);
            let out_of_cycle = runs && cycle.is_some_and(|c| !crate::cycles::holds(self, c, moved));
            if out_of_cycle {
                left.push(Container::Cycle);
            }
            // An outer panel frames an inner one left empty.
            let area = |p: &&crate::graphics::Graphic| {
                let (lo, hi) = p.shape_bounds();
                (hi.x - lo.x) * (hi.y - lo.y)
            };
            let frame = self
                .graphics
                .iter()
                .filter(|p| p.panel.is_some() && !set.contains(&p.id) && owns(p.id))
                .max_by(|a, b| area(a).total_cmp(&area(b)));
            // A cycle sharing the panel's group keeps a part in its reach.
            let outside = if let Some(frame) = frame {
                !crate::graphics::panel::holds(self, frame.id, moved) && (!runs || out_of_cycle)
            } else if runs {
                out_of_cycle
            } else {
                let rest: Vec<u64> = (g.members.iter().copied())
                    .filter(|id| !set.contains(id))
                    .collect();
                let before = (
                    ink.0.offset(-delta.x, -delta.y),
                    ink.1.offset(-delta.x, -delta.y),
                );
                let (lo, hi) =
                    crate::scene::selection_bounds(self, &rest).map_or(before, |(lo, hi)| {
                        (
                            Point::new(lo.x.min(before.0.x), lo.y.min(before.0.y)),
                            Point::new(hi.x.max(before.1.x), hi.y.max(before.1.y)),
                        )
                    });
                let mid = ink.0.lerp(ink.1, 0.5);
                !((lo.x..=hi.x).contains(&mid.x) && (lo.y..=hi.y).contains(&mid.y))
            };
            if !outside {
                break;
            }
            groups.push(g.id);
            if frame.is_some() {
                left.push(Container::Panel);
            } else if !runs {
                left.push(Container::Group);
            }
        }
        if left.is_empty() {
            return vec![];
        }
        let left_cycle = left.contains(&Container::Cycle);
        let freed = if left_cycle {
            crate::cycles::leave(self, moved)
        } else {
            HashSet::new()
        };
        for group in &mut self.groups {
            if groups.contains(&group.id) {
                group.members.retain(|id| !set.contains(id));
            }
        }
        // Side parts of a cycle left with no steps leave it whole.
        let others: HashSet<u64> = freed.difference(&set).copied().collect();
        crate::cycles::ungroup_freed(self, &others);
        self.prune_groups();
        left
    }

    /// Expand abbreviations, then each integral group with a selected member.
    /// The result is in [`Document::all_ids`] order.
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
        let ids: Vec<_> = self
            .all_ids()
            .into_iter()
            .filter(|id| selected.contains(id))
            .collect();
        crate::molecule_names::include_annotations(self, &ids)
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
            layout: None,
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
                existing.layout = existing.layout.take().or(group.layout);
            } else {
                merged.push(group);
            }
        }
        self.groups = merged;
    }
    pub fn validate_groups(&self) -> Result<(), String> {
        if self.groups.is_empty() {
            return Ok(());
        }
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
            if let Some(layout) = &g.layout {
                layout.validate()?;
            }
        }
        #[cfg(test)]
        crate::generic::probe::enter(crate::generic::probe::Stage::GroupPairs);
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

#[cfg(test)]
mod tests;
