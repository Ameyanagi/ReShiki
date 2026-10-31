//! Indicated and additive hydrogen for retained parents.
use super::{
    assembly_parts::{AssemblyParts, HydroOperation},
    assembly_utils::parse_locant,
    molecule::{AtomSet, Molecule},
    rules::{elements, retained},
};
use std::collections::{BTreeMap, BTreeSet};

type Site = (String, usize);

pub fn parent_indicated_hydrogen_quota(parts: &AssemblyParts) -> Option<usize> {
    let plan = if let Some(parent) = &parts.parent_hydride {
        parent
            .uses_fusion_plan()
            .then_some(parent.fusion_plan.as_ref())
            .flatten()
    } else {
        parts.fusion_plan.as_ref()
    };
    if let Some(plan) = plan {
        let model =
            super::fusion::numbering::parent_bond_model(&plan.abstract_parent_graph).ok()?;
        return Some(
            super::fusion::numbering::parent_pi_capable_atom_ids(&plan.abstract_parent_graph)
                .ok()?
                .len()
                .saturating_sub(2 * model.maximum_non_cumulative_double_bonds),
        );
    }
    parts
        .retained_parent_metadata
        .as_ref()
        .filter(|metadata| metadata.mancude_double_bonds > 0)
        .map(|metadata| metadata.indicated_hydrogen_count)
}

pub fn name_indicated_hydrogen_locants(name: Option<&str>) -> BTreeSet<String> {
    let Some((run, _)) = name.and_then(|s| s.split_once('-')) else {
        return BTreeSet::new();
    };
    let locants: Option<BTreeSet<_>> = run
        .split(',')
        .map(|part| {
            let locant = part.strip_suffix('H')?;
            let n = locant.chars().take_while(char::is_ascii_digit).count();
            (n > 0 && locant[n..].chars().all(|c| c.is_ascii_lowercase()))
                .then(|| locant.to_owned())
        })
        .collect();
    locants.unwrap_or_default()
}

fn operation(key: &str, sites: &[Site]) -> HydroOperation {
    HydroOperation {
        key: key.into(),
        operation_kind: if key == "additive_hydrogen" {
            key.into()
        } else {
            "indicated_hydrogen".into()
        },
        locants: sites.iter().map(|s| s.0.clone()).collect(),
        atom_ids: sites.iter().map(|s| s.1).collect(),
        ..Default::default()
    }
}
fn add_citations(parts: &mut AssemblyParts, key: &str, sites: &[Site]) {
    for site in sites {
        parts.indicated_hydrogens.push(site.0.clone());
        parts
            .hydro_operations
            .push(operation(key, std::slice::from_ref(site)));
    }
}
fn sort_sites(sites: &mut [Site]) {
    sites.sort_by_key(|s| parse_locant(&s.0));
}

