//! Extended von Baeyer discovery from upstream `chains.py` and `von_baeyer.py`.
//! Candidate ranking never substitutes for reconstruction: callers retain only
//! paths whose descriptor rebuilds every edge of the parent skeleton.
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use super::graph_kernel::{EdgeSet, adjacency_from_edges, connected_components, normalize_edge};
use super::molecule::{AtomSet, Molecule};
use super::polycycle_topology::{
    RingNumbering, audit_von_baeyer_descriptor, build_von_baeyer_numbering,
};
use super::ring_renderer::render_von_baeyer_descriptor;

type Adjacency = BTreeMap<usize, AtomSet>;
pub type DescriptorPaths = (Option<String>, Vec<Vec<usize>>);
const MAIN_RING_WALK_STATES: usize = 2_000_000;
const MAX_CYCLE_SPACE_RANK: usize = 28;
const MAX_LONGEST_CYCLES: usize = 4096;

#[derive(Clone, Debug)]
struct LegacyBridge {
    nodes: AtomSet,
    endpoints: [usize; 2],
    length: usize,
}

fn bridge_interior_path(adj: &Adjacency, nodes: &AtomSet, start: usize, end: usize) -> Vec<usize> {
    let mut queue = VecDeque::from([(start, Vec::new())]);
    let mut visited = AtomSet::new();
    while let Some((current, path)) = queue.pop_front() {
        if current == end && !path.is_empty() {
            return path[..path.len() - 1].to_vec();
        }
        visited.insert(current);
        for &neighbor in &adj[&current] {
            if (nodes.contains(&neighbor) || neighbor == end) && !visited.contains(&neighbor) {
                let mut next = path.clone();
                next.push(neighbor);
                queue.push_back((neighbor, next));
            }
        }
    }
    Vec::new()
}

fn largest_cycles_walk(atoms: &AtomSet, adj: &Adjacency) -> Option<Vec<Vec<usize>>> {
    let mut states = 0;
    let mut longest = 0;
    let mut cycles = Vec::new();
    for &start in atoms {
        states += 1;
        let mut path = vec![start];
        let mut stack = vec![(start, 0)];
        while let Some(&(current, index)) = stack.last() {
            let Some(&neighbor) = adj[&current].iter().nth(index) else {
                stack.pop();
                path.pop();
                continue;
            };
            if let Some(frame) = stack.last_mut() {
                frame.1 += 1;
            }
            if neighbor == start && path.len() >= 3 {
                if path.len() > longest {
                    longest = path.len();
                    cycles.clear();
                }
                if path.len() == longest {
                    cycles.push(path.clone());
                }
            } else if !path.contains(&neighbor) {
                states += 1;
                if states > MAIN_RING_WALK_STATES {
                    return None;
                }
                path.push(neighbor);
                stack.push((neighbor, 0));
            }
        }
    }
    Some(cycles)
}

fn xor_mask(target: &mut [u64], source: &[u64]) {
    for (target, source) in target.iter_mut().zip(source) {
        *target ^= source;
    }
}

fn cycle_path_from_mask(mask: &[u64], edges: &[(usize, usize)]) -> Option<Vec<usize>> {
    let mut adj: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (index, &(left, right)) in edges.iter().enumerate() {
        if mask[index / 64] & (1_u64 << (index % 64)) == 0 {
            continue;
        }
        adj.entry(left).or_default().push(right);
        adj.entry(right).or_default().push(left);
    }
    if adj.is_empty() || adj.values().any(|ends| ends.len() != 2) {
        return None;
    }
    let start = *adj.first_key_value()?.0;
    let mut path = vec![start];
    let mut previous = None;
    let mut current = start;
    loop {
        let ends = &adj[&current];
        let next = if Some(ends[0]) != previous {
            ends[0]
        } else {
            ends[1]
        };
        if next == start {
            break;
        }
        path.push(next);
        previous = Some(current);
        current = next;
        if path.len() > adj.len() {
            return None;
        }
    }
    (path.len() == adj.len()).then_some(path)
}

