//! Principal characteristic-group selection and suffix assembly.
use super::{
    assembly_parts::{AssemblyParts, PrincipalGroupItem},
    chains::get_cyclic_atoms,
    graph_kernel::biconnected_edge_components,
    locants::parse_locant,
    molecule::{AtomSet, Molecule, bond_ids_within},
    nomenclature::RULES,
    perception::{PerceivedGroup, perceive_groups},
};
use std::collections::BTreeMap;

pub fn component_groups(mol: &Molecule, atoms: &AtomSet) -> Vec<PerceivedGroup> {
    perceive_groups(mol)
        .into_iter()
        .filter(|g| atoms.contains(&g.attachment_carbon))
        .collect()
}

/// P-41: a skeletal anionic suffix is senior to neutral characteristic groups.
/// Apply this before excluding group atoms and enumerating parent chains: a
/// nitrile cited as cyano contributes no carbon to the parent skeleton.
pub fn demote_neutral_suffixes_for_skeletal_anion(
    mol: &Molecule,
    atoms: &AtomSet,
    groups: &mut [PerceivedGroup],
) {
    let group_atoms: AtomSet = groups
        .iter()
        .flat_map(|g| {
            g.atoms_involved
                .iter()
                .copied()
                .filter(|id| *id != g.attachment_carbon)
        })
        .collect();
    let skeletal_anions: AtomSet = atoms
        .iter()
        .copied()
        .filter(|id| mol.atoms[id].charge < 0 && !group_atoms.contains(id))
        .collect();
    if skeletal_anions.is_empty() {
        return;
    }

    // Oxo substitution on a mancude nitrogen anion can require added
    // hydrogen. Multiply cyclic ketones likewise need the fusion planner's
    // suffix-based hydrogen composition. Keep those existing derivations
    // until their prefix-based hydrogen composition is supported.
    let cyclic = get_cyclic_atoms(mol, &AtomSet::new());
    let protected_ketone_rings: Vec<AtomSet> = biconnected_edge_components(mol, &cyclic)
        .into_iter()
        .filter_map(|edges| {
            let ring: AtomSet = edges.iter().flat_map(|&(u, v)| [u, v]).collect();
            let fused_anion = edges.len() > ring.len() && !ring.is_disjoint(&skeletal_anions);
            let added_hydrogen_anion = edges.len() >= ring.len()
                && ring
                    .iter()
                    .any(|id| skeletal_anions.contains(id) && mol.atoms[id].symbol == "N")
                && edges.iter().any(|&(u, v)| {
                    mol.atoms[&u].is_aromatic
                        || mol.atoms[&v].is_aromatic
                        || mol.bond_order(u, v) > 1
                });
            (fused_anion || added_hydrogen_anion).then_some(ring)
        })
        .collect();
    for group in groups {
        // The attachment may itself be anionic; only the characteristic
        // group's own charge decides whether its suffix is neutral.
        let charge = group
            .atom_ids()
            .iter()
            .filter(|id| !skeletal_anions.contains(id))
            .map(|id| mol.atoms[id].charge)
            .sum::<i32>();
        if charge == 0
            && !(group.key == "ketone"
                && protected_ketone_rings
                    .iter()
                    .any(|ring| ring.contains(&group.attachment_carbon)))
        {
            group.is_principal_candidate = false;
        }
    }
}

fn neutral_anion_seniority(group: &PerceivedGroup) -> Option<&[i64]> {
    let neutral = match group.key.as_str() {
        "olate" => Some("alcohol"),
        "thiolate" => Some("thiol"),
        "selenolate" => Some("selenol"),
        "tellurolate" => Some("tellurol"),
        _ => None,
    };
    neutral
        .and_then(|key| RULES.functional_groups.get(key))
        .or(group.resolved_rule.as_ref())
        .and_then(|r| r.seniority.as_deref())
}

