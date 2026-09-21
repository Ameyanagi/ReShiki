//! Selected aromatic rings can change presentation without changing chemistry.
use super::{Error, Molecule, at, for_drawing, invalid, prepare};
use crate::{chemistry::smiles::write, document::Document};
use std::collections::{HashMap, HashSet};

/// A verified display edit. The document is ready for one history operation;
/// the prepared molecule and SMILES feed the remaining analysis calculations.
#[derive(Debug)]
pub struct AromaticEdit {
    pub document: Document,
    pub molecule: Molecule,
    pub smiles: String,
}

#[derive(Debug)]
pub struct Aromatic {
    before: Molecule,
    after: Molecule,
    document: Document,
}
impl Aromatic {
    pub fn before(&self) -> &Molecule {
        &self.before
    }
    pub fn after(&self) -> &Molecule {
        &self.after
    }
    pub fn finish(self) -> Result<AromaticEdit, Error> {
        let before = write::write(&self.before.state, write::Options::default())?.text;
        let after = write::write(&self.after.state, write::Options::default())?.text;
        if before.is_empty() || before != after {
            return Err(Error::IdentityChanged);
        }
        Ok(AromaticEdit {
            document: self.document,
            molecule: self.after,
            smiles: after,
        })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{document::Point, editing};

    #[test]
    fn changed_chemical_identity_cannot_publish_a_display_edit() -> anyhow::Result<()> {
        let mut original = Document::default();
        editing::ring(&mut original, Point::default(), 6, false, 42.);
        for (i, bond) in original.bonds.iter_mut().enumerate() {
            bond.order = if i % 2 == 0 { 2 } else { 1 };
        }
        let selected = original.atoms.iter().map(|a| a.id).collect::<Vec<_>>();
        for variant in 0..3 {
            let mut draft = aromatic_display(&original, &selected)?;
            match variant {
                0 => {
                    draft
                        .after
                        .state
                        .graph
                        .atoms
                        .first_mut()
                        .ok_or_else(|| anyhow::anyhow!("Missing atom"))?
                        .isotope = 13
                }
                1 => {
                    draft
                        .after
                        .state
                        .metadata
                        .atoms
                        .first_mut()
                        .ok_or_else(|| anyhow::anyhow!("Missing atom metadata"))?
                        .map_number = 7
                }
                _ => {
                    draft
                        .after
                        .state
                        .graph
                        .bonds
                        .first_mut()
                        .ok_or_else(|| anyhow::anyhow!("Missing bond"))?
                        .b = usize::MAX
                }
            }
            assert!(
                draft.finish().is_err(),
                "Changed identity variant {variant}"
            );
        }
        Ok(())
    }
}
