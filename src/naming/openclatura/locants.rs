//! Atom and multiple-bond locants for acyclic and monocyclic parents.
pub use super::assembly_utils::parse_locant;
use super::molecule::{AtomSet, Molecule};
use std::collections::BTreeSet;

pub fn get_atom_locants(oriented_path: &[usize], target_indices: &AtomSet) -> Vec<usize> {
    oriented_path
        .iter()
        .enumerate()
        .filter_map(|(i, idx)| target_indices.contains(idx).then_some(i + 1))
        .collect()
}

pub fn get_bond_locants(
    mol: &Molecule,
    oriented_path: &[usize],
    is_bicycle: bool,
    is_spiro: bool,
    is_polycycle: bool,
) -> (Vec<usize>, Vec<usize>) {
    let mut doubles = Vec::new();
    let mut triples = Vec::new();
    let mut seen = BTreeSet::new();
    for (i, &u) in oriented_path.iter().enumerate() {
        for &v in mol.get_neighbors(u) {
            let Some(j) = oriented_path.iter().position(|&idx| idx == v) else {
                continue;
            };
            let Some(bond) = mol.get_bond(u, v) else {
                continue;
            };
            if bond.order <= 1 || !seen.insert(bond.idx) {
                continue;
            }
            let (low, high) = ((i + 1).min(j + 1), (i + 1).max(j + 1));
            let locant = if high == low + 1 {
                low
            } else if low == 1
                && high == oriented_path.len()
                && !(is_bicycle || is_spiro || is_polycycle)
            {
                high
            } else {
                low
            };
            match bond.order {
                2 => doubles.push(locant),
                3 => triples.push(locant),
                _ => (),
            }
        }
    }
    doubles.sort();
    triples.sort();
    (doubles, triples)
}
