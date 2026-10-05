//! Conjugation and hybridization passes from RDKit ConjugHybrid.cpp (2026.03.6).
//! Copyright (C) 2001-2024 Greg Landrum and other RDKit contributors.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
use super::{
    ELEMENTS,
    graph::{Graph, Valence, pi_electron_count},
};
use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Hybridization {
    Unspecified,
    S,
    Sp,
    Sp2,
    Sp3,
    Sp2d,
    Sp3d,
    Sp3d2,
    Other,
}

fn at<T>(items: &[T], id: usize) -> Result<&T, String> {
    items
        .get(id)
        .ok_or_else(|| "Invalid electronic-state index".into())
}

struct Topology<'a> {
    graph: &'a Graph,
    valences: Vec<Valence>,
    neighbors: Vec<Vec<(usize, usize)>>,
}

impl<'a> Topology<'a> {
    fn new(graph: &'a Graph, cache: Option<&[Valence]>) -> Result<Self, String> {
        let valences = graph.cached_valences(cache)?;
        let mut neighbors = vec![Vec::new(); graph.atoms.len()];
        for (id, bond) in graph.bonds.iter().enumerate() {
            for (a, b) in [(bond.a, bond.b), (bond.b, bond.a)] {
                neighbors
                    .get_mut(a)
                    .ok_or("Missing electronic-state endpoint")?
                    .push((b, id));
            }
        }
        Ok(Self {
            graph,
            valences,
            neighbors,
        })
    }

    fn degree(&self, id: usize) -> Result<i32, String> {
        // Graph validation bounds both degree and cached valence well below i32.
        Ok(at(&self.neighbors, id)?.len() as i32
            + i32::from(at(&self.graph.atoms, id)?.explicit_hydrogens)
            + at(&self.valences, id)?.implicit_hydrogens as i32)
    }

    fn electrons(&self, id: usize) -> Result<i32, String> {
        let neighbors = at(&self.neighbors, id)?;
        let mut zero_bonds = 0;
        for &(_, bond) in neighbors {
            zero_bonds += usize::from(at(&self.graph.bonds, bond)?.twice_contribution(id) == 0);
        }
        pi_electron_count(
            at(&self.graph.atoms, id)?,
            at(&self.valences, id)?,
            neighbors.len(),
            zero_bonds,
        )
    }

    fn candidate(&self, id: usize) -> Result<bool, String> {
        let atom = at(&self.graph.atoms, id)?;
        let element = at(ELEMENTS, usize::from(atom.atomic_number))?;
        let valence = at(&self.valences, id)?;
        let default = *element
            .valences
            .first()
            .ok_or("Missing conjugation valence")?;
        if atom.charge == 0
            && default >= 0
            && valence.explicit_valence + valence.implicit_hydrogens > default as u32
        {
            return Ok(false);
        }
        Ok((atom.atomic_number <= 10
            || !matches!(element.outer_electrons, 5 | 6)
            || element.outer_electrons == 6 && self.degree(id)? < 2)
            && self.electrons(id)? > 0)
    }
}

/// Available pi electrons for each atom, using the intermediate property cache.
pub fn pi_electrons(graph: &Graph) -> Result<Vec<i32>, String> {
    let top = Topology::new(graph, None)?;
    (0..graph.atoms.len()).map(|id| top.electrons(id)).collect()
}

/// Assign conjugation without changing atom/bond identity or aromaticity.
/// Every central candidate has at most three neighbors, so the nested bond
/// comparisons remain linear in the size of the validated graph.
pub fn conjugation(graph: &Graph) -> Result<Vec<bool>, String> {
    conjugation_cached(graph, None)
}

