//! Enhanced stereochemistry groups (#766): what is known about a sample beyond
//! its drawn configuration. A stereocentre without a group is absolute (ABS).
//! Centres sharing an AND group are a 1:1 mixture of the drawn form and its
//! mirror image at those centres; centres sharing an OR group are one of the
//! two, not known which. A group belongs to one molecule (connected
//! component), as in a MOL file: `and1` on two separate molecules are two
//! independent groups.
use crate::document::{Atom, Document};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StereoGroup {
    pub kind: Kind,
    /// Starts at 1. AND 1 and OR 1 are different groups.
    pub number: u32,
}

impl std::fmt::Display for StereoGroup {
    /// The canvas label: `and1`, `or2`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match self.kind {
            Kind::And => "and",
            Kind::Or => "or",
        };
        write!(f, "{kind}{}", self.number)
    }
}

/// The groups of the molecules that contain `ids`, in label order.
pub fn used(doc: &Document, ids: &[u64]) -> Vec<StereoGroup> {
    let groups: BTreeSet<_> = crate::reactions::molecules(doc, ids)
        .into_iter()
        .flatten()
        .filter_map(|id| doc.atom(id)?.stereo_group)
        .collect();
    groups.into_iter().collect()
}

/// The lowest number of this kind that the molecules containing `ids` do not
/// use yet.
pub fn next(doc: &Document, ids: &[u64], kind: Kind) -> StereoGroup {
    let used = used(doc, ids);
    (1..)
        .map(|number| StereoGroup { kind, number })
        .find(|group| !used.contains(group))
        .unwrap_or(StereoGroup { kind, number: 1 })
}

/// Number each molecule's AND and OR groups 1, 2, 3… in their current order.
/// Returns whether anything changed.
pub fn renumber(doc: &mut Document) -> bool {
    let all: Vec<_> = doc.atoms.iter().map(|a| a.id).collect();
    let mut renamed = HashMap::new();
    for molecule in crate::reactions::molecules(doc, &all) {
        let mut counts = HashMap::new();
        for group in used(doc, &molecule) {
            let count = counts.entry(group.kind).or_insert(0);
            *count += 1;
            let fresh = StereoGroup {
                kind: group.kind,
                number: *count,
            };
            renamed.extend(molecule.iter().map(|&id| ((id, group), fresh)));
        }
    }
    let mut changed = false;
    for atom in &mut doc.atoms {
        if let Some(group) = &mut atom.stereo_group
            && let Some(&fresh) = renamed.get(&(atom.id, *group))
            && fresh != *group
        {
            *group = fresh;
            changed = true;
        }
    }
    changed
}

pub(crate) fn validate(atom: &Atom) -> Result<(), String> {
    if atom.stereo_group.is_some_and(|g| g.number == 0) {
        return Err("Stereo group numbers start at 1".into());
    }
    Ok(())
}