fn largest_cycles_from_cycle_space(
    atoms: &AtomSet,
    edges: &EdgeSet,
    adj: &Adjacency,
) -> Vec<Vec<usize>> {
    let rank = edges.len() + 1 - atoms.len();
    if rank == 0 || rank > MAX_CYCLE_SPACE_RANK {
        return Vec::new();
    }
    let ordered_edges: Vec<_> = edges.iter().copied().collect();
    let edge_index: BTreeMap<_, _> = ordered_edges
        .iter()
        .enumerate()
        .map(|(i, &edge)| (edge, i))
        .collect();
    let mut parent = BTreeMap::new();
    let mut depth = BTreeMap::new();
    let mut tree = EdgeSet::new();
    let mut seen = AtomSet::new();
    for &root in atoms {
        if !seen.insert(root) {
            continue;
        }
        depth.insert(root, 0_usize);
        let mut stack = vec![root];
        while let Some(current) = stack.pop() {
            for &neighbor in &adj[&current] {
                if !seen.insert(neighbor) {
                    continue;
                }
                parent.insert(neighbor, current);
                depth.insert(neighbor, depth[&current] + 1);
                tree.insert(normalize_edge(current, neighbor));
                stack.push(neighbor);
            }
        }
    }
    let words = edges.len().div_ceil(64);
    let mut basis = Vec::new();
    for &edge in edges {
        if tree.contains(&edge) {
            continue;
        }
        let (mut left, mut right) = edge;
        let mut ring = EdgeSet::from([edge]);
        while left != right {
            if depth[&left] >= depth[&right] {
                ring.insert(normalize_edge(left, parent[&left]));
                left = parent[&left];
            } else {
                ring.insert(normalize_edge(right, parent[&right]));
                right = parent[&right];
            }
        }
        let mut mask = vec![0; words];
        for edge in ring {
            let index = edge_index[&edge];
            mask[index / 64] |= 1_u64 << (index % 64);
        }
        basis.push(mask);
    }
    if basis.is_empty() {
        return Vec::new();
    }
    let mut current = vec![0; words];
    let mut longest = 0;
    let mut best = Vec::new();
    for step in 1_usize..1_usize << basis.len() {
        xor_mask(&mut current, &basis[step.trailing_zeros() as usize]);
        let length = current
            .iter()
            .map(|word| word.count_ones() as usize)
            .sum::<usize>();
        if length < longest
            || (length == longest && (best.is_empty() || best.len() >= MAX_LONGEST_CYCLES))
        {
            continue;
        }
        let Some(path) = cycle_path_from_mask(&current, &ordered_edges) else {
            continue;
        };
        if length > longest {
            longest = length;
            best.clear();
        }
        best.push(path);
    }
    best.sort_by(|a, b| (a.len(), a).cmp(&(b.len(), b)));
    best
}

fn main_ring_division(
    bridge: &LegacyBridge,
    position: &BTreeMap<usize, usize>,
    size: usize,
) -> Option<usize> {
    let first = *position.get(&bridge.endpoints[0])?;
    let second = *position.get(&bridge.endpoints[1])?;
    let span = first.abs_diff(second).min(size - first.abs_diff(second));
    Some(size.abs_diff(2 * span))
}

