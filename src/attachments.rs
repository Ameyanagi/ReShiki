//! Semantic attachment nodes. The legacy `centroid` member list remains the
//! native-file storage so old drawing anchors are never silently reinterpreted.
use crate::document::{Atom, Document};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    MultiCenter,
    Variable,
}
impl Kind {
    pub fn cdxml(self) -> &'static str {
        match self {
            Self::MultiCenter => "MultiAttachment",
            Self::Variable => "VariableAttachment",
        }
    }
    pub fn from_cdxml(value: &str) -> Option<Self> {
        match value {
            "MultiAttachment" => Some(Self::MultiCenter),
            "VariableAttachment" => Some(Self::Variable),
            _ => None,
        }
    }
}

/// Detached import patch with stable drawing IDs, never positional guesses.
#[derive(Debug, Clone, Serialize)]
pub struct Attachment {
    pub id: u64,
    pub kind: Kind,
    pub members: Vec<u64>,
}

pub fn present(doc: &Document) -> bool {
    doc.atoms.iter().any(|a| a.attachment.is_some())
}

pub fn add(doc: &mut Document, ids: &[u64], kind: Kind) -> Result<u64, String> {
    doc.validate()?;
    if ids.len() < 2
        || ids.len() > 300
        || ids.iter().any(|id| {
            doc.atom(*id)
                .is_none_or(|a| a.element == "*" || !a.centroid.is_empty())
        })
    {
        return Err("Select 2–300 atoms for an attachment point".into());
    }
    let id = crate::projection::add_centroid(doc, ids)?;
    doc.atom_mut(id)
        .ok_or("Missing attachment point")?
        .attachment = Some(kind);
    Ok(id)
}

pub(crate) fn validate(doc: &Document) -> Result<(), String> {
    for a in doc.atoms.iter().filter(|a| a.attachment.is_some()) {
        if a.centroid.is_empty()
            || a.element != "*"
            || !a.no_implicit
            || a.isotope != 0
            || a.explicit_h != 0
            || a.aromatic
            || a.stereo.is_some()
            || a.display.variable.is_some()
        {
            return Err("Invalid semantic attachment point".into());
        }
        for b in doc.bonds.iter().filter(|b| b.a == a.id || b.b == a.id) {
            let other = if b.a == a.id { b.b } else { b.a };
            if a.centroid.contains(&other) || doc.atom(other).is_none_or(|a| !a.centroid.is_empty())
            {
                return Err(
                    "Connect the attachment point to an atom outside its target set".into(),
                );
            }
        }
    }
    Ok(())
}

/// A temporary wildcard graph used ONLY to preserve atom indexing and assign
/// ordinary ligand bond orders for drawing interchange, never for identifiers
/// or molecular properties. ALL/ANY semantics live in the original document.
pub(crate) fn interchange_graph(doc: &Document) -> Result<Document, String> {
    doc.validate()?;
    let mut copy = doc.clone();
    // Labels do not affect chemistry; haptic group definitions include their
    // anchor through attachment membership rather than a covalent bond.
    copy.abbreviations.clear();
    for a in &mut copy.atoms {
        if a.attachment.take().is_some() {
            a.centroid.clear();
        }
    }
    Ok(copy)
}

/// Connectivity for selection/abbreviation validation, never chemical bonds.
pub fn edges(doc: &Document) -> impl Iterator<Item = (u64, u64)> + '_ {
    doc.bonds.iter().map(|b| (b.a, b.b)).chain(
        doc.atoms
            .iter()
            .filter(|a| a.attachment.is_some())
            .flat_map(|a| a.centroid.iter().map(move |id| (a.id, *id))),
    )
}

/// Anonymous dummy/attachment symbols are editing aids, never figure content.
/// A collapsed chemical group or an explicitly entered label remains visible.
pub fn hidden(a: &Atom, doc: &Document) -> bool {
    a.element == "*" && a.display.variable.is_none() && doc.abbreviation(a.id).is_none()
}

pub fn editor_markers(doc: &Document) -> impl Iterator<Item = &Atom> {
    doc.atoms
        .iter()
        .filter(|a| doc.atom_visible(a.id) && hidden(a, doc))
}

