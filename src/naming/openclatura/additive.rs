// SPDX-License-Identifier: MIT AND (MIT OR Apache-2.0)
// Origin: mixed
// Source: openclatura | src/openclatura/additive.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: ReShiki | src/naming/openclatura/additive.rs | 5f559fd482e237307a589dfcb7ea333cee0a6e13
// Source: openclatura | src/openclatura/fusion/mancude.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | src/openclatura/fusion/indicated_hydrogen.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | src/openclatura/retained_derivative_hydrogen.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Copyright (c) 2026 lamalab-org
// Copyright (c) 2026 Ameyanagi and ReShiki contributors
// Changes: Openclatura hydrogen-role collection extended by local complete-parent suffix/lambda proofs.
// Local contributions: MIT OR Apache-2.0; see NOTICE and PROVENANCE-nazuchem.toml.

//! Indicated and additive hydrogen for retained parents.
use super::{
    assembly_parts::{AssemblyParts, HydroOperation},
    assembly_utils::parse_locant,
    molecule::{AtomSet, Molecule},
    rules::{elements, retained},
};
use std::collections::{BTreeMap, BTreeSet};

type Site = (String, usize);

/// The bridge-prefix oxo composition is known to choose different added-H
/// locants for equivalent input atom orders. Keep this uncertainty on that
/// rendered route rather than all retained or additive-hydrogen parents.
// Provenance: fn has_atom_order_sensitive_bridge_hydrogen | original
// Source: ReShiki | src/naming/openclatura/additive.rs::has_atom_order_sensitive_bridge_hydrogen
pub(super) fn has_atom_order_sensitive_bridge_hydrogen(parts: &AssemblyParts) -> bool {
    parts
        .parent_hydride
        .as_ref()
        .is_some_and(|parent| parent.kind == super::ring_parent::ParentHydrideKind::BridgedFusion)
        && parts
            .principal_group
            .as_ref()
            .is_some_and(|group| group.key == "ketone")
        && parts.hydro_operations.iter().any(|operation| {
            matches!(
                operation.key.as_str(),
                "indicated_hydrogen" | "added_hydrogen"
            ) && !operation.locants.is_empty()
        })
}

// Provenance: fn parent_indicated_hydrogen_quota | adapted
// Source: openclatura | src/openclatura/additive.py::parent_indicated_hydrogen_quota
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

// Provenance: fn name_indicated_hydrogen_locants | adapted
// Source: openclatura | src/openclatura/additive.py::_name_indicated_hydrogen_locants
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

// Provenance: fn operation | adapted
// Source: openclatura | src/openclatura/additive.py::add_indicated_hydrogens
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
// Provenance: fn add_citations | adapted
// Source: openclatura | src/openclatura/additive.py::add_indicated_hydrogens
fn add_citations(parts: &mut AssemblyParts, key: &str, sites: &[Site]) {
    for site in sites {
        parts.indicated_hydrogens.push(site.0.clone());
        parts
            .hydro_operations
            .push(operation(key, std::slice::from_ref(site)));
    }
}
// Provenance: fn sort_sites | adapted
// Source: openclatura | src/openclatura/additive.py::add_indicated_hydrogens
fn sort_sites(sites: &mut [Site]) {
    sites.sort_by_key(|s| parse_locant(&s.0));
}

// Provenance: fn ring_bonds | adapted
// Source: openclatura | src/openclatura/additive.py::inline in _is_oxo_ring_site
fn ring_bonds<'a>(mol: &'a Molecule, idx: usize, ring: &AtomSet) -> Vec<&'a super::molecule::Bond> {
    mol.get_neighbors(idx)
        .iter()
        .filter(|n| ring.contains(n))
        .filter_map(|&n| mol.get_bond(idx, n))
        .collect()
}
// Provenance: fn saturated_ring_site | adapted
// Source: openclatura | src/openclatura/additive.py::_is_saturated_ring_site
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
// Provenance: fn exocyclic_double_bond_site | adapted
// Source: openclatura | src/openclatura/additive.py::_exocyclic_double_bond_site
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
// Provenance: fn oxo_ring_site | adapted
// Source: openclatura | src/openclatura/additive.py::_is_oxo_ring_site
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
// Provenance: fn respell_indicated_hydrogen | adapted
// Source: openclatura | src/openclatura/additive.py::_respell_indicated_hydrogen
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
// Provenance: fn fold_stem_hydro_prefix | adapted
// Source: openclatura | src/openclatura/additive.py::_fold_stem_hydro_prefix
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
    let atom_by_locant: BTreeMap<_, _> = locants.iter().map(|(&i, l)| (l.as_str(), i)).collect();
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
    (op.locants, op.atom_ids) = merged.into_iter().unzip();
    parts.retained_name = Some(rest.into());
}
// Provenance: fn declared_split | adapted
// Source: openclatura | src/openclatura/additive.py::_declared_indicated_hydrogen_split
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
// Provenance: fn relocate_preferred_hydrogenation | adapted
// Source: openclatura | src/openclatura/additive.py::_relocate_preferred_hydrogenation
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
// Provenance: fn add_indicated_hydrogens | adapted
// Source: openclatura | src/openclatura/additive.py::add_indicated_hydrogens
pub fn add_indicated_hydrogens(
    mol: &Molecule,
    parts: &mut AssemblyParts,
    numbered_path: &[usize],
    locants: &BTreeMap<usize, String>,
) -> Result<(), String> {
    if let Some(assembly) = parts
        .parent_hydride
        .as_ref()
        .and_then(|parent| parent.assembly.as_ref())
    {
        // P-28.2.3: cite indicated hydrogen at the front of the entire assembly.
        let mut sites: Vec<_> = assembly
            .indicated_hydrogen_atoms
            .iter()
            .map(|&atom| (locants[&atom].clone(), atom))
            .collect();
        sort_sites(&mut sites);
        add_citations(parts, "indicated_hydrogen", &sites);
        return Ok(());
    }
    collect_indicated_hydrogens(mol, parts, numbered_path, locants)?;
    validate_retained_hydrogen_sites(mol, parts)
}

// Provenance: fn validate_retained_hydrogen_sites | original
// Source: ReShiki | src/naming/openclatura/additive.rs::validate_retained_hydrogen_sites
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
        // Upstream divergence: reject cited H on unsaturated ring atoms to preserve the tautomer.
        return Err("openclatura: retained hydrogen state conflicts with ring bonding".into());
    }
    Ok(())
}