/// Upstream main-ring search, in preference order. The per-call walk and
/// cycle-space bounds deliberately abstain on an unmanageable graph.
pub fn descriptor_candidates(atoms: &AtomSet, edges: &EdgeSet) -> Vec<DescriptorPaths> {
    let adj = adjacency_from_edges(atoms, edges);
    let largest = largest_cycles_walk(atoms, &adj)
        .unwrap_or_else(|| largest_cycles_from_cycle_space(atoms, edges, &adj));
    if largest.is_empty() {
        return vec![(None, vec![atoms.iter().copied().collect()])];
    }
    let mut candidates = Vec::new();
    for main_ring in &largest {
        let main_atoms: AtomSet = main_ring.iter().copied().collect();
        let ring_edges: EdgeSet = (0..main_ring.len())
            .map(|i| normalize_edge(main_ring[i], main_ring[(i + 1) % main_ring.len()]))
            .collect();
        let mut bridges = Vec::new();
        for &(first, second) in edges {
            if main_atoms.contains(&first)
                && main_atoms.contains(&second)
                && !ring_edges.contains(&(first, second))
            {
                bridges.push(LegacyBridge {
                    nodes: AtomSet::new(),
                    endpoints: [first, second],
                    length: 0,
                });
            }
        }
        let outside: AtomSet = atoms.difference(&main_atoms).copied().collect();
        for component in connected_components(&outside, edges) {
            let mut connections = Vec::new();
            for &atom in &component {
                for &neighbor in &adj[&atom] {
                    if main_atoms.contains(&neighbor) {
                        connections.push((atom, neighbor));
                    }
                }
            }
            if connections.len() >= 2 {
                bridges.push(LegacyBridge {
                    length: component.len(),
                    nodes: component,
                    endpoints: [connections[0].1, connections[1].1],
                });
                for &(first, second) in &connections[2..] {
                    bridges.push(LegacyBridge {
                        length: 0,
                        nodes: AtomSet::new(),
                        endpoints: [first, second],
                    });
                }
            }
        }
        if bridges.is_empty()
            || bridges
                .iter()
                .any(|bridge| bridge.endpoints[0] == bridge.endpoints[1])
        {
            continue;
        }
        bridges.sort_by_key(|bridge| std::cmp::Reverse(bridge.length));
        let position: BTreeMap<_, _> = main_ring
            .iter()
            .enumerate()
            .map(|(i, &atom)| (atom, i))
            .collect();
        let maximum = bridges[0].length;
        let selected = bridges
            .iter()
            .enumerate()
            .filter(|(_, bridge)| bridge.length == maximum)
            .filter_map(|(i, bridge)| {
                let division = main_ring_division(bridge, &position, main_ring.len())?;
                let first = position[&bridge.endpoints[0]];
                let second = position[&bridge.endpoints[1]];
                Some(((division, first.min(second), first.max(second)), i))
            })
            .min()
            .map(|(_, index)| index);
        if let Some(index) = selected {
            let bridge = bridges.remove(index);
            bridges.insert(0, bridge);
        }
        let division =
            main_ring_division(&bridges[0], &position, main_ring.len()).unwrap_or(main_ring.len());
        candidates.push(((std::cmp::Reverse(maximum), division), main_ring, bridges));
    }
    if candidates.is_empty() {
        return vec![(None, vec![largest[0].clone()])];
    }
    // Upstream divergence: ascending Rust sets settle otherwise equal search
    // preferences deterministically; Python's bare set traversal may offer a
    // different audited descriptor first. The P-23 selection key is unchanged.
    candidates.sort_by_key(|candidate| candidate.0);
    // The upstream caller consumes at most 256 rendered choices. Applying the
    // same bound here avoids materializing the rest of a symmetric skeleton.
    candidates
        .into_iter()
        .flat_map(|(_, ring, bridges)| legacy_from_candidate(ring, &bridges, atoms, &adj))
        .take(256)
        .collect()
}

