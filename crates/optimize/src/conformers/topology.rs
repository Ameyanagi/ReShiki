//! Validate molecular connectivity and enumerate heavy-atom graph automorphisms.

use std::collections::{BTreeSet, VecDeque};

use super::Error;

const AUTOMORPHISM_ASSIGNMENT_BUDGET: usize = 200_000;

/// Molecular connectivity used to distinguish chemically equivalent heavy atoms.
#[derive(Clone, Debug)]
pub struct Topology {
    atom_count: usize,
    heavy_atoms: Vec<usize>,
    adjacency: Vec<Vec<usize>>,
    colors: Vec<usize>,
}

impl Topology {
    /// Validate atomic numbers and undirected bonds, retaining a heavy-atom graph.
    pub fn new(numbers: Vec<u8>, bonds: Vec<[usize; 2]>) -> Result<Self, Error> {
        if numbers.is_empty() {
            return Err(Error::Invalid(
                "topology must contain at least one atom".into(),
            ));
        }
        if let Some((index, number)) = numbers
            .iter()
            .enumerate()
            .find(|(_, number)| !(1..=118).contains(*number))
        {
            return Err(Error::Invalid(format!(
                "atom {index} has invalid atomic number {number}"
            )));
        }

        let mut canonical_bonds = Vec::with_capacity(bonds.len());
        for [first, second] in bonds {
            if first >= numbers.len() || second >= numbers.len() {
                return Err(Error::Invalid(format!(
                    "bond [{first}, {second}] has an atom index outside the topology"
                )));
            }
            if first == second {
                return Err(Error::Invalid(format!(
                    "atom {first} cannot be bonded to itself"
                )));
            }
            canonical_bonds.push([first.min(second), first.max(second)]);
        }
        canonical_bonds.sort_unstable();
        if let Some(pair) = canonical_bonds.windows(2).find(|pair| pair[0] == pair[1]) {
            return Err(Error::Invalid(format!(
                "duplicate bond [{}, {}]",
                pair[0][0], pair[0][1]
            )));
        }

        let heavy_atoms: Vec<_> = numbers
            .iter()
            .enumerate()
            .filter_map(|(index, &number)| (number != 1).then_some(index))
            .collect();
        if heavy_atoms.len().saturating_sub(1) > u32::MAX as usize {
            return Err(Error::Invalid(
                "too many heavy atoms to represent an automorphism".into(),
            ));
        }
        let mut heavy_positions = vec![usize::MAX; numbers.len()];
        for (position, &atom) in heavy_atoms.iter().enumerate() {
            heavy_positions[atom] = position;
        }
        let mut adjacency = vec![Vec::new(); heavy_atoms.len()];
        let mut hydrogen_counts = vec![0usize; heavy_atoms.len()];
        for [first, second] in canonical_bonds {
            match (numbers[first] == 1, numbers[second] == 1) {
                (false, false) => {
                    let first = heavy_positions[first];
                    let second = heavy_positions[second];
                    adjacency[first].push(second);
                    adjacency[second].push(first);
                }
                (false, true) => hydrogen_counts[heavy_positions[first]] += 1,
                (true, false) => hydrogen_counts[heavy_positions[second]] += 1,
                (true, true) => {}
            }
        }
        for neighbors in &mut adjacency {
            neighbors.sort_unstable();
        }
        let initial_keys: Vec<_> = heavy_atoms
            .iter()
            .enumerate()
            .map(|(position, &atom)| {
                (
                    numbers[atom],
                    adjacency[position].len(),
                    hydrogen_counts[position],
                )
            })
            .collect();
        let mut colors = color_ids(&initial_keys);
        loop {
            let refined_keys: Vec<_> = adjacency
                .iter()
                .enumerate()
                .map(|(position, neighbors)| {
                    let mut neighbor_colors: Vec<_> =
                        neighbors.iter().map(|&neighbor| colors[neighbor]).collect();
                    neighbor_colors.sort_unstable();
                    (colors[position], neighbor_colors)
                })
                .collect();
            let refined = color_ids(&refined_keys);
            if refined == colors {
                break;
            }
            colors = refined;
        }

        Ok(Self {
            atom_count: numbers.len(),
            heavy_atoms,
            adjacency,
            colors,
        })
    }

