//! Graph-fragment helpers shared by component and recursive substituent naming.

use std::collections::{BTreeMap, VecDeque};

use super::assembly_parts::AssemblyParts;
use super::chains::get_cyclic_atoms;
use super::locants::parse_locant;
use super::molecule::{AtomSet, Molecule};
use super::ring_parent::ParentHydrideKind;

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
            if atom.is_carbon() || atom.total_h_count == 0 {
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
            // Upstream divergence: reject H requiring an unrendered lambda citation at ordinary valence.
            let implied_valence = elements::get(&atom.symbol)
                .and_then(|e| reshiki_chemistry::ELEMENTS.get(e.atomic_number))
                .and_then(|e| {
                    e.valences
                        .iter()
                        // Upstream divergence: prove retained-atom H at the charge-adjusted valence.
                        .filter_map(|&v| usize::try_from(v + atom.charge).ok())
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
    let absorbs_replacement = parts
        .parent_hydride
        .as_ref()
        .map_or(parts.fusion_plan.is_some(), |parent| {
            parent.kind != ParentHydrideKind::SkeletalReplacementFusion
        });
    for &idx in numbered_path.iter().filter(|_| !absorbs_replacement) {
        let atom = &mol.atoms[&idx];
        if atom.is_carbon() {
            continue;
        }
        // Upstream divergence: every noncarbon parent atom needs an explicit replacement stem.
        let element = elements::get(&atom.symbol)
            .ok_or_else(|| format!("openclatura: unsupported parent element {}", atom.symbol))?;
        let stem = element.hw_stem.as_deref().ok_or_else(|| {
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
    let model = parts
        .parent_hydride
        .as_ref()
        .and_then(|parent| parent.bond_model())
        .or(parts.parent_bond_model.as_ref());
    if parts.parent_hydride.is_some() && model.is_none() {
        return Ok(());
    }
    let mut implied_multiple_bonds = AtomSet::new();
    if let Some(model) = model {
        use super::fusion::mancude::{CompareOptions, compare_actual_parent_to_implied_parent};
        let delta = if let Some(state) = parts
            .parent_hydride
            .as_ref()
            .and_then(|parent| parent.derivative_state())
            .or(parts.parent_derivative_state.as_ref())
        {
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
        if let Some(delta) = delta.as_ref().filter(|delta| delta.compatible) {
            implied_multiple_bonds.extend(delta.implied_multiple_bond_ids.iter().copied());
        }
        parts.parent_bond_delta = delta;
        if let Some(wrapper) = parts
            .parent_hydride
            .as_ref()
            .and_then(|parent| parent.fusion_wrapper_plan.as_ref())
        {
            // The nondetachable bridge prefix already expresses these bonds.
            implied_multiple_bonds.extend(
                wrapper
                    .bridges
                    .iter()
                    .flat_map(|bridge| bridge.bond_ids.iter().copied()),
            );
        }
    }
    let indices: BTreeMap<usize, usize> = numbered_path
        .iter()
        .enumerate()
        .map(|(i, &a)| (a, i))
        .collect();
    let mut seen = AtomSet::new();
    for &u in numbered_path {
        for &v in mol.get_neighbors(u) {
            let Some(&vi) = indices.get(&v) else {
                continue;
            };
            let Some(bond) = mol.get_bond(u, v) else {
                continue;
            };
            if bond.order <= 1
                || !seen.insert(bond.idx)
                || implied_multiple_bonds.contains(&bond.idx)
            {
                continue;
            }
            let bond_key = if bond.order == 2 { "double" } else { "triple" };
            let ui = indices[&u];
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
pub fn add_substituent(
    parts: &mut AssemblyParts,
    mut item: super::assembly_parts::SubstituentItem,
) {
    if let Some(spiro) = &mut item.spiro
        && let Some(locant) = item.locants.first()
    {
        spiro.parent_locant = locant.clone();
    }
    if let Some(existing) = parts.substituents.iter_mut().find(|existing| {
        existing.name == item.name
            && existing.outer_parentheses_optional == item.outer_parentheses_optional
            // Upstream divergence: keep spiro instances separate to preserve their component metadata.
            && existing.spiro.is_none()
            && item.spiro.is_none()
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
    fn skeletal_replacement_handoff_keeps_heteroatom_prefixes() {
        use super::super::ring_parent::RingParent;
        let mol = read_smiles("O1CCCC1").unwrap();
        let path: Vec<_> = mol.atoms.keys().copied().collect();
        let atoms: AtomSet = path.iter().copied().collect();
        let locants: BTreeMap<_, _> = path
            .iter()
            .enumerate()
            .map(|(i, &id)| (id, (i + 1).to_string()))
            .collect();
        let oxygen = mol
            .atoms
            .values()
            .find(|atom| atom.symbol == "O")
            .unwrap()
            .idx;
        let parent = RingParent::from_retained_locant_maps(
            atoms.clone(),
            vec![locants.clone()],
            "cyclopentane".into(),
            None,
        )
        .unwrap()
        .into_skeletal_replacement_fusion(vec![oxygen], vec!["replacement proof".into()])
        .unwrap();
        let mut parts = AssemblyParts::new(path.len());
        parts.parent_hydride = Some(parent);
        parts.parent_atom_ids = atoms;
        add_parent_features(&mol, &mut parts, &path, &locants).unwrap();
        assert_eq!(parts.a_prefixes.len(), 1);
        assert_eq!(parts.a_prefixes[0].name, "oxa");
        assert_eq!(parts.a_prefixes[0].atom_ids, AtomSet::from([oxygen]));
    }

    #[test]
    fn bridge_prefix_and_parent_model_own_their_multiple_bonds() {
        use super::super::{
            fusion::wrappers::plan_bridged_fusion_wrapper, ring_parent::RingParent,
        };
        let mol = read_smiles("C12=CC=C(C3=CC=CC=C13)C=C2").unwrap();
        let atoms: AtomSet = mol.atoms.keys().copied().collect();
        let wrapper = plan_bridged_fusion_wrapper(&mol, &atoms).unwrap().unwrap();
        let locants = wrapper.atom_to_locant();
        let mut path: Vec<_> = atoms.iter().copied().collect();
        path.sort_by_key(|id| parse_locant(&locants[id]));
        let mut parts = AssemblyParts::new(path.len());
        parts.is_ring = true;
        parts.is_polycycle = true;
        parts.parent_atom_ids = atoms;
        parts.parent_hydride = Some(RingParent::from_fusion_wrapper(wrapper).unwrap());
        add_parent_features(&mol, &mut parts, &path, &locants).unwrap();
        assert!(parts.parent_bond_delta.as_ref().unwrap().compatible);
        assert!(parts.unsaturations.is_empty());
    }

    #[test]
    fn same_name_spiro_branches_keep_their_graph_metadata() {
        use super::super::{assembly_parts::SubstituentItem, assembly_spiro::SpiroAssembly};
        let mut parts = AssemblyParts::new(6);
        add_substituent(
            &mut parts,
            SubstituentItem::new("spiro side", vec!["1".into()]),
        );
        let mut side = SubstituentItem::new("spiro side", vec!["4".into()]);
        side.spiro = Some(Box::new(SpiroAssembly::new("2", "oxirane")));
        add_substituent(&mut parts, side);
        assert_eq!(parts.substituents.len(), 2);
        let spiro = parts.substituents[1].spiro.as_ref().unwrap();
        assert_eq!(spiro.parent_locant, "4");
        assert_eq!(spiro.side_locant, "2");
        assert_eq!(spiro.side_parent_name, "oxirane");
    }

    #[test]
    fn retained_sulfur_uses_the_ordinary_valence_implied_by_its_substituents() {
        assert_eq!(
            super::super::name_smiles("Cl[SH]1C=c2ccccc2=C1")
                .map(|name| name.text)
                .as_deref(),
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
