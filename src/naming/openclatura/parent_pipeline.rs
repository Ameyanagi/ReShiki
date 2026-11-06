//! Shared parent numbering and assembly preparation (plain-name path).
use std::collections::BTreeMap;

use super::assembly_parts::{AssemblyParts, ParentChargeItem, SubstituentItem};
use super::molecule::{AtomSet, Molecule, bond_ids_within};
use super::numbering::{
    NumberingModifiers, NumberingTopology, ProvenHydrogenLocants, choose_parent_numbering,
};
use super::parent_selection::ParentSelection;
use super::ring_parent::{ParentHydrideKind, RingParent};
use crate::naming::{NameReason, NameStatus};

/// Assess only a parent that has entered a successfully rendered name.
/// Fusion and wrapper plans are also cached and tried speculatively, so their
/// construction and numbering routines must not mutate the name assessment.
pub(super) fn mark_rendered_parent_status(mol: &Molecule, parts: &AssemblyParts) {
    // Retained-name exceptions are vetoed separately below.
    let audited = match parts.parent_hydride.as_ref().map(|parent| parent.kind) {
        Some(ParentHydrideKind::SkeletalReplacementFusion) => false,
        Some(_) => true,
        None => {
            parts.retained_name.is_some()
                || parts.fusion_plan.is_some()
                || parts.is_bicycle
                || parts.is_spiro
                || if parts.is_polycycle {
                    !super::ring_renderer::is_von_baeyer_descriptor(
                        parts.polycycle_descriptor.as_deref(),
                    )
                } else {
                    parts.a_prefixes.is_empty()
                }
        }
    };
    // Fused ketone anions retain their suffix-based hydrogen derivation.
    // Prefix-based hydrogen composition is outside this anion-parent audit.
    if parts.fusion_plan.is_some()
        && parts.parent_charges.iter().any(|site| site.charge < 0)
        && super::perception::perceive_groups(mol).iter().any(|group| {
            group.key == "ketone" && parts.parent_atom_ids.contains(&group.attachment_carbon)
        })
    {
        mol.mark_name(NameStatus::Unverified(NameReason::AnionParent));
    }
    if super::additive::has_atom_order_sensitive_bridge_hydrogen(parts) {
        mol.mark_name(NameStatus::Unverified(NameReason::AtomOrder));
    }
    if let Some(parent) = &parts.parent_hydride {
        mark_parent_hydride_status(mol, parent);
    } else if let Some(plan) = &parts.fusion_plan {
        mark_fusion_plan_status(mol, plan);
    } else if parts.retained_name.is_none()
        && !parts.is_spiro
        && (parts.is_bicycle
            || super::ring_renderer::is_von_baeyer_descriptor(
                parts.polycycle_descriptor.as_deref(),
            ))
        && super::fusion::faces::bridge_free_fusion_parent_eligible(mol, &parts.parent_atom_ids)
    {
        mol.mark_name(NameStatus::NotPreferred(
            NameReason::VonBaeyerFusionFallback,
        ));
    }
    if let Some(name) = &parts.retained_name {
        mark_retained_parent_status(mol, name);
    }
    if parts.locanted_total_hydrogenation {
        mol.mark_name(NameStatus::Unverified(NameReason::HydroPrefixLocants));
    }
    if !audited {
        mol.mark_name(NameStatus::Unverified(NameReason::UnauditedParent));
    }
}

fn mark_parent_hydride_status(mol: &Molecule, parent: &RingParent) {
    if let Some(assembly) = &parent.assembly {
        mol.record_ring_assembly(&assembly.junction_bond_ids);
        mark_retained_parent_status(mol, &assembly.unit_name);
        if !assembly.units_audited {
            mol.mark_name(NameStatus::Unverified(NameReason::UnauditedParent));
        }
    }
    if let Some(name) = parent.base_name() {
        mark_retained_parent_status(mol, name);
    }
    if let Some(plan) = &parent.fusion_plan {
        mark_fusion_plan_status(mol, plan);
    }
    if let Some(wrapper) = &parent.fusion_wrapper_plan {
        mark_parent_hydride_status(mol, &wrapper.parent.hydride);
        mol.mark_name(NameStatus::Unverified(NameReason::BridgedFusedParent));
    }
}

