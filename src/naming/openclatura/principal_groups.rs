//! Principal characteristic-group selection and suffix assembly.
use super::{
    assembly_parts::{AssemblyParts, PrincipalGroupItem},
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
        let neutral = match group.key.as_str() {
            "olate" => Some("alcohol"),
            "thiolate" => Some("thiol"),
            "selenolate" => Some("selenol"),
            "tellurolate" => Some("tellurol"),
            _ => None,
        };
        if let Some(rank) = neutral
            .and_then(|key| RULES.functional_groups.get(key))
            .and_then(|r| r.seniority.as_ref())
        {
            best = rank.clone();
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
        if Some(group.key.as_str()) == key {
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
        .filter(|g| g.key == key && path.contains(&g.attachment_carbon))
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