fn ring_bonds<'a>(mol: &'a Molecule, idx: usize, ring: &AtomSet) -> Vec<&'a super::molecule::Bond> {
    mol.get_neighbors(idx)
        .iter()
        .filter(|n| ring.contains(n))
        .filter_map(|&n| mol.get_bond(idx, n))
        .collect()
}
fn saturated_ring_site(mol: &Molecule, idx: usize, ring: &AtomSet) -> bool {
    let bonds = ring_bonds(mol, idx, ring);
    !bonds.is_empty()
        && bonds.iter().all(|b| b.order == 1)
        && mol
            .get_neighbors(idx)
            .iter()
            .filter(|n| !ring.contains(n))
            .all(|&n| mol.get_bond(idx, n).is_none_or(|b| b.order <= 1))
}
fn exocyclic_double_bond_site(mol: &Molecule, idx: usize, ring: &AtomSet) -> bool {
    let bonds = ring_bonds(mol, idx, ring);
    mol.atoms[&idx].is_carbon()
        && !bonds.is_empty()
        && bonds.iter().all(|b| b.order == 1)
        && mol
            .get_neighbors(idx)
            .iter()
            .filter(|n| !ring.contains(n))
            .any(|&n| mol.get_bond(idx, n).is_some_and(|b| b.order == 2))
}
fn oxo_ring_site(mol: &Molecule, idx: usize, ring: &AtomSet) -> bool {
    exocyclic_double_bond_site(mol, idx, ring)
        && mol
            .get_neighbors(idx)
            .iter()
            .filter(|n| !ring.contains(n))
            .any(|&n| {
                matches!(mol.atoms[&n].symbol.as_str(), "O" | "S" | "N" | "C")
                    && mol.get_bond(idx, n).is_some_and(|b| b.order == 2)
            })
}
fn respell_indicated_hydrogen(name: &str, sites: &BTreeSet<String>) -> String {
    if name_indicated_hydrogen_locants(Some(name)).is_empty() {
        return name.into();
    }
    let Some((_, rest)) = name.split_once('-') else {
        return name.into();
    };
    let mut sites: Vec<_> = sites.iter().collect();
    sites.sort_by_key(|l| parse_locant(l));
    format!(
        "{}-{rest}",
        sites
            .iter()
            .map(|l| format!("{l}H"))
            .collect::<Vec<_>>()
            .join(",")
    )
}
fn fold_stem_hydro_prefix(parts: &mut AssemblyParts, locants: &BTreeMap<usize, String>) {
    let Some(name) = parts.retained_name.as_deref() else {
        return;
    };
    let mut split = name.splitn(3, '-');
    let (Some(run), Some(prefix), Some(rest)) = (split.next(), split.next(), split.next()) else {
        return;
    };
    if ![
        "dihydro",
        "trihydro",
        "tetrahydro",
        "pentahydro",
        "hexahydro",
        "heptahydro",
        "octahydro",
        "nonahydro",
        "decahydro",
    ]
    .contains(&prefix)
    {
        return;
    }
    let atom_by_locant: BTreeMap<_, _> = locants.iter().map(|(&i, l)| (l.clone(), i)).collect();
    let Some(stem_sites) = run
        .split(',')
        .map(|l| atom_by_locant.get(l).map(|&i| (l.to_owned(), i)))
        .collect::<Option<Vec<_>>>()
    else {
        return;
    };
    let Some(op) = parts.hydro_operations.last_mut() else {
        return;
    };
    let merged: BTreeMap<_, _> = op
        .locants
        .iter()
        .cloned()
        .zip(op.atom_ids.iter().copied())
        .chain(stem_sites)
        .collect();
    let mut merged: Vec<_> = merged.into_iter().collect();
    sort_sites(&mut merged);
    op.locants = merged.iter().map(|s| s.0.clone()).collect();
    op.atom_ids = merged.iter().map(|s| s.1).collect();
    parts.retained_name = Some(rest.into());
}
fn declared_split(
    pool: &[Site],
    defaults: &BTreeSet<String>,
    supported: usize,
    name_states: bool,
) -> Option<(Vec<Site>, Vec<Site>)> {
    if !name_states || defaults.is_empty() {
        return None;
    }
    let held: Vec<_> = pool
        .iter()
        .filter(|s| defaults.contains(&s.0))
        .cloned()
        .collect();
    if held.len() != supported {
        return None;
    }
    let mut surplus: Vec<_> = pool
        .iter()
        .filter(|s| !defaults.contains(&s.0))
        .cloned()
        .collect();
    sort_sites(&mut surplus);
    Some((held, surplus))
}
fn relocate_preferred_hydrogenation(
    mol: &Molecule,
    parts: &mut AssemblyParts,
    ring: &AtomSet,
    locants: &BTreeMap<usize, String>,
) {
    let (Some(name), Some(group)) = (&parts.retained_name, &parts.principal_group) else {
        return;
    };
    let Some(hydrogenation) = super::retained_name_policy::retained_parent_name_policies()
        .iter()
        .find(|policy| policy.preferred_name == *name && policy.hydrogenation.is_some())
        .and_then(|policy| policy.hydrogenation.as_ref())
    else {
        return;
    };
    let suffix_locants: BTreeSet<_> = group
        .atom_ids
        .intersection(ring)
        .filter(|&&idx| exocyclic_double_bond_site(mol, idx, ring))
        .map(|idx| locants[idx].clone())
        .collect();
    parts.retained_name = Some(hydrogenation.relocated(&suffix_locants).render());
}

/// Collect hydrogen roles after the retained graph and its numbering are proved.
pub fn add_indicated_hydrogens(
    mol: &Molecule,
    parts: &mut AssemblyParts,
    numbered_path: &[usize],
    locants: &BTreeMap<usize, String>,
) -> Result<(), String> {
    collect_indicated_hydrogens(mol, parts, numbered_path, locants)?;
    validate_retained_hydrogen_sites(mol, parts)
}

