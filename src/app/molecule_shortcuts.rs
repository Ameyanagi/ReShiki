//! Molecule shortcuts and edit context. Context is session-only, follows document
//! history, and never uses selection/hover as evidence that a molecule was edited.
use super::{App, Message, shortcuts::Action};
use crate::canvas::{Edit, Tool};
use reshiki::{
    document::{Arrow, Document, Point},
    editing,
    reactions::{self, Role},
};
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Default)]
pub(super) struct Recent {
    epoch: u64,
    atoms: Vec<u64>,
    edit_atoms: Vec<u64>,
    undo: VecDeque<Vec<u64>>,
    redo: VecDeque<Vec<u64>>,
}

impl Recent {
    fn check_epoch(&mut self, epoch: u64) {
        if self.epoch != epoch {
            *self = Self {
                epoch,
                ..Self::default()
            };
        }
    }

    pub(super) fn record(
        &mut self,
        before: &Document,
        after: &Document,
        epoch: u64,
        continuing: bool,
    ) {
        self.check_epoch(epoch);
        if before == after {
            return;
        }
        // Mirror History, including nonmolecular edits and continuous gestures.
        if !continuing || self.undo.is_empty() {
            self.edit_atoms.clear();
            self.undo.push_back(self.atoms.clone());
            if self.undo.len() > 100 {
                let _ = self.undo.pop_front();
            }
        }
        self.redo.clear();
        let changed = changed_atoms(before, after);
        if !changed.is_empty() {
            // A drag is one edit even when successive segments touch different
            // molecules. Keep only this edit's atoms, not the previous action's.
            self.edit_atoms.extend(changed);
            self.edit_atoms.sort_unstable();
            self.edit_atoms.dedup();
            self.atoms.clone_from(&self.edit_atoms);
        }
        let existing: HashSet<_> = after.atoms.iter().map(|a| a.id).collect();
        self.atoms.retain(|id| existing.contains(id));
    }

    pub(super) fn restore(&mut self, redo: bool, epoch: u64) {
        self.check_epoch(epoch);
        self.edit_atoms.clear();
        let (source, target) = if redo {
            (&mut self.redo, &mut self.undo)
        } else {
            (&mut self.undo, &mut self.redo)
        };
        // Loading a document starts a new context even if drawing Undo remains
        // available across an import. Do not infer edits from reused atom IDs.
        let atoms = source.pop_back().unwrap_or_default();
        target.push_back(std::mem::replace(&mut self.atoms, atoms));
    }

    fn selection(&mut self, doc: &Document, epoch: u64) -> Vec<u64> {
        self.check_epoch(epoch);
        reactions::molecules(doc, &self.atoms)
            .into_iter()
            .flatten()
            .collect()
    }
}

fn changed_atoms(before: &Document, after: &Document) -> Vec<u64> {
    let mut changed = HashSet::new();
    let previous_atoms: HashMap<_, _> = before.atoms.iter().map(|a| (a.id, a)).collect();
    let current_atoms: HashSet<_> = after.atoms.iter().map(|a| a.id).collect();
    for atom in &after.atoms {
        let same = previous_atoms.get(&atom.id).is_some_and(|previous| {
            let mut previous = (*previous).clone();
            // Engine-derived labels can refresh on unrelated molecules.
            previous.cip_label = atom.cip_label.clone();
            previous.label_h = atom.label_h;
            previous == *atom
        });
        if !same {
            changed.insert(atom.id);
        }
    }
    let removed: Vec<_> = before
        .atoms
        .iter()
        .filter(|a| !current_atoms.contains(&a.id))
        .map(|a| a.id)
        .collect();
    // Removing atoms edits the remaining pieces of their molecules. Resolve
    // each old component once, even when a deletion removes most of its atoms.
    changed.extend(reactions::molecules(before, &removed).into_iter().flatten());
    for (a, b) in [(before, after), (after, before)] {
        let other_bonds: HashMap<_, _> = b
            .bonds
            .iter()
            .map(|bond| ((bond.a, bond.b), bond))
            .collect();
        for bond in &a.bonds {
            if !other_bonds.get(&(bond.a, bond.b)).is_some_and(|other| {
                let mut bond = bond.clone();
                bond.cip_label = other.cip_label.clone();
                bond == **other
            }) {
                changed.extend([bond.a, bond.b]);
            }
        }
        for group in &a.abbreviations {
            if !b.abbreviations.contains(group) {
                changed.extend(&group.members);
            }
        }
        for fill in &a.ring_fills {
            if !b.ring_fills.contains(fill) {
                changed.extend(&fill.atoms);
            }
        }
    }
    let mut ids: Vec<_> = changed.into_iter().collect();
    ids.sort_unstable();
    ids
}

