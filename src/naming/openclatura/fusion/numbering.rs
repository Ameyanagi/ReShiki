//! Maximum non-cumulative parent-bond assignments; retained numbering is supplied by templates.
use super::super::{
    molecule::{AtomSet, Molecule},
    retained_graph_model::RetainedGraphTemplate,
    rules::elements,
};
use super::model::*;
use std::collections::{BTreeMap, BTreeSet};

pub const MANCUDE_SEARCH_BUDGET: usize = 100_000;
pub const RETAINED_PARENT_BOND_CAPACITY_ERROR: &str =
    "retained template and derived parent bond model disagree on mancude capacity";
pub fn mancude_search_budget_exceeded(error: &str) -> bool {
    error.starts_with("mancude assignment search exceeded its budget of ")
}

// Preserve the source API's Molecule compatibility path alongside the graph API.
#[allow(dead_code)]
pub fn parent_bond_graph(mol: &Molecule, atoms: &AtomSet) -> FusionGraph {
    FusionGraph {
        atoms: atoms
            .iter()
            .map(|&id| {
                let atom = &mol.atoms[&id];
                let mut site = FusionGraphAtom::new(id, &atom.symbol);
                site.formal_charge = atom.charge;
                site.forced_single = atom.element().is_some_and(|e| e.mancude_forced_single);
                site
            })
            .collect(),
        bonds: mol
            .bonds
            .values()
            .filter(|b| atoms.contains(&b.u) && atoms.contains(&b.v))
            .map(|b| FusionGraphBond {
                atoms: normalize_edge(b.u, b.v),
                bond_class: "mancude".into(),
            })
            .collect(),
    }
}
fn parent_bond_loads(
    graph: &FusionGraph,
    assignment: &BondAssignment,
) -> Result<BTreeMap<usize, usize>, String> {
    let expected: EdgeSet = graph
        .bonds
        .iter()
        .map(|b| normalize_edge(b.atoms.0, b.atoms.1))
        .collect();
    let supplied: EdgeSet = assignment
        .orders
        .iter()
        .map(|&(e, _)| normalize_edge(e.0, e.1))
        .collect();
    if supplied != expected || supplied.len() != assignment.orders.len() {
        return Err("parent valence assignment must cover every parent bond exactly once".into());
    }
    let mut loads: BTreeMap<_, usize> = graph.atoms.iter().map(|a| (a.id, 0)).collect();
    for &(edge, order) in &assignment.orders {
        if !matches!(order, 1 | 2) {
            return Err("parent valence assignment requires single or double bond orders".into());
        }
        *loads.entry(edge.0).or_default() += order;
        *loads.entry(edge.1).or_default() += order;
    }
    Ok(loads)
}
pub fn validate_parent_bond_valence(
    graph: &FusionGraph,
    assignment: &BondAssignment,
) -> Result<(), String> {
    let loads = parent_bond_loads(graph, assignment)?;
    for site in &graph.atoms {
        let policy = elements::get(&site.symbol)
            .ok_or_else(|| format!("Unknown element symbol: {}", site.symbol))?;
        if let Some(limit) = policy.mancude_limit_for_charge(site.formal_charge)?
            && loads[&site.id] > limit
        {
            return Err(format!(
                "parent atom {} ({}, charge {:+}) has bond-order load {} above fixed-valence limit {}",
                site.id, site.symbol, site.formal_charge, loads[&site.id], limit
            ));
        }
    }
    Ok(())
}
pub fn parent_pi_capable_atom_ids(graph: &FusionGraph) -> Result<AtomSet, String> {
    let mut orders: Vec<_> = graph
        .bonds
        .iter()
        .map(|b| {
            (
                normalize_edge(b.atoms.0, b.atoms.1),
                if b.bond_class == "double" { 2 } else { 1 },
            )
        })
        .collect();
    orders.sort();
    let baseline = BondAssignment { orders };
    validate_parent_bond_valence(graph, &baseline)?;
    let loads = parent_bond_loads(graph, &baseline)?;
    let mut capable = AtomSet::new();
    for site in &graph.atoms {
        let policy = elements::get(&site.symbol)
            .ok_or_else(|| format!("Unknown element symbol: {}", site.symbol))?;
        if site.pi_capacity > 0
            && !site.forced_single
            && policy
                .mancude_limit_for_charge(site.formal_charge)?
                .is_none_or(|limit| loads[&site.id] < limit)
        {
            capable.insert(site.id);
        }
    }
    Ok(capable)
}
pub fn parent_bond_model(graph: &FusionGraph) -> Result<ParentBondModel, String> {
    parent_bond_model_with_budget(graph, MANCUDE_SEARCH_BUDGET)
}
pub fn parent_bond_model_with_budget(
    graph: &FusionGraph,
    search_budget: usize,
) -> Result<ParentBondModel, String> {
    let classes: BTreeMap<_, _> = graph
        .bonds
        .iter()
        .map(|b| (normalize_edge(b.atoms.0, b.atoms.1), b.bond_class.as_str()))
        .collect();
    let required_double: EdgeSet = classes
        .iter()
        .filter(|(_, c)| **c == "double")
        .map(|(&e, _)| e)
        .collect();
    let occupied: AtomSet = required_double.iter().flat_map(|&(u, v)| [u, v]).collect();
    if occupied.len() != required_double.len() * 2 {
        return Err("required-double bonds must be non-cumulative".into());
    }
    let capable = parent_pi_capable_atom_ids(graph)?;
    let eligible: EdgeSet = classes
        .iter()
        .filter(|(e, c)| {
            matches!(**c, "aromatic" | "mancude" | "fusion")
                && !occupied.contains(&e.0)
                && !occupied.contains(&e.1)
                && capable.contains(&e.0)
                && capable.contains(&e.1)
        })
        .map(|(&e, _)| e)
        .collect();
    let required = classes
        .keys()
        .filter(|e| !eligible.contains(e) && !required_double.contains(e))
        .copied()
        .collect();
    let matchings = maximum_matchings(&eligible, search_budget)?;
    Ok(ParentBondModel {
        maximum_non_cumulative_double_bonds: required_double.len()
            + matchings.iter().map(BTreeSet::len).max().unwrap_or(0),
        allowed_kekule_assignments: matchings
            .iter()
            .map(|m| BondAssignment {
                orders: classes
                    .keys()
                    .map(|&e| {
                        (
                            e,
                            if m.contains(&e) || required_double.contains(&e) {
                                2
                            } else {
                                1
                            },
                        )
                    })
                    .collect(),
            })
            .collect(),
        required_single_bonds: required,
        pi_eligible_edges: eligible,
        required_double_bonds: required_double,
    })
}
pub fn retained_template_parent_bond_model(
    template: &RetainedGraphTemplate,
    mapping: &BTreeMap<String, usize>,
    indicated_h: Option<&[String]>,
) -> Result<ParentBondModel, String> {
    retained_template_parent_bond_model_with_budget(
        template,
        mapping,
        indicated_h,
        MANCUDE_SEARCH_BUDGET,
    )
}
pub fn retained_template_parent_bond_model_with_budget(
    template: &RetainedGraphTemplate,
    mapping: &BTreeMap<String, usize>,
    indicated_h: Option<&[String]>,
    budget: usize,
) -> Result<ParentBondModel, String> {
    if mapping.keys().collect::<BTreeSet<_>>() != template.locants.iter().collect::<BTreeSet<_>>() {
        return Err("retained parent bond model requires a complete locant map".into());
    }
    let saturated_parent = template.mancude_double_bonds.is_none();
    let mut atoms = template.atoms.clone();
    if let Some(h) = indicated_h.filter(|h| *h != template.default_indicated_h.as_slice()) {
        if h.len() != template.indicated_hydrogen_count()
            || h.iter().any(|l| !mapping.contains_key(l))
        {
            return Err(
                "retained indicated H must preserve template count and locant domain".into(),
            );
        }
        let relocatable =
            super::super::retained_fused_templates::relocatable_atom_by_locant(template);
        // Template order is stable even when the locant lookup uses a sorted map.
        atoms = template
            .atoms
            .iter()
            .map(|a| {
                let mut a = relocatable[&a.locant].clone();
                if h.contains(&a.locant) {
                    a.saturated = true;
                    a.default_h = true;
                }
                a
            })
            .collect();
    }
    let mut degrees: BTreeMap<&str, usize> =
        template.locants.iter().map(|l| (l.as_str(), 0)).collect();
    for b in &template.bonds {
        *degrees.entry(&b.locants.0).or_default() += 1;
        *degrees.entry(&b.locants.1).or_default() += 1;
    }
    for a in &mut atoms {
        if a.symbol == "N" && a.charge == 0 && degrees[a.locant.as_str()] == 3 {
            a.forced_single = true;
        }
    }
    let graph = FusionGraph {
        atoms: atoms
            .iter()
            .map(|a| FusionGraphAtom {
                id: mapping[&a.locant],
                symbol: a.symbol.clone(),
                formal_charge: a.charge,
                pi_capacity: if saturated_parent {
                    0
                } else {
                    a.resolved_pi_capacity()
                },
                forced_single: saturated_parent || a.forced_single,
                indicated_h_site: a.indicated_h_site || a.default_h,
                saturated: a.saturated,
            })
            .collect(),
        bonds: template
            .bonds
            .iter()
            .map(|b| FusionGraphBond {
                atoms: normalize_edge(mapping[&b.locants.0], mapping[&b.locants.1]),
                bond_class: if saturated_parent {
                    "single".into()
                } else {
                    b.bond_class.clone()
                },
            })
            .collect(),
    };
    let model = parent_bond_model_with_budget(&graph, budget)?;
    if model.maximum_non_cumulative_double_bonds != template.mancude_double_bonds.unwrap_or(0) {
        return Err(RETAINED_PARENT_BOND_CAPACITY_ERROR.into());
    }
    Ok(model)
}
pub fn maximum_matchings(edges: &EdgeSet, budget: usize) -> Result<Vec<EdgeSet>, String> {
    let vertices: AtomSet = edges.iter().flat_map(|&(u, v)| [u, v]).collect();
    let neighbors: BTreeMap<_, Vec<_>> = vertices
        .iter()
        .map(|&v| {
            (
                v,
                edges
                    .iter()
                    .filter_map(|&(u, w)| {
                        if u == v {
                            Some(w)
                        } else if w == v {
                            Some(u)
                        } else {
                            None
                        }
                    })
                    .collect(),
            )
        })
        .collect();
    fn search(
        available: &AtomSet,
        neighbors: &BTreeMap<usize, Vec<usize>>,
        memo: &mut BTreeMap<AtomSet, Vec<EdgeSet>>,
        states: &mut usize,
        budget: usize,
    ) -> Result<Vec<EdgeSet>, String> {
        if let Some(result) = memo.get(available) {
            return Ok(result.clone());
        }
        *states += 1;
        if *states > budget {
            return Err(format!(
                "mancude assignment search exceeded its budget of {budget} states"
            ));
        }
        let Some(&atom) = available.first() else {
            return Ok(vec![EdgeSet::new()]);
        };
        let mut without = available.clone();
        without.remove(&atom);
        let mut candidates = search(&without, neighbors, memo, states, budget)?;
        for neighbor in &neighbors[&atom] {
            if !available.contains(neighbor) {
                continue;
            }
            let mut residual = without.clone();
            residual.remove(neighbor);
            for mut matching in search(&residual, neighbors, memo, states, budget)? {
                matching.insert(normalize_edge(atom, *neighbor));
                candidates.push(matching);
            }
        }
        let maximum = candidates.iter().map(BTreeSet::len).max().unwrap_or(0);
        let result: Vec<_> = candidates
            .into_iter()
            .filter(|m| m.len() == maximum)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        memo.insert(available.clone(), result.clone());
        Ok(result)
    }
    search(&vertices, &neighbors, &mut BTreeMap::new(), &mut 0, budget)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exhaustive_assignments_preserve_order_and_budget() {
        let edges = EdgeSet::from([(0, 1), (0, 3), (1, 2), (2, 3)]);
        assert_eq!(
            maximum_matchings(&edges, MANCUDE_SEARCH_BUDGET).unwrap(),
            vec![
                EdgeSet::from([(0, 1), (2, 3)]),
                EdgeSet::from([(0, 3), (1, 2)])
            ]
        );
        assert_eq!(
            maximum_matchings(&edges, 0).unwrap_err(),
            "mancude assignment search exceeded its budget of 0 states"
        );
        let graph = FusionGraph {
            atoms: (0..4).map(|id| FusionGraphAtom::new(id, "C")).collect(),
            bonds: edges
                .iter()
                .map(|&atoms| FusionGraphBond {
                    atoms,
                    bond_class: "mancude".into(),
                })
                .collect(),
        };
        let model = parent_bond_model(&graph).unwrap();
        assert_eq!(model.maximum_non_cumulative_double_bonds, 2);
        assert_eq!(model.allowed_kekule_assignments.len(), 2);
        assert!(parent_bond_model_with_budget(&graph, 1).is_err());
    }

    #[test]
    fn mandatory_bonds_and_fixed_valence_bound_optional_pi() {
        let mut graph = FusionGraph {
            atoms: (0..4)
                .map(|id| FusionGraphAtom::new(id, if id == 0 { "O" } else { "C" }))
                .collect(),
            bonds: vec![
                FusionGraphBond {
                    atoms: (0, 1),
                    bond_class: "mancude".into(),
                },
                FusionGraphBond {
                    atoms: (0, 3),
                    bond_class: "mancude".into(),
                },
                FusionGraphBond {
                    atoms: (1, 2),
                    bond_class: "double".into(),
                },
                FusionGraphBond {
                    atoms: (2, 3),
                    bond_class: "mancude".into(),
                },
            ],
        };
        let model = parent_bond_model(&graph).unwrap();
        assert_eq!(model.required_double_bonds, EdgeSet::from([(1, 2)]));
        assert!(model.pi_eligible_edges.is_empty());
        assert_eq!(model.maximum_non_cumulative_double_bonds, 1);
        graph.bonds[1].bond_class = "double".into();
        assert!(
            parent_bond_model(&graph)
                .unwrap_err()
                .contains("above fixed-valence limit 2")
        );
    }
}
