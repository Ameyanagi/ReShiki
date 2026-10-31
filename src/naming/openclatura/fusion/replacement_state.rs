//! Joint pi and hydrogen proof after replacement of a carbon fusion parent.
use super::super::{
    locants::retained_locant_sort_key as locant_key,
    molecule::{AtomSet, Molecule},
};
use super::{
    mancude::{
        ParentDerivativeOptions, ParentDerivativeState, indicated_hydrogen_parent_bond_model,
        parent_derivative_state,
    },
    model::*,
    numbering::parent_bond_model,
};
use std::collections::BTreeMap;
#[derive(Clone, Debug)]
pub struct ReplacementHydrogenBalance {
    pub atom_id: usize,
    pub parent_hydrogens: i32,
    pub additive_hydrogens: i32,
    pub external_bond_order: i32,
    pub observed_hydrogens: i32,
}
impl ReplacementHydrogenBalance {
    pub fn exact(&self) -> bool {
        self.parent_hydrogens >= 0
            && self.additive_hydrogens >= 0
            && self.external_bond_order >= 0
            && self.observed_hydrogens >= 0
            && self.parent_hydrogens + self.additive_hydrogens - self.external_bond_order
                == self.observed_hydrogens
    }
}
#[derive(Clone, Debug)]
pub struct ReplacementFusionState {
    pub graph: FusionGraph,
    // Keep the proved replacement bond domain and citation as evidence for the
    // deferred renderer. Its applicability gate currently reads only audit_ok.
    #[allow(dead_code)]
    pub bond_model: ParentBondModel,
    pub derivative_state: ParentDerivativeState,
    #[allow(dead_code)]
    pub indicated_hydrogens: Vec<String>,
    pub hydrogen_balances: Vec<ReplacementHydrogenBalance>,
    #[allow(dead_code)]
    pub rendered_name: String,
    pub audit_checks: Vec<String>,
}
impl ReplacementFusionState {
    pub fn audit_ok(&self) -> bool {
        !self.audit_checks.is_empty()
            && self.derivative_state.bond_delta.compatible
            && self
                .hydrogen_balances
                .iter()
                .map(|b| b.atom_id)
                .collect::<AtomSet>()
                == self.graph.atoms.iter().map(|a| a.id).collect()
            && self
                .hydrogen_balances
                .iter()
                .all(ReplacementHydrogenBalance::exact)
    }
}
pub fn replacement_state_scope(mol: &Molecule, atoms: &AtomSet) -> bool {
    for &id in atoms {
        let a = &mol.atoms[&id];
        let ns = mol.get_neighbors(id);
        let orders: Vec<_> = ns
            .iter()
            .filter_map(|&n| mol.get_bond(id, n))
            .map(|b| b.order)
            .collect();
        let sum: usize = orders.iter().sum();
        let standard = a.element().map_or(0, |e| e.standard_valence);
        if a.charge != 0
            || a.is_aromatic
            || orders.iter().any(|o| !matches!(o, 1 | 2))
            || a.total_h_count as i32 != standard as i32 - sum as i32
        {
            return false;
        }
        if a.is_carbon() {
            continue;
        }
        if orders.iter().any(|o| *o != 1) {
            return false;
        }
        let degree = ns.iter().filter(|n| atoms.contains(n)).count();
        if !(a.element().is_some_and(|e| e.mancude_forced_single) && degree == 2
            || a.symbol == "N" && degree == 3 && ns.len() == 3)
        {
            return false;
        }
    }
    true
}
pub fn prove_replacement_state(
    mol: &Molecule,
    carbon_plan: &FusionParentPlan,
) -> Result<Option<ReplacementFusionState>, String> {
    let Some(locants) = carbon_plan
        .numbering
        .string_input_locant_maps()
        .into_iter()
        .next()
    else {
        return Ok(None);
    };
    let atoms: AtomSet = locants.keys().copied().collect();
    if !carbon_plan.audit.confirmed() || !replacement_state_scope(mol, &atoms) {
        return Ok(None);
    }
    let mut graph = carbon_plan.abstract_parent_graph.clone();
    for a in &mut graph.atoms {
        let observed = &mol.atoms[&a.id];
        a.symbol = observed.symbol.clone();
        if !observed.is_carbon() {
            a.pi_capacity = 0;
            a.forced_single = true;
        }
    }
    let initial = parent_bond_model(&graph)?;
    let mut candidates = Vec::new();
    for assignment in &initial.allowed_kekule_assignments {
        let paired: AtomSet = assignment
            .orders
            .iter()
            .filter(|(_, o)| *o == 2)
            .flat_map(|(e, _)| [e.0, e.1])
            .collect();
        let intrinsic: AtomSet = atoms
            .difference(&paired)
            .copied()
            .filter(|&a| {
                mol.atoms[&a].is_carbon()
                    && mol
                        .get_neighbors(a)
                        .iter()
                        .filter(|n| atoms.contains(n))
                        .count()
                        < 4
            })
            .collect();
        let model = indicated_hydrogen_parent_bond_model(&graph, &intrinsic)?;
        if model.maximum_non_cumulative_double_bonds != initial.maximum_non_cumulative_double_bonds
        {
            continue;
        }
        let Some(state) = parent_derivative_state(
            mol,
            &atoms,
            &model,
            &locants,
            &ParentDerivativeOptions {
                indicated_hydrogen_atom_ids: &intrinsic,
                preserve_retained_parent_state: true,
                compose_retained_external_pi: true,
                ..Default::default()
            },
        )?
        else {
            continue;
        };
        if !state.unsaturation_operations.is_empty() {
            continue;
        }
        let Some(balances) = hydrogen_balances(mol, &atoms, &model, &state) else {
            continue;
        };
        let mut indicated: Vec<_> = intrinsic.iter().map(|a| locants[a].clone()).collect();
        indicated.sort_by_key(|l| locant_key(l));
        let rank = (
            indicated.iter().map(|l| locant_key(l)).collect::<Vec<_>>(),
            state
                .hydro_operations
                .iter()
                .flat_map(|op| op.locants.iter().map(|l| locant_key(l)))
                .collect::<Vec<_>>(),
        );
        candidates.push((rank, model, state, indicated, balances));
    }
    let Some((_, bond_model, derivative_state, indicated_hydrogens, hydrogen_balances)) =
        candidates.into_iter().min_by(|a, b| a.0.cmp(&b.0))
    else {
        return Ok(None);
    };
    // The plain-name pipeline preserves the same token boundary through the
    // plan's typed indicated-hydrogen list, without carrying rich text tokens.
    let previous_h = carbon_plan
        .indicated_hydrogens
        .iter()
        .map(|l| format!("{l}H"))
        .collect::<Vec<_>>()
        .join(",");
    let core = if previous_h.is_empty() {
        carbon_plan.rendered_base_name.as_str()
    } else {
        let prefix = format!("{previous_h}-");
        carbon_plan
            .rendered_base_name
            .strip_prefix(&prefix)
            .ok_or_else(|| {
                "replacement parent indicated-hydrogen prefix disagrees with its plan".to_string()
            })?
    };
    let mut rendered_name = indicated_hydrogens
        .iter()
        .map(|l| format!("{l}H"))
        .collect::<Vec<_>>()
        .join(",");
    if !rendered_name.is_empty() {
        rendered_name.push('-');
    }
    rendered_name.push_str(core);
    Ok(Some(ReplacementFusionState {
        graph,
        bond_model,
        derivative_state,
        indicated_hydrogens,
        hydrogen_balances,
        rendered_name,
        audit_checks: [
            "postreplacement_pi_capacity",
            "joint_carbon_pi_hydrogen_state",
            "exact_restored_bonds",
            "exact_atom_hydrogen_balance",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
    }))
}
fn hydrogen_balances(
    mol: &Molecule,
    atoms: &AtomSet,
    model: &ParentBondModel,
    state: &ParentDerivativeState,
) -> Option<Vec<ReplacementHydrogenBalance>> {
    let delta = &state.bond_delta;
    let assignment: BTreeMap<_, _> = delta.assignment.orders.iter().copied().collect();
    let mut external_pi = AtomSet::new();
    for &a in atoms {
        for &n in mol.get_neighbors(a) {
            if !atoms.contains(&n) {
                let b = mol.get_bond(a, n)?;
                if b.order == 2 {
                    external_pi.insert(b.idx);
                }
            }
        }
    }
    if external_pi
        != state
            .external_pi_operations()
            .map(|op| op.bond_id)
            .collect()
    {
        return None;
    }
    let pi_atoms = model.pi_atoms();
    let paired: AtomSet = assignment
        .iter()
        .filter(|(_, o)| **o == 2)
        .flat_map(|(e, _)| [e.0, e.1])
        .collect();
    let consumed: AtomSet = state
        .external_pi_operations()
        .map(|op| op.parent_atom_id)
        .collect();
    let added: Vec<_> = state
        .added_hydrogen_operations()
        .iter()
        .flat_map(|op| op.atom_ids.iter().copied())
        .collect();
    let added_set: AtomSet = added.iter().copied().collect();
    if added.len() != added_set.len()
        || added_set
            != pi_atoms
                .difference(&paired)
                .copied()
                .filter(|a| !consumed.contains(a))
                .collect()
        || added.iter().any(|a| !mol.atoms[a].is_carbon())
    {
        return None;
    }
    let hydro: EdgeSet = delta.hydrogenated_edges.iter().copied().collect();
    let expected_doubles: EdgeSet = assignment
        .iter()
        .filter(|(_, o)| **o == 2)
        .map(|(&e, _)| e)
        .collect();
    let hydro_atoms = delta.hydrogenated_atom_ids();
    if !hydro.is_subset(&expected_doubles)
        || hydro_atoms.len() != 2 * hydro.len()
        || state
            .hydro_operations
            .iter()
            .flat_map(|op| op.atom_ids.iter().copied())
            .collect::<AtomSet>()
            != hydro_atoms
    {
        return None;
    }
    if assignment.iter().any(|(&e, &o)| {
        mol.get_bond(e.0, e.1)
            .is_none_or(|b| b.order != if hydro.contains(&e) { 1 } else { o })
    }) {
        return None;
    }
    let mut balances = Vec::new();
    for &id in atoms {
        let parent_valence: usize = assignment
            .iter()
            .filter(|(e, _)| e.0 == id || e.1 == id)
            .map(|(_, o)| *o)
            .sum();
        let external: usize = mol
            .get_neighbors(id)
            .iter()
            .filter(|n| !atoms.contains(n))
            .filter_map(|&n| mol.get_bond(id, n))
            .map(|b| b.order)
            .sum();
        let balance = ReplacementHydrogenBalance {
            atom_id: id,
            parent_hydrogens: mol.atoms[&id].element()?.standard_valence as i32
                - parent_valence as i32,
            additive_hydrogens: i32::from(hydro_atoms.contains(&id)),
            external_bond_order: external as i32,
            observed_hydrogens: mol.atoms[&id].total_h_count as i32,
        };
        if !balance.exact() {
            return None;
        }
        balances.push(balance);
    }
    Some(balances)
}
