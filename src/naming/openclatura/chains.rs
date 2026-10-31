//! Carbon-chain enumeration and pure spiro ring systems from Python `chains.py`.
use std::collections::{BTreeMap, BTreeSet};

use super::graph_kernel::{EdgeSet, biconnected_edge_components};
use super::molecule::{AtomSet, Molecule};
use super::polycycle_topology::{
    audit_ring_numbering, dedupe_paths, edges_within_atoms, has_multiple_spiro_centers,
    linear_dispiro_descriptor_and_paths, monospiro_proof,
};

#[derive(Clone, Debug, Default)]
pub struct RingSystem {
    pub atoms: AtomSet,
    pub paths: Vec<Vec<usize>>,
    pub is_bicycle: bool,
    pub is_spiro: bool,
    pub is_polycycle: bool,
    pub x: usize,
    pub y: usize,
    pub z: usize,
    pub polycycle_descriptor: Option<String>,
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
    biconnected_edge_components(mol, &valid)
        .into_iter()
        .filter_map(|block| {
            let atoms: AtomSet = block.iter().flat_map(|&(u, v)| [u, v]).collect();
            (block.len() >= atoms.len()).then_some(atoms)
        })
        .flatten()
        .collect()
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

#[derive(Clone)]
struct RingBlock {
    atoms: AtomSet,
    edges: EdgeSet,
    is_monocycle: bool,
}

/// Ring blocks and spiro merging in upstream `find_ring_systems` order.
pub fn find_ring_systems(mol: &Molecule, exclude_atoms: &AtomSet) -> Vec<RingSystem> {
    // Upstream divergence: pure spiro blocks cannot match retained/fusion
    // preflight, so skip it after the input boundary rejects fused/bridged blocks.
    let cyclic = get_cyclic_atoms(mol, exclude_atoms);
    let mut blocks: Vec<_> = biconnected_edge_components(mol, &cyclic)
        .into_iter()
        .filter_map(|edges| {
            let atoms: AtomSet = edges.iter().flat_map(|&(u, v)| [u, v]).collect();
            (edges.len() >= atoms.len()).then_some(RingBlock {
                is_monocycle: edges.len() == atoms.len(),
                atoms,
                edges,
            })
        })
        .collect();
    let mut changed = true;
    while changed {
        changed = false;
        let mut new_blocks = Vec::new();
        while !blocks.is_empty() {
            let first = blocks.remove(0);
            let mut merged = false;
            for second in &mut blocks {
                let shared = first.atoms.intersection(&second.atoms).count();
                let merged_atoms: AtomSet = first.atoms.union(&second.atoms).copied().collect();
                let merged_edges: EdgeSet = first.edges.union(&second.edges).copied().collect();
                if (shared == 1 && first.is_monocycle && second.is_monocycle)
                    || (shared != 0 && has_multiple_spiro_centers(&merged_atoms, &merged_edges))
                {
                    *second = RingBlock {
                        atoms: merged_atoms,
                        edges: merged_edges,
                        is_monocycle: false,
                    };
                    merged = true;
                    changed = true;
                    break;
                }
            }
            if !merged {
                new_blocks.push(first);
            }
        }
        blocks = new_blocks;
    }
    let mut systems = Vec::new();
    for block in blocks {
        let RingBlock { atoms, edges, .. } = block;
        if edges.len() == atoms.len() + 1 {
            if let Some(proof) = monospiro_proof(&atoms, &edges) {
                let paths = dedupe_paths(
                    proof
                        .numbering_paths
                        .into_iter()
                        .filter(|path| {
                            audit_ring_numbering("spiro", &proof.descriptor_numbers, path, &edges)
                        })
                        .collect(),
                );
                if !paths.is_empty() {
                    systems.push(RingSystem {
                        atoms,
                        paths,
                        is_spiro: true,
                        x: proof.descriptor_numbers[0],
                        y: proof.descriptor_numbers[1],
                        ..RingSystem::default()
                    });
                }
            }
            continue;
        }
        if edges.len() >= atoms.len() + 2 {
            if let Some((descriptor, paths)) =
                linear_dispiro_descriptor_and_paths(mol, &atoms, &edges)
            {
                systems.push(RingSystem {
                    atoms,
                    paths,
                    is_polycycle: true,
                    polycycle_descriptor: Some(descriptor),
                    ..RingSystem::default()
                });
            } else {
                // Upstream divergence: the general von Baeyer fallback is not
                // ported. Retain the full boundary so a larger spiro system
                // without an audited descriptor declines during assembly.
                systems.push(RingSystem {
                    paths: vec![atoms.iter().copied().collect()],
                    atoms,
                    is_polycycle: true,
                    ..RingSystem::default()
                });
            }
            continue;
        }
        if atoms.len() < 3 || edges.len() != atoms.len() {
            continue;
        }
        let mut adjacency: BTreeMap<usize, AtomSet> = BTreeMap::new();
        for (u, v) in edges {
            adjacency.entry(u).or_default().insert(v);
            adjacency.entry(v).or_default().insert(u);
        }
        if adjacency.values().any(|neighbors| neighbors.len() != 2) {
            continue;
        }
        let Some(&start) = atoms.first() else {
            continue;
        };
        let mut current = start;
        let mut path = vec![start];
        let mut visited = AtomSet::from([start]);
        while path.len() < atoms.len() {
            let next = adjacency[&current]
                .iter()
                .find(|neighbor| !visited.contains(neighbor))
                .copied();
            let Some(next) = next else {
                break;
            };
            visited.insert(next);
            path.push(next);
            current = next;
        }
        if path.len() == atoms.len() && adjacency[&current].contains(&start) {
            systems.push(RingSystem {
                atoms,
                paths: vec![path],
                ..RingSystem::default()
            });
        }
    }
    merge_polyspiro_ring_systems(mol, systems)
}

/// Keep upstream's final overlap merge for ring systems split around a
/// multi-spiro parent. Separate cycles connected by a bridge never overlap.
fn merge_polyspiro_ring_systems(mol: &Molecule, systems: Vec<RingSystem>) -> Vec<RingSystem> {
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    for (idx, system) in systems.iter().enumerate() {
        if !seen.insert(idx) {
            continue;
        }
        let mut group_indexes = BTreeSet::from([idx]);
        let mut queue = std::collections::VecDeque::from([idx]);
        while let Some(current_idx) = queue.pop_front() {
            for (other_idx, other) in systems.iter().enumerate() {
                if !seen.contains(&other_idx)
                    && !systems[current_idx].atoms.is_disjoint(&other.atoms)
                {
                    seen.insert(other_idx);
                    group_indexes.insert(other_idx);
                    queue.push_back(other_idx);
                }
            }
        }
        if group_indexes.len() == 1 {
            result.push(system.clone());
            continue;
        }
        let union_atoms: AtomSet = group_indexes
            .iter()
            .flat_map(|&index| systems[index].atoms.iter().copied())
            .collect();
        let union_edges = edges_within_atoms(mol, &union_atoms);
        if has_multiple_spiro_centers(&union_atoms, &union_edges)
            && let Some((descriptor, paths)) =
                linear_dispiro_descriptor_and_paths(mol, &union_atoms, &union_edges)
        {
            result.push(RingSystem {
                atoms: union_atoms,
                paths,
                is_polycycle: true,
                polycycle_descriptor: Some(descriptor),
                ..RingSystem::default()
            });
            continue;
        }
        result.extend(
            group_indexes
                .into_iter()
                .map(|index| systems[index].clone()),
        );
    }
    result
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
    fn separate_ring_systems_follow_upstream_block_and_walk_order() {
        let mol = read_smiles("C1(C2CCC2)CCC1").unwrap();
        let systems = find_ring_systems(&mol, &AtomSet::new());
        assert_eq!(
            systems
                .iter()
                .map(|system| system.paths.clone())
                .collect::<Vec<_>>(),
            vec![vec![vec![0, 5, 6, 7]], vec![vec![1, 2, 3, 4]]]
        );
        // The bridge between two cyclic atoms belongs to neither ring block.
        assert_eq!(systems[0].atoms, AtomSet::from([0, 5, 6, 7]));
        assert_eq!(systems[1].atoms, AtomSet::from([1, 2, 3, 4]));

        let remaining = find_ring_systems(&mol, &AtomSet::from([0]));
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].paths, vec![vec![1, 2, 3, 4]]);
    }

    #[test]
    fn linking_chain_atoms_are_not_cyclic() {
        let mol = read_smiles("C1(CC2CCC2)CCC1").unwrap();
        assert_eq!(
            get_cyclic_atoms(&mol, &AtomSet::new()),
            AtomSet::from([0, 2, 3, 4, 5, 6, 7, 8])
        );
        assert_eq!(
            find_all_carbon_paths(&mol, &AtomSet::new()).unwrap(),
            vec![vec![1]]
        );
    }

    #[test]
    fn pure_spiro_merging_preserves_separate_ring_systems() {
        let mol = read_smiles("C1CC1CC2CC23CC3").unwrap();
        let systems = find_ring_systems(&mol, &AtomSet::new());
        assert_eq!(systems.len(), 2);
        assert_eq!(systems[0].atoms, AtomSet::from([0, 1, 2]));
        assert_eq!(systems[0].paths, vec![vec![0, 1, 2]]);
        assert!(!systems[0].is_spiro);
        assert_eq!(systems[1].atoms, AtomSet::from([4, 5, 6, 7, 8]));
        assert!(systems[1].is_spiro);
        assert_eq!((systems[1].x, systems[1].y), (2, 2));
        assert_eq!(
            systems[1].paths,
            vec![
                vec![4, 5, 6, 7, 8],
                vec![4, 5, 6, 8, 7],
                vec![5, 4, 6, 7, 8],
                vec![5, 4, 6, 8, 7],
                vec![7, 8, 6, 4, 5],
                vec![7, 8, 6, 5, 4],
                vec![8, 7, 6, 4, 5],
                vec![8, 7, 6, 5, 4],
            ]
        );
        assert_eq!(get_cyclic_atoms(&mol, &AtomSet::new()).len(), 8);
        assert_eq!(
            find_all_carbon_paths(&mol, &AtomSet::new()).unwrap(),
            vec![vec![3]]
        );
    }

    #[test]
    fn dispiro_is_one_complete_descriptor_parent() {
        let mol = read_smiles("C1CC2(C1)CCC1(CC2)CC1").unwrap();
        let systems = find_ring_systems(&mol, &AtomSet::new());
        assert_eq!(systems.len(), 1);
        assert!(systems[0].is_polycycle);
        assert_eq!(
            systems[0].polycycle_descriptor.as_deref(),
            Some("dispiro[2.2.3.2]")
        );
        assert_eq!(systems[0].atoms.len(), 11);
        assert_eq!(systems[0].paths.len(), 4);
        assert!(systems[0].paths.iter().all(|path| path.len() == 11));
    }

    #[test]
    fn unsupported_larger_spiro_retains_its_whole_parent_boundary() {
        let mol = read_smiles("C1CC12CC23CC34CC4").unwrap();
        let systems = find_ring_systems(&mol, &AtomSet::new());
        assert_eq!(systems.len(), 1);
        assert!(systems[0].is_polycycle);
        assert!(systems[0].polycycle_descriptor.is_none());
        assert_eq!(systems[0].atoms.len(), mol.atoms.len());
        assert_eq!(systems[0].paths[0].len(), mol.atoms.len());
    }

    #[test]
    #[ignore = "development topology parity against installed Python oracle TSV"]
    fn spiro_topology_parity_file() {
        let path = std::env::var("OPENCLATURA_TOPOLOGY").expect("OPENCLATURA_TOPOLOGY TSV path");
        let input = std::fs::read_to_string(path).expect("read topology oracle");
        let mut differences = Vec::new();
        for (index, line) in input.lines().enumerate() {
            let (smiles, expected) = line.split_once('\t').expect("SMILES and topology JSON");
            let mol = read_smiles(smiles)
                .unwrap_or_else(|error| panic!("row {index}: {smiles}: {error}"));
            let systems = find_ring_systems(&mol, &AtomSet::new());
            let actual = serde_json::Value::Array(
                systems
                    .iter()
                    .map(|system| {
                        serde_json::json!({
                            "atoms": system.atoms,
                            "paths": system.paths,
                            "is_spiro": system.is_spiro,
                            "is_polycycle": system.is_polycycle,
                            "x": system.x,
                            "y": system.y,
                            "descriptor": system.polycycle_descriptor,
                        })
                    })
                    .collect(),
            );
            let expected: serde_json::Value =
                serde_json::from_str(expected).expect("topology JSON");
            if actual != expected {
                differences.push(format!(
                    "row {index}: {smiles}\nexpected={expected}\nactual={actual}"
                ));
            }
        }
        assert!(
            differences.is_empty(),
            "{} topology differences:\n{}",
            differences.len(),
            differences.join("\n")
        );
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
