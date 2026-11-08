// SPDX-License-Identifier: MIT AND (MIT OR Apache-2.0)
// Origin: mixed
// Source: openclatura | src/openclatura/group_atom_roles.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | src/openclatura/component_group_rules.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: ReShiki | src/naming/openclatura/component_group_rules.rs | 5f559fd482e237307a589dfcb7ea333cee0a6e13
// Source: openclatura | tests/fixtures/diverse_corpus.csv | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Copyright (c) 2026 lamalab-org
// Copyright (c) 2026 Ameyanagi and ReShiki contributors
// Changes: Openclatura parent preprocessing and atom roles with local external-chain carboxylic selection.
// Local contributions: MIT OR Apache-2.0; see NOTICE and PROVENANCE-nazuchem.toml.

//! Functional-group atom roles and preprocessing of parent candidates.
use super::molecule::{AtomSet, Molecule};
use super::nomenclature::RULES;
use super::parent_selection::{ParentSelection, select_principal_parent};
use super::perception::{LigandRole, PerceivedGroup};
use super::principal_groups::{is_neutral_amide, selects_neutral_amides};

pub struct ExternalCarboxylicParent {
    pub groups: Vec<PerceivedGroup>,
    pub key: String,
    pub selection: ParentSelection,
}

/// P-65.1.2.2.1: compare chains with the carboxy carbons outside the parent,
/// retaining this mode only when the chosen chain carries more than two groups.
// Provenance: fn external_chain_carboxylic_parent | original
// Source: ReShiki | src/naming/openclatura/component_group_rules.rs::external_chain_carboxylic_parent
pub fn external_chain_carboxylic_parent(
    mol: &Molecule,
    groups: &[PerceivedGroup],
    key: Option<&str>,
    base_exclude: &AtomSet,
    cyclic: &AtomSet,
) -> Result<Option<ExternalCarboxylicParent>, String> {
    let carboxylic: Vec<_> = groups
        .iter()
        .enumerate()
        .filter_map(|(index, group)| {
            let d = group.descriptor.as_ref()?;
            (d.family == "acyl"
                && d.centers.len() == 1
                && d.ligands.len() == 2
                && d.ligands.iter().all(|l| l.element == "O")
                && matches!(d.derivative.as_str(), "acid" | "anion" | "ester"))
            .then_some((index, group, d))
        })
        .collect();
    let Some((_, _, descriptor)) = carboxylic
        .iter()
        .find(|(_, g, _)| Some(g.key.as_str()) == key)
    else {
        return Ok(None);
    };
    let derivative = descriptor.derivative.as_str();
    if carboxylic.len() <= 2
        || carboxylic
            .iter()
            .any(|(_, _, d)| d.derivative != derivative)
    {
        return Ok(None);
    }
    if derivative == "ester" {
        let centers: AtomSet = carboxylic
            .iter()
            .flat_map(|(_, _, d)| &d.centers)
            .copied()
            .collect();
        let linkers: AtomSet = carboxylic
            .iter()
            .flat_map(|(_, _, d)| &d.ligands)
            .filter(|l| l.role == LigandRole::CarbonLink)
            .map(|l| l.atom)
            .collect();
        let mut reference = None;
        for (_, _, descriptor) in &carboxylic {
            let Some(ligand) = descriptor
                .ligands
                .iter()
                .find(|l| l.role == LigandRole::CarbonLink)
            else {
                return Ok(None);
            };
            let Some(root) = ligand.attachment_atom else {
                return Ok(None);
            };
            let side =
                super::subgraph_tools::subgraph_component(mol, root, &AtomSet::from([ligand.atom]));
            if !side.is_disjoint(&centers) || !side.is_disjoint(&linkers) {
                return Ok(None);
            }
            if let Some((left, left_root)) = &reference {
                if !super::identical_multiplicative_branches(
                    mol,
                    left,
                    &side,
                    &AtomSet::new(),
                    &AtomSet::new(),
                    *left_root,
                    root,
                ) {
                    return Ok(None);
                }
            } else {
                reference = Some((side, root));
            }
        }
    }

    let external_key = if derivative == "acid" {
        "ring_carboxylic_acid"
    } else {
        "ring_carboxylate"
    };
    let mut remapped = groups.to_vec();
    let mut exclude = base_exclude.clone();
    for (index, group, descriptor) in carboxylic {
        let center = descriptor.centers[0];
        if cyclic.contains(&center) {
            continue;
        }
        let neighbors: Vec<_> = mol
            .get_neighbors(center)
            .iter()
            .copied()
            .filter(|n| mol.atoms[n].is_carbon() && !group.atoms_involved.contains(n))
            .collect();
        let [attachment] = neighbors.as_slice() else {
            return Ok(None);
        };
        let Some(mut rule) = RULES.functional_groups.get(external_key).cloned() else {
            return Ok(None);
        };
        if !group
            .resolved_rule
            .as_ref()
            .is_some_and(|r| r.has_capability("front_modifier"))
        {
            rule.families.remove("front_modifier");
            rule.capabilities.remove("front_modifier");
        }
        let remapped = &mut remapped[index];
        remapped.key = external_key.into();
        remapped.resolved_rule = Some(rule);
        remapped.attachment_carbon = *attachment;
        remapped.atoms_involved.insert(center);
        if let Some(descriptor) = &mut remapped.descriptor {
            descriptor.is_external = true;
            descriptor.attachment_atom = Some(*attachment);
        }
        exclude.insert(center);
    }
    retarget_external_carbonyl_groups(mol, &mut remapped, Some(external_key), &mut exclude, cyclic);
    exclude_nonparent_group_atoms(mol, &remapped, &mut exclude, cyclic);
    let (principal, _) = super::principal_groups::partition_principal_and_prefix_groups(
        &remapped,
        Some(external_key),
    );
    let chains = super::chains::find_all_carbon_paths(mol, &exclude)?;
    let rings = super::chains::find_ring_systems(mol, &exclude)?;
    let Some(selection) = select_principal_parent(mol, &chains, &rings, &principal, &remapped)
    else {
        return Ok(None);
    };
    if selection.is_ring
        || principal
            .iter()
            .filter(|atom| selection.primary_path().contains(atom))
            .count()
            <= 2
    {
        return Ok(None);
    }
    // Anion reassessment still includes carboxy carbons in candidate parents.
    // Keep that representation when any anionic centre lies outside the suffix.
    let suffix_anions = remapped
        .iter()
        .filter(|g| {
            is_carboxylate_group(g) && selection.primary_path().contains(&g.attachment_carbon)
        })
        .count();
    let anions = mol
        .atoms
        .iter()
        .filter(|(i, a)| !base_exclude.contains(i) && a.charge < 0)
        .count();
    if anions > suffix_anions {
        return Ok(None);
    }
    Ok(Some(ExternalCarboxylicParent {
        groups: remapped,
        key: external_key.into(),
        selection,
    }))
}

