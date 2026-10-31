//! Exact component maximum-matching witnesses for carbon hydrogen consumed by fusion.
use super::super::molecule::AtomSet;
use super::{
    indicated_hydrogen::{
        component_graph, component_h_locants, component_parent_atoms, component_parent_graph,
        intrinsic_carbon_fusion_scope,
    },
    model::*,
    numbering::parent_bond_model,
};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug)]
pub struct ComponentHydrogenConsumption {
    pub default_sites: BTreeSet<(usize, String)>,
    // Preserve the exact consumed junctions and the completed matching witness;
    // the current composition audit consumes only their default-site projection.
    #[allow(dead_code)]
    pub junction_sites: BTreeSet<(usize, usize)>,
    #[allow(dead_code)]
    pub parent_model: ParentBondModel,
}
fn paired(edges: &EdgeSet) -> AtomSet {
    edges.iter().flat_map(|&(a, b)| [a, b]).collect()
}
pub fn component_hydrogen_consumption(
    ast: &FusionNameAst,
    specs: &BTreeMap<usize, FusionComponentSpec>,
) -> Result<Option<ComponentHydrogenConsumption>, String> {
    if !specs.values().any(|s| {
        s.atoms().iter().any(|a| {
            a.symbol == "C" && a.saturated && s.template.default_indicated_h.contains(&a.locant)
        })
    }) || !intrinsic_carbon_fusion_scope(ast, specs)
    {
        return Ok(None);
    }
    if specs.values().any(|s| {
        s.bonds()
            .iter()
            .any(|b| !matches!(b.bond_class.as_str(), "aromatic" | "mancude" | "fusion"))
    }) {
        return Ok(None);
    }
    let graph = component_parent_graph(ast, specs, true)?;
    if graph.atoms.iter().any(|a| {
        a.formal_charge != 0
            || a.saturated
            || (a.symbol == "C" && (a.forced_single || a.pi_capacity != 1))
    }) {
        return Ok(None);
    }
    let mut model = parent_bond_model(&graph)?;
    if model.allowed_kekule_assignments.is_empty() {
        return Ok(None);
    }
    let pi_atoms = model.pi_atoms();
    let junctions: AtomSet = ast
        .joins
        .iter()
        .flat_map(|j| j.shared_input_atoms())
        .collect();
    let mut domains = Vec::new();
    let mut defaults = BTreeSet::new();
    for m in &ast.component_occurrences {
        let spec = &specs[&m.occurrence_id];
        let atoms = component_parent_atoms(spec)?;
        let local_model = parent_bond_model(&component_graph(spec, &atoms))?;
        if spec
            .template
            .mancude_double_bonds
            .is_some_and(|n| n != local_model.maximum_non_cumulative_double_bonds)
        {
            return Ok(None);
        }
        let mapping = m.input_atom_by_locant();
        let edges: EdgeSet = spec
            .bonds()
            .iter()
            .map(|b| normalize_edge(mapping[&b.locants.0], mapping[&b.locants.1]))
            .collect();
        let assignments: BTreeSet<EdgeSet> = local_model
            .allowed_kekule_assignments
            .iter()
            .map(|a| {
                a.orders
                    .iter()
                    .filter(|(_, o)| *o == 2)
                    .map(|(e, _)| {
                        normalize_edge(mapping[&atoms[e.0].locant], mapping[&atoms[e.1].locant])
                    })
                    .collect()
            })
            .collect();
        let local_pi: AtomSet = local_model
            .pi_atoms()
            .iter()
            .map(|&a| mapping[&atoms[a].locant])
            .collect();
        let carbon_h: AtomSet = component_h_locants(spec, "C")?
            .iter()
            .map(|l| mapping[l])
            .collect();
        for l in &spec.template.default_indicated_h {
            if !carbon_h.contains(&mapping[l]) {
                return Ok(None);
            }
            defaults.insert((m.occurrence_id, l.clone()));
        }
        domains.push((m.occurrence_id, edges, assignments, local_pi, carbon_h));
    }
    let mut consumed = BTreeSet::new();
    let mut witnesses = Vec::new();
    for assignment in &model.allowed_kekule_assignments {
        let doubles: EdgeSet = assignment
            .orders
            .iter()
            .filter(|(_, o)| *o == 2)
            .map(|(e, _)| *e)
            .collect();
        if paired(&doubles) != pi_atoms {
            return Ok(None);
        }
        let mut assignment_consumed = BTreeSet::new();
        let mut valid = true;
        for (occurrence, edges, assignments, local_pi, carbon_h) in &domains {
            let local: EdgeSet = doubles.intersection(edges).copied().collect();
            let local_paired = paired(&local);
            let local_witnesses: Vec<_> = assignments
                .iter()
                .filter(|candidate| {
                    local_paired
                        .symmetric_difference(&paired(candidate))
                        .all(|a| junctions.contains(a))
                })
                .collect();
            if local_witnesses.is_empty() {
                valid = false;
                break;
            }
            let unpaired: AtomSet = local_pi.difference(&local_paired).copied().collect();
            let allowed: AtomSet = carbon_h.intersection(&junctions).copied().collect();
            if !local_witnesses.iter().any(|candidate| {
                unpaired
                    .difference(&paired(candidate))
                    .all(|a| allowed.contains(a))
            }) {
                valid = false;
                break;
            }
            assignment_consumed.extend(unpaired.intersection(carbon_h).map(|&a| (*occurrence, a)));
        }
        if valid {
            witnesses.push(assignment.clone());
            consumed.extend(assignment_consumed);
        }
    }
    if defaults.is_empty() || consumed.is_empty() || witnesses.is_empty() {
        return Ok(None);
    }
    model.allowed_kekule_assignments = witnesses;
    Ok(Some(ComponentHydrogenConsumption {
        default_sites: defaults,
        junction_sites: consumed,
        parent_model: model,
    }))
}

