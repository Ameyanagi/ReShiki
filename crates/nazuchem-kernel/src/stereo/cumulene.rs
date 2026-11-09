// SPDX-License-Identifier: MIT OR Apache-2.0
// Origin: original
// Source: ReShiki | crates/chemistry/src/stereo/cumulene.rs | be63a799aa03a83dc6ec74bb294f4d42b89298d3

//! Odd cumulene chains with distinguishable ligands at both ends.
// Copied unchanged from crates/chemistry/src/stereo/cumulene.rs at be63a799aa03a83dc6ec74bb294f4d42b89298d3
use super::{at, double_bond_directions, perception, perception::State};
use crate::ranking;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cumulene {
    /// Each terminal atom and its inner chain neighbor, in graph atom indices.
    pub ends: [(usize, usize); 2],
    /// Graph bond indices in order from the first end to the second.
    pub bonds: Vec<usize>,
}

#[derive(PartialEq, Eq)]
enum Ligand {
    Atom(u32),
    Hydrogen,
    LonePair,
}

fn without_maps(state: &State) -> Result<State, String> {
    let mut unmapped = state.clone();
    for atom in &mut unmapped.metadata.atoms {
        atom.map_number = 0;
        atom.map_present = false;
    }
    // Directions reconstructed for adjacent specified bonds can flank an
    // unspecified double bond. Temporarily mark it unknown to prevent the
    // second perception from inventing a configuration there.
    for (bond, meta) in unmapped
        .graph
        .bonds
        .iter()
        .zip(&mut unmapped.metadata.bonds)
    {
        if bond.order == 2 && meta.stereo == 0 {
            meta.stereo = 1;
        }
    }
    let geometry = double_bond_directions(
        &unmapped.graph,
        &unmapped.metadata,
        &unmapped.directions,
        None,
        &unmapped.rings.atoms,
    )?;
    unmapped.metadata = geometry.metadata;
    unmapped.directions = geometry.directions;
    let mut unmapped = perception::perceive(
        &unmapped,
        perception::Options {
            clean: true,
            force: true,
            flag_possible: false,
        },
    )?;
    for ((bond, original), meta) in state
        .graph
        .bonds
        .iter()
        .zip(&state.metadata.bonds)
        .zip(&mut unmapped.metadata.bonds)
    {
        if bond.order == 2 && original.stereo == 0 {
            *meta = original.clone();
        }
    }
    Ok(unmapped)
}

/// Find maximal chains of an odd number (at least three) of double bonds whose
/// ends each have two different ligands. The state must be prepared, with
/// perceived ordinary stereo and caches belonging to the same graph. Isotopes
/// and stereo distinguish ligands; atom maps do not. No configuration is assigned.
pub fn stereogenic_odd_cumulenes(state: &State) -> Result<Vec<Cumulene>, String> {
    let mut neighbors = vec![Vec::new(); state.graph.atoms.len()];
    let mut doubles = neighbors.clone();
    for (id, bond) in state.graph.bonds.iter().enumerate() {
        for (end, other) in [(bond.a, bond.b), (bond.b, bond.a)] {
            neighbors
                .get_mut(end)
                .ok_or("Invalid cumulene atom index")?
                .push((other, id));
            if bond.order == 2 {
                doubles
                    .get_mut(end)
                    .ok_or("Invalid cumulene atom index")?
                    .push((other, id));
            }
        }
    }
    let mut chains = Vec::new();
    for (start, adjacent) in doubles.iter().enumerate() {
        let &[(first, bond)] = adjacent.as_slice() else {
            continue;
        };
        let (mut previous, mut end) = (start, first);
        let mut bonds = vec![bond];
        while let &[a, b] = at(&doubles, end)?.as_slice() {
            let (next, bond) = if a.0 == previous { b } else { a };
            bonds.push(bond);
            (previous, end) = (end, next);
        }
        if start < end && bonds.len() >= 3 && bonds.len() % 2 == 1 && at(&doubles, end)?.len() == 1
        {
            chains.push(Cumulene {
                ends: [(start, first), (end, previous)],
                bonds,
            });
        }
    }
    if chains.is_empty() {
        return Ok(chains);
    }
    // Legacy perception itself uses maps in its priorities. Merely excluding
    // maps from ranking would retain map-dependent E/Z or tetrahedral labels.
    let unmapped = state
        .metadata
        .atoms
        .iter()
        .any(|a| a.map_present || a.map_number != 0)
        .then(|| without_maps(state))
        .transpose()?;
    let state = unmapped.as_ref().unwrap_or(state);
    let ranks = ranking::rank(
        &state.graph,
        &state.rings.atoms,
        &state.metadata,
        ranking::Options {
            break_ties: false,
            include_chirality: true,
            include_isotopes: true,
            include_maps: false,
            ..ranking::Options::default()
        },
    )?;
    let distinct = |end, inner| -> Result<bool, String> {
        let atom = at(&state.graph.atoms, end)?;
        let hydrogens =
            u32::from(atom.explicit_hydrogens) + at(&state.valences, end)?.implicit_hydrogens;
        let mut ligands = Vec::new();
        for &(other, _) in at(&neighbors, end)? {
            if other != inner {
                let atom = at(&state.graph.atoms, other)?;
                // An explicit 1H is the same ligand as H (P-92.3).
                ligands.push(if atom.atomic_number == 1 && atom.isotope <= 1 {
                    Ligand::Hydrogen
                } else {
                    Ligand::Atom(*at(&ranks, other)?)
                });
            }
        }
        ligands.extend((0..hydrogens).map(|_| Ligand::Hydrogen));
        while ligands.len() < 2 {
            ligands.push(Ligand::LonePair);
        }
        Ok(matches!(ligands.as_slice(), [a, b] if a != b))
    };
    let mut result = Vec::new();
    for chain in chains {
        let [(a, inner_a), (b, inner_b)] = chain.ends;
        if distinct(a, inner_a)? && distinct(b, inner_b)? {
            result.push(chain);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
