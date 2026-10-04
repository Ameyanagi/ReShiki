//! Detached molecular geometry, stable drawing identities and isolated solving.
//! The editor's graph is never reconstructed from the native force field.
mod client;
mod view;
mod wire;
pub mod worker;
pub use crate::chemistry::stereo::Point3;
use crate::{
    chemistry::document::{self as molecular, Molecule},
    document::{AtomStereo, Document},
};
pub use client::{Client, Limits};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
pub use view::{Rotation, ViewFrame};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Selection(&'static str),
    #[error("3D geometry does not support {0}")]
    Unsupported(&'static str),
    #[error("Invalid 3D coordinates: {0}")]
    Coordinates(&'static str),
    #[error("3D geometry: {0}")]
    Invalid(String),
    #[error(transparent)]
    Chemistry(#[from] molecular::Error),
    #[error("3D worker I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("3D worker protocol: {0}")]
    Protocol(String),
    #[error("3D worker {0} exceeds its byte limit")]
    Limit(&'static str),
    #[error("3D operation exceeded its time limit")]
    Timeout,
    #[error("3D worker exited with code {code:?}: {diagnostic}")]
    Exit {
        code: Option<i32>,
        diagnostic: String,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ForceField {
    Mmff94,
    #[default]
    Mmff94s,
    Uff,
}
pub type Field = ForceField;
impl ForceField {
    pub fn name(self) -> &'static str {
        match self {
            Self::Mmff94 => "MMFF94",
            Self::Mmff94s => "MMFF94s",
            Self::Uff => "UFF",
        }
    }
    fn native(self) -> reshiki_geometry::ForceField {
        match self {
            Self::Mmff94 => reshiki_geometry::ForceField::MMFF94,
            Self::Mmff94s => reshiki_geometry::ForceField::MMFF94s,
            Self::Uff => reshiki_geometry::ForceField::UFF,
        }
    }
    fn from_native(field: reshiki_geometry::ForceField) -> Self {
        match field {
            reshiki_geometry::ForceField::MMFF94 => Self::Mmff94,
            reshiki_geometry::ForceField::MMFF94s => Self::Mmff94s,
            reshiki_geometry::ForceField::UFF => Self::Uff,
        }
    }
}
impl std::fmt::Display for ForceField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

#[derive(Clone, Debug)]
pub struct Conformer {
    /// Physical angstrom XYZ. Original drawing atoms precede temporary H atoms.
    pub positions: Vec<Point3>,
    pub original_atom_count: usize,
    /// One original atom parent for each appended temporary hydrogen.
    pub hydrogen_parents: Vec<usize>,
}
impl Conformer {
    pub fn validate(&self, originals: usize) -> Result<(), Error> {
        if self.original_atom_count != originals
            || self.positions.len() < originals
            || self.positions.len() > reshiki_geometry::MAX_COORDINATES
            || self.hydrogen_parents.len() != self.positions.len() - originals
            || self.hydrogen_parents.iter().any(|&i| i >= originals)
            || self.positions.iter().any(|p| !valid_point(*p))
        {
            return Err(Error::Coordinates(
                "Conformer dimensions, hydrogen mapping or numeric values changed",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Pin {
    pub atom: usize,
    pub position: Point3,
}

#[derive(Clone, Debug)]
pub struct Optimized {
    pub conformer: Conformer,
    pub initial_energy: f64,
    pub energy: f64,
    pub gradient: Option<Vec<Point3>>,
    pub converged: bool,
    pub iterations: u32,
    pub force_field: ForceField,
    pub diagnostics: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Prepared {
    source: Document,
    materialized: Document,
    molecule: Molecule,
    ids: Vec<u64>,
    drawing_positions: Vec<Point3>,
    units: f64,
}
impl Prepared {
    /// Select one full molecular component. An empty selection is accepted only
    /// when the drawing contains exactly one chemical component.
    pub fn new(source: &Document, selection: &[u64]) -> Result<Self, Error> {
        source.validate().map_err(Error::Invalid)?;
        let selected = component(source, selection)?;
        if selected.len() > reshiki_geometry::MAX_ATOMS {
            return Err(Error::Unsupported("molecules larger than 512 atoms"));
        }
        if source
            .atoms
            .iter()
            .any(|a| !a.centroid.is_empty() && a.centroid.iter().any(|id| selected.contains(id)))
        {
            return Err(Error::Unsupported(
                "ring-centre or multi-centre attachment drawings",
            ));
        }
        let mut part = source.clone();
        part.atoms.retain(|a| selected.contains(&a.id));
        part.bonds
            .retain(|b| selected.contains(&b.a) && selected.contains(&b.b));
        part.annotations.clear();
        part.arrows.clear();
        part.graphics.clear();
        part.groups.clear();
        part.reactions.clear();
        part.ring_fills.clear();
        part.abbreviations
            .retain(|a| a.members.iter().all(|id| selected.contains(id)));
        for atom in &part.atoms {
            if !atom.centroid.is_empty() || atom.attachment.is_some() {
                return Err(Error::Unsupported("multi-centre or variable attachments"));
            }
            if atom.element == "*" || atom.display.variable.is_some() {
                return Err(Error::Unsupported("wildcard or query atoms"));
            }
            if atom.radical_electrons != 0 {
                return Err(Error::Unsupported("radicals"));
            }
        }
        if part.bonds.iter().any(|b| !matches!(b.order, 1..=4)) {
            return Err(Error::Unsupported(
                "hydrogen, coordination, partial or quadruple bonds",
            ));
        }
        let molecule = molecular::prepare(&part)?;
        if molecule.state.graph.bonds.len() > reshiki_geometry::MAX_BONDS {
            return Err(Error::Unsupported("molecules larger than 2048 bonds"));
        }
        for atom in &molecule.state.graph.atoms {
            if ![1, 5, 6, 7, 8, 9, 14, 15, 16, 17, 33, 34, 35, 53].contains(&atom.atomic_number) {
                return Err(Error::Unsupported(
                    "elements outside H/B/C/N/O/F/Si/P/S/Cl/As/Se/Br/I",
                ));
            }
            if atom.radical_electrons != 0 {
                return Err(Error::Unsupported("radicals or incomplete valence"));
            }
            if !(-8..=8).contains(&atom.charge) || atom.isotope > 500 || atom.explicit_hydrogens > 8
            {
                return Err(Error::Unsupported("this charge, isotope or hydrogen count"));
            }
        }
        if !molecule.state.metadata.groups.is_empty()
            || molecule
                .state
                .metadata
                .atoms
                .iter()
                .any(|a| a.chiral_tag > 2)
            || molecule.state.metadata.bonds.iter().any(|b| b.stereo > 5)
        {
            return Err(Error::Unsupported(
                "enhanced or non-tetrahedral stereochemistry",
            ));
        }
        let ids = molecule.ids.clone();
        let units = f64::from(source.drawing_style.bond_length_world) / 1.5;
        if !units.is_finite() || units <= 0. {
            return Err(Error::Coordinates("Invalid drawing scale"));
        }
        let drawing_positions = ids
            .iter()
            .map(|id| {
                let a = source
                    .atom(*id)
                    .ok_or(Error::Coordinates("Missing source atom"))?;
                Ok(Point3 {
                    x: f64::from(a.position.x) / units,
                    y: -f64::from(a.position.y) / units,
                    z: f64::from(a.depth) / units,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let mut materialized = source.clone();
        materialize_stereo(&molecule, &mut materialized)?;
        materialized.validate().map_err(Error::Invalid)?;
        let prepared = Self {
            source: source.clone(),
            materialized,
            molecule,
            ids,
            drawing_positions,
            units,
        };
        prepared
            .native_request(
                ForceField::default(),
                reshiki_geometry::Operation::Generate,
                None,
                &[],
                500,
            )?
            .validate()
            .map_err(Error::Invalid)?;
        Ok(prepared)
    }
    pub fn ids(&self) -> &[u64] {
        &self.ids
    }
    pub fn index(&self, id: u64) -> Option<usize> {
        self.ids.iter().position(|a| *a == id)
    }
    pub fn source(&self) -> &Document {
        &self.source
    }
    pub fn molecule(&self) -> &Molecule {
        &self.molecule
    }
    pub fn view_frame(&self, conformer: &Conformer) -> Result<ViewFrame, Error> {
        conformer.validate(self.ids.len())?;
        ViewFrame::fitted(
            conformer
                .positions
                .get(..self.ids.len())
                .ok_or(Error::Coordinates("Missing original positions"))?,
            &self.drawing_positions,
            self.units,
        )
    }
    /// Produce a detached drawing; topology, identities and unrelated artwork
    /// are copied exactly, and temporary force-field H atoms never enter it.
    pub fn document(&self, conformer: &Conformer, frame: &ViewFrame) -> Result<Document, Error> {
        conformer.validate(self.ids.len())?;
        frame.validate()?;
        let mut document = self.materialized.clone();
        for (id, p) in self.ids.iter().zip(&conformer.positions) {
            let (xy, z) = frame.project(*p);
            if !xy.x.is_finite() || !xy.y.is_finite() || !z.is_finite() || z.abs() > 1_000_000. {
                return Err(Error::Coordinates(
                    "Projected coordinates exceed the drawing range",
                ));
            }
            let atom = document
                .atom_mut(*id)
                .ok_or(Error::Coordinates("Missing drawing atom"))?;
            atom.position = xy;
            atom.depth = z;
        }
        document.validate().map_err(Error::Invalid)?;
        Ok(document)
    }
    pub(crate) fn native_request(
        &self,
        field: ForceField,
        operation: reshiki_geometry::Operation,
        conformer: Option<&Conformer>,
        pins: &[Pin],
        iterations: u32,
    ) -> Result<reshiki_geometry::Request, Error> {
        let graph = &self.molecule.state.graph;
        let atoms = graph
            .atoms
            .iter()
            .zip(&self.molecule.state.metadata.atoms)
            .map(|(a, m)| reshiki_geometry::AtomInput {
                atomic_number: u32::from(a.atomic_number),
                isotope: u32::from(a.isotope),
                charge: i32::from(a.charge),
                explicit_h: u32::from(a.explicit_hydrogens),
                no_implicit: a.no_implicit,
                aromatic: a.aromatic,
                radical: u32::from(a.radical_electrons),
                chiral_tag: u32::from(m.chiral_tag),
            })
            .collect();
        // The prepared graph retains drawing bond insertion order. Therefore
        // its chiral tag is already relative to the exact native neighbor order.
        let bonds = graph
            .bonds
            .iter()
            .zip(&self.molecule.state.metadata.bonds)
            .map(|(b, m)| {
                let stereo_atoms = match m.stereo_atoms.as_slice() {
                    [] => None,
                    [a, c] => Some([*a, *c]),
                    _ => return Err(Error::Coordinates("Invalid stereo reference dimensions")),
                };
                Ok(reshiki_geometry::BondInput {
                    a: b.a,
                    b: b.b,
                    order: if b.aromatic { 12 } else { u32::from(b.order) },
                    aromatic: b.aromatic,
                    stereo: u32::from(m.stereo),
                    stereo_atoms,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let mut coordinates = if let Some(c) = conformer {
            c.validate(self.ids.len())?;
            c.positions.iter().map(|p| [p.x, p.y, p.z]).collect()
        } else {
            Vec::new()
        };
        let mut fixed = HashSet::new();
        for pin in pins {
            if !valid_point(pin.position) || !fixed.insert(pin.atom) {
                return Err(Error::Coordinates("Invalid or duplicate pinned atom"));
            }
            let p = coordinates
                .get_mut(pin.atom)
                .ok_or(Error::Coordinates("Pinned atom is absent"))?;
            *p = [pin.position.x, pin.position.y, pin.position.z];
        }
        let mut fixed_atoms: Vec<_> = fixed.into_iter().collect();
        fixed_atoms.sort_unstable();
        let request = reshiki_geometry::Request {
            atoms,
            bonds,
            field: field.native(),
            operation,
            coordinates,
            fixed_atoms,
            conformers: 8,
            seed: 0x52534b,
            max_iterations: iterations,
        };
        request.validate().map_err(Error::Invalid)?;
        Ok(request)
    }
}

pub(super) fn valid_point(p: Point3) -> bool {
    [p.x, p.y, p.z]
        .iter()
        .all(|v| v.is_finite() && v.abs() <= 1_000_000.)
}

fn component(source: &Document, selection: &[u64]) -> Result<HashSet<u64>, Error> {
    let atom_ids: HashSet<_> = source.atoms.iter().map(|a| a.id).collect();
    if atom_ids.is_empty() {
        return Err(Error::Selection("Draw a molecule first"));
    }
    let expanded = source.expand_abbreviation_selection(selection);
    if !expanded.is_empty() && expanded.iter().any(|id| !atom_ids.contains(id)) {
        return Err(Error::Selection("Select an atom or bond in one molecule"));
    }
    let mut adjacent = HashMap::<u64, Vec<u64>>::new();
    for bond in &source.bonds {
        adjacent.entry(bond.a).or_default().push(bond.b);
        adjacent.entry(bond.b).or_default().push(bond.a);
    }
    let mut remaining = atom_ids;
    let mut components = Vec::new();
    for atom in &source.atoms {
        if !remaining.remove(&atom.id) {
            continue;
        }
        let mut component = HashSet::from([atom.id]);
        let mut pending = vec![atom.id];
        while let Some(id) = pending.pop() {
            for &id in adjacent.get(&id).into_iter().flatten() {
                if remaining.remove(&id) {
                    component.insert(id);
                    pending.push(id);
                }
            }
        }
        components.push(component);
    }
    if expanded.is_empty() {
        if components.len() != 1 {
            return Err(Error::Selection("Select the molecule to convert to 3D"));
        }
        return components
            .pop()
            .ok_or(Error::Selection("Draw a molecule first"));
    }
    let first = *expanded
        .first()
        .ok_or(Error::Selection("Select a molecule"))?;
    let component = components
        .into_iter()
        .find(|c| c.contains(&first))
        .ok_or(Error::Selection("Selected molecule is absent"))?;
    if expanded.iter().any(|id| !component.contains(id)) {
        return Err(Error::Selection("Select atoms or a bond in one molecule"));
    }
    Ok(component)
}

fn materialize_stereo(molecule: &Molecule, document: &mut Document) -> Result<(), Error> {
    let graph = &molecule.state.graph;
    let mut neighbors = vec![Vec::new(); molecule.ids.len()];
    for b in &graph.bonds {
        let a = *molecule
            .ids
            .get(b.a)
            .ok_or(Error::Coordinates("Missing stereo atom"))?;
        let c = *molecule
            .ids
            .get(b.b)
            .ok_or(Error::Coordinates("Missing stereo atom"))?;
        neighbors
            .get_mut(b.a)
            .ok_or(Error::Coordinates("Missing stereo neighbors"))?
            .push(c);
        neighbors
            .get_mut(b.b)
            .ok_or(Error::Coordinates("Missing stereo neighbors"))?
            .push(a);
    }
    for ((id, meta), neighbors) in molecule
        .ids
        .iter()
        .zip(&molecule.state.metadata.atoms)
        .zip(neighbors)
    {
        let atom = document
            .atom_mut(*id)
            .ok_or(Error::Coordinates("Missing drawing stereocenter"))?;
        atom.stereo = match meta.chiral_tag {
            0 => None,
            1 | 2 => Some(AtomStereo {
                winding: if meta.chiral_tag == 1 { "cw" } else { "ccw" }.into(),
                neighbors,
            }),
            _ => return Err(Error::Unsupported("non-tetrahedral stereo")),
        };
    }
    for (b, meta) in graph.bonds.iter().zip(&molecule.state.metadata.bonds) {
        if b.order != 2 {
            continue;
        }
        let a = *molecule
            .ids
            .get(b.a)
            .ok_or(Error::Coordinates("Missing stereo endpoint"))?;
        let c = *molecule
            .ids
            .get(b.b)
            .ok_or(Error::Coordinates("Missing stereo endpoint"))?;
        let bond = document
            .bonds
            .iter_mut()
            .find(|old| (old.a == a && old.b == c) || (old.a == c && old.b == a))
            .ok_or(Error::Coordinates("Missing drawing double bond"))?;
        bond.stereo = match meta.stereo {
            0 => None,
            1 => Some("any"),
            2 => Some("z"),
            3 => Some("e"),
            4 => Some("cis"),
            5 => Some("trans"),
            _ => return Err(Error::Unsupported("nonstandard bond stereo")),
        }
        .map(str::to_owned);
        bond.stereo_atoms = meta
            .stereo_atoms
            .iter()
            .map(|i| {
                molecule
                    .ids
                    .get(*i)
                    .copied()
                    .ok_or(Error::Coordinates("Missing stereo control atom"))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        if bond.a != a {
            bond.stereo_atoms.reverse();
        }
        bond.stereo_authoritative = true;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