#[cfg(test)]
mod tests {
    use super::super::super::retained_graph_model::{
        RetainedGraphAtomTemplate, RetainedGraphBondTemplate, RetainedGraphTemplate,
    };
    use super::*;

    fn fused_five_rings() -> (FusionNameAst, BTreeMap<usize, FusionComponentSpec>) {
        let mut specs = BTreeMap::new();
        let mut matches = Vec::new();
        for (occurrence, cycle) in [(0, [0, 1, 2, 3, 4]), (1, [3, 4, 5, 6, 7])] {
            let locants: Vec<_> = (1..=5).map(|a| a.to_string()).collect();
            let template = RetainedGraphTemplate {
                default_indicated_h: if occurrence == 0 {
                    vec!["4".into()]
                } else {
                    Vec::new()
                },
                mancude_double_bonds: Some(2),
                atoms: locants
                    .iter()
                    .map(|l| RetainedGraphAtomTemplate {
                        locant: l.clone(),
                        symbol: "C".into(),
                        saturated: occurrence == 0 && l == "4",
                        default_h: occurrence == 0 && l == "4",
                        ..Default::default()
                    })
                    .collect(),
                bonds: (0..5)
                    .map(|i| RetainedGraphBondTemplate {
                        locants: (locants[i].clone(), locants[(i + 1) % 5].clone()),
                        bond_class: "mancude".into(),
                    })
                    .collect(),
                locants,
                ..Default::default()
            };
            specs.insert(
                occurrence,
                FusionComponentSpec {
                    template,
                    ..Default::default()
                },
            );
            matches.push(FusionComponentMatch {
                occurrence_id: occurrence,
                local_to_input_atom: cycle
                    .into_iter()
                    .enumerate()
                    .map(|(i, a)| ((i + 1).to_string(), a))
                    .collect(),
                ..Default::default()
            });
        }
        let ast = FusionNameAst {
            component_occurrences: matches,
            joins: vec![FusionJoin {
                interface: OrderedFusionInterface {
                    attached_occurrence: 0,
                    host_occurrence: 1,
                    ordered_input_atoms: vec![3, 4],
                    ..Default::default()
                },
                ..Default::default()
            }],
            ..Default::default()
        };
        (ast, specs)
    }
    #[test]
    fn fusion_consumes_declared_component_h_only_with_complete_local_witnesses() {
        let (ast, mut specs) = fused_five_rings();
        let proof = component_hydrogen_consumption(&ast, &specs)
            .unwrap()
            .unwrap();
        assert_eq!(proof.default_sites, BTreeSet::from([(0, "4".into())]));
        assert_eq!(proof.parent_model.maximum_non_cumulative_double_bonds, 4);
        assert!(!proof.junction_sites.is_empty());
        assert!(proof.junction_sites.iter().all(|(_, a)| [3, 4].contains(a)));
        // Inherited fixed saturation at a shared atom cannot be waived by the
        // other component's movable carbon hydrogen convention.
        specs.get_mut(&1).unwrap().template.atoms[0].saturated = true;
        assert!(
            component_hydrogen_consumption(&ast, &specs)
                .unwrap()
                .is_none()
        );
    }
}