fn legacy_from_candidate(
    ring: &[usize],
    bridges: &[LegacyBridge],
    atoms: &AtomSet,
    adj: &Adjacency,
) -> Vec<DescriptorPaths> {
    let main = &bridges[0];
    let [first, second] = main.endpoints;
    let i = ring.iter().position(|&atom| atom == first).unwrap_or(0);
    let j = ring.iter().position(|&atom| atom == second).unwrap_or(0);
    let low = i.min(j);
    let high = i.max(j);
    let path_a = ring[low..=high].to_vec();
    let path_b: Vec<_> = ring[high..].iter().chain(&ring[..=low]).copied().collect();
    let (mut branch1, mut branch2) = if path_a.len() >= path_b.len() {
        (path_a, path_b.into_iter().rev().collect::<Vec<_>>())
    } else {
        (path_b, path_a.into_iter().rev().collect())
    };
    if branch1[0] != first {
        branch1.reverse();
        branch2.reverse();
    }
    let bridge_path = if main.length > 0 {
        bridge_interior_path(adj, &main.nodes, first, second)
    } else {
        Vec::new()
    };
    let mut path1 = branch1.clone();
    path1.extend(branch2[1..branch2.len() - 1].iter().rev().copied());
    path1.extend(&bridge_path);
    let mut path2: Vec<_> = branch1.iter().rev().copied().collect();
    path2.extend_from_slice(&branch2[1..branch2.len() - 1]);
    path2.extend(bridge_path.iter().rev().copied());
    let mut primary = [branch1.len() - 2, branch2.len() - 2, main.length];
    primary.sort_by(|a, b| b.cmp(a));
    let mut orientations = Vec::new();
    for mut path in [path1, path2] {
        let base: BTreeMap<_, _> = path
            .iter()
            .enumerate()
            .map(|(i, &atom)| (atom, i + 1))
            .collect();
        let mut extras: Vec<_> = bridges[1..]
            .iter()
            .filter(|bridge| bridge.length > 0)
            .collect();
        extras.sort_by_key(|bridge| {
            secondary_numbering_key(
                bridge.length,
                *base.get(&bridge.endpoints[0]).unwrap_or(&1),
                *base.get(&bridge.endpoints[1]).unwrap_or(&1),
            )
        });
        let mut visited: AtomSet = path.iter().copied().collect();
        for bridge in extras {
            for atom in
                bridge_interior_path(adj, &bridge.nodes, bridge.endpoints[0], bridge.endpoints[1])
            {
                if visited.insert(atom) {
                    path.push(atom);
                }
            }
        }
        for &atom in atoms {
            if visited.insert(atom) {
                path.push(atom);
            }
        }
        let pos: BTreeMap<_, _> = path
            .iter()
            .enumerate()
            .map(|(i, &atom)| (atom, i + 1))
            .collect();
        let mut chords: Vec<_> = bridges[1..]
            .iter()
            .map(|bridge| {
                let a = *pos.get(&bridge.endpoints[0]).unwrap_or(&1);
                let b = *pos.get(&bridge.endpoints[1]).unwrap_or(&1);
                (bridge.length, a.min(b), a.max(b))
            })
            .collect();
        chords.sort_by_key(|&(length, low, high)| secondary_numbering_key(length, low, high));
        let mut body = format!("[{}.{}.{}", primary[0], primary[1], primary[2]);
        let mut preference = Vec::new();
        for (length, low, high) in chords {
            preference.extend([low, high]);
            body.push_str(&format!(".{length}^{{{low},{high}}}"));
        }
        body.push(']');
        orientations.push((
            preference,
            render_von_baeyer_descriptor(bridges.len(), &body),
            path,
        ));
    }
    orientations.sort_by(|a, b| a.0.cmp(&b.0));
    orientations
        .into_iter()
        .map(|(_, descriptor, path)| (Some(descriptor), vec![path]))
        .collect()
}

fn secondary_numbering_key(
    length: usize,
    first: usize,
    second: usize,
) -> (
    std::cmp::Reverse<usize>,
    std::cmp::Reverse<usize>,
    std::cmp::Reverse<usize>,
) {
    (
        std::cmp::Reverse(length),
        std::cmp::Reverse(first.max(second)),
        std::cmp::Reverse(first.min(second)),
    )
}

#[derive(Clone, Debug)]
pub struct VonBaeyerBridge {
    pub length: usize,
    pub attachments: [usize; 2],
    pub atoms: Vec<usize>,
    pub dependent: bool,
}

type CandidateRank = (
    isize,
    isize,
    usize,
    Vec<isize>,
    usize,
    Vec<usize>,
    Vec<usize>,
    [usize; 3],
);

#[derive(Clone, Debug)]
pub struct VonBaeyerCandidate {
    pub descriptor: String,
    pub path: Vec<usize>,
    pub rank: CandidateRank,
    pub numbering: RingNumbering,
}

