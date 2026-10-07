//! Organometallic normalization from RDKit MolOps.cpp (2026.03.6).
//! Copyright (C) 2001-2023 Greg Landrum and other RDKit contributors.
//! Metal classification from QueryOps.cpp, Copyright (C) 2003-2021.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
use super::at;
use crate::{
    ELEMENTS,
    graph::{Atom, Graph, Valence},
    ranking::{self, Metadata},
    rings,
};
use std::cmp::Reverse;

fn is_metal(number: u8) -> bool {
    !matches!(number, 0..=2 | 5..=10 | 14..=18 | 33..=36 | 52..=54 | 85..=86)
}

fn hypervalent_nonmetal(atom: &Atom, valence: &Valence, degree: usize) -> Result<bool, String> {
    if is_metal(atom.atomic_number) {
        return Ok(false);
    }
    let effective = i32::from(atom.atomic_number) - i32::from(atom.charge);
    if effective <= 0 {
        return Ok(false);
    }
    // Unlike the general property cache, this pass does not clamp the effective
    // element. The reference rejects an out-of-range periodic-table lookup.
    let element = ELEMENTS
        .get(effective as usize)
        .ok_or("Invalid effective element in metal cleanup")?;
    let maximum = *element
        .valences
        .last()
        .ok_or("Missing metal-cleanup valence")?;
    let total_degree =
        degree + usize::from(atom.explicit_hydrogens) + valence.implicit_hydrogens as usize;
    Ok(maximum > 0
        && (valence.explicit_valence > maximum as u32
            || valence.explicit_valence == maximum as u32 && atom.aromatic && total_degree == 4))
}

/// Convert one hypervalent donor--metal single bond per donor to donor->metal.
/// Preserve a caller's existing ring cache when supplied; otherwise ranking uses
/// the bounded fast ring pass. Return a copy so failures cannot edit a document.
pub fn organometallics(
    graph: &Graph,
    metadata: &Metadata,
    cached_rings: Option<&[Vec<usize>]>,
) -> Result<Graph, String> {
    let valences = graph.provisional_valences()?;
    let mut neighbors = vec![Vec::new(); graph.atoms.len()];
    let mut datives = vec![0usize; graph.atoms.len()];
    for (id, bond) in graph.bonds.iter().enumerate() {
        for (a, b) in [(bond.a, bond.b), (bond.b, bond.a)] {
            neighbors
                .get_mut(a)
                .ok_or("Missing metal-cleanup endpoint")?
                .push((b, id));
            if bond.order == 5 {
                *datives.get_mut(a).ok_or("Missing dative endpoint")? += 1;
            }
        }
    }
    let mut candidates = Vec::new();
    for (id, atom) in graph.atoms.iter().enumerate() {
        let adjacent = at(&neighbors, id)?;
        // Call the valence test before excluding H/He/F/Ne, matching the
        // reference's validation of extreme formal charges on those atoms.
        if !hypervalent_nonmetal(atom, at(&valences, id)?, adjacent.len())?
            || matches!(atom.atomic_number, 1 | 2 | 9 | 10)
        {
            continue;
        }
        let mut metals = Vec::new();
        for &(other, bond) in adjacent {
            if at(&graph.bonds, bond)?.order == 1
                && is_metal(at(&graph.atoms, other)?.atomic_number)
            {
                metals.push((other, bond));
            }
        }
        if !metals.is_empty() {
            candidates.push((id, metals));
        }
    }
    if candidates.is_empty() {
        return Ok(graph.clone());
    }
    let discovered;
    let cached_rings = match cached_rings {
        Some(rings) => rings,
        None => {
            discovered = rings::fast(graph)?.atoms;
            &discovered
        }
    };
    let ranks = ranking::rank(graph, cached_rings, metadata, ranking::Options::default())?;
    let mut ordered = candidates
        .into_iter()
        .map(|(id, metals)| Ok((*at(&ranks, id)?, id, metals)))
        .collect::<Result<Vec<_>, String>>()?;
    ordered.sort_unstable_by_key(|entry| entry.0);
    let mut result = graph.clone();
    // Changes touch only the current non-metal and a metal. Thus other donors'
    // initial valences remain valid, while each metal's live dative count must
    // be updated before choosing the destination for the next donor.
    for (_, donor, metals) in ordered {
        let mut choice = None;
        for (metal, bond) in metals {
            let key = (
                *at(&datives, metal)?,
                Reverse(*at(&ranks, metal)?),
                metal,
                bond,
            );
            if choice.is_none_or(|previous| key < previous) {
                choice = Some(key);
            }
        }
        let (_, _, metal, id) = choice.ok_or("Missing metal-cleanup candidate")?;
        let bond = result.bonds.get_mut(id).ok_or("Missing donor bond")?;
        bond.order = 5;
        bond.a = donor;
        bond.b = metal;
        for atom in [donor, metal] {
            *datives
                .get_mut(atom)
                .ok_or("Missing updated dative endpoint")? += 1;
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
