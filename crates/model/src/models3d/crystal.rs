//! Crystal and periodic structures (#417): the unit cell a 3D model is drawn
//! in, the bonds that leave it and what each atom's site keeps from the file.
//!
//! The model's atom positions are Cartesian ångström in the lattice's frame,
//! with the cell's origin at (0, 0, 0), so fractional coordinates are the
//! positions in the basis of [`Cell::vectors`]. A neighbouring cell's copy of
//! an atom is its position plus whole lattice vectors, which is all later
//! repeats and ghost copies need.
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// The unit cell of a model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cell {
    /// The lattice vectors a, b and c in ångström.
    pub vectors: [[f32; 3]; 3],
    /// Bonds that leave the cell in an extended network, each drawn from its
    /// atom as a dashed half-bond toward the partner's copy in the next cell.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub crossing: Vec<Crossing>,
}

/// A bond from atom `a` in the cell to the copy of atom `b` in the cell
/// `image` lattice vectors away. Stored once, with `a < b`, or for an atom
/// bonded to its own copy, with the first nonzero `image` component positive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Crossing {
    pub a: u64,
    pub b: u64,
    pub image: [i8; 3],
}

/// What an atom's site keeps from a crystal file besides its element and
/// place: its label, displacement parameters and asymmetric-unit provenance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Site {
    pub id: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adp: Option<Adp>,
    /// A listed CIF input site, rather than a symmetry-generated packing
    /// copy. Completing its connected molecule can require generated atoms.
    /// Older files omit this flag and conservatively retain every molecule.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub asymmetric: bool,
}

/// Atomic displacement parameters in Å², as U (B values are converted).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Adp {
    /// U_iso or U_equiv.
    Iso(f32),
    /// U11, U22, U33, U12, U13, U23 as CIF gives them, in the basis of the
    /// reciprocal-length-scaled lattice vectors.
    Aniso([f32; 6]),
}

/// Most lattice vectors' length and the farthest an ADP may reach.
const MAX_LENGTH: f32 = 1_000.;
const MAX_U: f32 = 10.;
const MAX_LABEL: usize = 64;

impl Cell {
    /// The cell's eight corners, corner `i` at `(i & 1) a + (i >> 1 & 1) b +
    /// (i >> 2 & 1) c`.
    pub fn corners(&self) -> [[f32; 3]; 8] {
        std::array::from_fn(|i| self.at([i & 1, (i >> 1) & 1, (i >> 2) & 1].map(|v| v as f32)))
    }

    /// The point at fractional coordinates `f`.
    pub fn at(&self, f: [f32; 3]) -> [f32; 3] {
        let [a, b, c] = self.vectors;
        let [x, y, z] = f;
        super::zip3(
            super::zip3(a.map(|v| v * x), b.map(|v| v * y), |p, q| p + q),
            c,
            |p, v| p + v * z,
        )
    }

    /// The fractional coordinates of the point `p`.
    pub fn fractional(&self, p: [f32; 3]) -> [f32; 3] {
        let [a, b, c] = self.vectors;
        let cross = |u: [f32; 3], w: [f32; 3]| {
            [
                u[1] * w[2] - u[2] * w[1],
                u[2] * w[0] - u[0] * w[2],
                u[0] * w[1] - u[1] * w[0],
            ]
        };
        let dot = |u: [f32; 3], w: [f32; 3]| u[0] * w[0] + u[1] * w[1] + u[2] * w[2];
        let volume = self.volume();
        [cross(b, c), cross(c, a), cross(a, b)].map(|n| dot(p, n) / volume)
    }

    /// The centre of the cell.
    pub fn center(&self) -> [f32; 3] {
        self.at([0.5; 3])
    }

    /// The vector of `image` lattice translations.
    pub fn translation(&self, image: [i8; 3]) -> [f32; 3] {
        self.at(image.map(f32::from))
    }

