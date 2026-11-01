//! Formal charge operations bound to completed fusion parent atoms.
use super::super::{
    locants::retained_locant_sort_key as locant_key,
    molecule::{AtomSet, Molecule},
};
use super::{
    mancude::{all_single, parent_degree},
    model::*,
};
fn require_deprotonation_valence(mol: &Molecule, id: usize) -> Result<(), String> {
    let atom = &mol.atoms[&id];
    let valence = atom.total_h_count
        + mol
            .get_neighbors(id)
            .iter()
            .filter_map(|&n| mol.get_bond(id, n))
            .map(|b| b.order)
            .sum::<usize>();
    if Some(valence)
        != match atom.symbol.as_str() {
            "C" => Some(3),
            "N" => Some(2),
            _ => None,
        }
    {
        return Err("fusion deprotonation requires a proton-removal C/N valence".into());
    }
    Ok(())
}
pub(super) fn fusion_component_charge_parent(
    mol: &Molecule,
    atoms: &AtomSet,
) -> Result<Molecule, String> {
    let mut parent = mol.clone();
    for &id in atoms {
        let atom = &mol.atoms[&id];
        if atom.charge != -1 || !matches!(atom.symbol.as_str(), "C" | "N") {
            continue;
        }
        require_deprotonation_valence(mol, id)?;
        let Some(restored) = parent.atoms.get_mut(&id) else {
            continue;
        };
        restored.charge = 0;
        restored.total_h_count += 1;
        restored.explicit_h_count += 1;
    }
    Ok(parent)
}
pub(super) fn fusion_charge_lone_pair_sites(mol: &Molecule, graph: &FusionGraph) -> AtomSet {
    if !graph.atoms.iter().any(|a| mol.atoms[&a.id].charge == -1) {
        return AtomSet::new();
    }
    graph
        .atoms
        .iter()
        .filter(|a| {
            let atom = &mol.atoms[&a.id];
            atom.symbol == "N"
                && matches!(atom.charge, -1 | 0)
                && atom.is_aromatic
                && (mol.degree(a.id) + atom.total_h_count) as i32 == 3 + atom.charge
                && all_single(mol, a.id)
        })
        .map(|a| a.id)
        .collect()
}
pub(super) fn protonated_pi_nitrogen_atoms(mol: &Molecule, graph: &FusionGraph) -> AtomSet {
    let atoms: AtomSet = graph.atoms.iter().map(|a| a.id).collect();
    graph
        .atoms
        .iter()
        .filter(|a| {
            let atom = &mol.atoms[&a.id];
            let ns = mol.get_neighbors(a.id);
            let mut orders: Vec<_> = ns
                .iter()
                .filter_map(|&n| mol.get_bond(a.id, n))
                .map(|b| b.order)
                .collect();
            orders.sort_unstable();
            a.symbol == "N"
                && a.formal_charge == 0
                && atom.charge == 1
                && atom.total_h_count == 1
                && ns.len() == 2
                && ns.iter().all(|n| atoms.contains(n))
                && orders == [1, 2]
        })
        .map(|a| a.id)
        .collect()
}

fn has_unproved_protonated_junction_donor(mol: &Molecule, atoms: &AtomSet) -> bool {
    if !mol
        .bonds
        .values()
        .any(|b| atoms.contains(&b.u) && atoms.contains(&b.v) && b.order == 2)
    {
        return false;
    }
    let mixed_donor = atoms.iter().any(|id| {
        let atom = &mol.atoms[id];
        atom.charge == 0
            && atom
                .element()
                .is_some_and(|element| element.mancude_forced_single)
    });
    atoms.iter().any(|&id| {
        let atom = &mol.atoms[&id];
        let neighbors = mol.get_neighbors(id);
        if atom.symbol != "N"
            || atom.charge != 1
            || atom.total_h_count == 0
            || neighbors.len() + atom.total_h_count != 4
            || !matches!(parent_degree(mol, atoms, id), 2 | 3)
            || neighbors
                .iter()
                .any(|&n| mol.get_bond(id, n).is_none_or(|b| b.order != 1))
        {
            return false;
        }
        if mixed_donor {
            return true;
        }
        neighbors
            .iter()
            .copied()
            .filter(|n| {
                atoms.contains(n)
                    && mol.atoms[n].symbol == "C"
                    && parent_degree(mol, atoms, *n) == 3
            })
            .any(|junction| {
                mol.get_neighbors(junction)
                    .iter()
                    .filter(|n| atoms.contains(n))
                    .all(|&n| mol.get_bond(junction, n).is_some_and(|b| b.order == 1))
            })
    })
}