/// P-44.1: amide and carboxamide are suffix spellings of the same principal class.
pub fn equivalent_amide_keys(left: &str, right: &str) -> bool {
    matches!(left, "amide" | "ring_amide") && matches!(right, "amide" | "ring_amide")
}

/// Equivalent amide and anionic suffix classes remain candidates until
/// P-44.1 or P-72.7 chooses their parent.
pub fn candidate_principal_attachments(
    mol: &Molecule,
    groups: &[PerceivedGroup],
    key: Option<&str>,
) -> Vec<usize> {
    let selected = groups
        .iter()
        .find(|g| g.is_principal_candidate && Some(g.key.as_str()) == key);
    let anionic = |g: &PerceivedGroup| {
        g.atom_ids()
            .iter()
            .map(|id| mol.atoms[id].charge)
            .sum::<i32>()
            < 0
    };
    groups
        .iter()
        .filter(|g| {
            g.is_principal_candidate
                && (Some(g.key.as_str()) == key
                    || key.is_some_and(|key| equivalent_amide_keys(&g.key, key))
                    || selected.is_some_and(|selected| {
                        anionic(selected)
                            && anionic(g)
                            && neutral_anion_seniority(g) == neutral_anion_seniority(selected)
                    }))
        })
        .map(|g| g.attachment_carbon)
        .collect()
}

pub fn component_principal_key(
    mol: &Molecule,
    groups: &[PerceivedGroup],
    is_substituent: bool,
) -> Option<String> {
    if is_substituent {
        return None;
    }
    let markers: Vec<_> = groups
        .iter()
        .filter(|g| {
            g.is_principal_candidate
                && g.resolved_rule
                    .as_ref()
                    .is_some_and(|r| r.has_capability("promotes_member_groups"))
        })
        .collect();
    let effective = |group: &PerceivedGroup, seniority: &[i64]| {
        let mut best = seniority.to_vec();
        if let Some(descriptor) = &group.descriptor {
            for marker in &markers {
                if let Some(route) = &marker.descriptor
                    && !descriptor.centers.is_empty()
                    && descriptor.centers.iter().all(|c| route.centers.contains(c))
                    && let Some(priority) = marker
                        .resolved_rule
                        .as_ref()
                        .and_then(|r| r.seniority.as_ref())
                    && priority < &best
                {
                    best = priority.clone();
                }
            }
        }
        let charge: i32 = group.atom_ids().iter().map(|i| mol.atoms[i].charge).sum();
        // P-72.7(e): anionic suffixes rank as their neutral counterparts, so
        // carboxylate precedes olate.
        if matches!(
            group.key.as_str(),
            "olate" | "thiolate" | "selenolate" | "tellurolate"
        ) && let Some(rank) = neutral_anion_seniority(group)
        {
            best = rank.to_vec();
        }
        // P-70.2: ions precede neutral classes, including charge-separated
        // neutral groups such as azides and nitrile oxides.
        (charge == 0, best, seniority.to_vec())
    };
    // min_by keeps the first minimum, as Python's min does.
    groups
        .iter()
        .filter(|g| g.is_principal_candidate)
        .filter_map(|g| {
            g.resolved_rule.as_ref().and_then(|r| {
                (!r.has_capability("promotes_member_groups"))
                    .then_some(r)
                    .and_then(|r| r.seniority.as_ref())
                    .map(|s| (g, s))
            })
        })
        .min_by_key(|(g, s)| effective(g, s))
        .map(|(g, _)| g.key.clone())
}

pub fn partition_principal_and_prefix_groups(
    groups: &[PerceivedGroup],
    key: Option<&str>,
) -> (Vec<usize>, Vec<PerceivedGroup>) {
    let mut principal = Vec::new();
    let mut prefixes = Vec::new();
    for group in groups {
        if group.is_principal_candidate && Some(group.key.as_str()) == key {
            principal.push(group.attachment_carbon);
        } else {
            prefixes.push(group.clone());
        }
    }
    (principal, prefixes)
}