fn validate_retained_hydrogen_sites(mol: &Molecule, parts: &AssemblyParts) -> Result<(), String> {
    let Some(metadata) = parts
        .retained_parent_metadata
        .as_ref()
        .filter(|m| !m.fusion_locants.is_empty())
    else {
        return Ok(());
    };
    let mut sites: BTreeSet<_> = parts
        .hydro_operations
        .iter()
        .flat_map(|op| op.locants.iter().cloned())
        .chain(parts.indicated_hydrogens.iter().cloned())
        .chain(metadata.inherent_saturated_locants.iter().cloned())
        .collect();
    if let Some(name) = parts.retained_name.as_deref() {
        // Hydrogenation prefixes can precede the indicated-H parent name.
        sites.extend(name_indicated_hydrogen_locants(Some(name)));
        for (offset, _) in name.match_indices('-') {
            sites.extend(name_indicated_hydrogen_locants(Some(&name[offset + 1..])));
        }
    }
    if sites.iter().any(|locant| {
        parts
            .parent_atom_ids_by_locant
            .get(locant)
            .is_some_and(|&id| {
                ring_bonds(mol, id, &parts.parent_atom_ids)
                    .iter()
                    .any(|bond| bond.order > 1)
            })
    }) {
        // Upstream divergence: retained derivative admission and hydrogen-site
        // counting can leave indicated/added/hydro H on an unsaturated ring
        // atom (including N=C oxo/imino tautomers). Such a name changes the
        // tautomer. Check the final cited state for parents and substituents.
        return Err("openclatura: retained hydrogen state conflicts with ring bonding".into());
    }
    Ok(())
}