pub(super) fn fusion_charge_operations(
    mol: &Molecule,
    graph: &FusionGraph,
    numbering: &FusionNumberingProof,
) -> Result<Vec<FusionChargeOperation>, String> {
    let atoms = graph.atoms.iter().map(|a| a.id).collect();
    // Upstream divergence: protonating a sigma donor adjacent to even one
    // carbon fusion junction is not jointly bound to the reduced junction's H.
    // Indicated N-H can consume a parent pi bond while its carbon endpoint's
    // hydrogen is lost, allowing reconstruction to move the ring reduction.
    // A second fixed-single skeletal donor creates the same competing carbon-H
    // role even when the protonated sigma N is farther from the fusion edge.
    if has_unproved_protonated_junction_donor(mol, &atoms) {
        return Err(
            "openclatura: protonated fusion-junction donor hydrogen state not ported".into(),
        );
    }
    let locants = numbering
        .string_input_locant_maps()
        .into_iter()
        .next()
        .unwrap_or_default();
    let mut operations = Vec::new();
    for base in &graph.atoms {
        let observed = &mol.atoms[&base.id];
        if observed.symbol != base.symbol {
            return Err("fusion charge operation cannot change the parent element".into());
        }
        if observed.charge == base.formal_charge {
            continue;
        }
        let locant = locants
            .get(&base.id)
            .ok_or_else(|| format!("charged atom {} has no completed-system locant", base.id))?;
        let operation_kind = if observed.charge < 0 {
            require_deprotonation_valence(mol, base.id)?;
            if !matches!(observed.symbol.as_str(), "C" | "N")
                || base.formal_charge != 0
                || observed.charge != -1
            {
                return Err("deprotonation requires a neutral C/N parent site becoming -1".into());
            }
            FusionChargeOperationKind::Deprotonation
        } else {
            if !matches!(observed.symbol.as_str(), "N" | "O")
                || base.formal_charge != 0
                || observed.charge != 1
            {
                return Err(
                    "heteroatom cationization requires a neutral N/O parent site becoming +1"
                        .into(),
                );
            }
            FusionChargeOperationKind::HeteroatomCationization
        };
        operations.push(FusionChargeOperation {
            atom_id: base.id,
            locant: locant.clone(),
            symbol: observed.symbol.clone(),
            base_charge: base.formal_charge,
            observed_charge: observed.charge,
            operation_kind,
        });
    }
    operations.sort_by_key(|op| locant_key(&op.locant));
    Ok(operations)
}

#[cfg(test)]
mod tests {
    use super::super::super::graph_io::read_smiles;
    use super::*;

    fn largest_ring_block(mol: &Molecule) -> AtomSet {
        super::super::super::graph_kernel::biconnected_edge_components(
            mol,
            &mol.atoms.keys().copied().collect(),
        )
        .into_iter()
        .filter_map(|edges| {
            let atoms: AtomSet = edges.iter().flat_map(|&(a, b)| [a, b]).collect();
            (edges.len() > atoms.len()).then_some(atoms)
        })
        .max_by_key(AtomSet::len)
        .unwrap()
    }

