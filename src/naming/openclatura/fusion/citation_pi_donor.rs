//! Construction-order pi witnesses for charged parents with sigma donors.
use super::super::molecule::{AtomSet, Molecule};
use super::{citation_pi::forced_surviving_child_edge, model::*};
use std::collections::BTreeMap;
pub fn charged_donor_citation_pi_dead_end(mol: &Molecule, plan: &FusionParentPlan) -> Option<Edge> {
    let ast = &plan.ast;
    let state = &plan.derivative_state;
    let model = &plan.bond_model;
    if plan.charge_operations.is_empty()
        || !plan.indicated_hydrogens.is_empty()
        || !plan.lambda_descriptors.is_empty()
        || !state.hydro_operations.is_empty()
        || !state.unsaturation_operations.is_empty()
        || state.external_pi_operations().next().is_some()
        || !state.bond_delta.added_hydrogen_operations.is_empty()
        || !state.bond_delta.intrinsic_hydro_operations.is_empty()
        || state.pi_redistribution.is_some()
        || ast.parent_occurrences.len() != 1
        || !ast.multiplicative_groups.is_empty()
        || ast.joins.len() + 1 != ast.component_occurrences.len()
        || ast.joins.iter().any(|j| {
            !matches!(
                j.kind(),
                FusionJoinKind::Ortho | FusionJoinKind::HigherOrder
            ) || j.shared_input_atoms().len() != 2
        })
        || !model.required_double_bonds.is_empty()
        || model.allowed_kekule_assignments.is_empty()
    {
        return None;
    }
    let graph = &plan.abstract_parent_graph;
    let atoms: AtomSet = graph.atoms.iter().map(|a| a.id).collect();
    if graph
        .atoms
        .iter()
        .any(|a| !matches!(a.symbol.as_str(), "C" | "N") || a.formal_charge != 0)
    {
        return None;
    }
    let neighbors: BTreeMap<_, AtomSet> = atoms
        .iter()
        .map(|&a| {
            (
                a,
                mol.get_neighbors(a)
                    .iter()
                    .copied()
                    .filter(|n| atoms.contains(n))
                    .collect(),
            )
        })
        .collect();
    let active = model.allowed_kekule_assignments[0].paired_atoms();
    let donors: AtomSet = atoms.difference(&active).copied().collect();
    if donors.is_empty()
        || model
            .allowed_kekule_assignments
            .iter()
            .any(|a| a.paired_atoms() != active)
        || model
            .required_single_bonds
            .iter()
            .any(|e| !donors.contains(&e.0) && !donors.contains(&e.1))
    {
        return None;
    }
    let charged: AtomSet = plan.charge_operations.iter().map(|op| op.atom_id).collect();
    if charged
        != atoms
            .iter()
            .copied()
            .filter(|a| mol.atoms[a].charge != 0)
            .collect()
    {
        return None;
    }
    for &id in &atoms {
        let observed = &mol.atoms[&id];
        let external: AtomSet = mol
            .get_neighbors(id)
            .iter()
            .copied()
            .filter(|n| !atoms.contains(n))
            .collect();
        if external
            .iter()
            .any(|&n| mol.get_bond(id, n).is_none_or(|b| b.order != 1))
        {
            return None;
        }
        if donors.contains(&id) {
            if observed.symbol != "N"
                || observed.charge != 0
                || observed.total_h_count != 0
                || neighbors[&id].len() != 2
                || external.len() != 1
                || neighbors[&id]
                    .iter()
                    .any(|&n| mol.get_bond(id, n).is_none_or(|b| b.order != 1))
            {
                return None;
            }
        } else if charged.contains(&id) {
            if observed.symbol != "N"
                || observed.charge != 1
                || observed.total_h_count != 0
                || neighbors[&id].len() != 2
                || external.len() != 1
                || neighbors[&id]
                    .iter()
                    .filter_map(|&n| mol.get_bond(id, n))
                    .map(|b| b.order)
                    .sum::<usize>()
                    != 3
            {
                return None;
            }
            let oxide = &mol.atoms[external.first()?];
            if oxide.symbol != "O" || oxide.charge != -1 || mol.degree(oxide.idx) != 1 {
                return None;
            }
        } else if observed.symbol == "N"
            && (!external.is_empty() || observed.total_h_count != 0 || neighbors[&id].len() != 2)
        {
            return None;
        }
    }
    forced_surviving_child_edge(plan, &active)
}