fn mark_fusion_plan_status(mol: &Molecule, plan: &super::fusion::model::FusionParentPlan) {
    if plan.ast.plan_kind == "benzoheterocycle" {
        mol.mark_benzo_fusion_alternative();
    }
    if plan.ast.benzoheterocycle_merge {
        mol.mark_higher_order_fusion_alternative();
    }
    mark_retained_parent_status(mol, &plan.rendered_base_name);
    if super::fusion::registry::fusion_component_registry()
        .has_senior_unregistered_retained_component(mol, plan)
    {
        mol.mark_name(NameStatus::NotPreferred(
            NameReason::UnregisteredRetainedComponent,
        ));
    }
    // PIN fusion names use [1,2]oxazole and [1,2]thiazole (P-25.3.2.1.2).
    if ["isoxazole", "isothiazole"]
        .iter()
        .any(|base| plan.rendered_base_name.ends_with(base))
    {
        mol.mark_name(NameStatus::NotPreferred(NameReason::FusionComponent));
    }
    if plan.ast.opsin_citation_order
        || matches!(
            plan.numbering.selected_layout.kind,
            super::fusion::model::LayoutKind::OpsinEntry
                | super::fusion::model::LayoutKind::OpsinConstruction
        )
    {
        mol.mark_name(NameStatus::Unverified(NameReason::OpsinFusionLayout));
    }
    // P-25.3.7.1: two identical senior components ortho-fused to a central
    // one take a multiparent name, so a base repeated twice more is unchecked.
    let repeats_base = plan.ast.component_occurrences.iter().any(|base| {
        plan.ast.parent_occurrences.contains(&base.occurrence_id)
            && plan
                .ast
                .component_occurrences
                .iter()
                .filter(|other| other.spec_key == base.spec_key)
                .count()
                >= 3
    });
    if repeats_base {
        mol.mark_name(NameStatus::Unverified(NameReason::FusionMultiparent));
    }
}

fn mark_retained_parent_status(mol: &Molecule, name: &str) {
    if matches!(name, "porphyrin" | "corrin" | "gonane") {
        mol.mark_name(NameStatus::Unverified(NameReason::UnauditedParent));
    }
}

