//! Iterative searches adapted from RDKit FindRings.cpp (2026.03.6).
//! Copyright (C) 2003-2021 Greg Landrum and other RDKit contributors.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
use super::{Budget, Ring, Topology, at};
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

#[derive(Clone, Copy, Default)]
struct Visit {
    color: u8,
    parent: Option<usize>,
    depth: usize,
}

/// Figueras breadth-first search with RDKit's multiple-ring/forbidden-node rules.
/// Sparse visits and a deque avoid whole-molecule allocation and quadratic path
/// insertion when processing disconnected components or large macrocycles.
pub(super) fn smallest(
    top: &Topology,
    root: usize,
    active: &[bool],
    forbidden: &[usize],
    budget: &mut Budget,
) -> Result<Vec<Ring>, String> {
    let mut visits = HashMap::<usize, Visit>::new();
    for &id in forbidden {
        visits.insert(
            id,
            Visit {
                color: 2,
                ..Default::default()
            },
        );
    }
    visits.insert(root, Visit::default());
    let mut queue = VecDeque::from([root]);
    let mut size = usize::MAX;
    let mut rings = Vec::new();
    while let Some(current) = queue.pop_front() {
        budget.spend(1)?;
        if queue.len() >= 200_000 {
            return Err("Ring breadth-first search limit exceeded".into());
        }
        let visit = visits.get_mut(&current).ok_or("Missing ring search node")?;
        visit.color = 2;
        let Visit { parent, depth, .. } = *visit;
        let depth = depth + 1;
        if depth > size {
            break;
        }
        for &(neighbor, bond) in top.neighbors(current)? {
            budget.spend(1)?;
            if !*at(active, bond)? {
                continue;
            }
            let old = visits.get(&neighbor).copied().unwrap_or_default();
            if old.color == 2 || parent == Some(neighbor) {
                continue;
            }
            if old.color == 0 {
                visits.insert(
                    neighbor,
                    Visit {
                        color: 1,
                        parent: Some(current),
                        depth,
                    },
                );
                queue.push_back(neighbor);
            } else {
                let mut ring = VecDeque::from([neighbor]);
                let mut members = HashSet::from([neighbor]);
                let mut next = old.parent;
                while let Some(p) = next.filter(|&p| p != root) {
                    budget.spend(1)?;
                    ring.push_back(p);
                    members.insert(p);
                    next = visits.get(&p).ok_or("Missing ring parent")?.parent;
                }
                ring.push_front(current);
                members.insert(current);
                next = parent;
                while let Some(p) = next {
                    budget.spend(1)?;
                    if !members.insert(p) {
                        ring.clear();
                        break;
                    }
                    ring.push_front(p);
                    next = visits.get(&p).ok_or("Missing ring parent")?.parent;
                }
                if ring.len() > 1 {
                    if ring.len() > size {
                        return Ok(rings);
                    }
                    size = ring.len();
                    budget.store(ring.len())?;
                    rings.push(ring.into_iter().collect());
                }
            }
        }
    }
    Ok(rings)
}

/// Recovery search used by RDKit for highly fused rings missed by pruning.
pub(super) fn connection(
    top: &Topology,
    start: usize,
    end: usize,
    ring_atoms: &[bool],
    seen: &BTreeSet<Vec<usize>>,
    budget: &mut Budget,
) -> Result<Option<Ring>, String> {
    let mut queue = VecDeque::from([vec![start]]);
    let mut queued_nodes = 1usize;
    while let Some(path) = queue.pop_front() {
        queued_nodes -= path.len();
        budget.spend(path.len())?;
        if queue.len() >= 200_000 {
            return Err("Ring breadth-first search limit exceeded".into());
        }
        let &current = path.last().ok_or("Empty ring search path")?;
        // RDKit's recovery pass intentionally uses the complete topology.
        for &(neighbor, _) in top.neighbors(current)? {
            budget.spend(1)?;
            if neighbor == end {
                if current != start {
                    budget.spend(path.len())?;
                    let mut candidate = path.clone();
                    candidate.push(neighbor);
                    if !seen.contains(&super::invariant(&candidate)) {
                        return Ok(Some(candidate));
                    }
                }
            } else if *at(ring_atoms, neighbor)? && !path.contains(&neighbor) {
                budget.spend(path.len() + 1)?;
                if queued_nodes + path.len() + 1 > 2_000_000 {
                    return Err("Ring search path storage limit exceeded".into());
                }
                let mut next = path.clone();
                next.push(neighbor);
                queued_nodes += next.len();
                queue.push_back(next);
            }
        }
    }
    Ok(None)
}

/// Iterative equivalent of RDKit fastFindRings, including its fallback use of
/// the complete topology. Avoid recursive stack growth on long structures.
pub(super) fn fast(top: &Topology, budget: &mut Budget) -> Result<Vec<Ring>, String> {
    let mut color = vec![0u8; top.adjacency.len()];
    let mut rings = Vec::new();
    for root in 0..color.len() {
        if *at(&color, root)? != 0 {
            continue;
        }
        if top.neighbors(root)?.len() < 2 {
            super::set(&mut color, root, 2)?;
            continue;
        }
        super::set(&mut color, root, 1)?;
        let mut stack = vec![(root, None, 0usize)];
        let mut path = vec![root];
        while let Some(&(node, parent, next)) = stack.last() {
            budget.spend(1)?;
            let Some(&(neighbor, _)) = top.neighbors(node)?.get(next) else {
                super::set(&mut color, node, 2)?;
                stack.pop();
                path.pop();
                continue;
            };
            let frame = stack.last_mut().ok_or("Missing ring traversal frame")?;
            frame.2 += 1;
            match *at(&color, neighbor)? {
                0 => {
                    if top.neighbors(neighbor)?.len() < 2 {
                        super::set(&mut color, neighbor, 2)?;
                    } else {
                        super::set(&mut color, neighbor, 1)?;
                        stack.push((neighbor, Some(node), 0));
                        path.push(neighbor);
                    }
                }
                1 if parent != Some(neighbor) => {
                    let mut cycle = Vec::new();
                    for &id in path.iter().rev() {
                        cycle.push(id);
                        if id == neighbor {
                            break;
                        }
                    }
                    budget.store(cycle.len())?;
                    rings.push(cycle);
                }
                _ => {}
            }
        }
    }
    Ok(rings)
}
