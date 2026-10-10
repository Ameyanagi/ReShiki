//! Drawing import policy, separate from the lossless MOL parser snapshot.
use super::{Imported, ReadError};
use crate::chemistry::{
    document, hydrogens, sanitize,
    stereo::{cip::label, perception},
};
use std::collections::{BTreeMap, HashMap};

impl Imported {
    /// Fold ordinary terminal H only after file stereo has been assigned.
    /// The shared graph operation adjusts winding and double-bond controls;
    /// kept indices remap coordinates, stable IDs and file annotations together.
    /// The raw parser result stays available and is never modified.
    pub(crate) fn for_display(&self) -> Result<Self, ReadError> {
        document::validate_molecule(&self.molecule)
            .map_err(|error| ReadError::Chemistry(error.to_string()))?;
        let state = &self.molecule.state;
        let n = state.graph.atoms.len();
        if self.annotations.dummy_labels.len() != n || self.annotations.attachment_points.len() != n
        {
            return Err(invalid("MOL display annotation dimensions changed"));
        }
        let mut adjacent = vec![Vec::new(); n];
        for (index, bond) in state.graph.bonds.iter().enumerate() {
            for (atom, other) in [(bond.a, bond.b), (bond.b, bond.a)] {
                adjacent
                    .get_mut(atom)
                    .ok_or_else(|| invalid("Missing MOL hydrogen neighbor"))?
                    .push((other, index));
            }
        }
        let ids: HashMap<_, _> = self
            .molecule
            .ids
            .iter()
            .enumerate()
            .map(|(index, &id)| (id, index))
            .collect();
        let mut protected = vec![false; n];
        for attachment in &self.annotations.attachments {
            for id in std::iter::once(&attachment.id).chain(&attachment.members) {
                let index = ids
                    .get(id)
                    .copied()
                    .ok_or_else(|| invalid("Missing MOL attachment identity"))?;
                *protected
                    .get_mut(index)
                    .ok_or_else(|| invalid("Missing MOL attachment atom"))? = true;
            }
        }
        let mut counts = vec![0usize; n];
        let mut eligible = false;
        for (index, atom) in state.graph.atoms.iter().enumerate() {
            let metadata = at(&state.metadata.atoms, index)?;
            let property = at(&state.properties.atoms, index)?;
            let edges = at(&adjacent, index)?;
            let ordinary = atom.atomic_number == 1
                && atom.isotope == 0
                && atom.charge == 0
                && atom.radical_electrons == 0
                && !metadata.map_present
                && !property.unknown
                && at(&self.annotations.attachment_points, index)?.is_none()
                && at(&self.annotations.dummy_labels, index)?.is_none()
                && !*at(&protected, index)?;
            let removable = if ordinary && let [(parent, bond)] = edges.as_slice() {
                let parent_atom = at(&state.graph.atoms, *parent)?;
                let bond_meta = at(&state.metadata.bonds, *bond)?;
                // A metal hydride or coordinate contact is not an ordinary
                // implicit-H substituent, even when it is terminal and neutral.
                matches!(parent_atom.atomic_number, 5..=9 | 14..=17 | 34..=35 | 53)
                    && at(&state.graph.bonds, *bond)?.order == 1
                    && !bond_meta.unknown_stereo
                    && bond_meta.stereo == 0
            } else {
                false
            };
            if removable {
                let &(parent, _) = edges
                    .first()
                    .ok_or_else(|| invalid("Missing removable MOL hydrogen bond"))?;
                let count = counts
                    .get_mut(parent)
                    .ok_or_else(|| invalid("Missing MOL hydrogen parent"))?;
                *count = count
                    .checked_add(1)
                    .ok_or_else(|| invalid("MOL hydrogen count overflow"))?;
                if *count + usize::from(at(&state.graph.atoms, parent)?.explicit_hydrogens)
                    > usize::from(u8::MAX)
                {
                    return Err(invalid("MOL hydrogen count exceeds its storage limit"));
                }
                eligible = true;
            } else if atom.atomic_number == 1 {
                *protected
                    .get_mut(index)
                    .ok_or_else(|| invalid("Missing protected MOL hydrogen"))? = true;
            }
        }
        if !eligible {
            return Ok(self.clone());
        }
        let removed = hydrogens::remove(&hydrogens::Input {
            graph: state.graph.clone(),
            metadata: state.metadata.clone(),
            directions: state.directions.clone(),
            unknown_atoms: state.properties.atoms.iter().map(|p| p.unknown).collect(),
            annotations: hydrogens::Annotations {
                protected_atoms: protected
                    .iter()
                    .enumerate()
                    .filter_map(|(index, &yes)| yes.then_some(index))
                    .collect(),
                substance_groups: Vec::new(),
            },
        })
        .map_err(|error| ReadError::Chemistry(error.to_string()))?;
        if removed.kept_atoms.len() == n {
            return Ok(self.clone());
        }
        let sanitized = sanitize::sanitize(&removed.graph, &removed.metadata, &removed.directions)?;
        let mut properties = perception::Properties::unspecified(&sanitized.graph);
        for (property, &unknown) in properties.atoms.iter_mut().zip(&removed.unknown_atoms) {
            property.unknown = unknown;
        }
        let folded = perception::perceive(
            &perception::State {
                graph: sanitized.graph,
                metadata: sanitized.metadata,
                directions: sanitized.directions,
                valences: sanitized.valences,
                conjugated: sanitized.conjugated,
                hybridizations: sanitized.hybridizations,
                rings: perception::RingCache {
                    kind: perception::RingKind::Symmetric,
                    atoms: sanitized.rings,
                },
                properties,
            },
            perception::Options {
                clean: true,
                force: true,
                flag_possible: true,
            },
        )
        .map_err(ReadError::Chemistry)?;
        // Verify composition/charge/radical inventory and full CIP descriptors
        // independently of the removal operation's neighbor-parity updates.
        if inventory(state)? != inventory(&folded)? {
            return Err(invalid("MOL hydrogen folding changed chemical composition"));
        }
        let before = label::assign(state, &Default::default())
            .map_err(|error| ReadError::Chemistry(error.to_string()))?
            .state;
        let after = label::assign(&folded, &Default::default())
            .map_err(|error| ReadError::Chemistry(error.to_string()))?
            .state;
        for (new, &old) in removed.kept_atoms.iter().enumerate() {
            if at(&before.properties.atoms, old)?.cip_code
                != at(&after.properties.atoms, new)?.cip_code
            {
                return Err(invalid("MOL hydrogen folding changed atom stereochemistry"));
            }
        }
        for (new, &old) in removed.kept_bonds.iter().enumerate() {
            if at(&before.properties.bond_codes, old)? != at(&after.properties.bond_codes, new)? {
                return Err(invalid("MOL hydrogen folding changed bond stereochemistry"));
            }
        }
        let mut display = self.clone();
        display.molecule.state = folded;
        display.molecule.ids = select(&self.molecule.ids, &removed.kept_atoms)?;
        display.molecule.positions = select(&self.molecule.positions, &removed.kept_atoms)?;
        display.annotations.attachment_points =
            select(&self.annotations.attachment_points, &removed.kept_atoms)?;
        display.annotations.dummy_labels =
            select(&self.annotations.dummy_labels, &removed.kept_atoms)?;
        if !self.annotations.chemdraw_directions.is_empty() {
            display.annotations.chemdraw_directions =
                select(&self.annotations.chemdraw_directions, &removed.kept_bonds)?;
        }
        document::validate_molecule(&display.molecule)
            .map_err(|error| ReadError::Chemistry(error.to_string()))?;
        Ok(display)
    }

