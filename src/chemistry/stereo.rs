//! Stereo cleanup from RDKit Chirality.cpp, MolOps.cpp and Atropisomers.cpp.
//! Copyright (C) 2001-2024 Greg Landrum and other RDKit contributors;
//! Copyright (C) 2004-2021 Tad hurst/CDD and other RDKit contributors.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
//!
//! Cleanup repairs annotations; the drawing pass reads wedged-bond geometry.
//! Absolute configurations remain separate. Results never edit the caller's data.
mod drawing;
pub mod perception;
mod priority;
use super::{
    electronic::{self, Hybridization},
    graph::{Graph, Valence},
    ranking::{Metadata, StereoGroup},
};
pub use drawing::{
    BondGeometry, DrawnStereo, Point3, bond_stereo_from_directions, detect_bond_stereo,
    double_bond_directions, from_directions,
};
pub use priority::atom_priorities;
use std::collections::{HashMap, HashSet};

#[cfg(test)]
mod tests;

fn at<T>(items: &[T], id: usize) -> Result<&T, String> {
    items
        .get(id)
        .ok_or_else(|| "Invalid stereo-cleanup index".into())
}

/// Remove impossible atom chiral tags and out-of-range coordination permutations.
/// The supplied hybridizations must belong to this graph at the current stage.
pub fn chirality(
    graph: &Graph,
    metadata: &Metadata,
    hybridizations: &[Hybridization],
) -> Result<Metadata, String> {
    chirality_cached(graph, metadata, hybridizations, None)
}

pub(crate) fn chirality_cached(
    graph: &Graph,
    metadata: &Metadata,
    hybridizations: &[Hybridization],
    cache: Option<&[Valence]>,
) -> Result<Metadata, String> {
    let valences = graph.cached_valences(cache)?;
    metadata.validate(graph)?;
    if hybridizations.len() != graph.atoms.len() {
        return Err("Invalid stereo-cleanup hybridizations".into());
    }
    let mut degree = graph
        .atoms
        .iter()
        .zip(&valences)
        .map(|(a, v)| u32::from(a.explicit_hydrogens) + v.implicit_hydrogens)
        .collect::<Vec<_>>();
    for bond in &graph.bonds {
        for id in [bond.a, bond.b] {
            *degree.get_mut(id).ok_or("Missing stereo degree")? += 1;
        }
    }
    let mut result = metadata.clone();
    let mut clean_groups = false;
    for (id, atom) in result.atoms.iter_mut().enumerate() {
        let maximum = match atom.chiral_tag {
            1 | 2 | 4 => {
                if *at(hybridizations, id)? != Hybridization::Sp3 {
                    atom.chiral_tag = 0;
                    clean_groups = true;
                    None
                } else if atom.chiral_tag == 4 {
                    Some(2)
                } else {
                    None
                }
            }
            6..=8 => {
                let (max_degree, max_permutation) = match atom.chiral_tag {
                    6 => (4, 3),
                    7 => (5, 20),
                    _ => (6, 30),
                };
                if !(2..=max_degree).contains(at(&degree, id)?) {
                    atom.chiral_tag = 0;
                    // The reference only triggers group cleanup for removed
                    // tetrahedral tags, even when coordination tags are cleared.
                    None
                } else {
                    Some(max_permutation)
                }
            }
            _ => None,
        };
        if let Some(maximum) = maximum
            && atom.chiral_permutation.is_some_and(|p| p > maximum)
        {
            atom.chiral_permutation = Some(0);
        }
    }
    if clean_groups {
        let mut groups = Vec::new();
        for group in &result.groups {
            let mut atoms = Vec::new();
            let mut bonds = Vec::new();
            for &id in &group.atoms {
                if at(&result.atoms, id)?.chiral_tag != 0 {
                    atoms.push(id);
                }
            }
            for &id in &group.bonds {
                if matches!(at(&result.bonds, id)?.stereo, 6 | 7) {
                    bonds.push(id);
                }
            }
            if atoms.len() == group.atoms.len() && bonds.len() == group.bonds.len() {
                groups.push(group.clone());
            } else if !atoms.is_empty() {
                // Reconstructed groups retain read IDs; write IDs are reassigned
                // by a later serializer. Match the reference's atom-presence gate.
                groups.push(StereoGroup {
                    kind: group.kind,
                    atoms,
                    bonds,
                    read_id: group.read_id,
                    write_id: 0,
                });
            }
        }
        result.groups = groups;
    }
    Ok(result)
}

struct Work(usize);
impl Work {
    fn spend(&mut self, amount: usize) -> Result<(), String> {
        self.0 = self
            .0
            .checked_sub(amount)
            .ok_or("Atropisomer cleanup work limit exceeded")?;
        Ok(())
    }
}