// Provenance: fn collect_indicated_hydrogens | adapted
// Source: openclatura | src/openclatura/additive.py::add_indicated_hydrogens
fn collect_indicated_hydrogens(
    mol: &Molecule,
    parts: &mut AssemblyParts,
    numbered_path: &[usize],
    locants: &BTreeMap<usize, String>,
) -> Result<(), String> {
    let ring: AtomSet = numbered_path.iter().copied().collect();
    relocate_preferred_hydrogenation(mol, parts, &ring, locants);
    if apply_suffix_hydrogens(mol, parts, locants)? {
        return Ok(());
    }
    if add_proved_parent_hydrogens(mol, parts, locants)? {
        return Ok(());
    }
    if parts.is_ring
        && (parts
            .parent_bond_orders_by_locants
            .values()
            .any(|&order| order > 1)
            || retained::cites_indicated_hydrogen(
                mol,
                numbered_path,
                parts.retained_name.as_deref(),
            ))
    {
        use super::fusion::exocyclic::neutral_lambda_oxo_bonding_number;

        // Generated HW parents can already place intrinsic H on a supported
        // lambda-oxo centre. Other saturated sites still need the complete
        // expanded-valence parent to distinguish indicated, added and hydro H.
        let unresolved_site = ring.iter().any(|&id| {
            neutral_lambda_oxo_bonding_number(mol, id).is_none()
                && retained::is_saturated_ring_site(mol, id, &ring)
        });
        if ring.iter().any(|&id| {
            let atom = &mol.atoms[&id];
            !atom.is_carbon()
                && atom.charge == 0
                && atom.element().is_some_and(|element| {
                    atom.total_h_count
                        + mol
                            .get_neighbors(id)
                            .iter()
                            .map(|&other| mol.bond_order(id, other))
                            .sum::<usize>()
                        > element.standard_valence
                })
                && (unresolved_site || neutral_lambda_oxo_bonding_number(mol, id).is_none())
        }) {
            mol.mark_name(crate::naming::NameStatus::Unverified(
                crate::naming::NameReason::LambdaParentHydrogen,
            ));
        }
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
    let metadata = parts.retained_parent_metadata.as_ref();
    let has_metadata = metadata.is_some();
    let name_states =
        !retained::cites_indicated_hydrogen(mol, numbered_path, parts.retained_name.as_deref())
            && !metadata.is_some_and(|m| m.relocated_indicated_h);
    if name_states && metadata.is_none() {
        return Ok(());
    }
    if name_states && metadata.is_some_and(|m| m.mancude_double_bonds == 0) {
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
        .map(|m| m.default_indicated_h.iter().cloned().collect())
        .unwrap_or_default();
    let pyran_parent = matches!(
        parts.retained_name.as_deref(),
        Some("pyran" | "thiopyran" | "selenopyran" | "telluropyran")
    );
    // Pyran's template saturation supports fusion hydrogen consumption;
    // the standalone parent still cites its movable carbon indicated H.
    let inherent: BTreeSet<_> = metadata
        .filter(|_| !pyran_parent)
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
                && ((pyran_parent && saturated_ring_site(mol, i, &ring))
                    || mol.atoms[&i].explicit_h_count + mol.atoms[&i].total_h_count > 0
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
    if let Some(name) = parts.retained_name.as_deref() {
        let spelled = name_indicated_hydrogen_locants(Some(name));
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
                parts.retained_name = Some(respell_indicated_hydrogen(name, &relocated));
                declared = relocated.clone();
                if defaults.is_empty() {
                    defaults = relocated;
                }
            }
        }
    }
    if let Some(name) = parts
        .retained_name
        .as_deref()
        .filter(|_| !defaults.is_empty())
    {
        let cited = name_indicated_hydrogen_locants(Some(name));
        if !cited.is_empty() && cited != defaults && cited.len() == defaults.len() {
            parts.retained_name = Some(respell_indicated_hydrogen(name, &defaults));
            declared = defaults.clone();
        }
    }
    let fusion: BTreeSet<_> = metadata
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
        // An exocyclic replacement can consume the hydrogen at an indicated
        // parent site; its citation still identifies the parent tautomer.
        if !oxo && has_metadata && !defaults.contains(&loc) && oxo_ring_site(mol, idx, &ring) {
            if !suffix_sites.contains(&idx) {
                hydro_only.push((loc, idx));
            }
            continue;
        }
        if has_metadata && !defaults.is_empty() && atom.is_carbon() && !defaults.contains(&loc) {
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
                if has_metadata && saturated_ring_site(mol, idx, &ring) {
                    hydro_only.push((loc, idx));
                }
                continue;
            }
            if !defaults.is_empty() && !defaults.contains(&loc) {
                if has_metadata
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
        let fusion_carbon = has_metadata
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
    if has_metadata && surplus_count > 0 && surplus_count % 2 == 0 {
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
    if has_metadata && oxo && (!name_states || added_needed) && surplus_count % 2 != 0 {
        candidates.extend(hydro_only.iter().cloned());
        sort_sites(&mut candidates);
    }
    if name_states && !added_needed {
        if has_metadata
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

// Provenance: fn recast_ring_ketone_hydrogens | adapted
// Source: openclatura | src/openclatura/additive.py::_recast_ring_ketone_hydrogens
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

// Provenance: fn add_proved_parent_hydrogens | adapted
// Source: openclatura | src/openclatura/additive.py::add_indicated_hydrogens
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
        .or(parts.parent_derivative_state.as_ref());
    let delta = if let Some(delta) = &parts.parent_bond_delta {
        Some(delta.clone())
    } else if let Some(state) = state {
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
    if let Some(state) = state
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

// P-58.2.3.1.1: relocate intrinsic indicated H to suffix sites before
// assigning residual hydro prefixes, preserving the parent's hydrogen quota.
// Provenance: fn suffix_hydrogen_citations | original
// Source: ReShiki | src/naming/openclatura/additive.rs::suffix_hydrogen_citations
fn suffix_hydrogen_citations(
    mol: &Molecule,
    atoms: &AtomSet,
    locants: &BTreeMap<usize, String>,
    suffix: &BTreeSet<String>,
    seeds: impl IntoIterator<Item = Vec<String>>,
) -> Vec<Vec<String>> {
    let mut citations: BTreeSet<BTreeSet<String>> = seeds
        .into_iter()
        .map(|sites| sites.into_iter().collect())
        .collect();
    let mut pending: Vec<_> = citations.iter().cloned().collect();
    while let Some(cited) = pending.pop() {
        for old in cited.difference(suffix) {
            for new in suffix.difference(&cited) {
                let moved: BTreeSet<_> = cited
                    .iter()
                    .filter(|l| *l != old)
                    .cloned()
                    .chain([new.clone()])
                    .collect();
                if citations.insert(moved.clone()) {
                    pending.push(moved);
                }
            }
        }
    }
    let by_loc: BTreeMap<_, _> = locants.iter().map(|(&id, l)| (l, id)).collect();
    citations
        .into_iter()
        .filter(|sites| {
            sites.iter().all(|l| {
                by_loc
                    .get(l)
                    .is_some_and(|&id| ring_bonds(mol, id, atoms).iter().all(|b| b.order == 1))
            })
        })
        .map(|sites| {
            let mut sites: Vec<_> = sites.into_iter().collect();
            sites.sort_by_key(|l| parse_locant(l));
            sites
        })
        .collect()
}

// Provenance: fn suffix_hydrogen_rank | adapted
// Source: openclatura | src/openclatura/fusion/mancude.py::_external_pi_parent_delta
// Source: openclatura | src/openclatura/fusion/indicated_hydrogen.py::intrinsic_carbon_parent_model
fn suffix_hydrogen_rank(
    indicated: &[String],
    suffix: &BTreeSet<String>,
    suffix_count: usize,
    fusion: &[String],
    state: &super::fusion::mancude::ParentDerivativeState,
) -> impl Ord + use<> {
    // P-58.2.3.1.3: if suffix groups outnumber intrinsic H, first reserve
    // the lowest peripheral indicated-H site.
    let lowest_indicated = if !indicated.is_empty() && indicated.len() < suffix_count {
        let lowest = indicated
            .iter()
            .filter(|l| !fusion.contains(*l))
            .map(|l| parse_locant(l))
            .min();
        (lowest.is_none(), lowest)
    } else {
        (false, None)
    };
    let hydro: Vec<_> = state
        .hydro_operations
        .iter()
        .flat_map(|op| op.locants.iter().map(|l| parse_locant(l)))
        .collect();
    (
        lowest_indicated,
        std::cmp::Reverse(indicated.iter().filter(|l| suffix.contains(*l)).count()),
        indicated
            .iter()
            .map(|l| parse_locant(l))
            .collect::<Vec<_>>(),
        hydro.len(),
        state
            .bond_delta
            .added_hydrogen_operations
            .iter()
            .flat_map(|op| op.locants.iter().map(|l| parse_locant(l)))
            .collect::<Vec<_>>(),
        hydro,
    )
}

type SuffixHydrogenState = (
    Vec<String>,
    super::fusion::model::ParentBondModel,
    super::fusion::mancude::ParentDerivativeState,
);

// Provenance: fn systematic_suffix_hydrogen_state | adapted
// Source: openclatura | src/openclatura/additive.py::parent_indicated_hydrogen_quota
pub(super) fn systematic_suffix_hydrogen_state(
    mol: &Molecule,
    plan: &super::fusion::model::FusionParentPlan,
    suffix: &AtomSet,
    locants: &BTreeMap<usize, String>,
) -> Result<Option<SuffixHydrogenState>, String> {
    use super::fusion::{mancude::spiro_carbon_sites, numbering};
    let atoms: AtomSet = locants.keys().copied().collect();
    if suffix.is_empty()
        || !suffix.is_subset(&atoms)
        || atoms.iter().any(|id| mol.atoms[id].charge != 0)
        || !spiro_carbon_sites(mol, &atoms).is_empty()
    {
        return Ok(None);
    }
    let model = numbering::parent_bond_model(&plan.abstract_parent_graph)?;
    let capable = parent_pi_atoms(&model);
    let quota = capable
        .len()
        .saturating_sub(2 * model.maximum_non_cumulative_double_bonds);
    // The abstract graph may relax nitrogen lone-pair roles. Extend only
    // the intrinsic carbon-H quota already proved by the fusion plan.
    let carbon_h = locants
        .iter()
        .filter(|(id, l)| mol.atoms[id].symbol == "C" && plan.indicated_hydrogens.contains(l))
        .count();
    if quota > carbon_h {
        return Ok(None);
    }
    graph_suffix_hydrogen_state(
        mol,
        &plan.abstract_parent_graph,
        suffix,
        suffix,
        locants,
        None,
    )
}

// Provenance: fn parent_pi_atoms | adapted
// Source: openclatura | src/openclatura/fusion/mancude.py::_external_pi_parent_delta
// Source: openclatura | src/openclatura/fusion/indicated_hydrogen.py::intrinsic_carbon_parent_model
fn parent_pi_atoms(model: &super::fusion::model::ParentBondModel) -> AtomSet {
    // Atoms that can never carry a parent double bond are inherently
    // saturated, not movable indicated-H sites (e.g. carbon between two O).
    model
        .allowed_kekule_assignments
        .iter()
        .flat_map(|assignment| {
            assignment
                .orders
                .iter()
                .filter(|(_, order)| *order == 2)
                .flat_map(|(edge, _)| [edge.0, edge.1])
        })
        .collect()
}

// Provenance: fn graph_suffix_hydrogen_state | adapted
// Source: openclatura | src/openclatura/fusion/indicated_hydrogen.py::intrinsic_carbon_parent_model
// Source: openclatura | src/openclatura/fusion/mancude.py::indicated_hydrogen_parent_bond_model
// Source: openclatura | src/openclatura/additive.py::parent_indicated_hydrogen_quota
pub(super) fn graph_suffix_hydrogen_state(
    mol: &Molecule,
    graph: &super::fusion::model::FusionGraph,
    suffix: &AtomSet,
    free_valences: &AtomSet,
    locants: &BTreeMap<usize, String>,
    spiro_junction: Option<usize>,
) -> Result<Option<SuffixHydrogenState>, String> {
    use super::fusion::{mancude::suffix_hydrogen_state, numbering};
    let atoms: AtomSet = locants.keys().copied().collect();
    let suffix_locants: BTreeSet<_> = suffix
        .union(free_valences)
        .map(|a| locants[a].clone())
        .collect();
    let fusion: Vec<_> = locants
        .iter()
        .filter(|(id, _)| ring_bonds(mol, **id, &atoms).len() > 2)
        .map(|(_, l)| l.clone())
        .collect();
    let model = numbering::parent_bond_model(graph)?;
    let capable = if let Some(junction) = spiro_junction {
        // A site isolated from pi bonding by the junction still needs indicated
        // H (benzofuran C2 next to spiro C3). Inherently saturated sites, such
        // as carbon between two oxygens, remain outside the parent pi set.
        let mut component = graph.clone();
        for atom in &mut component.atoms {
            if atom.id == junction {
                atom.forced_single = false;
            }
        }
        let mut capable = parent_pi_atoms(&numbering::parent_bond_model(&component)?);
        capable.remove(&junction);
        capable
    } else {
        parent_pi_atoms(&model)
    };
    let quota = capable
        .len()
        .saturating_sub(2 * model.maximum_non_cumulative_double_bonds);
    let citations = model.allowed_kekule_assignments.iter().map(|assignment| {
        let paired: AtomSet = assignment
            .orders
            .iter()
            .filter(|(_, order)| *order == 2)
            .flat_map(|(edge, _)| [edge.0, edge.1])
            .collect();
        capable
            .difference(&paired)
            .map(|id| locants[id].clone())
            .collect()
    });
    let mut candidates = Vec::new();
    for indicated in suffix_hydrogen_citations(mol, &atoms, locants, &suffix_locants, citations) {
        if indicated.len() != quota
            || indicated
                .iter()
                .any(|l| !capable.iter().any(|id| locants[id] == *l))
        {
            continue;
        }
        let mut graph = graph.clone();
        for atom in &mut graph.atoms {
            atom.forced_single |= indicated.contains(&locants[&atom.id]);
        }
        let constrained = numbering::parent_bond_model(&graph)?;
        if constrained.maximum_non_cumulative_double_bonds
            != model.maximum_non_cumulative_double_bonds
        {
            continue;
        }
        let Some(state) = suffix_hydrogen_state(mol, &atoms, &constrained, suffix, locants)? else {
            continue;
        };
        let rank = suffix_hydrogen_rank(
            &indicated,
            &suffix_locants,
            suffix_locants.len(),
            &fusion,
            &state,
        );
        candidates.push((rank, indicated, constrained, state));
    }
    Ok(candidates
        .into_iter()
        .min_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, indicated, model, state)| (indicated, model, state)))
}

// Provenance: fn apply_parent_hydrogen_state | adapted
// Source: openclatura | src/openclatura/additive.py::_apply_retained_oxo_carbon_hydrogen
// Source: openclatura | src/openclatura/additive.py::add_indicated_hydrogens
fn apply_parent_hydrogen_state(
    parts: &mut AssemblyParts,
    indicated: Vec<String>,
    model: super::fusion::model::ParentBondModel,
    state: super::fusion::mancude::ParentDerivativeState,
) {
    parts
        .hydro_operations
        .extend(state.hydro_operations.clone());
    parts
        .hydro_operations
        .extend(state.bond_delta.added_hydrogen_operations.clone());
    for plan in parts
        .parent_hydride
        .iter_mut()
        .flat_map(|p| p.fusion_plan.iter_mut())
        .chain(parts.fusion_plan.iter_mut())
    {
        plan.rendered_base_name = super::retained_name_policy::render_retained_hydrogen_state(
            &plan.rendered_base_name,
            &plan.indicated_hydrogens,
            Some(&indicated),
        );
        plan.indicated_hydrogens = indicated.clone();
        plan.bond_model = model.clone();
        plan.derivative_state = state.clone();
    }
    for metadata in parts.retained_parent_metadata.iter_mut().chain(
        parts
            .parent_hydride
            .iter_mut()
            .flat_map(|p| p.hydride_metadata.iter_mut()),
    ) {
        metadata.indicated_hydrogen_count = indicated.len();
        metadata.relocated_indicated_h = metadata.default_indicated_h != indicated;
        metadata.default_indicated_h = indicated.clone();
        metadata.mancude_double_bonds = model.maximum_non_cumulative_double_bonds;
    }
    if let Some(parent) = &mut parts.parent_hydride {
        parent.parent_bond_model = Some(model.clone());
    }
    parts.parent_bond_model = Some(model);
    parts.parent_bond_delta = Some(state.bond_delta.clone());
    parts.parent_derivative_state = Some(state);
}

// Provenance: fn apply_lambda_parent_hydrogens | original
// Source: ReShiki | src/naming/openclatura/additive.rs::apply_lambda_parent_hydrogens
fn apply_lambda_parent_hydrogens(
    mol: &Molecule,
    parts: &mut AssemblyParts,
    locants: &BTreeMap<usize, String>,
) -> Result<bool, String> {
    use super::fusion::{exocyclic::neutral_lambda_oxo_bonding_number, numbering};
    if parts
        .retained_parent_metadata
        .as_ref()
        .is_none_or(|m| m.mancude_double_bonds == 0)
        || !parts
            .parent_atom_ids
            .iter()
            .any(|&id| neutral_lambda_oxo_bonding_number(mol, id).is_some())
    {
        return Ok(false);
    }
    // P-25.6 / P-25.7.1.1, Lm-3.1–3.2: complete the entire mancude
    // lambda parent before assigning indicated H, suffix H and hydro prefixes.
    let graph = numbering::parent_bond_graph(mol, &parts.parent_atom_ids);
    let mut suffix: AtomSet = parts
        .principal_group
        .as_ref()
        .filter(|group| group.key == "ketone")
        .map(|group| {
            group
                .atom_ids
                .intersection(&parts.parent_atom_ids)
                .copied()
                .collect()
        })
        .unwrap_or_default();
    let free_valences: AtomSet = if parts.is_substituent {
        AtomSet::from([parts.parent_atom_ids_by_locant[&parts.attachment_locant]])
    } else {
        AtomSet::new()
    };
    if parts.attachment_order == 2 {
        suffix.extend(&free_valences);
    }
    let Some((indicated, model, state)) =
        graph_suffix_hydrogen_state(mol, &graph, &suffix, &free_valences, locants, None)?
    else {
        return Err("openclatura: lambda parent hydrogen state not proved".into());
    };
    if let Some(name) = &mut parts.retained_name {
        let declared = name_indicated_hydrogen_locants(Some(name))
            .into_iter()
            .collect::<Vec<_>>();
        *name = super::retained_name_policy::render_retained_hydrogen_state(
            name,
            &declared,
            Some(&indicated),
        );
    }
    if let Some(parent) = &mut parts.parent_hydride {
        parent.parent_name = parts.retained_name.clone();
    }
    for plan in parts
        .parent_hydride
        .iter_mut()
        .flat_map(|parent| parent.fusion_plan.iter_mut())
        .chain(parts.fusion_plan.iter_mut())
    {
        plan.abstract_parent_graph = graph.clone();
    }
    apply_parent_hydrogen_state(parts, indicated, model, state);
    Ok(true)
}

// Provenance: fn retained_suffix_hydrogen_state | adapted
// Source: openclatura | src/openclatura/retained_derivative_hydrogen.py::prove_retained_oxo_carbon_hydrogen
pub(super) fn retained_suffix_hydrogen_state(
    mol: &Molecule,
    template: &super::retained_graph_model::RetainedGraphTemplate,
    atoms: &AtomSet,
    suffix: &AtomSet,
    locants: &BTreeMap<usize, String>,
    indicated_seed: &[String],
) -> Result<Option<SuffixHydrogenState>, String> {
    use super::fusion::{mancude::suffix_hydrogen_state, numbering};
    let by_loc = locants.iter().map(|(&id, l)| (l.clone(), id)).collect();
    let suffix_locants: BTreeSet<_> = suffix.iter().map(|a| locants[a].clone()).collect();
    let mut citations = vec![template.default_indicated_h.clone()];
    if indicated_seed.len() == template.indicated_hydrogen_count() {
        citations.push(indicated_seed.to_vec());
    }
    let mut candidates = Vec::new();
    for indicated in suffix_hydrogen_citations(mol, atoms, locants, &suffix_locants, citations) {
        if indicated.len() != template.indicated_hydrogen_count() {
            continue;
        }
        let model = match numbering::retained_template_parent_bond_model(
            template,
            &by_loc,
            Some(&indicated),
        ) {
            Ok(model) => model,
            Err(error) if error == numbering::RETAINED_PARENT_BOND_CAPACITY_ERROR => continue,
            Err(error) => return Err(error),
        };
        let Some(state) = suffix_hydrogen_state(mol, atoms, &model, suffix, locants)? else {
            continue;
        };
        let rank = suffix_hydrogen_rank(
            &indicated,
            &suffix_locants,
            suffix.len(),
            &template.fusion_atoms,
            &state,
        );
        candidates.push((rank, indicated, model, state));
    }
    Ok(candidates
        .into_iter()
        .min_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, indicated, model, state)| (indicated, model, state)))
}

// Provenance: fn apply_suffix_hydrogens | adapted
// Source: openclatura | src/openclatura/additive.py::_apply_retained_oxo_carbon_hydrogen
fn apply_suffix_hydrogens(
    mol: &Molecule,
    parts: &mut AssemblyParts,
    locants: &BTreeMap<usize, String>,
) -> Result<bool, String> {
    use super::graph_kernel::normalize_edge;

    // Spiro components leave suffix composition to the complete spiro parent.
    if parts.is_spiro_side_projection || parts.substituents.iter().any(|s| s.spiro.is_some()) {
        return Ok(false);
    }
    // Charge operations have their own parent-hydride composition.
    if parts
        .parent_atom_ids
        .iter()
        .any(|id| mol.atoms[id].charge != 0)
    {
        return Ok(false);
    }
    if apply_lambda_parent_hydrogens(mol, parts, locants)? {
        return Ok(true);
    }
    let Some(group) = parts.principal_group.as_ref().filter(|g| g.key == "ketone") else {
        return Ok(false);
    };
    let atoms = &parts.parent_atom_ids;
    let suffix: AtomSet = group.atom_ids.intersection(atoms).copied().collect();
    if suffix.is_empty() {
        return Ok(false);
    }
    let suffix_locants: BTreeSet<_> = suffix.iter().map(|a| locants[a].clone()).collect();
    if let Some(plan) = parts
        .parent_hydride
        .as_ref()
        .filter(|p| p.is_systematic_fusion())
        .and_then(|p| p.fusion_plan.as_ref())
    {
        let Some((indicated, model, state)) =
            systematic_suffix_hydrogen_state(mol, plan, &suffix, locants)?
        else {
            return Ok(false);
        };
        apply_parent_hydrogen_state(parts, indicated, model, state);
        return Ok(true);
    }
    if parts
        .retained_parent_metadata
        .as_ref()
        .is_none_or(|m| m.fusion_locants.is_empty())
        || parts
            .parent_hydride
            .as_ref()
            .is_some_and(|p| p.is_fusion_parent())
    {
        return Ok(false);
    }
    let by_loc: BTreeMap<_, _> = locants.iter().map(|(&i, l)| (l.clone(), i)).collect();
    let edges: BTreeSet<_> = mol
        .bonds
        .values()
        .filter(|b| atoms.contains(&b.u) && atoms.contains(&b.v))
        .map(|b| normalize_edge(b.u, b.v))
        .collect();
    let mut candidates = Vec::new();
    for template in super::retained_fused_templates::retained_graph_templates(false) {
        let Some(capacity) = template.mancude_double_bonds else {
            continue;
        };
        if template.family != "fused"
            || template.locants.iter().collect::<BTreeSet<_>>() != by_loc.keys().collect()
            || template.atoms.iter().any(|a| {
                let observed = &mol.atoms[&by_loc[&a.locant]];
                a.symbol != observed.symbol || a.charge != observed.charge
            })
            || template
                .bonds
                .iter()
                .map(|b| normalize_edge(by_loc[&b.locants.0], by_loc[&b.locants.1]))
                .collect::<BTreeSet<_>>()
                != edges
        {
            continue;
        }
        let seed = parts
            .retained_parent_metadata
            .as_ref()
            .map_or(&[][..], |metadata| metadata.default_indicated_h.as_slice());
        let Some((indicated, model, state)) =
            retained_suffix_hydrogen_state(mol, template, atoms, &suffix, locants, seed)?
        else {
            continue;
        };
        let rank = (
            std::cmp::Reverse(capacity),
            suffix_hydrogen_rank(
                &indicated,
                &suffix_locants,
                group.locants.len(),
                &template.fusion_atoms,
                &state,
            ),
            template.priority,
        );
        candidates.push((rank, template, indicated, model, state));
    }
    let Some((_, template, indicated, model, state)) =
        candidates.into_iter().min_by(|a, b| a.0.cmp(&b.0))
    else {
        return Ok(false);
    };
    parts.retained_name = Some(super::retained_name_policy::retained_parent_output_name(
        &template.name,
        "composite_parent",
        &template.default_indicated_h,
        Some(&indicated),
    ));
    let mut metadata = template.metadata();
    metadata.default_indicated_h = indicated.clone();
    metadata.relocated_indicated_h = indicated != template.default_indicated_h;
    metadata.inherent_saturated_locants.clear();
    parts.retained_parent_metadata = Some(metadata.clone());
    let declared = name_indicated_hydrogen_locants(parts.retained_name.as_deref());
    let sites: Vec<_> = indicated
        .iter()
        .filter(|l| !declared.contains(*l))
        .map(|l| (l.clone(), by_loc[l]))
        .collect();
    add_citations(parts, "indicated_hydrogen", &sites);
    parts
        .hydro_operations
        .extend(state.hydro_operations.clone());
    parts
        .hydro_operations
        .extend(state.bond_delta.added_hydrogen_operations.clone());
    parts.parent_bond_delta = Some(state.bond_delta.clone());
    parts.parent_derivative_state = Some(state);
    if let Some(parent) = &mut parts.parent_hydride {
        parent.parent_name = parts.retained_name.clone();
        parent.hydride_metadata = Some(metadata);
        parent.parent_bond_model = Some(model.clone());
    }
    parts.parent_bond_model = Some(model);
    Ok(true)
}

#[cfg(test)]
// Provenance: mod tests | original
// Source: ReShiki | src/naming/openclatura/additive.rs::tests
mod tests {
    #[test]
    fn ring_ketone_numbering_compares_suffix_before_added_hydrogen() {
        use crate::naming::{NameStatus, verify_identity};
        let parser = opsin::Parser::new().unwrap();
        for (smiles, expected) in [
            ("O=C1C(=O)Cc2ccccc21", "1H-indene-1,2(3H)-dione"),
            (
                "CC1(C)C(=O)C(=O)c2ccccc21",
                "3,3-dimethyl-1H-indene-1,2(3H)-dione",
            ),
            (
                "O=C1C(=O)Cc2ccc(Cl)cc21",
                "6-chloro-1H-indene-1,2(3H)-dione",
            ),
            ("O=C1Cc2ccccc2C1=O", "1H-indene-1,2(3H)-dione"),
            ("C1C(=O)C(=O)c2ccccc12", "1H-indene-1,2(3H)-dione"),
            (
                "O=C1C(=O)C(C)(C)c2ccccc12",
                "3,3-dimethyl-1H-indene-1,2(3H)-dione",
            ),
            ("O=C1CC(=O)c2ccccc21", "1H-indene-1,3(2H)-dione"),
            (
                "O=C1CC(=O)c2ccc(Cl)cc21",
                "5-chloro-1H-indene-1,3(2H)-dione",
            ),
            (
                "O=C1CC(=O)c2c1ccc1ccccc21",
                "1H-cyclopenta[a]naphthalene-1,3(2H)-dione",
            ),
            (
                "C1C(=O)c2c(C1=O)c1ccccc1cc2",
                "1H-cyclopenta[a]naphthalene-1,3(2H)-dione",
            ),
            ("O=C1Cc2ccccc2C1", "1,3-dihydro-2H-inden-2-one"),
            ("O=C1Nc2ccccc2C1=O", "1H-indole-2,3-dione"),
            ("O=C1CC(=O)C(=O)c2ccccc12", "naphthalene-1,2,4(3H)-trione"),
            ("O=C1C(=O)CCc2ccccc21", "3,4-dihydronaphthalene-1,2-dione"),
            ("O=C1CCc2ccccc12", "2,3-dihydro-1H-inden-1-one"),
            ("O=C1Cc2ccccc2C(=O)C1", "naphthalene-1,3(2H,4H)-dione"),
            ("O=C1C=Cc2ccccc12", "1H-inden-1-one"),
        ] {
            let name = super::super::name_smiles(smiles, &Default::default()).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            assert_eq!(name.status, NameStatus::Preferred, "{smiles}");
            let parsed = parser.parse(&name.text, &opsin::ParseOptions::strict());
            assert_eq!(
                parsed.status,
                opsin::Status::Success,
                "{}: {}",
                name.text,
                parsed.message
            );
            let reconstructed = parsed.structure.unwrap().semantic_cxsmiles().unwrap();
            verify_identity(smiles, &reconstructed)
                .unwrap_or_else(|error| panic!("{smiles} -> {}: {error}", name.text));
        }
    }

    #[test]
    fn suffix_hydrogen_names_are_preferred_and_round_trip() {
        use crate::naming::{NameStatus, verify_identity};
        let parser = opsin::Parser::new().unwrap();
        let mut failures = Vec::new();
        for (smiles, expected) in [
            (
                "CN1C(=O)CN=C(C2=C1C=CC(=C2)Cl)C3=CC=CC=C3",
                "7-chloro-1-methyl-5-phenyl-1H-1,4-benzodiazepin-2(3H)-one",
            ),
            (
                "CCCC1=NN(C2=C1N=C(NC2=O)C3=C(C=CC(=C3)S(=O)(=O)N4CCN(CC4)C)OCC)C",
                "5-[2-ethoxy-5-(4-methylpiperazine-1-sulfonyl)phenyl]-1-methyl-3-propyl-6H-pyrazolo[4,3-d]pyrimidin-7(1H)-one",
            ),
            (
                "O=c1c2ccccc2nc2n1CC=C2",
                "pyrrolo[2,1-b]quinazolin-9(1H)-one",
            ),
            (
                "C1=CC=C2C(=C1)C(=O)N(N=N2)CSC3=CC(=C(C=C3)F)F",
                "3-{[(3,4-difluorophenyl)sulfanyl]methyl}-1,2,3-benzotriazin-4(3H)-one",
            ),
            (
                "CN1N=Nc2ccccc2C1=O",
                "3-methyl-1,2,3-benzotriazin-4(3H)-one",
            ),
            ("O=c1[nH]nnc2ccccc12", "1,2,3-benzotriazin-4(3H)-one"),
            ("O=C1C=CC2=C1NC=C2", "cyclopenta[b]pyrrol-6(1H)-one"),
            (
                "C1Cc2cnccc2C1=O",
                "6,7-dihydro-5H-cyclopenta[c]pyridin-5-one",
            ),
            (
                "C1Cc2nccnc2C1=O",
                "6,7-dihydro-5H-cyclopenta[b]pyrazin-5-one",
            ),
            (
                "C1C(C)c2ncccc2C1=O",
                "7-methyl-6,7-dihydro-5H-cyclopenta[b]pyridin-5-one",
            ),
            ("O=C1CCCc2ncccc12", "7,8-dihydroquinolin-5(6H)-one"),
            ("O=C1NC2=C(O1)C=CO2", "furo[2,3-d][1,3]oxazol-2(3H)-one"),
            ("O=C1NC2=C(OC=C2)O1", "furo[3,2-d][1,3]oxazol-2(1H)-one"),
            ("O=C1OC2=C(O1)C=CN2", "4H-[1,3]dioxolo[4,5-b]pyrrol-2-one"),
            (
                "N1=CC=CC2=CC=C3C(NC(CC3=C12)=O)=O",
                "1,8-phenanthroline-7,9(8H,10H)-dione",
            ),
            (
                "COC1=C(C=CC(=C1)[N+](=O)[O-])NC2=C3C=CC=CC3=NC2=O",
                "3-[(2-methoxy-4-nitrophenyl)amino]-2H-indol-2-one",
            ),
            (
                "CC1C(=O)NC2=C(O1)C=C(C=C2)C3(CC3)O",
                "7-(1-hydroxycyclopropyl)-2-methyl-2H-1,4-benzoxazin-3(4H)-one",
            ),
            (
                "CCCCCN1C(=O)COC2=C1C=C(C(=C2)F)N3C(=O)C4=CC=CC=C4N=N3",
                "7-fluoro-6-(4-oxo-3,4-dihydro-1,2,3-benzotriazin-3-yl)-4-pentyl-2H-1,4-benzoxazin-3(4H)-one",
            ),
            ("O=C1NC(=O)c2ccccc12", "1H-isoindole-1,3(2H)-dione"),
            ("N1C(N=C2N=CN=C2C1=O)=O", "1H-purine-2,6-dione"),
            (
                "CN1C(=O)c2ccccc2C1=O",
                "2-methyl-1H-isoindole-1,3(2H)-dione",
            ),
            (
                "O=C1c2ccccc2C(=O)N1O",
                "2-hydroxy-1H-isoindole-1,3(2H)-dione",
            ),
            ("O=C1CCCc2ccccc12", "3,4-dihydronaphthalen-1(2H)-one"),
            // P-58.2.2.3 / P-58.2.5: pair suffix deficits before
            // citing the remaining reduction with hydro prefixes.
            ("O=C1CCC(=O)c2ccccc21", "2,3-dihydronaphthalene-1,4-dione"),
            ("O=C1CC(=O)Cc2ccccc21", "naphthalene-1,3(2H,4H)-dione"),
            ("O=C1C(=O)C(=O)Cc2ccccc21", "naphthalene-1,2,3(4H)-trione"),
            ("O=C1CC(=O)c2ccccc21", "1H-indene-1,3(2H)-dione"),
            ("O=C1C(=O)C=Cc2ccccc21", "naphthalene-1,2-dione"),
            // P-16.7.1(c): a numerical multiplier loses terminal a
            // only before a suffix beginning with a or o.
            ("O=C1C(=O)C(=O)C(=O)CC1", "cyclohexane-1,2,3,4-tetrone"),
            ("OCC(O)C(O)CO", "butane-1,2,3,4-tetrol"),
            ("NCC(N)C(N)CN", "butane-1,2,3,4-tetramine"),
            ("O=C1CCc2ccccc2C1", "3,4-dihydronaphthalen-2(1H)-one"),
            ("O=C1C=CCc2ccccc12", "naphthalen-1(4H)-one"),
            ("O=C1C=CC(=N)c2ccccc21", "4-iminonaphthalen-1(4H)-one"),
            (
                "CON1C(=C)C2=CC=CC=C2C1=O",
                "2-methoxy-3-methylidene-2,3-dihydro-1H-isoindol-1-one",
            ),
            (
                "CC1=C2C(=CC=C1)C3=C(N2)N=C(N=N3)NN=C4C5=C(C=CC(=C5)[N+](=O)[O-])N(C4=O)C",
                "1-methyl-3-{[(6-methyl-5H-[1,2,4]triazino[5,6-b]indol-3-yl)amino]imino}-5-nitro-1,3-dihydro-2H-indol-2-one",
            ),
            ("O=C1CCc2ccccc2N1", "3,4-dihydroquinolin-2(1H)-one"),
            ("O=C1CCc2ccccc12", "2,3-dihydro-1H-inden-1-one"),
            ("O=C1CCOc2ccccc12", "2,3-dihydro-4H-1-benzopyran-4-one"),
            ("O=c1cccc[nH]1", "pyridin-2(1H)-one"),
            ("Cn1ccccc1=O", "1-methylpyridin-2(1H)-one"),
            ("O=c1ccc2ccccc2[nH]1", "quinolin-2(1H)-one"),
            ("Cn1c(=O)ccc2ccccc21", "1-methylquinolin-2(1H)-one"),
            ("O=c1cc[nH]c(=O)[nH]1", "pyrimidine-2,4(1H,3H)-dione"),
            ("C1C=Cc2ccccc21", "1H-indene"),
            ("O=C1c2ccccc2-c2ccccc21", "9H-fluoren-9-one"),
            ("O=C1CCCCC1", "cyclohexanone"),
            ("O=C1CCCCN1", "piperidin-2-one"),
            ("C1CCc2ccccc2C1", "1,2,3,4-tetrahydronaphthalene"),
            ("OC1CCCc2ccccc12", "1,2,3,4-tetrahydronaphthalen-1-ol"),
        ] {
            let name = super::super::name_smiles(smiles, &Default::default()).unwrap();
            if name.text != expected {
                failures.push(format!("{smiles}: {} -> {expected}", name.text));
            }
            assert_eq!(name.status, NameStatus::Preferred, "{smiles}");
            let parsed = parser.parse(&name.text, &opsin::ParseOptions::strict());
            assert_eq!(
                parsed.status,
                opsin::Status::Success,
                "{}: {}",
                name.text,
                parsed.message
            );
            let reconstructed = parsed.structure.unwrap().semantic_cxsmiles().unwrap();
            verify_identity(smiles, &reconstructed)
                .unwrap_or_else(|error| panic!("{smiles} -> {}: {error}", name.text));
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn projected_spiro_components_keep_unverified_hydrogen_fallback() {
        let smiles = "CC1C2C(C(=O)N(C2=O)C3=CC=CC=C3OC)C4(N1)C5=CC=CC=C5N(C4=O)CCBr";
        let name = super::super::name_smiles(smiles, &Default::default()).unwrap();
        assert_eq!(
            name.text,
            "1-(2-bromoethyl)-5'-(2-methoxyphenyl)-3'-methyl-2-oxospiro[2,3-dihydro-1H-indole-3,1'-2H-pyrrolo[3,4-c]pyrrole]-4',6'(3'H,3a'H,6a'H)-dione"
        );
        assert_eq!(
            name.status,
            crate::naming::NameStatus::Unverified(crate::naming::NameReason::SpiroHydrogen)
        );
        let parsed = opsin::Parser::new()
            .unwrap()
            .parse(&name.text, &opsin::ParseOptions::strict());
        assert_eq!(parsed.status, opsin::Status::Success, "{}", parsed.message);
        let reconstructed = parsed.structure.unwrap().semantic_cxsmiles().unwrap();
        crate::naming::verify_identity(smiles, &reconstructed).unwrap();
    }

    #[test]
    fn bridge_oxo_composition_does_not_invent_hydrogenation() {
        assert_eq!(
            super::super::name_smiles("C12C(C=C(C3=CC=CC=C13)O2)=O", &Default::default())
                .map(|name| name.text)
                .as_deref(),
            Ok("1,4-epoxynaphthalen-2(1H)-one")
        );
    }

    #[test]
    fn retained_derivatives_use_bond_consistent_hydrogen_states() {
        for (smiles, expected) in [
            // The generic external-pi fallback does not own suffix or
            // intrinsic-H allocation; keep its existing composition state.
            (
                "C[C@]12CC[C@H]3[C@H]([C@@H]1CC[C@@H]2O)CCC4=CC(=O)CC[C@]34C",
                "(1S,3aS,3bR,9aR,9bS,11aS)-1-hydroxy-9a,11a-dimethyl-2,3,3a,3b,4,5,9,9a,9b,10,11,11a-dodecahydrocyclopenta[a]phenanthren-7(1H,8H)-one",
            ),
            ("O=C1C=C2C=CC=CC2=N1", "2H-indol-2-one"),
            ("O=C1Cc2ccccc2N1", "1,3-dihydro-2H-indol-2-one"),
            ("O=C1C=Nc2ccccc21", "3H-indol-3-one"),
            ("O=C1C=CC2=CC=CC=C2N1", "quinolin-2(1H)-one"),
            ("C=C1C=C2C=CC=CC2=N1", "2-methylidene-2H-indole"),
        ] {
            assert_eq!(
                super::super::name_smiles(smiles, &Default::default())
                    .map(|name| name.text)
                    .as_deref(),
                Ok(expected)
            );
        }
    }

    #[test]
    fn supplied_retained_bond_model_collects_hydrogenated_edges() {
        use super::super::fusion::numbering::{parent_bond_graph, parent_bond_model};
        use super::*;
        let mol = super::super::graph_io::read_smiles("C1CCCCC1", &Default::default()).unwrap();
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
                "7-(1-hydroxycyclopropyl)-2-methyl-2H-1,4-benzoxazin-3(4H)-one",
            ),
            (
                "CON1C(=C)C2=CC=CC=C2C1=O",
                "2-methoxy-3-methylidene-2,3-dihydro-1H-isoindol-1-one",
            ),
        ] {
            assert_eq!(
                super::super::name_smiles(smiles, &Default::default())
                    .map(|name| name.text)
                    .as_deref(),
                Ok(expected),
                "{smiles}"
            );
        }
    }

    #[test]
    fn xanthene_congeners_retain_indicated_hydrogen_and_oxo_locants() {
        let parser = opsin::Parser::new().unwrap();
        for (smiles, expected) in [
            ("S1c2ccccc2Cc2ccccc21", "9H-thioxanthene"),
            (
                "C1=CC(=CC=2SC3=CC=CC=C3CC12)C(=O)O",
                "9H-thioxanthene-3-carboxylic acid",
            ),
            ("O=C1c2ccccc2Sc2ccccc12", "9H-thioxanthen-9-one"),
            ("[Se]1c2ccccc2Cc2ccccc21", "9H-selenoxanthene"),
            ("[Te]1c2ccccc2Cc2ccccc21", "9H-telluroxanthene"),
            (
                "O=S1c2ccccc2Cc2ccccc21",
                "10lambda^4-thioxanthen-10(9H)-one",
            ),
            (
                "O=S1(=O)c2ccccc2Cc2ccccc21",
                "10lambda^6-thioxanthene-10,10(9H)-dione",
            ),
            (
                "O=[Se]1c2ccccc2Cc2ccccc21",
                "10lambda^4-selenoxanthen-10(9H)-one",
            ),
            (
                "O=C1C2=CC=CC=C2S(C=2C=C(C=CC12)C(=O)O)(=O)=O",
                "9,10,10-trioxo-9,10-dihydro-10lambda^6-thioxanthene-3-carboxylic acid",
            ),
        ] {
            let name = super::super::name_smiles(smiles, &Default::default()).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            assert_eq!(name.status, crate::naming::NameStatus::Preferred);
            for candidate in std::iter::once(&name.text).chain(
                name.alternatives
                    .iter()
                    .map(|alternative| &alternative.name),
            ) {
                let parsed = parser.parse(candidate, &opsin::ParseOptions::strict());
                assert_eq!(
                    parsed.status,
                    opsin::Status::Success,
                    "{candidate}: {}",
                    parsed.message
                );
                assert!(
                    parsed.warnings.is_empty(),
                    "{candidate}: {:?}",
                    parsed.warnings
                );
                let reconstructed = parsed.structure.unwrap().semantic_cxsmiles().unwrap();
                crate::naming::verify_identity(smiles, &reconstructed)
                    .unwrap_or_else(|error| panic!("{smiles} -> {candidate}: {error}"));
            }
        }
    }
}