    /// Signed volume in Å³; positive for a right-handed cell.
    pub fn volume(&self) -> f32 {
        let [a, b, c] = self.vectors;
        let cross = [
            b[1] * c[2] - b[2] * c[1],
            b[2] * c[0] - b[0] * c[2],
            b[0] * c[1] - b[1] * c[0],
        ];
        a[0] * cross[0] + a[1] * cross[1] + a[2] * cross[2]
    }

    fn valid(&self, ids: &HashSet<u64>) -> bool {
        let lengths = self
            .vectors
            .iter()
            .all(|v| v.iter().all(|x| x.is_finite()) && super::length(*v) <= MAX_LENGTH);
        let volume = self.volume();
        let mut seen = HashSet::new();
        lengths
            && volume.is_finite()
            && volume > 1e-3
            && self.crossing.len() <= ids.len().saturating_mul(16)
            && self.crossing.iter().all(|c| {
                ids.contains(&c.a)
                    && ids.contains(&c.b)
                    && c.image != [0; 3]
                    && c.image.iter().all(|v| (-3..=3).contains(v))
                    && seen.insert(*c)
            })
    }
}

impl Adp {
    fn valid(&self) -> bool {
        match self {
            Self::Iso(u) => u.is_finite() && (0. ..=MAX_U).contains(u),
            Self::Aniso(u) => u.iter().all(|v| v.is_finite() && v.abs() <= MAX_U),
        }
    }
}

/// Whether a model's cell and sites are valid for its atoms `ids`.
pub(super) fn valid(cell: Option<&Cell>, sites: &[Site], ids: &HashSet<u64>) -> bool {
    let mut seen = HashSet::new();
    cell.is_none_or(|c| c.valid(ids))
        && sites.len() <= ids.len()
        && sites.iter().all(|s| {
            ids.contains(&s.id)
                && seen.insert(s.id)
                && s.label.len() <= MAX_LABEL
                && s.adp.is_none_or(|a| a.valid())
        })
}

/// Whether `doc` holds a crystal: a model with a unit cell.
pub fn present(doc: &crate::document::Document) -> bool {
    doc.models.iter().any(|m| m.cell.is_some())
}

/// Atoms shown by a crystal's molecule-only view, without changing the
/// document. Every finite connected molecule containing a listed CIF input
/// site is kept whole; symmetry-related packing molecules are hidden.
/// Extended networks keep their atoms in the unit cell. A model without
/// asymmetric-unit provenance (including legacy native files) keeps all its
/// molecules, since its input sites cannot be recovered reliably.
///
/// When `include_solvent` is false, a selected discrete component is hidden
/// if it has fewer heavy atoms than the largest selected discrete component
/// and at most eight heavy atoms. This size rule is a display convenience,
/// not chemical solvent identification; equal-sized molecules remain shown.
pub fn molecule_atoms(
    doc: &crate::document::Document,
    model: &super::Model3d,
    include_solvent: bool,
) -> HashSet<u64> {
    let ids = model.ids();
    let asymmetric: HashSet<_> = model
        .sites
        .iter()
        .filter(|site| site.asymmetric && ids.contains(&site.id))
        .map(|site| site.id)
        .collect();
    let mut neighbors: std::collections::HashMap<u64, Vec<u64>> = std::collections::HashMap::new();
    let mut connect = |a, b| {
        if ids.contains(&a) && ids.contains(&b) {
            neighbors.entry(a).or_default().push(b);
            neighbors.entry(b).or_default().push(a);
        }
    };
    for bond in &doc.bonds {
        connect(bond.a, bond.b);
    }
    let mut network = HashSet::new();
    if let Some(cell) = &model.cell {
        for crossing in &cell.crossing {
            connect(crossing.a, crossing.b);
            network.extend([crossing.a, crossing.b]);
        }
    }
    let heavy: HashSet<_> = doc
        .atoms
        .iter()
        .filter(|atom| ids.contains(&atom.id) && atom.element != "H")
        .map(|atom| atom.id)
        .collect();
    let mut visited = HashSet::new();
    let mut components = vec![];
    for atom in &model.atoms {
        if !visited.insert(atom.id) {
            continue;
        }
        let mut members = vec![];
        let mut pending = vec![atom.id];
        while let Some(id) = pending.pop() {
            members.push(id);
            for &other in neighbors.get(&id).into_iter().flatten() {
                if visited.insert(other) {
                    pending.push(other);
                }
            }
        }
        let extended = members.iter().any(|id| network.contains(id));
        if extended || asymmetric.is_empty() || members.iter().any(|id| asymmetric.contains(id)) {
            let heavy_count = members.iter().filter(|id| heavy.contains(id)).count();
            components.push((members, extended, heavy_count));
        }
    }
    let largest = components
        .iter()
        .filter(|(_, extended, _)| !extended)
        .map(|(_, _, heavy_count)| *heavy_count)
        .max()
        .unwrap_or(0);
    components
        .into_iter()
        .filter(|(_, extended, heavy_count)| {
            include_solvent || *extended || *heavy_count == largest || *heavy_count > 8
        })
        .flat_map(|(members, _, _)| members)
        .collect()
}