/// The registry's ring-carboxylate key also represents neutral esters, so the
/// descriptor, rather than that key alone, establishes an acid anion prefix.
// Provenance: fn is_carboxylate_group | original
// Source: ReShiki | src/naming/openclatura/component_group_rules.rs::is_carboxylate_group
pub fn is_carboxylate_group(g: &PerceivedGroup) -> bool {
    g.descriptor.as_ref().is_some_and(|d| {
        d.family == "acyl"
            && d.derivative == "anion"
            && d.ligands.len() == 2
            && d.ligands.iter().all(|l| l.element == "O")
    })
}

// Provenance: fn is_carboxylic_acid_group | original
// Source: ReShiki | src/naming/openclatura/component_group_rules.rs::is_carboxylic_acid_group
pub fn is_carboxylic_acid_group(g: &PerceivedGroup) -> bool {
    g.descriptor.as_ref().is_some_and(|d| {
        d.family == "acyl"
            && d.derivative == "acid"
            && d.ligands.len() == 2
            && d.ligands.iter().all(|l| l.element == "O")
    })
}

// Provenance: fn bridge_oxygen | port
// Source: openclatura | src/openclatura/group_atom_roles.py::bridge_oxygen
pub fn bridge_oxygen(m: &Molecule, g: &PerceivedGroup) -> Option<usize> {
    g.atoms_involved
        .iter()
        .copied()
        .find(|n| m.degree(*n) == 2 && m.atoms[n].symbol == "O")
}
// Provenance: fn ester_single_oxygen | port
// Source: openclatura | src/openclatura/group_atom_roles.py::ester_single_oxygen
pub fn ester_single_oxygen(m: &Molecule, g: &PerceivedGroup) -> Option<usize> {
    if let Some(d) = &g.descriptor
        && let Some(l) = d.ligands.iter().find(|l| l.role == LigandRole::CarbonLink)
    {
        return Some(l.atom);
    }
    g.atoms_involved
        .iter()
        .copied()
        .find(|n| m.atoms[n].symbol == "O" && (m.degree(*n) == 2 || m.atoms[n].charge == -1))
}
// Provenance: fn peroxy_ester_single_oxygen | port
// Source: openclatura | src/openclatura/group_atom_roles.py::peroxy_ester_single_oxygen
pub fn peroxy_ester_single_oxygen(m: &Molecule, g: &PerceivedGroup) -> Option<usize> {
    g.atoms_involved.iter().copied().find(|n| {
        m.atoms[n].symbol == "O"
            && m.get_bond(*n, g.attachment_carbon).is_none()
            && m.degree(*n) == 2
    })
}
// Provenance: fn ester_or_peroxy_single_oxygen | port
// Source: openclatura | src/openclatura/group_atom_roles.py::ester_or_peroxy_single_oxygen
pub fn ester_or_peroxy_single_oxygen(m: &Molecule, g: &PerceivedGroup) -> Option<usize> {
    if RULES.prefixes.peroxy_ester_groups.contains(&g.key) {
        peroxy_ester_single_oxygen(m, g)
    } else {
        ester_single_oxygen(m, g)
    }
}
// Provenance: fn sulfonyl_sulfur | port
// Source: openclatura | src/openclatura/group_atom_roles.py::sulfonyl_sulfur
pub fn sulfonyl_sulfur(m: &Molecule, g: &PerceivedGroup) -> Option<usize> {
    g.atoms_involved
        .iter()
        .copied()
        .find(|n| m.atoms[n].symbol == "S")
}
// Provenance: fn amide_nitrogen | port
// Source: openclatura | src/openclatura/group_atom_roles.py::amide_nitrogen
pub fn amide_nitrogen(m: &Molecule, g: &PerceivedGroup) -> Option<usize> {
    g.atoms_involved
        .iter()
        .copied()
        .find(|n| m.atoms[n].symbol == "N")
}
// Provenance: fn hydrazone_characteristic_carbon | port
// Source: openclatura | src/openclatura/group_atom_roles.py::hydrazone_characteristic_carbon
pub fn hydrazone_characteristic_carbon(m: &Molecule, g: &PerceivedGroup) -> Option<usize> {
    g.atoms_involved
        .iter()
        .filter(|&n| m.atoms[n].symbol == "N")
        .find_map(|&n| {
            m.get_neighbors(n)
                .iter()
                .copied()
                .find(|c| m.atoms[c].is_carbon() && m.bond_order(n, *c) == 2)
        })
}
// Provenance: fn group_carbon_off_attachment | port
// Source: openclatura | src/openclatura/group_atom_roles.py::group_carbon_off_attachment
pub fn group_carbon_off_attachment(m: &Molecule, g: &PerceivedGroup) -> Option<usize> {
    let c = g
        .atoms_involved
        .iter()
        .copied()
        .find(|n| m.atoms[n].is_carbon() && *n != g.attachment_carbon)?;
    m.atoms
        .values()
        .any(|a| a.is_carbon() && a.idx != c)
        .then_some(c)
}
// Provenance: fn retarget_external_carbonyl_groups | adapted
// Source: openclatura | src/openclatura/component_group_rules.py::retarget_external_carbonyl_groups
pub fn retarget_external_carbonyl_groups(
    m: &Molecule,
    groups: &mut [PerceivedGroup],
    principal_key: Option<&str>,
    exclude: &mut AtomSet,
    cyclic: &AtomSet,
) {
    // The chain and ring spellings describe the same anionic suffix class.
    // Both remain candidates until parent selection resolves their tie.
    let selects_carboxylate = groups.iter().any(|g| {
        g.is_principal_candidate && Some(g.key.as_str()) == principal_key && is_carboxylate_group(g)
    });
    let neutral_amides = selects_neutral_amides(m, groups, principal_key);
    for g in groups {
        // Only an established prefix excludes an acid-anion carbon. Other
        // carboxylates must remain available as parent candidates.
        let prefix_carboxylate = principal_key.is_none() && is_carboxylate_group(g);
        if g.is_principal_candidate
            && (Some(g.key.as_str()) == principal_key
                || neutral_amides && is_neutral_amide(m, g)
                || selects_carboxylate && is_carboxylate_group(g))
            || !(prefix_carboxylate
                || g.resolved_rule
                    .as_ref()
                    .is_some_and(|r| r.has_capability("chain_external_carbonyl")))
        {
            continue;
        }
        let c = g.attachment_carbon;
        if cyclic.contains(&c) {
            continue;
        }
        let adj = m
            .get_neighbors(c)
            .iter()
            .copied()
            .filter(|n| m.atoms[n].is_carbon() && !g.atoms_involved.contains(n))
            .collect::<Vec<_>>();
        if adj.len() == 1 {
            g.attachment_carbon = adj[0];
            g.atoms_involved.insert(c);
            exclude.insert(c);
        }
    }
}
// Provenance: fn exclude_nonparent_group_atoms | adapted
// Source: openclatura | src/openclatura/component_group_rules.py::exclude_nonparent_group_atoms
pub fn exclude_nonparent_group_atoms(
    m: &Molecule,
    groups: &[PerceivedGroup],
    exclude: &mut AtomSet,
    cyclic: &AtomSet,
) {
    for g in groups {
        let r = g.resolved_rule.as_ref();
        let family = |f: &str| r.is_some_and(|r| r.families.contains(f));
        let selected = if g.key == "anhydride" {
            bridge_oxygen(m, g)
        } else if family("peroxy_ester") {
            peroxy_ester_single_oxygen(m, g)
        } else if family("ester_like") {
            ester_single_oxygen(m, g)
        } else if family("sulfonyl") {
            sulfonyl_sulfur(m, g)
        } else if family("amide_like") {
            amide_nitrogen(m, g)
        } else if matches!(
            g.key.as_str(),
            "isocyano" | "isocyanato" | "isothiocyanato" | "cyanato" | "thiocyanato"
        ) {
            group_carbon_off_attachment(m, g)
        } else {
            None
        };
        let mut atoms = selected.into_iter().collect::<AtomSet>();
        if let Some(d) = &g.descriptor {
            if atoms.is_empty() && d.family == "acyl" && d.derivative == "ester" {
                atoms.extend(
                    d.ligands
                        .iter()
                        .filter(|l| l.role == LigandRole::CarbonLink)
                        .map(|l| l.atom),
                );
            }
            if atoms.is_empty()
                && d.family == "acyl"
                && matches!(d.derivative.as_str(), "amide" | "hydrazide" | "urea")
            {
                atoms.extend(amide_nitrogen(m, g));
            }
            if d.derivative == "acyl_pseudohalide"
                && let Some(l) = &d.leaving_group
            {
                atoms.extend(&l.atom_ids);
            }
            if d.family == "central_acid" {
                atoms.extend(&d.centers);
                atoms.extend(
                    d.ligands
                        .iter()
                        .filter(|l| l.role == LigandRole::CarbonLink)
                        .map(|l| l.atom),
                );
            }
        }
        exclude.extend(atoms.into_iter().filter(|n| !cyclic.contains(n)));
    }
}
// Provenance: fn principal_involved_atoms | adapted
// Source: openclatura | src/openclatura/component_group_rules.py::principal_involved_atoms
pub fn principal_involved_atoms(
    m: &Molecule,
    groups: &[PerceivedGroup],
    principal_key: Option<&str>,
    parent_path: &[usize],
) -> AtomSet {
    let mut atoms = AtomSet::new();
    if let Some(key) = principal_key {
        for g in groups {
            if g.is_principal_candidate
                && g.key == key
                && parent_path.contains(&g.attachment_carbon)
            {
                atoms.extend(&g.atoms_involved);
                if matches!(
                    g.key.as_str(),
                    "ring_aldehyde_hydrazone" | "ring_aldehyde_amidinohydrazone"
                ) && let Some(c) = hydrazone_characteristic_carbon(m, g)
                    && !parent_path.contains(&c)
                {
                    atoms.insert(c);
                }
            }
        }
    }
    atoms
}

