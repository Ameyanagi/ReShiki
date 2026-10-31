//! Exact graph proofs for optional constitutional locants.
//!
//! Stage one supports paths and single cycles. Attachment equivalence uses
//! their complete automorphism groups; optional feature locants use the same
//! budgeted labeled-graph search and failure boundary as the upstream proof.
use super::{assembly_parts::AssemblyParts, assembly_utils::parse_locant};
use std::collections::{BTreeMap, BTreeSet};
pub const MAX_CANDIDATE_PLACEMENTS: usize = 10_000;
type Edge = (usize, usize);
type NodeLabel = (String, i32, Option<u16>, bool);
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Position {
    Node(usize),
    Edge(Edge),
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Feature {
    category: &'static str,
    key: String,
    positions: Vec<Position>,
}
struct Graph {
    locants: Vec<String>,
    nodes: Vec<NodeLabel>,
    edges: BTreeMap<Edge, usize>,
    automorphisms: Vec<Vec<usize>>,
}
fn edge(a: usize, b: usize) -> Edge {
    (a.min(b), a.max(b))
}
fn graph(parts: &AssemblyParts, include_isotope: bool) -> Option<Graph> {
    let mut locants: Vec<_> = parts
        .parent_atom_symbols_by_locant
        .keys()
        .cloned()
        .collect();
    locants.sort_by_key(|s| parse_locant(s));
    if locants.is_empty() {
        return None;
    }
    let index: BTreeMap<_, _> = locants
        .iter()
        .enumerate()
        .map(|(i, s)| (s.clone(), i))
        .collect();
    let nodes = locants
        .iter()
        .map(|l| {
            (
                parts.parent_atom_symbols_by_locant[l].clone(),
                parts
                    .parent_atom_charges_by_locant
                    .get(l)
                    .copied()
                    .unwrap_or(0),
                if include_isotope {
                    parts
                        .parent_atom_isotopes_by_locant
                        .get(l)
                        .copied()
                        .flatten()
                } else {
                    None
                },
                parts.indicated_hydrogens.contains(l),
            )
        })
        .collect();
    let mut edges = BTreeMap::new();
    let mut adj = vec![vec![]; locants.len()];
    for ((a, b), order) in &parts.parent_bond_orders_by_locants {
        let (Some(&i), Some(&j)) = (index.get(a), index.get(b)) else {
            return None;
        };
        edges.insert(edge(i, j), *order);
        adj[i].push(j);
        adj[j].push(i);
    }
    let n = locants.len();
    let ring = adj.iter().all(|v| v.len() == 2) && n >= 3;
    let path = adj.iter().filter(|v| v.len() == 1).count() == 2 && adj.iter().all(|v| v.len() <= 2);
    if !ring && !path {
        return None;
    }
    let start = if ring {
        0
    } else {
        adj.iter().position(|v| v.len() == 1)?
    };
    let mut route = vec![start];
    let mut prev = usize::MAX;
    let mut current = start;
    while route.len() < n {
        let next = *adj[current]
            .iter()
            .find(|&&v| v != prev && !route.contains(&v))?;
        route.push(next);
        prev = current;
        current = next;
    }
    let mut automorphisms = Vec::new();
    for reverse in [false, true] {
        for shift in 0..if ring { n } else { 1 } {
            let mut p = vec![0; n];
            for i in 0..n {
                let j = if reverse {
                    (n - 1 - i + shift) % n
                } else {
                    (i + shift) % n
                };
                p[route[i]] = route[j];
            }
            automorphisms.push(p);
        }
    }
    Some(Graph {
        locants,
        nodes,
        edges,
        automorphisms,
    })
}
fn labels_match(g: &Graph, p: &[usize]) -> bool {
    (0..g.nodes.len()).all(|i| g.nodes[i] == g.nodes[p[i]])
        && g.edges
            .iter()
            .all(|(&(a, b), &o)| g.edges.get(&edge(p[a], p[b])) == Some(&o))
}
pub fn retained_parent_attachment_is_ambiguous(parts: &AssemblyParts, locs: &[String]) -> bool {
    if parts.retained_name.is_none()
        || !parts.is_ring
        || parts.is_bicycle
        || parts.is_spiro
        || parts.is_polycycle
        || locs.len() != 1
    {
        return false;
    }
    if !parts.parent_atom_symbols_by_locant.contains_key(&locs[0]) {
        return true;
    }
    let Some(g) = graph(parts, false) else {
        return true;
    };
    let Some(source) = g.locants.iter().position(|l| l == &locs[0]) else {
        return true;
    };
    let orbit: BTreeSet<_> = g
        .automorphisms
        .iter()
        .filter(|p| labels_match(&g, p))
        .map(|p| p[source])
        .collect();
    orbit.len() < g.nodes.len()
}
pub fn substituent_locant_set_is_unique(
    parts: &AssemblyParts,
    locs: &[String],
    grouped_count: usize,
) -> bool {
    if locs.is_empty()
        || parts.locant_map_source != "generated"
        || grouped_count != 1
        || parts.is_substituent
        || parts.is_bicycle
        || parts.is_spiro
        || parts.is_polycycle
        || parts.principal_group.is_some()
        || !parts.unsaturations.is_empty()
        || !parts.a_prefixes.is_empty()
    {
        return false;
    }
    if locs.iter().collect::<BTreeSet<_>>().len() != locs.len()
        || locs.iter().any(|s| !s.chars().all(|c| c.is_ascii_digit()))
        || (locs.len() == 1 && !parts.is_ring)
        || (locs.len() == 1 && retained_parent_attachment_is_ambiguous(parts, locs))
    {
        return false;
    }
    let Some(g) = graph(parts, false) else {
        return false;
    };
    let selected: Option<BTreeSet<_>> = locs
        .iter()
        .map(|l| g.locants.iter().position(|s| s == l))
        .collect();
    let Some(selected) = selected else {
        return false;
    };
    let Some(&first_selected) = selected.first() else {
        return false;
    };
    let label = &g.nodes[first_selected];
    if selected.iter().any(|&i| &g.nodes[i] != label) {
        return false;
    }
    let candidates: Vec<_> = g
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, l)| *l == label)
        .map(|(i, _)| i)
        .collect();
    if candidates.iter().any(|&i| {
        g.edges
            .iter()
            .filter(|(e, _)| e.0 == i || e.1 == i)
            .map(|(_, o)| *o)
            .sum::<usize>()
            < 3
    }) {
        return false;
    }
    if selected.len() == candidates.len() {
        return true;
    }
    if candidates.len() > 12 && selected.len() != 1 && selected.len() + 1 != candidates.len() {
        return false;
    }
    let mut budget = MAX_CANDIDATE_PLACEMENTS;
    let Some(options) = combinations(&candidates, selected.len(), &mut budget) else {
        return false;
    };
    options.iter().all(|target| {
        let target: BTreeSet<_> = target.iter().copied().collect();
        g.automorphisms.iter().any(|p| {
            labels_match(&g, p) && selected.iter().map(|&i| p[i]).collect::<BTreeSet<_>>() == target
        })
    })
}
fn consume(budget: &mut usize) -> Option<()> {
    if *budget == 0 {
        None
    } else {
        *budget -= 1;
        Some(())
    }
}
fn combinations<T: Clone>(items: &[T], k: usize, budget: &mut usize) -> Option<Vec<Vec<T>>> {
    fn recur<T: Clone>(
        items: &[T],
        k: usize,
        start: usize,
        current: &mut Vec<T>,
        out: &mut Vec<Vec<T>>,
        budget: &mut usize,
    ) -> Option<()> {
        if current.len() == k {
            consume(budget)?;
            out.push(current.clone());
            return Some(());
        }
        if k - current.len() > items.len().saturating_sub(start) {
            return Some(());
        }
        for i in start..=items.len() - (k - current.len()) {
            current.push(items[i].clone());
            recur(items, k, i + 1, current, out, budget)?;
            current.pop();
        }
        Some(())
    }
    if k > items.len() {
        return Some(vec![]);
    }
    let mut out = vec![];
    recur(items, k, 0, &mut vec![], &mut out, budget)?;
    Some(out)
}
fn supported(parts: &AssemblyParts) -> bool {
    !(parts.is_substituent
        || parts.is_bicycle
        || parts.is_spiro
        || parts.is_polycycle
        || !parts.stereo_features.is_empty()
        || !parts.relative_stereo_prefixes.is_empty()
        || !parts.indicated_hydrogens.is_empty()
        || !parts.parent_charges.is_empty()
        || !parts.a_prefixes.is_empty()
        || !parts.front_modifier_locants.is_empty()
        || !parts.principal_suffix_modifiers.is_empty()
        || !parts.hydro_operations.is_empty()
        || parts
            .parent_atom_charges_by_locant
            .values()
            .any(|&v| v != 0))
        && parts
            .parent_atom_symbols_by_locant
            .keys()
            .all(|l| !l.is_empty() && l.chars().all(|c| c.is_ascii_digit()))
}
fn features(parts: &AssemblyParts, g: &Graph) -> Vec<Feature> {
    let mut result = Vec::new();
    let mut subs: BTreeMap<String, Vec<Position>> = BTreeMap::new();
    for item in &parts.substituents {
        let positions: Option<Vec<_>> = item
            .locants
            .iter()
            .map(|l| g.locants.iter().position(|s| s == l).map(Position::Node))
            .collect();
        if let Some(positions) = positions {
            subs.entry(item.name.clone()).or_default().extend(positions);
        }
    }
    for (key, positions) in subs {
        if !positions.is_empty() {
            result.push(Feature {
                category: "substituent",
                key,
                positions,
            });
        }
    }
    if let Some(group) = &parts.principal_group {
        if group.locants.len() == 1
            && parts.substituents.is_empty()
            && g.nodes.iter().all(|l| l.0 == "C")
            && let Some(i) = g.locants.iter().position(|l| l == &group.locants[0])
        {
            result.push(Feature {
                category: "suffix",
                key: group.key.clone(),
                positions: vec![Position::Node(i)],
            });
        }
        return result;
    }
    let index: BTreeMap<_, _> = g.locants.iter().enumerate().map(|(i, l)| (l, i)).collect();
    let bonds: BTreeMap<_, _> = parts
        .parent_bond_ids_by_locants
        .iter()
        .map(|((a, b), id)| (*id, edge(index[a], index[b])))
        .collect();
    let mut unsats: BTreeMap<String, Vec<Position>> = BTreeMap::new();
    for u in &parts.unsaturations {
        let positions: Vec<_> = u
            .bond_ids
            .iter()
            .filter_map(|id| bonds.get(id).copied().map(Position::Edge))
            .collect();
        if positions.len() == u.locants.len() {
            unsats
                .entry(u.bond_key.clone())
                .or_default()
                .extend(positions);
        }
    }
    for (key, positions) in unsats {
        result.push(Feature {
            category: "unsaturation",
            key,
            positions,
        });
    }
    result
}
type Decoration = (
    Vec<Vec<(String, String)>>,
    BTreeMap<Edge, Vec<(String, String)>>,
);
fn decorate(g: &Graph, groups: &[Feature]) -> Decoration {
    let mut nodes = vec![vec![]; g.nodes.len()];
    let mut edges: BTreeMap<_, _> = g.edges.keys().map(|e| (*e, vec![])).collect();
    for f in groups {
        let marker = (f.category.to_owned(), f.key.clone());
        for p in &f.positions {
            match p {
                Position::Node(i) => nodes[*i].push(marker.clone()),
                Position::Edge(e) => edges.entry(*e).or_default().push(marker.clone()),
            }
        }
    }
    for n in &mut nodes {
        n.sort();
    }
    for e in edges.values_mut() {
        e.sort();
    }
    (nodes, edges)
}
fn signature(p: &Position, g: &Graph) -> String {
    match p {
        Position::Node(i) => format!("{:?}", g.nodes[*i]),
        Position::Edge((a, b)) => {
            let mut labels = [g.nodes[*a].clone(), g.nodes[*b].clone()];
            labels.sort();
            format!("{:?}:{:?}", labels, g.edges[&edge(*a, *b)])
        }
    }
}
fn isomorphic(
    g: &Graph,
    actual: &Decoration,
    candidate: &Decoration,
    budget: &mut usize,
) -> Option<bool> {
    type EdgeLabel = (usize, Vec<(String, String)>);
    let n = g.nodes.len();
    let adjacency = |decoration: &Decoration| {
        let mut adj = vec![BTreeMap::<usize, EdgeLabel>::new(); n];
        for (&(a, b), &order) in &g.edges {
            let label = (order, decoration.1[&(a, b)].clone());
            adj[a].insert(b, label.clone());
            adj[b].insert(a, label);
        }
        adj
    };
    let source_adj = adjacency(actual);
    let target_adj = adjacency(candidate);
    let multiset = |neighbors: &BTreeMap<usize, EdgeLabel>| {
        let mut labels: Vec<_> = neighbors.values().cloned().collect();
        labels.sort();
        labels
    };
    let source_signatures: Vec<_> = source_adj.iter().map(multiset).collect();
    let target_signatures: Vec<_> = target_adj.iter().map(multiset).collect();
    type LabeledAdjacency = [BTreeMap<usize, EdgeLabel>];
    type EdgeSignatures = [Vec<EdgeLabel>];
    fn search(
        graphs: (&Graph, &Decoration, &Decoration),
        adjacency: (&LabeledAdjacency, &LabeledAdjacency),
        signatures: (&EdgeSignatures, &EdgeSignatures),
        mapping: &mut [Option<usize>],
        used: &mut [bool],
        depth: usize,
        budget: &mut usize,
    ) -> Option<bool> {
        let (g, actual, candidate) = graphs;
        let (source_adj, target_adj) = adjacency;
        let (source_signatures, target_signatures) = signatures;
        if depth == mapping.len() {
            return Some(true);
        }
        // Preserve Python min's stable tie order and its ascending assigned-neighbor count.
        let source = (0..mapping.len())
            .filter(|&i| mapping[i].is_none())
            .min_by_key(|&i| {
                source_adj[i]
                    .keys()
                    .filter(|&&j| mapping[j].is_some())
                    .count()
            })?;
        for target in 0..mapping.len() {
            consume(budget)?;
            if used[target]
                || g.nodes[source] != g.nodes[target]
                || actual.0[source] != candidate.0[target]
                || source_signatures[source] != target_signatures[target]
            {
                continue;
            }
            if mapping.iter().enumerate().any(|(other, &mapped)| {
                mapped.is_some_and(|mapped| {
                    source_adj[source].get(&other) != target_adj[target].get(&mapped)
                })
            }) {
                continue;
            }
            mapping[source] = Some(target);
            used[target] = true;
            if search(
                graphs,
                adjacency,
                signatures,
                mapping,
                used,
                depth + 1,
                budget,
            )? {
                return Some(true);
            }
            mapping[source] = None;
            used[target] = false;
        }
        Some(false)
    }
    search(
        (g, actual, candidate),
        (&source_adj, &target_adj),
        (&source_signatures, &target_signatures),
        &mut vec![None; n],
        &mut vec![false; n],
        0,
        budget,
    )
}
fn safe(g: &Graph, all: &[Feature], omitted: &[usize], budget: &mut usize) -> Option<bool> {
    let mut base = Graph {
        locants: g.locants.clone(),
        nodes: g.nodes.clone(),
        edges: g.edges.clone(),
        automorphisms: g.automorphisms.clone(),
    };
    for f in all.iter().filter(|f| f.category == "unsaturation") {
        for p in &f.positions {
            if let Position::Edge(e) = p {
                base.edges.insert(*e, 1);
            }
        }
    }
    let actual = decorate(&base, all);
    let mut options = Vec::new();
    for &i in omitted {
        let f = &all[i];
        let universe: Vec<_> = if f.category == "unsaturation" {
            base.edges.keys().copied().map(Position::Edge).collect()
        } else {
            (0..base.nodes.len()).map(Position::Node).collect()
        };
        let mut actual_sig: Vec<_> = f.positions.iter().map(|p| signature(p, &base)).collect();
        actual_sig.sort();
        let candidates = combinations(&universe, f.positions.len(), budget)?
            .into_iter()
            .filter(|candidate| {
                let mut sig: Vec<_> = candidate.iter().map(|p| signature(p, &base)).collect();
                sig.sort();
                sig == actual_sig
            })
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return Some(false);
        }
        options.push(candidates);
    }
    if options
        .iter()
        .try_fold(1_usize, |a, b| a.checked_mul(b.len()))
        .is_none_or(|n| n > MAX_CANDIDATE_PLACEMENTS)
    {
        return Some(false);
    }
    fn visit(
        graph: (&Graph, &Decoration),
        all: &[Feature],
        omitted: &[usize],
        options: &[Vec<Vec<Position>>],
        depth: usize,
        chosen: &mut Vec<Feature>,
        budget: &mut usize,
    ) -> Option<bool> {
        let (g, actual) = graph;
        if depth < omitted.len() {
            for positions in &options[depth] {
                let mut f = all[omitted[depth]].clone();
                f.positions = positions.clone();
                chosen.push(f);
                if !visit(graph, all, omitted, options, depth + 1, chosen, budget)? {
                    return Some(false);
                }
                chosen.pop();
            }
            return Some(true);
        }
        consume(budget)?;
        let candidate = decorate(g, chosen);
        isomorphic(g, actual, &candidate, budget)
    }
    let mut fixed: Vec<_> = all
        .iter()
        .enumerate()
        .filter(|(i, _)| !omitted.contains(i))
        .map(|(_, f)| f.clone())
        .collect();
    visit(
        (&base, &actual),
        all,
        omitted,
        &options,
        0,
        &mut fixed,
        budget,
    )
}
pub fn apply_redundant_locant_elision(parts: &mut AssemblyParts) {
    apply_redundant_locant_elision_with_limit(parts, MAX_CANDIDATE_PLACEMENTS)
}
pub fn apply_redundant_locant_elision_with_limit(parts: &mut AssemblyParts, limit: usize) {
    if !parts.omit_redundant_locants || !supported(parts) {
        return;
    }
    let Some(g) = graph(parts, true) else { return };
    if parts.is_ring != (g.edges.len() == g.nodes.len()) {
        return;
    }
    let all = features(parts, &g);
    let mut ordered: Vec<_> = all
        .iter()
        .enumerate()
        .filter(|(_, f)| f.category != "substituent")
        .map(|(i, _)| i)
        .collect();
    ordered.sort_by_key(|&i| {
        (
            if all[i].category == "unsaturation" {
                1
            } else {
                2
            },
            all[i].category,
            all[i].key.clone(),
        )
    });
    let mut budget = limit;
    let mut selected = vec![];
    'outer: for size in (1..=ordered.len()).rev() {
        let mut subset_budget = usize::MAX;
        let Some(subsets) = combinations(&ordered, size, &mut subset_budget) else {
            return;
        };
        for subset in subsets {
            if consume(&mut budget).is_none() {
                return;
            }
            match safe(&g, &all, &subset, &mut budget) {
                Some(true) => {
                    selected = subset;
                    break 'outer;
                }
                Some(false) => (),
                None => return,
            }
        }
    }
    for i in selected {
        let f = &all[i];
        if f.category == "unsaturation" {
            parts.elided_unsaturation_locants.insert(f.key.clone());
        } else {
            parts.elide_principal_group_locants = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::assembly_parts::{PrincipalGroupItem, UnsaturationItem};
    use super::*;

    fn parent(n: usize, ring: bool, heteroatom: bool) -> AssemblyParts {
        let mut parts = AssemblyParts::new(n);
        parts.is_ring = ring;
        for i in 1..=n {
            parts.parent_atom_symbols_by_locant.insert(
                i.to_string(),
                if i == 1 && heteroatom { "N" } else { "C" }.into(),
            );
        }
        for i in 1..if ring { n + 1 } else { n } {
            let pair = (i.to_string(), (i % n + 1).to_string());
            parts.parent_bond_orders_by_locants.insert(pair.clone(), 1);
            parts.parent_bond_ids_by_locants.insert(pair, i - 1);
        }
        parts
    }
    #[test]
    fn elision_requires_every_compatible_placement_to_be_equivalent() {
        for (n, ring, heteroatom, expected) in [
            (3, false, false, true),
            (4, false, false, false),
            (6, true, false, true),
            (6, true, true, true),
        ] {
            let mut p = parent(n, ring, heteroatom);
            p.parent_bond_orders_by_locants
                .insert(("1".into(), "2".into()), 2);
            p.unsaturations.push(UnsaturationItem {
                bond_key: "double".into(),
                locants: vec!["1".into()],
                bond_ids: BTreeSet::from([0]),
                ..Default::default()
            });
            apply_redundant_locant_elision(&mut p);
            assert_eq!(
                p.elided_unsaturation_locants.contains("double"),
                expected,
                "n={n}, ring={ring}, heteroatom={heteroatom}"
            );
        }
        let mut p = parent(6, true, false);
        p.principal_group = Some(PrincipalGroupItem::new("alcohol", vec!["1".into()]));
        apply_redundant_locant_elision(&mut p);
        assert!(p.elide_principal_group_locants);
        let mut p = parent(6, true, true);
        p.principal_group = Some(PrincipalGroupItem::new("alcohol", vec!["1".into()]));
        apply_redundant_locant_elision(&mut p);
        assert!(!p.elide_principal_group_locants);

        // Match the upstream conservative search limit even for a symmetric
        // placement that could be established with a faster specialized proof.
        let mut p = parent(9, true, false);
        p.principal_group = Some(PrincipalGroupItem::new("alcohol", vec!["6".into()]));
        apply_redundant_locant_elision(&mut p);
        assert!(!p.elide_principal_group_locants);
    }
}
