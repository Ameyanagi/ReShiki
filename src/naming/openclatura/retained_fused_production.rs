// SPDX-License-Identifier: MIT AND (MIT OR Apache-2.0)
// Origin: mixed; upstream-derived code with original nazuchem/ReShiki extensions.
//   openclatura 0.4.3, src/openclatura/retained_fused_production.py @ 90dc5bcb4910c196122ec0405d06a32929a8f308.
//   ReShiki, src/naming/openclatura/retained_fused_production.rs @ 5f559fd482e237307a589dfcb7ea333cee0a6e13.
// Copyright (c) 2026 lamalab-org
// Copyright (c) 2026 Ameyanagi and ReShiki contributors
// Changes: Adapted openclatura retained-parent production/derivative gate with native suffix-H integration and local regression tests.
// Local contributions: MIT OR Apache-2.0; see NOTICE and PROVENANCE-nazuchem.toml.

//! Retained fused-parent production gate and derivative admission policies.
use super::{
    additive::retained_suffix_hydrogen_state,
    assembly_parts::{RetainedParentMetadata, SubstituentItem},
    fusion::{
        exocyclic::is_neutral_external_pi_ligand,
        mancude::suffix_hydrogen_state,
        numbering::{RETAINED_PARENT_BOND_CAPACITY_ERROR, retained_template_parent_bond_model},
    },
    locants::retained_locant_sort_key,
    molecule::{AtomSet, Molecule},
    perception::PerceivedGroup,
    retained_derivative_hydrogen::prove_retained_oxo_carbon_hydrogen,
    retained_fused_templates::{
        GRAPH_DATA, MatchOptions, is_saturated_site,
        match_retained_graph_template_maps_with_options,
        match_retained_graph_templates_with_options,
    },
    retained_graph_model::RetainedGraphTemplateMatch,
    retained_name_policy::retained_parent_name_policy,
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::LazyLock,
};
#[allow(
    clippy::expect_used,
    reason = "Immutable checked-in parser capability snapshot"
)]
static GATE: LazyLock<Value> = LazyLock::new(|| {
    let data: Value = serde_json::from_str(include_str!("data/parser_grammar_snapshot.json"))
        .expect("retained production gate");
    data["retained_fused_derivative_gate"].clone()
});
#[derive(Clone, Debug)]
pub struct ProductionRetainedFusedParent {
    pub name: String,
    pub locant_maps: Vec<BTreeMap<usize, String>>,
    pub metadata: RetainedParentMetadata,
}
#[allow(clippy::too_many_arguments)]
// Adapted from src/openclatura/retained_fused_production.py::production_retained_fused_parent (adapted; local fn production_retained_fused_parent)
pub fn production_retained_fused_parent(
    mol: &Molecule,
    parent_path: &[usize],
    component_atoms: &AtomSet,
    perceived_groups: &[PerceivedGroup],
    principal_key: Option<&str>,
    substituent_mapping: &BTreeMap<usize, Vec<SubstituentItem>>,
    attachment_atom: Option<usize>,
) -> Result<Option<ProductionRetainedFusedParent>, String> {
    let parent_atoms: AtomSet = parent_path.iter().copied().collect();
    if component_atoms.iter().any(|a| {
        let atom = &mol.atoms[a];
        atom.charge != 0 && (!parent_atoms.contains(a) || atom.symbol != "C" || atom.charge != -1)
    }) || parent_atoms.iter().any(|a| {
        mol.get_neighbors(*a).iter().any(|n| {
            parent_atoms.contains(n)
                && mol
                    .get_bond(*a, *n)
                    .is_none_or(|b| !matches!(b.order, 1 | 2))
        })
    }) {
        return Ok(None);
    }
    if !GATE["allowed_principal_keys"]
        .as_array()
        .is_some_and(|keys| keys.iter().any(|k| k.as_str() == principal_key))
    {
        return Ok(None);
    }
    let unlisted_groups: Vec<_> = perceived_groups
        .iter()
        .filter(|g| parent_atoms.contains(&g.attachment_carbon))
        .filter(|g| {
            !g.resolved_rule
                .as_ref()
                .is_some_and(|r| r.has_capability("promotes_member_groups"))
                && !listed("allowed_group_keys", &g.key)
        })
        .collect();
    let required_suffix_state = if unlisted_groups.is_empty() {
        None
    } else {
        if principal_key != Some("ketone")
            || unlisted_groups.iter().any(|g| {
                let ligands: Vec<_> = g
                    .atoms_involved
                    .iter()
                    .filter(|a| !parent_atoms.contains(a))
                    .collect();
                ligands.is_empty()
                    || ligands
                        .iter()
                        .any(|&&a| !is_neutral_external_pi_ligand(mol, g.attachment_carbon, a))
            })
        {
            return Ok(None);
        }
        Some(
            perceived_groups
                .iter()
                .filter(|g| g.key == "ketone" && parent_atoms.contains(&g.attachment_carbon))
                .map(|g| g.attachment_carbon)
                .collect::<AtomSet>(),
        )
    };
    if substituent_mapping
        .values()
        .flatten()
        .any(|item| item.spiro.is_some())
    {
        return Ok(None);
    }
    let mut features: AtomSet = substituent_mapping.keys().copied().collect();
    features.extend(attachment_atom);
    features.extend(
        perceived_groups
            .iter()
            .filter(|g| parent_atoms.contains(&g.attachment_carbon))
            .map(|g| g.attachment_carbon),
    );
    let strict = match_retained_graph_templates_with_options(
        mol,
        &parent_atoms,
        MatchOptions {
            family: Some("fused"),
            ..Default::default()
        },
    );
    let strict_present = !strict.is_empty();
    let mut matches = eligible(
        mol,
        &parent_atoms,
        strict,
        principal_key,
        required_suffix_state.as_ref(),
    )?;
    if strict_present && matches.is_empty() {
        return Ok(None);
    }
    if matches.is_empty() {
        let mut pooled = Vec::new();
        let mut seen = BTreeSet::new();
        for (nonaromatic, relocated) in [(true, false), (false, true), (true, true)] {
            let found = match_retained_graph_templates_with_options(
                mol,
                &parent_atoms,
                MatchOptions {
                    allow_nonaromatic: nonaromatic,
                    allow_relocated_indicated_h: relocated,
                    family: Some("fused"),
                    ..Default::default()
                },
            );
            for m in eligible(
                mol,
                &parent_atoms,
                found,
                principal_key,
                required_suffix_state.as_ref(),
            )? {
                if seen.insert((m.template.name.clone(), m.atom_to_locant.clone())) {
                    pooled.push(m);
                }
            }
        }
        if let Some(best) = pooled
            .iter()
            .find(|m| !m.indicated_h.is_empty())
            .or_else(|| pooled.first())
        {
            let name = best.template.name.clone();
            let indicated = best.indicated_h.clone();
            matches = pooled
                .into_iter()
                .filter(|m| m.template.name == name && m.indicated_h == indicated)
                .collect();
        }
    }
    let Some(first) = matches.first() else {
        return Ok(None);
    };
    if principal_key == Some("ketone")
        && first.template.indicated_hydrogen_count() > 0
        && parent_atoms.iter().all(|id| mol.atoms[id].charge == 0)
    {
        let suffix: AtomSet = perceived_groups
            .iter()
            .filter(|g| g.key == "ketone" && parent_atoms.contains(&g.attachment_carbon))
            .map(|g| g.attachment_carbon)
            .collect();
        // P-58.2: admit reduced sites with the parent H seed intact;
        // suffix composition below determines its permissible relocation.
        let alternatives = eligible(
            mol,
            &parent_atoms,
            match_retained_graph_template_maps_with_options(
                mol,
                &parent_atoms,
                &first.template,
                true,
                false,
            ),
            principal_key,
            required_suffix_state.as_ref(),
        )?;
        let keys = |sites: Vec<&String>| {
            let mut keys: Vec<_> = sites
                .into_iter()
                .map(|l| retained_locant_sort_key(l))
                .collect();
            keys.sort();
            keys
        };
        let mut candidates = Vec::new();
        for m in matches
            .iter()
            .chain(&alternatives)
            .filter(|m| m.template.name == first.template.name)
        {
            let Some((indicated, _, state)) = retained_suffix_hydrogen_state(
                mol,
                &m.template,
                &parent_atoms,
                &suffix,
                &m.atom_to_locant,
                &m.indicated_h,
            )?
            else {
                continue;
            };
            // P-59.1.10 / P-14.4: compare permissible P-58.2 assignments
            // by indicated H, principal groups, added H, then hydro prefixes.
            let rank = (
                keys(indicated.iter().collect()),
                keys(suffix.iter().map(|id| &m.atom_to_locant[id]).collect()),
                keys(
                    state
                        .bond_delta
                        .added_hydrogen_operations
                        .iter()
                        .flat_map(|op| &op.locants)
                        .collect(),
                ),
                keys(
                    state
                        .hydro_operations
                        .iter()
                        .flat_map(|op| &op.locants)
                        .collect(),
                ),
            );
            let mut m = m.clone();
            m.indicated_h = indicated;
            candidates.push((rank, m));
        }
        if let Some(best) = candidates.iter().map(|(rank, _)| rank).min().cloned() {
            matches = candidates
                .into_iter()
                .filter(|(rank, _)| *rank == best)
                .map(|(_, m)| m)
                .collect();
        }
    }
    let first = &matches[0];
    let parent_name = &first.template.name;
    let maps: Vec<_> = matches
        .iter()
        .filter(|m| {
            &m.template.name == parent_name
                && features.iter().all(|a| {
                    m.atom_to_locant
                        .get(a)
                        .is_some_and(|l| !l.chars().any(char::is_alphabetic))
                })
        })
        .map(|m| m.atom_to_locant.clone())
        .collect();
    if maps.is_empty() || !added_hydrogen_is_citable(mol, first) {
        return Ok(None);
    }
    let context =
        if attachment_atom.is_none() && substituent_mapping.is_empty() && principal_key.is_none() {
            "unsubstituted_parent"
        } else {
            "composite_parent"
        };
    let name = retained_parent_name_policy(parent_name)
        .map_or_else(|| parent_name.clone(), |p| p.output_name(context));
    let mut metadata = first.template.metadata();
    metadata.inherent_saturated_locants.clear();
    metadata.default_indicated_h = first.indicated_h.clone();
    metadata.relocated_indicated_h = first.indicated_h != first.template.default_indicated_h;
    Ok(Some(ProductionRetainedFusedParent {
        name,
        locant_maps: maps,
        metadata,
    }))
}
// Adapted from src/openclatura/retained_fused_production.py (adapted; upstream function mapping not recorded; local fn listed)
fn listed(key: &str, value: &str) -> bool {
    GATE[key]
        .as_array()
        .is_some_and(|rows| rows.iter().any(|v| v.as_str() == Some(value)))
}
// Adapted from src/openclatura/retained_fused_production.py::ProductionRetainedFusedParent.eligible (adapted; local fn eligible)
fn eligible(
    mol: &Molecule,
    parent_atoms: &AtomSet,
    mut matches: Vec<RetainedGraphTemplateMatch>,
    principal_key: Option<&str>,
    required_suffix_state: Option<&AtomSet>,
) -> Result<Vec<RetainedGraphTemplateMatch>, String> {
    if principal_key == Some("ketone") {
        for m in &mut matches {
            if let Some(indicated_h) =
                prove_retained_oxo_carbon_hydrogen(mol, &m.template, &m.locant_to_atom, &[])?
            {
                m.indicated_h = indicated_h;
            }
        }
    }
    matches.sort_by_key(|m| m.indicated_h.is_empty());
    let mut admitted = Vec::new();
    for m in matches {
        if !listed("production_parent_names", &m.template.name)
            || !m.template.derivative_production_enabled
            || (parent_atoms.iter().any(|a| mol.atoms[a].charge != 0)
                && m.template.charge_policy != "deprotonated_indicated_h")
            || (principal_key == Some("ketone")
                && !has_mancude_unsaturation(mol, parent_atoms, &m)
                && !saturation_is_hydro_citable(mol, &m))
            || !added_hydrogen_is_citable(mol, &m)
        {
            continue;
        }
        if let Some(suffix) = required_suffix_state {
            let model = match retained_template_parent_bond_model(
                &m.template,
                &m.locant_to_atom,
                Some(&m.indicated_h),
            ) {
                Ok(model) => model,
                Err(error) if error == RETAINED_PARENT_BOND_CAPACITY_ERROR => continue,
                Err(error) => return Err(error),
            };
            if suffix_hydrogen_state(mol, &m.matched_atoms, &model, suffix, &m.atom_to_locant)?
                .is_none()
            {
                continue;
            }
        }
        admitted.push(m);
    }
    Ok(admitted)
}
// Adapted from src/openclatura/retained_fused_production.py::_added_hydrogen_is_citable (adapted; local fn added_hydrogen_is_citable)
pub fn added_hydrogen_is_citable(mol: &Molecule, m: &RetainedGraphTemplateMatch) -> bool {
    let by = m.template.atom_by_locant();
    let mut positions = 0usize;
    let mut saturated = 0usize;
    for (&a, l) in &m.atom_to_locant {
        if !by[l].aromatic {
            continue;
        }
        positions += 1;
        if mol
            .get_neighbors(a)
            .iter()
            .any(|n| m.matched_atoms.contains(n))
            && is_saturated_site(mol, a, &m.matched_atoms)
        {
            saturated += 1;
        }
    }
    if saturated > 0 && saturated >= positions {
        return false;
    }
    let excluded = m.indicated_h.iter().filter(|l| !by[*l].aromatic).count();
    let surplus =
        saturated as isize - (m.template.indicated_hydrogen_count() as isize - excluded as isize);
    surplus <= 0 || surplus % 2 == 0 || indicated_h_retained_name(&m.template.name)
}
// Adapted from src/openclatura/retained_fused_production.py (adapted; upstream function mapping not recorded; local fn indicated_h_retained_name)
fn indicated_h_retained_name(name: &str) -> bool {
    // The source set combines explicit monocycle names with the declared
    // indicated_hydrogen_name column; no inference from an alias is needed.
    GRAPH_DATA
        .parents
        .iter()
        .any(|row| row.indicated_hydrogen_name.as_deref() == Some(name))
}
// Adapted from src/openclatura/retained_fused_production.py::_has_mancude_unsaturation (adapted; local fn has_mancude_unsaturation)
fn has_mancude_unsaturation(
    mol: &Molecule,
    atoms: &AtomSet,
    m: &RetainedGraphTemplateMatch,
) -> bool {
    let Some(expected) = m.template.mancude_double_bonds else {
        return false;
    };
    let carbonyls = m
        .template
        .atoms
        .iter()
        .filter(|a| {
            a.symbol == "C"
                && !a.aromatic
                && mol
                    .get_neighbors(m.locant_to_atom[&a.locant])
                    .iter()
                    .any(|n| {
                        !atoms.contains(n)
                            && mol.atoms[n].symbol == "O"
                            && mol
                                .get_bond(m.locant_to_atom[&a.locant], *n)
                                .is_some_and(|b| b.order == 2)
                    })
        })
        .count();
    mol.bonds
        .values()
        .filter(|b| b.order == 2 && (atoms.contains(&b.u) || atoms.contains(&b.v)))
        .count()
        >= expected + carbonyls
}
// Adapted from src/openclatura/retained_fused_production.py::_saturation_is_hydro_citable (adapted; local fn saturation_is_hydro_citable)
fn saturation_is_hydro_citable(mol: &Molecule, m: &RetainedGraphTemplateMatch) -> bool {
    if m.template.mancude_double_bonds.is_none() {
        return false;
    }
    let by = m.template.atom_by_locant();
    let saturated = m
        .atom_to_locant
        .iter()
        .filter(|(a, l)| {
            by[*l].aromatic
                && mol
                    .get_neighbors(**a)
                    .iter()
                    .any(|n| m.matched_atoms.contains(n))
                && is_saturated_site(mol, **a, &m.matched_atoms)
        })
        .count();
    saturated > 0 && saturated < m.atom_to_locant.len()
}