fn collect_indicated_hydrogens(
    mol: &Molecule,
    parts: &mut AssemblyParts,
    numbered_path: &[usize],
    locants: &BTreeMap<usize, String>,
) -> Result<(), String> {
    let ring: AtomSet = numbered_path.iter().copied().collect();
    relocate_preferred_hydrogenation(mol, parts, &ring, locants);
    if apply_retained_oxo_carbon_hydrogen(mol, parts, locants)? {
        return Ok(());
    }
    if add_proved_parent_hydrogens(mol, parts, locants)? {
        return Ok(());
    }
    if let Some(plan) =
        retained::mancude_monocycle_hydro_plan(mol, numbered_path, parts.retained_name.as_deref())
    {
        let mut ordered: Vec<_> = plan
            .citable
            .into_iter()
            .map(|i| (locants[&i].clone(), i))
            .collect();
        sort_sites(&mut ordered);
        let held = (plan.indicated + plan.added).min(ordered.len());
        add_citations(parts, "indicated_hydrogen", &ordered[..held]);
        if held < ordered.len() {
            parts
                .hydro_operations
                .push(operation("additive_hydrogen", &ordered[held..]));
        }
        recast_ring_ketone_hydrogens(parts, locants);
        return Ok(());
    }
    let metadata = parts.retained_parent_metadata.clone();
    let name_states =
        !retained::cites_indicated_hydrogen(mol, numbered_path, parts.retained_name.as_deref())
            && !metadata.as_ref().is_some_and(|m| m.relocated_indicated_h);
    if name_states && metadata.is_none() {
        return Ok(());
    }
    if name_states
        && metadata
            .as_ref()
            .is_some_and(|m| m.mancude_double_bonds == 0)
    {
        return Ok(());
    }
    if parts.retained_name.as_deref() == Some("tetrazole")
        && numbered_path.iter().any(|i| mol.atoms[i].charge != 0)
    {
        return Ok(());
    }
    let oxo = parts
        .principal_group
        .as_ref()
        .is_some_and(|g| g.key == "ketone");
    let suffix_sites: AtomSet = parts
        .principal_group
        .as_ref()
        .map(|g| g.atom_ids.intersection(&ring).copied().collect())
        .unwrap_or_default();
    let mut defaults: BTreeSet<_> = metadata
        .as_ref()
        .map(|m| m.default_indicated_h.iter().cloned().collect())
        .unwrap_or_default();
    let inherent: BTreeSet<_> = metadata
        .as_ref()
        .map(|m| m.inherent_saturated_locants.iter().cloned().collect())
        .unwrap_or_default();
    let mut declared = if defaults.is_empty() {
        name_indicated_hydrogen_locants(parts.retained_name.as_deref())
    } else {
        defaults.clone()
    };
    let observed: BTreeSet<_> = numbered_path
        .iter()
        .filter(|&&i| {
            mol.atoms[&i].is_carbon()
                && {
                    let b = ring_bonds(mol, i, &ring);
                    !b.is_empty() && b.iter().all(|b| b.order == 1)
                }
                && (mol.atoms[&i].explicit_h_count + mol.atoms[&i].total_h_count > 0
                    || oxo_ring_site(mol, i, &ring))
        })
        .map(|i| locants[i].clone())
        .collect();
    let unambiguous = numbered_path.iter().all(|i| {
        !defaults.contains(&locants[i])
            || mol.atoms[i].is_carbon()
            || numbered_path
                .iter()
                .filter(|j| mol.atoms[*j].symbol == mol.atoms[i].symbol)
                .count()
                <= 1
    });
    if !defaults.is_empty() && observed.len() == defaults.len() && unambiguous {
        defaults = observed;
    }
    if let Some(name) = parts.retained_name.clone() {
        let spelled = name_indicated_hydrogen_locants(Some(&name));
        let by_loc: BTreeMap<_, _> = locants.iter().map(|(&i, l)| (l.clone(), i)).collect();
        let stranded: BTreeSet<_> = spelled
            .iter()
            .filter(|l| {
                by_loc
                    .get(*l)
                    .is_some_and(|&i| !saturated_ring_site(mol, i, &ring))
            })
            .cloned()
            .collect();
        if !stranded.is_empty() {
            let mut free: Vec<_> = by_loc
                .iter()
                .filter(|(l, i)| {
                    !spelled.contains(*l)
                        && saturated_ring_site(mol, **i, &ring)
                        && !oxo_ring_site(mol, **i, &ring)
                        && !inherent.contains(*l)
                        && !matches!(mol.atoms[i].symbol.as_str(), "O" | "S" | "Se" | "Te")
                })
                .map(|(l, _)| l.clone())
                .collect();
            free.sort_by_key(|l| parse_locant(l));
            if free.len() >= stranded.len() {
                let relocated: BTreeSet<_> = spelled
                    .difference(&stranded)
                    .cloned()
                    .chain(free.into_iter().take(stranded.len()))
                    .collect();
                parts.retained_name = Some(respell_indicated_hydrogen(&name, &relocated));
                declared = relocated.clone();
                if defaults.is_empty() {
                    defaults = relocated;
                }
            }
        }
    }
    if let Some(name) = parts.retained_name.clone().filter(|_| !defaults.is_empty()) {
        let cited = name_indicated_hydrogen_locants(Some(&name));
        if !cited.is_empty() && cited != defaults && cited.len() == defaults.len() {
            parts.retained_name = Some(respell_indicated_hydrogen(&name, &defaults));
            declared = defaults.clone();
        }
    }
    let fusion: BTreeSet<_> = metadata
        .as_ref()
        .map(|m| m.fusion_locants.iter().cloned().collect())
        .unwrap_or_default();
    let mut candidates = Vec::new();
    let mut hydro_only = Vec::new();
    for &idx in numbered_path {
        let atom = &mol.atoms[&idx];
        let loc = locants[&idx].clone();
        if inherent.contains(&loc) {
            continue;
        }
        if !oxo && metadata.is_some() && oxo_ring_site(mol, idx, &ring) {
            if !suffix_sites.contains(&idx) {
                hydro_only.push((loc, idx));
            }
            continue;
        }
        if metadata.is_some()
            && !defaults.is_empty()
            && atom.is_carbon()
            && !defaults.contains(&loc)
        {
            if saturated_ring_site(mol, idx, &ring) {
                hydro_only.push((loc, idx));
            }
            continue;
        }
        let bonds = ring_bonds(mol, idx, &ring);
        let order: usize = bonds.iter().map(|b| b.order).sum();
        if oxo {
            let substituted_n = atom.symbol == "N" && order == 2;
            let oxo_indicated = defaults.contains(&loc) && oxo_ring_site(mol, idx, &ring);
            if atom.explicit_h_count + atom.total_h_count == 0 && !substituted_n && !oxo_indicated {
                continue;
            }
            if atom.is_carbon()
                && !defaults.contains(&loc)
                && !(defaults.is_empty() && bonds.len() == 3)
            {
                if metadata.is_some() && saturated_ring_site(mol, idx, &ring) {
                    hydro_only.push((loc, idx));
                }
                continue;
            }
            if !defaults.is_empty() && !defaults.contains(&loc) {
                if metadata.is_some()
                    && inherent.is_empty()
                    && !atom.is_carbon()
                    && saturated_ring_site(mol, idx, &ring)
                {
                    hydro_only.push((loc, idx));
                }
                continue;
            }
        }
        if !matches!(
            atom.symbol.as_str(),
            "C" | "N" | "P" | "As" | "Sb" | "Bi" | "B" | "Si" | "Ge" | "Sn" | "Pb"
        ) || elements::get(&atom.symbol).is_some_and(|e| e.mancude_forced_single)
        {
            continue;
        }
        let fusion_carbon = metadata.is_some()
            && defaults.is_empty()
            && atom.is_carbon()
            && fusion.contains(&loc)
            && bonds.len() == 3
            && saturated_ring_site(mol, idx, &ring);
        let saturated_fusion_h =
            bonds.len() == 3 && order == 3 && atom.total_h_count + atom.explicit_h_count > 0;
        let indicated = (order == 2 || saturated_fusion_h)
            && (!atom.is_carbon()
                || saturated_ring_site(mol, idx, &ring)
                || (defaults.contains(&loc) && exocyclic_double_bond_site(mol, idx, &ring)));
        if indicated || fusion_carbon {
            candidates.push((loc, idx));
        }
    }
    let supported = metadata
        .as_ref()
        .map_or(candidates.len(), |m| m.indicated_hydrogen_count)
        .saturating_sub(defaults.intersection(&inherent).count());
    let additive = candidates
        .iter()
        .any(|(l, i)| mol.atoms[i].is_carbon() && fusion.contains(l) && !declared.contains(l));
    if additive && candidates.len() > 1 {
        let mut pool: Vec<_> = candidates.into_iter().chain(hydro_only).collect();
        sort_sites(&mut pool);
        let held = if (pool.len() as isize - supported as isize) % 2 == 0 {
            supported.min(pool.len())
        } else {
            0
        };
        let (candidates, surplus) = declared_split(&pool, &declared, held, name_states)
            .unwrap_or_else(|| (pool[..held].to_vec(), pool[held..].to_vec()));
        if !surplus.is_empty() {
            parts
                .hydro_operations
                .push(operation("additive_hydrogen", &surplus));
        }
        if !name_states {
            add_citations(parts, "indicated_hydrogen", &candidates);
        }
        return Ok(());
    }
    let surplus_count = candidates.len() as isize + hydro_only.len() as isize - supported as isize;
    if metadata.is_some() && surplus_count > 0 && surplus_count % 2 == 0 {
        sort_sites(&mut candidates);
        let pool: Vec<_> = candidates.iter().chain(&hydro_only).cloned().collect();
        let (held, surplus) =
            if let Some(split) = declared_split(&pool, &declared, supported, name_states) {
                split
            } else if candidates.len() < supported {
                let mut p = pool;
                sort_sites(&mut p);
                let n = supported.min(p.len());
                (p[..n].to_vec(), p[n..].to_vec())
            } else {
                let mut s: Vec<_> = candidates[supported..]
                    .iter()
                    .chain(&hydro_only)
                    .cloned()
                    .collect();
                sort_sites(&mut s);
                (candidates[..supported].to_vec(), s)
            };
        candidates = held;
        parts
            .hydro_operations
            .push(operation("additive_hydrogen", &surplus));
        fold_stem_hydro_prefix(parts, locants);
    }
    let added_needed = oxo
        && supported == 0
        && declared.is_empty()
        && parts
            .retained_name
            .as_deref()
            .is_some_and(|n| !n.contains("H-"));
    if metadata.is_some() && oxo && (!name_states || added_needed) && surplus_count % 2 != 0 {
        candidates.extend(hydro_only.clone());
        sort_sites(&mut candidates);
    }
    if name_states && !added_needed {
        if metadata.is_some()
            && oxo
            && surplus_count > 0
            && surplus_count % 2 != 0
            && !hydro_only.is_empty()
        {
            let mut pool: Vec<_> = candidates
                .iter()
                .skip(supported)
                .chain(&hydro_only)
                .filter(|s| !declared.contains(&s.0))
                .cloned()
                .collect();
            sort_sites(&mut pool);
            if pool.is_empty() {
                return Ok(());
            }
            let ketones: AtomSet = numbered_path
                .iter()
                .filter(|&&i| oxo_ring_site(mol, i, &ring))
                .copied()
                .collect();
            let pos = pool
                .iter()
                .position(|s| mol.get_neighbors(s.1).iter().any(|n| ketones.contains(n)))
                .unwrap_or(0);
            let added = pool.remove(pos);
            add_citations(parts, "added_hydrogen", &[added]);
            if !pool.is_empty() {
                parts
                    .hydro_operations
                    .push(operation("additive_hydrogen", &pool));
            }
        }
        return Ok(());
    }
    if added_needed && candidates.len() >= 3 && (candidates.len() - 1) % 2 == 0 {
        sort_sites(&mut candidates);
        parts
            .hydro_operations
            .push(operation("additive_hydrogen", &candidates[1..]));
        candidates.truncate(1);
    }
    add_citations(
        parts,
        if added_needed {
            "added_hydrogen"
        } else {
            "indicated_hydrogen"
        },
        &candidates,
    );
    if oxo && !added_needed {
        recast_ring_ketone_hydrogens(parts, locants);
    }
    Ok(())
}

