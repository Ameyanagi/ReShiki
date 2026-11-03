//! Cyclic phanes with disjoint monocyclic mancude amplificants (PhI-2/PhI-3).
use super::{
    graph_kernel::{adjacency_from_edges, connected_components, normalize_edge},
    molecule::{AtomSet, Molecule},
    rules::multipliers,
};
use std::collections::{BTreeMap, BTreeSet};

struct Amplificant {
    prefix: &'static str,
    // PhI-2.2.3: heterocycles precede benzene, with N > O > S.
    seniority: usize,
    numberings: Vec<BTreeMap<usize, usize>>,
}

/// P-52.2.5.3, P-51.3.2: a linear phane has at least four whole
/// ring systems and seven skeletal nodes along a ring-to-ring path.
pub(super) fn has_linear_skeleton(mol: &Molecule) -> bool {
    let systems = super::ring_assembly::cyclic_units(mol);
    let rings: AtomSet = systems.iter().filter_map(|s| s.first().copied()).collect();
    let mut collapsed: BTreeMap<_, _> = mol
        .atoms
        .values()
        .filter(|a| a.symbol != "H")
        .map(|a| (a.idx, a.idx))
        .collect();
    for system in &systems {
        if let Some(&root) = system.first() {
            for &atom in system {
                collapsed.insert(atom, root);
            }
        }
    }
    let nodes = collapsed.values().copied().collect();
    let edges = mol
        .bonds
        .values()
        .filter_map(|bond| {
            let (&u, &v) = (collapsed.get(&bond.u)?, collapsed.get(&bond.v)?);
            (u != v).then_some(normalize_edge(u, v))
        })
        .collect();
    let adjacency = adjacency_from_edges(&nodes, &edges);
    // Collapsing every cyclic component leaves a forest: avoiding the
    // preceding node visits exactly the simple paths from each terminal ring.
    rings.iter().any(|&start| {
        let mut pending = vec![(start, start, 1, 1)];
        while let Some((previous, current, length, ring_count)) = pending.pop() {
            if rings.contains(&current) && ring_count >= 4 && length >= 7 {
                return true;
            }
            for &next in &adjacency[&current] {
                if next != previous {
                    pending.push((
                        current,
                        next,
                        length + 1,
                        ring_count + usize::from(rings.contains(&next)),
                    ));
                }
            }
        }
        false
    })
}

