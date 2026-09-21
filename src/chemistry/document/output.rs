//! Convert verified chemistry back to an editable drawing. Full CIP labeling
//! is an explicit dependency; legacy CIP codes must never become display labels.
use super::{Error, Molecule, at, invalid};
use crate::{
    chemistry::{
        ELEMENTS, RDKIT_VERSION, kekulize, ranking,
        stereo::{perception::RingKind, wedging},
    },
    document::{AtomStereo, Document, Point},
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[cfg(test)]
mod tests;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Labels {
    pub rdkit_version: String,
    pub atoms: Vec<Option<String>>,
    pub bonds: Vec<BondLabel>,
}

/// Full CIP labeling may normalize E/Z to cis/trans and choose new controls.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BondLabel {
    pub code: Option<String>,
    pub stereo: u8,
    pub stereo_atoms: Vec<usize>,
}

/// Detached drawing plus the exact molecular state that needs full CIP labels.
/// Finishing consumes the draft; a partial result never changes the input.
#[derive(Debug)]
pub struct Drawing {
    molecule: Molecule,
    document: Document,
    bond_indices: Vec<usize>,
}
impl Drawing {
    pub fn molecule(&self) -> &Molecule {
        &self.molecule
    }
    pub fn finish(mut self, labels: Labels) -> Result<Document, Error> {
        if labels.rdkit_version != RDKIT_VERSION
            || labels.atoms.len() != self.document.atoms.len()
            || labels.bonds.len() != self.document.bonds.len()
        {
            return Err(invalid("Drawing label version or dimensions changed"));
        }
        for (atom, label) in self.document.atoms.iter_mut().zip(labels.atoms) {
            atom.cip_label = label;
        }
        let graph = &self.molecule.state.graph;
        let edges: HashSet<_> = graph
            .bonds
            .iter()
            .flat_map(|b| [(b.a, b.b), (b.b, b.a)])
            .collect();
        for (bond, index) in self.document.bonds.iter_mut().zip(&self.bond_indices) {
            let label = at(&labels.bonds, *index)?;
            let chemical = at(&graph.bonds, *index)?;
            if label.stereo > 5
                || !matches!(label.stereo_atoms.len(), 0 | 2)
                || label.stereo_atoms.len() == 2
                    && (!edges.contains(&(chemical.a, *at(&label.stereo_atoms, 0)?))
                        || !edges.contains(&(chemical.b, *at(&label.stereo_atoms, 1)?)))
            {
                return Err(invalid("Invalid labeled bond stereochemistry"));
            }
            bond.cip_label = label.code.clone();
            bond.stereo = stereo_name(label.stereo);
            bond.stereo_atoms = label
                .stereo_atoms
                .iter()
                .map(|&i| at(&self.molecule.ids, i).copied())
                .collect::<Result<Vec<_>, _>>()?;
            if bond.a != *at(&self.molecule.ids, chemical.a)? {
                bond.stereo_atoms.reverse();
            }
        }
        self.document.validate().map_err(Error::Drawing)?;
        Ok(self.document)
    }
}
fn pair(a: u64, b: u64) -> (u64, u64) {
    (a.min(b), a.max(b))
}
fn stereo_name(code: u8) -> Option<String> {
    match code {
        2 => Some("z"),
        3 => Some("e"),
        4 => Some("cis"),
        5 => Some("trans"),
        _ => None,
    }
    .map(String::from)
}

/// Generate Kekulé and wedge bonds, then reconstruct drawing-owned styles and
/// stable IDs. This path updates an existing drawing without moving its atoms.
pub fn for_drawing(molecule: &Molecule, base: &Document) -> Result<Drawing, Error> {
    molecule.state.graph.validate().map_err(Error::Drawing)?;
    base.validate().map_err(Error::Drawing)?;
    let state = &molecule.state;
    let (n, e) = (state.graph.atoms.len(), state.graph.bonds.len());
    let previous: HashMap<_, _> = base.atoms.iter().map(|a| (a.id, a)).collect();
    if molecule.rdkit_version != RDKIT_VERSION
        || molecule.ids.len() != n
        || molecule.positions.len() != n
        || previous.len() != n
        || base.bonds.len() != e
        || molecule.ids.iter().collect::<HashSet<_>>().len() != n
        || molecule.ids.iter().any(|id| !previous.contains_key(id))
        || state.hybridizations.len() != n
        || state.conjugated.len() != e
        || state.properties.atoms.len() != n
        || state.properties.bond_codes.len() != e
        || state.rings.kind != RingKind::Symmetric
    {
        return Err(invalid("Drawing molecule dimensions or identities changed"));
    }
    if state.graph.atoms.iter().any(|a| a.radical_electrons > 2)
        || state.metadata.atoms.iter().any(|a| a.chiral_tag > 2)
        || state.metadata.bonds.iter().any(|b| b.stereo > 5)
        || !state.metadata.groups.is_empty()
    {
        return Err(invalid(
            "Unsupported drawing stereochemistry or radical count",
        ));
    }
    let kekule = kekule(molecule)?;
    let wedged = wedging::wedge_molecule(
        &wedging::WedgeState {
            graph: kekule.assignment.graph,
            metadata: state.metadata.clone(),
            directions: kekule.assignment.directions,
            rings: state.rings.clone(),
        },
        &wedging::WedgeProperties {
            valences: kekule.cache.clone(),
            attachment_points: vec![false; n],
        },
        Some(&wedging::Conformer {
            positions: molecule.positions.clone(),
            is_3d: false,
        }),
        false,
    )
    .map_err(Error::WedgeAssignment)?;
    let mut work = molecule.clone();
    work.state.graph = wedged.graph;
    work.state.metadata = wedged.metadata;
    work.state.directions = wedged.directions;
    work.state.rings = wedged.rings;
    work.state.valences = kekule.cache;
    reconstruct(work, molecule, base, &previous)
}