/// The independent audited decomposition used when the main-ring search has
/// no reconstructable descriptor. Limits match upstream's bounded scope.
pub fn find_von_baeyer_candidates(
    mol: &Molecule,
    atoms: &AtomSet,
    edges: &EdgeSet,
) -> Vec<VonBaeyerCandidate> {
    if edges.len() + 1 < atoms.len() + 3 || atoms.iter().any(|atom| mol.atoms[atom].is_aromatic) {
        return Vec::new();
    }
    let adj = adjacency_from_edges(atoms, edges);
    if atoms.iter().any(|&atom| {
        adj[&atom].len() >= 4 && {
            let remaining: AtomSet = atoms
                .iter()
                .filter(|&&other| other != atom)
                .copied()
                .collect();
            connected_components(&remaining, edges).len() >= 2
        }
    }) {
        return Vec::new();
    }
    let ring_count = edges.len() + 1 - atoms.len();
    let heads: Vec<_> = atoms
        .iter()
        .filter(|&&atom| adj[&atom].len() >= 3)
        .copied()
        .collect();
    if ring_count > 8 || heads.len() > 12 {
        return Vec::new();
    }
    let mut candidates = Vec::new();
    let mut seen = BTreeSet::new();
    for first in 0..heads.len() {
        for second in first + 1..heads.len() {
            let paths = simple_paths_between(heads[first], heads[second], &adj);
            if paths.len() >= 96 {
                continue;
            }
            for a in 0..paths.len() {
                for b in a + 1..paths.len() {
                    for c in b + 1..paths.len() {
                        let indexes = [a, b, c];
                        let mut internal = AtomSet::new();
                        if indexes.iter().any(|&index| {
                            paths[index][1..paths[index].len() - 1]
                                .iter()
                                .any(|&atom| !internal.insert(atom))
                        }) {
                            continue;
                        }
                        for main in 0..3 {
                            let ring: Vec<_> = indexes
                                .iter()
                                .enumerate()
                                .filter_map(|(i, &index)| {
                                    (i != main).then_some(paths[index].clone())
                                })
                                .collect();
                            if let Some(candidate) = build_candidate(
                                atoms,
                                edges,
                                [
                                    ring[0].clone(),
                                    ring[1].clone(),
                                    paths[indexes[main]].clone(),
                                ],
                                [heads[first], heads[second]],
                                ring_count,
                            ) && seen
                                .insert((candidate.descriptor.clone(), candidate.path.clone()))
                            {
                                candidates.push(candidate);
                            }
                        }
                    }
                }
            }
        }
    }
    candidates.sort_by(|a, b| a.rank.cmp(&b.rank));
    candidates
}

fn simple_paths_between(start: usize, end: usize, adj: &Adjacency) -> Vec<Vec<usize>> {
    let mut paths = Vec::new();
    let mut stack = vec![(start, vec![start])];
    while !stack.is_empty() && paths.len() < 96 {
        let Some((current, path)) = stack.pop() else {
            break;
        };
        for &neighbor in adj[&current].iter().rev() {
            if neighbor == end {
                let mut next = path.clone();
                next.push(neighbor);
                paths.push(next);
            } else if !path.contains(&neighbor) {
                let mut next = path.clone();
                next.push(neighbor);
                stack.push((neighbor, next));
            }
        }
    }
    paths.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    paths
}