    /// Original atom indices of all non-hydrogen atoms, in ascending order.
    pub fn heavy_atoms(&self) -> &[usize] {
        &self.heavy_atoms
    }

    pub(super) fn atom_count(&self) -> usize {
        self.atom_count
    }

    /// Enumerate up to `cap` symmetry mappings, then add their missing inverses.
    ///
    /// Each permutation maps positions in [`Self::heavy_atoms`] to positions in
    /// the same list. Identity comes first, followed by deterministic breadth-first
    /// backtracking order, then missing inverses in that same order. At most
    /// 200,000 candidate assignments are examined, and inverse completion can
    /// retain up to twice `cap` mappings so RMSD remains symmetric after truncation.
    /// Hydrogen-only topologies have one empty mapping when `cap` is positive.
    pub fn automorphisms(&self, cap: usize) -> Automorphisms {
        if cap == 0 {
            return Automorphisms {
                perms: Vec::new(),
                truncated: true,
            };
        }
        let identity = (0..self.heavy_atoms.len())
            .map(|index| index as u32)
            .collect();
        let mut available_by_color =
            vec![BTreeSet::new(); self.colors.iter().max().map_or(0, |max| max + 1)];
        for (target, &color) in self.colors.iter().enumerate() {
            available_by_color[color].insert(target);
        }
        let mut search = AutomorphismSearch {
            topology: self,
            order: self.bfs_order(),
            available_by_color,
            mapping: vec![usize::MAX; self.heavy_atoms.len()],
            inverse: vec![usize::MAX; self.heavy_atoms.len()],
            cap,
            result: Automorphisms {
                perms: vec![identity],
                truncated: false,
            },
        };
        search.enumerate();
        if search.result.truncated {
            let retained = search.result.perms.len();
            let mut seen: BTreeSet<_> = search.result.perms.iter().cloned().collect();
            for i in 0..retained {
                let mut inverse = vec![0; self.heavy_atoms.len()];
                for (source, &target) in search.result.perms[i].iter().enumerate() {
                    inverse[target as usize] = source as u32;
                }
                if seen.insert(inverse.clone()) {
                    search.result.perms.push(inverse);
                }
            }
        }
        search.result
    }

    fn bfs_order(&self) -> Vec<usize> {
        let mut order = Vec::with_capacity(self.heavy_atoms.len());
        let mut visited = vec![false; self.heavy_atoms.len()];
        let mut queue = VecDeque::new();
        for root in 0..self.heavy_atoms.len() {
            if visited[root] {
                continue;
            }
            visited[root] = true;
            queue.push_back(root);
            while let Some(atom) = queue.pop_front() {
                order.push(atom);
                for &neighbor in &self.adjacency[atom] {
                    if !visited[neighbor] {
                        visited[neighbor] = true;
                        queue.push_back(neighbor);
                    }
                }
            }
        }
        order
    }
}

/// Heavy-atom symmetry mappings and whether enumeration was incomplete.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Automorphisms {
    perms: Vec<Vec<u32>>,
    truncated: bool,
}

impl Automorphisms {
    /// Mappings between positions in [`Topology::heavy_atoms`], identity first.
    pub fn perms(&self) -> &[Vec<u32>] {
        &self.perms
    }

    /// Whether enumeration omitted mappings at the cap or exhausted its work budget.
    ///
    /// This describes enumeration before inverse completion, which can restore
    /// omitted mappings. When the budget is exhausted, more mappings may or may not exist.
    pub fn truncated(&self) -> bool {
        self.truncated
    }
}