    #[test]
    fn protonated_donor_citation_cannot_hide_a_reduced_carbon_junction() {
        for smiles in [
            "C1=CC2CCCCC2[NH2+]1",
            "C1=CC2CCCCC2[NH+](C)1",
            "C[NH+]1C2C=CC=CC2C2=C1CCC=C2",
            "[NH2+]1C2C=CC=CC2C2=C1CCC=C2",
        ] {
            let mol = read_smiles(smiles).unwrap();
            let atoms = largest_ring_block(&mol);
            let graph = FusionGraph {
                atoms: atoms
                    .iter()
                    .map(|&id| FusionGraphAtom::new(id, &mol.atoms[&id].symbol))
                    .collect(),
                bonds: Vec::new(),
            };
            let numbering = FusionNumberingProof {
                input_locant_maps: vec![
                    atoms.iter().map(|&id| (id, (id + 1).to_string())).collect(),
                ],
                ..Default::default()
            };
            assert_eq!(
                fusion_charge_operations(&mol, &graph, &numbering).unwrap_err(),
                "openclatura: protonated fusion-junction donor hydrogen state not ported"
            );
        }
        for smiles in [
            "CN1C2C=CC=CC2C2=C1CCC=C2",
            "C[NH+]1C2CCCCC2C2C1CCCC2",
            "[NH2+]1C2CCCCC2C2C1CCCC2",
            "C[NH+]1CCC2=CC=CC=C2C1",
            "[NH2+]1CCC2=CC=CC=C2C1",
            "O=C1[CH-]NC2=C1C[NH+](C)C2",
        ] {
            let mol = read_smiles(smiles).unwrap();
            assert!(
                !has_unproved_protonated_junction_donor(&mol, &largest_ring_block(&mol)),
                "{smiles}"
            );
        }
    }

    #[test]
    fn mixed_donors_require_a_joint_protonation_pi_and_hydrogen_proof() {
        for smiles in [
            "C[NH+]1CCC2=C(C1)SC=C2",
            "[NH2+]1CCC2=C(C1)SC=C2",
            "C[NH+]1CCC2=C(C1)OC=C2",
        ] {
            let mol = read_smiles(smiles).unwrap();
            assert!(
                has_unproved_protonated_junction_donor(&mol, &largest_ring_block(&mol)),
                "{smiles}"
            );
        }
        for smiles in [
            "CN1CCC2=C(C1)SC=C2",
            "C[NH+]1CCC2C(C1)SCC2",
            "C1=C[NH+]=CC2=C1SC=C2",
            "C[NH+]1CCC2=CC=CC=C2C1",
        ] {
            let mol = read_smiles(smiles).unwrap();
            assert!(
                !has_unproved_protonated_junction_donor(&mol, &largest_ring_block(&mol)),
                "{smiles}"
            );
        }
    }

    #[test]
    fn deprotonation_restores_matching_view_and_preserves_charge_evidence() {
        let mol = read_smiles("c1ccc2[n-]ccc2c1").unwrap();
        let atoms = mol.atoms.keys().copied().collect();
        let parent = fusion_component_charge_parent(&mol, &atoms).unwrap();
        let nitrogen = *mol.atoms.iter().find(|(_, a)| a.symbol == "N").unwrap().0;
        assert_eq!(mol.atoms[&nitrogen].charge, -1);
        assert_eq!(parent.atoms[&nitrogen].charge, 0);
        assert_eq!(parent.atoms[&nitrogen].total_h_count, 1);
        let graph = FusionGraph {
            atoms: atoms
                .iter()
                .map(|&id| FusionGraphAtom::new(id, &parent.atoms[&id].symbol))
                .collect(),
            bonds: Vec::new(),
        };
        assert_eq!(
            fusion_charge_lone_pair_sites(&mol, &graph),
            AtomSet::from([nitrogen])
        );
        let numbering = FusionNumberingProof {
            input_locant_maps: vec![atoms.iter().map(|&id| (id, (id + 1).to_string())).collect()],
            ..Default::default()
        };
        let operations = fusion_charge_operations(&mol, &graph, &numbering).unwrap();
        assert_eq!(operations.len(), 1);
        assert_eq!(
            operations[0].operation_kind,
            FusionChargeOperationKind::Deprotonation
        );
        assert_eq!(operations[0].atom_id, nitrogen);
    }
}
