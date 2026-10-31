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
        // Upstream divergence: normalize traversal-dependent tetrahedral tags before comparing ring faces.
        let face = |idx| relative_ring_face(mol, parts, path, idx);
        let term = if face(raw[0])? == face(raw[1])? {
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

fn relative_ring_face(
    mol: &Molecule,
    parts: &AssemblyParts,
    path: &[usize],
    idx: usize,
) -> Result<bool, String> {
    let fail = || "openclatura: unsupported relative ring stereochemistry".to_owned();
    let offset = path.iter().position(|&atom| atom == idx).ok_or_else(fail)?;
    if path.len() < 3 || mol.degree(idx) != 3 || mol.atoms[&idx].total_h_count != 1 {
        return Err(fail());
    }
    let previous = path[(offset + path.len() - 1) % path.len()];
    let next = path[(offset + 1) % path.len()];
    let outside = mol
        .get_neighbors(idx)
        .iter()
        .copied()
        .find(|atom| !parts.parent_atom_ids.contains(atom))
        .ok_or_else(fail)?;
    let target = [previous, next, outside];
    let permutation: Vec<_> = target
        .iter()
        .map(|atom| {
            mol.get_neighbors(idx)
                .iter()
                .position(|neighbor| neighbor == atom)
                .ok_or_else(fail)
        })
        .collect::<Result<_, _>>()?;
    let inversions = permutation
        .iter()
        .enumerate()
        .map(|(i, rank)| {
            permutation[i + 1..]
                .iter()
                .filter(|other| *other < rank)
                .count()
        })
        .sum::<usize>();
    // The implicit or bracket hydrogen stays last in both tetrahedral orders.
    let clockwise = match mol.atoms[&idx].raw_stereo.as_deref() {
        Some("CW") => true,
        Some("CCW") => false,
        _ => return Err(fail()),
    };
    Ok(clockwise ^ (inversions % 2 != 0))
}

#[cfg(test)]
mod tests {
    use super::super::name_smiles;

    #[test]
    fn relative_ring_stereo_is_independent_of_smiles_neighbor_order() {
        for (smiles, expected) in [
            ("C[C@H]1C[C@@H](C)C1", "cis-1,3-dimethylcyclobutane"),
            ("C[C@H]1C[C@H](C1)C", "cis-1,3-dimethylcyclobutane"),
            ("C[C@H]1C[C@H](C)C1", "trans-1,3-dimethylcyclobutane"),
            ("C[C@H]1C[C@@H](C1)C", "trans-1,3-dimethylcyclobutane"),
        ] {
            assert_eq!(name_smiles(smiles).as_deref(), Ok(expected), "{smiles}");
        }
    }

    #[test]
    fn external_double_bond_uses_modern_cip_priority() {
        let smiles = "C/C=C\\C(=C/C)[C@H](C)/C=C\\C";
        let name = name_smiles(smiles).unwrap();
        assert_eq!(name, "(2Z,4E,5R,6Z)-4-ethylidene-5-methylocta-2,6-diene");
        let request = crate::naming::local::parse_request(&name).unwrap();
        let reply = crate::naming::worker::respond(&request).unwrap();
        let parsed = crate::naming::local::parse_result(&name, &reply).unwrap();
        assert_eq!(
            crate::naming::canonical_smiles(smiles).unwrap(),
            crate::naming::canonical_smiles(&parsed.smiles).unwrap()
        );
    }
}
