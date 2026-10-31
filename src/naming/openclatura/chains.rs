//! Carbon-chain enumeration and audited ring systems from Python `chains.py`.
use std::collections::{BTreeMap, BTreeSet};

use super::graph_kernel::{EdgeSet, adjacency_from_edges, biconnected_edge_components};
use super::molecule::{AtomSet, Molecule};
use super::polycycle_topology::{
    RingNumbering, audit_ring_numbering, bicyclo_proof, build_ring_numbering,
    build_von_baeyer_numbering, dedupe_paths, edges_within_atoms, has_multiple_spiro_centers,
    linear_dispiro_descriptor_and_paths, monospiro_proof,
};
use super::python_int_set::PythonIntSet;
use super::ring_renderer::is_von_baeyer_descriptor;

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

#[derive(Clone, Debug)]
pub struct PolycycleDescriptorCandidate {
    pub descriptor: Option<String>,
    pub paths: Vec<Vec<usize>>,
    pub is_von_baeyer: bool,
    pub numberings: Vec<RingNumbering>,
}

impl PolycycleDescriptorCandidate {
    pub fn descriptor_allowed(&self) -> bool {
        !self.is_von_baeyer || !self.numberings.is_empty()
    }
}

pub fn get_von_baeyer_descriptor_candidates(
    atoms: &AtomSet,
    edges: &EdgeSet,
) -> Vec<super::von_baeyer::DescriptorPaths> {
    super::von_baeyer::descriptor_candidates(atoms, edges)
}

fn audited_von_baeyer_numberings(
    descriptor: &str,
    paths: &[Vec<usize>],
    edges: &EdgeSet,
) -> Vec<RingNumbering> {
    let mut seen = BTreeSet::new();
    paths
        .iter()
        .map(|path| build_von_baeyer_numbering(descriptor, path, edges))
        .filter(|numbering| numbering.audit_ok && seen.insert(numbering.path.clone()))
        .collect()
}

