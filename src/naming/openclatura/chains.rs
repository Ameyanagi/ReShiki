//! Carbon-chain enumeration and the single-cycle part of Python `chains.py`.
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use super::molecule::{AtomSet, Molecule};

#[derive(Clone, Debug, Default)]
pub struct RingSystem {
    pub atoms: AtomSet,
    pub paths: Vec<Vec<usize>>,
}

/// Cycle membership in the induced graph. This deliberately tests membership,
/// rather than using the 2-core (which can also contain bridges between rings).
pub fn get_cyclic_atoms(mol: &Molecule, exclude_atoms: &AtomSet) -> AtomSet {
    let valid: AtomSet = mol
        .atoms
        .keys()
        .filter(|i| !exclude_atoms.contains(i))
        .copied()
        .collect();
    let mut cyclic = AtomSet::new();
    for &start in &valid {
        'edges: for &next in mol.get_neighbors(start) {
            if !valid.contains(&next) {
                continue;
            }
            let mut visited = BTreeSet::from([start, next]);
            let mut queue = VecDeque::from([next]);
            while let Some(current) = queue.pop_front() {
                for &neighbor in mol.get_neighbors(current) {
                    if !valid.contains(&neighbor) {
                        continue;
                    }
                    if neighbor == start && current != next {
                        cyclic.insert(start);
                        break 'edges;
                    }
                    if visited.insert(neighbor) {
                        queue.push_back(neighbor);
                    }
                }
            }
        }
    }
    cyclic
}

pub fn find_all_carbon_paths(
    mol: &Molecule,
    exclude_atoms: &AtomSet,
) -> Result<Vec<Vec<usize>>, String> {
    let cyclic = get_cyclic_atoms(mol, exclude_atoms);
    let valid: AtomSet = mol
        .atoms
        .iter()
        .filter_map(|(&idx, atom)| {
            (atom.symbol == "C" && !exclude_atoms.contains(&idx) && !cyclic.contains(&idx))
                .then_some(idx)
        })
        .collect();
    let endpoints: Vec<usize> = valid
        .iter()
        .filter(|&&idx| {
            mol.get_neighbors(idx)
                .iter()
                .filter(|n| valid.contains(n))
                .count()
                <= 1
        })
        .copied()
        .collect();
    let starts: Vec<usize> = if endpoints.is_empty() {
        valid.iter().copied().collect()
    } else {
        endpoints
    };
    let mut unique = Vec::new();
    let mut seen = BTreeSet::new();
    for start in starts {
        let mut stack = vec![(start, vec![start], AtomSet::from([start]))];
        while let Some((current, path, visited)) = stack.pop() {
            mol.spend_naming_work(path.len())?;
            let neighbors: Vec<usize> = mol
                .get_neighbors(current)
                .iter()
                .filter(|n| valid.contains(n) && !visited.contains(n))
                .copied()
                .collect();
            if neighbors.is_empty() {
                // Removing cyclic atoms leaves a forest: maximal paths differ only by reversal.
                let mut key = path.clone();
                if key.first() > key.last() {
                    key.reverse();
                }
                if seen.insert(key) {
                    unique.push(path);
                }
                continue;
            }
            for neighbor in neighbors.into_iter().rev() {
                let mut next_path = path.clone();
                next_path.push(neighbor);
                let mut next_visited = visited.clone();
                next_visited.insert(neighbor);
                stack.push((neighbor, next_path, next_visited));
            }
        }
    }
    Ok(unique)
}

/// Stage 1 admits exactly one monocycle; fused, bridged, spiro and separate
/// multiple-ring systems have no match here.
pub fn find_ring_systems(mol: &Molecule, exclude_atoms: &AtomSet) -> Vec<RingSystem> {
    let atoms = get_cyclic_atoms(mol, exclude_atoms);
    if atoms.len() < 3
        || atoms.iter().any(|&idx| {
            mol.get_neighbors(idx)
                .iter()
                .filter(|n| atoms.contains(n))
                .count()
                != 2
        })
    {
        return Vec::new();
    }
    let Some(&start) = atoms.first() else {
        return Vec::new();
    };
    let mut current = start;
    let mut path = vec![start];
    let mut visited = AtomSet::from([start]);
    while path.len() < atoms.len() {
        let next = mol
            .get_neighbors(current)
            .iter()
            .filter(|n| atoms.contains(n) && !visited.contains(n))
            .min()
            .copied();
        let Some(next) = next else {
            return Vec::new();
        };
        visited.insert(next);
        path.push(next);
        current = next;
    }
    if mol.get_bond(current, start).is_none() {
        return Vec::new();
    }
    vec![RingSystem {
        atoms,
        paths: vec![path],
    }]
}

fn ranking_order(mol: &Molecule, u: usize, v: usize) -> usize {
    match mol.get_bond(u, v) {
        None => 0,
        Some(_) if mol.atoms[&u].is_aromatic && mol.atoms[&v].is_aromatic => 4,
        Some(bond) => bond.order,
    }
}