#[cfg(test)]
mod tests {
    use super::super::{
        chains::find_ring_systems, graph_io::read_smiles, perception::perceive_groups,
    };
    use super::*;

    // Original nazuchem code (not in openclatura 0.4.3): fn anthracene_and_phenanthrene_admit_retained_derivatives
    #[test]
    fn anthracene_and_phenanthrene_admit_retained_derivatives() {
        for (smiles, parent, principal) in [
            ("c1ccc2cc3ccccc3cc2c1", "anthracene", None),
            ("c1ccc2c(c1)ccc1ccccc12", "phenanthrene", None),
            ("C1c2ccccc2Cc2ccccc21", "anthracene", None),
            ("C1Cc2ccccc2-c2ccccc21", "phenanthrene", None),
            ("OC1c2ccccc2Cc2ccccc21", "anthracene", Some("alcohol")),
            ("OC1Cc2ccccc2-c2ccccc21", "phenanthrene", Some("alcohol")),
            ("O=C1c2ccccc2Cc2ccccc12", "anthracene", Some("ketone")),
            ("O=C1Cc2ccccc2-c2ccccc12", "phenanthrene", Some("ketone")),
            ("O=C1c2ccccc2C(=O)c2ccccc12", "anthracene", Some("ketone")),
        ] {
            let mol = read_smiles(smiles, &Default::default()).unwrap();
            let rings = find_ring_systems(&mol, &AtomSet::new()).unwrap();
            let retained = production_retained_fused_parent(
                &mol,
                &rings[0].atoms.iter().copied().collect::<Vec<_>>(),
                &mol.atoms.keys().copied().collect(),
                &perceive_groups(&mol),
                principal,
                &BTreeMap::new(),
                None,
            )
            .unwrap()
            .unwrap_or_else(|| panic!("{smiles}"));
            assert_eq!(retained.name, parent, "{smiles}");
            assert!(!retained.locant_maps.is_empty(), "{smiles}");
            assert!(
                retained
                    .locant_maps
                    .iter()
                    .all(|map| map.len() == 14 && map.values().any(|locant| locant == "10a")),
                "{smiles}"
            );
            assert_eq!(retained.metadata.mancude_double_bonds, 7, "{smiles}");
            assert_eq!(retained.metadata.indicated_hydrogen_count, 0, "{smiles}");
            assert!(retained.metadata.default_indicated_h.is_empty(), "{smiles}");
        }
    }
}
