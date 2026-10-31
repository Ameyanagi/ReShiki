//! Shared parent numbering and assembly preparation (plain-name path).
use std::collections::BTreeMap;

use super::assembly_parts::{AssemblyParts, ParentChargeItem, SubstituentItem};
use super::molecule::{AtomSet, Molecule, bond_ids_within};
use super::numbering::{NumberingTopology, ProvenHydrogenLocants, choose_parent_numbering};
use super::parent_selection::ParentSelection;
use super::ring_parent::RingParent;

/// Apply the retained-parent eligibility checks before sharing a retained
/// identity between component and recursive-substituent assembly.
pub fn resolve_retained_parent(
    mol: &Molecule,
    selection: &ParentSelection,
) -> Option<super::rules::retained::RetainedMatch> {
    if !selection.is_ring {
        return None;
    }
    let path = selection.primary_path();
    let retained = super::rules::retained::get_retained_ring(mol, path)?;
    let allowed = super::nomenclature::section("retained_ring_elements");
    if path.iter().any(|idx| {
        !allowed
            .as_array()
            .into_iter()
            .flatten()
            .any(|value| value.as_str() == Some(mol.atoms[idx].symbol.as_str()))
    }) {
        return None;
    }
    if retained.locant_maps.is_empty()
        && (selection.is_bicycle || selection.is_polycycle)
        && path
            .iter()
            .any(|idx| mol.atoms[idx].symbol != "C" || mol.atoms[idx].charge != 0)
    {
        return None;
    }
    Some(retained)
}