#[cfg(test)]
// Provenance: mod tests | original
// Source: ReShiki | src/naming/openclatura/component_group_rules.rs::tests
mod tests {
    use super::*;

    fn external_parent(smiles: &str) -> Option<ExternalCarboxylicParent> {
        let mol = super::super::graph_io::read_smiles(smiles, &Default::default()).unwrap();
        let groups = super::super::principal_groups::component_groups(
            &mol,
            &mol.atoms.keys().copied().collect(),
        );
        let key = super::super::principal_groups::component_principal_key(&mol, &groups, false);
        let cyclic = super::super::chains::get_cyclic_atoms(&mol, &AtomSet::new());
        external_chain_carboxylic_parent(&mol, &groups, key.as_deref(), &AtomSet::new(), &cyclic)
            .unwrap()
    }

    #[test]
    fn external_carboxylic_chains_count_repeated_attachments_before_length() {
        for (smiles, length, count) in [
            ("OC(=O)C(C(=O)O)C(=O)O", 1, 3),
            ("OC(=O)C(C)(C(=O)O)C(=O)O", 2, 3),
            ("OC(=O)C(C(=O)O)C(C(=O)O)C(=O)O", 2, 4),
            ("OC(=O)C(C(=O)O)CCC(=O)O", 3, 3),
            ("OC(=O)C(C(=O)O)C(CCCCC)CC(=O)O", 3, 3),
        ] {
            let candidate = external_parent(smiles).unwrap();
            let path = candidate.selection.primary_path();
            assert_eq!(path.len(), length, "{smiles}");
            assert_eq!(
                candidate
                    .groups
                    .iter()
                    .filter(|g| g.key == candidate.key && path.contains(&g.attachment_carbon))
                    .count(),
                count,
                "{smiles}"
            );
            assert!(
                candidate
                    .groups
                    .iter()
                    .filter(|g| g.key == candidate.key)
                    .all(|g| {
                        g.descriptor.as_ref().is_some_and(|d| {
                            d.is_external && d.centers.iter().all(|c| !path.contains(c))
                        })
                    })
            );
        }
    }