/// Include a selected point's target atoms; complete target selections also
/// carry their attachment points. No chemical bonds are synthesized here.
pub fn selection(doc: &Document, ids: &[u64]) -> Vec<u64> {
    let mut selected: std::collections::HashSet<_> = ids.iter().copied().collect();
    for a in doc.atoms.iter().filter(|a| a.attachment.is_some()) {
        if selected.contains(&a.id) {
            selected.extend(a.centroid.iter().copied());
        }
    }
    for a in doc.atoms.iter().filter(|a| a.attachment.is_some()) {
        if a.centroid.iter().all(|id| selected.contains(id)) {
            selected.insert(a.id);
        }
    }
    let mut selected: Vec<_> = selected.into_iter().collect();
    selected.sort_unstable();
    selected
}

/// Moving a centroid moves its ligand, including covalent substituents, while
/// atoms connected to the attachment point itself remain outside that ligand.
/// This is an editor selection rule, not chemical graph connectivity.
pub fn movement_selection(doc: &Document, ids: &[u64]) -> Vec<u64> {
    use std::collections::{HashMap, HashSet};
    let mut selected: HashSet<_> = doc.expand_abbreviation_selection(ids).into_iter().collect();
    let points: Vec<_> = doc
        .atoms
        .iter()
        .filter(|a| selected.contains(&a.id) && !a.centroid.is_empty())
        .collect();
    if points.is_empty() {
        return ids.to_vec();
    }
    let anchors: HashSet<_> = points.iter().map(|a| a.id).collect();
    let all_points: HashSet<_> = doc
        .atoms
        .iter()
        .filter(|a| !a.centroid.is_empty())
        .map(|a| a.id)
        .collect();
    let mut fixed = HashSet::new();
    let mut adjacent = HashMap::<u64, Vec<u64>>::new();
    for b in &doc.bonds {
        if anchors.contains(&b.a) {
            fixed.insert(b.b);
        }
        if anchors.contains(&b.b) {
            fixed.insert(b.a);
        }
        if !all_points.contains(&b.a) && !all_points.contains(&b.b) {
            adjacent.entry(b.a).or_default().push(b.b);
            adjacent.entry(b.b).or_default().push(b.a);
        }
    }
    let mut pending: Vec<_> = points
        .iter()
        .flat_map(|a| a.centroid.iter().copied())
        .collect();
    let mut visited = HashSet::new();
    while let Some(id) = pending.pop() {
        if fixed.contains(&id) || !visited.insert(id) {
            continue;
        }
        selected.insert(id);
        pending.extend(adjacent.get(&id).into_iter().flatten().copied());
    }
    // Carry any other points on this same ligand, without following their
    // metal contacts into a second ligand or an entire coordination complex.
    for a in doc.atoms.iter().filter(|a| !a.centroid.is_empty()) {
        if a.centroid.iter().all(|id| selected.contains(id)) {
            selected.insert(a.id);
        }
    }
    doc.all_ids()
        .into_iter()
        .filter(|id| selected.contains(id))
        .collect()
}

/// Count the explicitly defined ligands without treating anchors as atoms or
/// inventing covalent metal–carbon bonds. No identifier or valence claim for
/// the assembled coordination complex is made by this composition calculation.
pub fn composition(doc: &Document) -> Result<crate::chemistry::Properties, String> {
    doc.validate()?;
    if doc.atoms.iter().any(|a| {
        a.element == "*"
            && (a.attachment != Some(Kind::MultiCenter)
                || a.charge != 0
                || a.radical_electrons != 0)
    }) {
        return Err("Composition requires defined atoms and fixed multi-center attachments".into());
    }
    let atoms: Vec<_> = doc
        .atoms
        .iter()
        .filter(|a| a.element != "*")
        .cloned()
        .collect();
    let ids: std::collections::HashSet<_> = atoms.iter().map(|a| a.id).collect();
    let bonds = doc
        .bonds
        .iter()
        .filter(|b| ids.contains(&b.a) && ids.contains(&b.b))
        .cloned()
        .collect();
    let molecule = crate::chemistry::document::prepare(&Document {
        atoms,
        bonds,
        ..Document::default()
    })
    .map_err(|e| e.to_string())?;
    crate::chemistry::properties(&molecule.state.graph.atom_facts()?)
}

pub const ANALYSIS_NOTICE: &str = "Attachment targets retained. Complex identifiers and coordination-valence analysis are unavailable. Formula, when shown, counts the defined atoms and ligands with charges as entered.";
