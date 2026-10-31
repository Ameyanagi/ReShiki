//! Shared parent numbering and assembly preparation (plain-name path).
use std::collections::BTreeMap;

use super::assembly_parts::{AssemblyParts, ParentChargeItem, SubstituentItem};
use super::molecule::{AtomSet, Molecule, bond_ids_within};
use super::numbering::choose_parent_numbering;
use super::parent_selection::ParentSelection;

pub struct ParentAssemblyPlan {
    pub parts: AssemblyParts,
    pub numbered_path: Vec<usize>,
    pub locant_map: BTreeMap<usize, String>,
}

#[allow(clippy::too_many_arguments)]
pub fn build_parent_assembly_plan(
    mol: &Molecule,
    selection: &ParentSelection,
    principal_carbons: &[usize],
    root: Option<usize>,
    upstream: Option<usize>,
    substituents: &BTreeMap<usize, Vec<SubstituentItem>>,
    retained_name: Option<String>,
    locant_maps: &[BTreeMap<usize, String>],
) -> Result<ParentAssemblyPlan, String> {
    let principal: AtomSet = principal_carbons.iter().copied().collect();
    let names = substituents
        .iter()
        .map(|(&idx, items)| (idx, items.iter().map(|item| item.name.clone()).collect()))
        .collect();
    let (numbered_path, selected_map) = choose_parent_numbering(
        mol,
        &selection.paths,
        &principal,
        &names,
        locant_maps,
        selection.is_ring,
        retained_name.as_deref(),
        selection.fixed_start_required,
    );
    let locant_map = selected_map.unwrap_or_else(|| {
        numbered_path
            .iter()
            .enumerate()
            .map(|(i, &idx)| (idx, (i + 1).to_string()))
            .collect()
    });
    let parts = build_parent_parts(
        mol,
        &numbered_path,
        &locant_map,
        retained_name,
        selection.is_ring,
        root,
        upstream,
    )?;
    Ok(ParentAssemblyPlan {
        parts,
        numbered_path,
        locant_map,
    })
}

pub fn build_parent_parts(
    mol: &Molecule,
    numbered_path: &[usize],
    locants: &BTreeMap<usize, String>,
    retained_name: Option<String>,
    is_ring: bool,
    root: Option<usize>,
    upstream: Option<usize>,
) -> Result<AssemblyParts, String> {
    let mut parts = AssemblyParts::new(numbered_path.len());
    parts.is_ring = is_ring;
    parts.retained_parent_metadata = retained_name
        .as_deref()
        .and_then(super::rules::retained::parent_metadata);
    parts.retained_name = retained_name;
    parts.parent_atom_ids = numbered_path.iter().copied().collect();
    parts.parent_bond_ids = bond_ids_within(mol, &parts.parent_atom_ids);
    if let Some(root) = root {
        parts.is_substituent = true;
        parts.attachment_locant = locants[&root].clone();
        let order = upstream
            .and_then(|up| mol.get_bond(root, up))
            .map_or(1, |b| b.order);
        parts.is_double_attach = order == 2;
        parts.is_triple_attach = order == 3;
    }
    for &idx in numbered_path {
        let atom = &mol.atoms[&idx];
        let locant = &locants[&idx];
        parts.parent_atom_ids_by_locant.insert(locant.clone(), idx);
        parts
            .parent_atom_symbols_by_locant
            .insert(locant.clone(), atom.symbol.clone());
        parts
            .parent_atom_charges_by_locant
            .insert(locant.clone(), atom.charge);
        parts
            .parent_atom_isotopes_by_locant
            .insert(locant.clone(), atom.isotope);
        if let Some(stereo) = &atom.stereo {
            parts.stereo_features.push((locant.clone(), stereo.clone()));
        }
        if atom.charge != 0 {
            parts.parent_charges.push(ParentChargeItem {
                locant: locant.clone(),
                symbol: atom.symbol.clone(),
                charge: atom.charge,
            });
        }
        for &neighbor in mol.get_neighbors(idx) {
            if parts.parent_atom_ids.contains(&neighbor) && idx < neighbor {
                let other = &locants[&neighbor];
                let key = if super::locants::parse_locant(locant)
                    <= super::locants::parse_locant(other)
                {
                    (locant.clone(), other.clone())
                } else {
                    (other.clone(), locant.clone())
                };
                let bond = mol
                    .get_bond(idx, neighbor)
                    .ok_or("openclatura: missing parent bond")?;
                parts
                    .parent_bond_orders_by_locants
                    .insert(key.clone(), bond.order);
                parts.parent_bond_ids_by_locants.insert(key, bond.idx);
            }
        }
    }
    add_relative_ring_stereo(mol, &mut parts, numbered_path, locants)?;
    Ok(parts)
}

fn add_relative_ring_stereo(
    mol: &Molecule,
    parts: &mut AssemblyParts,
    path: &[usize],
    locants: &BTreeMap<usize, String>,
) -> Result<(), String> {
    if !parts.is_ring {
        return Ok(());
    }
    let scoped =
        super::small_ring_stereo::scoped_small_ring_stereo_features(mol, parts, path, |idx| {
            locants[&idx].clone()
        });
    if !scoped.is_empty() {
        parts.stereo_features.extend(scoped);
        return Ok(());
    }
    let raw: Vec<_> = path
        .iter()
        .copied()
        .filter(|idx| mol.atoms[idx].raw_stereo.is_some() && mol.atoms[idx].stereo.is_none())
        .collect();
    if raw.is_empty() {
        return Ok(());
    }
    if raw.len() != 2 {
        return Err("openclatura: unresolved ring stereochemistry not ported".into());
    }
    if raw.iter().all(|&idx| {
        mol.get_neighbors(idx)
            .iter()
            .filter(|n| !parts.parent_atom_ids.contains(n))
            .count()
            == 1
    }) {
        let term = if mol.atoms[&raw[0]].raw_stereo == mol.atoms[&raw[1]].raw_stereo {
            "cis"
        } else {
            "trans"
        };
        parts.relative_stereo_prefixes.push(term.into());
    } else {
        for idx in raw {
            let label = mol
                .accurate_cip
                .get(&idx)
                .ok_or("openclatura: local small-ring CIP labeling not ported")?;
            parts
                .stereo_features
                .push((locants[&idx].clone(), label.clone()));
        }
    }
    Ok(())
}