fn build_candidate(
    atoms: &AtomSet,
    edges: &EdgeSet,
    [mut first, mut second, mut main]: [Vec<usize>; 3],
    heads: [usize; 2],
    ring_count: usize,
) -> Option<VonBaeyerCandidate> {
    if second.len() > first.len() {
        std::mem::swap(&mut first, &mut second);
    } else if first.len() == second.len() {
        let reverse = (
            first.iter().rev().copied().collect::<Vec<_>>(),
            second.iter().rev().copied().collect::<Vec<_>>(),
        );
        if reverse < (first.clone(), second.clone()) {
            (first, second) = reverse;
        }
    }
    if first[0] != heads[0] {
        first.reverse();
    }
    if second[0] != heads[0] {
        second.reverse();
    }
    if main[0] != heads[0] {
        main.reverse();
    }
    let primary_edges: EdgeSet = [&first, &second, &main]
        .iter()
        .flat_map(|path| path.windows(2).map(|pair| normalize_edge(pair[0], pair[1])))
        .collect();
    let primary_atoms: AtomSet = first.iter().chain(&second).chain(&main).copied().collect();
    let remaining: EdgeSet = edges.difference(&primary_edges).copied().collect();
    let mut secondary = classify_secondary(atoms, edges, &primary_atoms, &remaining)?;
    if secondary.len() + 2 != ring_count {
        return None;
    }
    let mut path = first.clone();
    path.extend(second[1..second.len() - 1].iter().rev().copied());
    path.extend_from_slice(&main[1..main.len() - 1]);
    let mut locants: BTreeMap<_, _> = path
        .iter()
        .enumerate()
        .map(|(i, &atom)| (atom, i + 1))
        .collect();
    secondary.sort_by_key(|bridge| {
        (
            bridge.dependent,
            std::cmp::Reverse(bridge.length),
            bridge.attachments,
        )
    });
    for bridge in &mut secondary {
        if !bridge
            .attachments
            .iter()
            .all(|atom| locants.contains_key(atom))
        {
            return None;
        }
        bridge.attachments.sort_by_key(|atom| locants[atom]);
        if bridge.length > 0 {
            if locants[&bridge.attachments[0]] <= locants[&bridge.attachments[1]] {
                path.extend(&bridge.atoms);
            } else {
                path.extend(bridge.atoms.iter().rev().copied());
            }
        }
        locants = path
            .iter()
            .enumerate()
            .map(|(i, &atom)| (atom, i + 1))
            .collect();
    }
    let lengths = [first.len() - 2, second.len() - 2, main.len() - 2];
    let mut parts: Vec<_> = lengths.iter().map(usize::to_string).collect();
    for bridge in &secondary {
        let a = locants[&bridge.attachments[0]];
        let b = locants[&bridge.attachments[1]];
        parts.push(format!("{}^{{{},{}}}", bridge.length, a.min(b), a.max(b)));
    }
    let descriptor =
        render_von_baeyer_descriptor(secondary.len() + 1, &format!("[{}]", parts.join(".")));
    if !audit_von_baeyer_descriptor(&descriptor, &path, edges) {
        return None;
    }
    let ring_atoms: AtomSet = first.iter().chain(&second).copied().collect();
    let main_non_ring = main[1..main.len() - 1]
        .iter()
        .filter(|atom| !ring_atoms.contains(atom))
        .count();
    let in_order: Vec<_> = secondary
        .iter()
        .flat_map(|bridge| bridge.attachments.iter().map(|atom| locants[atom]))
        .collect();
    let mut sorted = in_order.clone();
    sorted.sort_unstable();
    let rank = (
        -(ring_atoms.len() as isize),
        -(main_non_ring as isize),
        lengths[0].abs_diff(lengths[1]),
        secondary
            .iter()
            .filter(|bridge| !bridge.dependent)
            .map(|bridge| -(bridge.length as isize))
            .collect(),
        secondary.iter().filter(|bridge| bridge.dependent).count(),
        sorted,
        in_order,
        lengths,
    );
    let numbering = build_von_baeyer_numbering(&descriptor, &path, edges);
    Some(VonBaeyerCandidate {
        descriptor,
        path,
        rank,
        numbering,
    })
}

fn classify_secondary(
    atoms: &AtomSet,
    edges: &EdgeSet,
    primary: &AtomSet,
    remaining: &EdgeSet,
) -> Option<Vec<VonBaeyerBridge>> {
    let mut bridges = Vec::new();
    for &(first, second) in remaining {
        if primary.contains(&first) && primary.contains(&second) {
            bridges.push(VonBaeyerBridge {
                length: 0,
                attachments: [first, second],
                atoms: Vec::new(),
                dependent: false,
            });
        }
    }
    let outside: AtomSet = atoms.difference(primary).copied().collect();
    for component in connected_components(&outside, edges) {
        let mut connections = Vec::new();
        for &atom in primary {
            for &node in &component {
                if remaining.contains(&normalize_edge(atom, node)) {
                    connections.push((atom, node));
                }
            }
        }
        if connections
            .iter()
            .map(|(atom, _)| atom)
            .collect::<BTreeSet<_>>()
            .len()
            < 2
        {
            return None;
        }
        let mut independent = Vec::new();
        for i in 0..connections.len() {
            for j in i + 1..connections.len() {
                let first = connections[i];
                let second = connections[j];
                if first.0 == second.0 || first.1 == second.1 {
                    continue;
                }
                let internal = component_path(&component, first.0, second.0, edges);
                if internal.iter().copied().collect::<AtomSet>() == component {
                    independent.push((first, second, internal));
                }
            }
        }
        independent.sort_by(|a, b| {
            b.2.len()
                .cmp(&a.2.len())
                .then_with(|| a.0.cmp(&b.0))
                .then_with(|| a.1.cmp(&b.1))
        });
        let (first, second, internal) = independent.into_iter().next()?;
        if internal.is_empty() {
            return None;
        }
        bridges.push(VonBaeyerBridge {
            length: component.len(),
            attachments: [first.0, second.0],
            atoms: internal,
            dependent: false,
        });
        for connection in connections {
            if connection == first || connection == second {
                continue;
            }
            bridges.push(VonBaeyerBridge {
                length: 0,
                attachments: [
                    connection.0.min(connection.1),
                    connection.0.max(connection.1),
                ],
                atoms: Vec::new(),
                dependent: true,
            });
        }
    }
    Some(bridges)
}

