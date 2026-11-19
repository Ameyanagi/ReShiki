//! What a molecule or arrow is for. Only an explicit choice changes it:
//! colour, z-order, grouping and position never do.
use crate::document::Document;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Purpose {
    /// Takes part in reactions, atom maps, chemical copy and checks.
    #[default]
    Chemistry,
    /// A real structure shown for context: it keeps its formula, name,
    /// captions and condition text, but never joins a reaction.
    Reference,
    /// A picture: drawn and exported, ignored by every chemistry tool.
    Illustration,
}
impl Purpose {
    pub const ALL: [Self; 3] = [Self::Chemistry, Self::Reference, Self::Illustration];
    pub fn is_chemistry(&self) -> bool {
        *self == Self::Chemistry
    }
    /// Chemistry and Reference objects keep their chemical information.
    pub fn is_chemical(&self) -> bool {
        *self != Self::Illustration
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Chemistry => "Chemistry",
            Self::Reference => "Reference",
            Self::Illustration => "Illustration",
        }
    }
}

/// Whether any molecule or arrow has a purpose other than Chemistry.
pub fn used(doc: &Document) -> bool {
    doc.atoms.iter().any(|a| !a.purpose.is_chemistry())
        || doc.arrows.iter().any(|a| !a.purpose.is_chemistry())
}

/// The purpose of the object with this ID, if it is an atom or an arrow.
pub fn of(doc: &Document, id: u64) -> Option<Purpose> {
    doc.atom(id)
        .map(|a| a.purpose)
        .or_else(|| doc.arrows.iter().find(|a| a.id == id).map(|a| a.purpose))
}

/// Give every molecule that `ids` touches, and every arrow among `ids`, this
/// purpose. Returns whether anything changed.
pub fn set(doc: &mut Document, ids: &[u64], purpose: Purpose) -> bool {
    let atoms: HashSet<_> = crate::reactions::molecules(doc, ids)
        .into_iter()
        .flatten()
        .collect();
    let arrows: HashSet<_> = ids.iter().copied().collect();
    let mut changed = false;
    for atom in doc.atoms.iter_mut().filter(|a| atoms.contains(&a.id)) {
        changed |= std::mem::replace(&mut atom.purpose, purpose) != purpose;
    }
    for arrow in doc.arrows.iter_mut().filter(|a| arrows.contains(&a.id)) {
        changed |= std::mem::replace(&mut arrow.purpose, purpose) != purpose;
    }
    changed
}

/// Keep one purpose per molecule after an edit. New atoms take the purpose
/// of the molecule they join; joining molecules of different purposes keeps
/// the more chemical one (Chemistry, then Reference).
pub fn reconcile(doc: &mut Document, before: &Document) {
    if !doc.atoms.iter().any(|a| !a.purpose.is_chemistry()) {
        return;
    }
    let old: HashSet<_> = before.atoms.iter().map(|a| a.id).collect();
    let current: HashMap<_, _> = doc.atoms.iter().map(|a| (a.id, a.purpose)).collect();
    let ids: Vec<_> = doc.atoms.iter().map(|a| a.id).collect();
    let mut wanted = HashMap::new();
    for molecule in crate::reactions::molecules(doc, &ids) {
        let purposes: Vec<_> = molecule.iter().filter_map(|id| current.get(id)).collect();
        if purposes.iter().all(|p| Some(p) == purposes.first()) {
            continue;
        }
        let existing: Vec<_> = molecule
            .iter()
            .filter(|id| old.contains(id))
            .filter_map(|id| current.get(id))
            .collect();
        let voters = if existing.is_empty() {
            &purposes
        } else {
            &existing
        };
        let Some(purpose) = Purpose::ALL.into_iter().find(|p| voters.contains(&p)) else {
            continue;
        };
        wanted.extend(molecule.into_iter().map(|id| (id, purpose)));
    }
    for atom in &mut doc.atoms {
        if let Some(purpose) = wanted.get(&atom.id) {
            atom.purpose = *purpose;
        }
    }
}

/// Make a read drawing keep the purpose rules: one purpose per molecule, and
/// no reaction roles or condition text on objects that cannot have them.
pub fn repair(doc: &mut Document) {
    if !used(doc) {
        return;
    }
    let all = doc.clone();
    reconcile(doc, &all);
    crate::reactions::prune(doc);
    crate::arrow_labels::prune(doc);
}

/// The drawing without the molecules and arrows `keep` rejects, nor their
/// captions and condition text. Everything else stays.
pub fn only(doc: &Document, keep: impl Fn(&Purpose) -> bool) -> Cow<'_, Document> {
    if doc.atoms.iter().all(|a| keep(&a.purpose)) && doc.arrows.iter().all(|a| keep(&a.purpose)) {
        return Cow::Borrowed(doc);
    }
    let dropped: HashSet<_> = doc
        .atoms
        .iter()
        .filter(|a| !keep(&a.purpose))
        .map(|a| a.id)
        .chain(
            doc.arrows
                .iter()
                .filter(|a| !keep(&a.purpose))
                .map(|a| a.id),
        )
        .collect();
    let owned = doc
        .molecule_names
        .iter()
        .map(|n| (n.annotation, &n.atoms))
        .chain(
            doc.property_captions
                .iter()
                .map(|c| (c.annotation, &c.atoms)),
        )
        .filter(|(_, atoms)| atoms.iter().any(|id| dropped.contains(id)))
        .map(|(annotation, _)| annotation)
        .chain(
            doc.arrow_labels
                .iter()
                .filter(|l| dropped.contains(&l.arrow))
                .map(|l| l.annotation),
        );
    let dropped: Vec<_> = dropped.iter().copied().chain(owned).collect();
    // Delete rather than select: a selection would pull back what generic
    // brackets or attachments reach.
    let mut part = doc.clone();
    part.delete_preserving_layout(&dropped);
    Cow::Owned(part)
}