/// Preserve source provenance before a live crystal model can be discarded.
/// Legacy native files did not record this flag, so their cell supplies it.
pub(crate) fn mark_distance_bonds(doc: &mut crate::document::Document) {
    let cells: Vec<_> = doc
        .models
        .iter()
        .filter(|m| m.cell.is_some())
        .map(|m| m.ids())
        .collect();
    for bond in &mut doc.bonds {
        if cells
            .iter()
            .any(|ids| ids.contains(&bond.a) && ids.contains(&bond.b))
        {
            bond.distance_derived = true;
        }
    }
    require_distance_version(doc);
}

/// Restore legacy provenance from the transaction's snapshot even if deleting
/// an atom already pruned the live cell model. Existing marked bonds retain
/// their provenance through order, stereo and appearance edits.
pub(super) fn inherit_distance_bonds(
    doc: &mut crate::document::Document,
    before: &crate::document::Document,
) {
    let cells: Vec<_> = before
        .models
        .iter()
        .filter(|m| m.cell.is_some())
        .map(|m| m.ids())
        .collect();
    let pairs: HashSet<_> = before
        .bonds
        .iter()
        .filter(|bond| {
            bond.distance_derived
                || cells
                    .iter()
                    .any(|ids| ids.contains(&bond.a) && ids.contains(&bond.b))
        })
        .map(|bond| (bond.a.min(bond.b), bond.a.max(bond.b)))
        .collect();
    for bond in &mut doc.bonds {
        if pairs.contains(&(bond.a.min(bond.b), bond.a.max(bond.b))) {
            bond.distance_derived = true;
        }
    }
    // Only a newly introduced cell supplies provenance for all of its edges.
    // A new user-drawn bond inside an existing cell invalidates that model; it
    // must not be relabelled as an imported distance edge during the edit.
    let new_cells: Vec<_> = doc
        .models
        .iter()
        .filter(|model| {
            model.cell.is_some()
                && !before
                    .models
                    .iter()
                    .any(|old| old.cell.is_some() && old.ids() == model.ids())
        })
        .map(|model| model.ids())
        .collect();
    for bond in &mut doc.bonds {
        if new_cells
            .iter()
            .any(|ids| ids.contains(&bond.a) && ids.contains(&bond.b))
        {
            bond.distance_derived = true;
        }
    }
    require_distance_version(doc);
}

pub(crate) fn require_distance_version(doc: &mut crate::document::Document) {
    if doc.bonds.iter().any(|bond| bond.distance_derived) {
        doc.require_version(crate::document::DISTANCE_BONDS);
    }
}