impl App {
    pub(super) fn prepare_molecule_shortcut(&self, message: Message) -> Option<Message> {
        if matches!(
            message,
            Message::Shortcut(Action::SelectRecent | Action::ReactionCopy)
        ) {
            // Focused widgets normally capture keys before subscription dispatch.
            // These guards also cover an unfocused, still-open text draft.
            if self.tab.inline_text.is_some()
                || self.tab.atom_text.is_some()
                || self.help_open
                || self.updates.open
            {
                return None;
            }
            if matches!(message, Message::Shortcut(Action::SelectRecent))
                && (self.tool != Tool::Select
                    || self.palette.is_some()
                    || self.assistant.menu.is_some()
                    || self.tab.cleanup.is_some()
                    || self.tab.joining.is_some())
            {
                return Some(Message::Tool(Tool::Select));
            }
            if matches!(message, Message::Shortcut(Action::ReactionCopy))
                && self.tab.joining.is_some()
            {
                return None;
            }
        }
        Some(message)
    }

    pub(super) fn select_recent_shortcut(&mut self) {
        let ids = self
            .tab
            .recent_molecules
            .selection(&self.tab.doc, self.tab.file_epoch);
        if ids.is_empty() {
            self.status = "No recently edited molecule in this drawing".into();
            self.error = false;
            return;
        }
        // Reuse normal selection synchronization without expanding a molecule
        // into unrelated captions or structures in its user-created group.
        self.edit(Edit::Select(ids));
        self.status = "Selected the most recently edited molecule(s)".into();
        self.error = false;
    }

    pub(super) fn reaction_copy_shortcut(&mut self) {
        match reaction_copy(
            &self.tab.doc,
            &self.tab.selected,
            self.tab.bond_drawing.length,
        ) {
            Ok((doc, ids)) => {
                let before = std::mem::replace(&mut self.tab.doc, doc);
                self.changed(before);
                if !self.error {
                    self.tool = Tool::Select;
                    self.edit(Edit::Select(ids));
                    self.status =
                        "Reaction arrow and product copy created · Undo removes both".into();
                }
            }
            Err(error) => {
                self.status = error;
                self.error = true;
            }
        }
    }
}

fn reaction_copy(
    source: &Document,
    selected: &[u64],
    length: f32,
) -> Result<(Document, Vec<u64>), String> {
    source.validate()?;
    if !length.is_finite() || length <= 0. {
        return Err("Choose a positive bond length first".into());
    }
    let mut ids = source.expand_groups(selected);
    let molecules = reactions::molecules(source, &ids);
    if molecules.is_empty() || source.arrows.iter().any(|arrow| ids.contains(&arrow.id)) {
        return Err("Select a molecule or molecular group without reaction arrows first".into());
    }
    for id in molecules.iter().flatten() {
        if !ids.contains(id) {
            ids.push(*id);
        }
    }
    let part = editing::selection(source, &ids);
    let (lo, hi) = reshiki::scene::selection_bounds(&part, &part.all_ids())
        .ok_or("The selected molecule has no drawing bounds")?;
    let gap = length;
    let arrow_length = length * 2.;
    let offset = Point::new(hi.x - lo.x + gap * 2. + arrow_length, 0.);
    let mut doc = source.clone();
    let copied = editing::append(&mut doc, &part, offset);
    if copied.is_empty() {
        return Err("Could not copy the selected molecule".into());
    }
    let arrow = doc.next_id();
    let y = (lo.y + hi.y) / 2.;
    doc.arrows.push(Arrow {
        id: arrow,
        start: Point::new(hi.x + gap, y),
        end: Point::new(hi.x + gap + arrow_length, y),
        kind: "forward".into(),
        control: None,
        cubic: None,
        style: None,
    });
    let mut new_ids = copied.clone();
    new_ids.push(arrow);
    let (new_lo, new_hi) = reshiki::scene::selection_bounds(&doc, &new_ids)
        .ok_or("Could not measure the product drawing")?;
    let original_ids: HashSet<_> = part.all_ids().into_iter().collect();
    let other_ids: Vec<_> = source
        .all_ids()
        .into_iter()
        .filter(|id| !original_ids.contains(id))
        .collect();
    // Measure whole components so long bonds cannot cross the destination
    // while both endpoints lie outside it.
    let mut obstacles = reactions::molecules(source, &other_ids);
    obstacles.extend(
        other_ids
            .into_iter()
            .filter(|id| {
                source.atom(*id).is_none()
                    && !source.graphics.iter().any(|g| g.id == *id && g.layer < 0)
            })
            .map(|id| vec![id]),
    );
    for obstacle in obstacles {
        if let Some((a, b)) = reshiki::scene::selection_bounds(source, &obstacle)
            && a.x < new_hi.x + gap / 4.
            && b.x > new_lo.x - gap / 4.
            && a.y < new_hi.y + gap / 4.
            && b.y > new_lo.y - gap / 4.
        {
            return Err(
                "Make room to the right of the selected molecule before adding a reaction".into(),
            );
        }
    }
    reactions::assign(&mut doc, arrow, &ids, Role::Reactant)?;
    reactions::assign(&mut doc, arrow, &copied, Role::Product)?;
    doc.validate()?;
    Ok((doc, copied))
}

#[cfg(test)]
mod tests;
