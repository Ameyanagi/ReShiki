//! Exact graph proofs for optional constitutional locants.
//!
//! Constitutional and stereochemical locants share the same decorated-parent
//! graph and equivalence proof.
use super::{
    assembly_parts::AssemblyParts,
    assembly_utils::parse_locant,
    graph_kernel::{Edge, normalize_edge},
};
use std::collections::{BTreeMap, BTreeSet};
const MAX_CANDIDATE_PLACEMENTS: usize = 10_000;
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
    substituent_capacity: Vec<usize>,
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
        .map(|(i, s)| (s.as_str(), i))
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
        let (Some(&i), Some(&j)) = (index.get(a.as_str()), index.get(b.as_str())) else {
            return None;
        };
        edges.insert(normalize_edge(i, j), *order);
        adj[i].push(j);
        adj[j].push(i);
    }
    let n = locants.len();
    let ring = adj.iter().all(|v| v.len() == 2) && n >= 3;
    let path = adj.iter().filter(|v| v.len() == 1).count() == 2 && adj.iter().all(|v| v.len() <= 2);
    let substituent_atoms: BTreeSet<_> = parts
        .substituents
        .iter()
        .flat_map(|s| &s.atom_ids)
        .collect();
    let substituent_capacity = locants
        .iter()
        .map(|l| {
            parts
                .parent_atom_hydrogens_by_locant
                .get(l)
                .copied()
                .unwrap_or(0)
                + parts
                    .parent_external_bonds_by_locant
                    .get(l)
                    .into_iter()
                    .flatten()
                    .filter(|(atom, _, _)| substituent_atoms.contains(atom))
                    .map(|(_, order, _)| order)
                    .sum::<usize>()
        })
        .collect();
    if !ring && !path && n != 1 {
        return Some(Graph {
            locants,
            nodes,
            edges,
            automorphisms: vec![],
            substituent_capacity,
        });
    }
    let start = if ring || n == 1 {
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
        substituent_capacity,
    })
}
fn labels_match(g: &Graph, p: &[usize]) -> bool {
    (0..g.nodes.len()).all(|i| g.nodes[i] == g.nodes[p[i]])
        && g.edges
            .iter()
            .all(|(&(a, b), &o)| g.edges.get(&normalize_edge(p[a], p[b])) == Some(&o))
}
pub fn retained_parent_attachment_is_ambiguous(parts: &AssemblyParts, locs: &[String]) -> bool {
    if parts.retained_name.is_none() || !parts.is_simple_ring() || locs.len() != 1 {
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
        || parts.proof_locant_map
        || grouped_count != 1
        || parts.is_substituent
        || parts.is_bicycle
        || parts.spiro_prefix_composition
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
    let Some(options) = combinations(&candidates, selected.len(), false, &mut budget) else {
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
fn combinations<T: Clone>(
    items: &[T],
    k: usize,
    repeated: bool,
    budget: &mut usize,
) -> Option<Vec<Vec<T>>> {
    fn recur<T: Clone>(
        items: &[T],
        k: usize,
        repeated: bool,
        current: &mut Vec<T>,
        out: &mut Vec<Vec<T>>,
        budget: &mut usize,
    ) -> Option<()> {
        if k == 0 {
            consume(budget)?;
            out.push(current.clone());
            return Some(());
        }
        if items.is_empty() || !repeated && k > items.len() {
            return Some(());
        }
        for i in 0..if repeated {
            items.len()
        } else {
            items.len() - k + 1
        } {
            current.push(items[i].clone());
            recur(
                &items[i + usize::from(!repeated)..],
                k - 1,
                repeated,
                current,
                out,
                budget,
            )?;
            current.pop();
        }
        Some(())
    }
    let mut out = vec![];
    recur(items, k, repeated, &mut vec![], &mut out, budget)?;
    Some(out)
}
fn supported(parts: &AssemblyParts) -> bool {
    // P-65.6.3.3.1: a single ester alcohol component leaves acid locant elision intact.
    let single_ester_modifier = parts.principal_group.as_ref().is_some_and(|group| {
        group.key == "ester"
            && group.locants.len() == 1
            && parts.front_modifiers.len() == 1
            && parts.front_modifier_locants == [Some(group.locants[0].clone())]
    });
    !(parts.is_substituent
        || parts.is_bicycle
        || parts.is_spiro
        || parts.is_polycycle
        || parts.stereo_features.iter().collect::<BTreeSet<_>>().len() > 1
        || !parts.relative_stereo_prefixes.is_empty()
        || !parts.indicated_hydrogens.is_empty()
        || !parts.parent_charges.is_empty()
        || !parts.a_prefixes.is_empty()
        || !parts.front_modifier_locants.is_empty() && !single_ester_modifier
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
        let positions = group
            .locants
            .iter()
            .filter_map(|l| g.locants.iter().position(|s| s == l).map(Position::Node))
            .collect::<Vec<_>>();
        if !positions.is_empty() {
            result.push(Feature {
                category: "suffix",
                key: group.key.clone(),
                positions,
            });
        }
    }
    if parts.is_substituent
        && let Some(i) = g.locants.iter().position(|l| l == &parts.attachment_locant)
    {
        result.push(Feature {
            category: "attachment",
            key: parts.attachment_order.to_string(),
            positions: vec![Position::Node(i)],
        });
    }
    let index: BTreeMap<_, _> = g.locants.iter().enumerate().map(|(i, l)| (l, i)).collect();
    let bonds: BTreeMap<_, _> = parts
        .parent_bond_ids_by_locants
        .iter()
        .map(|((a, b), id)| (*id, normalize_edge(index[a], index[b])))
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
            let mut labels = [&g.nodes[*a], &g.nodes[*b]];
            labels.sort();
            format!("{:?}:{:?}", labels, g.edges[&normalize_edge(*a, *b)])
        }
    }
}
fn isomorphic<'a>(
    g: &Graph,
    actual: &'a Decoration,
    candidate: &'a Decoration,
    budget: &mut usize,
) -> Option<bool> {
    type EdgeLabel<'a> = (usize, &'a [(String, String)]);
    let n = g.nodes.len();
    let adjacency = |decoration: &'a Decoration| {
        let mut adj = vec![BTreeMap::<usize, EdgeLabel<'a>>::new(); n];
        for (&(a, b), &order) in &g.edges {
            let label = (order, decoration.1[&(a, b)].as_slice());
            adj[a].insert(b, label);
            adj[b].insert(a, label);
        }
        adj
    };
    let source_adj = adjacency(actual);
    let target_adj = adjacency(candidate);
    let multiset = |neighbors: &BTreeMap<usize, EdgeLabel<'a>>| {
        let mut labels: Vec<_> = neighbors.values().copied().collect();
        labels.sort();
        labels
    };
    let source_signatures: Vec<_> = source_adj.iter().map(multiset).collect();
    let target_signatures: Vec<_> = target_adj.iter().map(multiset).collect();
    type LabeledAdjacency<'a> = [BTreeMap<usize, EdgeLabel<'a>>];
    type EdgeSignatures<'a> = [Vec<EdgeLabel<'a>>];
    fn search(
        graphs: (&Graph, &Decoration, &Decoration),
        adjacency: (&LabeledAdjacency<'_>, &LabeledAdjacency<'_>),
        signatures: (&EdgeSignatures<'_>, &EdgeSignatures<'_>),
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
fn stereo_positions(parts: &AssemblyParts, g: &Graph, atom: bool) -> Vec<Position> {
    if atom {
        return g
            .locants
            .iter()
            .enumerate()
            .filter_map(|(i, l)| {
                parts
                    .parent_atom_ids_by_locant
                    .get(l)
                    .is_some_and(|id| parts.stereogenic_atom_ids.contains(id))
                    .then_some(Position::Node(i))
            })
            .collect();
    }
    let mut positions = Vec::new();
    for ((a, b), id) in &parts.parent_bond_ids_by_locants {
        if parts.stereogenic_bond_ids.contains(id)
            && let (Some(a), Some(b)) = (
                g.locants.iter().position(|l| l == a),
                g.locants.iter().position(|l| l == b),
            )
        {
            positions.push(Position::Edge(normalize_edge(a, b)));
        }
    }
    for (i, l) in g.locants.iter().enumerate() {
        if parts
            .parent_external_bonds_by_locant
            .get(l)
            .into_iter()
            .flatten()
            .any(|(_, _, id)| parts.stereogenic_bond_ids.contains(id))
        {
            positions.push(Position::Node(i));
        }
    }
    positions
}

/// P-91.3: a lone descriptor is unambiguous only when every possible unit of
/// that kind belongs to the same orbit of the substituted parent.
pub fn single_stereo_locant_is_unambiguous(parts: &AssemblyParts) -> bool {
    let distinct: BTreeSet<_> = parts.stereo_features.iter().collect();
    let Some((locant, descriptor)) = distinct.first().copied().filter(|_| distinct.len() == 1)
    else {
        return false;
    };
    if locant.is_empty() {
        return true;
    }
    let atom = !matches!(descriptor.as_str(), "E" | "Z")
        && parts
            .parent_atom_ids_by_locant
            .get(locant)
            .is_some_and(|id| parts.stereogenic_atom_ids.contains(id));
    if atom
        && parts
            .parent_atom_ids_by_locant
            .values()
            .filter(|id| parts.stereogenic_atom_ids.contains(id))
            .count()
            == 1
    {
        return true;
    }
    let Some(g) = graph(parts, true) else {
        return false;
    };
    let positions = stereo_positions(parts, &g, atom);
    let Some(position) = positions.iter().find(|p| match p {
        Position::Node(i) => &g.locants[*i] == locant,
        Position::Edge((a, b)) => &g.locants[*a] == locant || &g.locants[*b] == locant,
    }) else {
        return false;
    };
    if positions.len() == 1 {
        return true;
    }
    let mut all = features(parts, &g);
    let fixed = all.len();
    all.push(Feature {
        category: "stereo",
        key: if atom { "atom" } else { "bond" }.into(),
        positions: vec![position.clone()],
    });
    let actual = decorate(&g, &all);
    let mut budget = MAX_CANDIDATE_PLACEMENTS;
    positions
        .iter()
        .filter(|&p| p != position)
        .all(|candidate| {
            if signature(position, &g) != signature(candidate, &g) {
                return false;
            }
            all[fixed].positions[0] = candidate.clone();
            let candidate = decorate(&g, &all);
            if !g.automorphisms.is_empty() {
                return g.automorphisms.iter().any(|p| {
                    labels_match(&g, p)
                        && actual
                            .0
                            .iter()
                            .enumerate()
                            .all(|(i, labels)| labels == &candidate.0[p[i]])
                        && actual.1.iter().all(|(&(a, b), labels)| {
                            candidate.1.get(&normalize_edge(p[a], p[b])) == Some(labels)
                        })
                });
            }
            isomorphic(&g, &actual, &candidate, &mut budget) == Some(true)
        })
}

/// Functional-class shortcuts use the same proof on their complete molecular
/// word, whose constituent parents otherwise have separate assembly plans.
pub fn bond_stereo_locant_is_unambiguous(mol: &super::molecule::Molecule, bond: usize) -> bool {
    let mut parts = AssemblyParts::new(mol.atoms.len());
    parts.stereogenic_atom_ids = mol.stereogenic_atoms.clone();
    parts.stereogenic_bond_ids = mol.stereogenic_bonds.clone();
    for (&id, atom) in &mol.atoms {
        let locant = id.to_string();
        parts.parent_atom_ids_by_locant.insert(locant.clone(), id);
        parts
            .parent_atom_symbols_by_locant
            .insert(locant.clone(), atom.symbol.clone());
        parts
            .parent_atom_charges_by_locant
            .insert(locant.clone(), atom.charge);
        parts
            .parent_atom_isotopes_by_locant
            .insert(locant, atom.isotope);
    }
    for b in mol.bonds.values() {
        let pair = (b.u.to_string(), b.v.to_string());
        parts
            .parent_bond_orders_by_locants
            .insert(pair.clone(), b.order);
        parts.parent_bond_ids_by_locants.insert(pair, b.idx);
    }
    let b = &mol.bonds[&bond];
    parts
        .stereo_features
        .push((b.u.to_string(), b.stereo.clone().unwrap_or_default()));
    single_stereo_locant_is_unambiguous(&parts)
}

fn safe(g: &Graph, all: &[Feature], omitted: &[usize], budget: &mut usize) -> Option<bool> {
    let actual = decorate(g, all);
    let mut options = Vec::new();
    for &i in omitted {
        let f = &all[i];
        let universe: Vec<_> = if f.category == "unsaturation" {
            g.edges.keys().copied().map(Position::Edge).collect()
        } else {
            (0..g.nodes.len()).map(Position::Node).collect()
        };
        let mut actual_sig: Vec<_> = f.positions.iter().map(|p| signature(p, g)).collect();
        actual_sig.sort();
        let candidates = combinations(
            &universe,
            f.positions.len(),
            f.category == "substituent",
            budget,
        )?
        .into_iter()
        .filter(|candidate| {
            let mut sig: Vec<_> = candidate.iter().map(|p| signature(p, g)).collect();
            sig.sort();
            sig == actual_sig
                && (f.category != "substituent"
                    || candidate
                        .iter()
                        .all(|p| matches!(p, Position::Node(i) if g.substituent_capacity[*i] > 0)))
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
                let f = &all[omitted[depth]];
                chosen.push(Feature {
                    category: f.category,
                    key: f.key.clone(),
                    positions: positions.clone(),
                });
                if !visit(graph, all, omitted, options, depth + 1, chosen, budget)? {
                    return Some(false);
                }
                chosen.pop();
            }
            return Some(true);
        }
        consume(budget)?;
        let candidate = decorate(g, chosen);
        if omitted.iter().any(|&i| all[i].category == "substituent")
            && candidate.0.iter().enumerate().any(|(i, labels)| {
                labels
                    .iter()
                    .filter(|(category, _)| category == "substituent")
                    .count()
                    > g.substituent_capacity[i]
            })
        {
            return Some(true);
        }
        isomorphic(g, actual, &candidate, budget)
    }
    let mut fixed: Vec<_> = all
        .iter()
        .enumerate()
        .filter(|(i, _)| !omitted.contains(i))
        .map(|(_, f)| f.clone())
        .collect();
    visit((g, &actual), all, omitted, &options, 0, &mut fixed, budget)
}
pub fn apply_redundant_locant_elision(parts: &mut AssemblyParts) {
    if !parts.omit_redundant_locants || !supported(parts) {
        return;
    }
    let Some(mut g) = graph(parts, true) else {
        return;
    };
    if parts.is_ring != (g.edges.len() == g.nodes.len()) {
        return;
    }
    // P-14.3.4.4: propa-1,2-diene always cites its locants.
    if parts.principal_group.is_none()
        && !parts.is_ring
        && g.nodes.len() == 3
        && g.nodes.iter().all(|l| l.0 == "C")
        && g.edges.len() == 2
        && g.edges.values().all(|&order| order == 2)
    {
        return;
    }
    let all = features(parts, &g);
    for f in all.iter().filter(|f| f.category == "unsaturation") {
        for p in &f.positions {
            if let Position::Edge(e) = p {
                g.edges.insert(*e, 1);
            }
        }
    }
    let mut ordered: Vec<_> = all
        .iter()
        .enumerate()
        .filter(|(_, f)| match f.category {
            "substituent" => {
                parts
                    .principal_group
                    .as_ref()
                    .is_some_and(|group| !group.external_substitution_sites)
                    && parts.parent_atom_hydrogens_by_locant.len() == g.nodes.len()
            }
            "suffix" => {
                parts.substituents.is_empty()
                    && g.nodes.iter().all(|l| l.0 == "C")
                    && f.positions.len() == 1
            }
            "unsaturation" => parts.principal_group.is_none(),
            _ => false,
        })
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
            all[i].key.as_str(),
        )
    });
    let mut budget = MAX_CANDIDATE_PLACEMENTS;
    let mut selected = vec![];
    'outer: for size in (1..=ordered.len()).rev() {
        let mut subset_budget = usize::MAX;
        let Some(subsets) = combinations(&ordered, size, false, &mut subset_budget) else {
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
    if selected.is_empty() {
        return;
    }
    // P-14.3.3: omit all parent locants together, or cite them all.
    // Dinuclear unsaturation keeps its independent omission rule.
    if parts.principal_group.is_none() && g.nodes.len() != 2 && selected.len() != all.len() {
        selected = (0..all.len()).collect();
        if safe(&g, &all, &selected, &mut budget) != Some(true) {
            return;
        }
    }
    for i in selected {
        let f = &all[i];
        if f.category == "unsaturation" {
            parts.elided_unsaturation_locants.insert(f.key.clone());
        } else if f.category == "substituent" {
            parts.elided_substituent_locants.insert(f.key.clone());
        } else {
            parts.elide_principal_group_locants = true;
        }
    }
    if !parts.stereo_features.is_empty() {
        let cites_locants = super::assembly_prefixes::format_substituent_prefixes(parts).1
            || !super::assembly_parent::parent_stem_and_terminal(parts)
                .and_then(|(stem, terminal)| {
                    super::assembly_parent::format_parent_tail(parts, &stem, &terminal)
                })
                .is_ok_and(|tail| !tail.cites_locants);
        if cites_locants || !single_stereo_locant_is_unambiguous(parts) {
            parts.elided_substituent_locants.clear();
            parts.elided_unsaturation_locants.clear();
            parts.elide_principal_group_locants = false;
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
    fn long_stereogenic_chain_keeps_its_locants() {
        let smiles = format!("{}[C@H](F)Cl", "C".repeat(400));
        let name = super::super::name_smiles(&smiles).unwrap();
        assert_eq!(name.text, "(1R)-1-chloro-1-fluorohentetractane");
    }

    #[test]
    fn single_ester_acid_component_omits_only_redundant_locants() {
        let parser = opsin::Parser::new().unwrap();
        for (smiles, expected) in [
            ("FCC(=O)OC", "methyl fluoroacetate"),
            ("FC(F)C(=O)OC", "methyl difluoroacetate"),
            ("FC(F)(F)C(=O)OC", "methyl trifluoroacetate"),
            ("ClC(Cl)C(=O)OCC", "ethyl dichloroacetate"),
            ("COC(=O)Cc1ccc2[nH]ccc2c1", "methyl (1H-indol-5-yl)acetate"),
            ("COC(=O)[C@H](F)Cl", "methyl (R)-chlorofluoroacetate"),
            ("OCC(=O)OCC", "ethyl hydroxyacetate"),
            ("FCC(=O)Oc1ccccc1", "phenyl fluoroacetate"),
            ("FCC(=O)OCC(=O)OC", "2-methoxy-2-oxoethyl fluoroacetate"),
            ("FCC(=O)O", "fluoroacetic acid"),
            ("CC(=O)OC", "methyl acetate"),
            ("FCCC(=O)OC", "methyl 3-fluoropropanoate"),
            ("CC(F)C(=O)OC", "methyl 2-fluoropropanoate"),
            ("O=C(NC)CF", "2-fluoro-N-methylacetamide"),
            (
                "COC(=O)C(F)CC(=O)OCC",
                "4-ethyl 1-methyl 2-fluorobutanedioate",
            ),
            ("FCCOC(=O)CF", "2-fluoroethyl fluoroacetate"),
            (
                "C[C@H](CC)OC(=O)[C@H](F)Cl",
                "(2R)-butan-2-yl (R)-chlorofluoroacetate",
            ),
            (
                "C[C@@H](CC)OC(=O)[C@H](F)Cl",
                "(2S)-butan-2-yl (R)-chlorofluoroacetate",
            ),
            ("O=C(O)[C@H](F)Cl", "(R)-chlorofluoroacetic acid"),
            ("COC(=O)CC(=O)OCC", "1-ethyl 3-methyl propanedioate"),
        ] {
            let canonical = crate::naming::canonical_smiles(smiles).unwrap();
            let name = super::super::name_smiles(&canonical).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            assert_eq!(
                name.status,
                crate::naming::NameStatus::Preferred,
                "{smiles}"
            );
            let parsed = parser.parse(&name.text, &opsin::ParseOptions::strict());
            assert_eq!(
                parsed.status,
                opsin::Status::Success,
                "{expected}: {}",
                parsed.message
            );
            let actual = parsed.structure.unwrap().semantic_cxsmiles().unwrap();
            crate::naming::verify_identity(smiles, &actual).unwrap();
        }
        let input = reshiki_chemistry::smiles::read("C[C@H](CC)OC(=O)[C@H](F)Cl").unwrap();
        let labeled = reshiki_chemistry::stereo::cip::label::assign(
            &input.prepared.state,
            &Default::default(),
        )
        .unwrap();
        for atom in [1, 7] {
            assert_eq!(
                labeled.state.properties.atoms[atom].cip_code.as_deref(),
                Some("R")
            );
        }
    }

    #[test]
    fn unsaturated_parent_locants() {
        let parser = opsin::Parser::new().unwrap();
        for (smiles, expected) in [
            ("FC=C=C=CCl", "1-chloro-4-fluorobuta-1,2,3-triene"),
            ("ClC=C=C=C", "chlorobutatriene"),
            ("FC(Cl)=C=C=C", "1-chloro-1-fluorobuta-1,2,3-triene"),
            ("FC=C=CCl", "1-chloro-3-fluoropropa-1,2-diene"),
            ("ClC=C=C", "1-chloropropa-1,2-diene"),
            ("ClC=C=CCl", "1,3-dichloropropa-1,2-diene"),
            ("C=C=C", "propa-1,2-diene"),
            ("FC(F)=C=C", "1,1-difluoropropa-1,2-diene"),
            ("ClC=C=C=C=C", "chloropentatetraene"),
            ("ClC=C=C=C=C=C", "chlorohexapentaene"),
            ("FC=C=C=C=C=CCl", "1-chloro-6-fluorohexa-1,2,3,4,5-pentaene"),
            ("COC=C=C=COC", "1,4-dimethoxybuta-1,2,3-triene"),
            ("ClC(Cl)=C=C=C", "1,1-dichlorobuta-1,2,3-triene"),
            ("ClC=C=C=CCl", "1,4-dichlorobuta-1,2,3-triene"),
            ("BrC=C=C=CCl", "1-bromo-4-chlorobuta-1,2,3-triene"),
            ("ClC(Cl)=C=C=CCl", "trichlorobutatriene"),
            ("ClC(Cl)=C=C=C(Cl)Cl", "tetrachlorobutatriene"),
            (
                "ClC(Br)=C=C(F)I",
                "1-bromo-1-chloro-3-fluoro-3-iodopropa-1,2-diene",
            ),
            ("ClC(Cl)=C=C(Cl)Cl", "1,1,3,3-tetrachloropropa-1,2-diene"),
            ("C/C=C/OC=C=C=C", "{[(1E)-prop-1-en-1-yl]oxy}butatriene"),
            (
                "C/C=C/OC=C=C",
                "1-{[(1E)-prop-1-en-1-yl]oxy}propa-1,2-diene",
            ),
            ("CC(C)=C", "2-methylprop-1-ene"),
            ("CC(F)=C", "2-fluoroprop-1-ene"),
            ("ClC1=C(Cl)C1Cl", "1,2,3-trichlorocycloprop-1-ene"),
        ] {
            let name = super::super::name_smiles(smiles).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            assert_eq!(
                name.status,
                crate::naming::NameStatus::Preferred,
                "{smiles}"
            );
            let parsed = parser.parse(&name.text, &opsin::ParseOptions::strict());
            assert_eq!(
                parsed.status,
                opsin::Status::Success,
                "{expected}: {}",
                parsed.message
            );
            assert!(
                parsed.warnings.is_empty(),
                "{expected}: {:?}",
                parsed.warnings
            );
            let actual = parsed.structure.unwrap().semantic_cxsmiles().unwrap();
            crate::naming::verify_identity(smiles, &actual).unwrap();
        }
    }

    #[test]
    fn cumulene_locant_elision_controls() {
        for (smiles, expected) in [
            ("C=C=C=C", "butatriene"),
            ("C=C=C=C=C", "pentatetraene"),
            ("C=C=C=C=C=C", "hexapentaene"),
            ("FC=CCl", "1-chloro-2-fluoroethene"),
            ("ClC=C", "chloroethene"),
            ("ClC#CCl", "dichloroethyne"),
            ("FC(F)=C(F)F", "tetrafluoroethene"),
            ("CC=C", "propene"),
            ("CC#C", "propyne"),
            ("ClCC#C", "3-chloroprop-1-yne"),
            ("ClC#CC", "1-chloroprop-1-yne"),
            ("OC=C=C", "propa-1,2-dien-1-ol"),
            ("OC(=O)C=C=CCl", "4-chlorobuta-2,3-dienoic acid"),
            ("C=C=CC", "buta-1,2-diene"),
            ("ClC=C=CC", "1-chlorobuta-1,2-diene"),
            ("ClC=C=C=CC", "1-chloropenta-1,2,3-triene"),
            ("CC(C)=C=C=C(C)C", "2,5-dimethylhexa-2,3,4-triene"),
            ("C/C=C/C=C=C", "(4E)-hexa-1,2,4-triene"),
            ("C/C=C/C=C=C=C", "(5E)-hepta-1,2,3,5-tetraene"),
            ("C=C=C=CC#C", "hexa-1,2,3-trien-5-yne"),
            ("ClC(=C=C=C)C=C=C", "4-chlorohepta-1,2,3,5,6-pentaene"),
            ("C[C@H](Cl)C=C=C", "(4S)-4-chloropenta-1,2-diene"),
            ("[2H]C=C=C=C", "(1-2H1)buta-1,2,3-triene"),
            ("C=C=CC1CCCCC1", "(propa-1,2-dien-1-yl)cyclohexane"),
            ("C=C=C=CC1CCCCC1", "(buta-1,2,3-trien-1-yl)cyclohexane"),
            ("BrC1C=CCCC1", "3-bromocyclohex-1-ene"),
            ("CC1=CCCCC1", "1-methylcyclohex-1-ene"),
            ("C1=CCCCC1", "cyclohexene"),
        ] {
            let name = super::super::name_smiles(smiles).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            if smiles == "[2H]C=C=C=C" {
                assert!(matches!(
                    name.status,
                    crate::naming::NameStatus::Unverified(_)
                ));
            } else {
                assert_eq!(
                    name.status,
                    crate::naming::NameStatus::Preferred,
                    "{smiles}"
                );
            }
        }
        let error = super::super::name_smiles("F/C=C=C=C/Cl").unwrap_err();
        assert!(error.contains("cumulene E/Z"), "{error}");
    }

    #[test]
    fn stereo_equivalence_uses_path_and_ring_automorphisms() {
        for (n, ring, candidates) in [(401, false, vec![1, 401]), (32, true, (1..=32).collect())] {
            let mut parts = parent(n, ring, false);
            parts.parent_atom_ids_by_locant = (1..=n).map(|i| (i.to_string(), i)).collect();
            parts.stereogenic_atom_ids = candidates.into_iter().collect();
            parts.stereo_features.push(("1".into(), "R".into()));
            assert!(single_stereo_locant_is_unambiguous(&parts));
            parts
                .substituents
                .push(super::super::assembly_parts::SubstituentItem::new(
                    "chloro",
                    vec!["1".into()],
                ));
            assert!(!single_stereo_locant_is_unambiguous(&parts));
        }
    }

    #[test]
    fn general_equivalence_search_respects_the_budget() {
        let mut parts = parent(6, true, false);
        parts
            .parent_bond_orders_by_locants
            .insert(("1".into(), "4".into()), 1);
        let g = graph(&parts, true).unwrap();
        assert!(g.automorphisms.is_empty());
        let actual = decorate(&g, &[]);
        let mut budget = 0;
        assert_eq!(isomorphic(&g, &actual, &actual, &mut budget), None);
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
