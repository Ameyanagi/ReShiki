//! OPSIN 2.9.0 CycleDetector, commit b91b610af5ab07560fedb20730d7aef46bb2bca0.
//! MIT, Daniel Lowe and contributors. Chain compression and traversal order
//! follow the original source; paths exclude both their start and end atoms.
use crate::graph::{AtomId, BondId, FragmentId, Graph};
use std::collections::HashSet;

pub fn assign_cycle_membership(graph: &mut Graph, fragment: FragmentId) {
    let atoms = graph.fragment(fragment).atoms.clone();
    let mut depth = vec![None; graph.atoms.len()];
    for &atom in &atoms {
        graph.atom_mut(atom).in_cycle = false;
    }
    for atom in atoms {
        if depth[atom.0].is_none() {
            traverse_rings(graph, atom, None, 0, &mut depth);
        }
    }
}

fn traverse_rings(
    graph: &mut Graph,
    mut current: AtomId,
    mut previous: Option<AtomId>,
    mut current_depth: usize,
    depths: &mut [Option<usize>],
) -> usize {
    if let Some(depth) = depths[current.0] {
        return depth;
    }
    depths[current.0] = Some(current_depth);
    let mut equivalent = vec![current];
    let neighbours = loop {
        let mut neighbours = graph.neighbours(current);
        if let Some(previous) = previous {
            neighbours.retain(|atom| *atom != previous);
        }
        if neighbours.len() != 1 {
            break neighbours;
        }
        let next = neighbours[0];
        if depths[next.0].is_some() {
            break neighbours;
        }
        previous = Some(current);
        current = next;
        equivalent.push(current);
        current_depth += 1;
        depths[current.0] = Some(current_depth);
    };
    let mut result = current_depth + 1;
    for neighbour in neighbours {
        result = result.min(traverse_rings(
            graph,
            neighbour,
            Some(current),
            current_depth + 1,
            depths,
        ));
    }
    if result < current_depth {
        for atom in equivalent {
            graph.atom_mut(atom).in_cycle = true;
        }
    } else if result == current_depth {
        graph.atom_mut(current).in_cycle = true;
    }
    result
}

pub fn paths_between_atoms_using_bonds(
    graph: &Graph,
    start: AtomId,
    end: AtomId,
    permitted_bonds: &[BondId],
) -> Vec<Vec<AtomId>> {
    let permitted: HashSet<_> = permitted_bonds.iter().copied().collect();
    let mut paths = Vec::new();
    let mut stack = vec![(start, Vec::new())];
    while let Some((atom, mut visited)) = stack.pop() {
        visited.push(atom);
        for &bond in &graph.atom(atom).bonds {
            if !permitted.contains(&bond) {
                continue;
            }
            let neighbour = graph.bond(bond).other_atom(atom).unwrap();
            if visited.contains(&neighbour) {
                continue;
            }
            if neighbour == end {
                paths.push(visited[1..].to_vec());
            } else {
                stack.push((neighbour, visited.clone()));
            }
        }
    }
    paths
}

pub fn is_bond_in_cycle(graph: &Graph, bond: BondId) -> bool {
    let b = graph.bond(bond);
    let mut seen = HashSet::new();
    let mut stack = vec![b.from];
    while let Some(atom) = stack.pop() {
        if !seen.insert(atom) {
            continue;
        }
        if atom == b.to {
            return true;
        }
        for &other_bond in &graph.atom(atom).bonds {
            if other_bond != bond {
                stack.push(graph.bond(other_bond).other_atom(atom).unwrap());
            }
        }
    }
    false
}
