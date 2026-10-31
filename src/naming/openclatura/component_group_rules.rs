//! Functional-group atom roles and preprocessing of parent candidates.
use super::molecule::{AtomSet, Molecule};
use super::nomenclature::RULES;
use super::perception::PerceivedGroup;

pub fn bridge_oxygen(m: &Molecule, g: &PerceivedGroup) -> Option<usize> {
    g.atoms_involved
        .iter()
        .copied()
        .find(|n| m.degree(*n) == 2 && m.atoms[n].symbol == "O")
}
pub fn ester_single_oxygen(m: &Molecule, g: &PerceivedGroup) -> Option<usize> {
    if let Some(d) = &g.descriptor
        && let Some(l) = d.ligands.iter().find(|l| l.role == "carbon_link")
    {
        return Some(l.atom);
    }
    g.atoms_involved
        .iter()
        .copied()
        .find(|n| m.atoms[n].symbol == "O" && (m.degree(*n) == 2 || m.atoms[n].charge == -1))
}
pub fn peroxy_ester_single_oxygen(m: &Molecule, g: &PerceivedGroup) -> Option<usize> {
    g.atoms_involved.iter().copied().find(|n| {
        m.atoms[n].symbol == "O"
            && m.get_bond(*n, g.attachment_carbon).is_none()
            && m.degree(*n) == 2
    })
}
pub fn ester_or_peroxy_single_oxygen(m: &Molecule, g: &PerceivedGroup) -> Option<usize> {
    if RULES.prefixes.peroxy_ester_groups.contains(&g.key) {
        peroxy_ester_single_oxygen(m, g)
    } else {
        ester_single_oxygen(m, g)
    }
}
pub fn sulfonyl_sulfur(m: &Molecule, g: &PerceivedGroup) -> Option<usize> {
    g.atoms_involved
        .iter()
        .copied()
        .find(|n| m.atoms[n].symbol == "S")
}
pub fn amide_nitrogen(m: &Molecule, g: &PerceivedGroup) -> Option<usize> {
    g.atoms_involved
        .iter()
        .copied()
        .find(|n| m.atoms[n].symbol == "N")
}
pub fn hydrazone_characteristic_carbon(m: &Molecule, g: &PerceivedGroup) -> Option<usize> {
    for n in &g.atoms_involved {
        if m.atoms[n].symbol != "N" {
            continue;
        }
        if let Some(c) =
            m.get_neighbors(*n).iter().copied().find(|c| {
                m.atoms[c].is_carbon() && m.get_bond(*n, *c).is_some_and(|b| b.order == 2)
            })
        {
            return Some(c);
        }
    }
    None
}
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
pub fn retarget_external_carbonyl_groups(
    m: &Molecule,
    groups: &mut [PerceivedGroup],
    principal_key: Option<&str>,
    exclude: &mut AtomSet,
    cyclic: &AtomSet,
) {
    for g in groups {
        if Some(g.key.as_str()) == principal_key
            || !g
                .resolved_rule
                .as_ref()
                .is_some_and(|r| r.has_capability("chain_external_carbonyl"))
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
                        .filter(|l| l.role == "carbon_link")
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
                        .filter(|l| l.role == "carbon_link")
                        .map(|l| l.atom),
                );
            }
        }
        exclude.extend(atoms.into_iter().filter(|n| !cyclic.contains(n)));
    }
}
pub fn principal_involved_atoms(
    m: &Molecule,
    groups: &[PerceivedGroup],
    principal_key: Option<&str>,
    parent_path: &[usize],
) -> AtomSet {
    let mut atoms = AtomSet::new();
    if let Some(key) = principal_key {
        for g in groups {
            if g.key == key && parent_path.contains(&g.attachment_carbon) {
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