fn recast_ring_ketone_hydrogens(parts: &mut AssemblyParts, locants: &BTreeMap<usize, String>) {
    if parts
        .parent_hydride
        .as_ref()
        .is_some_and(|parent| parent.uses_fusion_plan() && parent.derivative_state().is_some())
        || parts.parent_hydride.is_none() && parts.fusion_plan.is_some()
    {
        return;
    }
    let Some(group) = parts.principal_group.as_ref().filter(|g| g.key == "ketone") else {
        return;
    };
    let ketone_locs = group.locants.clone();
    let declared = name_indicated_hydrogen_locants(parts.retained_name.as_deref());
    let indicated: Vec<_> = parts
        .hydro_operations
        .iter()
        .filter(|o| {
            o.key == "indicated_hydrogen" && o.locants.iter().all(|l| !declared.contains(l))
        })
        .cloned()
        .collect();
    if indicated.is_empty()
        || parts
            .hydro_operations
            .iter()
            .any(|o| o.key == "added_hydrogen")
    {
        return;
    }
    let nh: Vec<_> = indicated
        .iter()
        .filter_map(|o| o.locants.first().cloned())
        .collect();
    if nh.iter().any(|l| ketone_locs.contains(l)) {
        return;
    }
    if nh.len() == ketone_locs.len() {
        for op in &mut parts.hydro_operations {
            if indicated
                .iter()
                .any(|i| i.locants == op.locants && i.key == op.key)
            {
                op.key = "added_hydrogen".into();
            }
        }
    } else if ketone_locs.len() == 1 && nh.len() > 1 {
        let Some((&idx, _)) = locants.iter().find(|(_, l)| **l == ketone_locs[0]) else {
            return;
        };
        parts.hydro_operations.retain(|o| {
            !indicated
                .iter()
                .any(|i| i.locants == o.locants && i.key == o.key)
        });
        parts.indicated_hydrogens.retain(|l| !nh.contains(l));
        add_citations(
            parts,
            "indicated_hydrogen",
            &[(ketone_locs[0].clone(), idx)],
        );
        let hydro: Vec<_> = indicated
            .iter()
            .flat_map(|o| o.locants.iter().cloned().zip(o.atom_ids.iter().copied()))
            .collect();
        parts
            .hydro_operations
            .push(operation("additive_hydrogen", &hydro));
    }
}