pub(crate) fn conjugation_cached(
    graph: &Graph,
    cache: Option<&[Valence]>,
) -> Result<Vec<bool>, String> {
    let top = Topology::new(graph, cache)?;
    let candidates = (0..graph.atoms.len())
        .map(|id| top.candidate(id))
        .collect::<Result<Vec<_>, _>>()?;
    let mut result = graph.bonds.iter().map(|b| b.aromatic).collect::<Vec<_>>();
    for id in 0..graph.atoms.len() {
        if !*at(&candidates, id)? || !(2..=3).contains(&top.degree(id)?) {
            continue;
        }
        for &(other, first) in at(&top.neighbors, id)? {
            if at(&graph.bonds, first)?.twice_contribution(id) < 3 || !*at(&candidates, other)? {
                continue;
            }
            for &(other, second) in at(&top.neighbors, id)? {
                if first != second && *at(&candidates, other)? && top.degree(other)? <= 3 {
                    *result
                        .get_mut(first)
                        .ok_or("Missing first conjugated bond")? = true;
                    *result
                        .get_mut(second)
                        .ok_or("Missing second conjugated bond")? = true;
                }
            }
        }
    }
    Ok(result)
}

/// Assign atom hybridization from conjugation and existing chiral tags (0..=8,
/// the same codes as ranking metadata). This does not perceive stereochemistry.
pub fn hybridization(
    graph: &Graph,
    chiral_tags: &[u8],
    conjugated: &[bool],
) -> Result<Vec<Hybridization>, String> {
    hybridization_cached(graph, chiral_tags, conjugated, None)
}

pub(crate) fn hybridization_cached(
    graph: &Graph,
    chiral_tags: &[u8],
    conjugated: &[bool],
    cache: Option<&[Valence]>,
) -> Result<Vec<Hybridization>, String> {
    if chiral_tags.len() != graph.atoms.len()
        || chiral_tags.iter().any(|&tag| tag > 8)
        || conjugated.len() != graph.bonds.len()
    {
        return Err("Invalid hybridization metadata".into());
    }
    let top = Topology::new(graph, cache)?;
    let mut result = Vec::with_capacity(graph.atoms.len());
    for (id, atom) in graph.atoms.iter().enumerate() {
        use Hybridization::*;
        if atom.atomic_number == 0 {
            result.push(Unspecified);
            continue;
        }
        let degree = top.degree(id)?;
        let explicit = match *at(chiral_tags, id)? {
            1 | 2 | 4 if degree == 4 => Some(Sp3),
            6 if (2..=4).contains(&degree) => Some(Sp2d),
            7 if (2..=5).contains(&degree) => Some(Sp3d),
            8 if (2..=6).contains(&degree) => Some(Sp3d2),
            _ => None,
        };
        if let Some(value) = explicit {
            result.push(value);
            continue;
        }
        let mut orbitals = degree;
        let mut has_conjugated = false;
        for &(_, bond) in at(&top.neighbors, id)? {
            has_conjugated |= *at(conjugated, bond)?;
            let bond = at(&graph.bonds, bond)?;
            // Hydrogen bonds still count here. Only outgoing dative bonds have
            // their degree removed; graph order 0 is HYDROGEN, not ZERO.
            if atom.atomic_number < 89 && bond.order == 5 && bond.b != id {
                orbitals -= 1;
            }
        }
        if (2..89).contains(&atom.atomic_number) {
            let outer = at(ELEMENTS, usize::from(atom.atomic_number))?.outer_electrons;
            let valence = at(&top.valences, id)?;
            let total_valence = valence.explicit_valence as i32 + valence.implicit_hydrogens as i32;
            let charge = i32::from(atom.charge);
            let free = outer - total_valence - charge;
            if total_valence + outer - charge < 8 {
                let radicals = i32::from(atom.radical_electrons);
                // Rust and C++ signed division both truncate toward zero.
                orbitals += (free - radicals) / 2 + radicals;
            } else {
                orbitals += free / 2;
            }
        }
        result.push(match orbitals {
            0 | 1 => S,
            2 => Sp,
            3 => Sp2,
            4 if degree <= 3 && has_conjugated => Sp2,
            4 => Sp3,
            5 => Sp3d,
            6 => Sp3d2,
            _ => Unspecified,
        });
    }
    Ok(result)
}
