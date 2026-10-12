//! Move existing fragments through the same geometry used by template attachment.
//! Preparation and previews never mutate the source document.
use crate::{
    document::{Document, Point},
    editing,
    templates::{self, Anchor, Connection},
};
use std::collections::{HashMap, HashSet};

mod atom_merge;
pub use atom_merge::{merge_atoms, merge_atoms_at};
mod coordination;
pub use coordination::{Coordination, coordinate_atoms, coordination_donor, coordination_metal};
mod plan;
pub use plan::{JoinPlan, join_plan};

/// The result of placing a prepared fragment or adding an in-place contact.
#[derive(Debug, Clone, PartialEq)]
pub enum Placed {
    /// A moved fragment was joined to its destination.
    Joined(Document),
    /// An in-place contact was added or was already present.
    Coordinated(Coordination),
}

impl Placed {
    /// Return the drawing to install, preserving duplicate-contact copy behavior.
    pub fn into_document(self, original: &Document) -> Document {
        match self {
            Self::Joined(document) => document,
            Self::Coordinated(outcome) => outcome.into_document(original),
        }
    }
}

enum Destination {
    Coordinate {
        donor: u64,
        metal: u64,
    },
    Attach {
        source_ids: Vec<u64>,
        target_ids: Vec<u64>,
    },
}

/// Source IDs that Move & attach will actually carry, before pruning data.
/// See also [`crate::editing::reference::scope`], whose expansion order differs.
fn moving_ids(doc: &Document, selected: &[u64]) -> Vec<u64> {
    // Moving a selected part of a molecule must never cut its external bonds.
    let mut moving: HashSet<_> = doc.expand_integral_groups(selected).into_iter().collect();
    loop {
        let previous = moving.len();
        moving.extend(crate::attachments::selection(
            doc,
            &moving.iter().copied().collect::<Vec<_>>(),
        ));
        for bond in &doc.bonds {
            if moving.contains(&bond.a) || moving.contains(&bond.b) {
                moving.insert(bond.a);
                moving.insert(bond.b);
            }
        }
        moving
            .extend(doc.expand_abbreviation_selection(&moving.iter().copied().collect::<Vec<_>>()));
        if previous == moving.len() {
            break;
        }
    }
    doc.all_ids()
        .into_iter()
        .filter(|id| moving.contains(id))
        .collect()
}

/// Read-only admission for routes that prepare a fragment. Direct atom merge
/// and in-place coordination deliberately do not expand integral groups.
pub fn source_intersects_generic(doc: &Document, selected: &[u64]) -> bool {
    let moving = moving_ids(doc, selected);
    crate::generic::touches(doc, &moving)
}

#[derive(Debug, Clone)]
pub struct Prepared {
    pub original: Document,
    pub fragment: Document,
    pub base: Document,
    pub moving: Vec<u64>,
}
impl Prepared {
    pub fn new(doc: &Document, selected: &[u64]) -> Result<Self, String> {
        Self::with_mode(doc, selected, Connection::Connect)
    }