fn add_proved_parent_hydrogens(
    mol: &Molecule,
    parts: &mut AssemblyParts,
    locants: &BTreeMap<usize, String>,
) -> Result<bool, String> {
    use super::fusion::mancude::{CompareOptions, compare_actual_parent_to_implied_parent};
    let Some(model) = parts
        .parent_hydride
        .as_ref()
        .and_then(|parent| parent.bond_model())
        .or(parts.parent_bond_model.as_ref())
    else {
        return Ok(false);
    };
    let state = parts
        .parent_hydride
        .as_ref()
        .and_then(|parent| parent.derivative_state())
        .or(parts.parent_derivative_state.as_ref())
        .cloned();
    let delta = if let Some(delta) = &parts.parent_bond_delta {
        Some(delta.clone())
    } else if let Some(state) = &state {
        Some(state.bond_delta.clone())
    } else {
        compare_actual_parent_to_implied_parent(
            mol,
            &parts.parent_atom_ids,
            model,
            &CompareOptions {
                preserve_retained_parent_state: true,
                ..Default::default()
            },
        )?
    };
    parts.parent_bond_delta = delta.clone();
    let Some(delta) = delta.filter(|delta| delta.compatible) else {
        return Ok(false);
    };
    let plan = if let Some(parent) = &parts.parent_hydride {
        parent
            .uses_fusion_plan()
            .then_some(parent.fusion_plan.as_ref())
            .flatten()
    } else {
        parts.fusion_plan.as_ref()
    };
    if plan.is_some_and(|plan| plan.audit.paired_external_pi_consumption) {
        return Ok(true);
    }
    let fusion_added_only = plan.is_some()
        && !delta.added_hydrogen_operations.is_empty()
        && delta.hydrogenated_edges.is_empty()
        && delta.additional_multiple_bond_ids.is_empty();
    parts
        .hydro_operations
        .extend(delta.added_hydrogen_operations);
    let intrinsic = !delta.intrinsic_hydro_operations.is_empty();
    parts
        .hydro_operations
        .extend(delta.intrinsic_hydro_operations);
    if intrinsic || fusion_added_only {
        return Ok(true);
    }
    if let Some(state) = &state
        && !state.hydro_operations.is_empty()
    {
        for op in &state.hydro_operations {
            let mut op = op.clone();
            op.atom_ids.sort_by_key(|idx| parse_locant(&locants[idx]));
            op.locants = op.atom_ids.iter().map(|idx| locants[idx].clone()).collect();
            parts.hydro_operations.push(op);
        }
        return Ok(true);
    }
    if delta.hydrogenated_edges.is_empty() {
        return Ok(false);
    }
    let indicated: AtomSet = parts
        .parent_hydride
        .as_ref()
        .and_then(|parent| parent.hydride_metadata.as_ref())
        .or(parts.retained_parent_metadata.as_ref())
        .into_iter()
        .flat_map(|metadata| metadata.default_indicated_h.iter())
        .filter_map(|locant| {
            locants
                .iter()
                .find(|(_, value)| *value == locant)
                .map(|(&idx, _)| idx)
        })
        .collect();
    let edges: Vec<_> = delta
        .hydrogenated_edges
        .into_iter()
        .filter(|(u, v)| !indicated.contains(u) && !indicated.contains(v))
        .collect();
    if edges.is_empty() {
        return Ok(true);
    }
    let atoms: AtomSet = edges.iter().flat_map(|&(u, v)| [u, v]).collect();
    let mut sites: Vec<_> = atoms
        .into_iter()
        .map(|idx| (locants[&idx].clone(), idx))
        .collect();
    sort_sites(&mut sites);
    let mut op = operation("additive_hydrogen", &sites);
    op.bond_ids = edges
        .iter()
        .filter_map(|&(u, v)| mol.get_bond(u, v).map(|bond| bond.idx))
        .collect();
    op.bond_ids.sort_unstable();
    parts.hydro_operations.push(op);
    Ok(true)
}

