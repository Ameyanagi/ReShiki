//! Exact component overlap graphs and their independently checked topology.
use std::collections::{BTreeMap, BTreeSet};

use super::model::{Edge, EdgeSet, normalize_edge};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentScope<T: Ord> {
    pub key: T,
    pub atom_ids: BTreeSet<usize>,
    pub edges: EdgeSet,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FusionInterface<T> {
    pub left: T,
    pub right: T,
    pub shared_atom_ids: BTreeSet<usize>,
    pub shared_edges: EdgeSet,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoverGraph<T: Ord> {
    pub nodes: Vec<T>,
    pub interfaces: Vec<FusionInterface<T>>,
    pub adjacency: BTreeMap<T, Vec<T>>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoverProof {
    pub kind: String,
    pub cycle_rank: usize,
    pub topology: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentCoverAudit<T: Ord> {
    pub ok: bool,
    pub errors: Vec<String>,
    pub graph: CoverGraph<T>,
    pub proof: CoverProof,
    pub reconstructed_atom_ids: BTreeSet<usize>,
    pub reconstructed_edges: EdgeSet,
}
pub fn component_scope<T: Ord>(
    key: T,
    atom_ids: impl IntoIterator<Item = usize>,
    edges: impl IntoIterator<Item = Edge>,
) -> Result<ComponentScope<T>, String> {
    let atom_ids: BTreeSet<_> = atom_ids.into_iter().collect();
    let edges: EdgeSet = edges
        .into_iter()
        .map(|(u, v)| normalize_edge(u, v))
        .collect();
    if edges
        .iter()
        .any(|(u, v)| !atom_ids.contains(u) || !atom_ids.contains(v))
    {
        return Err("Every component edge must have both endpoints in atom_ids".into());
    }
    Ok(ComponentScope {
        key,
        atom_ids,
        edges,
    })
}
pub fn build_cover_graph<T: Ord + Clone>(
    nodes: impl IntoIterator<Item = T>,
    interfaces: impl IntoIterator<Item = FusionInterface<T>>,
) -> CoverGraph<T> {
    let mut ordered = Vec::new();
    let mut positions = BTreeMap::new();
    for node in nodes {
        if !positions.contains_key(&node) {
            positions.insert(node.clone(), ordered.len());
            ordered.push(node);
        }
    }
    let mut adjacency: BTreeMap<T, Vec<T>> = ordered
        .iter()
        .cloned()
        .map(|node| (node, Vec::new()))
        .collect();
    let mut normalized = Vec::new();
    let mut seen = BTreeSet::new();
    for mut interface in interfaces {
        let (Some(&left), Some(&right)) = (
            positions.get(&interface.left),
            positions.get(&interface.right),
        ) else {
            continue;
        };
        if left == right || !seen.insert(normalize_edge(left, right)) {
            continue;
        }
        if left > right {
            std::mem::swap(&mut interface.left, &mut interface.right);
        }
        adjacency
            .entry(interface.left.clone())
            .or_default()
            .push(interface.right.clone());
        adjacency
            .entry(interface.right.clone())
            .or_default()
            .push(interface.left.clone());
        normalized.push(interface);
    }
    for neighbors in adjacency.values_mut() {
        neighbors.sort_by_key(|node| positions[node]);
    }
    normalized.sort_by_key(|edge| (positions[&edge.left], positions[&edge.right]));
    CoverGraph {
        nodes: ordered,
        interfaces: normalized,
        adjacency,
    }
}
pub fn build_component_cover_graph<T: Ord + Clone>(
    scopes: &[ComponentScope<T>],
) -> Result<CoverGraph<T>, String> {
    if scopes.iter().map(|s| &s.key).collect::<BTreeSet<_>>().len() != scopes.len() {
        return Err("Component keys must be unique".into());
    }
    let mut interfaces = Vec::new();
    for (index, left) in scopes.iter().enumerate() {
        for right in &scopes[index + 1..] {
            let shared_edges: EdgeSet = left.edges.intersection(&right.edges).copied().collect();
            if shared_edges.is_empty() {
                continue;
            }
            interfaces.push(FusionInterface {
                left: left.key.clone(),
                right: right.key.clone(),
                shared_atom_ids: shared_edges.iter().flat_map(|&(u, v)| [u, v]).collect(),
                shared_edges,
            });
        }
    }
    Ok(build_cover_graph(
        scopes.iter().map(|s| s.key.clone()),
        interfaces,
    ))
}
fn connected_nodes<'a, T: Ord>(graph: &'a CoverGraph<T>, start: &'a T) -> BTreeSet<&'a T> {
    let mut seen = BTreeSet::new();
    let mut stack = vec![start];
    while let Some(node) = stack.pop() {
        if !seen.insert(node) {
            continue;
        }
        stack.extend(graph.adjacency[node].iter().filter(|n| !seen.contains(*n)));
    }
    seen
}
fn connected_components<T: Ord>(graph: &CoverGraph<T>) -> usize {
    let mut seen = BTreeSet::new();
    let mut count = 0;
    for node in &graph.nodes {
        if seen.contains(node) {
            continue;
        }
        count += 1;
        seen.extend(connected_nodes(graph, node));
    }
    count
}
pub fn is_connected<T: Ord>(graph: &CoverGraph<T>) -> bool {
    !graph.nodes.is_empty() && connected_nodes(graph, &graph.nodes[0]).len() == graph.nodes.len()
}
pub fn cycle_rank<T: Ord>(graph: &CoverGraph<T>) -> usize {
    graph.interfaces.len() + connected_components(graph) - graph.nodes.len()
}
fn is_cactus_graph<T: Ord>(graph: &CoverGraph<T>) -> bool {
    struct Walk<'a, T: Ord> {
        discovery: BTreeMap<&'a T, usize>,
        low: BTreeMap<&'a T, usize>,
        stack: Vec<(&'a T, &'a T)>,
        blocks: Vec<Vec<(&'a T, &'a T)>>,
        counter: usize,
    }
    fn visit<'a, T: Ord>(
        node: &'a T,
        parent: Option<&T>,
        graph: &'a CoverGraph<T>,
        state: &mut Walk<'a, T>,
    ) {
        state.counter += 1;
        state.discovery.insert(node, state.counter);
        state.low.insert(node, state.counter);
        for neighbor in &graph.adjacency[node] {
            if Some(neighbor) == parent {
                continue;
            }
            let edge = (node, neighbor);
            if !state.discovery.contains_key(neighbor) {
                state.stack.push(edge);
                visit(neighbor, Some(node), graph, state);
                state
                    .low
                    .insert(node, state.low[node].min(state.low[neighbor]));
                if state.low[neighbor] >= state.discovery[node] {
                    let mut block = Vec::new();
                    while let Some(stacked) = state.stack.pop() {
                        block.push(stacked);
                        if stacked == edge {
                            break;
                        }
                    }
                    if block.len() > 1 {
                        state.blocks.push(block);
                    }
                }
            } else if state.discovery[neighbor] < state.discovery[node] {
                state.stack.push(edge);
                state
                    .low
                    .insert(node, state.low[node].min(state.discovery[neighbor]));
            }
        }
    }
    let mut state = Walk {
        discovery: BTreeMap::new(),
        low: BTreeMap::new(),
        stack: Vec::new(),
        blocks: Vec::new(),
        counter: 0,
    };
    for node in &graph.nodes {
        if !state.discovery.contains_key(node) {
            visit(node, None, graph, &mut state);
        }
    }
    !state.blocks.is_empty()
        && state.blocks.iter().all(|block| {
            block.len()
                == block
                    .iter()
                    .flat_map(|(u, v)| [u, v])
                    .collect::<BTreeSet<_>>()
                    .len()
        })
}
pub fn build_cover_proof<T: Ord>(graph: &CoverGraph<T>) -> CoverProof {
    let cycle_rank = cycle_rank(graph);
    let (kind, topology) = if !is_connected(graph) {
        ("disconnected", "disconnected")
    } else if cycle_rank == 0 {
        ("tree", "tree")
    } else if cycle_rank == 1 {
        ("multiparent", "unicyclic")
    } else if is_cactus_graph(graph) {
        ("multiparent", "cactus")
    } else {
        ("multiparent", "complex")
    };
    CoverProof {
        kind: kind.into(),
        cycle_rank,
        topology: topology.into(),
    }
}
pub fn audit_component_cover<T: Ord + Clone>(
    scopes: &[ComponentScope<T>],
    target_atom_ids: &BTreeSet<usize>,
    target_edges: &EdgeSet,
) -> Result<ComponentCoverAudit<T>, String> {
    let graph = build_component_cover_graph(scopes)?;
    let proof = build_cover_proof(&graph);
    let reconstructed_atom_ids = scopes
        .iter()
        .flat_map(|s| s.atom_ids.iter().copied())
        .collect();
    let reconstructed_edges = scopes
        .iter()
        .flat_map(|s| s.edges.iter().copied())
        .collect();
    let target_edges: EdgeSet = target_edges
        .iter()
        .map(|&(u, v)| normalize_edge(u, v))
        .collect();
    let mut errors = Vec::new();
    if scopes.len() < 2 {
        errors.push("a fusion cover requires at least two components".into());
    }
    if &reconstructed_atom_ids != target_atom_ids {
        errors.push("component atoms do not exactly reconstruct the target".into());
    }
    if reconstructed_edges != target_edges {
        errors.push("component edges do not exactly reconstruct the target".into());
    }
    if proof.kind == "disconnected" {
        errors.push("component overlap graph is disconnected".into());
    }
    let mut edge_counts = BTreeMap::new();
    for edge in scopes.iter().flat_map(|s| &s.edges) {
        *edge_counts.entry(edge).or_insert(0usize) += 1;
    }
    if edge_counts.values().any(|&count| count > 2) {
        errors.push("a molecular edge belongs to more than two components".into());
    }
    Ok(ComponentCoverAudit {
        ok: errors.is_empty(),
        errors,
        graph,
        proof,
        reconstructed_atom_ids,
        reconstructed_edges,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph(nodes: usize, edges: &[(usize, usize)]) -> CoverGraph<usize> {
        build_cover_graph(
            0..nodes,
            edges.iter().map(|&(left, right)| FusionInterface {
                left,
                right,
                shared_atom_ids: BTreeSet::new(),
                shared_edges: BTreeSet::new(),
            }),
        )
    }

    #[test]
    fn component_overlap_topology_proves_cyclic_blocks() {
        assert_eq!(build_cover_proof(&graph(3, &[(0, 1), (1, 2)])).kind, "tree");
        assert_eq!(
            build_cover_proof(&graph(3, &[(0, 1), (1, 2), (2, 0)])).topology,
            "unicyclic"
        );
        assert_eq!(
            build_cover_proof(&graph(5, &[(0, 1), (1, 2), (2, 0), (2, 3), (3, 4), (4, 2)]))
                .topology,
            "cactus"
        );
        assert_eq!(
            build_cover_proof(&graph(4, &[(0, 1), (1, 2), (2, 0), (1, 3), (3, 2)])).topology,
            "complex"
        );
        assert_eq!(
            build_cover_proof(&graph(4, &[(0, 1), (2, 3)])).kind,
            "disconnected"
        );
    }

    #[test]
    fn audit_rejects_unreconstructed_edges_and_triple_membership() {
        let scopes = vec![
            component_scope(0, [0, 1, 2], [(0, 1), (1, 2)]).unwrap(),
            component_scope(1, [1, 2, 3], [(1, 2), (2, 3)]).unwrap(),
            component_scope(2, [1, 2, 4], [(1, 2), (2, 4)]).unwrap(),
        ];
        let audit = audit_component_cover(
            &scopes,
            &[0, 1, 2, 3, 4].into_iter().collect(),
            &[(0, 1), (1, 2), (2, 3), (2, 4), (0, 4)]
                .into_iter()
                .collect(),
        )
        .unwrap();
        assert!(!audit.ok);
        assert!(audit.errors.iter().any(|e| e.contains("more than two")));
        assert!(
            audit
                .errors
                .iter()
                .any(|e| e.contains("edges do not exactly"))
        );
    }
}