fn color_ids<T: Clone + Ord>(keys: &[T]) -> Vec<usize> {
    let mut unique = keys.to_vec();
    unique.sort_unstable();
    unique.dedup();
    keys.iter()
        .map(|key| unique.binary_search(key).expect("key is present"))
        .collect()
}

struct AutomorphismSearch<'a> {
    topology: &'a Topology,
    order: Vec<usize>,
    available_by_color: Vec<BTreeSet<usize>>,
    mapping: Vec<usize>,
    inverse: Vec<usize>,
    cap: usize,
    result: Automorphisms,
}

impl AutomorphismSearch<'_> {
    fn enumerate(&mut self) {
        if self.order.is_empty() {
            return;
        }
        // Explicit cursors keep the native call stack independent of atom count.
        let mut next_target = vec![0; self.order.len()];
        let mut depth = 0;
        let mut assignments_remaining = AUTOMORPHISM_ASSIGNMENT_BUDGET;
        loop {
            if depth == self.order.len() {
                // Identity is already present, even when BFS encounters it later.
                if !self
                    .mapping
                    .iter()
                    .enumerate()
                    .all(|(atom, &image)| atom == image)
                {
                    if self.result.perms.len() == self.cap {
                        self.result.truncated = true;
                        return;
                    }
                    self.result
                        .perms
                        .push(self.mapping.iter().map(|&image| image as u32).collect());
                }
                depth -= 1;
                self.unassign(self.order[depth]);
                continue;
            }

            let source = self.order[depth];
            let color = self.topology.colors[source];
            let mut candidate = None;
            while let Some(target) = self.available_by_color[color]
                .range(next_target[depth]..)
                .next()
                .copied()
            {
                if assignments_remaining == 0 {
                    self.result.truncated = true;
                    return;
                }
                assignments_remaining -= 1;
                next_target[depth] = target + 1;
                if self.preserves_adjacency(source, target) {
                    candidate = Some(target);
                    break;
                }
            }
            if let Some(target) = candidate {
                self.mapping[source] = target;
                self.inverse[target] = source;
                self.available_by_color[color].remove(&target);
                depth += 1;
                if depth < self.order.len() {
                    next_target[depth] = 0;
                }
            } else {
                if depth == 0 {
                    return;
                }
                depth -= 1;
                self.unassign(self.order[depth]);
            }
        }
    }

    fn unassign(&mut self, source: usize) {
        let target = self.mapping[source];
        self.available_by_color[self.topology.colors[source]].insert(target);
        self.inverse[target] = usize::MAX;
        self.mapping[source] = usize::MAX;
    }

    fn preserves_adjacency(&self, source: usize, target: usize) -> bool {
        // Check both directions so an extra edge to an assigned target is rejected.
        self.topology.adjacency[source].iter().all(|&neighbor| {
            let image = self.mapping[neighbor];
            image == usize::MAX
                || self.topology.adjacency[target]
                    .binary_search(&image)
                    .is_ok()
        }) && self.topology.adjacency[target].iter().all(|&neighbor| {
            let preimage = self.inverse[neighbor];
            preimage == usize::MAX
                || self.topology.adjacency[source]
                    .binary_search(&preimage)
                    .is_ok()
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    fn with_hydrogens(
        heavy_numbers: &[u8],
        heavy_bonds: &[[usize; 2]],
        hydrogens: &[usize],
    ) -> Topology {
        assert_eq!(heavy_numbers.len(), hydrogens.len());
        let mut numbers = heavy_numbers.to_vec();
        let mut bonds = heavy_bonds.to_vec();
        for (atom, &count) in hydrogens.iter().enumerate() {
            for _ in 0..count {
                bonds.push([atom, numbers.len()]);
                numbers.push(1);
            }
        }
        Topology::new(numbers, bonds).unwrap()
    }

    fn assert_group_size(topology: &Topology, size: usize) {
        let group = topology.automorphisms(size + 1);
        assert_eq!(group.perms().len(), size);
        assert!(!group.truncated());
        assert_eq!(
            group.perms()[0],
            (0..topology.heavy_atoms().len())
                .map(|index| index as u32)
                .collect::<Vec<_>>()
        );
        assert_eq!(group.perms().iter().collect::<BTreeSet<_>>().len(), size);
    }

    #[test]
    fn benzene_and_cyclohexane_have_twelve_automorphisms() {
        let ring = [[0, 1], [1, 2], [2, 3], [3, 4], [4, 5], [5, 0]];
        assert_group_size(&with_hydrogens(&[6; 6], &ring, &[1; 6]), 12);
        assert_group_size(&with_hydrogens(&[6; 6], &ring, &[2; 6]), 12);
    }

    #[test]
    fn toluene_has_two_automorphisms() {
        let bonds = [[0, 1], [1, 2], [2, 3], [3, 4], [4, 5], [5, 0], [0, 6]];
        assert_group_size(&with_hydrogens(&[6; 7], &bonds, &[0, 1, 1, 1, 1, 1, 3]), 2);
    }

    #[test]
    fn tert_butanol_has_six_automorphisms() {
        let bonds = [[0, 1], [0, 2], [0, 3], [0, 4]];
        assert_group_size(
            &with_hydrogens(&[6, 6, 6, 6, 8], &bonds, &[0, 3, 3, 3, 1]),
            6,
        );
    }

    #[test]
    fn neopentane_has_twenty_four_automorphisms() {
        let bonds = [[0, 1], [0, 2], [0, 3], [0, 4]];
        assert_group_size(&with_hydrogens(&[6; 5], &bonds, &[0, 3, 3, 3, 3]), 24);
    }

    #[test]
    fn acetic_acid_hydrogen_distinguishes_oxygen_atoms() {
        let bonds = [[0, 1], [1, 2], [1, 3]];
        assert_group_size(&with_hydrogens(&[6, 6, 8, 8], &bonds, &[3, 0, 0, 1]), 1);
    }

    #[test]
    fn caps_report_only_genuine_omissions() {
        let topology = Topology::new(vec![6, 6, 6], vec![[0, 1], [1, 2], [2, 0]]).unwrap();
        let zero = topology.automorphisms(0);
        assert!(zero.perms().is_empty());
        assert!(zero.truncated());
        let small = topology.automorphisms(2);
        assert_eq!(small.perms().len(), 2);
        assert_eq!(small.perms()[0], vec![0, 1, 2]);
        assert!(small.truncated());
        let exact = topology.automorphisms(6);
        assert_eq!(exact.perms().len(), 6);
        assert!(!exact.truncated());
        assert_eq!(small, topology.automorphisms(2));
        let asymmetric = Topology::new(vec![6, 8], vec![[0, 1]])
            .unwrap()
            .automorphisms(1);
        assert_eq!(asymmetric.perms(), vec![vec![0, 1]]);
        assert!(!asymmetric.truncated());
    }

    #[test]
    fn truncated_mappings_include_each_inverse_once() {
        let topology = Topology::new(vec![7, 6, 6, 6], vec![[0, 1], [0, 2], [0, 3]]).unwrap();
        let group = topology.automorphisms(4);
        assert!(group.truncated());
        assert_eq!(group.perms().len(), 5);
        assert_eq!(group.perms()[0], vec![0, 1, 2, 3]);
        let unique: BTreeSet<_> = group.perms().iter().collect();
        assert_eq!(unique.len(), group.perms().len());
        for permutation in group.perms() {
            let mut inverse = vec![0; permutation.len()];
            for (source, &target) in permutation.iter().enumerate() {
                inverse[target as usize] = source as u32;
            }
            assert!(unique.contains(&inverse));
        }
        assert_eq!(group, topology.automorphisms(4));
    }

    #[test]
    fn hydrogen_only_topology_has_one_empty_mapping() {
        let topology = Topology::new(vec![1, 1], vec![[0, 1]]).unwrap();
        assert!(topology.heavy_atoms().is_empty());
        assert_eq!(
            topology.automorphisms(1),
            Automorphisms {
                perms: vec![vec![]],
                truncated: false
            }
        );
        assert!(topology.automorphisms(0).truncated());
    }

    #[test]
    fn heavy_indices_are_original_atom_indices_and_permutations_are_dense() {
        let topology = Topology::new(vec![1, 6, 1, 6, 1], vec![[1, 3], [0, 1], [3, 4]]).unwrap();
        assert_eq!(topology.atom_count(), 5);
        assert_eq!(topology.heavy_atoms(), &[1, 3]);
        assert_eq!(
            topology.automorphisms(2).perms(),
            vec![vec![0, 1], vec![1, 0]]
        );
    }

    #[test]
    fn disconnected_components_can_exchange() {
        assert_group_size(&Topology::new(vec![6; 4], vec![[0, 2], [1, 3]]).unwrap(), 8);
    }

    #[test]
    fn bond_order_does_not_change_enumeration() {
        let first = Topology::new(vec![6; 4], vec![[0, 3], [3, 1], [1, 2], [2, 0]]).unwrap();
        let second = Topology::new(vec![6; 4], vec![[0, 2], [2, 1], [1, 3], [3, 0]]).unwrap();
        assert_eq!(first.automorphisms(8), second.automorphisms(8));
    }

    #[test]
    fn ten_thousand_heavy_atoms_do_not_need_recursive_search() {
        let topology = Topology::new(vec![6; 10_000], vec![]).unwrap();
        let group = topology.automorphisms(1);
        assert_eq!(group.perms().len(), 1);
        assert_eq!(group.perms()[0].len(), 10_000);
        assert_eq!(group.perms()[0][9_999], 9_999);
        assert!(group.truncated());
    }

    #[test]
    fn large_cubic_graph_has_bounded_search_and_valid_partial_mappings() {
        // A 60-atom Möbius ladder is connected and cubic, so color refinement
        // leaves all atoms equivalent and backtracking has substantial work.
        let mut bonds: Vec<_> = (0..60).map(|atom| [atom, (atom + 1) % 60]).collect();
        bonds.extend((0..30).map(|atom| [atom, atom + 30]));
        let topology = Topology::new(vec![6; 60], bonds).unwrap();
        // An unrestricted output cap isolates truncation due to the work budget.
        let group = topology.automorphisms(usize::MAX);
        assert!(group.truncated());
        assert!(group.perms().len() > 1);
        let identity: Vec<u32> = (0..60).collect();
        assert_eq!(group.perms()[0], identity);
        assert_eq!(
            group.perms().iter().collect::<BTreeSet<_>>().len(),
            group.perms().len()
        );
        for permutation in group.perms() {
            let mut images = permutation.clone();
            images.sort_unstable();
            assert_eq!(images, identity);
            for (source, &target) in permutation.iter().enumerate() {
                let mut mapped_neighbors: Vec<_> = topology.adjacency[source]
                    .iter()
                    .map(|&neighbor| permutation[neighbor] as usize)
                    .collect();
                mapped_neighbors.sort_unstable();
                assert_eq!(mapped_neighbors, topology.adjacency[target as usize]);
            }
        }
    }

    #[test]
    fn invalid_topologies_are_rejected() {
        for (numbers, bonds) in [
            (vec![], vec![]),
            (vec![0], vec![]),
            (vec![119], vec![]),
            (vec![6], vec![[0, 1]]),
            (vec![6], vec![[0, 0]]),
            (vec![6, 8], vec![[0, 1], [1, 0]]),
        ] {
            assert!(matches!(
                Topology::new(numbers, bonds),
                Err(Error::Invalid(_))
            ));
        }
        assert!(Topology::new(vec![1, 118], vec![]).is_ok());
    }
}
