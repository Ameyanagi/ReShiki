//! Functional-group normalization before the strict property cache.
//!
//! Adapted from RDKit MolOps.cpp cleanUp (2026.03.6).
//! Copyright (C) 2001-2023 Greg Landrum and other RDKit contributors.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
use super::graph::Graph;

fn at<T>(items: &[T], index: usize) -> Result<&T, String> {
    items
        .get(index)
        .ok_or_else(|| "Invalid normalization index".into())
}

/// Normalize neutral nitro/azide, phosphorus and oxygen-bound halogen groups.
/// This is one sanitization pass, not full molecule sanitization. Return a copy
/// so rejected inputs and later pipeline failures cannot partially edit a graph.
pub fn functional_groups(graph: &Graph) -> Result<Graph, String> {
    let valences = graph.provisional_valences()?;
    let mut neighbors = vec![Vec::new(); graph.atoms.len()];
    for (id, bond) in graph.bonds.iter().enumerate() {
        for (a, b) in [(bond.a, bond.b), (bond.b, bond.a)] {
            neighbors
                .get_mut(a)
                .ok_or("Missing normalization atom")?
                .push((b, id));
        }
    }
    let mut result = graph.clone();
    // Preserve the two-pass nitrogen traversal. The second pass uses the
    // candidates collected before N=O changes, even if their charge changed.
    let mut nitrogens = Vec::new();
    for (id, atom) in graph.atoms.iter().enumerate() {
        if atom.atomic_number != 7 || atom.charge != 0 || at(&valences, id)?.explicit_valence != 5 {
            continue;
        }
        nitrogens.push(id);
        for &(other, bond) in at(&neighbors, id)? {
            let target = at(&result.atoms, other)?;
            if target.atomic_number == 8
                && target.charge == 0
                && at(&result.bonds, bond)?.order == 2
            {
                charge_separate(&mut result, id, other, bond, 1)?;
                break;
            }
        }
    }
    for id in nitrogens {
        for &(other, bond) in at(&neighbors, id)? {
            let target = at(&result.atoms, other)?;
            if target.atomic_number == 7
                && target.charge == 0
                && at(&result.bonds, bond)?.order == 3
            {
                charge_separate(&mut result, id, other, bond, 2)?;
                break;
            }
        }
    }
    for (id, atom) in graph.atoms.iter().enumerate() {
        let adjacent = at(&neighbors, id)?;
        let valence = at(&valences, id)?.explicit_valence;
        if atom.charge != 0 {
            continue;
        }
        if atom.atomic_number == 15 && valence == 5 && adjacent.len() == 3 {
            let mut oxygen = None;
            let mut carbon_or_nitrogen = false;
            for &(other, bond) in adjacent {
                if at(&result.bonds, bond)?.order != 2 {
                    continue;
                }
                let target = at(&result.atoms, other)?;
                if target.atomic_number == 8 && target.charge == 0 {
                    // RDKit retains the last oxygen in the neighbor order.
                    oxygen = Some((other, bond));
                } else if matches!(target.atomic_number, 6 | 7) && at(&neighbors, other)?.len() >= 2
                {
                    carbon_or_nitrogen = true;
                }
            }
            if carbon_or_nitrogen && let Some((other, bond)) = oxygen {
                charge_separate(&mut result, id, other, bond, 1)?;
            }
        } else if matches!(atom.atomic_number, 17 | 35 | 53) && matches!(valence, 3 | 5 | 7) {
            let mut all_oxygen = true;
            for &(other, _) in adjacent {
                all_oxygen &= at(&result.atoms, other)?.atomic_number == 8;
            }
            if all_oxygen {
                let mut charge = 0i8;
                for &(other, bond) in adjacent {
                    if at(&result.bonds, bond)?.order == 2 {
                        charge_separate(&mut result, id, other, bond, 1)?;
                        charge = charge
                            .checked_add(1)
                            .ok_or("Halogen charge exceeds supported range")?;
                    }
                }
                result.atoms.get_mut(id).ok_or("Missing halogen")?.charge = charge;
            }
        }
    }
    Ok(result)
}

fn charge_separate(
    graph: &mut Graph,
    center: usize,
    other: usize,
    bond: usize,
    order: u8,
) -> Result<(), String> {
    graph
        .atoms
        .get_mut(center)
        .ok_or("Missing normalization atom")?
        .charge = 1;
    graph
        .atoms
        .get_mut(other)
        .ok_or("Missing normalization atom")?
        .charge = -1;
    graph
        .bonds
        .get_mut(bond)
        .ok_or("Missing normalization bond")?
        .order = order;
    Ok(())
}
