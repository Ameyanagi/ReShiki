// SPDX-License-Identifier: MIT OR Apache-2.0
// Origin: original nazuchem/ReShiki code.
//   ReShiki, src/naming/openclatura/heterospiro.rs @ 5f559fd482e237307a589dfcb7ea333cee0a6e13.
// Copyright (c) 2026 Ameyanagi and ReShiki contributors

//! P-24.5: hydrogen and suffix composition of two different spiro components.
use super::{
    assembly_parts::{AssemblyParts, HydroOperation},
    assembly_spiro::{spiro_component, spiro_component_sort_key},
    molecule::{AtomSet, Molecule},
    spirobi::{Component, completed_graph, component, prime_locant, project},
};
use std::collections::BTreeMap;

pub(super) fn has_nonstandard_bonding(mol: &Molecule, atoms: &AtomSet) -> bool {
    atoms.iter().any(|&id| {
        let atom = &mol.atoms[&id];
        atom.charge == 0
            && atom.element().is_some_and(|element| {
                atom.total_h_count
                    + mol
                        .get_neighbors(id)
                        .iter()
                        .map(|&neighbor| mol.bond_order(id, neighbor))
                        .sum::<usize>()
                    > element.standard_valence
            })
    })
}

fn identity(mol: &Molecule, parts: &AssemblyParts) -> Result<Option<(String, bool)>, String> {
    match component(parts)? {
        Some(Component::Fused(name)) => {
            let indicated: Vec<_> = super::additive::name_indicated_hydrogen_locants(Some(&name))
                .into_iter()
                .collect();
            Ok(Some((
                super::retained_name_policy::render_retained_hydrogen_state(
                    &name,
                    &indicated,
                    Some(&[]),
                ),
                true,
            )))
        }
        Some(Component::VonBaeyer(_)) => Ok(None),
        None if parts.is_simple_ring() => {
            // The side projection can use skeletal replacements internally;
            // recover the retained/Hantzsch–Widman citation from its ring graph.
            if !parts.a_prefixes.is_empty() {
                let mut numbered: Vec<_> = parts.parent_atom_ids_by_locant.iter().collect();
                numbered.sort_by_key(|(l, _)| super::assembly_utils::parse_locant(l));
                let path: Vec<_> = numbered.into_iter().map(|(_, &id)| id).collect();
                return Ok(super::rules::retained::get_retained_ring(mol, &path)
                    .map(|parent| (parent.name, false)));
            }
            let mut parent = parts.clone();
            parent.principal_group = None;
            let (stem, terminal) = super::assembly_parent::parent_stem_and_terminal(&parent)?;
            let tail = super::assembly_parent::format_parent_tail(&parent, &stem, &terminal)?;
            Ok(Some((
                format!("{}{}{}", tail.stem, tail.unsaturation, tail.terminal),
                false,
            )))
        }
        None => Ok(None),
    }
}

