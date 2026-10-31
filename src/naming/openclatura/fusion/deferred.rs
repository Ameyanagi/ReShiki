//! Positive applicability proofs at the deferred round-three naming seams.
//!
//! These predicates never emit a wrapper or replacement-fusion name. They use
//! the same parent, component, and bond-state proofs as the corresponding
//! upstream route so a legacy fallback cannot silently replace such a route.
use super::super::{
    graph_kernel::connected_components,
    locants::retained_locant_sort_key as locant_key,
    molecule::{AtomSet, Molecule},
    polycycle_topology::edges_within_atoms,
    retained_fused_templates::{MatchOptions, match_retained_graph_templates_with_options},
    retained_graph_model::RetainedGraphTemplateMatch,
    retained_name_policy::retained_parent_name_policy,
};
use super::{
    composite_bridges::{composite_bridge_constructions, localized_carbon_bridge_bond_orders},
    config::fusion_nomenclature_config,
    cover::{audit_component_cover, component_scope},
    descriptor::{
        DescriptorOptions, build_fusion_name_ast, iter_fusion_name_asts, render_fusion_name,
    },
    faces::{cached_bounded_face_model, typed_face_model},
    indicated_hydrogen::component_h_locants,
    layout::preferred_intrinsic_layouts,
    mancude::{ParentDerivativeOptions, ParentDerivativeState, parent_derivative_state},
    model::{FusionParentPlan, ParentBondModel},
    numbering::retained_template_parent_bond_model,
    planner::{plan_fusion_parent, plan_numbered_candidate},
    registry::fusion_component_registry,
    replacement_state::{prove_replacement_state, replacement_state_scope},
    rules::fusion_ring_size_gate,
};
use std::collections::{BTreeMap, BTreeSet};

type WrapperParentModels = Vec<(BTreeMap<usize, String>, ParentBondModel)>;

fn deferred_carbon_planning_error(error: String) -> String {
    if error == "openclatura: localized nonaromatic fused pi state not ported" {
        // The cyclic component cover is already proved before carbonization.
        // A surrogate that cannot supply a validated standalone name still
        // belongs to this deferred composition seam; it cannot render here.
        "openclatura: systematic fusion not ported".into()
    } else {
        error
    }
}

pub fn require_no_deferred_parent(mol: &Molecule, atoms: &AtomSet) -> Result<(), String> {
    if third_component_applies(mol, atoms)? || bridged_wrapper_applies(mol, atoms)? {
        return Err("openclatura: systematic fusion not ported".into());
    }
    Ok(())
}

fn retained_matches(
    mol: &Molecule,
    atoms: &AtomSet,
    nonaromatic: bool,
    relocated: bool,
    pre_descriptor: bool,
) -> Vec<RetainedGraphTemplateMatch> {
    match_retained_graph_templates_with_options(
        mol,
        atoms,
        MatchOptions {
            family: Some("fused"),
            allow_nonaromatic: nonaromatic,
            allow_relocated_indicated_h: relocated,
            pre_descriptor_only: pre_descriptor,
            ..Default::default()
        },
    )
}

