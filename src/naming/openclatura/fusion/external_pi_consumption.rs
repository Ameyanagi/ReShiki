//! Alternating carbon path witnesses for paired external pi consumption.
use super::super::molecule::{AtomSet, Molecule};
use super::{
    exocyclic::is_neutral_external_pi_ligand, mancude::ParentDerivativeState, model::normalize_edge,
};
use std::collections::BTreeMap;
pub fn has_paired_external_pi_consumption(
    mol: &Molecule,
    atoms: &AtomSet,
    state: &ParentDerivativeState,
) -> bool {
    if !state.bond_delta.compatible
        || !state.hydro_operations.is_empty()
        || !state.unsaturation_operations.is_empty()
        || !state.added_hydrogen_operations().is_empty()
        || !state.intrinsic_hydro_operations().is_empty()
    {
        return false;
    }
    let mut endpoints = AtomSet::new();
    for op in state.external_pi_operations() {
        let p = op.parent_atom_id;
        let e = op.external_atom_id;
        if !atoms.contains(&p)
            || atoms.contains(&e)
            || !mol.atoms.contains_key(&e)
            || endpoints.contains(&p)
            || mol.atoms[&p].symbol != "C"
            || mol
                .get_neighbors(p)
                .iter()
                .filter(|n| atoms.contains(n))
                .count()
                != 2
            || !mol
                .bonds
                .get(&op.bond_id)
                .is_some_and(|b| b.order == 2 && normalize_edge(b.u, b.v) == normalize_edge(p, e))
            || mol.atoms[&e].symbol != op.external_atom_symbol
            || !is_neutral_external_pi_ligand(mol, p, e)
        {
            return false;
        }
        endpoints.insert(p);
    }
    if endpoints.is_empty() || !endpoints.len().is_multiple_of(2) {
        return false;
    }
    let expected: BTreeMap<_, _> = state.bond_delta.assignment.orders.iter().copied().collect();
    let observed: BTreeMap<_, _> = mol
        .bonds
        .values()
        .filter(|b| atoms.contains(&b.u) && atoms.contains(&b.v))
        .map(|b| (normalize_edge(b.u, b.v), b.order))
        .collect();
    if expected.keys().ne(observed.keys())
        || expected
            .values()
            .chain(observed.values())
            .any(|o| !matches!(o, 1 | 2))
    {
        return false;
    }
    let mut changed: BTreeMap<usize, Vec<i32>> = BTreeMap::new();
    let mut parent_pi: BTreeMap<_, usize> = atoms.iter().map(|&a| (a, 0)).collect();
    let mut actual_pi = parent_pi.clone();
    for (&edge, &order) in &expected {
        for atom in [edge.0, edge.1] {
            *parent_pi.entry(atom).or_default() += order - 1;
            *actual_pi.entry(atom).or_default() += observed[&edge] - 1;
            if order != observed[&edge] {
                changed
                    .entry(atom)
                    .or_default()
                    .push(order as i32 - observed[&edge] as i32);
            }
        }
    }
    if !endpoints.iter().all(|a| changed.contains_key(a)) {
        return false;
    }
    for (&id, diffs) in &changed {
        let atom = &mol.atoms[&id];
        if atom.symbol != "C"
            || atom.charge != 0
            || atom.total_h_count
                + mol
                    .get_neighbors(id)
                    .iter()
                    .filter_map(|&n| mol.get_bond(id, n))
                    .map(|b| b.order)
                    .sum::<usize>()
                != 4
            || parent_pi[&id] != 1
        {
            return false;
        }
        if endpoints.contains(&id) {
            if diffs != &[1] || actual_pi[&id] != 0 {
                return false;
            }
        } else if !matches!(diffs.as_slice(), [-1, 1] | [1, -1]) || actual_pi[&id] != 1 {
            return false;
        }
    }
    true
}
