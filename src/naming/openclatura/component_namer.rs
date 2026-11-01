//! The plain-name component pipeline, retaining upstream decision order.
use super::{
    assembly_parts::SubstituentItem,
    chains::{find_all_carbon_paths, find_ring_systems, get_cyclic_atoms},
    component_group_rules::{
        exclude_nonparent_group_atoms, principal_involved_atoms, retarget_external_carbonyl_groups,
    },
    component_modifiers::{add_component_front_modifiers, add_component_n_substituents},
    functional_prefixes::collect_component_prefix_substituents,
    molecule::{AtomSet, Molecule, bond_ids_within},
    namer::name_subgraph_rendered,
    parent_pipeline::{
        ParentAssemblyPlan, build_parent_assembly_plan, build_parent_assembly_plan_with_parent,
        resolve_retained_parent,
    },
    parent_selection::select_principal_parent,
    principal_groups::{
        add_component_principal_group, component_groups, component_principal_key,
        partition_principal_and_prefix_groups,
    },
    special_cases::{
        blocking_suffix_defers_to_chain_parent, single_atom_component_name,
        structural_replacement_parent_result, try_name_anhydride_component,
    },
    subgraph_tools::{
        add_indicated_hydrogens, add_parent_features, emit_bond_stereo, subgraph_component,
    },
};
use std::collections::BTreeMap;

pub(super) fn name_component(mol: &Molecule, component_atoms: &AtomSet) -> Result<String, String> {
    name_component_with_citation(mol, component_atoms, None)
}

pub(super) fn name_component_with_citation(
    mol: &Molecule,
    component_atoms: &AtomSet,
    citation: Option<&str>,
) -> Result<String, String> {
    match component_pipeline(mol, component_atoms, citation, None)? {
        ComponentResult::Shortcut(name) => Ok(name),
        ComponentResult::Plan(mut plan) => {
            render_component_parent(mol, &mut plan.parts, &plan.numbered_path, &plan.locant_map)
        }
    }
}

struct SpiroProjection<'a> {
    junction: usize,
    required_core: Option<&'a AtomSet>,
    prefix_only: bool,
}
enum ComponentResult {
    Shortcut(String),
    Plan(Box<ParentAssemblyPlan>),
}

pub(super) fn build_spiro_component_parts(
    mol: &Molecule,
    component_atoms: &AtomSet,
    junction: usize,
    required_core: Option<&AtomSet>,
    prefix_only: bool,
) -> Result<Option<ParentAssemblyPlan>, String> {
    mol.candidate(|| {
        Ok(
            match component_pipeline(
                mol,
                component_atoms,
                None,
                Some(SpiroProjection {
                    junction,
                    required_core,
                    prefix_only,
                }),
            )? {
                ComponentResult::Shortcut(_) => None,
                ComponentResult::Plan(plan) => Some(*plan),
            },
        )
    })
}