/// Every atom of a connected component containing a live crystal or a bond
/// inferred from crystal distances. A connected ordinary extension cannot
/// turn the remaining unvalidated edges into a chemical graph.
pub fn analysis_atoms(doc: &crate::document::Document) -> HashSet<u64> {
    let mut blocked: HashSet<_> = doc
        .models
        .iter()
        .filter(|m| m.cell.is_some())
        .flat_map(|m| m.atoms.iter().map(|a| a.id))
        .collect();
    for bond in doc.bonds.iter().filter(|b| b.distance_derived) {
        blocked.extend([bond.a, bond.b]);
    }
    if blocked.is_empty() {
        return blocked;
    }
    let mut neighbors: std::collections::HashMap<u64, Vec<u64>> = std::collections::HashMap::new();
    for bond in &doc.bonds {
        neighbors.entry(bond.a).or_default().push(bond.b);
        neighbors.entry(bond.b).or_default().push(bond.a);
    }
    let mut pending: Vec<_> = blocked.iter().copied().collect();
    while let Some(id) = pending.pop() {
        for other in neighbors.get(&id).into_iter().flatten() {
            if blocked.insert(*other) {
                pending.push(*other);
            }
        }
    }
    blocked
}

/// Whether `ids` (all atoms when empty) intersect a connected component whose
/// distance-derived bonds have not been assigned as chemistry. Query the
/// source before creating a selected fragment, since its cut bonds disappear.
pub fn touches(doc: &crate::document::Document, ids: &[u64]) -> bool {
    if ids.is_empty() {
        present(doc) || doc.bonds.iter().any(|b| b.distance_derived)
    } else {
        let blocked = analysis_atoms(doc);
        ids.iter().any(|id| blocked.contains(id))
    }
}

/// The molecular-properties notice after an edit has removed the lattice.
pub const DISTANCE_ANALYSIS_NOTICE: &str = "Distance-derived bonds: chemical analysis is unavailable. The formula counts the atoms shown, with charges as entered. Save as ReShiki or export a figure to retain this drawing.";

/// Distinguish a live unit cell from the provenance retained after its removal.
pub fn analysis_notice(doc: &crate::document::Document) -> &'static str {
    if present(doc) {
        ANALYSIS_NOTICE
    } else {
        DISTANCE_ANALYSIS_NOTICE
    }
}

/// Import notes of the models containing `ids`, or all models for an empty
/// selection. Notes describe the source import, rather than chemistry errors.
pub fn import_notes<'a>(doc: &'a crate::document::Document, ids: &[u64]) -> Vec<&'a str> {
    let mut seen = HashSet::new();
    doc.models
        .iter()
        .filter(|m| ids.is_empty() || ids.iter().any(|id| m.contains(*id)))
        .flat_map(|m| m.notes.iter().map(String::as_str))
        .filter(|note| seen.insert(*note))
        .collect()
}

/// What the Properties panel says of a crystal, whose bonds come from
/// distances, not chemistry.
pub const ANALYSIS_NOTICE: &str = "Crystal structure: bonds come from atom distances, so chemical analysis is unavailable. The formula counts the atoms shown, with charges as entered.";

/// The formula and mass of the atoms `ids` of `doc` (all of them when
/// empty), counted as drawn, without perceiving bonds or hydrogens.
pub fn composition(
    doc: &crate::document::Document,
    ids: &[u64],
) -> Result<crate::chemistry::Properties, String> {
    let facts = doc
        .atoms
        .iter()
        .filter(|a| ids.is_empty() || ids.contains(&a.id))
        .map(|a| {
            let number = crate::editing::ELEMENTS
                .iter()
                .position(|e| *e == a.element)
                .and_then(|i| u8::try_from(i + 1).ok())
                .ok_or_else(|| format!("No formula for element {}", a.element))?;
            Ok(crate::chemistry::AtomFacts {
                atomic_number: number,
                isotope: u16::try_from(a.isotope).unwrap_or(0),
                charge: i8::try_from(a.charge).unwrap_or(0),
                hydrogens: 0,
                radical_electrons: 0,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    crate::chemistry::properties(&facts)
}

#[cfg(test)]
mod tests;