fn apply_retained_oxo_carbon_hydrogen(
    mol: &Molecule,
    parts: &mut AssemblyParts,
    locants: &BTreeMap<usize, String>,
) -> Result<bool, String> {
    let (Some(name), Some(metadata), Some(group)) = (
        &parts.retained_name,
        &parts.retained_parent_metadata,
        &parts.principal_group,
    ) else {
        return Ok(false);
    };
    if group.key != "ketone"
        || parts
            .parent_hydride
            .as_ref()
            .is_some_and(|parent| parent.is_fusion_parent())
    {
        return Ok(false);
    }
    let mut declared: Vec<_> = name_indicated_hydrogen_locants(Some(name))
        .into_iter()
        .collect();
    declared.sort_by_key(|locant| parse_locant(locant));
    let relocated = metadata.relocated_indicated_h && !metadata.default_indicated_h.is_empty();
    if !relocated && declared.is_empty() {
        return Ok(false);
    }
    let Some(template) = super::retained_fused_templates::retained_graph_templates(false)
        .iter()
        .find(|t| t.name == *name || t.output_name() == *name)
    else {
        return Ok(false);
    };
    let by_loc: BTreeMap<_, _> = locants.iter().map(|(&i, l)| (l.clone(), i)).collect();
    let Some(proof) = super::retained_derivative_hydrogen::prove_retained_oxo_carbon_hydrogen(
        mol,
        template,
        &by_loc,
        if relocated { &[] } else { &declared },
    )?
    else {
        return Ok(false);
    };
    let expected = if relocated {
        &metadata.default_indicated_h
    } else {
        &declared
    };
    let suffix_atoms: AtomSet = parts
        .parent_atom_ids
        .intersection(&group.atom_ids)
        .copied()
        .collect();
    if proof.indicated_h != *expected || proof.oxo_atom_ids != suffix_atoms {
        return Ok(false);
    }
    parts.retained_name = Some(super::retained_name_policy::render_retained_hydrogen_state(
        name,
        &declared,
        Some(&proof.indicated_h),
    ));
    parts.hydro_operations.push(proof.added_hydrogen);
    if let Some(parent) = &mut parts.parent_hydride {
        parent.parent_name = parts.retained_name.clone();
        parent.parent_bond_model = Some(proof.model.clone());
    }
    parts.parent_bond_model = Some(proof.model);
    Ok(true)
}

