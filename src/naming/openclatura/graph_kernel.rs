//! Undirected graph blocks ported from Python `graph_kernel.py`.
use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};

use super::molecule::{AtomSet, Molecule};

pub type Edge = (usize, usize);
pub type EdgeSet = BTreeSet<Edge>;

fn normalize_edge(first: usize, second: usize) -> Edge {
    (first.min(second), first.max(second))
}

/// Maximal biconnected edge blocks of the selected molecular graph.
///
/// The molecule owns endpoint validation and excludes self and duplicate bonds.
/// One-edge bridge blocks are retained; spiro blocks share their articulation
/// atom, while fused and bridged rings remain one cyclic block. The iterative
/// Tarjan walk and final block ordering follow upstream `graph_kernel.py`.
pub fn biconnected_edge_components(mol: &Molecule, nodes: &AtomSet) -> Vec<EdgeSet> {
    let adjacency: BTreeMap<usize, Vec<usize>> = nodes
        .iter()
        .map(|&node| {
            let mut neighbors: Vec<_> = mol
                .get_neighbors(node)
                .iter()
                .filter(|neighbor| nodes.contains(neighbor))
                .copied()
                .collect();
            neighbors.sort_unstable();
            (node, neighbors)
        })
        .collect();
    let mut discovery = BTreeMap::new();
    let mut low = BTreeMap::new();
    let mut parent = BTreeMap::new();
    let mut edge_stack = Vec::new();
    let mut blocks = Vec::new();
    let mut counter = 0;

    for &root in nodes {
        if discovery.contains_key(&root) {
            continue;
        }
        counter += 1;
        discovery.insert(root, counter);
        low.insert(root, counter);
        parent.insert(root, None);
        let mut stack = vec![(root, 0)];
        while let Some(&(node, neighbor_index)) = stack.last() {
            let neighbors = &adjacency[&node];
            if let Some(&neighbor) = neighbors.get(neighbor_index) {
                if let Some(frame) = stack.last_mut() {
                    frame.1 += 1;
                }
                let edge = normalize_edge(node, neighbor);
                if let Entry::Vacant(slot) = discovery.entry(neighbor) {
                    counter += 1;
                    slot.insert(counter);
                    parent.insert(neighbor, Some(node));
                    edge_stack.push(edge);
                    low.insert(neighbor, counter);
                    stack.push((neighbor, 0));
                } else if Some(neighbor) != parent[&node] && discovery[&neighbor] < discovery[&node]
                {
                    edge_stack.push(edge);
                    low.insert(node, low[&node].min(discovery[&neighbor]));
                }
                continue;
            }

            stack.pop();
            let Some(ancestor) = parent[&node] else {
                if !edge_stack.is_empty() {
                    blocks.push(edge_stack.drain(..).collect());
                }
                continue;
            };
            low.insert(ancestor, low[&ancestor].min(low[&node]));
            if low[&node] < discovery[&ancestor] {
                continue;
            }
            let tree_edge = normalize_edge(ancestor, node);
            let mut block = EdgeSet::new();
            while let Some(stacked) = edge_stack.pop() {
                block.insert(stacked);
                if stacked == tree_edge {
                    break;
                }
            }
            if !block.is_empty() {
                blocks.push(block);
            }
        }
    }

    blocks.sort_by(|left, right| {
        (left.first(), left.len(), left).cmp(&(right.first(), right.len(), right))
    });
    blocks
}

#[cfg(test)]
mod tests {
    use super::super::molecule::{Atom, Bond};
    use super::*;

    fn graph(edges: &[Edge]) -> Molecule {
        let mut mol = Molecule::new();
        for atom in edges.iter().flat_map(|&(u, v)| [u, v]).collect::<AtomSet>() {
            mol.add_atom(Atom::new(atom, "C")).unwrap();
        }
        for (idx, &(u, v)) in edges.iter().enumerate() {
            mol.add_bond(Bond::new(idx, u, v, 1)).unwrap();
        }
        mol
    }

    #[test]
    fn blocks_keep_bridge_edges_and_spiro_articulations() {
        let mol = graph(&[
            (0, 1),
            (1, 2),
            (2, 0),
            (2, 3),
            (3, 4),
            (4, 2),
            (4, 5),
            (5, 6),
            (6, 7),
            (7, 5),
        ]);
        assert_eq!(
            biconnected_edge_components(&mol, &mol.atoms.keys().copied().collect()),
            vec![
                EdgeSet::from([(0, 1), (0, 2), (1, 2)]),
                EdgeSet::from([(2, 3), (2, 4), (3, 4)]),
                EdgeSet::from([(4, 5)]),
                EdgeSet::from([(5, 6), (5, 7), (6, 7)]),
            ]
        );
    }

    #[test]
    fn fused_and_bridged_cycles_remain_one_block() {
        for edges in [
            vec![(0, 1), (1, 2), (2, 0), (1, 3), (3, 2)],
            vec![(0, 1), (1, 3), (0, 2), (2, 3), (0, 4), (4, 3)],
        ] {
            let mol = graph(&edges);
            assert_eq!(
                biconnected_edge_components(&mol, &mol.atoms.keys().copied().collect()),
                vec![
                    edges
                        .into_iter()
                        .map(|(u, v)| normalize_edge(u, v))
                        .collect()
                ]
            );
        }
    }
}
