//! Graph-fragment helpers shared by component and recursive substituent naming.

use std::collections::{BTreeMap, VecDeque};

use super::assembly_parts::AssemblyParts;
use super::chains::get_cyclic_atoms;
use super::locants::parse_locant;
use super::molecule::{AtomSet, Molecule};

/// Return the connected branch reached without crossing the parent boundary.
pub fn subgraph_component(mol: &Molecule, start_idx: usize, exclude_atoms: &AtomSet) -> AtomSet {
    let mut visited = exclude_atoms.clone();
    let mut component = AtomSet::new();
    let mut queue = VecDeque::from([start_idx]);
    while let Some(curr) = queue.pop_front() {
        if mol.atoms.contains_key(&curr) && visited.insert(curr) {
            component.insert(curr);
            queue.extend(
                mol.get_neighbors(curr)
                    .iter()
                    .copied()
                    .filter(|n| !visited.contains(n)),
            );
        }
    }
    component
}

/// Collect E/Z descriptors at their numbered parent bond locant.
pub fn emit_bond_stereo(
    mol: &Molecule,
    parts: &mut AssemblyParts,
    numbered_path: &[usize],
    locants: &BTreeMap<usize, String>,
    upstream_atom: Option<usize>,
) {
    let path_set: AtomSet = numbered_path.iter().copied().collect();
    let cyclic = get_cyclic_atoms(mol, &AtomSet::new());
    for &u in numbered_path {
        for &v in mol.get_neighbors(u) {
            let Some(bond) = mol.get_bond(u, v) else {
                continue;
            };
            let Some(stereo) = bond.stereo.as_ref() else {
                continue;
            };
            if bond.in_small_ring {
                continue;
            }
            if Some(v) == upstream_atom && (mol.atoms[&v].is_carbon() || cyclic.contains(&v)) {
                continue;
            }
            let Some(loc_u) = locants.get(&u) else {
                continue;
            };
            let loc = if path_set.contains(&v) {
                let Some(loc_v) = locants.get(&v) else {
                    continue;
                };
                if parse_locant(loc_u) <= parse_locant(loc_v) {
                    loc_u
                } else {
                    loc_v
                }
            } else {
                loc_u
            };
            if !parts
                .stereo_features
                .iter()
                .any(|(l, s)| l == loc && matches!(s.as_str(), "E" | "Z"))
            {
                parts.stereo_features.push((loc.clone(), stereo.clone()));
            }
        }
    }
}

/// Add the selected parent's replacement and explicit multiple-bond features.
pub fn add_parent_features(
    mol: &Molecule,
    parts: &mut AssemblyParts,
    numbered_path: &[usize],
    locants: &BTreeMap<usize, String>,
) {
    use super::assembly_parts::{SubstituentItem, UnsaturationItem};
    use super::nomenclature::section;
    use super::rules::elements;
    if parts.retained_name.is_some() {
        return;
    }
    for &idx in numbered_path {
        let atom = &mol.atoms[&idx];
        if atom.is_carbon() {
            continue;
        }
        let Some(element) = elements::get(&atom.symbol) else {
            continue;
        };
        let Some(stem) = element.hw_stem.as_ref() else {
            continue;
        };
        let valence: usize = mol
            .get_neighbors(idx)
            .iter()
            .filter_map(|&n| mol.get_bond(idx, n))
            .map(|b| b.order)
            .sum();
        let charged = section("replacement_charge_states")
            .as_array()
            .and_then(|rows| {
                rows.iter().find(|row| {
                    row["symbol"].as_str() == Some(atom.symbol.as_str())
                        && row["charge"].as_i64() == Some(atom.charge as i64)
                        && row["valence"].as_u64() == Some((valence + atom.total_h_count) as u64)
                })
            })
            .and_then(|row| row["prefix"].as_str());
        let mut loc = locants[&idx].clone();
        if atom.charge == 0 && valence > element.standard_valence {
            loc = format!("{loc}lambda^{valence}");
        }
        let mut item = SubstituentItem::new(charged.unwrap_or(stem), vec![loc]);
        item.atom_ids.insert(idx);
        if charged.is_some() {
            item.charge_atom_ids.insert(idx);
        }
        parts.a_prefixes.push(item);
    }
    let indices: BTreeMap<usize, usize> = numbered_path
        .iter()
        .enumerate()
        .map(|(i, &a)| (a, i))
        .collect();
    let mut seen = AtomSet::new();
    for &u in numbered_path {
        for &v in mol.get_neighbors(u) {
            if !indices.contains_key(&v) {
                continue;
            }
            let Some(bond) = mol.get_bond(u, v) else {
                continue;
            };
            if bond.order <= 1 || !seen.insert(bond.idx) {
                continue;
            }
            let bond_key = if bond.order == 2 { "double" } else { "triple" };
            let (ui, vi) = (indices[&u], indices[&v]);
            let (low_idx, high_idx) = (ui.min(vi), ui.max(vi));
            let (lu, lv) = (&locants[&u], &locants[&v]);
            let (low_loc, high_loc) = if parse_locant(lu) <= parse_locant(lv) {
                (lu, lv)
            } else {
                (lv, lu)
            };
            let loc = if high_idx == low_idx + 1 {
                low_loc.clone()
            } else if low_idx == 0 && high_idx == numbered_path.len() - 1 {
                high_loc.clone()
            } else {
                format!("{low_loc}({high_loc})")
            };
            if let Some(existing) = parts
                .unsaturations
                .iter_mut()
                .find(|item| item.bond_key == bond_key)
            {
                existing.locants.push(loc);
                existing.atom_ids.extend([u, v]);
                existing.bond_ids.insert(bond.idx);
            } else {
                parts.unsaturations.push(UnsaturationItem {
                    bond_key: bond_key.to_string(),
                    locants: vec![loc],
                    atom_ids: [u, v].into_iter().collect(),
                    bond_ids: [bond.idx].into_iter().collect(),
                });
            }
        }
    }
}

