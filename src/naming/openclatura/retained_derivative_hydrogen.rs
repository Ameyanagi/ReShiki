//! Exact oxo reconstruction for retained parents with unresolved carbon H.
use super::{
    assembly_parts::HydroOperation,
    fusion::{
        exocyclic::is_neutral_external_pi_ligand, model::ParentBondModel,
        numbering::retained_template_parent_bond_model,
    },
    graph_kernel::normalize_edge as pair,
    locants::retained_locant_sort_key,
    molecule::{AtomSet, Molecule},
    retained_graph_model::RetainedGraphTemplate,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub struct RetainedOxoHydrogenProof {
    pub indicated_h: Vec<String>,
    pub oxo_atom_ids: AtomSet,
    pub model: ParentBondModel,
    pub added_hydrogen: HydroOperation,
}
/// Only reconstruct a retained tautomer when oxo substitution explains every delta.
pub fn prove_retained_oxo_carbon_hydrogen(
    mol: &Molecule,
    template: &RetainedGraphTemplate,
    locant_to_atom: &BTreeMap<String, usize>,
    declared_indicated_h: &[String],
) -> Result<Option<RetainedOxoHydrogenProof>, String> {
    if (!template.default_indicated_h.is_empty() && declared_indicated_h.is_empty())
        || template.indicated_hydrogen_count() == 0
    {
        return Ok(None);
    }
    if locant_to_atom.keys().collect::<BTreeSet<_>>()
        != template.locants.iter().collect::<BTreeSet<_>>()
    {
        return Ok(None);
    }
    let declared: BTreeSet<_> = declared_indicated_h.iter().collect();
    if declared.len() != declared_indicated_h.len()
        || !declared.is_subset(&template.locants.iter().collect())
    {
        return Ok(None);
    }
    let atoms: AtomSet = locant_to_atom.values().copied().collect();
    if atoms.len() != template.locants.len() || atoms.iter().any(|i| !mol.atoms.contains_key(i)) {
        return Ok(None);
    }
    let observed: BTreeMap<_, _> = mol
        .bonds
        .values()
        .filter(|b| atoms.contains(&b.u) && atoms.contains(&b.v))
        .map(|b| (pair(b.u, b.v), b.order))
        .collect();
    let edges: BTreeSet<_> = template
        .bonds
        .iter()
        .map(|b| pair(locant_to_atom[&b.locants.0], locant_to_atom[&b.locants.1]))
        .collect();
    if observed.keys().copied().collect::<BTreeSet<_>>() != edges
        || observed.values().any(|&n| n != 1 && n != 2)
    {
        return Ok(None);
    }
    let by_loc = template.atom_by_locant();
    let mut oxo_sites = AtomSet::new();
    for (locant, &idx) in locant_to_atom {
        let atom = &mol.atoms[&idx];
        let neighbors = mol.get_neighbors(idx);
        let valence: usize = neighbors
            .iter()
            .filter_map(|&n| mol.get_bond(idx, n))
            .map(|b| b.order)
            .sum();
        if atom.symbol != by_loc[locant].symbol
            || atom.charge != 0
            || atom
                .element()
                .is_none_or(|e| atom.total_h_count + valence != e.standard_valence)
        {
            return Ok(None);
        }
        for &neighbor in neighbors {
            if atoms.contains(&neighbor)
                || mol.get_bond(idx, neighbor).is_some_and(|b| b.order == 1)
            {
                continue;
            }
            if mol.atoms[&neighbor].symbol != "O"
                || !is_neutral_external_pi_ligand(mol, idx, neighbor)
                || !oxo_sites.insert(idx)
            {
                return Ok(None);
            }
        }
    }
    if oxo_sites.is_empty() {
        return Ok(None);
    }
    let indicated = if declared_indicated_h.is_empty() {
        let mut indicated: Vec<_> = locant_to_atom
            .iter()
            .filter(|(l, i)| {
                mol.atoms[i].is_carbon()
                    && !oxo_sites.contains(i)
                    && by_loc[*l].resolved_pi_capacity() > 0
                    && !by_loc[*l].fusion
                    && observed
                        .iter()
                        .filter(|(e, _)| e.0 == **i || e.1 == **i)
                        .all(|(_, o)| *o == 1)
            })
            .map(|(l, _)| l.clone())
            .collect();
        indicated.sort_by_key(|l| retained_locant_sort_key(l));
        indicated
    } else {
        declared_indicated_h.to_vec()
    };
    if indicated.len() != template.indicated_hydrogen_count() {
        return Ok(None);
    }
    let model =
        match retained_template_parent_bond_model(template, locant_to_atom, Some(&indicated)) {
            Ok(model) => model,
            Err(error)
                if error == super::fusion::numbering::RETAINED_PARENT_BOND_CAPACITY_ERROR =>
            {
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
    for assignment in &model.allowed_kekule_assignments {
        let mut consumed: BTreeMap<_, usize> = oxo_sites.iter().map(|&i| (i, 0)).collect();
        let mut added_sites = AtomSet::new();
        let mut consumed_bonds = AtomSet::new();
        let mut valid = true;
        for &(edge, expected) in &assignment.orders {
            let actual = observed[&edge];
            if actual == expected {
                continue;
            }
            let suffix: Vec<_> = [edge.0, edge.1]
                .into_iter()
                .filter(|i| oxo_sites.contains(i))
                .collect();
            if expected != 2 || actual != 1 || suffix.len() != 1 {
                valid = false;
                break;
            }
            *consumed.entry(suffix[0]).or_default() += 1;
            added_sites.extend(
                [edge.0, edge.1]
                    .into_iter()
                    .filter(|i| !oxo_sites.contains(i)),
            );
            if let Some(b) = mol.get_bond(edge.0, edge.1) {
                consumed_bonds.insert(b.idx);
            }
        }
        if valid && consumed.values().all(|&n| n == 1) {
            let mut added_locants: Vec<_> = locant_to_atom
                .iter()
                .filter(|(_, i)| added_sites.contains(i))
                .map(|(l, _)| l.clone())
                .collect();
            added_locants.sort_by_key(|l| retained_locant_sort_key(l));
            return Ok(Some(RetainedOxoHydrogenProof {
                indicated_h: indicated,
                oxo_atom_ids: oxo_sites,
                model,
                added_hydrogen: HydroOperation {
                    key: "added_hydrogen".into(),
                    locants: added_locants.clone(),
                    atom_ids: added_locants.iter().map(|l| locant_to_atom[l]).collect(),
                    bond_ids: consumed_bonds.into_iter().collect(),
                    operation_kind: "indicated_hydrogen".into(),
                },
            }));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::super::molecule::{Atom, Bond};
    use super::*;

    #[test]
    fn exact_oxo_proof_separates_intrinsic_and_added_hydrogen() {
        for (parent, oxo, nitrogen) in [
            ("1H-1,4-benzodiazepine", "5", "4"),
            ("1H-1,5-benzodiazepine", "4", "5"),
        ] {
            let template = super::super::retained_fused_templates::retained_graph_templates(false)
                .iter()
                .find(|t| t.name == parent)
                .unwrap();
            let locants: BTreeMap<_, _> = template
                .atoms
                .iter()
                .enumerate()
                .map(|(idx, atom)| (atom.locant.clone(), idx))
                .collect();
            let model =
                retained_template_parent_bond_model(template, &locants, Some(&["3".into()]))
                    .unwrap();
            let consumed = pair(locants[oxo], locants[nitrogen]);
            let assignment = model
                .allowed_kekule_assignments
                .iter()
                .find(|a| a.orders.contains(&(consumed, 2)))
                .unwrap();
            let mut mol = Molecule::new();
            for atom in &template.atoms {
                mol.add_atom(Atom::new(locants[&atom.locant], &atom.symbol))
                    .unwrap();
            }
            for &(edge, order) in &assignment.orders {
                let order = if edge == consumed { 1 } else { order };
                mol.add_bond(Bond::new(mol.bonds.len(), edge.0, edge.1, order))
                    .unwrap();
            }
            let oxygen = mol.atoms.len();
            mol.add_atom(Atom::new(oxygen, "O")).unwrap();
            mol.add_bond(Bond::new(mol.bonds.len(), locants[oxo], oxygen, 2))
                .unwrap();
            let loads: BTreeMap<_, usize> = mol
                .atoms
                .keys()
                .map(|&idx| {
                    (
                        idx,
                        mol.get_neighbors(idx)
                            .iter()
                            .filter_map(|&n| mol.get_bond(idx, n))
                            .map(|b| b.order)
                            .sum(),
                    )
                })
                .collect();
            for (&idx, atom) in &mut mol.atoms {
                atom.total_h_count = atom
                    .element()
                    .unwrap()
                    .standard_valence
                    .saturating_sub(loads[&idx]);
            }
            let proof = prove_retained_oxo_carbon_hydrogen(&mol, template, &locants, &[])
                .unwrap()
                .unwrap();
            assert_eq!(proof.indicated_h, ["3"]);
            assert_eq!(proof.added_hydrogen.locants, [nitrogen]);
            assert_eq!(proof.added_hydrogen.atom_ids, [locants[nitrogen]]);
            assert_eq!(
                proof.model.maximum_non_cumulative_double_bonds,
                template.mancude_double_bonds.unwrap()
            );
            assert!(
                prove_retained_oxo_carbon_hydrogen(&mol, template, &locants, &["99".into()])
                    .unwrap()
                    .is_none()
            );
            let extra = mol
                .bonds
                .values()
                .find(|bond| bond.order == 2 && bond.u != oxygen && bond.v != oxygen)
                .unwrap()
                .clone();
            mol.set_bond_order(extra.idx, 1).unwrap();
            mol.atoms.get_mut(&extra.u).unwrap().total_h_count += 1;
            mol.atoms.get_mut(&extra.v).unwrap().total_h_count += 1;
            assert!(
                prove_retained_oxo_carbon_hydrogen(&mol, template, &locants, &[])
                    .unwrap()
                    .is_none()
            );
        }
    }
}