    #[test]
    fn external_carboxylic_chains_require_homogeneous_independent_ester_ligands() {
        for smiles in [
            "[O-]C(=O)CC(O)(CC(=O)[O-])C(=O)[O-]",
            "CCOC(=O)CC(O)(CC(=O)OCC)C(=O)OCC",
        ] {
            let candidate = external_parent(smiles).unwrap();
            assert_eq!(candidate.key, "ring_carboxylate", "{smiles}");
            assert!(
                candidate
                    .groups
                    .iter()
                    .filter(|g| g.key == candidate.key)
                    .all(|g| {
                        g.resolved_rule
                            .as_ref()
                            .is_some_and(|r| r.has_capability("front_modifier"))
                    })
            );
        }
        for smiles in [
            "CCOC(=O)CC(O)(CC(=O)O)C(=O)O",
            "[O-]C(=O)CC(O)(CC(=O)O)C(=O)O",
            "COC(=O)CC(O)(CC(=O)OCC)C(=O)OC",
            "CCCOC(=O)CC(O)(CC(=O)OCCC)C(=O)OC(C)C",
            "COC(=O)CC(CC(=O)OC)C(=O)OCCOC(=O)C",
        ] {
            assert!(external_parent(smiles).is_none(), "{smiles}");
        }
    }

