//! Convert verified chemistry back to an editable drawing. Full CIP labeling
//! is an explicit dependency; legacy CIP codes must never become display labels.
use super::{Error, Molecule, at, invalid};
use crate::{
    chemistry::{
        ELEMENTS, RDKIT_VERSION, kekulize, ranking,
        stereo::{cip::label, perception::RingKind, wedging},
    },
    document::{Atom, AtomStereo, Bond, Document, Point},
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
#[derive(Clone, Debug)]
pub struct Drawing {
    molecule: Molecule,
    document: Document,
    bond_indices: Vec<usize>,
}
impl Drawing {
    pub(crate) fn with_attachments(
        mut self,
        attachments: &[crate::attachments::Attachment],
    ) -> Result<Self, Error> {
        for attachment in attachments {
            let atom = self
                .document
                .atom_mut(attachment.id)
                .ok_or_else(|| invalid("Missing attachment atom"))?;
            if atom.attachment.is_some()
                && (atom.attachment != Some(attachment.kind) || atom.centroid != attachment.members)
            {
                return Err(invalid("Conflicting attachment target lists"));
            }
            atom.attachment = Some(attachment.kind);
            atom.element = "*".into();
            atom.centroid.clone_from(&attachment.members);
            atom.no_implicit = true;
            atom.label_h = 0;
        }
        self.document.validate().map_err(invalid)?;
        Ok(self)
    }
    pub fn molecule(&self) -> &Molecule {
        &self.molecule
    }
    /// Compute display labels from the prepared drawing. Clear legacy or stale
    /// codes first; only the complete CIP pass may supply drawing labels.
    pub fn labels(&self) -> Result<Labels, Error> {
        let mut state = self.molecule.state.clone();
        for atom in &mut state.properties.atoms {
            atom.cip_code = None;
        }
        state.properties.bond_codes.fill(None);
        let state = label::assign(&state, &Default::default())?.state;
        Ok(Labels {
            rdkit_version: RDKIT_VERSION.into(),
            atoms: state
                .properties
                .atoms
                .into_iter()
                .map(|atom| atom.cip_code)
                .collect(),
            bonds: state
                .metadata
                .bonds
                .into_iter()
                .zip(state.properties.bond_codes)
                .map(|(bond, code)| BondLabel {
                    code,
                    stereo: bond.stereo,
                    stereo_atoms: bond.stereo_atoms,
                })
                .collect(),
        })
    }
    /// Reaction layout uses full-precision file coordinates until every row is
    /// placed. Replace canvas positions before validating the finished drawing;
    /// the conformer used for stereochemistry remains unchanged.
    pub(crate) fn finish_at(
        mut self,
        labels: Labels,
        positions: &[Point],
    ) -> Result<Document, Error> {
        if positions.len() != self.document.atoms.len() {
            return Err(invalid("Reaction drawing position dimensions changed"));
        }
        for (atom, position) in self.document.atoms.iter_mut().zip(positions) {
            atom.position = *position;
        }
        self.finish(labels)
    }
    pub fn finish(self, labels: Labels) -> Result<Document, Error> {
        self.finish_with(labels, |_| Ok(()))
    }
    /// Restore import-owned appearance after CIP labels and before final
    /// validation. The callback only receives a detached draft; an error
    /// publishes no partial drawing and leaves the caller's source unchanged.
    pub(crate) fn finish_with<E: From<Error>>(
        self,
        labels: Labels,
        restore: impl FnOnce(&mut Document) -> Result<(), E>,
    ) -> Result<Document, E> {
        let mut document = self.labeled(labels)?;
        restore(&mut document)?;
        document.validate().map_err(Error::Drawing)?;
        Ok(document)
    }
    fn labeled(mut self, labels: Labels) -> Result<Document, Error> {
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
    validate_molecule(molecule)?;
    base.validate().map_err(Error::Drawing)?;
    let state = &molecule.state;
    let (n, e) = (state.graph.atoms.len(), state.graph.bonds.len());
    let previous: HashMap<_, _> = base.atoms.iter().map(|a| (a.id, a)).collect();
    if previous.len() != n
        || base.bonds.len() != e
        || molecule.ids.iter().any(|id| !previous.contains_key(id))
    {
        return Err(invalid("Drawing molecule dimensions or identities changed"));
    }
    let work = wedge(molecule, false, vec![false; n])?;
    let drawing = reconstruct(work, molecule, Some(base), &previous, None)?;
    drawing.document.validate().map_err(Error::Drawing)?;
    Ok(drawing)
}

/// Construct a new drawing from an imported molecular state. File coordinates
/// determine the generated wedges; no previous drawing styles override those
/// directions or endpoints. Unexpanded file attachment markers are not the
/// generated attachment atoms that receive different wedge priorities.
pub fn for_import(
    molecule: &Molecule,
    is_3d: bool,
    dummy_labels: &[Option<String>],
) -> Result<Drawing, Error> {
    for_import_with_attachments(
        molecule,
        is_3d,
        dummy_labels,
        vec![false; molecule.ids.len()],
    )
}

pub(crate) fn for_import_with_attachments(
    molecule: &Molecule,
    is_3d: bool,
    dummy_labels: &[Option<String>],
    attachments: Vec<bool>,
) -> Result<Drawing, Error> {
    validate_molecule(molecule)?;
    if dummy_labels.len() != molecule.state.graph.atoms.len() {
        return Err(invalid("Imported label dimensions changed"));
    }
    let work = wedge(molecule, is_3d, attachments)?;
    let drawing = reconstruct(work, molecule, None, &HashMap::new(), Some(dummy_labels))?;
    drawing.document.validate().map_err(Error::Drawing)?;
    Ok(drawing)
}

/// CDXML restores the native bond appearance after full CIP reconstruction.
/// Temporary order-zero bonds have plain appearance and cannot be validated as
/// editable bonds yet. Only finish/finish_with may publish this detached draft.
pub(crate) fn for_import_scene(molecule: &Molecule, is_3d: bool) -> Result<Drawing, Error> {
    validate_molecule(molecule)?;
    let work = wedge(molecule, is_3d, vec![false; molecule.ids.len()])?;
    reconstruct(work, molecule, None, &HashMap::new(), None)
}

pub(crate) fn validate_molecule(molecule: &Molecule) -> Result<(), Error> {
    let state = &molecule.state;
    state.graph.validate().map_err(Error::Drawing)?;
    state
        .metadata
        .validate(&state.graph)
        .map_err(Error::Drawing)?;
    let (n, e) = (state.graph.atoms.len(), state.graph.bonds.len());
    if molecule.rdkit_version != RDKIT_VERSION
        || molecule.ids.len() != n
        || molecule.positions.len() != n
        || molecule.ids.iter().collect::<HashSet<_>>().len() != n
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
    Ok(())
}

fn wedge(
    molecule: &Molecule,
    is_3d: bool,
    attachment_points: Vec<bool>,
) -> Result<Molecule, Error> {
    let state = &molecule.state;
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
            attachment_points,
        },
        Some(&wedging::Conformer {
            positions: molecule.positions.clone(),
            is_3d,
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
    Ok(work)
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
    base: Option<&Document>,
    previous: &HashMap<u64, &crate::document::Atom>,
    dummy_labels: Option<&[Option<String>]>,
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
        .into_iter()
        .flat_map(|d| &d.bonds)
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
            let metadata = at(&work.state.metadata.atoms, i)?;
            let p = at(&work.positions, i)?;
            let old = previous.get(&id).copied();
            let element = if a.atomic_number == 0 {
                dummy_labels
                    .and_then(|labels| labels.get(i))
                    .and_then(Option::as_deref)
            } else {
                None
            }
            .unwrap_or(at(ELEMENTS, usize::from(a.atomic_number))?.symbol);
            let mut atom = Atom {
                depth: 0.,
                centroid: vec![],
                attachment: None,
                id,
                element: element.into(),
                position: Point {
                    x: (p.x * 28.) as f32,
                    y: (-p.y * 28.) as f32,
                },
                charge: i32::from(a.charge),
                isotope: u32::from(a.isotope),
                radical_electrons: a.radical_electrons,
                explicit_h: u32::from(a.explicit_hydrogens),
                no_implicit: a.no_implicit,
                aromatic: a.aromatic,
                map_num: u32::try_from(metadata.map_number)
                    .map_err(|_| invalid("Negative atom map"))?,
                stereo: None,
                label_h: 0,
                cip_label: None,
                display: old.map(|a| a.display.clone()).unwrap_or_default(),
                marks: old.map(|a| a.marks.clone()).unwrap_or_default(),
                text_style: old.and_then(|a| a.text_style.clone()),
            };
            atom.explicit_h = u32::from(if circular.contains(&id) {
                at(&state.graph.atoms, i)?.explicit_hydrogens
            } else {
                a.explicit_hydrogens
            });
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
    let ordered = if let Some(base) = base {
        base.bonds
            .iter()
            .map(|old| {
                let index = *bonds_by_pair
                    .get(&pair(old.a, old.b))
                    .ok_or_else(|| invalid("Drawing bond endpoints changed"))?;
                Ok((index, Some(old)))
            })
            .collect::<Result<Vec<_>, Error>>()?
    } else {
        (0..e).map(|i| (i, None)).collect()
    };
    let mut bond_indices = Vec::with_capacity(e);
    let bonds = ordered
        .into_iter()
        .map(|(i, old)| {
            bond_indices.push(i);
            let b = at(&work.state.graph.bonds, i)?;
            let meta = at(&work.state.metadata.bonds, i)?;
            let mut bond = Bond {
                ring_arc: false,
                projection: false,
                a: *at(&work.ids, b.a)?,
                b: *at(&work.ids, b.b)?,
                order: b.order,
                display: if meta.stereo == 1 {
                    "wavy"
                } else {
                    match at(&work.state.directions, i)? {
                        crate::chemistry::kekulize::Direction::Wedge => "wedge",
                        crate::chemistry::kekulize::Direction::Hash => "hash",
                        crate::chemistry::kekulize::Direction::Unknown => "wavy",
                        _ => "plain",
                    }
                }
                .into(),
                stereo: None,
                stereo_atoms: Vec::new(),
                cip_label: None,
                double_position: Default::default(),
                secondary_display: None,
                color: [0; 3],
                indicator: Default::default(),
                z_order: 0,
            };
            if let Some(old) = old {
                bond = old.clone();
            }
            bond.order = if old.is_some_and(|b| b.order == 4) && at(&state.graph.bonds, i)?.aromatic
            {
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
            if bond.a != *at(&work.ids, b.a)? {
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
        ..base.cloned().unwrap_or_default()
    };
    Ok(Drawing {
        molecule: work,
        document,
        bond_indices,
    })
}