pub fn add_component_principal_group(
    mol: &Molecule,
    parts: &mut AssemblyParts,
    groups: &[PerceivedGroup],
    key: Option<&str>,
    principal: &[usize],
    path: &[usize],
    locants: &BTreeMap<usize, String>,
) {
    let Some(key) = key else {
        return;
    };
    let mut group_locants: Vec<_> = principal
        .iter()
        .filter(|idx| path.contains(idx))
        .map(|idx| locants[idx].clone())
        .collect();
    group_locants.sort_by_key(|s| parse_locant(s));
    let mut item = PrincipalGroupItem::new(key, group_locants);
    for group in groups
        .iter()
        .filter(|g| g.is_principal_candidate && g.key == key && path.contains(&g.attachment_carbon))
    {
        item.resolved_rule = group.resolved_rule.clone();
        item.atom_ids.insert(group.attachment_carbon);
        item.atom_ids.extend(&group.atoms_involved);
        if matches!(
            key,
            "ring_amidine" | "aldehyde_hydrazone" | "ring_aldehyde_hydrazone"
        ) {
            for &n in &group.atoms_involved {
                if mol.atoms[&n].symbol != "N" {
                    continue;
                }
                for &c in mol.get_neighbors(n) {
                    if mol.atoms[&c].symbol != "C" {
                        continue;
                    }
                    if let Some(bond) = mol.get_bond(c, n).filter(|b| b.order == 2)
                        && let Some(stereo) = &bond.stereo
                    {
                        parts.stereo_features.push((String::new(), stereo.clone()));
                    }
                }
            }
        }
    }
    item.bond_ids = bond_ids_within(mol, &item.atom_ids);
    item.charge_atom_ids = item
        .atom_ids
        .iter()
        .copied()
        .filter(|i| mol.atoms[i].charge != 0)
        .collect();
    // The parent graph cannot prove locant omission when a suffix also offers
    // substitution at atoms outside that graph, such as the nitrogen of an amide.
    item.external_substitution_sites = item.atom_ids.iter().any(|id| {
        mol.projected_substitution_centers.contains(id)
            || !parts.parent_atom_ids.contains(id)
                && !matches!(mol.atoms[id].symbol.as_str(), "H" | "O" | "S" | "Se" | "Te")
                && (mol.atoms[id].total_h_count > 0
                    || mol
                        .get_neighbors(*id)
                        .iter()
                        .any(|other| !item.atom_ids.contains(other)))
    });
    parts.principal_group = Some(item);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amide_parent_candidates_keep_both_suffix_spellings() {
        use super::super::{
            component_group_rules::retarget_external_carbonyl_groups, graph_io::read_smiles,
        };
        for (smiles, count) in [
            ("NC(=O)CNC(=O)c1ccccc1", 2),
            ("NC(=O)C(NC(=O)c1ccccc1)C(N)=O", 3),
        ] {
            let mol = read_smiles(smiles).unwrap();
            let groups = perceive_groups(&mol);
            let expected: Vec<_> = groups
                .iter()
                .filter(|g| g.is_principal_candidate)
                .map(|g| g.attachment_carbon)
                .collect();
            assert_eq!(expected.len(), count, "{smiles}");
            for key in ["amide", "ring_amide"] {
                let mut groups = groups.clone();
                let mut exclude = AtomSet::new();
                retarget_external_carbonyl_groups(
                    &mol,
                    &mut groups,
                    Some(key),
                    &mut exclude,
                    &get_cyclic_atoms(&mol, &AtomSet::new()),
                );
                assert_eq!(
                    candidate_principal_attachments(&mol, &groups, Some(key)),
                    expected,
                    "{smiles}: {key}"
                );
                assert!(exclude.is_empty(), "{smiles}: {key}");
            }
        }
        for smiles in ["O=C(O)Cc1ccc(C(=O)O)cc1", "N#CCc1ccc(C#N)cc1"] {
            let mol = read_smiles(smiles).unwrap();
            let groups = perceive_groups(&mol);
            let key = component_principal_key(&mol, &groups, false);
            assert_eq!(
                candidate_principal_attachments(&mol, &groups, key.as_deref()).len(),
                1,
                "{smiles}"
            );
        }
    }

    #[test]
    fn skeletal_anion_demotion_excludes_cyano_carbons_before_chain_selection() {
        use super::super::{
            chains::find_all_carbon_paths,
            component_group_rules::retarget_external_carbonyl_groups, graph_io::read_smiles,
        };
        for (smiles, expected_length) in [("N#C[CH-]C#N", 1), ("N#CCC[CH-]CC#N", 4)] {
            let mol = read_smiles(smiles).unwrap();
            let atoms = mol.atoms.keys().copied().collect();
            let mut groups = component_groups(&mol, &atoms);
            demote_neutral_suffixes_for_skeletal_anion(&mol, &atoms, &mut groups);
            assert!(component_principal_key(&mol, &groups, false).is_none());
            let mut exclude = AtomSet::new();
            retarget_external_carbonyl_groups(
                &mol,
                &mut groups,
                None,
                &mut exclude,
                &AtomSet::new(),
            );
            let chains = find_all_carbon_paths(&mol, &exclude).unwrap();
            assert_eq!(
                chains.iter().map(Vec::len).max(),
                Some(expected_length),
                "{smiles}"
            );
            assert_eq!(exclude.len(), 2, "{smiles}");
        }
    }

    #[test]
    fn skeletal_anion_demotion_keeps_cationic_suffixes_and_added_hydrogen_boundary() {
        use super::super::graph_io::read_smiles;
        for (smiles, key) in [
            ("[NH3+]CC[CH-]C#N", Some("aminium")),
            ("[CH2-]CCS", None),
            ("[CH2-]CC=N", None),
            ("O=C1C=CC=C[N-]1", Some("ketone")),
            ("O=C1[CH-]NC2=C1C[NH2+]C2", Some("ketone")),
            ("[N-]1C(=O)NC(=O)C1", None),
        ] {
            let mol = read_smiles(smiles).unwrap();
            let atoms = mol.atoms.keys().copied().collect();
            let mut groups = component_groups(&mol, &atoms);
            demote_neutral_suffixes_for_skeletal_anion(&mol, &atoms, &mut groups);
            assert_eq!(
                component_principal_key(&mol, &groups, false).as_deref(),
                key,
                "{smiles}"
            );
        }
        let mut mol = read_smiles("CC#N").unwrap();
        let mut external_anion = super::super::molecule::Atom::new(100, "C");
        external_anion.charge = -1;
        mol.add_atom(external_anion).unwrap();
        let atoms = mol
            .atoms
            .keys()
            .copied()
            .filter(|id| mol.atoms[id].charge == 0)
            .collect();
        let mut groups = component_groups(&mol, &atoms);
        demote_neutral_suffixes_for_skeletal_anion(&mol, &atoms, &mut groups);
        assert_eq!(
            component_principal_key(&mol, &groups, false).as_deref(),
            Some("nitrile")
        );
    }

    #[test]
    fn anionic_suffixes_precede_neutral_and_charge_separated_groups() {
        for (smiles, key) in [
            ("[NH-]CC(=O)[O-]", "aminide"),
            ("[O-]CCC(=O)N=[N+]=[N-]", "olate"),
            ("[O-]CC[Se-]", "olate"),
            ("[S-]CC[Te-]", "thiolate"),
            ("[O-]CCC(=O)[O-]", "carboxylate"),
        ] {
            let mol = super::super::graph_io::read_smiles(smiles).unwrap();
            let groups = perceive_groups(&mol);
            assert_eq!(
                component_principal_key(&mol, &groups, false).as_deref(),
                Some(key),
                "{smiles}"
            );
        }
    }
}