fn rank_invariants<T: Ord + Clone>(invariants: BTreeMap<usize, T>) -> BTreeMap<usize, usize> {
    let ordering: BTreeMap<T, usize> = invariants
        .values()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .enumerate()
        .map(|(rank, key)| (key, rank))
        .collect();
    invariants
        .into_iter()
        .map(|(idx, key)| (idx, ordering[&key]))
        .collect()
}

/// Python's Morgan equivalence ranks, including aromatic-order normalization.
pub fn canonical_ranks(mol: &Molecule, atom_ids: Option<&AtomSet>) -> BTreeMap<usize, usize> {
    let all_atoms;
    let atoms = match atom_ids {
        Some(a) => a,
        None => {
            all_atoms = mol.atoms.keys().copied().collect();
            &all_atoms
        }
    };
    let invariants = atoms
        .iter()
        .map(|&idx| {
            let atom = &mol.atoms[&idx];
            let neighbors: Vec<usize> = mol
                .get_neighbors(idx)
                .iter()
                .filter(|n| atoms.contains(n))
                .copied()
                .collect();
            let external: usize = mol
                .get_neighbors(idx)
                .iter()
                .filter(|n| !atoms.contains(n))
                .filter_map(|&n| mol.get_bond(idx, n))
                .map(|b| b.order)
                .sum();
            let mut orders: Vec<usize> = neighbors
                .iter()
                .map(|&n| ranking_order(mol, idx, n))
                .collect();
            orders.sort();
            (
                idx,
                (
                    atom.symbol.clone(),
                    atom.charge,
                    atom.isotope.unwrap_or(0),
                    neighbors.len(),
                    atom.total_h_count + external,
                    atom.is_aromatic,
                    orders,
                ),
            )
        })
        .collect();
    let mut ranks = rank_invariants(invariants);
    let mut previous_classes = ranks.values().collect::<BTreeSet<_>>().len();
    for _ in 0..atoms.len() {
        let refined = atoms
            .iter()
            .map(|&idx| {
                let mut neighbors: Vec<_> = mol
                    .get_neighbors(idx)
                    .iter()
                    .filter(|n| atoms.contains(n))
                    .map(|&n| (ranking_order(mol, idx, n), ranks[&n]))
                    .collect();
                neighbors.sort();
                (idx, (ranks[&idx], neighbors))
            })
            .collect();
        ranks = rank_invariants(refined);
        let classes = ranks.values().collect::<BTreeSet<_>>().len();
        if classes == previous_classes {
            break;
        }
        previous_classes = classes;
    }
    ranks
}

#[cfg(test)]
mod tests {
    use super::super::graph_io::read_smiles;
    use super::*;
    #[test]
    fn carbon_paths_keep_only_maximal_leaf_to_leaf_walks() {
        let mol = read_smiles("CCC(C)CC").unwrap();
        let paths = find_all_carbon_paths(&mol, &AtomSet::new()).unwrap();
        assert_eq!(
            paths,
            vec![vec![0, 1, 2, 3], vec![0, 1, 2, 4, 5], vec![3, 2, 4, 5]]
        );
        assert_eq!(
            paths.iter().map(Vec::len).collect::<Vec<_>>(),
            vec![4, 5, 4]
        );
        assert_eq!(
            paths
                .iter()
                .map(|p| p.iter().copied().collect::<AtomSet>())
                .collect::<BTreeSet<_>>()
                .len(),
            3
        );
    }
    #[test]
    fn ring_atoms_are_removed_from_chain_enumeration() {
        let mol = read_smiles("CCC1CCCCC1").unwrap();
        let excluded = AtomSet::new();
        assert_eq!(get_cyclic_atoms(&mol, &excluded).len(), 6);
        assert_eq!(find_ring_systems(&mol, &excluded)[0].paths[0].len(), 6);
        assert_eq!(
            find_all_carbon_paths(&mol, &excluded)
                .unwrap()
                .iter()
                .map(Vec::len)
                .collect::<Vec<_>>(),
            vec![2]
        );
    }
    #[test]
    fn cutting_a_ring_breaks_its_cycle_membership() {
        let mol = read_smiles("C1CCCCC1").unwrap();
        assert!(get_cyclic_atoms(&mol, &AtomSet::from([0])).is_empty());
    }
    #[test]
    fn aromatic_morgan_ranks_preserve_ring_symmetry() {
        let mol = read_smiles("c1ccccc1").unwrap();
        assert_eq!(
            canonical_ranks(&mol, None)
                .values()
                .collect::<BTreeSet<_>>()
                .len(),
            1
        );
    }

    #[test]
    fn public_naming_bounds_exhaustive_branched_chain_search() {
        use super::super::name_smiles;
        use std::time::{Duration, Instant};

        let mut tree = "C".to_owned();
        for depth in 1..=8 {
            tree = format!("C({tree}){tree}");
            if depth == 3 {
                assert!(name_smiles(&tree).is_ok(), "supported 15-carbon tree");
            }
        }
        let started = Instant::now();
        assert_eq!(
            name_smiles(&tree),
            Err("openclatura: naming complexity limit exceeded".into())
        );
        assert!(started.elapsed() < Duration::from_secs(10));
        assert!(name_smiles(&"C".repeat(100)).is_ok());
        assert_eq!(name_smiles("CCC(C)CC"), Ok("3-methylpentane".into()));
    }
}