/// Preserve every reconstructable numbering of the first accepted descriptor,
/// so heteroatom, unsaturation and substituent rules settle its orientation.
pub fn polyspiro_or_von_baeyer_candidate(
    mol: &Molecule,
    atoms: &AtomSet,
    edges: &EdgeSet,
) -> PolycycleDescriptorCandidate {
    if let Some((descriptor, paths)) = linear_dispiro_descriptor_and_paths(mol, atoms, edges) {
        return PolycycleDescriptorCandidate {
            descriptor: Some(descriptor),
            paths,
            is_von_baeyer: false,
            numberings: Vec::new(),
        };
    }
    let ranks = canonical_ranks(mol, None);
    let mut canonical: Vec<_> = atoms.iter().copied().collect();
    canonical.sort_by_key(|atom| ranks[atom]);
    let original: Vec<_> = atoms.iter().copied().collect();
    let mut legacy_descriptor = None;
    let mut legacy_paths = vec![original.clone()];
    for (ordering_index, ordered) in [canonical, original].into_iter().enumerate() {
        let ids: BTreeMap<_, _> = ordered
            .iter()
            .enumerate()
            .map(|(i, &atom)| (atom, i))
            .collect();
        let canonical_atoms = (0..ordered.len()).collect();
        let canonical_edges: EdgeSet = edges
            .iter()
            .map(|&(u, v)| {
                let u = ids[&u];
                let v = ids[&v];
                (u.min(v), u.max(v))
            })
            .collect();
        let mut accepted_descriptor = None;
        let mut accepted = Vec::new();
        for (index, (descriptor, paths)) in
            get_von_baeyer_descriptor_candidates(&canonical_atoms, &canonical_edges)
                .into_iter()
                .enumerate()
        {
            let restored: Vec<Vec<_>> = paths
                .iter()
                .map(|path| path.iter().map(|&i| ordered[i]).collect())
                .collect();
            if index == 0 && ordering_index == 0 {
                legacy_descriptor = descriptor.clone();
                legacy_paths = restored.clone();
            }
            if index >= 256 {
                break;
            }
            let Some(descriptor) =
                descriptor.filter(|descriptor| is_von_baeyer_descriptor(Some(descriptor)))
            else {
                continue;
            };
            let numberings = audited_von_baeyer_numberings(&descriptor, &restored, edges);
            if numberings.is_empty() {
                continue;
            }
            if accepted_descriptor.is_none() {
                accepted_descriptor = Some(descriptor.clone());
            }
            if accepted_descriptor.as_deref() == Some(&descriptor) {
                accepted.extend(numberings);
            }
        }
        if accepted_descriptor.is_some() {
            let mut seen = BTreeSet::new();
            accepted.retain(|numbering| seen.insert(numbering.path.clone()));
            return PolycycleDescriptorCandidate {
                descriptor: accepted_descriptor,
                paths: accepted
                    .iter()
                    .map(|numbering| numbering.path.clone())
                    .collect(),
                is_von_baeyer: true,
                numberings: accepted,
            };
        }
    }
    if !atoms.iter().any(|atom| mol.atoms[atom].symbol == "Si") {
        let candidates = super::von_baeyer::find_von_baeyer_candidates(mol, atoms, edges);
        if let Some(first) = candidates.first() {
            let descriptor = first.descriptor.clone();
            let mut numberings = if legacy_descriptor.as_deref() == Some(&descriptor) {
                audited_von_baeyer_numberings(&descriptor, &legacy_paths, edges)
            } else {
                Vec::new()
            };
            numberings.extend(
                candidates
                    .into_iter()
                    .filter(|candidate| candidate.descriptor == descriptor)
                    .map(|candidate| candidate.numbering),
            );
            let mut seen = BTreeSet::new();
            numberings.retain(|numbering| seen.insert(numbering.path.clone()));
            return PolycycleDescriptorCandidate {
                descriptor: Some(descriptor),
                paths: numberings
                    .iter()
                    .map(|numbering| numbering.path.clone())
                    .collect(),
                is_von_baeyer: true,
                numberings,
            };
        }
    }
    let is_von_baeyer = is_von_baeyer_descriptor(legacy_descriptor.as_deref());
    let numberings = legacy_descriptor
        .as_ref()
        .filter(|_| is_von_baeyer)
        .map_or_else(Vec::new, |descriptor| {
            audited_von_baeyer_numberings(descriptor, &legacy_paths, edges)
        });
    let paths = if is_von_baeyer {
        numberings
            .iter()
            .map(|numbering| numbering.path.clone())
            .collect()
    } else {
        legacy_paths
    };
    PolycycleDescriptorCandidate {
        descriptor: legacy_descriptor,
        paths,
        is_von_baeyer,
        numberings,
    }
}