#[cfg(test)]
mod tests {
    #[test]
    fn bridge_oxo_composition_does_not_invent_hydrogenation() {
        assert_eq!(
            super::super::name_smiles("C12C(C=C(C3=CC=CC=C13)O2)=O").as_deref(),
            Ok("1,4-epoxynaphthalen-2(1H)-one")
        );
    }

    #[test]
    fn retained_derivatives_reject_conflicting_cited_hydrogen_states() {
        assert_eq!(
            super::super::name_smiles("COC1=C(C=CC(=C1)[N+](=O)[O-])NC2=C3C=CC=CC3=NC2=O")
                .unwrap_err(),
            "openclatura: retained hydrogen state conflicts with ring bonding"
        );
        for (smiles, expected) in [
            ("O=C1C=C2C=CC=CC2=N1", "2H-indol-2-one"),
            ("O=C1Cc2ccccc2N1", "1,3-dihydro-2H-indol-2-one"),
            ("O=C1C=Nc2ccccc21", "3H-indol-3-one"),
            ("O=C1C=CC2=CC=CC=C2N1", "quinolin-2(1H)-one"),
            ("C=C1C=C2C=CC=CC2=N1", "2-methylidene-2H-indole"),
        ] {
            assert_eq!(super::super::name_smiles(smiles).as_deref(), Ok(expected));
        }
    }

    #[test]
    fn supplied_retained_bond_model_collects_hydrogenated_edges() {
        use super::super::fusion::numbering::{parent_bond_graph, parent_bond_model};
        use super::*;
        let mol = super::super::graph_io::read_smiles("C1CCCCC1").unwrap();
        let ring: AtomSet = mol.atoms.keys().copied().collect();
        let locants: BTreeMap<_, _> = ring
            .iter()
            .enumerate()
            .map(|(i, &idx)| (idx, (i + 1).to_string()))
            .collect();
        let mut parts = AssemblyParts::new(6);
        parts.retained_name = Some("benzene".into());
        parts.parent_atom_ids = ring.clone();
        parts.parent_bond_model = Some(parent_bond_model(&parent_bond_graph(&mol, &ring)).unwrap());
        add_indicated_hydrogens(
            &mol,
            &mut parts,
            &ring.into_iter().collect::<Vec<_>>(),
            &locants,
        )
        .unwrap();
        let delta = parts.parent_bond_delta.as_ref().unwrap();
        assert!(delta.compatible);
        assert_eq!(delta.hydrogenated_edges.len(), 3);
        assert_eq!(parts.hydro_operations.len(), 1);
        assert_eq!(parts.hydro_operations[0].key, "additive_hydrogen");
        assert_eq!(
            parts.hydro_operations[0].locants,
            ["1", "2", "3", "4", "5", "6"]
        );
    }

    #[test]
    fn retained_fused_hydrogen_roles_keep_source_ordering() {
        for (smiles, expected) in [
            ("C1CC2=CC=CC=C2C1", "2,3-dihydro-1H-indene"),
            ("C1CCC2=CC=CC=C2C1", "1,2,3,4-tetrahydronaphthalene"),
            (
                "CN1C=NC2=C1C(=O)N(C(=O)N2C)C",
                "1,3,7-trimethyl-3,7-dihydro-1H-purine-2,6-dione",
            ),
            (
                "C1=NC2=C(N1)C(=O)NC(=N2)N",
                "2-amino-1,7-dihydro-6H-purin-6-one",
            ),
            (
                "COc1ccc2c(c1)Cc1cc(F)ccc1O2",
                "2-fluoro-7-methoxy-9H-xanthene",
            ),
            (
                "CC1C(=O)NC2=C(O1)C=C(C=C2)C3(CC3)O",
                "7-(1-hydroxycyclopropyl)-2-methyl-1,4-benzoxazin-3(4H)-one",
            ),
            (
                "CON1C(=C)C2=CC=CC=C2C1=O",
                "2-methoxy-3-methylidene-1H-isoindol-1(2H)-one",
            ),
        ] {
            assert_eq!(
                super::super::name_smiles(smiles).as_deref(),
                Ok(expected),
                "{smiles}"
            );
        }
    }
}