/// Shared canonical bond assignment for drawing reconstruction and MOL output.
pub(crate) fn kekule(molecule: &Molecule) -> Result<kekulize::Attempt, Error> {
    let state = &molecule.state;
    let cache = state
        .graph
        .refresh_implicit(&state.valences)
        .map_err(Error::BondAssignment)?;
    let ranks = if state.graph.atoms.iter().any(|a| a.aromatic)
        || state.graph.bonds.iter().any(|b| b.aromatic)
    {
        Some(
            ranking::rank_cached(
                &state.graph,
                &state.rings.atoms,
                &state.metadata,
                ranking::Options {
                    fragment: true,
                    ..Default::default()
                },
                &cache,
            )
            .map_err(Error::BondAssignment)?,
        )
    } else {
        None
    };
    kekulize::assign_cached(
        &state.graph,
        &state.rings.atoms,
        &state.directions,
        kekulize::Options {
            ranks: ranks.as_deref(),
            ..Default::default()
        },
        &state.valences,
    )
    .map_err(Error::BondAssignment)
}

fn reconstruct(
    mut work: Molecule,
    molecule: &Molecule,
    base: &Document,
    previous: &HashMap<u64, &crate::document::Atom>,
) -> Result<Drawing, Error> {
    let state = &molecule.state;
    let (n, e) = (state.graph.atoms.len(), state.graph.bonds.len());
    for properties in &mut work.state.properties.atoms {
        properties.cip_code = None;
    }
    work.state.properties.bond_codes.fill(None);
    let mut adjacent = vec![Vec::new(); n];
    let mut bonds_by_pair = HashMap::new();
    for (i, bond) in work.state.graph.bonds.iter().enumerate() {
        let a = *at(&work.ids, bond.a)?;
        let b = *at(&work.ids, bond.b)?;
        bonds_by_pair.insert(pair(a, b), i);
        for (index, neighbor) in [(bond.a, b), (bond.b, a)] {
            adjacent
                .get_mut(index)
                .ok_or_else(|| invalid("Missing drawing neighbors"))?
                .push(neighbor);
        }
    }
    let circular: HashSet<_> = base
        .bonds
        .iter()
        .filter(|b| b.order == 4)
        .flat_map(|b| [b.a, b.b])
        .collect();
    let atoms = work
        .state
        .graph
        .atoms
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let id = *at(&work.ids, i)?;
            let mut atom = (*previous
                .get(&id)
                .ok_or_else(|| invalid("Missing original atom"))?)
            .clone();
            let metadata = at(&work.state.metadata.atoms, i)?;
            let p = at(&work.positions, i)?;
            atom.element = at(ELEMENTS, usize::from(a.atomic_number))?.symbol.into();
            atom.position = Point {
                x: (p.x * 28.) as f32,
                y: (-p.y * 28.) as f32,
            };
            atom.charge = i32::from(a.charge);
            atom.isotope = u32::from(a.isotope);
            atom.radical_electrons = a.radical_electrons;
            atom.explicit_h = u32::from(if circular.contains(&id) {
                at(&state.graph.atoms, i)?.explicit_hydrogens
            } else {
                a.explicit_hydrogens
            });
            atom.no_implicit = a.no_implicit;
            atom.aromatic = a.aromatic;
            atom.map_num =
                u32::try_from(metadata.map_number).map_err(|_| invalid("Negative atom map"))?;
            atom.stereo = match metadata.chiral_tag {
                0 => None,
                tag => Some(AtomStereo {
                    winding: if tag == 1 { "cw" } else { "ccw" }.into(),
                    neighbors: at(&adjacent, i)?.clone(),
                }),
            };
            atom.label_h = u32::from(a.explicit_hydrogens)
                .checked_add(at(&work.state.valences, i)?.implicit_hydrogens)
                .ok_or_else(|| invalid("Drawing hydrogen count overflow"))?;
            atom.cip_label = None;
            Ok(atom)
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let mut bond_indices = Vec::with_capacity(e);
    let bonds = base
        .bonds
        .iter()
        .map(|old| {
            let i = *bonds_by_pair
                .get(&pair(old.a, old.b))
                .ok_or_else(|| invalid("Drawing bond endpoints changed"))?;
            bond_indices.push(i);
            let b = at(&work.state.graph.bonds, i)?;
            let meta = at(&work.state.metadata.bonds, i)?;
            let mut bond = old.clone();
            bond.order = if old.order == 4 && at(&state.graph.bonds, i)?.aromatic {
                4
            } else {
                b.order
            };
            bond.stereo = match meta.stereo {
                2 => Some("z"),
                3 => Some("e"),
                4 => Some("cis"),
                5 => Some("trans"),
                _ => None,
            }
            .map(String::from);
            bond.stereo_atoms = meta
                .stereo_atoms
                .iter()
                .map(|&i| at(&work.ids, i).copied())
                .collect::<Result<Vec<_>, _>>()?;
            if old.a != *at(&work.ids, b.a)? {
                bond.stereo_atoms.reverse();
            }
            bond.cip_label = None;
            Ok(bond)
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let document = Document {
        version: 15,
        atoms,
        bonds,
        ..base.clone()
    };
    document.validate().map_err(Error::Drawing)?;
    Ok(Drawing {
        molecule: work,
        document,
        bond_indices,
    })
}