pub fn resolve_parent_hydride_plan(
    mol: &Molecule,
    selection: &ParentSelection,
    retained_name: Option<&str>,
) -> Result<Option<RingParent>, String> {
    if retained_name.is_some() || !(selection.is_bicycle || selection.is_polycycle) {
        return Ok(None);
    }
    let atoms = selection.atom_set();
    if let Some(plan) = super::fusion::planner::plan_fusion_parent(mol, &atoms)? {
        return RingParent::from_fusion_plan(plan).map(Some);
    }
    if let Some(plan) =
        super::fusion::third_component::plan_third_component_fusion_parent(mol, &atoms)?
    {
        return Ok(Some(plan.parent));
    }
    super::fusion::wrappers::plan_bridged_fusion_wrapper(mol, &atoms)?
        .map(RingParent::from_fusion_wrapper)
        .transpose()
}

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
    let parent = if root.is_some() {
        if retained_name.is_none() && (selection.is_bicycle || selection.is_polycycle) {
            super::fusion::planner::plan_fusion_parent(mol, &selection.atom_set())?
                .map(RingParent::from_fusion_plan)
                .transpose()?
        } else {
            None
        }
    } else {
        resolve_parent_hydride_plan(mol, selection, retained_name.as_deref())?
    };
    build_parent_assembly_plan_with_parent(
        mol,
        selection,
        principal_carbons,
        root,
        upstream,
        substituents,
        retained_name,
        locant_maps,
        parent,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn build_parent_assembly_plan_with_parent(
    mol: &Molecule,
    selection: &ParentSelection,
    principal_carbons: &[usize],
    root: Option<usize>,
    upstream: Option<usize>,
    substituents: &BTreeMap<usize, Vec<SubstituentItem>>,
    retained_name: Option<String>,
    locant_maps: &[BTreeMap<usize, String>],
    mut parent: Option<RingParent>,
) -> Result<ParentAssemblyPlan, String> {
    let retained_name = parent
        .as_ref()
        .filter(|p| p.kind == super::ring_parent::ParentHydrideKind::Retained)
        .and_then(|p| p.base_name().map(str::to_owned))
        .or(retained_name);
    let variants: Vec<_> = parent
        .as_ref()
        .filter(|p| p.uses_fusion_plan() && p.replacement_fusion_state.is_none())
        .and_then(|p| p.fusion_plan.as_ref())
        .map_or_else(Vec::new, |plan| {
            if plan.numbering_variants.is_empty() {
                vec![plan.clone()]
            } else {
                plan.numbering_variants.clone()
            }
        });
    let fusion_maps = if variants.is_empty() {
        parent
            .as_ref()
            .map_or_else(Vec::new, RingParent::proof_locant_maps)
    } else {
        variants
            .iter()
            .flat_map(|p| p.numbering.string_input_locant_maps())
            .collect()
    };
    let proven_hydrogens: ProvenHydrogenLocants = variants
        .iter()
        .flat_map(|plan| {
            let hydrogen = (
                plan.indicated_hydrogens.clone(),
                plan.derivative_state
                    .hydro_operations
                    .iter()
                    .flat_map(|op| op.locants.iter().cloned())
                    .collect(),
            );
            plan.numbering
                .string_input_locant_maps()
                .into_iter()
                .map(move |map| (map.into_iter().collect(), hydrogen.clone()))
        })
        .collect();
    let principal: AtomSet = principal_carbons.iter().copied().collect();
    let names = substituents
        .iter()
        .map(|(&idx, items)| (idx, items.iter().map(|item| item.name.clone()).collect()))
        .collect();
    // A von Baeyer descriptor and its accepted numbering paths are one
    // topology proof. Upstream consumes those paths as locant maps, whose
    // preference order differs from ordinary ring-path numbering.
    let proof_maps: Vec<BTreeMap<usize, String>> = if locant_maps.is_empty()
        && super::ring_renderer::is_von_baeyer_descriptor(selection.polycycle_descriptor.as_deref())
    {
        selection
            .paths
            .iter()
            .map(|path| {
                path.iter()
                    .enumerate()
                    .map(|(index, &atom)| (atom, (index + 1).to_string()))
                    .collect()
            })
            .collect()
    } else {
        Vec::new()
    };
    let locant_maps = if !fusion_maps.is_empty() {
        &fusion_maps
    } else if proof_maps.is_empty() {
        locant_maps
    } else {
        &proof_maps
    };
    let (numbered_path, selected_map) = choose_parent_numbering(
        mol,
        &selection.paths,
        &principal,
        &names,
        locant_maps,
        selection.is_ring,
        retained_name.as_deref(),
        selection.requires_fixed_substituent_start(),
        NumberingTopology {
            is_bicycle: selection.is_bicycle,
            is_spiro: selection.is_spiro,
            is_polycycle: selection.is_polycycle,
        },
        parent
            .as_ref()
            .filter(|p| p.is_systematic_fusion())
            .map(|_| &proven_hydrogens),
    );
    let locant_map = selected_map.unwrap_or_else(|| {
        numbered_path
            .iter()
            .enumerate()
            .map(|(i, &idx)| (idx, (i + 1).to_string()))
            .collect()
    });
    if !variants.is_empty() {
        let selected = variants
            .into_iter()
            .find(|plan| {
                plan.numbering
                    .string_input_locant_maps()
                    .contains(&locant_map)
            })
            .ok_or("openclatura: fusion assembly numbering is not audited")?;
        let previous = parent
            .take()
            .ok_or("openclatura: fusion numbering variants have no parent")?;
        let mut selected = RingParent::from_fusion_plan(selected)?;
        if previous.is_skeletal_replacement_fusion() {
            selected = selected.into_skeletal_replacement_fusion(
                previous.skeletal_replacement_atom_ids,
                previous.skeletal_replacement_audit_checks,
            )?;
        }
        parent = Some(selected);
    }
    let mut parts = build_parent_parts(
        mol,
        &numbered_path,
        &locant_map,
        retained_name,
        selection,
        root,
        upstream,
        parent.as_ref(),
    )?;
    if let Some(parent) = parent {
        if let Some(metadata) = &parent.hydride_metadata {
            parts.retained_parent_metadata = Some(metadata.clone());
        }
        parts.parent_bond_model = parent.bond_model().cloned();
        parts.parent_bond_delta = parent
            .derivative_state()
            .map(|state| state.bond_delta.clone());
        parts.parent_derivative_state = parent.derivative_state().cloned();
        parts.fusion_plan = parent.fusion_plan.clone();
        parts.parent_hydride = Some(parent);
        parts.proof_locant_map = true;
    }
    Ok(ParentAssemblyPlan {
        parts,
        numbered_path,
        locant_map,
    })
}

#[allow(clippy::too_many_arguments)]
fn build_parent_parts(
    mol: &Molecule,
    numbered_path: &[usize],
    locants: &BTreeMap<usize, String>,
    retained_name: Option<String>,
    selection: &ParentSelection,
    root: Option<usize>,
    upstream: Option<usize>,
    parent_hydride: Option<&RingParent>,
) -> Result<AssemblyParts, String> {
    if retained_name.is_none()
        && parent_hydride.is_none()
        && let Some(descriptor) = selection.polycycle_descriptor.as_deref()
        && super::ring_renderer::is_von_baeyer_descriptor(Some(descriptor))
        && !super::polycycle_topology::audit_von_baeyer_opsin_reading(
            descriptor,
            numbered_path,
            &super::polycycle_topology::edges_within_atoms(
                mol,
                &numbered_path.iter().copied().collect(),
            ),
        )
    {
        // Upstream divergence: reject secondary-bridge numbering that moves atoms or substituents in OPSIN.
        return Err("openclatura: secondary-bridge numbering disagrees with OPSIN reading".into());
    }
    let mut parts = AssemblyParts::new(numbered_path.len());
    parts.is_ring = selection.is_ring;
    parts.is_bicycle = selection.is_bicycle;
    parts.bicycle_xyz = if selection.is_bicycle {
        selection.xyz
    } else {
        (0, 0, 0)
    };
    parts.is_spiro = selection.is_spiro;
    parts.is_polycycle = selection.is_polycycle;
    parts.spiro_xy = if selection.is_spiro {
        (selection.xyz.0, selection.xyz.1)
    } else {
        (0, 0)
    };
    parts.polycycle_descriptor = selection.polycycle_descriptor.clone();
    parts.retained_parent_metadata = retained_name
        .as_deref()
        .and_then(super::rules::retained::parent_metadata);
    parts.retained_name = retained_name;
    parts.parent_atom_ids = numbered_path.iter().copied().collect();
    parts.parent_bond_ids = bond_ids_within(mol, &parts.parent_atom_ids);
    if let Some(root) = root {
        parts.is_substituent = true;
        parts.attachment_locant = locants[&root].clone();
        parts.attachment_order = upstream
            .and_then(|up| mol.get_bond(root, up))
            .map_or(1, |b| b.order);
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
                atom_id: Some(idx),
                locant: locant.clone(),
                symbol: atom.symbol.clone(),
                charge: atom.charge,
            });
        }
        for &neighbor in mol.get_neighbors(idx) {
            if parts.parent_atom_ids.contains(&neighbor) && idx < neighbor {
                let other = &locants[&neighbor];
                let key = super::locants::canonical_locant_pair(locant, other);
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
    // Upstream divergence: decline multicyclic shortcuts that can drop unresolved relative stereotags.
    if (parts.is_bicycle || parts.is_spiro || parts.is_polycycle)
        && path
            .iter()
            .any(|idx| mol.atoms[idx].raw_stereo.is_some() && mol.atoms[idx].stereo.is_none())
    {
        return Err("openclatura: unresolved multicyclic stereochemistry not ported".into());
    }
    // Upstream divergence: decline von Baeyer pseudoasymmetry that OPSIN loses or reads as another isomer.
    if (parts.is_bicycle
        || super::ring_renderer::is_von_baeyer_descriptor(parts.polycycle_descriptor.as_deref()))
        && path.iter().any(|idx| {
            mol.atoms[idx]
                .stereo
                .as_deref()
                .is_some_and(|label| label == "r" || label == "s")
        })
    {
        return Err("openclatura: pseudoasymmetric von Baeyer centres are not validated".into());
    }
    if !parts.is_simple_ring() {
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
    fn spiro_parents_keep_descriptor_numbering_and_internal_bond_locants() {
        for (smiles, expected) in [
            ("C1CCC2(CC1)CCCC2", "spiro[4.5]decane"),
            ("C1CCC2(CC1)OCCO2", "1,4-dioxaspiro[4.5]decane"),
            ("BrC1=CCC12CCCCO2", "1-bromo-5-oxaspiro[3.5]non-1-ene"),
            (
                "BrC1C2(CCCS2)C12CCS2",
                "10-bromo-1,6-dithiadispiro[3.0.4.1]decane",
            ),
        ] {
            assert_eq!(name_smiles(smiles).as_deref(), Ok(expected), "{smiles}");
        }
    }

    #[test]
    fn von_baeyer_numbering_preserves_replacements_suffixes_and_substituent_roots() {
        for (smiles, expected) in [
            (
                "NC(=O)C1N2CCCCCP3C(CCC2)CC13F",
                "1-fluoro-8-aza-2-phosphatricyclo[6.5.1.0^{2,12}]tetradecane-14-carboxamide",
            ),
            (
                "SC12CC3=C1B32",
                "1-boratricyclo[2.1.0.0^{2,5}]pent-4-ene-2-thiol",
            ),
            (
                "COC12C3CC1(Cl)C32",
                "2-chloro-1-methoxytricyclo[2.1.0.0^{2,5}]pentane",
            ),
            (
                "CC(=O)NC12C=C1BNC2",
                "N-(3-aza-2-borabicyclo[3.1.0]hex-1(6)-en-5-yl)acetamide",
            ),
        ] {
            assert_eq!(name_smiles(smiles).as_deref(), Ok(expected), "{smiles}");
        }
    }

    #[test]
    fn von_baeyer_parents_decline_pseudoasymmetric_centres() {
        for smiles in [
            "C12=C3C4=C1[C@@H]1[C@H]2[C@H]3N41",
            "CN1[C@@H]2CC[C@H]1C[C@@H](OC(=O)[C@@H](CO)c1ccccc1)C2",
        ] {
            assert_eq!(
                name_smiles(smiles).unwrap_err(),
                "openclatura: pseudoasymmetric von Baeyer centres are not validated",
                "{smiles}",
            );
        }
    }

    #[test]
    fn bicyclic_parents_decline_unresolved_relative_stereochemistry() {
        for smiles in ["C1CC[C@H]2CCCCC[C@H](CC1)OCCC2", "C1C[C@H]2CC[C@@H]12"] {
            assert!(name_smiles(smiles).is_err(), "{smiles}");
        }
    }

    #[test]
    fn secondary_bridge_reading_disagreement_declines_with_or_without_stereo() {
        for smiles in [
            "OC[C@@H]1C=C2C[C@H](Cl)[C@H]1CC[C@@H]1CCCCCC[C@@H]2CCCCC1",
            "OC[C@H]1C=C2C[C@H](Cl)[C@H]1CC[C@@H]1CCCCCC[C@@H]2CCCCC1",
            "OCC1C=C2CC(Cl)C1CCC1CCCCCCC2CCCCC1",
        ] {
            assert_eq!(
                name_smiles(smiles).unwrap_err(),
                "openclatura: secondary-bridge numbering disagrees with OPSIN reading",
                "{smiles}",
            );
        }
        assert_eq!(
            name_smiles("CC1C(F)CCCC2CCCC=CCC1C1CC(CC2)C1").as_deref(),
            Ok("17-fluoro-18-methyltricyclo[6.5.5.1^{9,11}]nonadec-5-ene"),
        );
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