pub(super) fn compose(
    mol: &Molecule,
    parts: &AssemblyParts,
) -> Result<Option<(String, String)>, String> {
    let sides: Vec<_> = parts
        .substituents
        .iter()
        .filter_map(|s| s.spiro.as_deref())
        .collect();
    let [side] = sides.as_slice() else {
        return Ok(None);
    };
    let Some(other) = side.side_parts.as_deref() else {
        return Ok(None);
    };
    if side.continuation.is_some() || other.substituents.iter().any(|s| s.spiro.is_some()) {
        return Ok(None);
    }
    let (Some(first), Some(second)) = (identity(mol, parts)?, identity(mol, other)?) else {
        return Ok(None);
    };
    if first.0 == second.0 || !(first.1 || second.1) {
        return Ok(None);
    }
    let shared: Vec<_> = parts
        .parent_atom_ids
        .intersection(&other.parent_atom_ids)
        .copied()
        .collect();
    let [junction] = shared.as_slice() else {
        return Ok(None);
    };
    let junction = *junction;
    if super::spiro_subgraph::heterogeneous_spiro_junction(mol) != Some(junction) {
        return Ok(None);
    }
    for c in [parts, other] {
        if c.is_substituent
            || !c.principal_suffix_modifiers.is_empty()
            || c.principal_group
                .as_ref()
                .is_some_and(|g| g.key != "ketone")
            || !c.stereo_features.is_empty()
            || !c.endo_exo_descriptors.is_empty()
            || !c.relative_stereo_prefixes.is_empty()
            || !c.front_modifiers.is_empty()
        {
            return Ok(None);
        }
    }
    let principal_key = parts
        .principal_group
        .as_ref()
        .or(other.principal_group.as_ref())
        .map(|group| group.key.as_str());
    let groups: Vec<_> = super::perception::perceive_groups(mol)
        .into_iter()
        .filter(|group| Some(group.key.as_str()) == principal_key)
        .collect();
    let mut components = [(parts, first), (other, second)];
    components.sort_by_key(|(_, (name, _))| spiro_component_sort_key(name));
    let mut projected = AssemblyParts::new(parts.parent_length + other.parent_length - 1);
    projected.is_ring = true;
    projected.omit_redundant_locants = false;
    projected.spiro_prefix_composition = true;
    let mut names = Vec::new();
    let mut junctions = Vec::new();
    let mut hydrogen_allocations = 0;
    for (index, (c, (name, fused))) in components.iter().enumerate() {
        let map: BTreeMap<_, _> = c
            .parent_atom_ids_by_locant
            .iter()
            .map(|(l, &id)| {
                (
                    id,
                    if index == 0 {
                        l.clone()
                    } else {
                        prime_locant(l)
                    },
                )
            })
            .collect();
        if map.keys().copied().collect::<AtomSet>() != c.parent_atom_ids {
            return Ok(None);
        }
        let Some(mut local) = project(mol, c, &map, false) else {
            return Ok(None);
        };
        // The monocycle citation already contains its skeletal heteroatoms.
        local.a_prefixes.clear();
        super::spirobi::promote_suffixes(mol, &mut local, &groups, &[&map]);
        if *fused {
            let suffix: AtomSet = map
                .iter()
                .filter(|(_, locant)| {
                    local
                        .principal_group
                        .as_ref()
                        .is_some_and(|g| g.locants.contains(locant))
                })
                .map(|(&id, _)| id)
                .collect();
            let graph = completed_graph(mol, &c.parent_atom_ids, &map, junction);
            let Some((indicated, _, state)) = super::additive::graph_suffix_hydrogen_state(
                mol,
                &graph,
                &suffix,
                &suffix,
                &map,
                Some(junction),
            )?
            else {
                return Ok(None);
            };
            if !state.bond_delta.compatible || !state.unsaturation_operations.is_empty() {
                return Ok(None);
            }
            if !suffix.is_empty() || !indicated.is_empty() {
                hydrogen_allocations += 1;
                if hydrogen_allocations > 1 {
                    // P-58.2.3.1 must rank these sites across the completed
                    // parent; independent component allocations are insufficient.
                    return Ok(None);
                }
            }
            projected.hydro_operations.push(HydroOperation {
                key: "indicated_hydrogen".into(),
                atom_ids: indicated
                    .iter()
                    .filter_map(|l| map.iter().find(|(_, v)| *v == l).map(|(&id, _)| id))
                    .collect(),
                locants: indicated,
                operation_kind: "indicated_hydrogen".into(),
                ..Default::default()
            });
            projected.hydro_operations.extend(state.hydro_operations);
            projected
                .hydro_operations
                .extend(state.bond_delta.added_hydrogen_operations);
        }
        projected.substituents.append(&mut local.substituents);
        if let Some(group) = local.principal_group {
            if let Some(principal) = &mut projected.principal_group {
                if principal.key != group.key {
                    return Ok(None);
                }
                principal.locants.extend(group.locants);
                principal.atom_ids.extend(group.atom_ids);
            } else {
                projected.principal_group = Some(group);
            }
        }
        junctions.push(map[&junction].clone());
        names.push(spiro_component(name));
    }
    let mut suffix = String::new();
    if projected.principal_group.is_some() {
        let (terminal, rendered, _) =
            super::assembly_parent::format_principal_suffix(&projected, "e")?;
        if terminal.is_empty() && names[1].ends_with('e') {
            names[1].pop();
        }
        suffix = rendered;
    }
    let core = format!(
        "spiro[{}-{},{}-{}]",
        names[0], junctions[0], junctions[1], names[1]
    );
    super::assembler::assemble_completed_parent(&mut projected, &core, &suffix).map(Some)
}

/// Component-local H and mancude ketones require a completed-parent proof.
pub(super) fn needs_hydrogen_proof(mol: &Molecule, parts: &AssemblyParts) -> bool {
    let component_needs_proof = |c: &AssemblyParts| {
        has_nonstandard_bonding(mol, &c.parent_atom_ids)
            || (c.is_simple_ring() && !c.unsaturations.is_empty())
            || !c.hydro_operations.is_empty()
            || !c.indicated_hydrogens.is_empty()
            || c.retained_name.as_deref().is_some_and(|n| {
                n.contains("hydro")
                    || !super::additive::name_indicated_hydrogen_locants(Some(n)).is_empty()
            })
            || ((c.fusion_plan.is_some()
                || c.retained_parent_metadata
                    .as_ref()
                    .is_some_and(|m| m.mancude_double_bonds > 0))
                && super::perception::perceive_groups(mol).iter().any(|group| {
                    group.key == "ketone" && c.parent_atom_ids.contains(&group.attachment_carbon)
                }))
    };
    parts
        .substituents
        .iter()
        .filter_map(|s| s.spiro.as_deref())
        .any(|s| {
            if s.continuation.is_none()
                && let (Ok(Some(first)), Some(other)) = (component(parts), s.side_parts.as_deref())
                && component(other)
                    .ok()
                    .flatten()
                    .is_some_and(|second| first == second)
            {
                // Identical-component fallbacks retain their existing assessment.
                return false;
            }
            component_needs_proof(parts)
                || s.side_parts
                    .as_deref()
                    .is_some_and(|c| component_needs_proof(c) || needs_hydrogen_proof(mol, c))
        })
}
