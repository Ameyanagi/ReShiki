//! Enhanced stereochemistry groups (#766): what is known about a sample beyond
//! its drawn configuration. A stereocentre without a group is absolute (ABS).
//! Centres sharing an AND group are a 1:1 mixture of the drawn form and its
//! mirror image at those centres; centres sharing an OR group are one of the
//! two, not known which. Numbers are shared by the whole drawing, so equal
//! labels always mean the same group.
use crate::document::{Atom, Document};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

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

/// The groups the drawing uses, in label order.
pub fn used(doc: &Document) -> Vec<StereoGroup> {
    let mut groups: Vec<_> = doc
        .atoms
        .iter()
        .filter_map(|a| a.stereo_group)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    groups.sort();
    groups
}

/// The lowest number of this kind that no atom uses.
pub fn next(doc: &Document, kind: Kind) -> StereoGroup {
    let taken: HashSet<_> = doc
        .atoms
        .iter()
        .filter_map(|a| a.stereo_group.filter(|g| g.kind == kind))
        .map(|g| g.number)
        .collect();
    let number = (1..).find(|n| !taken.contains(n)).unwrap_or(1);
    StereoGroup { kind, number }
}

/// Give pasted or inserted atoms group numbers the drawing does not use yet,
/// so a pasted racemate never joins an existing group by accident. Groups
/// within the incoming atoms stay together.
pub fn renumber_incoming(doc: &Document, incoming: &mut [Atom]) {
    let mut taken: HashSet<_> = doc.atoms.iter().filter_map(|a| a.stereo_group).collect();
    let mut renamed = HashMap::new();
    for atom in incoming {
        let Some(group) = atom.stereo_group else {
            continue;
        };
        atom.stereo_group = Some(*renamed.entry(group).or_insert_with(|| {
            let fresh = (!taken.contains(&group))
                .then_some(group)
                .or_else(|| {
                    (1..)
                        .map(|number| StereoGroup {
                            kind: group.kind,
                            number,
                        })
                        .find(|g| !taken.contains(g))
                })
                .unwrap_or(group);
            taken.insert(fresh);
            fresh
        }));
    }
}

pub(crate) fn validate(atom: &Atom) -> Result<(), String> {
    if atom.stereo_group.is_some_and(|g| g.number == 0) {
        return Err("Stereo group numbers start at 1".into());
    }
    Ok(())
}

/// Number each kind's groups 1, 2, 3… in their current order. Returns whether
/// anything changed.
pub fn renumber(doc: &mut Document) -> bool {
    let mut counts = HashMap::new();
    let renamed: HashMap<_, _> = used(doc)
        .into_iter()
        .map(|group| {
            let count = counts.entry(group.kind).or_insert(0);
            *count += 1;
            (
                group,
                StereoGroup {
                    kind: group.kind,
                    number: *count,
                },
            )
        })
        .collect();
    let mut changed = false;
    for atom in &mut doc.atoms {
        if let Some(group) = &mut atom.stereo_group
            && let Some(&fresh) = renamed.get(group)
            && fresh != *group
        {
            *group = fresh;
            changed = true;
        }
    }
    changed
}
