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
) -> Result<(), String> {
    use super::assembly_parts::{SubstituentItem, UnsaturationItem};
    use super::nomenclature::section;
    use super::rules::elements;
    if parts.retained_name.is_some() {
        for &idx in numbered_path {
            let atom = &mol.atoms[&idx];
            if atom.is_carbon() || atom.charge != 0 || atom.total_h_count == 0 {
                continue;
            }
            let heavy_bonding = mol
                .get_neighbors(idx)
                .iter()
                .filter_map(|&n| {
                    let bond = mol.get_bond(idx, n)?;
                    (!(bond.order == 2 && mol.atoms[&n].symbol == "O")).then_some(bond.order)
                })
                .sum::<usize>();
            // Upstream skips replacement/lambda prefixes for a retained parent.
            // Its ordinary valence must therefore imply the observed H count:
            // three heavy single bonds at sulfur imply one H at valence four, while two
            // heavy bonds do not imply the extra H of a ring [SH2] atom.
            // Upstream divergence: reject H that would need an unrendered
            // lambda citation, using the smallest accommodating ordinary valence.
            let implied_valence = elements::get(&atom.symbol)
                .and_then(|e| reshiki_chemistry::ELEMENTS.get(e.atomic_number))
                .and_then(|e| {
                    e.valences
                        .iter()
                        .filter_map(|&v| usize::try_from(v).ok())
                        .filter(|&v| v >= heavy_bonding)
                        .min()
                });
            if implied_valence != Some(heavy_bonding + atom.total_h_count) {
                return Err(format!(
                    "openclatura: retained parent hydrogen state for {} is not supported",
                    atom.symbol
                ));
            }
        }
        return Ok(());
    }
    for &idx in numbered_path {
        if parts.fusion_plan.is_some() {
            break;
        }
        let atom = &mol.atoms[&idx];
        if atom.is_carbon() {
            continue;
        }
        // Upstream divergence: every noncarbon parent atom needs an explicit replacement stem.
        let element = elements::get(&atom.symbol)
            .ok_or_else(|| format!("openclatura: unsupported parent element {}", atom.symbol))?;
        let stem = element.hw_stem.as_ref().ok_or_else(|| {
            format!(
                "openclatura: parent replacement for {} is not supported",
                atom.symbol
            )
        })?;
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
        // Upstream divergence: replacement lambda citations include every attached hydrogen in the bonding number.
        let bonding = valence + atom.total_h_count;
        if atom.charge == 0 && bonding > element.standard_valence {
            loc = format!("{loc}lambda^{bonding}");
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
            if bond.order <= 1
                || !seen.insert(bond.idx)
                || parts.fusion_plan.is_some()
                    && parts.parent_bond_delta.as_ref().is_some_and(|delta| {
                        delta.compatible && delta.implied_multiple_bond_ids.contains(&bond.idx)
                    })
            {
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
            } else if low_idx == 0
                && high_idx == numbered_path.len() - 1
                && !(parts.is_bicycle || parts.is_spiro || parts.is_polycycle)
            {
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
    Ok(())
}

/// Collect retained-parent hydrogen citations using the parent bond model.
pub fn add_indicated_hydrogens(
    mol: &Molecule,
    parts: &mut AssemblyParts,
    numbered_path: &[usize],
    locants: &BTreeMap<usize, String>,
) -> Result<(), String> {
    super::additive::add_indicated_hydrogens(mol, parts, numbered_path, locants)
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

#[cfg(test)]
mod tests {
    use super::super::graph_io::read_smiles;
    use super::*;

    #[test]
    fn retained_sulfur_uses_the_ordinary_valence_implied_by_its_substituents() {
        assert_eq!(
            super::super::name_smiles("Cl[SH]1C=c2ccccc2=C1").as_deref(),
            Ok("2-chloro-2-benzothiophene")
        );
        assert!(super::super::name_smiles("[SH2]1CCCC1").is_err());
    }

    #[test]
    fn retained_parent_does_not_absorb_unrepresented_extra_hydrogen() {
        for (smiles, symbol, name) in [
            ("S1CCCC1", "S", "thiolane"),
            ("N1CCCC1", "N", "pyrrolidine"),
        ] {
            let mut mol = read_smiles(smiles).unwrap();
            let atom = mol
                .atoms
                .values_mut()
                .find(|atom| atom.symbol == symbol)
                .unwrap();
            atom.total_h_count = 2;
            let numbered_path: Vec<_> = mol.atoms.keys().copied().collect();
            let locants: BTreeMap<_, _> = numbered_path
                .iter()
                .enumerate()
                .map(|(i, &idx)| (idx, (i + 1).to_string()))
                .collect();
            let mut parts = AssemblyParts::new(numbered_path.len());
            parts.retained_name = Some(name.into());
            assert!(add_parent_features(&mol, &mut parts, &numbered_path, &locants).is_err());
        }
    }
}
