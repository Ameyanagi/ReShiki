//! Gonane topology and OPSIN's steroid alpha/beta reference order. Coordinates,
//! elements, aromaticity and bond orders do not identify or orient the nucleus.
use crate::{chemistry::document::Molecule, document::Document};
use std::collections::{HashMap, HashSet};

const EDGES: [(usize, usize); 20] = [
    (1, 2),
    (2, 3),
    (3, 4),
    (4, 5),
    (5, 10),
    (10, 1),
    (5, 6),
    (6, 7),
    (7, 8),
    (8, 9),
    (9, 10),
    (9, 11),
    (11, 12),
    (12, 13),
    (13, 14),
    (14, 8),
    (14, 15),
    (15, 16),
    (16, 17),
    (17, 13),
];
// Close the unique five-membered ring first, then its fused six-membered rings.
const SEARCH: [usize; 17] = [13, 14, 15, 16, 17, 8, 9, 11, 12, 10, 5, 6, 7, 1, 2, 3, 4];
// OPSIN alphaBetaClockwiseAtomOrdering for gonane and its steroid parents.
const CLOCKWISE: [usize; 17] = [11, 12, 13, 17, 16, 15, 14, 8, 7, 6, 5, 4, 3, 2, 1, 10, 9];

#[allow(clippy::indexing_slicing)] // Prepared graph indices and fixed locants are validated.
fn nuclei(molecule: &Molecule, adjacent: &[Vec<usize>]) -> Vec<[usize; 17]> {
    let mut pattern = [const { Vec::new() }; 17];
    for (a, b) in EDGES {
        pattern[a - 1].push(b - 1);
        pattern[b - 1].push(a - 1);
    }
    let cyclic: HashSet<_> = molecule
        .state
        .rings
        .atoms
        .iter()
        .flatten()
        .copied()
        .collect();
    let cyclic_adjacent: Vec<Vec<_>> = adjacent
        .iter()
        .enumerate()
        .map(|(i, neighbours)| {
            if cyclic.contains(&i) {
                neighbours
                    .iter()
                    .copied()
                    .filter(|n| cyclic.contains(n))
                    .collect()
            } else {
                Vec::new()
            }
        })
        .collect();
    let mut matches = Vec::new();
    for &start in &cyclic {
        let mut mapping = [usize::MAX; 17];
        mapping[SEARCH[0] - 1] = start;
        search(&pattern, &cyclic_adjacent, &mut mapping, 1, &mut matches);
    }
    matches
}

#[allow(clippy::indexing_slicing)] // Only prepared indices and fixed pattern locants enter recursion.
fn search(
    pattern: &[Vec<usize>; 17],
    adjacent: &[Vec<usize>],
    mapping: &mut [usize; 17],
    depth: usize,
    matches: &mut Vec<[usize; 17]>,
) {
    if depth == SEARCH.len() {
        // Gonane has no nontrivial automorphism: every embedding uniquely
        // assigns locants, even if more than one nucleus occurs in a molecule.
        matches.push(*mapping);
        return;
    }
    let locant = SEARCH[depth] - 1;
    let Some(&anchor) = pattern[locant].iter().find(|&&n| mapping[n] != usize::MAX) else {
        return;
    };
    for &atom in &adjacent[mapping[anchor]] {
        if adjacent[atom].len() < pattern[locant].len() || mapping.contains(&atom) {
            continue;
        }
        if mapping.iter().enumerate().any(|(other, &mapped)| {
            mapped != usize::MAX
                && pattern[locant].contains(&other) != adjacent[atom].contains(&mapped)
        }) {
            continue;
        }
        mapping[locant] = atom;
        search(pattern, adjacent, mapping, depth + 1, matches);
        mapping[locant] = usize::MAX;
    }
}

#[allow(clippy::indexing_slicing)] // Chemical graph indices are validated by prepare; locants are fixed.
pub(super) fn labels(molecule: &Molecule, checked: &Document) -> HashMap<u64, String> {
    let state = &molecule.state;
    let mut adjacent = vec![Vec::new(); state.graph.atoms.len()];
    for bond in &state.graph.bonds {
        adjacent[bond.a].push(bond.b);
        adjacent[bond.b].push(bond.a);
    }
    let cip: HashMap<_, _> = checked
        .atoms
        .iter()
        .map(|a| (a.id, a.cip_label.as_deref()))
        .collect();
    let mut result = HashMap::new();
    let mut ambiguous = HashSet::new();
    for nucleus in nuclei(molecule, &adjacent) {
        for (position, &locant) in CLOCKWISE.iter().enumerate() {
            let center = nucleus[locant - 1];
            let id = molecule.ids[center];
            if !matches!(cip.get(&id), Some(Some("R" | "S"))) {
                continue;
            }
            let tag = state.metadata.atoms[center].chiral_tag;
            if !matches!(tag, 1 | 2) {
                continue;
            }
            let mut neighbours: Vec<_> = adjacent[center].iter().copied().map(Some).collect();
            let hydrogens = u32::from(state.graph.atoms[center].explicit_hydrogens)
                + state.valences[center].implicit_hydrogens;
            if neighbours.len() == 3 && hydrogens == 1 {
                neighbours.push(None);
            } else if neighbours.len() != 4 || hydrogens != 0 {
                continue;
            }
            let previous = Some(nucleus[CLOCKWISE[(position + 16) % 17] - 1]);
            let next = Some(nucleus[CLOCKWISE[(position + 1) % 17] - 1]);
            let remaining: Vec<_> = neighbours
                .iter()
                .copied()
                .filter(|&n| n != previous && n != next)
                .collect();
            if remaining.len() != 2 {
                continue;
            }
            let hydrogen =
                |n: Option<usize>| n.is_none_or(|n| state.graph.atoms[n].atomic_number == 1);
            let second = if remaining[0].is_some_and(|n| nucleus.contains(&n)) {
                remaining[0]
            } else if remaining[1].is_some_and(|n| nucleus.contains(&n))
                || hydrogen(remaining[0]) && !hydrogen(remaining[1])
            {
                remaining[1]
            } else if hydrogen(remaining[1]) && !hydrogen(remaining[0]) {
                remaining[0]
            } else {
                // Two external non-H substituents need the named substituent
                // context which a drawing lacks (e.g. 17-ethynyl steroids).
                continue;
            };
            let third = if remaining[0] == second {
                remaining[1]
            } else {
                remaining[0]
            };
            let reference = [previous, second, third, next];
            let Some(permutation): Option<Vec<_>> = reference
                .iter()
                .map(|n| neighbours.iter().position(|other| other == n))
                .collect()
            else {
                continue;
            };
            let odd = (0..4)
                .flat_map(|i| (i + 1..4).map(move |j| (i, j)))
                .filter(|&(i, j)| permutation[i] > permutation[j])
                .count()
                % 2
                != 0;
            // Native tag 1 is SMILES @@: OPSIN parity +1 in the same four
            // carrier order, including an implicit H as the final carrier.
            let alpha = (tag == 1) != odd;
            let label = format!("{locant}{}", if alpha { 'α' } else { 'β' });
            if result.get(&id).is_some_and(|old| old != &label) {
                ambiguous.insert(id);
            }
            result.insert(id, label);
        }
    }
    result.retain(|id, _| !ambiguous.contains(id));
    result
}

#[cfg(test)]
mod tests;