fn component_pipeline(
    mol: &Molecule,
    component_atoms: &AtomSet,
    citation: Option<&str>,
    projection: Option<SpiroProjection<'_>>,
) -> Result<ComponentResult, String> {
    let _depth = mol.enter_naming()?;
    let as_prefix = projection.as_ref().is_some_and(|p| {
        p.prefix_only
            || p.required_core.is_some()
            || super::spiro_subgraph::polycyclic_side_core(mol, component_atoms, p.junction)
                .is_some()
    });
    if component_atoms.is_empty() {
        return Err("openclatura: empty component".into());
    }
    if let Some(name) = single_atom_component_name(mol, component_atoms) {
        return Ok(ComponentResult::Shortcut(name));
    }
    let mut groups = component_groups(mol, component_atoms);
    let mut key = component_principal_key(&groups, as_prefix);
    let blocks = groups
        .iter()
        .filter(|g| Some(g.key.as_str()) == key.as_deref())
        .any(|g| {
            g.resolved_rule
                .as_ref()
                .is_some_and(|r| r.has_capability("blocks_structural_parent"))
        });
    let functional_defer = key.is_some()
        && groups.iter().filter(|g| g.is_principal_candidate).any(|p| {
            groups
                .iter()
                .filter(|g| {
                    !g.is_principal_candidate
                        && g.descriptor.as_ref().is_some_and(|d| {
                            matches!(d.family.as_str(), "peroxide" | "nitrogen_chalcogenide")
                        })
                })
                .any(|g| p.atom_ids().is_disjoint(&g.atom_ids()))
        });
    let status = mol.name_status();
    let structural = if functional_defer {
        None
    } else {
        structural_replacement_parent_result(mol, component_atoms)?
    };
    let defers = blocking_suffix_defers_to_chain_parent(
        mol,
        component_atoms,
        &groups,
        key.as_deref(),
        structural.as_ref(),
    );
    if (!blocks || defers)
        && let Some(result) = structural
    {
        return Ok(ComponentResult::Shortcut(result.name));
    }
    mol.set_name_status(status);
    if let Some(name) = try_name_anhydride_component(mol, &groups, key.as_deref())? {
        return Ok(ComponentResult::Shortcut(name));
    }
    // The key is the first of equally senior groups, such as carboxylic_acid
    // and ring_carboxylic_acid, not the P-44.1 count and ring-over-chain choice.
    let seniority = |g: &super::perception::PerceivedGroup| {
        g.resolved_rule.as_ref().and_then(|r| r.seniority.clone())
    };
    if let Some(selected) = groups
        .iter()
        .find(|g| Some(g.key.as_str()) == key.as_deref())
        && groups.iter().any(|g| {
            g.is_principal_candidate && g.key != selected.key && seniority(g) == seniority(selected)
        })
    {
        mol.mark_name(crate::naming::NameStatus::Unverified(
            crate::naming::NameReason::PrincipalGroupTie,
        ));
    }

    let base_exclude: AtomSet = mol
        .atoms
        .keys()
        .copied()
        .filter(|i| !component_atoms.contains(i))
        .collect();
    let mut exclude = base_exclude.clone();
    let cyclic = get_cyclic_atoms(mol, &AtomSet::new());
    retarget_external_carbonyl_groups(mol, &mut groups, key.as_deref(), &mut exclude, &cyclic);
    let (principal, _) = partition_principal_and_prefix_groups(&groups, key.as_deref());
    exclude_nonparent_group_atoms(mol, &groups, &mut exclude, &cyclic);
    let selection = if let Some(projection) = &projection {
        super::namer::select_subgraph_parent(mol, projection.junction, component_atoms, &exclude)?
    } else {
        let chains = find_all_carbon_paths(mol, &exclude)?;
        let rings = find_ring_systems(mol, &exclude)?;
        select_principal_parent(mol, &chains, &rings, &principal)
            .ok_or("openclatura: no supported parent skeleton")?
    };
    let path = selection.primary_path();
    let parent = selection.atom_set();
    groups.retain(|g| parent.contains(&g.attachment_carbon));
    key = component_principal_key(&groups, as_prefix);
    super::identity_validation::validate_principal_charge_roles(&groups, key.as_deref())?;
    let (principal, prefix_groups) = partition_principal_and_prefix_groups(&groups, key.as_deref());
    let retained_match = resolve_retained_parent(mol, &selection);
    let mut retained_name = retained_match.as_ref().map(|r| r.name.clone());
    let mut maps = retained_match.map(|r| r.locant_maps).unwrap_or_default();
    let principal_atoms = principal_involved_atoms(mol, &groups, key.as_deref(), path);
    let sub_exclude: AtomSet = base_exclude
        .iter()
        .chain(&parent)
        .chain(&principal_atoms)
        .copied()
        .collect();
    let (mut mapping, handled) =
        collect_component_prefix_substituents(mol, &prefix_groups, path, &sub_exclude)?;
    collect_component_branch_substituents(
        mol,
        path,
        &mut mapping,
        handled,
        &principal_atoms,
        &base_exclude,
        &sub_exclude,
    )?;
    let retained_fused = super::retained_fused_production::production_retained_fused_parent(
        mol,
        path,
        component_atoms,
        &groups,
        key.as_deref(),
        &mapping,
        None,
    )?;
    let mut retained_metadata = None;
    if let Some(retained) = retained_fused {
        retained_name = Some(retained.name);
        maps = retained.locant_maps;
        retained_metadata = Some(retained.metadata);
    } else if let Some(name) = &retained_name {
        if super::retained_fused_templates::retained_parent_metadata(name).is_some_and(|metadata| {
            !metadata.fusion_locants.is_empty()
                && metadata.indicated_hydrogen_count > 0
                && super::additive::name_indicated_hydrogen_locants(Some(name)).is_empty()
                && parent.iter().any(|id| {
                    let atom = &mol.atoms[id];
                    atom.charge > 0 && atom.total_h_count > 1
                })
        }) {
            // Upstream divergence: the neutral retained match is not a charge
            // proof. With an implicit donor H, the ium renderer suppresses its
            // citation and can redistribute H between heteroatoms. A rejected
            // production match must not restore that uncertified fallback.
            return Err("openclatura: charged retained donor hydrogen state not supported".into());
        }
        let context = if mapping.is_empty() && key.is_none() {
            "unsubstituted_parent"
        } else {
            "composite_parent"
        };
        retained_name = Some(super::retained_name_policy::retained_parent_output_name(
            name,
            context,
            &[],
            None,
        ));
    }
    if retained_name.is_some()
        && maps.is_empty()
        && selection.is_polycycle
        && (!mapping.is_empty() || !groups.is_empty())
    {
        retained_name = None;
    }
    let mut plan = if let Some(projection) = &projection {
        if !parent.contains(&projection.junction) {
            return Ok(ComponentResult::Shortcut(String::new()));
        }
        if let Some(core) = projection.required_core {
            if &parent != core {
                return Ok(ComponentResult::Shortcut(String::new()));
            }
            let Some(proof) =
                super::fusion::wrappers::plan_fusion_spiro_side(mol, core, projection.junction)?
            else {
                return Ok(ComponentResult::Shortcut(String::new()));
            };
            build_parent_assembly_plan_with_parent(
                mol,
                &selection,
                &principal,
                None,
                None,
                &mapping,
                retained_name,
                &maps,
                Some(proof.parent.hydride),
            )?
        } else {
            let mut numbering_mapping = mapping.clone();
            for atom in &principal {
                numbering_mapping
                    .entry(*atom)
                    .or_default()
                    .push(SubstituentItem::new("", vec![]));
            }
            if selection.is_ring
                && !selection.is_bicycle
                && !selection.is_polycycle
                && !selection.is_spiro
                && parent.iter().filter(|a| !mol.atoms[a].is_carbon()).count() > 1
                && mol
                    .bonds
                    .values()
                    .filter(|b| parent.contains(&b.u) && parent.contains(&b.v))
                    .all(|b| b.order == 1)
            {
                retained_name = None;
                maps.clear();
                retained_metadata = None;
            }
            build_parent_assembly_plan(
                mol,
                &selection,
                &[projection.junction],
                None,
                None,
                &numbering_mapping,
                retained_name,
                &maps,
            )?
        }
    } else {
        build_parent_assembly_plan(
            mol,
            &selection,
            &principal,
            None,
            None,
            &mapping,
            retained_name,
            &maps,
        )?
    };
    if retained_metadata.is_some() {
        plan.parts.retained_parent_metadata = retained_metadata;
    }
    let numbered = &plan.numbered_path;
    let locants = &plan.locant_map;
    let parts = &mut plan.parts;
    parts.principal_citation_mode = citation.map(str::to_owned);
    emit_bond_stereo(mol, parts, numbered, locants, None);
    let front =
        add_component_front_modifiers(mol, parts, &groups, key.as_deref(), &sub_exclude, locants)?;
    let n_subs = add_component_n_substituents(
        mol,
        parts,
        &groups,
        key.as_deref(),
        numbered,
        locants,
        &sub_exclude,
    )?;
    add_parent_features(mol, parts, numbered, locants)?;
    add_component_principal_group(
        mol,
        parts,
        &groups,
        key.as_deref(),
        &principal,
        numbered,
        locants,
    );
    add_indicated_hydrogens(mol, parts, numbered, locants)?;
    for &idx in numbered {
        for mut item in mapping.remove(&idx).unwrap_or_default() {
            item.locants.push(locants[&idx].clone());
            super::subgraph_tools::add_substituent(parts, item);
        }
    }
    if projection.is_some() {
        parts.omit_redundant_locants = false;
        parts.is_spiro_side_projection = true;
    }
    super::locant_elision::apply_redundant_locant_elision(parts);
    // Preserve the upstream complete-name requirement without its trace/audit machinery.
    let mut named = parent;
    named.extend(principal_atoms);
    named.extend(front);
    named.extend(n_subs);
    named.extend(
        parts
            .substituents
            .iter()
            .flat_map(|s| s.atom_ids.iter().copied()),
    );
    if let Some(principal) = &parts.principal_group {
        named.extend(&principal.atom_ids);
    }
    if let Some(missing) = component_atoms
        .iter()
        .find(|i| !named.contains(i) && mol.atoms[i].symbol != "H")
    {
        return Err(format!(
            "openclatura: atom {missing} ({}) not named",
            mol.atoms[missing].symbol
        ));
    }
    Ok(ComponentResult::Plan(Box::new(plan)))
}

