// SPDX-License-Identifier: MIT AND (MIT OR Apache-2.0)
// Origin: mixed
// Source: openclatura | src/openclatura/special_cases.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | src/openclatura/charge_pair_roles.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | src/openclatura/chalcogen_vocabulary.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: ReShiki | src/naming/openclatura/amine_oxides.rs | 5f559fd482e237307a589dfcb7ea333cee0a6e13
// Copyright (c) 2026 lamalab-org
// Copyright (c) 2026 Ameyanagi and ReShiki contributors
// Changes: Upstream nitrogen-chalcogenide neutral-parent algorithm with local locants and status preservation.
// Local contributions: MIT OR Apache-2.0; see NOTICE and PROVENANCE-nazuchem.toml.
// Unresolved: The commit that added this describes it as a local feature and contains no explicit port attribution; structural correspondence to upstream nitrogen_chalcogenide_result establishes conservative adaptation ancestry, not a verbatim translation claim.

//! P-62.5 / P-74.2.1.2: functional-class names of amine and imine chalcogenides.
use super::{
    Name,
    assembly_parts::AssemblyParts,
    component_namer::{Derivation, name_component_with_citation},
    molecule::{AtomSet, Molecule},
};
use crate::naming::{NameReason, NameStatus};

// Provenance: fn sites | adapted
// Source: openclatura | src/openclatura/special_cases.py::module scope (mapping uncertain)
// Source: openclatura | src/openclatura/charge_pair_roles.py::_classify_charge_pair
// Source: openclatura | src/openclatura/chalcogen_vocabulary.py::module scope (mapping uncertain)
fn sites(mol: &Molecule) -> Vec<(usize, usize)> {
    mol.atoms
        .iter()
        .filter(|(n, atom)| {
            atom.symbol == "N"
                && atom.charge == 1
                && !mol.get_neighbors(**n).iter().any(|&other| {
                    let neighbor = &mol.atoms[&other];
                    mol.get_bond(**n, other).is_some_and(|bond| {
                        (neighbor.symbol == "O" && bond.order == 2)
                            || (neighbor.is_carbon() && bond.order == 3)
                            || (neighbor.symbol == "N" && !bond.is_aromatic && bond.order > 1)
                    })
                })
        })
        .flat_map(|(&n, _)| {
            mol.get_neighbors(n).iter().filter_map(move |&ch| {
                let atom = &mol.atoms[&ch];
                (matches!(atom.symbol.as_str(), "O" | "S" | "Se" | "Te")
                    && atom.charge == -1
                    && atom.total_h_count == 0
                    && mol.degree(ch) == 1
                    && mol.bond_order(n, ch) == 1)
                    .then_some((n, ch))
            })
        })
        .collect()
}

