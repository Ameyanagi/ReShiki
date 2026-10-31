//! Narrow proofs that every upstream fusion route must abstain.
//!
//! This module does not choose components, number a fused parent, or render a name.
use super::super::{
    molecule::{AtomSet, Molecule},
    retained_fused_templates::retained_graph_templates,
};
use std::collections::BTreeMap;

/// Prove abstention for a neutral, two-face fused bicycle with an unavailable face.
/// Unknown graphs and every unproved condition leave fusion eligibility intact.
pub fn planner_certainly_abstains(mol: &Molecule, atoms: &AtomSet) -> bool {
    if atoms
        .iter()
        .any(|id| mol.atoms.get(id).is_none_or(|a| a.charge != 0))
    {
        return false;
    }
    let adjacency: BTreeMap<_, Vec<_>> = atoms
        .iter()
        .map(|&id| {
            (
                id,
                mol.get_neighbors(id)
                    .iter()
                    .copied()
                    .filter(|n| atoms.contains(n))
                    .collect(),
            )
        })
        .collect();
    if adjacency
        .values()
        .any(|neighbors| !matches!(neighbors.len(), 2 | 3))
        || adjacency.values().map(Vec::len).sum::<usize>() != 2 * (atoms.len() + 1)
    {
        return false;
    }
    let junctions: Vec<_> = adjacency
        .iter()
        .filter(|(_, neighbors)| neighbors.len() == 3)
        .map(|(&id, _)| id)
        .collect();
    let [start, end] = junctions.as_slice() else {
        return false;
    };
    if !adjacency[start].contains(end) {
        return false;
    }
    let mut seen = AtomSet::from([*start, *end]);
    let mut faces = Vec::new();
    for &first in adjacency[start].iter().filter(|&&n| n != *end) {
        let mut face = vec![*start];
        let (mut previous, mut current) = (*start, first);
        while current != *end {
            if !seen.insert(current) {
                return false;
            }
            face.push(current);
            let following: Vec<_> = adjacency[&current]
                .iter()
                .copied()
                .filter(|&n| n != previous)
                .collect();
            let [next] = following.as_slice() else {
                return false;
            };
            (previous, current) = (current, *next);
        }
        face.push(*end);
        // These are precisely the two chordless cycles: their perimeter union
        // has the shared junction edge as a chord (fusion/faces.py).
        if !(5..=8).contains(&face.len()) {
            return false;
        }
        faces.push(face);
    }
    if faces.len() != 2 || seen != *atoms {
        return false;
    }

    // wrappers.py:plan_bridged_fusion_wrapper exits at cycle_rank < 3.
    // descriptor.py:_exact_component_covers partitions faces without overlap;
    // cover.py:audit_component_cover requires at least two components. With two
    // faces, a useful cover must therefore consist of the two single faces.
    // A whole-bicycle template cannot replace an unavailable single face.
    // third_component.py:_prohibited_pairwise_citation additionally requires a
    // cyclic component cover; a cover with at most two nodes has no such cycle.
    faces
        .iter()
        .any(|face| unavailable_heterocyclic_face(mol, face))
}

fn symbol_counts<'a>(symbols: impl Iterator<Item = &'a str>) -> BTreeMap<&'a str, usize> {
    let mut counts = BTreeMap::new();
    for symbol in symbols {
        *counts.entry(symbol).or_default() += 1;
    }
    counts
}