/// Only one cycle after amplification is collapsed; every atom is accounted for.
pub(super) fn name(mol: &Molecule) -> Option<String> {
    if mol
        .atoms
        .values()
        .any(|a| a.charge != 0 || a.isotope.is_some() || a.has_stereo() || a.radical_electrons != 0)
        || mol.bonds.values().any(|b| b.stereo.is_some())
    {
        return None;
    }
    let aromatic: AtomSet = mol
        .atoms
        .values()
        .filter(|a| a.is_aromatic)
        .map(|a| a.idx)
        .collect();
    let aromatic_edges = mol
        .bonds
        .values()
        .filter(|b| b.is_aromatic)
        .map(|b| normalize_edge(b.u, b.v))
        .collect();
    let adjacency = adjacency_from_edges(&aromatic, &aromatic_edges);
    let mut amplificants = BTreeMap::new();
    let mut collapsed: BTreeMap<_, _> = mol.atoms.keys().map(|&i| (i, i)).collect();
    for atoms in connected_components(&aromatic, &aromatic_edges) {
        if !matches!(atoms.len(), 5 | 6) || atoms.iter().any(|i| adjacency[i].len() != 2) {
            return None;
        }
        let start = *atoms.first()?;
        let mut ring = vec![start];
        let mut previous = start;
        let mut current = *adjacency[&start].first()?;
        while current != start {
            ring.push(current);
            let next = *adjacency[&current].iter().find(|&&n| n != previous)?;
            previous = current;
            current = next;
        }
        let hetero: Vec<_> = ring
            .iter()
            .filter(|&&i| !mol.atoms[&i].is_carbon())
            .collect();
        let (prefix, seniority) = match (ring.len(), hetero.as_slice()) {
            (6, []) => ("benzena", 3),
            (6, [i]) if mol.atoms[i].symbol == "N" => ("pyridina", 0),
            (5, [i]) if mol.atoms[i].symbol == "O" => ("furana", 1),
            (5, [i]) if mol.atoms[i].symbol == "S" => ("thiophena", 2),
            _ => return None,
        };
        if ring.iter().any(|i| {
            let a = &mol.atoms[i];
            if a.is_carbon() {
                mol.degree(*i) + a.total_h_count != 3
            } else {
                mol.degree(*i) != 2 || a.total_h_count != 0
            }
        }) {
            return None;
        }
        let mut numberings = vec![];
        for offset in 0..ring.len() {
            if !hetero.is_empty() && ring[offset] != *hetero[0] {
                continue;
            }
            for reverse in [false, true] {
                numberings.push(
                    (0..ring.len())
                        .map(|i| {
                            (
                                ring[(offset + if reverse { ring.len() - i } else { i })
                                    % ring.len()],
                                i + 1,
                            )
                        })
                        .collect(),
                );
            }
        }
        for i in atoms {
            collapsed.insert(i, start);
        }
        amplificants.insert(
            start,
            Amplificant {
                prefix,
                seniority,
                numberings,
            },
        );
    }
    if amplificants.len() < 2 {
        return None;
    }
    let nodes: AtomSet = collapsed.values().copied().collect();
    let mut edges = BTreeSet::new();
    // For each skeleton edge, retain the real atom at either end for attachments.
    let mut attachments = BTreeMap::new();
    for b in mol.bonds.values() {
        let (u, v) = (collapsed[&b.u], collapsed[&b.v]);
        if u == v {
            if !b.is_aromatic {
                return None;
            }
            continue;
        }
        if b.order != 1 || b.is_aromatic || !edges.insert(normalize_edge(u, v)) {
            return None;
        }
        attachments.insert((u, v), b.u);
        attachments.insert((v, u), b.v);
    }
    let adjacency = adjacency_from_edges(&nodes, &edges);
    // P-52.2.5.1: a cyclophane has at least six nodes.
    if nodes.len() < 6
        || adjacency.values().any(|neighbors| neighbors.len() != 2)
        || connected_components(&nodes, &edges).len() != 1
    {
        return None;
    }
    for &i in &nodes {
        if amplificants.contains_key(&i) {
            let ends: Vec<_> = adjacency[&i]
                .iter()
                .map(|&n| attachments[&(i, n)])
                .collect();
            // P-52.2.5.2.1: adjacent attachments make a fused ring system.
            if ends[0] == ends[1] || mol.get_neighbors(ends[0]).contains(&ends[1]) {
                return None;
            }
        } else {
            let a = &mol.atoms[&i];
            let hydrogens = match a.symbol.as_str() {
                "C" => 2,
                "O" | "S" => 0,
                "N" => 1,
                _ => return None,
            };
            if a.is_aromatic || a.total_h_count != hydrogens || mol.degree(i) != 2 {
                return None;
            }
        }
    }
    // Each numbering is compared numerically, never by the rendered name.
    let mut best = None;
    for &start in &nodes {
        for &second in &adjacency[&start] {
            let mut order = vec![start];
            let mut previous = start;
            let mut current = second;
            while current != start {
                order.push(current);
                let next = *adjacency[&current].iter().find(|&&n| n != previous)?;
                previous = current;
                current = next;
            }
            let locants: BTreeMap<_, _> =
                order.iter().enumerate().map(|(i, &n)| (n, i + 1)).collect();
            let superatoms: Vec<_> = order
                .iter()
                .filter(|n| amplificants.contains_key(n))
                .map(|n| locants[n])
                .collect();
            let hetero = |symbol: Option<&str>| -> Vec<usize> {
                order
                    .iter()
                    .filter(|n| {
                        !amplificants.contains_key(n)
                            && mol.atoms[n].symbol != "C"
                            && symbol.is_none_or(|s| mol.atoms[n].symbol == s)
                    })
                    .map(|n| locants[n])
                    .collect()
            };
            let seniority: Vec<Vec<usize>> = (0..4)
                .map(|rank| {
                    order
                        .iter()
                        .filter(|n| amplificants.get(n).is_some_and(|a| a.seniority == rank))
                        .map(|n| locants[n])
                        .collect()
                })
                .collect();
            let mut numbered = vec![];
            let mut attachment_sets = vec![];
            let mut attachment_order = vec![];
            for &node in &order {
                let Some(amplificant) = amplificants.get(&node) else {
                    continue;
                };
                let mut neighbors: Vec<_> = adjacency[&node].iter().copied().collect();
                neighbors.sort_by_key(|n| locants[n]);
                let ends = [neighbors[0], neighbors[1]].map(|n| attachments[&(node, n)]);
                let (set, pair) = amplificant
                    .numberings
                    .iter()
                    .map(|numbering| {
                        let pair = ends.map(|end| numbering[&end]);
                        let mut set = pair;
                        set.sort();
                        (set, pair)
                    })
                    .min()?;
                attachment_sets.push((amplificant.seniority, locants[&node], set));
                attachment_order.push(pair);
                numbered.push((amplificant.prefix, locants[&node], pair));
            }
            // PhI-3.2.2 compares attachment sets by amplificant seniority,
            // then by superatom locant within each cited amplification prefix.
            attachment_sets.sort_by_key(|&(rank, locant, _)| (rank, locant));
            let attachment_sets: Vec<_> =
                attachment_sets.into_iter().map(|(_, _, set)| set).collect();
            // PhI-4.1/4.5: heteroatoms are placed on the numbering of the
            // carbocyclic phane and decide only the choices left by PhI-3.
            let key = (
                superatoms,
                seniority,
                attachment_sets,
                hetero(None),
                hetero(Some("O")),
                hetero(Some("S")),
                hetero(Some("N")),
                attachment_order,
            );
            if best.as_ref().is_none_or(|(old, _)| key < *old) {
                let mut parts = vec![];
                for (symbol, prefix) in [("O", "oxa"), ("S", "thia"), ("N", "aza")] {
                    let positions = hetero(Some(symbol));
                    if !positions.is_empty() {
                        let multiplier = if positions.len() == 1 {
                            String::new()
                        } else {
                            multipliers::basic(positions.len())
                        };
                        parts.push(format!(
                            "{}-{multiplier}{prefix}",
                            positions
                                .iter()
                                .map(usize::to_string)
                                .collect::<Vec<_>>()
                                .join(",")
                        ));
                    }
                }
                for rank in 0..4 {
                    let Some(amplificant) = amplificants.values().find(|a| a.seniority == rank)
                    else {
                        continue;
                    };
                    let members: Vec<_> = numbered
                        .iter()
                        .filter(|(prefix, _, _)| *prefix == amplificant.prefix)
                        .collect();
                    let mut groups: Vec<([usize; 2], Vec<usize>)> = vec![];
                    for &&(_, locant, pair) in &members {
                        if let Some((_, positions)) = groups.iter_mut().find(|(p, _)| *p == pair) {
                            positions.push(locant);
                        } else {
                            groups.push((pair, vec![locant]));
                        }
                    }
                    let descriptor = groups
                        .iter()
                        .map(|(pair, positions)| {
                            format!(
                                "{}({},{})",
                                positions
                                    .iter()
                                    .map(usize::to_string)
                                    .collect::<Vec<_>>()
                                    .join(","),
                                pair[0],
                                pair[1]
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(",");
                    let multiplier = if members.len() == 1 {
                        String::new()
                    } else {
                        multipliers::basic(members.len())
                    };
                    parts.push(format!("{descriptor}-{multiplier}{}", amplificant.prefix));
                }
                let name = format!(
                    "{}cyclo{}phane",
                    parts.join("-"),
                    multipliers::numerical_term(nodes.len())
                );
                best = Some((key, name));
            }
        }
    }
    best.map(|(_, name)| name)
}

#[cfg(test)]
mod tests;