fn component_path(component: &AtomSet, first: usize, second: usize, edges: &EdgeSet) -> Vec<usize> {
    let adj = adjacency_from_edges(component, edges);
    let ends: AtomSet = component
        .iter()
        .filter(|&&atom| edges.contains(&normalize_edge(second, atom)))
        .copied()
        .collect();
    for &start in component {
        if !edges.contains(&normalize_edge(first, start)) {
            continue;
        }
        let mut seen = AtomSet::from([start]);
        let mut queue = VecDeque::from([(start, vec![start])]);
        while let Some((current, path)) = queue.pop_front() {
            if ends.contains(&current) {
                return path;
            }
            for &neighbor in &adj[&current] {
                if component.contains(&neighbor) && seen.insert(neighbor) {
                    let mut next = path.clone();
                    next.push(neighbor);
                    queue.push_back((neighbor, next));
                }
            }
        }
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::super::{graph_io::read_smiles, polycycle_topology::edges_within_atoms};
    use super::*;

    #[test]
    fn cycle_space_recovers_every_largest_simple_cycle() {
        let atoms = AtomSet::from([0, 1, 2, 3]);
        let edges = EdgeSet::from([(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)]);
        let adjacency = adjacency_from_edges(&atoms, &edges);
        let cycle_edges = |cycles: Vec<Vec<usize>>| {
            cycles
                .into_iter()
                .map(|cycle| {
                    (0..cycle.len())
                        .map(|i| normalize_edge(cycle[i], cycle[(i + 1) % cycle.len()]))
                        .collect::<EdgeSet>()
                })
                .collect::<BTreeSet<_>>()
        };
        let exhaustive = cycle_edges(largest_cycles_walk(&atoms, &adjacency).unwrap());
        let space = cycle_edges(largest_cycles_from_cycle_space(&atoms, &edges, &adjacency));
        assert_eq!(space.len(), 3);
        assert_eq!(space, exhaustive);
    }

    #[test]
    fn legacy_and_independent_search_offer_only_audited_parent_numberings() {
        let mol = read_smiles("C1NC23COC12C=CC3").unwrap();
        let atoms = mol.atoms.keys().copied().collect();
        let edges = edges_within_atoms(&mol, &atoms);
        let candidate =
            super::super::chains::polyspiro_or_von_baeyer_candidate(&mol, &atoms, &edges);
        assert_eq!(
            candidate.descriptor.as_deref(),
            Some("tricyclo[3.2.2.0^{1,5}]")
        );
        assert!(candidate.descriptor_allowed());
        assert!(!candidate.paths.is_empty());
        assert!(
            candidate
                .paths
                .iter()
                .all(|path| audit_von_baeyer_descriptor(
                    candidate.descriptor.as_deref().unwrap(),
                    path,
                    &edges
                ))
        );
        let independent = find_von_baeyer_candidates(&mol, &atoms, &edges);
        assert!(!independent.is_empty());
        assert!(
            independent
                .iter()
                .all(|candidate| candidate.numbering.audit_ok)
        );
    }
}
