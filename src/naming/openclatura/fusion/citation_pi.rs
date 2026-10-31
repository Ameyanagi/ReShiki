//! Construction-order witnesses for an uncompletable citation pi assignment.
use super::super::{locants::retained_locant_sort_key as locant_key, molecule::AtomSet};
use super::model::*;
use std::collections::BTreeMap;
fn paired(a: &BondAssignment) -> AtomSet {
    a.orders
        .iter()
        .filter(|(_, o)| *o == 2)
        .flat_map(|(e, _)| [e.0, e.1])
        .collect()
}
pub fn citation_pi_dead_end(plan: &FusionParentPlan) -> Option<Edge> {
    let ast = &plan.ast;
    let state = &plan.derivative_state;
    if ast.component_occurrences.len() != 2
        || ast.parent_occurrences.len() != 1
        || !ast.multiplicative_groups.is_empty()
        || ast.joins.len() != 1
        || ast.joins[0].kind() != FusionJoinKind::Ortho
        || ast.joins[0].host_occurrence() != ast.parent_occurrences[0]
        || plan.indicated_hydrogens.is_empty()
        || !state.hydro_operations.is_empty()
        || !state.unsaturation_operations.is_empty()
        || state.external_pi_operations().next().is_some()
        || !state.added_hydrogen_operations().is_empty()
        || !state.intrinsic_hydro_operations().is_empty()
        || state.pi_redistribution.is_some()
        || !plan.charge_operations.is_empty()
        || !plan.lambda_descriptors.is_empty()
    {
        return None;
    }
    let graph = &plan.abstract_parent_graph;
    let symbols: BTreeMap<_, _> = graph
        .atoms
        .iter()
        .map(|a| (a.id, a.symbol.as_str()))
        .collect();
    if graph
        .atoms
        .iter()
        .any(|a| !matches!(a.symbol.as_str(), "C" | "N") || a.formal_charge != 0)
    {
        return None;
    }
    let locants = plan
        .numbering
        .string_input_locant_maps()
        .into_iter()
        .next()?;
    let indicated: AtomSet = locants
        .iter()
        .filter(|(_, l)| plan.indicated_hydrogens.contains(l))
        .map(|(&a, _)| a)
        .collect();
    if indicated.iter().any(|a| symbols[a] != "C") {
        return None;
    }
    let model = &plan.bond_model;
    let active: AtomSet = symbols
        .keys()
        .copied()
        .filter(|a| !indicated.contains(a))
        .collect();
    if !model.required_double_bonds.is_empty()
        || model
            .required_single_bonds
            .iter()
            .any(|e| !indicated.contains(&e.0) && !indicated.contains(&e.1))
        || model.allowed_kekule_assignments.is_empty()
        || model
            .allowed_kekule_assignments
            .iter()
            .any(|a| paired(a) != active)
    {
        return None;
    }
    forced_surviving_child_edge(plan, &active)
}
pub fn forced_surviving_child_edge(plan: &FusionParentPlan, active: &AtomSet) -> Option<Edge> {
    let ast = &plan.ast;
    let model = &plan.bond_model;
    let graph = &plan.abstract_parent_graph;
    let symbols: BTreeMap<_, _> = graph
        .atoms
        .iter()
        .map(|a| (a.id, a.symbol.as_str()))
        .collect();
    let mut ns: BTreeMap<_, AtomSet> = symbols.keys().map(|&a| (a, AtomSet::new())).collect();
    for b in &graph.bonds {
        ns.entry(b.atoms.0).or_default().insert(b.atoms.1);
        ns.entry(b.atoms.1).or_default().insert(b.atoms.0);
    }
    let locants = plan
        .numbering
        .string_input_locant_maps()
        .into_iter()
        .next()?;
    if active
        .iter()
        .any(|a| ns[a].intersection(active).count() < 2)
    {
        return None;
    }
    let first = active
        .iter()
        .copied()
        .filter(|a| ns[a].len() == 2)
        .min_by_key(|a| locant_key(&locants[a]))?;
    if symbols[&first] != "N" || plan.charge_operations.iter().any(|op| op.atom_id == first) {
        return None;
    }
    let shared: AtomSet = ast
        .joins
        .iter()
        .flat_map(|j| j.shared_input_atoms())
        .collect();
    let matches: BTreeMap<_, _> = ast
        .component_occurrences
        .iter()
        .map(|m| (m.occurrence_id, m))
        .collect();
    for join in &ast.joins {
        let child = matches[&join.attached_occurrence()];
        let atoms: AtomSet = child.local_to_input_atom.iter().map(|(_, a)| *a).collect();
        if !atoms.contains(&first)
            || shared.contains(&first)
            || !ns[&first].is_subset(&atoms)
            || ast
                .joins
                .iter()
                .any(|j| j.host_occurrence() == child.occurrence_id)
        {
            continue;
        }
        let retained: AtomSet = ns[&first].difference(&shared).copied().collect();
        if retained.len() != 1 || ns[&first].intersection(&join.shared_input_atoms()).count() != 1 {
            continue;
        }
        let edge = normalize_edge(first, *retained.first()?);
        if model.pi_eligible_edges.contains(&edge)
            && !model.allowed_kekule_assignments.is_empty()
            && model
                .allowed_kekule_assignments
                .iter()
                .all(|a| a.orders.iter().any(|(e, o)| *e == edge && *o == 1))
        {
            return Some(edge);
        }
    }
    None
}