    #[test]
    fn external_carboxylate_chains_require_all_carboxylates_in_the_suffix() {
        assert!(external_parent("[O-]C(=O)CC(CC(=O)[O-])(CC(=O)[O-])C(=O)[O-]").is_none());
        assert!(external_parent("[O-]C(=O)CC([O-])(CC(=O)[O-])C(=O)[O-]").is_none());
    }

    #[test]
    // Provenance: fn tests::external_carboxylic_chains_require_three_groups_on_one_principal_chain | adapted
    // Source: ReShiki | src/naming/openclatura/component_group_rules.rs::tests::external_carboxylic_chains_require_three_groups_on_one_principal_chain
    // Source: openclatura | tests/fixtures/diverse_corpus.csv::<fixture data>
    // Test provenance: upstream inputs/expected names with local assertions and extensions;
    // exact matches are conservatively adapted; independent ancestry is not established.
    fn external_carboxylic_chains_require_three_groups_on_one_principal_chain() {
        for smiles in [
            "OC(=O)CC(=O)O",
            "OC(=O)CCC(=O)O",
            "OC(=O)CC(CC(=O)O)CC(=O)O",
            "O=C(O)c1cc(C(=O)O)cc(C(=O)O)c1",
            "[O-]CCC(CC(=O)O)(CC(=O)O)C(=O)O",
        ] {
            assert!(external_parent(smiles).is_none(), "{smiles}");
        }
    }
}