// Provenance: fn name | adapted
// Source: openclatura | src/openclatura/special_cases.py::nitrogen_chalcogenide_result
// Source: openclatura | src/openclatura/charge_pair_roles.py::module scope (mapping uncertain)
// Source: openclatura | src/openclatura/chalcogen_vocabulary.py::module scope (mapping uncertain)
pub(super) fn name(mol: &Molecule, atoms: &AtomSet) -> Result<Option<Name>, String> {
    let sites = sites(mol);
    let [(n, ch)] = sites.as_slice() else {
        return Ok(None);
    };
    let (n, ch) = (*n, *ch);
    // N-bound H and N-N oxides need oxide-led parent selection beyond this adapter.
    if mol.atoms[&n].total_h_count != 0
        || mol.atoms[&n].has_stereo()
        || mol
            .get_neighbors(n)
            .iter()
            .any(|i| mol.atoms[i].symbol == "N")
        || mol
            .atoms
            .iter()
            .any(|(&i, a)| i != n && i != ch && a.charge != 0)
    {
        return Ok(None);
    }
    let reduced_atoms: AtomSet = atoms.iter().copied().filter(|&i| i != ch).collect();
    let mut reduced = mol.subgraph(&reduced_atoms);
    reduced.set_atom_charge(n, 0)?;
    // The subgraph retains aromaticity and source stereodescriptors at other atoms.
    // Even a successful neutral proof is discarded: the source route may already
    // have a certified fused oxide template, and both graphs share naming status.
    let mut result = None;
    let _attempt: Result<Option<()>, String> = mol.candidate(|| {
        let parent = name_component_with_citation(&reduced, &reduced_atoms, None, None)?;
        // Leave alternatives to the source route's complete naming pass.
        if reduced.has_relative_ring_alternative()
            || reduced.has_endo_exo_alternative()
            || reduced.has_higher_order_fusion_alternative()
            || reduced.has_benzo_fusion_alternative()
        {
            return Ok(None);
        }
        let Derivation::Plan {
            parent_atoms,
            parent_locants,
            principal_group,
            ..
        } = &parent.derivation
        else {
            return Ok(None);
        };
        let (locant, unambiguous) = match principal_group {
            None if parent_atoms.contains(&n) => {
                let Some(locant) = parent_locants.get(&n) else {
                    return Ok(None);
                };
                let unambiguous = locant == "1"
                    && parent_atoms
                        .iter()
                        .filter(|i| reduced.atoms[i].symbol == "N")
                        .count()
                        == 1;
                (locant.as_str(), unambiguous)
            }
            Some((key, group))
                if matches!(key.as_str(), "amine" | "imine")
                    && group.len() == 2
                    && group.contains(&n)
                    && !parent_atoms.contains(&n)
                    && group.iter().any(|&c| {
                        parent_atoms.contains(&c)
                            && reduced.atoms[&c].is_carbon()
                            && reduced.bond_order(c, n) == if key == "amine" { 1 } else { 2 }
                    }) =>
            {
                ("N", true)
            }
            _ => return Ok(None),
        };
        super::assess_derivation(&reduced, &parent.derivation);
        let status = match reduced.name_status() {
            NameStatus::Preferred if !unambiguous => {
                NameStatus::Unverified(NameReason::AmineChalcogenide)
            }
            status => status,
        };
        let class = super::nomenclature::section("chalcogenide_class_names")
            [&mol.atoms[&ch].symbol]
            .as_str()
            .ok_or("openclatura: missing chalcogenide class name")?;
        result = Some(Name {
            text: super::nest_enclosing_marks(&format!("{} {locant}-{class}", parent.text)),
            status,
            alternatives: vec![],
            omitted_stereo: vec![],
            omitted_units: 0,
            verification: None,
        });
        Ok(None)
    });
    Ok(result)
}

// Provenance: fn assess_zwitterion | original
// Source: ReShiki | src/naming/openclatura/amine_oxides.rs::assess_zwitterion
pub(super) fn assess_zwitterion(mol: &Molecule, parts: &AssemblyParts) {
    // P-74.2.1.2: assess the ionic treatment still cited after template consumption.
    if sites(mol).iter().any(|&(n, _)| {
        parts
            .parent_charges
            .iter()
            .any(|charge| charge.atom_id == Some(n))
            || parts
                .a_prefixes
                .iter()
                .any(|prefix| prefix.charge_atom_ids.contains(&n))
            || parts.principal_group.as_ref().is_some_and(|group| {
                matches!(group.key.as_str(), "aminium" | "iminium") && group.atom_ids.contains(&n)
            })
    }) {
        mol.mark_name(NameStatus::NotPreferred(NameReason::AmineOxideZwitterion));
    }
}

// Provenance: fn assess_cationic_prefix | original
// Source: ReShiki | src/naming/openclatura/amine_oxides.rs::assess_cationic_prefix
pub(super) fn assess_cationic_prefix(mol: &Molecule, nitrogen: usize) {
    if sites(mol).iter().any(|&(n, _)| n == nitrogen) {
        mol.mark_name(NameStatus::NotPreferred(NameReason::AmineOxideZwitterion));
    }
}