fn first_retained_path(mol: &Molecule, paths: &[Vec<usize>]) -> Option<Vec<usize>> {
    paths
        .iter()
        .find(|path| super::rules::retained::get_retained_ring(mol, path).is_some())
        .cloned()
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
    // Upstream traverses the set comprehension's table when choosing the
    // first endpoint. Complete numbering ties retain that path orientation,
    // including N/N' assignments on equivalent terminal principal groups.
    let traversal: PythonIntSet = valid.iter().copied().collect();
    let endpoints: Vec<usize> = traversal
        .iter()
        .filter(|&idx| {
            mol.get_neighbors(idx)
                .iter()
                .filter(|n| valid.contains(n))
                .count()
                <= 1
        })
        .collect();
    let starts: Vec<usize> = if endpoints.is_empty() {
        traversal.iter().collect()
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
pub fn find_ring_systems(
    mol: &Molecule,
    exclude_atoms: &AtomSet,
) -> Result<Vec<RingSystem>, String> {
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
                    let paths = first_retained_path(mol, &paths).map_or(paths, |path| vec![path]);
                    systems.push(RingSystem {
                        atoms,
                        paths,
                        is_spiro: true,
                        x: proof.descriptor_numbers[0],
                        y: proof.descriptor_numbers[1],
                        ..RingSystem::default()
                    });
                }
            } else if let Some(proof) = bicyclo_proof(&atoms, &edges) {
                let mut paths = dedupe_paths(
                    proof
                        .numbering_paths
                        .into_iter()
                        .filter(|path| {
                            build_ring_numbering("bicyclo", &proof.descriptor_numbers, path, &edges)
                                .audit_ok
                        })
                        .collect(),
                );
                let fallback = fallback_bicyclo_paths(&atoms, &edges, proof.bridgeheads);
                let fallback: Vec<_> = fallback
                    .into_iter()
                    .filter(|path| {
                        build_ring_numbering("bicyclo", &proof.descriptor_numbers, path, &edges)
                            .audit_ok
                    })
                    .collect();
                let plain_hydrocarbon = atoms
                    .iter()
                    .all(|atom| mol.atoms[atom].symbol == "C" && mol.atoms[atom].charge == 0)
                    && edges
                        .iter()
                        .all(|&(u, v)| mol.get_bond(u, v).is_none_or(|bond| bond.order == 1));
                if plain_hydrocarbon {
                    paths.extend(fallback);
                    paths = dedupe_paths(paths);
                } else if !fallback.is_empty() {
                    paths = fallback;
                }
                if !paths.is_empty() {
                    let paths = first_retained_path(mol, &paths).map_or(paths, |path| vec![path]);
                    systems.push(RingSystem {
                        atoms,
                        paths,
                        is_bicycle: true,
                        x: proof.descriptor_numbers[0],
                        y: proof.descriptor_numbers[1],
                        z: proof.descriptor_numbers[2],
                        ..RingSystem::default()
                    });
                }
            }
            continue;
        }
        if edges.len() >= atoms.len() + 2 {
            if let Some(retained) = super::rules::retained::get_pre_descriptor_retained_ring(
                mol,
                &atoms.iter().copied().collect::<Vec<_>>(),
            )
            .or_else(|| {
                // Upstream divergence: a retained locant graph can replace the
                // planner's temporary numbering paths before the later retained
                // identity handoff. No legacy descriptor is selected here.
                super::rules::retained::get_retained_ring(
                    mol,
                    &atoms.iter().copied().collect::<Vec<_>>(),
                )
            })
            .filter(|retained| !retained.locant_maps.is_empty())
            {
                let paths = retained
                    .locant_maps
                    .iter()
                    .map(|map| {
                        let mut numbered: Vec<_> = map.iter().collect();
                        numbered.sort_by_key(|(_, locant)| {
                            super::locants::retained_locant_sort_key(locant)
                        });
                        numbered.into_iter().map(|(&atom, _)| atom).collect()
                    })
                    .collect();
                systems.push(RingSystem {
                    atoms,
                    paths,
                    is_polycycle: true,
                    ..RingSystem::default()
                });
                continue;
            }
            {
                use super::retained_fused_templates::{
                    MatchOptions, match_retained_graph_templates_with_options,
                };
                let candidate = [(false, false), (true, false), (false, true), (true, true)]
                    .into_iter()
                    .any(|(allow_nonaromatic, allow_relocated_indicated_h)| {
                        !match_retained_graph_templates_with_options(
                            mol,
                            &atoms,
                            MatchOptions {
                                family: Some("fused"),
                                allow_nonaromatic,
                                allow_relocated_indicated_h,
                                ..Default::default()
                            },
                        )
                        .is_empty()
                    });
                if candidate {
                    // Upstream divergence: the planner normally supplies these
                    // provisional full-block paths. Keep a retained derivative
                    // candidate intact until production_retained_fused_parent
                    // can check its suffix and attachments. No name or legacy
                    // descriptor is accepted by this topology-only handoff.
                    systems.push(RingSystem {
                        paths: vec![atoms.iter().copied().collect()],
                        atoms,
                        is_polycycle: true,
                        ..RingSystem::default()
                    });
                    continue;
                }
            }
            let fusion_paths = confirmed_fusion_numbering_paths(mol, &atoms)?;
            if !fusion_paths.is_empty() {
                systems.push(RingSystem {
                    atoms,
                    paths: fusion_paths,
                    is_polycycle: true,
                    ..RingSystem::default()
                });
                continue;
            }
            let candidate = polyspiro_or_von_baeyer_candidate(mol, &atoms, &edges);
            if candidate.descriptor.is_some() && candidate.descriptor_allowed() {
                let paths = if candidate.paths.first().is_some_and(|path| {
                    super::rules::retained::get_retained_ring(mol, path).is_some()
                }) {
                    vec![candidate.paths[0].clone()]
                } else {
                    candidate.paths
                };
                systems.push(RingSystem {
                    atoms,
                    paths,
                    is_polycycle: true,
                    polycycle_descriptor: candidate.descriptor,
                    ..RingSystem::default()
                });
            } else {
                systems.push(RingSystem {
                    paths: vec![
                        candidate
                            .paths
                            .first()
                            .cloned()
                            .unwrap_or_else(|| atoms.iter().copied().collect()),
                    ],
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
    Ok(merge_polyspiro_ring_systems(mol, systems))
}

fn confirmed_fusion_numbering_paths(
    mol: &Molecule,
    atoms: &AtomSet,
) -> Result<Vec<Vec<usize>>, String> {
    let Some(plan) = super::fusion::planner::plan_fusion_parent(mol, atoms)? else {
        return Ok(Vec::new());
    };
    if !plan.audit.confirmed() {
        return Err("openclatura: fusion planner returned an unconfirmed parent".into());
    }
    Ok(plan
        .numbering
        .string_input_locant_maps()
        .iter()
        .map(|map| {
            let mut path: Vec<_> = map.keys().copied().collect();
            path.sort_by_key(|id| super::locants::retained_locant_sort_key(&map[id]));
            path
        })
        .collect())
}

fn fallback_bicyclo_paths(
    atoms: &AtomSet,
    edges: &EdgeSet,
    [first, second]: [usize; 2],
) -> Vec<Vec<usize>> {
    let adj = adjacency_from_edges(atoms, edges);
    let mut between = Vec::new();
    if adj[&first].contains(&second) {
        between.push(Vec::new());
    }
    // Python builds a new integer set here. Its table traversal settles equal
    // bridge lengths before the chemical numbering criteria are compared.
    let python_atoms: PythonIntSet = atoms.iter().copied().collect();
    let outside = python_atoms.difference(&[first, second].into_iter().collect());
    let outside_atoms: AtomSet = outside.iter().collect();
    let mut visited = AtomSet::new();
    for root in outside.iter() {
        if visited.contains(&root) {
            continue;
        }
        let mut traversal = Vec::new();
        let mut queue = std::collections::VecDeque::from([root]);
        while let Some(current) = queue.pop_front() {
            if !visited.insert(current) {
                continue;
            }
            traversal.push(current);
            queue.extend(
                adj[&current]
                    .iter()
                    .filter(|neighbor| {
                        outside_atoms.contains(neighbor) && !visited.contains(neighbor)
                    })
                    .copied(),
            );
        }
        let component: AtomSet = traversal.iter().copied().collect();
        let Some(&start) = traversal.iter().find(|atom| adj[atom].contains(&first)) else {
            continue;
        };
        let mut path = vec![start];
        let mut seen = AtomSet::from([start]);
        while path.len() < component.len() {
            let Some(&neighbor) = adj[&path[path.len() - 1]]
                .iter()
                .find(|neighbor| component.contains(neighbor) && !seen.contains(neighbor))
            else {
                break;
            };
            path.push(neighbor);
            seen.insert(neighbor);
        }
        if path.last().is_some_and(|atom| !adj[atom].contains(&second)) {
            path.reverse();
        }
        between.push(path);
    }
    between.sort_by_key(|path| std::cmp::Reverse(path.len()));
    if between.len() != 3 {
        return Vec::new();
    }
    let mut paths = Vec::new();
    for [a, b, c] in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        if [between[a].len(), between[b].len(), between[c].len()]
            != [between[0].len(), between[1].len(), between[2].len()]
        {
            continue;
        }
        paths.push(
            std::iter::once(first)
                .chain(between[a].iter().copied())
                .chain(std::iter::once(second))
                .chain(between[b].iter().rev().copied())
                .chain(between[c].iter().copied())
                .collect(),
        );
        paths.push(
            std::iter::once(second)
                .chain(between[a].iter().rev().copied())
                .chain(std::iter::once(first))
                .chain(between[b].iter().copied())
                .chain(between[c].iter().rev().copied())
                .collect(),
        );
    }
    dedupe_paths(paths)
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
    fn symmetric_diamide_prime_ties_keep_upstream_endpoint_order() {
        let smiles = "C1CCC(CC1)NC(=O)CCC(=O)NC2=NN=C3N2C=CC=C3";
        let mol = read_smiles(smiles).unwrap();
        assert_eq!(
            find_all_carbon_paths(&mol, &AtomSet::new()).unwrap(),
            vec![vec![11, 10, 9, 7]],
        );
        assert_eq!(
            super::super::name_smiles(smiles).as_deref(),
            Ok("N-([1,2,4]triazolo[4,3-a]pyridin-3-yl)-N'-cyclohexylbutanediamide"),
        );
    }

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
        assert_eq!(
            find_ring_systems(&mol, &excluded).unwrap()[0].paths[0].len(),
            6
        );
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
        let systems = find_ring_systems(&mol, &AtomSet::new()).unwrap();
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

        let remaining = find_ring_systems(&mol, &AtomSet::from([0])).unwrap();
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
        let systems = find_ring_systems(&mol, &AtomSet::new()).unwrap();
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
        let systems = find_ring_systems(&mol, &AtomSet::new()).unwrap();
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
    fn larger_spiro_fallback_exposes_a_component_for_graph_composition() {
        let mol = read_smiles("C1CC12CC23CC34CC4").unwrap();
        let systems = find_ring_systems(&mol, &AtomSet::new()).unwrap();
        assert_eq!(systems.len(), 1);
        assert!(systems[0].is_polycycle);
        assert!(systems[0].polycycle_descriptor.is_none());
        assert_eq!(systems[0].atoms.len(), mol.atoms.len());
        assert_eq!(systems[0].paths[0].len(), 3);
        assert!(
            systems[0].paths[0]
                .iter()
                .all(|atom| systems[0].atoms.contains(atom))
        );
        assert!(super::super::name_smiles("C1CC12CC23CC34CC4").is_err());
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
            let systems = find_ring_systems(&mol, &AtomSet::new()).unwrap();
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
    fn bicyclo_bridge_ties_preserve_secondary_unsaturation_locants() {
        for (smiles, expected) in [
            (
                "FC12C=CC(=C1c1ccccc1)C2c1ccccc1",
                "4-fluoro-5,6-diphenylbicyclo[2.1.1]hexa-1(6),2-diene",
            ),
            (
                "COC(=O)SC1C2=CC1C2",
                "((bicyclo[1.1.1]pent-1(5)-en-2-yl)sulfanyl)(methoxy)methanone",
            ),
            (
                "Cc1cc(C2(F)C3=C=C2C3)ccn1",
                "4-(2-fluorobicyclo[1.1.1]penta-1(5),3(5)-dien-2-yl)-2-methylpyridine",
            ),
        ] {
            assert_eq!(super::super::name_smiles(smiles).as_deref(), Ok(expected));
        }
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
