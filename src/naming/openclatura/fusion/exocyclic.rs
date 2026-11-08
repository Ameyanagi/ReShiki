// SPDX-License-Identifier: MIT
// Origin: adapted
// Source: openclatura | src/openclatura/fusion/exocyclic.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Copyright (c) 2026 lamalab-org
// Changes: Openclatura exocyclic ligand checks extended locally for neutral phosphorus lambda5 oxides.

//! Graph-bound external pi ligands supported by parent composition.
use super::super::molecule::Molecule;

pub fn neutral_lambda_oxo_bonding_number(mol: &Molecule, parent: usize) -> Option<usize> {
    let atom = &mol.atoms[&parent];
    let policy = atom.element()?;
    let phosphorus = atom.symbol == "P";
    if !(phosphorus || matches!(atom.symbol.as_str(), "S" | "Se"))
        || policy.standard_valence != if phosphorus { 3 } else { 2 }
        || (!phosphorus && !policy.mancude_forced_single)
        || policy.mancude_bonding_limit.is_some()
        || atom.charge != 0
        || (!phosphorus && atom.total_h_count != 0)
        || atom.is_aromatic
    {
        return None;
    }
    let neighbors = mol.get_neighbors(parent);
    let mut single = 0;
    let mut oxo = 0;
    for &other in neighbors {
        let bond = mol.get_bond(parent, other)?;
        if bond.order == 1 {
            let ligand = &mol.atoms[&other];
            if ligand.symbol == "N" && ligand.charge < 0 {
                return None;
            }
            if ligand.symbol == "O" {
                let side: Vec<_> = mol
                    .get_neighbors(other)
                    .iter()
                    .copied()
                    .filter(|&n| n != parent)
                    .collect();
                if side.len() != 1
                    || !neighbors.contains(&side[0])
                    || matches!(mol.atoms[&side[0]].symbol.as_str(), "O" | "C")
                {
                    return None;
                }
            }
            single += 1;
            continue;
        }
        let ligand = &mol.atoms[&other];
        if bond.order != 2
            || ligand.symbol != "O"
            || ligand.charge != 0
            || ligand.total_h_count != 0
            || mol.degree(other) != 1
        {
            return None;
        }
        oxo += 1;
    }
    // P-74.2.1.4: neutral phosphine oxides are lambda5 heterones,
    // including a P-H bond. Keep the single-O ligand exclusions above:
    // phosphinic/phosphonic acids and P-OH rings have their own naming path.
    (single + atom.total_h_count == policy.standard_valence
        && if phosphorus {
            oxo == 1
        } else {
            matches!(oxo, 1 | 2)
        }
        && single + oxo == neighbors.len())
    .then_some(policy.standard_valence + 2 * oxo)
}
pub fn is_neutral_external_pi_ligand(mol: &Molecule, parent: usize, ligand: usize) -> bool {
    let atom = &mol.atoms[&ligand];
    let center = &mol.atoms[&parent];
    (center.symbol == "C"
        || (atom.symbol == "O" && neutral_lambda_oxo_bonding_number(mol, parent).is_some()))
        && center.charge == 0
        && matches!(atom.symbol.as_str(), "O" | "N" | "C")
        && atom.charge == 0
        && mol.get_bond(parent, ligand).is_some_and(|b| b.order == 2)
        && atom.element().is_some_and(|e| {
            atom.total_h_count
                + mol
                    .get_neighbors(ligand)
                    .iter()
                    .map(|&n| mol.get_bond(ligand, n).map_or(0, |b| b.order))
                    .sum::<usize>()
                == e.standard_valence
        })
        && mol
            .get_neighbors(ligand)
            .iter()
            .all(|&n| n == parent || mol.get_bond(ligand, n).is_some_and(|b| b.order == 1))
}
