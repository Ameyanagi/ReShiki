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
    parent_pipeline::build_parent_assembly_plan,
    parent_selection::select_principal_parent,
    principal_groups::{
        add_component_principal_group, component_groups, component_principal_key,
        partition_principal_and_prefix_groups,
    },
    rules::retained,
    special_cases::{
        blocking_suffix_defers_to_chain_parent, single_atom_component_name,
        structural_replacement_parent_result, try_name_anhydride_component,
    },
    subgraph_tools::{
        add_indicated_hydrogens, add_parent_features, emit_bond_stereo, subgraph_component,
    },
};
use std::collections::BTreeMap;

pub fn name_component(mol: &Molecule, component_atoms: &AtomSet) -> Result<String, String> {
    name_component_with_citation(mol, component_atoms, None)
}

pub fn name_component_with_citation(
    mol: &Molecule,
    component_atoms: &AtomSet,
    citation: Option<&str>,
) -> Result<String, String> {
    if component_atoms.is_empty() {
        return Err("openclatura: empty component".into());
    }
    if let Some(name) = single_atom_component_name(mol, component_atoms) {
        return Ok(name);
    }
    let mut groups = component_groups(mol, component_atoms);
    let mut key = component_principal_key(&groups, false);
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
        return Ok(result.name);
    }
    if let Some(name) = try_name_anhydride_component(mol, &groups, key.as_deref())? {
        return Ok(name);
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
    let chains = find_all_carbon_paths(mol, &exclude);
    let rings = find_ring_systems(mol, &exclude);
    let selection = select_principal_parent(mol, &chains, &rings, &principal)
        .ok_or("openclatura: no supported parent skeleton")?;
    let path = selection.primary_path();
    let parent = selection.atom_set();
    groups.retain(|g| parent.contains(&g.attachment_carbon));
    key = component_principal_key(&groups, false);
    let (principal, prefix_groups) = partition_principal_and_prefix_groups(&groups, key.as_deref());
    let retained_match = selection
        .is_ring
        .then(|| retained::get_retained_ring(mol, path))
        .flatten();
    let retained_name = retained_match.as_ref().map(|r| r.name.clone());
    let maps = retained_match.map(|r| r.locant_maps).unwrap_or_default();
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
        &handled,
        &principal_atoms,
        &base_exclude,
        &sub_exclude,
    )?;
    let mut plan = build_parent_assembly_plan(
        mol,
        &selection,
        &principal,
        None,
        None,
        &mapping,
        retained_name,
        &maps,
    )?;
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
    add_parent_features(mol, parts, numbered, locants);
    add_component_principal_group(
        mol,
        parts,
        &groups,
        key.as_deref(),
        &principal,
        numbered,
        locants,
    );
    add_indicated_hydrogens(mol, parts, numbered, locants);
    for &idx in numbered {
        for mut item in mapping.remove(&idx).unwrap_or_default() {
            item.locants.push(locants[&idx].clone());
            super::subgraph_tools::add_substituent(parts, item);
        }
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
    let name = super::assembler::assemble_name_raw(parts)?;
    let name = super::ionic_naming::apply_anionic_parent_names(
        &name,
        mol,
        numbered,
        locants,
        parts.retained_name.as_deref(),
    )?;
    let name =
        super::ionic_naming::apply_cationic_imino_parent_prefixes(&name, mol, numbered, locants)?;
    let name = super::ionic_naming::apply_cationic_imino_names(&name, mol)?;
    Ok(super::assembler::post_process_name(&name))
}

#[allow(clippy::too_many_arguments)]
fn collect_component_branch_substituents(
    mol: &Molecule,
    path: &[usize],
    mapping: &mut BTreeMap<usize, Vec<SubstituentItem>>,
    handled: &AtomSet,
    principal: &AtomSet,
    base_exclude: &AtomSet,
    sub_exclude: &AtomSet,
) -> Result<(), String> {
    let main: AtomSet = path.iter().copied().collect();
    for &idx in path {
        for &root in mol.get_neighbors(idx) {
            if main.contains(&root)
                || principal.contains(&root)
                || handled.contains(&root)
                || base_exclude.contains(&root)
            {
                continue;
            }
            let branch = name_subgraph_rendered(mol, root, sub_exclude, Some(idx))?;
            if branch.text.is_empty() {
                return Err("openclatura: empty branch prefix".into());
            }
            let atoms = subgraph_component(mol, root, sub_exclude);
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