pub fn third_component_applies(mol: &Molecule, atoms: &AtomSet) -> Result<bool, String> {
    let replacements: Vec<_> = atoms
        .iter()
        .filter(|a| !mol.atoms[a].is_carbon())
        .copied()
        .collect();
    if replacements.is_empty()
        || atoms.iter().any(|a| mol.atoms[a].charge != 0)
        || replacements
            .iter()
            .any(|a| mol.atoms[a].element().is_none_or(|e| e.hw_stem.is_none()))
    {
        return Ok(false);
    }
    let Some(bounded) = cached_bounded_face_model(mol, atoms).ok().flatten() else {
        return Ok(false);
    };
    let registry = fusion_component_registry();
    let matches = registry.match_face_model(mol, &typed_face_model(mol, &bounded))?;
    let options = DescriptorOptions {
        cover_kinds: BTreeSet::from(["multiparent".into()]),
        join_kinds: ["ortho", "ortho_peri", "higher_order"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        enforce_interoperability_limits: false,
        ..Default::default()
    };
    let Ok(ast) = build_fusion_name_ast(mol, &matches, registry, &options) else {
        return Ok(false);
    };
    if ast.plan_kind != "cyclic_component_cover"
        || ast
            .citation_plan
            .as_ref()
            .is_none_or(|p| p.cycle_closing_join_indices.is_empty())
    {
        return Ok(false);
    }
    let scopes = ast
        .component_occurrences
        .iter()
        .map(|m| {
            let map = m.input_atom_by_locant();
            let spec = registry.spec_for_match(m)?;
            component_scope(
                m.occurrence_id,
                map.values().copied(),
                spec.bonds()
                    .iter()
                    .map(|bond| (map[&bond.locants.0], map[&bond.locants.1])),
            )
        })
        .collect::<Result<Vec<_>, String>>()?;
    let cover = audit_component_cover(&scopes, atoms, &edges_within_atoms(mol, atoms))?;
    if !cover.ok
        || !matches!(cover.proof.topology.as_str(), "unicyclic" | "cactus")
        || !fusion_ring_size_gate(
            &bounded
                .faces
                .iter()
                .map(|f| f.atoms.len())
                .collect::<Vec<_>>(),
        )
    {
        return Ok(false);
    }
    let mut carbon = mol.clone();
    for &id in atoms {
        let hydrogens = 4usize.saturating_sub(
            mol.get_neighbors(id)
                .iter()
                .filter_map(|&n| mol.get_bond(id, n))
                .map(|b| b.order)
                .sum(),
        );
        if let Some(atom) = carbon.atoms.get_mut(&id) {
            atom.symbol = "C".into();
            atom.charge = 0;
            atom.isotope = None;
            atom.explicit_h_count = 0;
            atom.total_h_count = hydrogens;
        }
    }
    let mut retained = retained_matches(&carbon, atoms, false, false, false);
    if retained.is_empty() {
        retained = retained_matches(&carbon, atoms, true, false, true);
    }
    if !retained.is_empty() {
        return Ok(true);
    }
    let Some(plan) = plan_fusion_parent(&carbon, atoms).map_err(deferred_carbon_planning_error)?
    else {
        return Ok(false);
    };
    if !replacement_state_scope(mol, atoms) {
        return Ok(true);
    }
    // An accepted variant is a member of upstream's joint state search. A
    // failed variant does not establish applicability or veto a legacy name.
    for variant in plan.numbering_variants.iter().chain(std::iter::once(&plan)) {
        if prove_replacement_state(mol, variant)?.is_some_and(|state| state.audit_ok()) {
            return Ok(true);
        }
    }
    let Some(carbon_bounded) = cached_bounded_face_model(&carbon, atoms)? else {
        return Ok(false);
    };
    let carbon_faces = typed_face_model(&carbon, &carbon_bounded);
    let carbon_matches = registry.match_face_model(&carbon, &carbon_faces)?;
    let first_core = render_fusion_name(&plan.ast, registry)?;
    let mut layouts = Some(preferred_intrinsic_layouts(&carbon_faces)?);
    let mut numbering_cache = BTreeMap::new();
    for candidate in iter_fusion_name_asts(
        &carbon,
        &carbon_matches,
        registry,
        &DescriptorOptions::default(),
    )? {
        let ast = candidate?;
        if render_fusion_name(&ast, registry)? != first_core {
            break;
        }
        let Some(candidate_plan) = plan_numbered_candidate(
            &carbon,
            atoms,
            &ast,
            registry,
            &carbon_bounded,
            &carbon_faces,
            &mut layouts,
            &mut numbering_cache,
        )
        .map_err(deferred_carbon_planning_error)?
        else {
            continue;
        };
        for variant in candidate_plan
            .numbering_variants
            .iter()
            .chain(std::iter::once(&candidate_plan))
        {
            if prove_replacement_state(mol, variant)?.is_some_and(|state| state.audit_ok()) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

#[derive(Clone)]
struct BridgePath {
    atoms: Vec<usize>,
    endpoints: (usize, usize),
    orders: Vec<usize>,
    carbon: bool,
}

pub fn bridged_wrapper_applies(mol: &Molecule, atoms: &AtomSet) -> Result<bool, String> {
    if atoms.iter().any(|a| mol.atoms[a].charge != 0) {
        return Ok(false);
    }
    let edges = edges_within_atoms(mol, atoms);
    if edges.len() < atoms.len() + 2 {
        return Ok(false);
    }
    let degrees: BTreeMap<_, _> = atoms
        .iter()
        .map(|&a| {
            (
                a,
                mol.get_neighbors(a)
                    .iter()
                    .filter(|n| atoms.contains(n))
                    .count(),
            )
        })
        .collect();
    if !degrees.values().any(|&d| d >= 3) {
        return Ok(false);
    }
    let segments = junction_path_interiors(mol, atoms, &degrees);
    if segments.is_empty() {
        return Ok(false);
    }
    let limit = fusion_nomenclature_config()
        .search
        .component_selection_states;
    let groups = removal_groups(&segments, limit);
    let mut visited = 0;
    for retained in [true, false] {
        for removed_group in groups.values() {
            let mut candidate_found = false;
            for removed in removed_group {
                visited += 1;
                if visited > limit {
                    return Ok(false);
                }
                let parent: AtomSet = atoms.difference(removed).copied().collect();
                let Some(paths) = bridge_paths(mol, atoms, &parent, removed) else {
                    continue;
                };
                let provisional_locants: BTreeMap<_, _> =
                    parent.iter().map(|&a| (a, (a + 1).to_string())).collect();
                if orient_bridges(mol, &paths, &parent, &provisional_locants).is_none() {
                    continue;
                }
                if retained {
                    for (locants, model) in retained_wrapper_models(mol, &parent)? {
                        let Some(operations) = orient_bridges(mol, &paths, &parent, &locants)
                        else {
                            continue;
                        };
                        let Some(state) = parent_derivative_state(
                            mol,
                            &parent,
                            &model,
                            &locants,
                            &ParentDerivativeOptions {
                                preserve_retained_parent_state: true,
                                allow_pi_redistribution: Some(true),
                                compose_retained_external_pi: true,
                                ..Default::default()
                            },
                        )?
                        else {
                            continue;
                        };
                        if wrapper_reconstruction(mol, atoms, &parent, &operations, &state, false) {
                            candidate_found = true;
                        }
                    }
                } else if let Some(plan) = plan_fusion_parent(mol, &parent)? {
                    for variant in plan.numbering_variants.iter().chain(std::iter::once(&plan)) {
                        let Some(locants) = variant
                            .numbering
                            .string_input_locant_maps()
                            .into_iter()
                            .next()
                        else {
                            continue;
                        };
                        let Some(operations) = orient_bridges(mol, &paths, &parent, &locants)
                        else {
                            continue;
                        };
                        let prefer_dehydro = prefers_bridge_dehydro(variant)?;
                        if wrapper_reconstruction(
                            mol,
                            atoms,
                            &parent,
                            &operations,
                            &variant.derivative_state,
                            prefer_dehydro,
                        ) {
                            candidate_found = true;
                        }
                    }
                }
            }
            if candidate_found {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn junction_path_interiors(
    mol: &Molecule,
    atoms: &AtomSet,
    degrees: &BTreeMap<usize, usize>,
) -> Vec<AtomSet> {
    let junctions: AtomSet = degrees
        .iter()
        .filter(|(_, d)| **d != 2)
        .map(|(&a, _)| a)
        .collect();
    let mut segments = BTreeSet::new();
    for &junction in &junctions {
        for &neighbor in mol
            .get_neighbors(junction)
            .iter()
            .filter(|n| atoms.contains(n) && !junctions.contains(n))
        {
            let mut path = AtomSet::new();
            let (mut previous, mut current) = (junction, neighbor);
            while !junctions.contains(&current) {
                if degrees.get(&current) != Some(&2) || !path.insert(current) {
                    path.clear();
                    break;
                }
                let next: Vec<_> = mol
                    .get_neighbors(current)
                    .iter()
                    .copied()
                    .filter(|&n| atoms.contains(&n) && n != previous)
                    .collect();
                let [following] = next.as_slice() else {
                    path.clear();
                    break;
                };
                (previous, current) = (current, *following);
            }
            if !path.is_empty() && current != junction {
                segments.insert(path);
            }
        }
    }
    let mut result: Vec<_> = segments.into_iter().collect();
    result.sort_by_key(|s| (s.len(), s.clone()));
    result
}

fn removal_groups(segments: &[AtomSet], limit: usize) -> BTreeMap<usize, BTreeSet<AtomSet>> {
    fn choose(
        segments: &[AtomSet],
        start: usize,
        left: usize,
        current: &AtomSet,
        generated: &mut usize,
        limit: usize,
        result: &mut BTreeMap<usize, BTreeSet<AtomSet>>,
    ) {
        if *generated > limit {
            return;
        }
        if left == 0 {
            *generated += 1;
            if *generated <= limit {
                result
                    .entry(current.len())
                    .or_default()
                    .insert(current.clone());
            }
            return;
        }
        for index in start..segments.len() {
            if segments.len() - index < left {
                break;
            }
            choose(
                segments,
                index + 1,
                left - 1,
                &current.union(&segments[index]).copied().collect(),
                generated,
                limit,
                result,
            );
            if *generated > limit {
                break;
            }
        }
    }
    let mut result = BTreeMap::new();
    let mut generated = 0;
    for count in 1..=segments.len() {
        choose(
            segments,
            0,
            count,
            &AtomSet::new(),
            &mut generated,
            limit,
            &mut result,
        );
        if generated > limit {
            break;
        }
    }
    result
}

fn bridge_paths(
    mol: &Molecule,
    all: &AtomSet,
    parent: &AtomSet,
    removed: &AtomSet,
) -> Option<Vec<Vec<usize>>> {
    let edges = edges_within_atoms(mol, all)
        .into_iter()
        .filter(|(u, v)| removed.contains(u) && removed.contains(v))
        .collect();
    let mut paths = Vec::new();
    for component in connected_components(removed, &edges) {
        let degrees: BTreeMap<_, _> = component
            .iter()
            .map(|&a| {
                (
                    a,
                    mol.get_neighbors(a)
                        .iter()
                        .filter(|n| component.contains(n))
                        .count(),
                )
            })
            .collect();
        if degrees.values().any(|&d| d > 2) {
            return None;
        }
        let attachments: AtomSet = component
            .iter()
            .flat_map(|&a| mol.get_neighbors(a).iter().copied())
            .filter(|a| parent.contains(a))
            .collect();
        if attachments.len() != 2 {
            return None;
        }
        let &start = degrees.iter().find(|(_, d)| **d <= 1)?.0;
        let mut path = vec![start];
        while path.len() < component.len() {
            let &next = mol
                .get_neighbors(*path.last()?)
                .iter()
                .filter(|n| component.contains(n) && !path.contains(n))
                .min()?;
            path.push(next);
        }
        paths.push(path);
    }
    paths.sort();
    Some(paths)
}

fn bridge_class(mol: &Molecule, path: &[usize]) -> Option<(bool, Vec<usize>)> {
    let symbols: Vec<_> = path.iter().map(|a| mol.atoms[a].symbol.clone()).collect();
    let orders: Vec<_> = path
        .windows(2)
        .map(|p| mol.get_bond(p[0], p[1]).map(|b| b.order))
        .collect::<Option<_>>()?;
    if symbols.iter().all(|s| s == "C") {
        return orders
            .iter()
            .all(|o| [1, 2].contains(o))
            .then_some((true, orders));
    }
    if path.len() == 1 && matches!(symbols[0].as_str(), "O" | "S" | "N") {
        return Some((false, orders));
    }
    composite_bridge_constructions()
        .contains_key(&(symbols, orders.clone()))
        .then_some((false, orders))
}

fn orient_bridges(
    mol: &Molecule,
    paths: &[Vec<usize>],
    parent: &AtomSet,
    locants: &BTreeMap<usize, String>,
) -> Option<Vec<BridgePath>> {
    let mut result = Vec::new();
    for path in paths {
        let attachments = |a| {
            mol.get_neighbors(a)
                .iter()
                .copied()
                .filter(|a| parent.contains(a))
                .collect::<Vec<_>>()
        };
        let ends = if path.len() == 1 {
            let mut ends = attachments(path[0]);
            if ends.len() != 2 {
                return None;
            }
            ends.sort_by_key(|a| locant_key(&locants[a]));
            (ends[0], ends[1])
        } else {
            if path[1..path.len() - 1]
                .iter()
                .any(|&a| !attachments(a).is_empty())
            {
                return None;
            }
            let (a, b) = (attachments(path[0]), attachments(*path.last()?));
            if a.len() != 1 || b.len() != 1 {
                return None;
            }
            (a[0], b[0])
        };
        if ends.0 == ends.1
            || mol.get_bond(ends.0, ends.1).is_some()
            || mol.get_bond(path[0], ends.0)?.order != 1
            || mol.get_bond(*path.last()?, ends.1)?.order != 1
        {
            return None;
        }
        let reverse: Vec<_> = path.iter().rev().copied().collect();
        let composite = |p: &[usize]| {
            let symbols: Vec<_> = p.iter().map(|a| mol.atoms[a].symbol.clone()).collect();
            let orders: Vec<_> = p
                .windows(2)
                .filter_map(|p| mol.get_bond(p[0], p[1]).map(|b| b.order))
                .collect();
            composite_bridge_constructions().contains_key(&(symbols, orders))
        };
        let (oriented, endpoints) = if composite(path) {
            (path.clone(), ends)
        } else if composite(&reverse) {
            (reverse, (ends.1, ends.0))
        } else {
            let key = |p: &[usize], e: (usize, usize)| {
                (
                    p.windows(2)
                        .enumerate()
                        .filter_map(|(i, p)| {
                            mol.get_bond(p[0], p[1])
                                .filter(|b| b.order == 2)
                                .map(|_| i + 1)
                        })
                        .collect::<Vec<_>>(),
                    locant_key(&locants[&e.0]),
                    locant_key(&locants[&e.1]),
                )
            };
            if key(path, ends) <= key(&reverse, (ends.1, ends.0)) {
                (path.clone(), ends)
            } else {
                (reverse, (ends.1, ends.0))
            }
        };
        let (carbon, orders) = bridge_class(mol, &oriented)?;
        result.push(BridgePath {
            atoms: oriented,
            endpoints,
            orders,
            carbon,
        });
    }
    Some(result)
}

fn prefers_bridge_dehydro(plan: &FusionParentPlan) -> Result<bool, String> {
    let registry = fusion_component_registry();
    for m in &plan.ast.component_occurrences {
        let spec = registry.spec_for_match(m)?;
        if spec.rings.len() == 1 && !component_h_locants(&spec, "C")?.is_empty() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn wrapper_reconstruction(
    mol: &Molecule,
    all: &AtomSet,
    parent: &AtomSet,
    bridges: &[BridgePath],
    state: &ParentDerivativeState,
    prefer_dehydro: bool,
) -> bool {
    let bridge_atoms: AtomSet = bridges
        .iter()
        .flat_map(|b| b.atoms.iter().copied())
        .collect();
    if bridges.iter().map(|b| b.atoms.len()).sum::<usize>() != bridge_atoms.len()
        || !parent.is_disjoint(&bridge_atoms)
        || parent.union(&bridge_atoms).copied().collect::<AtomSet>() != *all
    {
        return false;
    }
    let mut reconstructed = edges_within_atoms(mol, parent);
    for bridge in bridges {
        let mut scope: AtomSet = bridge.atoms.iter().copied().collect();
        scope.extend([bridge.endpoints.0, bridge.endpoints.1]);
        reconstructed.extend(edges_within_atoms(mol, &scope));
    }
    if reconstructed != edges_within_atoms(mol, all) {
        return false;
    }
    if let [bridge] = bridges
        && bridge.carbon
        && bridge.orders.contains(&2)
        && (prefer_dehydro || !localized_carbon_bridge_bond_orders().contains(&bridge.orders))
    {
        let mut bridge_double = AtomSet::new();
        let mut doubles = Vec::new();
        for (pair, &order) in bridge.atoms.windows(2).zip(&bridge.orders) {
            if order == 2 {
                if pair.iter().any(|a| bridge_double.contains(a)) {
                    return true;
                }
                bridge_double.extend(pair);
                doubles.push((pair[0], pair[1]));
            }
        }
        let mut degree: BTreeMap<_, usize> = all.iter().map(|&a| (a, 0)).collect();
        for (u, v) in state
            .bond_delta
            .assignment
            .orders
            .iter()
            .filter(|(_, o)| *o == 2)
            .map(|(e, _)| *e)
            .chain(doubles)
        {
            *degree.entry(u).or_default() += 1;
            *degree.entry(v).or_default() += 1;
        }
        if degree.values().any(|&n| n > 1)
            || all
                .iter()
                .all(|a| mol.atoms[a].is_carbon() && mol.atoms[a].is_aromatic)
                && degree.values().any(|&n| n != 1)
        {
            return false;
        }
    }
    true
}

fn retained_wrapper_models(mol: &Molecule, atoms: &AtomSet) -> Result<WrapperParentModels, String> {
    let mut matches = retained_matches(mol, atoms, false, false, false);
    if matches.is_empty() {
        matches = retained_matches(mol, atoms, true, true, false);
    }
    let registry = fusion_component_registry();
    let generated_ring_gate = || {
        cached_bounded_face_model(mol, atoms)
            .ok()
            .flatten()
            .is_some_and(|model| {
                fusion_ring_size_gate(
                    &model
                        .faces
                        .iter()
                        .map(|face| face.atoms.len())
                        .collect::<Vec<_>>(),
                )
            })
    };
    matches.retain(|m| {
        registry
            .template_spec(&m.template.name)
            .map_or_else(generated_ring_gate, |s| {
                fusion_ring_size_gate(&s.ring_sizes())
            })
            && !(m.indicated_h != m.template.default_indicated_h
                && retained_parent_name_policy(&m.template.name)
                    .is_some_and(|p| p.hydrogenation.is_some()))
    });
    for m in &mut matches {
        let by = m.template.atom_by_locant();
        if m.template.default_indicated_h.is_empty()
            || m.template
                .default_indicated_h
                .iter()
                .chain(&m.indicated_h)
                .any(|l| by[l].symbol != "C" && !mol.atoms[&m.locant_to_atom[l]].is_aromatic)
        {
            m.indicated_h = m.template.default_indicated_h.clone();
        }
        relocate_oxo_wrapper_hydrogen(mol, m)?;
    }
    let Some(first) = matches.first().cloned() else {
        return Ok(Vec::new());
    };
    let mut same: Vec<_> = matches
        .into_iter()
        .filter(|m| m.template.name == first.template.name && m.indicated_h == first.indicated_h)
        .collect();
    if retained_template_parent_bond_model(
        &first.template,
        &first.locant_to_atom,
        Some(&first.indicated_h),
    )
    .is_err()
    {
        for m in &mut same {
            m.indicated_h = m.template.default_indicated_h.clone();
        }
    }
    let mut result = BTreeMap::new();
    for m in same {
        if let Ok(model) = retained_template_parent_bond_model(
            &m.template,
            &m.locant_to_atom,
            Some(&m.indicated_h),
        ) {
            result.entry(m.atom_to_locant).or_insert(model);
        }
    }
    Ok(result.into_iter().collect())
}

fn relocate_oxo_wrapper_hydrogen(
    mol: &Molecule,
    m: &mut RetainedGraphTemplateMatch,
) -> Result<(), String> {
    if m.indicated_h.is_empty() {
        return Ok(());
    }
    let oxo: Vec<_> = m
        .locant_to_atom
        .iter()
        .filter(|(_, a)| {
            mol.atoms[a].is_carbon()
                && mol.get_neighbors(**a).iter().any(|n| {
                    !m.matched_atoms.contains(n)
                        && mol.atoms[n].symbol == "O"
                        && mol.get_bond(**a, *n).is_some_and(|b| b.order == 2)
                        && super::exocyclic::is_neutral_external_pi_ligand(mol, **a, *n)
                })
        })
        .map(|(l, _)| l.clone())
        .collect();
    if oxo.is_empty() {
        return Ok(());
    }
    let additional = |citation: &[String]| -> Result<Option<usize>, String> {
        let Ok(model) =
            retained_template_parent_bond_model(&m.template, &m.locant_to_atom, Some(citation))
        else {
            return Ok(None);
        };
        Ok(parent_derivative_state(
            mol,
            &m.matched_atoms,
            &model,
            &m.atom_to_locant,
            &ParentDerivativeOptions {
                preserve_retained_parent_state: true,
                allow_pi_redistribution: Some(true),
                ..Default::default()
            },
        )?
        .map(|s| s.unsaturation_operations.len()))
    };
    let Some(mut best) = additional(&m.indicated_h)? else {
        return Ok(());
    };
    if best == 0 {
        return Ok(());
    }
    let mut selected = m.indicated_h.clone();
    for old in &m.indicated_h {
        if !mol.atoms[&m.locant_to_atom[old]].is_carbon() {
            continue;
        }
        for new in &oxo {
            if m.indicated_h.contains(new) {
                continue;
            }
            let mut citation: Vec<_> = m
                .indicated_h
                .iter()
                .filter(|l| *l != old)
                .cloned()
                .chain(std::iter::once(new.clone()))
                .collect();
            citation.sort_by_key(|l| locant_key(l));
            if let Some(count) = additional(&citation)?
                && count < best
            {
                selected = citation;
                best = count;
            }
        }
    }
    m.indicated_h = selected;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::super::graph_io::read_smiles;
    use super::*;

    #[test]
    fn saturated_hetero_adamantane_requires_the_third_component_seam() {
        let mol = read_smiles("C1[C@@H]2C[C@@H]3C[C@H]1N[C@H](C2)S3").unwrap();
        let atoms = mol.atoms.keys().copied().collect();
        assert!(third_component_applies(&mol, &atoms).unwrap());
        assert_eq!(
            require_no_deferred_parent(&mol, &atoms).unwrap_err(),
            "openclatura: systematic fusion not ported"
        );
    }

    #[test]
    fn upstream_round_three_wrappers_are_declined_at_the_composition_seam() {
        for smiles in [
            "C12=CC=C(C3=CC=CC=C13)O2",
            "C12=CC=C(C=3C4=CC=C(C13)C4)O2",
            "N12C=CC3=CC(=CC=C13)C2",
            "C12=CC=C(C3=CC=CC=C13)C=C2",
            "C12=CC=C(C3=CC=CC=C13)CCCCCC2",
        ] {
            let mol = read_smiles(smiles).unwrap();
            let atoms = mol.atoms.keys().copied().collect();
            assert!(bridged_wrapper_applies(&mol, &atoms).unwrap(), "{smiles}");
        }
    }

    #[test]
    fn upstream_cyclic_third_component_cover_requires_its_deferred_route() {
        for smiles in ["N1=C2C3=C(N=NC3=N1)N=N2", "N1=C2C3=C(N=NC3=N1)C=C2"] {
            let mol = read_smiles(smiles).unwrap();
            let atoms = mol.atoms.keys().copied().collect();
            assert_eq!(
                require_no_deferred_parent(&mol, &atoms).unwrap_err(),
                "openclatura: systematic fusion not ported",
                "{smiles}"
            );
        }
    }

    #[test]
    fn ordinary_fused_parents_and_cages_preserve_legacy_fallback() {
        for smiles in [
            "C1CC2CCC1C2",
            "C1C2CC3CC1CC(C2)C3",
            "C1OCC2COCC12",
            "C1=CCSC2SC=NC2=C1",
        ] {
            let mol = read_smiles(smiles).unwrap();
            let atoms = mol.atoms.keys().copied().collect();
            assert!(!third_component_applies(&mol, &atoms).unwrap(), "{smiles}");
            assert!(!bridged_wrapper_applies(&mol, &atoms).unwrap(), "{smiles}");
            assert!(require_no_deferred_parent(&mol, &atoms).is_ok(), "{smiles}");
        }
    }
}