/// Remove atropisomer annotations inconsistent with hybridization or a small
/// ring. Callers supply an SSSR-or-better ring cache, not the fast cycle basis.
/// As in RDKit, an unspecified first-atom hybridization triggers recalculation
/// in temporary data; the caller's annotations remain unchanged.
pub fn atropisomers(
    graph: &Graph,
    metadata: &Metadata,
    hybridizations: &[Hybridization],
    rings: &[Vec<usize>],
) -> Result<Metadata, String> {
    atropisomers_with_work(
        graph,
        metadata,
        hybridizations,
        rings,
        &mut Work(50_000_000),
    )
}

fn atropisomers_with_work(
    graph: &Graph,
    metadata: &Metadata,
    hybridizations: &[Hybridization],
    rings: &[Vec<usize>],
    work: &mut Work,
) -> Result<Metadata, String> {
    graph.validate()?;
    metadata.validate(graph)?;
    if hybridizations.len() != graph.atoms.len() {
        return Err("Invalid atropisomer hybridizations".into());
    }
    let recalculated;
    let hybridizations = if hybridizations.first() == Some(&Hybridization::Unspecified) {
        let flags = electronic::conjugation(graph)?;
        let tags = metadata
            .atoms
            .iter()
            .map(|a| a.chiral_tag)
            .collect::<Vec<_>>();
        recalculated = electronic::hybridization(graph, &tags, &flags)?;
        &recalculated
    } else {
        hybridizations
    };
    let mut by_pair = HashMap::new();
    let mut neighbors = vec![Vec::new(); graph.atoms.len()];
    for (id, bond) in graph.bonds.iter().enumerate() {
        by_pair.insert((bond.a.min(bond.b), bond.a.max(bond.b)), id);
        for atom in [bond.a, bond.b] {
            neighbors
                .get_mut(atom)
                .ok_or("Missing atropisomer endpoint")?
                .push(id);
        }
    }
    let mut minimum_ring = vec![usize::MAX; graph.bonds.len()];
    let mut stored = 0usize;
    for ring in rings {
        stored = stored
            .checked_add(ring.len())
            .ok_or("Stereo ring storage exceeded")?;
        if stored > 2_000_000
            || ring.len() < 3
            || ring.iter().collect::<HashSet<_>>().len() != ring.len()
        {
            return Err("Invalid stereo ring data".into());
        }
        work.spend(ring.len())?;
        for (&a, &b) in ring.iter().zip(ring.iter().cycle().skip(1)) {
            let id = *by_pair
                .get(&(a.min(b), a.max(b)))
                .ok_or("Missing stereo ring bond")?;
            let size = minimum_ring.get_mut(id).ok_or("Missing ring size")?;
            *size = (*size).min(ring.len());
        }
    }
    let mut result = metadata.clone();
    let mut clean_groups = false;
    for (id, (bond, meta)) in graph.bonds.iter().zip(result.bonds.iter_mut()).enumerate() {
        work.spend(1)?;
        if matches!(meta.stereo, 6 | 7)
            && (*at(hybridizations, bond.a)? != Hybridization::Sp2
                || *at(hybridizations, bond.b)? != Hybridization::Sp2
                || *at(&minimum_ring, id)? < 8)
        {
            meta.stereo = 0;
            clean_groups = true;
        }
    }
    if clean_groups {
        let mut output_storage = 0usize;
        for group in &mut result.groups {
            let mut atoms = Vec::new();
            let mut bonds = Vec::new();
            let mut seen = HashSet::new();
            for &atom in &group.atoms {
                let adjacent = at(&neighbors, atom)?;
                work.spend(adjacent.len() + 1)?;
                let mut found = false;
                for &bond in adjacent {
                    if matches!(at(&result.bonds, bond)?.stereo, 6 | 7) {
                        found = true;
                        if seen.insert(bond) {
                            bonds.push(bond);
                        }
                    }
                }
                if !found {
                    atoms.push(atom);
                }
            }
            if !bonds.is_empty() {
                // The reference reconstructs membership from incident bonds,
                // replacing old bond membership and resetting both group IDs.
                *group = StereoGroup {
                    kind: group.kind,
                    atoms,
                    bonds,
                    read_id: 0,
                    write_id: 0,
                };
            }
            output_storage = output_storage
                .checked_add(1)
                .and_then(|s| s.checked_add(group.atoms.len()))
                .and_then(|s| s.checked_add(group.bonds.len()))
                .ok_or("Atropisomer group storage exceeded")?;
            if output_storage > 2_000_000 {
                return Err("Atropisomer group storage exceeded".into());
            }
        }
    }
    Ok(result)
}
