//! Selected aromatic rings can change presentation without changing chemistry.
use super::{Error, Molecule, at, for_drawing, invalid, prepare};
use crate::{chemistry::RDKIT_VERSION, document::Document};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Canonical identifiers still come from the reference backend. Keep both so a
/// failed or incomplete identity check cannot expose the pending drawing.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub rdkit_version: String,
    pub before: String,
    pub after: String,
}

#[derive(Debug, Serialize)]
pub struct Aromatic {
    before: Molecule,
    after: Molecule,
    // Only chemical states cross the transport; the pending edit stays local.
    #[serde(skip)]
    document: Document,
}
impl Aromatic {
    pub fn before(&self) -> &Molecule {
        &self.before
    }
    pub fn after(&self) -> &Molecule {
        &self.after
    }
    pub fn finish(self, identity: Identity) -> Result<Document, Error> {
        if identity.rdkit_version != RDKIT_VERSION {
            return Err(invalid("Aromatic identity reference version changed"));
        }
        if identity.before.is_empty() || identity.before != identity.after {
            return Err(Error::IdentityChanged);
        }
        Ok(self.document)
    }
}

fn pair(a: u64, b: u64) -> (u64, u64) {
    (a.min(b), a.max(b))
}

pub fn aromatic_display(base: &Document, selection: &[u64]) -> Result<Aromatic, Error> {
    let selected: HashSet<_> = selection.iter().copied().collect();
    if selected.is_empty() {
        return Err(invalid("Select an aromatic ring to change its display"));
    }
    let before = prepare(base)?;
    let chemical: HashMap<_, _> = before
        .state
        .graph
        .bonds
        .iter()
        .map(|b| Ok((pair(*at(&before.ids, b.a)?, *at(&before.ids, b.b)?), b)))
        .collect::<Result<_, Error>>()?;
    let mut keys = HashSet::new();
    for ring in &before.state.rings.atoms {
        let ids = ring
            .iter()
            .map(|&i| at(&before.ids, i).copied())
            .collect::<Result<Vec<_>, _>>()?;
        if !ids.iter().all(|id| selected.contains(id)) {
            continue;
        }
        let edges: Vec<_> = ids
            .iter()
            .zip(ids.iter().cycle().skip(1))
            .map(|(&a, &b)| pair(a, b))
            .collect();
        if edges
            .iter()
            .all(|key| chemical.get(key).is_some_and(|b| b.aromatic))
        {
            keys.extend(edges);
        }
    }
    if keys.is_empty() {
        return Err(invalid(
            "Select all atoms of an aromatic ring; saturated rings keep their chemistry",
        ));
    }
    if base
        .abbreviations
        .iter()
        .any(|a| a.members.iter().any(|id| selected.contains(id)))
    {
        return Err(invalid(
            "Expand the abbreviation before changing its ring display",
        ));
    }
    let show = base
        .bonds
        .iter()
        .filter(|b| keys.contains(&pair(b.a, b.b)))
        .any(|b| b.order != 4);
    let kekule = for_drawing(&before, base)?;
    let orders: HashMap<_, _> = kekule
        .molecule()
        .state
        .graph
        .bonds
        .iter()
        .map(|b| {
            Ok((
                pair(*at(&before.ids, b.a)?, *at(&before.ids, b.b)?),
                b.order,
            ))
        })
        .collect::<Result<_, Error>>()?;
    let mut document = base.clone();
    for bond in &mut document.bonds {
        let key = pair(bond.a, bond.b);
        if keys.contains(&key) {
            bond.order = if show {
                4
            } else {
                *orders
                    .get(&key)
                    .ok_or_else(|| invalid("Missing aromatic drawing bond"))?
            };
            bond.display = "plain".into();
            bond.secondary_display = None;
            bond.stereo = None;
            bond.stereo_atoms.clear();
        }
    }
    if show {
        let ring_atoms: HashSet<_> = keys.iter().flat_map(|&(a, b)| [a, b]).collect();
        for (atom, source) in document.atoms.iter_mut().zip(&before.state.graph.atoms) {
            if ring_atoms.contains(&atom.id) && source.explicit_hydrogens != 0 {
                atom.explicit_h = u32::from(source.explicit_hydrogens);
            }
        }
    }
    let after = prepare(&document)?;
    let drawing = for_drawing(&after, &document)?;
    for ((atom, chemical), valence) in document
        .atoms
        .iter_mut()
        .zip(&drawing.molecule().state.graph.atoms)
        .zip(&drawing.molecule().state.valences)
    {
        atom.label_h = u32::from(chemical.explicit_hydrogens)
            .checked_add(valence.implicit_hydrogens)
            .ok_or_else(|| invalid("Drawing hydrogen count overflow"))?;
    }
    document.validate().map_err(Error::Drawing)?;
    Ok(Aromatic {
        before,
        after,
        document,
    })
}