/// Collect ordinary retained monocycle hydrogen citations and hydro operations.
pub fn add_indicated_hydrogens(
    mol: &Molecule,
    parts: &mut AssemblyParts,
    numbered_path: &[usize],
    locants: &BTreeMap<usize, String>,
) {
    use super::assembly_parts::HydroOperation;
    use super::rules::{elements, retained};
    let Some(name) = parts.retained_name.as_deref() else {
        return;
    };
    if let Some(plan) = retained::mancude_monocycle_hydro_plan(mol, numbered_path, Some(name)) {
        let mut ordered = plan.citable;
        ordered.sort_by_key(|idx| parse_locant(&locants[idx]));
        let held = plan.indicated + plan.added;
        for &idx in ordered.iter().take(held) {
            let loc = locants[&idx].clone();
            parts.indicated_hydrogens.push(loc.clone());
            parts.hydro_operations.push(HydroOperation {
                key: "indicated_hydrogen".into(),
                locants: vec![loc],
                operation_kind: "indicated_hydrogen".into(),
            });
        }
        let hydro: Vec<_> = ordered.into_iter().skip(held).collect();
        if !hydro.is_empty() {
            parts.hydro_operations.push(HydroOperation {
                key: "additive_hydrogen".into(),
                locants: hydro.iter().map(|idx| locants[idx].clone()).collect(),
                operation_kind: "additive_hydrogen".into(),
            });
        }
        recast_ring_ketone_hydrogens(parts, locants);
        return;
    }
    if !retained::cites_indicated_hydrogen(mol, numbered_path, Some(name)) {
        return;
    }
    if name == "tetrazole" && numbered_path.iter().any(|idx| mol.atoms[idx].charge != 0) {
        return;
    }
    for &idx in numbered_path {
        let atom = &mol.atoms[&idx];
        if !matches!(
            atom.symbol.as_str(),
            "C" | "N" | "P" | "As" | "Sb" | "Bi" | "B" | "Si" | "Ge" | "Sn" | "Pb"
        ) {
            continue;
        }
        if elements::get(&atom.symbol).is_some_and(|e| e.mancude_forced_single) {
            continue;
        }
        let ring_order: usize = mol
            .get_neighbors(idx)
            .iter()
            .filter(|n| locants.contains_key(n))
            .filter_map(|&n| mol.get_bond(idx, n))
            .map(|b| b.order)
            .sum();
        if ring_order == 2 {
            parts.indicated_hydrogens.push(locants[&idx].clone());
        }
    }
}

/// P-31.1.4.2.4 distinguishes intrinsic ring H from ketone-added H.
fn recast_ring_ketone_hydrogens(parts: &mut AssemblyParts, locants: &BTreeMap<usize, String>) {
    use super::assembly_parts::HydroOperation;
    let Some(group) = parts.principal_group.as_ref().filter(|g| g.key == "ketone") else {
        return;
    };
    let ketone_locs = group.locants.clone();
    let indicated: Vec<_> = parts
        .hydro_operations
        .iter()
        .filter(|op| op.key == "indicated_hydrogen")
        .cloned()
        .collect();
    if indicated.is_empty()
        || parts
            .hydro_operations
            .iter()
            .any(|op| op.key == "added_hydrogen")
    {
        return;
    }
    let nh: Vec<_> = indicated
        .iter()
        .filter_map(|op| op.locants.first().cloned())
        .collect();
    if nh.iter().any(|loc| ketone_locs.contains(loc)) {
        return;
    }
    if nh.len() == ketone_locs.len() {
        for op in &mut parts.hydro_operations {
            if op.key == "indicated_hydrogen" {
                op.key = "added_hydrogen".into();
            }
        }
    } else if ketone_locs.len() == 1 && nh.len() > 1 {
        if !locants.values().any(|loc| *loc == ketone_locs[0]) {
            return;
        }
        parts
            .hydro_operations
            .retain(|op| op.key != "indicated_hydrogen");
        parts.indicated_hydrogens.retain(|loc| !nh.contains(loc));
        parts.indicated_hydrogens.push(ketone_locs[0].clone());
        parts.hydro_operations.push(HydroOperation {
            key: "indicated_hydrogen".into(),
            locants: vec![ketone_locs[0].clone()],
            operation_kind: "indicated_hydrogen".into(),
        });
        parts.hydro_operations.push(HydroOperation {
            key: "additive_hydrogen".into(),
            locants: nh,
            operation_kind: "additive_hydrogen".into(),
        });
    }
}

/// Merge same-name branch citations before parent promotion, as the upstream
/// add_substituent_trace does even when no trace metadata is requested.
pub fn add_substituent(parts: &mut AssemblyParts, item: super::assembly_parts::SubstituentItem) {
    if let Some(existing) = parts.substituents.iter_mut().find(|existing| {
        existing.name == item.name
            && existing.outer_parentheses_optional == item.outer_parentheses_optional
    }) {
        existing.locants.extend(item.locants);
        existing.atom_ids.extend(item.atom_ids);
        existing.bond_ids.extend(item.bond_ids);
        existing.charge_atom_ids.extend(item.charge_atom_ids);
    } else {
        parts.substituents.push(item);
    }
}