    /// X/Y already use this single source-to-canvas factor. Retain Z with the
    /// same factor; do not fit, minimize, flatten or generate coordinates.
    pub(crate) fn restore_xyz(
        &self,
        document: &mut crate::document::Document,
    ) -> Result<(), document::Error> {
        if !self.annotations.is_3d
            && !self
                .molecule
                .positions
                .iter()
                .any(|position| position.z != 0.)
        {
            return Ok(());
        }
        if document.atoms.len() != self.molecule.ids.len() {
            return Err(document::Error::Drawing(
                "MOL XYZ dimensions changed".into(),
            ));
        }
        for ((atom, &id), position) in document
            .atoms
            .iter_mut()
            .zip(&self.molecule.ids)
            .zip(&self.molecule.positions)
        {
            if atom.id != id {
                return Err(document::Error::Drawing(
                    "MOL XYZ identities changed".into(),
                ));
            }
            atom.depth = (position.z * 28.) as f32;
        }
        // The molecular tags/controls were captured from the original XYZ.
        // A rotated or edge-on projection must not infer different chemistry.
        let mut unknown_doubles = std::collections::HashSet::new();
        let mut unknown_singles = std::collections::HashSet::new();
        for (bond, metadata) in self
            .molecule
            .state
            .graph
            .bonds
            .iter()
            .zip(&self.molecule.state.metadata.bonds)
        {
            if bond.order == 2 && metadata.stereo == 1 || bond.order == 1 && metadata.unknown_stereo
            {
                let a = self.molecule.ids.get(bond.a).copied().ok_or_else(|| {
                    document::Error::Drawing("Missing MOL double-bond identity".into())
                })?;
                let b = self.molecule.ids.get(bond.b).copied().ok_or_else(|| {
                    document::Error::Drawing("Missing MOL double-bond identity".into())
                })?;
                if bond.order == 2 {
                    unknown_doubles.insert((a.min(b), a.max(b)));
                } else {
                    unknown_singles.insert((a.min(b), a.max(b)));
                }
            }
        }
        for bond in &mut document.bonds {
            if bond.order == 1
                && unknown_singles.contains(&(bond.a.min(bond.b), bond.a.max(bond.b)))
            {
                bond.display = "wavy".into();
            }
            if bond.order == 2 {
                // Full CIP labels encode E/Z only. Preserve explicit ANY in
                // native chemical controls before projection disables the
                // otherwise usable wavy-paint inference.
                if unknown_doubles.contains(&(bond.a.min(bond.b), bond.a.max(bond.b))) {
                    bond.stereo = Some("any".into());
                }
                bond.stereo_authoritative = true;
            }
            // A wavy single bond is an explicit chemical unknown. Its
            // direction must remain available to molecular preparation;
            // unlike wedge paint it cannot infer a rotated configuration.
            if matches!(bond.order, 1 | 2 | 4) && !(bond.order == 1 && bond.display == "wavy") {
                bond.projection = true;
            }
        }
        Ok(())
    }
}

fn invalid(message: &str) -> ReadError {
    ReadError::Chemistry(message.into())
}

fn at<T>(items: &[T], index: usize) -> Result<&T, ReadError> {
    items
        .get(index)
        .ok_or_else(|| invalid("Missing MOL display graph item"))
}

fn select<T: Clone>(items: &[T], kept: &[usize]) -> Result<Vec<T>, ReadError> {
    kept.iter()
        .map(|&index| at(items, index).cloned())
        .collect()
}

type Inventory = BTreeMap<(u8, u16, i8, u8), u64>;

fn inventory(state: &perception::State) -> Result<Inventory, ReadError> {
    let mut result = BTreeMap::new();
    for (index, atom) in state.graph.atoms.iter().enumerate() {
        *result
            .entry((
                atom.atomic_number,
                atom.isotope,
                atom.charge,
                atom.radical_electrons,
            ))
            .or_default() += 1;
        let count = u64::from(atom.explicit_hydrogens)
            + u64::from(at(&state.valences, index)?.implicit_hydrogens);
        *result.entry((1, 0, 0, 0)).or_default() += count;
    }
    result.retain(|_, count| *count != 0);
    Ok(result)
}

#[cfg(test)]
mod tests;