/// Apply the retained-parent eligibility checks before sharing a retained
/// identity between component and recursive-substituent assembly.
pub fn resolve_retained_parent(
    mol: &Molecule,
    selection: &ParentSelection,
) -> Option<super::rules::retained::RetainedMatch> {
    if !selection.is_ring || selection.ring_assembly.is_some() {
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
    build_parent_assembly_plan_with_modifiers(
        mol,
        selection,
        principal_carbons,
        root,
        upstream,
        substituents,
        retained_name,
        locant_maps,
        &NumberingModifiers::default(),
    )
}

#[allow(clippy::too_many_arguments)]
pub fn build_parent_assembly_plan_with_modifiers(
    mol: &Molecule,
    selection: &ParentSelection,
    principal_carbons: &[usize],
    root: Option<usize>,
    upstream: Option<usize>,
    substituents: &BTreeMap<usize, Vec<SubstituentItem>>,
    retained_name: Option<String>,
    locant_maps: &[BTreeMap<usize, String>],
    modifiers: &NumberingModifiers,
) -> Result<ParentAssemblyPlan, String> {
    let parent = if let Some(assembly) = &selection.ring_assembly {
        Some(RingParent::from_ring_assembly(
            selection.atom_set(),
            assembly.clone(),
        )?)
    } else if root.is_some() {
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
        modifiers,
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
    modifiers: &NumberingModifiers,
    mut parent: Option<RingParent>,
) -> Result<ParentAssemblyPlan, String> {
    let retained_name = parent
        .as_ref()
        .filter(|p| {
            matches!(
                p.kind,
                super::ring_parent::ParentHydrideKind::Retained
                    | super::ring_parent::ParentHydrideKind::RingAssembly
            )
        })
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
    let wrapper_variants: Vec<_> = parent
        .as_ref()
        .and_then(|parent| parent.fusion_wrapper_plan.as_ref())
        .filter(|plan| !plan.numbering_variants.is_empty())
        .map_or_else(Vec::new, |plan| {
            std::iter::once(plan.as_ref())
                .chain(&plan.numbering_variants)
                .map(|variant| {
                    let mut variant = variant.clone();
                    variant.numbering_variants.clear();
                    variant
                })
                .collect()
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
    let mut proven_hydrogens: ProvenHydrogenLocants = variants
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
    if root.is_none()
        && !selection.is_spiro
        && !principal_carbons.is_empty()
        && parent
            .as_ref()
            .is_some_and(RingParent::is_systematic_fusion)
        && !variants.is_empty()
    {
        let ketones: AtomSet = super::perception::perceive_groups(mol)
            .into_iter()
            .filter(|group| group.key == "ketone")
            .map(|group| group.attachment_carbon)
            .collect();
        let suffix: AtomSet = principal_carbons.iter().copied().collect();
        if suffix.is_subset(&ketones) {
            for plan in &variants {
                for map in plan.numbering.string_input_locant_maps() {
                    if !suffix.iter().all(|atom| map.contains_key(atom)) {
                        continue;
                    }
                    if let Some((indicated, _, state)) =
                        super::additive::systematic_suffix_hydrogen_state(mol, plan, &suffix, &map)?
                    {
                        let hydro = state
                            .hydro_operations
                            .iter()
                            .flat_map(|op| op.locants.iter().cloned())
                            .collect();
                        proven_hydrogens.insert(map.into_iter().collect(), (indicated, hydro));
                    }
                }
            }
        }
    }
    if let Some(assembly) = &selection.ring_assembly {
        for map in &assembly.locant_maps {
            proven_hydrogens.insert(
                map.iter()
                    .map(|(&atom, locant)| (atom, locant.clone()))
                    .collect(),
                (
                    assembly
                        .indicated_hydrogen_atoms
                        .iter()
                        .map(|atom| map[atom].clone())
                        .collect(),
                    vec![],
                ),
            );
        }
    }
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
        modifiers,
        &selection.paths,
        principal_carbons,
        root,
        &names,
        locant_maps,
        selection.is_ring,
        retained_name.as_deref(),
        selection.requires_fixed_substituent_start(),
        NumberingTopology {
            is_assembly: selection.ring_assembly.is_some(),
            is_bicycle: selection.is_bicycle,
            is_spiro: selection.is_spiro,
            is_polycycle: selection.is_polycycle,
        },
        parent
            .as_ref()
            .filter(|p| p.is_systematic_fusion() || p.assembly.is_some())
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
    if !wrapper_variants.is_empty() {
        let selected = wrapper_variants
            .into_iter()
            .find(|plan| plan.atom_to_locant() == locant_map)
            .ok_or("openclatura: bridged fusion assembly numbering is not audited")?;
        parent = Some(RingParent::from_fusion_wrapper(selected)?);
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
    parts.stereogenic_atom_ids = mol.stereogenic_atoms.clone();
    parts.stereogenic_bond_ids = mol.stereogenic_bonds.clone();
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
            .parent_atom_hydrogens_by_locant
            .insert(locant.clone(), atom.total_h_count);
        parts.parent_external_bonds_by_locant.insert(
            locant.clone(),
            mol.get_neighbors(idx)
                .iter()
                .filter(|&&other| !parts.parent_atom_ids.contains(&other))
                .filter_map(|&other| {
                    mol.get_bond(idx, other)
                        .map(|bond| (other, bond.order, bond.idx))
                })
                .collect(),
        );
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
            if root.is_none() && matches!(stereo.as_str(), "Ξ" | "ξ") {
                mol.record_unknown_unit(locant.clone());
            }
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
    super::endo_exo_stereo::add_alternative(mol, &mut parts, numbered_path, locants);
    if selection.ring_assembly.is_none() {
        add_relative_ring_stereo(mol, &mut parts, numbered_path, locants)?;
    } else {
        for &atom in numbered_path {
            if mol.atoms[&atom].raw_stereo.is_some() && mol.atoms[&atom].stereo.is_none() {
                let descriptor = mol
                    .accurate_cip
                    .get(&atom)
                    .ok_or("openclatura: unresolved ring assembly stereochemistry")?;
                parts
                    .stereo_features
                    .push((locants[&atom].clone(), descriptor.clone()));
            }
        }
    }
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
    if !parts.is_simple_ring() || parts.is_ring_assembly() {
        return Ok(());
    }
    let relative: Vec<_> = path
        .iter()
        .copied()
        .filter(|idx| {
            let atom = &mol.atoms[idx];
            atom.raw_stereo.is_some() && matches!(atom.stereo.as_deref(), None | Some("r" | "s"))
        })
        .collect();
    if relative.len() == 2
        && relative.iter().all(|&idx| {
            mol.get_neighbors(idx)
                .iter()
                .filter(|n| !parts.parent_atom_ids.contains(n))
                .count()
                == 1
        })
    {
        let has_cip = relative
            .iter()
            .all(|idx| matches!(mol.atoms[idx].stereo.as_deref(), Some("r" | "s")));
        if has_cip && !mol.use_relative_ring_names() {
            mol.mark_relative_ring_alternative();
        }
        if mol.use_relative_ring_names() || !has_cip {
            let term = if relative_ring_face(mol, parts, path, relative[0])?
                == relative_ring_face(mol, parts, path, relative[1])?
            {
                "cis"
            } else {
                "trans"
            };
            parts
                .stereo_features
                .retain(|(locant, _)| !relative.iter().any(|idx| &locants[idx] == locant));
            parts.relative_stereo_prefixes.push(term.into());
            mol.mark_name(NameStatus::NotPreferred(NameReason::RelativeRingStereo));
        }
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
    for idx in raw {
        let label = mol
            .accurate_cip
            .get(&idx)
            .ok_or("openclatura: local small-ring CIP labeling not ported")?;
        parts
            .stereo_features
            .push((locants[&idx].clone(), label.clone()));
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
    use super::*;

    #[test]
    fn selected_retained_templates_keep_their_preference_status() {
        for (name, expected) in [
            (
                "[1,3]thiazolo[5,4-b]pyridine",
                NameStatus::Unverified(NameReason::UnauditedPath),
            ),
            (
                "[1,2]oxazolo[5,4-b]pyridine",
                NameStatus::Unverified(NameReason::UnauditedPath),
            ),
            (
                "porphyrin",
                NameStatus::Unverified(NameReason::UnauditedParent),
            ),
            (
                "corrin",
                NameStatus::Unverified(NameReason::UnauditedParent),
            ),
            (
                "gonane",
                NameStatus::Unverified(NameReason::UnauditedParent),
            ),
            (
                "naphthalene",
                NameStatus::Unverified(NameReason::UnauditedPath),
            ),
        ] {
            let mol = Molecule::new();
            let mut parts = AssemblyParts::new(1);
            parts.retained_name = Some(name.into());
            mark_rendered_parent_status(&mol, &parts);
            assert_eq!(mol.name_status(), expected, "{name}");
        }
    }

    #[test]
    fn unregistered_retained_fusion_status_is_deferred_until_rendering() {
        use super::super::{fusion::planner::plan_fusion_parent, graph_io::read_smiles};
        let mol = read_smiles("c1ccc2c(c1)Nc1c(ccc3ccccc13)S2").unwrap();
        let atoms = mol.atoms.keys().copied().collect();
        let plan = plan_fusion_parent(&mol, &atoms).unwrap().unwrap();
        assert_eq!(
            mol.name_status(),
            NameStatus::Unverified(NameReason::UnauditedPath)
        );
        let mut parts = AssemblyParts::new(1);
        parts.fusion_plan = Some(plan);
        mark_rendered_parent_status(&mol, &parts);
        let status = NameStatus::NotPreferred(NameReason::UnregisteredRetainedComponent);
        assert_eq!(mol.name_status(), status);
        assert_eq!(
            serde_json::to_value(status).unwrap()["reason"],
            "unregistered_retained_component"
        );
    }

    #[test]
    fn fusion_layout_status_is_deferred_until_rendering() {
        use super::super::fusion::model::{FusionParentPlan, LayoutKind};
        for (kind, opsin_citation_order) in [
            (LayoutKind::OpsinEntry, false),
            (LayoutKind::OpsinConstruction, false),
            (LayoutKind::Intrinsic, true),
        ] {
            let mol = Molecule::new();
            let mut plan = FusionParentPlan::default();
            plan.numbering.selected_layout.kind = kind;
            plan.ast.opsin_citation_order = opsin_citation_order;
            let mut parts = AssemblyParts::new(1);
            parts.fusion_plan = Some(plan);
            assert_eq!(
                mol.name_status(),
                NameStatus::Unverified(NameReason::UnauditedPath)
            );
            mark_rendered_parent_status(&mol, &parts);
            assert_eq!(
                mol.name_status(),
                NameStatus::Unverified(NameReason::OpsinFusionLayout)
            );
        }
    }

    #[test]
    fn repeated_base_fusion_status_is_deferred_until_rendering() {
        use super::super::{fusion::planner::plan_fusion_parent, graph_io::read_smiles};
        let mol = read_smiles("o1ccc2oc3ccoc3c12").unwrap();
        let atoms = mol.atoms.keys().copied().collect();
        let plan = plan_fusion_parent(&mol, &atoms).unwrap().unwrap();
        assert_eq!(plan.rendered_base_name, "difuro[3,2-b:2',3'-d]furan");
        assert_eq!(
            mol.name_status(),
            NameStatus::Unverified(NameReason::UnauditedPath)
        );
        let mut parts = AssemblyParts::new(atoms.len());
        parts.fusion_plan = Some(plan);
        mark_rendered_parent_status(&mol, &parts);
        assert_eq!(
            mol.name_status(),
            NameStatus::Unverified(NameReason::FusionMultiparent)
        );
    }

    #[test]
    fn complete_hydrogenation_veto_keeps_unlocanted_and_partial_controls() {
        for (smiles, text, status) in [
            (
                "O1CCC2CCCCC2C1",
                "3,4,4a,5,6,7,8,8a-octahydro-1H-2-benzopyran",
                NameStatus::Unverified(NameReason::HydroPrefixLocants),
            ),
            (
                "C1CCC2CCCCC2C1",
                "decahydronaphthalene",
                NameStatus::Preferred,
            ),
            (
                "O=C1CCOc2ccccc12",
                "2,3-dihydro-4H-1-benzopyran-4-one",
                NameStatus::Preferred,
            ),
        ] {
            let name = name_smiles(smiles).unwrap();
            assert_eq!(name.text, text, "{smiles}");
            assert_eq!(name.status, status, "{smiles}");
        }
    }

    #[test]
    fn rendered_units_veto_unaudited_parent_choices() {
        for (smiles, text, status) in [
            (
                "C1CC2C3CCC1C2C3",
                "tricyclo[4.3.0.0^{3,9}]nonane",
                NameStatus::Unverified(NameReason::UnauditedParent),
            ),
            (
                "C1CCCCCOCCCCC1",
                "1-oxacyclododecane",
                NameStatus::Unverified(NameReason::UnauditedParent),
            ),
            (
                "C1CC2CCC1C2",
                "bicyclo[2.2.1]heptane",
                NameStatus::Preferred,
            ),
            (
                "C1CCC2(CC1)CCCC2",
                "spiro[4.5]decane",
                NameStatus::Preferred,
            ),
            (
                "C1CCC2(CC1)OCCO2",
                "1,4-dioxaspiro[4.5]decane",
                NameStatus::Preferred,
            ),
        ] {
            let name = name_smiles(smiles).unwrap();
            assert_eq!(name.text, text, "{smiles}");
            assert_eq!(name.status, status, "{smiles}");
        }
    }

    #[test]
    fn relative_ring_stereo_is_independent_of_smiles_neighbor_order() {
        for (smiles, expected, alternative) in [
            (
                "C[C@H]1C[C@@H](C)C1",
                "(1s,3s)-1,3-dimethylcyclobutane",
                "cis-1,3-dimethylcyclobutane",
            ),
            (
                "C[C@H]1C[C@H](C1)C",
                "(1s,3s)-1,3-dimethylcyclobutane",
                "cis-1,3-dimethylcyclobutane",
            ),
            (
                "C[C@H]1C[C@H](C)C1",
                "(1r,3r)-1,3-dimethylcyclobutane",
                "trans-1,3-dimethylcyclobutane",
            ),
            (
                "C[C@H]1C[C@@H](C1)C",
                "(1r,3r)-1,3-dimethylcyclobutane",
                "trans-1,3-dimethylcyclobutane",
            ),
        ] {
            assert_relative_ring_name(smiles, expected, alternative);
        }
    }

    fn assert_relative_ring_name(smiles: &str, expected: &str, alternative: &str) {
        let name = name_smiles(smiles).unwrap();
        assert_eq!(name.text, expected, "{smiles}");
        // The primary name keeps the rest of its rule path's status.
        assert_ne!(
            name.status,
            NameStatus::NotPreferred(NameReason::RelativeRingStereo),
            "{smiles}"
        );
        assert_eq!(
            name.alternatives.len(),
            1,
            "{smiles}: {:?}",
            name.alternatives
        );
        assert_eq!(name.alternatives[0].name, alternative, "{smiles}");
        assert_eq!(
            name.alternatives[0].status,
            NameStatus::NotPreferred(NameReason::RelativeRingStereo),
            "{smiles}"
        );
        let identity = crate::naming::canonical_smiles(smiles).unwrap();
        for text in std::iter::once(&name.text).chain(name.alternatives.iter().map(|n| &n.name)) {
            let request = crate::naming::local::parse_request(text).unwrap();
            let reply = crate::naming::worker::respond(&request).unwrap();
            let parsed = crate::naming::local::parse_result(text, &reply).unwrap();
            assert_eq!(
                identity,
                crate::naming::canonical_smiles(&parsed.smiles).unwrap(),
                "{smiles}: {text}"
            );
        }
    }

    #[test]
    fn relative_ring_diols_and_tert_butyl_alcohol_use_canvas_cip_labels() {
        for (smiles, expected, alternative) in [
            (
                "O[C@H]1C[C@@H](O)C1",
                "(1s,3s)-cyclobutane-1,3-diol",
                "cis-cyclobutane-1,3-diol",
            ),
            (
                "O[C@H]1C[C@H](O)C1",
                "(1r,3r)-cyclobutane-1,3-diol",
                "trans-cyclobutane-1,3-diol",
            ),
            (
                "O[C@H]1CC[C@@H](O)CC1",
                "(1s,4s)-cyclohexane-1,4-diol",
                "cis-cyclohexane-1,4-diol",
            ),
            (
                "O[C@H]1CC[C@H](O)CC1",
                "(1r,4r)-cyclohexane-1,4-diol",
                "trans-cyclohexane-1,4-diol",
            ),
            (
                "O[C@H]1CC[C@@H](C(C)(C)C)CC1",
                "(1s,4s)-4-(tert-butyl)cyclohexan-1-ol",
                "cis-4-(tert-butyl)cyclohexan-1-ol",
            ),
            (
                "O[C@H]1CC[C@H](C(C)(C)C)CC1",
                "(1r,4r)-4-(tert-butyl)cyclohexan-1-ol",
                "trans-4-(tert-butyl)cyclohexan-1-ol",
            ),
        ] {
            assert_relative_ring_name(smiles, expected, alternative);
            let mol = super::super::graph_io::read_smiles(smiles).unwrap();
            for (&idx, label) in &mol.accurate_cip {
                assert_eq!(
                    mol.atoms[&idx].stereo.as_deref(),
                    Some(label.as_str()),
                    "{smiles}: atom {idx}"
                );
            }
        }
    }

    #[test]
    fn relative_ring_substituents_keep_cip_and_cis_trans_alternatives() {
        assert_relative_ring_name(
            "CC(=O)N[C@H]1C[C@@H](C)C1",
            "N-[(1s,3s)-3-methylcyclobutyl]acetamide",
            "N-(cis-3-methylcyclobutyl)acetamide",
        );
    }

    #[test]
    fn relative_ring_fallback_is_explicitly_not_preferred() {
        let mut mol = super::super::graph_io::read_smiles("C[C@H]1C[C@@H](C)C1").unwrap();
        for atom in mol.atoms.values_mut() {
            atom.stereo = None;
        }
        mol.accurate_cip.clear();
        let path = [1, 2, 3, 5];
        let locants = BTreeMap::from([
            (1, "1".into()),
            (2, "2".into()),
            (3, "3".into()),
            (5, "4".into()),
        ]);
        let mut parts = AssemblyParts::new(path.len());
        parts.is_ring = true;
        parts.parent_atom_ids = path.into_iter().collect();
        add_relative_ring_stereo(&mol, &mut parts, &path, &locants).unwrap();
        assert_eq!(parts.relative_stereo_prefixes, ["cis"]);
        assert!(parts.stereo_features.is_empty());
        assert_eq!(
            mol.name_status(),
            NameStatus::NotPreferred(NameReason::RelativeRingStereo)
        );
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
            assert_eq!(
                name_smiles(smiles).map(|name| name.text).as_deref(),
                Ok(expected),
                "{smiles}"
            );
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
                "N-[3-aza-2-borabicyclo[3.1.0]hex-1(6)-en-5-yl]acetamide",
            ),
        ] {
            assert_eq!(
                name_smiles(smiles).map(|name| name.text).as_deref(),
                Ok(expected),
                "{smiles}"
            );
        }
    }

    #[test]
    fn von_baeyer_pseudoasymmetric_centres_use_canvas_cip_and_round_trip() {
        for (smiles, expected, pseudo_atoms) in [
            (
                "CN1[C@@H]2CC[C@H]1C[C@@H](O)C2",
                "(1R,3r,5S)-8-methyl-8-azabicyclo[3.2.1]octan-3-ol",
                vec![(7, "r")],
            ),
            (
                "CN1[C@@H]2CC[C@H]1C[C@H](O)C2",
                "(1R,3s,5S)-8-methyl-8-azabicyclo[3.2.1]octan-3-ol",
                vec![(7, "s")],
            ),
            (
                "N1[C@@H]2CC[C@H]1C[C@H](O)C2",
                "(1R,3s,5S)-8-azabicyclo[3.2.1]octan-3-ol",
                vec![(6, "s")],
            ),
            (
                "CN1[C@@H]2CC[C@H]1C[C@@H](OC(=O)[C@H](CO)c1ccccc1)C2",
                "(1R,3r,5S)-8-methyl-8-azabicyclo[3.2.1]octan-3-yl (2S)-3-hydroxy-2-phenylpropanoate",
                vec![(7, "r")],
            ),
            // RCSB CCD OW0 CACTVS SMILES retains the pseudoasymmetric C7.
            (
                "CN1[C@@H]2C[C@H](C[C@H]1[C@@H]3O[C@H]23)OC(=O)[C@H](CO)c4ccccc4",
                "(1R,2R,4S,5S,7s)-9-methyl-3-oxa-9-azatricyclo[3.3.1.0^{2,4}]nonan-7-yl (2S)-3-hydroxy-2-phenylpropanoate",
                vec![(4, "s")],
            ),
            (
                "O[C@H]1[C@@H]2CC[C@H]1C[C@@H](O)C2",
                "(1R,3r,5S,8r)-bicyclo[3.2.1]octane-3,8-diol",
                vec![(1, "r"), (7, "r")],
            ),
            (
                "O[C@@H]1[C@@H]2CC[C@H]1C[C@@H](O)C2",
                "(1R,3r,5S,8s)-bicyclo[3.2.1]octane-3,8-diol",
                vec![(1, "s"), (7, "r")],
            ),
            (
                "C12=C3C4=C1[C@@H]1[C@H]2[C@H]3N41",
                "(2S,3s,8R)-1-azapentacyclo[4.2.0.0^{3,8}.0^{4,7}.0^{2,5}]octa-4(7),5-diene",
                vec![(5, "s")],
            ),
        ] {
            let mol = super::super::graph_io::read_smiles(smiles).unwrap();
            for (atom, label) in pseudo_atoms {
                // graph_io uses the same full CIP labeler as the canvas.
                assert_eq!(
                    mol.accurate_cip.get(&atom).map(String::as_str),
                    Some(label),
                    "{smiles}: {atom}"
                );
            }
            let name = name_smiles(smiles).unwrap().text;
            assert_eq!(name, expected, "{smiles}");
            let request = crate::naming::local::parse_request(&name).unwrap();
            let reply = crate::naming::worker::respond(&request).unwrap();
            let parsed = crate::naming::local::parse_result(&name, &reply).unwrap();
            assert_eq!(
                crate::naming::canonical_smiles(smiles).unwrap(),
                crate::naming::canonical_smiles(&parsed.smiles).unwrap(),
                "{name}",
            );
        }
    }

    #[test]
    fn multicyclic_pseudoasymmetric_centres_round_trip() {
        for (smiles, expected) in [
            (
                "C1CC[C@H]2CCCCC[C@H](CC1)OCCC2",
                "(1r,7r)-13-oxabicyclo[5.5.4]hexadecane",
            ),
            ("C1C[C@H]2CC[C@@H]12", "(1s,4s)-bicyclo[2.2.0]hexane"),
            (
                "C[C@H]1C2(CC2)[C@@H](C)C13CC3",
                "(4s,8s)-4,8-dimethyldispiro[2.1.2.1]octane",
            ),
            (
                "C[C@H]1C2(CC2)[C@H](C)C13CC3",
                "(4r,8r)-4,8-dimethyldispiro[2.1.2.1]octane",
            ),
        ] {
            let name = name_smiles(smiles).unwrap().text;
            assert_eq!(name, expected, "{smiles}");
            let request = crate::naming::local::parse_request(&name).unwrap();
            let reply = crate::naming::worker::respond(&request).unwrap();
            let parsed = crate::naming::local::parse_result(&name, &reply).unwrap();
            assert_eq!(
                crate::naming::canonical_smiles(smiles).unwrap(),
                crate::naming::canonical_smiles(&parsed.smiles).unwrap(),
                "{name}",
            );
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
            name_smiles("CC1C(F)CCCC2CCCC=CCC1C1CC(CC2)C1")
                .map(|name| name.text)
                .as_deref(),
            Ok("17-fluoro-18-methyltricyclo[6.5.5.1^{9,11}]nonadec-5-ene"),
        );
    }

    #[test]
    fn external_double_bond_uses_modern_cip_priority() {
        let smiles = "C/C=C\\C(=C/C)[C@H](C)/C=C\\C";
        let name = name_smiles(smiles).unwrap().text;
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