    pub fn with_mode(doc: &Document, selected: &[u64], mode: Connection) -> Result<Self, String> {
        doc.validate()?;
        let all: HashSet<_> = doc.object_ids().collect();
        if selected.is_empty() || selected.iter().any(|id| !all.contains(id)) {
            return Err("Select the fragment you want to move and attach.".into());
        }
        let moving = moving_ids(doc, selected);
        // Selection and deletion must not prune unresolved definitions in the moving fragment.
        if crate::generic::touches(doc, &moving) {
            return Err(crate::generic::JOIN_NOTICE.into());
        }
        let fragment = editing::selection(doc, &moving);
        if fragment.atoms.is_empty() {
            return Err("Select a molecule or atom to move and attach.".into());
        }
        let mut base = doc.clone();
        if mode != Connection::Coordinate {
            base.delete(&moving);
        }
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
                .find(|a| {
                    self.fragment.atom_visible(a.id)
                        && (mode != Connection::Coordinate || coordination_donor(a))
                })
                .map(|a| Anchor::Atom(a.id))
                .unwrap_or(Anchor::Auto)
        }
    }

    fn resolve_destination(
        &self,
        point: Point,
        radius: f32,
        anchor: Anchor,
        mode: Connection,
    ) -> Result<Destination, String> {
        let source_ids = match anchor {
            Anchor::Atom(id) if anchor.valid(&self.fragment) => vec![id],
            Anchor::Bond(a, b) if anchor.valid(&self.fragment) => vec![a, b],
            _ => {
                return Err("Choose an exact source atom or bond in the fragment preview.".into());
            }
        };
        if !point.x.is_finite() || !point.y.is_finite() || !radius.is_finite() || radius <= 0. {
            return Err("Invalid connection target geometry.".into());
        }
        // Prepared is public. Recheck the unpruned original dependencies and
        // the current fragment before either the pose or the Coordinate route.
        // The fragment keeps its direct check; generic::touches would add a component scan.
        if crate::generic::touches(&self.original, &self.moving)
            || crate::generic::intersects(&self.fragment, &self.fragment.all_ids())
        {
            return Err(crate::generic::JOIN_NOTICE.into());
        }
        if mode == Connection::Coordinate {
            let Anchor::Atom(donor) = anchor else {
                return Err("Choose a donor atom in the preview.".into());
            };
            let metal = self
                .original
                .nearest(point, radius)
                .ok_or("Point to a metal atom in the drawing.")?;
            if crate::generic::touches(&self.original, &[donor, metal]) {
                return Err(crate::generic::JOIN_NOTICE.into());
            }
            return Ok(Destination::Coordinate { donor, metal });
        }
        let (target, bond) =
            templates::attachment_target(&self.base, &self.fragment, point, radius);
        let bond = bond.and_then(|index| self.base.bonds.get(index));
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
        let target_ids: Vec<_> = target
            .into_iter()
            .chain(bond.into_iter().flat_map(|b| [b.a, b.b]))
            .collect();
        if crate::generic::touches(&self.base, &target_ids) {
            return Err(crate::generic::JOIN_NOTICE.into());
        }
        Ok(Destination::Attach {
            source_ids,
            target_ids,
        })
    }

    /// Generic-only preflight using the same destination admission as `place`,
    /// without posing a copy. Invalid anchors and geometry retain priority over
    /// generic definitions; ordinary destination errors do not trigger rejection.
    pub fn generic_rejection(
        &self,
        point: Point,
        radius: f32,
        anchor: Anchor,
        mode: Connection,
    ) -> bool {
        matches!(
            self.resolve_destination(point, radius, anchor, mode),
            Err(error) if error == crate::generic::JOIN_NOTICE
        )
    }

    pub fn place(
        &self,
        point: Point,
        direction: Option<Point>,
        radius: f32,
        anchor: Anchor,
        mode: Connection,
    ) -> Result<(Document, Vec<u64>), String> {
        self.place_outcome(point, direction, radius, anchor, mode)
            .map(|(placed, ids)| (placed.into_document(&self.original), ids))
    }

    /// Place with an explicit outcome for an added or already-present contact.
    pub fn place_outcome(
        &self,
        point: Point,
        direction: Option<Point>,
        radius: f32,
        anchor: Anchor,
        mode: Connection,
    ) -> Result<(Placed, Vec<u64>), String> {
        let (source_ids, target_ids) =
            match self.resolve_destination(point, radius, anchor, mode)? {
                Destination::Coordinate { donor, metal } => {
                    return Ok((
                        Placed::Coordinated(coordinate_atoms(&self.original, donor, metal)?),
                        vec![donor, metal],
                    ));
                }
                Destination::Attach {
                    source_ids,
                    target_ids,
                } => (source_ids, target_ids),
            };
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
        let host: HashSet<_> = self.base.object_ids().collect();
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
        let endpoint_atoms = placed.atoms.clone();
        let endpoint_mapping: HashMap<_, _> = endpoint_atoms
            .iter()
            .map(|atom| (atom.id, original_id(atom.id)))
            .collect();
        // Restore IDs of moved objects simultaneously. Only shared atoms disappear;
        // destination IDs, labels, remote stereo and unrelated objects retain identity.
        for a in &mut placed.atoms {
            a.id = original_id(a.id);
            for member in &mut a.centroid {
                *member = original_id(*member);
            }
            if let Some(stereo) = &mut a.stereo {
                for neighbor in &mut stereo.neighbors {
                    *neighbor = original_id(*neighbor);
                }
            }
        }
        for fill in &mut placed.ring_fills {
            for id in &mut fill.atoms {
                *id = original_id(*id);
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
        crate::arrow_anchors::remap_for_join(&mut placed, &endpoint_atoms, &endpoint_mapping);
        // Preparation deletes moving atoms only from a detached preview base.
        // An unselected arrow still belongs to the original document and must
        // follow the moved/shared target rather than lose its attachment.
        for arrow in &mut placed.arrows {
            if !self.moving.contains(&arrow.id)
                && let Some(original) = self.original.arrows.iter().find(|a| a.id == arrow.id)
            {
                arrow.start_anchor = original.start_anchor.clone();
                arrow.end_anchor = original.end_anchor.clone();
            }
        }
        let joined_mapping: HashMap<_, _> = self
            .original
            .atoms
            .iter()
            .map(|atom| (atom.id, merged.get(&atom.id).copied().unwrap_or(atom.id)))
            .collect();
        // Shared attachment sites cannot carry positioned marks. Other moved
        // marks retain their original host identity after temporary-ID restore.
        crate::arrow_anchors::remap_for_join(&mut placed, &self.original.atoms, &joined_mapping);
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
        // Selection can omit the reaction arrow, but its participant roles still
        // belong to the moved atoms. Restore them before checking the join.
        placed.reactions = self.original.reactions.clone();
        for reaction in &mut placed.reactions {
            for role in crate::reactions::Role::ALL {
                for participant in reaction.participants_mut(role) {
                    for id in &mut participant.atoms {
                        *id = merged.get(id).copied().unwrap_or(*id);
                    }
                }
            }
        }
        crate::reactions::reconcile(&mut placed)?;
        crate::arrow_anchors::reconcile(&mut placed);
        placed.validate()?;
        Ok((
            Placed::Joined(placed),
            ids.into_iter().map(original_id).collect(),
        ))
    }
}

#[cfg(test)]
mod endpoint_tests;
#[cfg(test)]
mod generic_tests;