pub(super) fn render_component_parent(
    mol: &Molecule,
    parts: &mut super::assembly_parts::AssemblyParts,
    numbered: &[usize],
    locants: &BTreeMap<usize, String>,
) -> Result<String, String> {
    let name = super::assembler::assemble_name_raw(parts)?;
    let represented: AtomSet = parts
        .a_prefixes
        .iter()
        .flat_map(|prefix| prefix.charge_atom_ids.iter().copied())
        .collect();
    let name = if parts
        .parent_hydride
        .as_ref()
        .map_or(parts.fusion_plan.is_some(), |parent| {
            parent.is_fusion_parent()
        }) {
        name
    } else {
        super::ionic_naming::apply_anionic_parent_names(
            &name,
            mol,
            numbered,
            locants,
            parts.retained_name.as_deref(),
            &represented,
        )?
    };
    let name =
        super::ionic_naming::apply_cationic_imino_parent_prefixes(&name, mol, numbered, locants)?;
    let name = super::ionic_naming::apply_cationic_imino_names(&name, mol)?;
    let name = super::assembler::post_process_name(&name);
    super::assembler::mark_assembled_name(mol, parts, &name);
    Ok(name)
}

fn collect_component_branch_substituents(
    mol: &Molecule,
    path: &[usize],
    mapping: &mut BTreeMap<usize, Vec<SubstituentItem>>,
    mut handled: AtomSet,
    principal: &AtomSet,
    base_exclude: &AtomSet,
    sub_exclude: &AtomSet,
) -> Result<(), String> {
    let main: AtomSet = path.iter().copied().collect();
    for &idx in path {
        let roots: Vec<_> = mol
            .get_neighbors(idx)
            .iter()
            .copied()
            .filter(|root| {
                !main.contains(root)
                    && !principal.contains(root)
                    && !handled.contains(root)
                    && !base_exclude.contains(root)
            })
            .collect();
        if let Some((start, _)) =
            super::spiro_subgraph::find_spiro_side_pair(mol, idx, &roots, &main, base_exclude)
        {
            let atoms =
                super::spiro_subgraph::spiro_side_component(mol, idx, start, &main, base_exclude);
            let mut item = SubstituentItem::new("", vec![]);
            item.atom_ids = atoms.iter().copied().filter(|a| *a != idx).collect();
            item.bond_ids = bond_ids_within(mol, &atoms);
            item.charge_atom_ids = super::molecule::charged_atoms(mol, &item.atom_ids);
            item.spiro = Some(Box::new(super::spiro_subgraph::spiro_subgraph_assembly(
                mol, idx, &atoms,
            )?));
            handled.extend(&item.atom_ids);
            mapping.entry(idx).or_default().push(item);
        }
        for root in roots {
            if handled.contains(&root) {
                continue;
            }
            let atoms = subgraph_component(mol, root, sub_exclude);
            super::spiro_subgraph::require_single_parent_attachment(mol, &atoms, &main)?;
            let branch = name_subgraph_rendered(mol, root, sub_exclude, Some(idx))?;
            if branch.text.is_empty() {
                return Err("openclatura: empty branch prefix".into());
            }
            let mut bond_atoms = atoms.clone();
            bond_atoms.insert(idx);
            let mut item = SubstituentItem::new(branch.text, vec![]);
            item.outer_parentheses_optional = branch.outer_parentheses_optional;
            item.bond_ids = bond_ids_within(mol, &bond_atoms);
            item.charge_atom_ids = atoms
                .iter()
                .copied()
                .filter(|i| mol.atoms[i].charge != 0)
                .collect();
            item.atom_ids = atoms;
            mapping.entry(idx).or_default().push(item);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn charged_retained_fallback_requires_citable_donor_hydrogen() {
        for smiles in ["C1=N[NH2+]c2ccccc21", "C1=Nc2ccccc2[NH2+]1"] {
            assert!(super::super::name_smiles(smiles).is_err(), "{smiles}");
        }
        for (smiles, name) in [
            ("c1ccc2[nH+]cccc2c1", "quinolin-1-ium"),
            ("c1ccc2[n-]ncc2c1", "indazol-1-ide"),
            ("C1=Cc2ccccc2[NH2+]1", "1H-indol-1-ium"),
        ] {
            assert_eq!(
                super::super::name_smiles(smiles)
                    .map(|name| name.text)
                    .as_deref(),
                Ok(name)
            );
        }
    }
}