fn unavailable_heterocyclic_face(mol: &Molecule, face: &[usize]) -> bool {
    if face.iter().all(|id| mol.atoms[id].is_carbon()) {
        return false;
    }
    let counts = symbol_counts(face.iter().map(|id| mol.atoms[id].symbol.as_str()));
    // registry.py:from_data draws explicit components from retained templates;
    // its extra generated carbocycles and annulenes contain only C. Admit every
    // retained cycle-sized graph, including disabled/unregistered ones, as a
    // possible match. An exact match to one face necessarily has E == V;
    // extra templates passing that count only make this overapproximation
    // more conservative. A multiset mismatch proves absence without chemical
    // recognition or dependence on stored ring-membership metadata.
    if retained_graph_templates(true).iter().any(|template| {
        template.bonds.len() == template.atoms.len()
            && symbol_counts(template.atoms.iter().map(|a| a.symbol.as_str())) == counts
    }) {
        return false;
    }

    // The only remaining source is hw_fusion_components_for_ring. Its lambda
    // gate uses the whole molecule, excluding doubly bonded oxygen ligands
    // (_needs_lambda in hantzsch_widman.py). Only a strict excess is needed here.
    if face.iter().any(|&id| {
        let atom = &mol.atoms[&id];
        !atom.is_carbon()
            && atom.charge == 0
            && atom.element().is_some_and(|policy| {
                atom.total_h_count
                    + mol
                        .get_neighbors(id)
                        .iter()
                        .map(|&n| {
                            mol.get_bond(id, n)
                                .filter(|b| !(b.order == 2 && mol.atoms[&n].symbol == "O"))
                                .map_or(0, |b| b.order)
                        })
                        .sum::<usize>()
                    > policy.standard_valence
            })
    }) {
        return true;
    }
    // Do not use an aromatic Kekule representation to establish abstention.
    if face.iter().any(|id| mol.atoms[id].is_aromatic) {
        return false;
    }
    let mut available: Vec<_> = face
        .iter()
        .map(|id| {
            !mol.atoms[id]
                .element()
                .is_some_and(|e| e.mancude_forced_single)
        })
        .collect();
    // mancude_bond_orders pairs every open run between forced-single symbols;
    // with none it pairs floor(n/2) cycle vertices. This is that same maximum,
    // with no additional nitrogen-degree or observed-valence constraints.
    let expected = maximum_cycle_matching_size(&available);
    if expected == 0 {
        return true;
    }
    let mut observed = 0;
    for (index, &left) in face.iter().enumerate() {
        let next = (index + 1) % face.len();
        let Some(bond) = mol.get_bond(left, face[next]) else {
            return false;
        };
        if !matches!(bond.order, 1 | 2) {
            return true;
        }
        if bond.order == 2 {
            // Replay _is_mancude_or_hydro_derivative's fixed occupied endpoints.
            if !available[index] || !available[next] {
                return true;
            }
            available[index] = false;
            available[next] = false;
            observed += 1;
        }
    }
    observed + maximum_cycle_matching_size(&available) != expected
}

fn maximum_cycle_matching_size(available: &[bool]) -> usize {
    let Some(blocked) = available.iter().position(|&free| !free) else {
        return available.len() / 2;
    };
    let (mut result, mut run) = (0, 0);
    for offset in 1..=available.len() {
        if available[(blocked + offset) % available.len()] {
            run += 1;
        } else {
            result += run / 2;
            run = 0;
        }
    }
    result + run / 2
}

#[cfg(test)]
mod tests {
    use super::super::super::{graph_io::read_smiles, graph_kernel::biconnected_edge_components};
    use super::*;

    fn proves(smiles: &str) -> bool {
        let mol = read_smiles(smiles).unwrap();
        let all = mol.atoms.keys().copied().collect();
        biconnected_edge_components(&mol, &all).iter().any(|edges| {
            let atoms = edges.iter().flat_map(|&(u, v)| [u, v]).collect();
            planner_certainly_abstains(&mol, &atoms)
        })
    }

    #[test]
    fn unavailable_hw_faces_prove_all_fusion_routes_abstain() {
        for smiles in [
            "C1=CCSC2SC=NC2=C1",
            "C1=CCC2C=NNC2SC1",
            "NS(=O)(=O)S12CCCNCC1CCC2",
        ] {
            assert!(proves(smiles), "{smiles}");
        }
    }

    #[test]
    fn supported_component_faces_and_carbon_annulenes_remain_eligible() {
        for smiles in [
            "C1=CNC2=C(C1)CNN2",      // tetrahydropyrazolopyridine
            "C1=CC2=CC=CC=C2N1",      // indole component inventory
            "C1=CC=C2C=CC=CC=CC2=C1", // seven/eight-membered carbon faces
            "C1CCC2CCCCC2C1",         // complete saturated carbon parent
        ] {
            assert!(!proves(smiles), "{smiles}");
        }
    }
}
