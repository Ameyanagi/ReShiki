//! Move existing fragments through the same geometry used by template attachment.
//! Preparation and previews never mutate the source document.
use crate::{
    document::{Document, Point},
    editing,
    templates::{self, Anchor, Connection},
};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct Prepared {
    pub original: Document,
    pub fragment: Document,
    pub base: Document,
    pub moving: Vec<u64>,
}
impl Prepared {
    pub fn new(doc: &Document, selected: &[u64]) -> Result<Self, String> {
        doc.validate()?;
        let all: HashSet<_> = doc.all_ids().into_iter().collect();
        if selected.is_empty() || selected.iter().any(|id| !all.contains(id)) {
            return Err("Select the fragment you want to move and attach.".into());
        }
        // Moving a selected part of a molecule must never cut its external bonds.
        let mut moving: HashSet<_> = doc.expand_integral_groups(selected).into_iter().collect();
        loop {
            let previous = moving.len();
            for bond in &doc.bonds {
                if moving.contains(&bond.a) || moving.contains(&bond.b) {
                    moving.insert(bond.a);
                    moving.insert(bond.b);
                }
            }
            moving.extend(
                doc.expand_abbreviation_selection(&moving.iter().copied().collect::<Vec<_>>()),
            );
            if previous == moving.len() {
                break;
            }
        }
        let moving: Vec<_> = doc
            .all_ids()
            .into_iter()
            .filter(|id| moving.contains(id))
            .collect();
        let fragment = editing::selection(doc, &moving);
        if fragment.atoms.is_empty() {
            return Err("Select a molecule or atom to move and attach.".into());
        }
        let mut base = doc.clone();
        base.delete(&moving);
        if base.atoms.is_empty() {
            return Err("Leave a separate molecule or atom available as the destination.".into());
        }
        Ok(Self {
            original: doc.clone(),
            fragment,
            base,
            moving,
        })
    }

    pub fn default_anchor(&self, mode: Connection) -> Anchor {
        if mode == Connection::FuseBond {
            self.fragment
                .bonds
                .iter()
                .find(|b| self.fragment.bond_visible(b.a, b.b))
                .map(|b| Anchor::Bond(b.a, b.b))
                .unwrap_or(Anchor::Auto)
        } else {
            self.fragment
                .atoms
                .iter()
                .find(|a| self.fragment.atom_visible(a.id))
                .map(|a| Anchor::Atom(a.id))
                .unwrap_or(Anchor::Auto)
        }
    }

    pub fn place(
        &self,
        point: Point,
        direction: Option<Point>,
        radius: f32,
        anchor: Anchor,
        mode: Connection,
    ) -> Result<(Document, Vec<u64>), String> {
        if !anchor.valid(&self.fragment) || anchor == Anchor::Auto {
            return Err("Choose an exact source atom or bond in the fragment preview.".into());
        }
        let target = self.base.nearest(point, radius);
        let bond = target
            .is_none()
            .then(|| editing::nearest_bond(&self.base, point, radius))
            .flatten()
            .and_then(|index| self.base.bonds.get(index));
        match mode {
            Connection::Connect | Connection::ShareAtom
                if target.is_none() || !matches!(anchor, Anchor::Atom(_)) =>
            {
                return Err("Choose a source atom, then point to a destination atom.".into());
            }
            Connection::FuseBond if bond.is_none() || !matches!(anchor, Anchor::Bond(..)) => {
                return Err("Choose a source bond, then point to a destination bond.".into());
            }
            Connection::Auto => return Err("Choose how to attach the fragment.".into()),
            _ => {}
        }
        let source_ids = match anchor {
            Anchor::Atom(id) => vec![id],
            Anchor::Bond(a, b) => vec![a, b],
            Anchor::Auto => vec![],
        };
        let target_ids: Vec<_> = target
            .into_iter()
            .chain(bond.into_iter().flat_map(|b| [b.a, b.b]))
            .collect();
        if source_ids
            .iter()
            .any(|id| self.fragment.abbreviation(*id).is_some())
            || target_ids
                .iter()
                .any(|id| self.base.abbreviation(*id).is_some())
        {
            return Err(
                "Expand the abbreviation before using its atoms as attachment points.".into(),
            );
        }
        let (mut placed, ids) = templates::place_with_mode(
            &self.base,
            &self.fragment,
            point,
            direction,
            radius,
            anchor,
            mode,
        )?;
        let originals = self.fragment.all_ids();
        if originals.len() != ids.len() {
            return Err("The fragment could not be joined.".into());
        }
        let host: HashSet<_> = self.base.all_ids().into_iter().collect();
        let restore: HashMap<_, _> = ids
            .iter()
            .copied()
            .zip(originals.iter().copied())
            .filter(|(new, _)| !host.contains(new))
            .collect();
        let merged: HashMap<_, _> = originals
            .into_iter()
            .zip(ids.iter().copied())
            .filter(|(_, new)| host.contains(new))
            .collect();
        let original_id = |id: u64| restore.get(&id).copied().unwrap_or(id);
        // Restore IDs of moved objects simultaneously. Only shared atoms disappear;
        // destination IDs, labels, remote stereo and unrelated objects retain identity.
        for a in &mut placed.atoms {
            a.id = original_id(a.id);
            if let Some(stereo) = &mut a.stereo {
                for neighbor in &mut stereo.neighbors {
                    *neighbor = original_id(*neighbor);
                }
            }
        }
        for b in &mut placed.bonds {
            b.a = original_id(b.a);
            b.b = original_id(b.b);
            for neighbor in &mut b.stereo_atoms {
                *neighbor = original_id(*neighbor);
            }
        }
        for a in &mut placed.annotations {
            a.id = original_id(a.id);
        }
        for a in &mut placed.arrows {
            a.id = original_id(a.id);
        }
        for g in &mut placed.graphics {
            g.id = original_id(g.id);
        }
        for g in &mut placed.abbreviations {
            g.anchor = original_id(g.anchor);
            for member in &mut g.members {
                *member = original_id(*member);
            }
        }
        // Restore logical groups, including groups that span the moved and fixed
        // fragments. Group reconciliation unites newly connected molecules.
        placed.groups = self.original.groups.clone();
        for group in &mut placed.groups {
            for id in &mut group.members {
                *id = merged.get(id).copied().unwrap_or(*id);
            }
            group.members.sort_unstable();
            group.members.dedup();
        }
        placed.reconcile_molecule_groups();
        crate::reactions::reconcile(&mut placed)?;
        placed.validate()?;
        Ok((placed, ids.into_iter().map(original_id).collect()))
    }
}
