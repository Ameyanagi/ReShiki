//! Audited systematic fusion planning in upstream candidate and rule order.
use super::super::{
    locants::retained_locant_sort_key as locant_key,
    molecule::{AtomSet, Molecule},
    polycycle_topology::{bicyclo_proof, edges_within_atoms},
};
use super::{
    audit::audit_fusion_plan,
    charges::{fusion_charge_operations, fusion_component_charge_parent},
    citation_numbering::numbered_parent_graphs_agree,
    citation_pi::citation_pi_dead_end,
    citation_pi_donor::charged_donor_citation_pi_dead_end,
    config::fusion_nomenclature_config,
    descriptor::{DescriptorOptions, iter_fusion_name_asts, render_fusion_name},
    faces::{BoundedFaceModel, cached_bounded_face_model, typed_face_model},
    indicated_hydrogen::{
        component_carbon_h_relocation_scope, component_h_locants, component_parent_atoms,
        component_parent_graph, intrinsic_carbon_candidate_atoms, intrinsic_carbon_parent_model,
        intrinsic_parent_lone_pair_sites,
    },
    layout::{
        can_use_component_entry_layout, component_entry_layouts, preferred_intrinsic_layouts,
    },
    mancude::{
        ParentDerivativeOptions, ParentDerivativeState, has_complete_saturated_hydrogenation,
        indicated_hydrogen_parent_bond_model, parent_derivative_state,
        protonated_hydrogenation_is_complete, saturated_nitrogen_hydrogen_sites,
    },
    model::*,
    numbering::{
        CompletedNumberingSelection, completed_system_numbering_selection,
        indicated_hydrogen_candidate_atoms, observed_parent_matches_bond_model, parent_bond_model,
    },
    registry::{FusionComponentRegistry, fusion_component_registry},
    rules::{fusion_ring_size_gate, fusion_supported},
    valence::fusion_lambda_descriptors,
};
use std::collections::BTreeMap;

type PlanResult = Result<Option<FusionParentPlan>, String>;
type Specs = BTreeMap<usize, FusionComponentSpec>;

/// Only a confirmed plan enters naming. An audited abstention preserves the
/// legacy route, while an unported branch cannot silently choose a different name.
pub fn plan_fusion_parent(mol: &Molecule, atoms: &AtomSet) -> PlanResult {
    if let Some(cached) = mol.fusion_plan_cache.borrow().get(atoms) {
        return cached.clone();
    }
    let result = plan_uncached(mol, atoms);
    mol.fusion_plan_cache
        .borrow_mut()
        .insert(atoms.clone(), result.clone());
    result
}

fn rejected(error: String) -> PlanResult {
    if error.starts_with("openclatura:") && error.contains("not ported") {
        Err(error)
    } else {
        Ok(None)
    }
}

fn plan_uncached(mol: &Molecule, atoms: &AtomSet) -> PlanResult {
    let config = fusion_nomenclature_config();
    let support = &config.rules.support;
    if atoms.len() < 6 || atoms.iter().any(|a| !mol.atoms.contains_key(a)) {
        return Ok(None);
    }
    if (!support.charged_parents && atoms.iter().any(|a| mol.atoms[a].charge != 0))
        || atoms
            .iter()
            .any(|a| !fusion_supported(&mol.atoms[a].symbol))
        || (!support.nonstandard_valence && !standard_valence_parent(mol, atoms))
    {
        return Ok(None);
    }
    let bounded = match cached_bounded_face_model(mol, atoms) {
        Ok(Some(bounded)) => bounded,
        Ok(None) => return Ok(None),
        Err(error) => return rejected(error),
    };
    if bounded.cycle_rank < 2 {
        return Ok(None);
    }
    if !bounded.interior_atoms().is_empty()
        && (bicyclo_proof(atoms, &edges_within_atoms(mol, atoms)).is_some()
            || !support.interior_atoms)
    {
        return Ok(None);
    }
    if !fusion_ring_size_gate(
        &bounded
            .faces
            .iter()
            .map(|f| f.atoms.len())
            .collect::<Vec<_>>(),
    ) {
        return Ok(None);
    }
    let registry = fusion_component_registry();
    let matching_parent = match fusion_component_charge_parent(mol, atoms) {
        Ok(parent) => parent,
        Err(error) => return rejected(error),
    };
    let face_model = typed_face_model(mol, &bounded);
    let matches = match registry.match_face_model(&matching_parent, &face_model) {
        Ok(matches) => matches,
        Err(error) => return rejected(error),
    };
    let mut intrinsic_layouts = None;
    if !can_use_component_entry_layout(&face_model) {
        match preferred_intrinsic_layouts(&face_model, config.search.layout_states) {
            Ok(layouts) if !layouts.is_empty() => intrinsic_layouts = Some(layouts),
            Ok(_) => return Ok(None),
            Err(error) => return rejected(error),
        }
    }
    let options = DescriptorOptions::default();
    let candidates = match iter_fusion_name_asts(mol, &matches, registry, &options) {
        Ok(candidates) => candidates,
        Err(error) => return rejected(error),
    };
    let mut numbering_cache = BTreeMap::new();
    for candidate in candidates.take(config.search.maximum_name_candidates) {
        let ast = match candidate {
            Ok(ast) => ast,
            Err(error) => return rejected(error),
        };
        match plan_numbered_candidate(
            mol,
            atoms,
            &ast,
            registry,
            &bounded,
            &face_model,
            &mut intrinsic_layouts,
            &mut numbering_cache,
        ) {
            Ok(Some(plan)) => return Ok(Some(plan)),
            Ok(None) => (),
            Err(error) => {
                rejected(error)?;
            }
        }
    }
    Ok(None)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn plan_numbered_candidate(
    mol: &Molecule,
    atoms: &AtomSet,
    ast: &FusionNameAst,
    registry: &FusionComponentRegistry,
    bounded: &BoundedFaceModel,
    face_model: &FaceModel,
    intrinsic_layouts: &mut Option<Vec<FusedLayout>>,
    numbering_cache: &mut BTreeMap<bool, CompletedNumberingSelection>,
) -> PlanResult {
    let specs = component_specs(ast, registry)?;
    let directed = component_entry_layouts(face_model, ast, &specs)?;
    let component_directed = directed.is_some();
    let layouts = if let Some(layouts) = &directed {
        layouts.as_slice()
    } else {
        if intrinsic_layouts.is_none() {
            *intrinsic_layouts = Some(preferred_intrinsic_layouts(
                face_model,
                fusion_nomenclature_config().search.layout_states,
            )?);
        }
        intrinsic_layouts.as_deref().unwrap_or_default()
    };
    if layouts.is_empty() {
        return Ok(None);
    }
    let mut changed_component_parent = false;
    for spec in specs.values() {
        changed_component_parent |= component_parent_atoms(spec)? != spec.atoms();
    }
    let prove_carbon_h = !intrinsic_carbon_candidate_atoms(ast, &specs, mol)?.is_empty()
        || (changed_component_parent
            && specs
                .values()
                .all(|s| s.atoms().iter().all(|a| a.symbol == "C" && a.charge == 0)));
    let selection = if !component_directed {
        if let Some(cached) = numbering_cache.get(&prove_carbon_h) {
            cached.clone()
        } else {
            let selection = completed_system_numbering_selection(
                mol,
                bounded,
                Some(face_model),
                layouts,
                prove_carbon_h,
            )?;
            numbering_cache.insert(prove_carbon_h, selection.clone());
            selection
        }
    } else {
        completed_system_numbering_selection(
            mol,
            bounded,
            Some(face_model),
            layouts,
            prove_carbon_h,
        )?
    };
    if selection.accepted.is_empty() {
        return Ok(None);
    }
    if selection.accepted.len() > 1
        && !numbered_parent_graphs_agree(
            mol,
            &selection
                .accepted
                .iter()
                .map(|n| n.string_map())
                .collect::<Vec<_>>(),
        )
    {
        return Ok(None);
    }
    let graph = component_parent_graph(
        ast,
        &specs,
        component_carbon_h_relocation_scope(ast, &specs),
    )?;
    let intrinsic_sites = intrinsic_parent_lone_pair_sites(mol, &graph);
    let initial_model = if intrinsic_sites.is_empty() {
        parent_bond_model(&graph)?
    } else {
        indicated_hydrogen_parent_bond_model(&graph, &intrinsic_sites)?
    };
    let mut alternatives = Vec::new();
    for candidate in &selection.accepted {
        let Some(index) = candidate.layout_index else {
            return Ok(None);
        };
        let layout = &layouts[index];
        let proof = FusionNumberingProof {
            selected_face_model: face_model.clone(),
            selected_layout: layout.clone(),
            orientation_score: ComparisonKey::Tuple(vec![
                layout.orientation_score.clone().into(),
                candidate.score.clone(),
            ]),
            abstract_atom_to_locant: candidate.atom_to_locant.clone(),
            input_locant_maps: vec![candidate.atom_to_locant.clone()],
            rejected_numberings: selection.rejected.clone(),
        };
        match complete_fusion_plan(
            mol,
            atoms,
            ast,
            registry,
            &specs,
            proof,
            &graph,
            &initial_model,
        ) {
            Ok(Some(plan))
                if citation_pi_dead_end(&plan).is_none()
                    && charged_donor_citation_pi_dead_end(mol, &plan).is_none() =>
            {
                alternatives.push(plan)
            }
            Err(error) => {
                rejected(error)?;
            }
            _ => (),
        }
    }
    if alternatives.is_empty() {
        return Ok(None);
    }
    if prove_carbon_h {
        let best = alternatives
            .iter()
            .map(|p| {
                p.indicated_hydrogens
                    .iter()
                    .map(|s| locant_key(s))
                    .collect::<Vec<_>>()
            })
            .min()
            .unwrap_or_default();
        alternatives.retain(|p| {
            p.indicated_hydrogens
                .iter()
                .map(|s| locant_key(s))
                .collect::<Vec<_>>()
                == best
        });
    }
    let mut plan = alternatives[0].clone();
    plan.numbering.input_locant_maps = alternatives
        .iter()
        .map(|p| p.numbering.input_locant_maps[0].clone())
        .collect();
    plan.numbering_variants = alternatives;
    Ok(Some(plan))
}

fn component_specs(
    ast: &FusionNameAst,
    registry: &FusionComponentRegistry,
) -> Result<Specs, String> {
    ast.component_occurrences
        .iter()
        .map(|m| Ok((m.occurrence_id, registry.spec_for_match(m)?)))
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn complete_fusion_plan(
    mol: &Molecule,
    atoms: &AtomSet,
    ast: &FusionNameAst,
    registry: &FusionComponentRegistry,
    specs: &Specs,
    numbering: FusionNumberingProof,
    graph: &FusionGraph,
    initial_model: &ParentBondModel,
) -> PlanResult {
    let charge_operations = fusion_charge_operations(mol, graph, &numbering)?;
    let intrinsic_n_h = intrinsic_parent_lone_pair_sites(mol, graph);
    let input_locants: BTreeMap<_, _> = numbering.input_locant_maps[0].iter().cloned().collect();
    let initial_citations = cited_indicated_hydrogens(mol, ast, specs, &numbering, initial_model)?;
    let cited_n_h = input_locants
        .iter()
        .filter(|(a, l)| {
            initial_citations.contains(l)
                && mol.atoms[a].symbol == "N"
                && mol.atoms[a].charge == 0
                && mol.atoms[a].total_h_count == 1
        })
        .map(|(&a, _)| a)
        .collect();
    let (bond_model, intrinsic_carbon_h) = intrinsic_carbon_parent_model(
        mol,
        graph,
        initial_model,
        &input_locants,
        &intrinsic_carbon_candidate_atoms(ast, specs, mol)?,
        &intrinsic_n_h,
        &cited_n_h,
    )?;
    let mut indicated_h = cited_indicated_hydrogens(mol, ast, specs, &numbering, &bond_model)?;
    indicated_h.extend(intrinsic_carbon_h.iter().map(|a| input_locants[a].clone()));
    indicated_h.sort_by_key(|l| locant_key(l));
    indicated_h.dedup();
    let indicated_atoms: AtomSet = input_locants
        .iter()
        .filter(|(_, l)| indicated_h.contains(l))
        .map(|(&a, _)| a)
        .collect();
    let Some(mut derivative_state) = parent_derivative_state(
        mol,
        atoms,
        &bond_model,
        &input_locants,
        &ParentDerivativeOptions {
            indicated_hydrogen_atom_ids: Some(&indicated_atoms),
            ..Default::default()
        },
    )?
    else {
        return Ok(None);
    };
    let hydrogenated = derivative_state.bond_delta.hydrogenated_atom_ids();
    if !indicated_atoms.is_empty()
        && atoms.len().is_multiple_of(2)
        && specs
            .values()
            .all(|s| s.template.default_indicated_h.is_empty())
        && (atoms
            .difference(&indicated_atoms)
            .filter(|a| !hydrogenated.contains(a))
            .count()
            > 1
            || hydrogenated.iter().any(|a| mol.atoms[a].symbol == "N"))
        && !saturated_nitrogen_hydrogen_sites(
            mol,
            atoms,
            &atoms
                .iter()
                .filter(|a| mol.atoms[a].symbol == "N")
                .copied()
                .collect(),
        )
        .is_empty()
        && let Some(complete_state) = parent_derivative_state(
            mol,
            atoms,
            &bond_model,
            &input_locants,
            &ParentDerivativeOptions::default(),
        )?
        && has_complete_saturated_hydrogenation(mol, atoms, &complete_state)
    {
        derivative_state = complete_state;
        indicated_h.clear();
    }
    // Upstream divergence: removing a reduced edge incident to indicated N-H
    // can lose the carbon endpoint's hydrogen. Require the cited H/hydro sites
    // to preserve the ring pi count and the pi degree of each nonaromatic carbon.
    if !protonated_hydrogenation_is_complete(
        mol,
        atoms,
        &bond_model,
        &derivative_state,
        &indicated_atoms,
    )? {
        return Err("openclatura: protonated fused hydrogenation state not ported".into());
    }
    if fusion_nomenclature_config()
        .rules
        .support
        .maximum_indicated_hydrogens
        .is_some_and(|n| indicated_h.len() > n)
    {
        return Ok(None);
    }
    let rendered_core = render_fusion_name(ast, registry)?;
    let lambda_descriptors = fusion_lambda_descriptors(mol, atoms, &input_locants)?;
    let mut rendered = String::new();
    if !indicated_h.is_empty() {
        rendered.push_str(
            &indicated_h
                .iter()
                .map(|l| format!("{l}H"))
                .collect::<Vec<_>>()
                .join(","),
        );
        rendered.push('-');
    }
    if !lambda_descriptors.is_empty() {
        rendered.push_str(
            &lambda_descriptors
                .iter()
                .map(|d| d.text())
                .collect::<Vec<_>>()
                .join(","),
        );
        rendered.push('-');
    }
    rendered.push_str(&rendered_core);
    let mut plan = FusionParentPlan {
        ast: ast.clone(),
        rendered_base_name: rendered,
        abstract_parent_graph: graph.clone(),
        numbering,
        bond_model,
        indicated_hydrogens: indicated_h,
        derivative_state,
        charge_operations,
        lambda_descriptors,
        ..Default::default()
    };
    plan.audit = audit_fusion_plan(mol, atoms, &plan, registry, &rendered_core)?;
    if !plan.audit.confirmed() {
        return Ok(None);
    }
    // Upstream divergence: aromatic equivalence can hide ring triples and
    // cumulative double bonds during parent-hydride matching. Reject a confirmed
    // fusion citation that would erase them; a planner abstention must still
    // permit the legacy route to cite bridged-parent unsaturation explicitly.
    if mol.bonds.values().any(|bond| {
        atoms.contains(&bond.u) && atoms.contains(&bond.v) && !matches!(bond.order, 1 | 2)
    }) || atoms.iter().any(|&atom| {
        mol.get_neighbors(atom)
            .iter()
            .filter(|&&neighbor| {
                atoms.contains(&neighbor)
                    && mol
                        .get_bond(atom, neighbor)
                        .is_some_and(|bond| bond.order == 2)
            })
            .count()
            > 1
    }) {
        return Err(
            "openclatura: fused-parent cumulative and higher bond orders are not ported".into(),
        );
    }
    if unresolved_localized_pi_cycle(
        mol,
        &plan.bond_model,
        &plan.derivative_state,
        &indicated_atoms,
    ) {
        return Err("openclatura: localized nonaromatic fused pi state not ported".into());
    }
    if !super::layout::parser_numbering_is_supported(&plan.numbering.selected_face_model) {
        return Err("openclatura: nonhexagonal fusion parser numbering not ported".into());
    }
    Ok(Some(plan))
}

fn cited_indicated_hydrogens(
    mol: &Molecule,
    ast: &FusionNameAst,
    specs: &Specs,
    numbering: &FusionNumberingProof,
    bond_model: &ParentBondModel,
) -> Result<Vec<String>, String> {
    let locants: BTreeMap<_, _> = numbering.abstract_atom_to_locant.iter().cloned().collect();
    let candidates = indicated_hydrogen_candidate_atoms(mol, &locants);
    let candidate_atoms: AtomSet = candidates.iter().copied().collect();
    let mut unpaired = AtomSet::new();
    for assignment in &bond_model.allowed_kekule_assignments {
        let paired = assignment.paired_atoms();
        unpaired.extend(candidate_atoms.difference(&paired));
    }
    let mut component_h_roles = AtomSet::new();
    for occurrence in &ast.component_occurrences {
        let map = occurrence.input_atom_by_locant();
        component_h_roles.extend(
            component_h_locants(&specs[&occurrence.occurrence_id], "N")?
                .iter()
                .map(|l| map[l]),
        );
    }
    let mut cited: AtomSet = candidates
        .iter()
        .filter(|a| {
            mol.atoms[a].symbol != "C"
                && (mol.atoms[a].is_aromatic
                    || mol.atoms[a].charge != 0
                    || unpaired.contains(a)
                    || component_h_roles.contains(a))
        })
        .copied()
        .collect();
    cited.extend(saturated_nitrogen_hydrogen_sites(
        mol,
        &locants.keys().copied().collect(),
        &candidates
            .iter()
            .filter(|a| mol.atoms[a].symbol == "N")
            .copied()
            .collect(),
    ));
    let has_attached_carbocycle = ast
        .component_occurrences
        .iter()
        .filter(|m| !ast.parent_occurrences.contains(&m.occurrence_id))
        .any(|m| {
            let spec = &specs[&m.occurrence_id];
            spec.rings.len() == 1
                && spec.usable_as_attached
                && !spec.usable_as_parent
                && spec.atoms().iter().all(|a| a.symbol == "C")
        });
    let has_polycyclic_parent = ast
        .parent_occurrences
        .iter()
        .any(|id| specs[id].rings.len() > 1);
    if has_attached_carbocycle
        && has_polycyclic_parent
        && observed_parent_matches_bond_model(mol, bond_model)
    {
        cited.extend(candidates.iter().filter(|a| mol.atoms[a].symbol == "C"));
    }
    let mut result = cited.iter().map(|a| locants[a].clone()).collect::<Vec<_>>();
    result.sort_by_key(|l| locant_key(l));
    Ok(result)
}

fn standard_valence_parent(mol: &Molecule, atoms: &AtomSet) -> bool {
    atoms.iter().all(|&id| {
        let atom = &mol.atoms[&id];
        let h = if atom.total_h_count != 0 {
            atom.total_h_count
        } else {
            atom.explicit_h_count
        };
        atom.element().is_some_and(|e| {
            h + mol
                .get_neighbors(id)
                .iter()
                .filter_map(|&n| mol.get_bond(id, n))
                .map(|b| b.order)
                .sum::<usize>()
                <= e.standard_valence
        })
    })
}

// Upstream divergence: maximum matching proves that the observed pi state
// exists, but not that an unqualified parent citation determines it. An
// alternating 4n cycle with a nonaromatic bond can retain its skeleton and
// hydrogens while reconstructing a different localized double-bond arrangement.
// Aromatic endpoints do not make an explicit inter-ring bond aromatic.
fn unresolved_localized_pi_cycle(
    mol: &Molecule,
    model: &ParentBondModel,
    state: &ParentDerivativeState,
    indicated: &AtomSet,
) -> bool {
    let model = state.bond_delta.composition_model.as_ref().unwrap_or(model);
    let chosen = &state.bond_delta.assignment;
    if model.allowed_kekule_assignments.len() < 2
        || !model.allowed_kekule_assignments.contains(chosen)
    {
        return false;
    }
    let mut protected = indicated.clone();
    protected.extend(state.bond_delta.hydrogenated_atom_ids());
    for operation in state
        .hydro_operations
        .iter()
        .chain(&state.bond_delta.added_hydrogen_operations)
        .chain(&state.bond_delta.intrinsic_hydro_operations)
    {
        protected.extend(operation.atom_ids.iter().copied());
    }
    protected.extend(
        state
            .unsaturation_operations
            .iter()
            .flat_map(|op| op.atom_ids.iter().copied()),
    );
    protected.extend(state.external_pi_operations().map(|op| op.parent_atom_id));
    if let Some(redistribution) = &state.pi_redistribution {
        protected.extend(&redistribution.hydrogenated_atom_ids);
        for id in redistribution
            .removed_bond_ids
            .union(&redistribution.added_bond_ids)
        {
            if let Some(bond) = mol.bonds.get(id) {
                protected.extend([bond.u, bond.v]);
            }
        }
    }
    let chosen: BTreeMap<_, _> = chosen.orders.iter().copied().collect();
    for alternative in &model.allowed_kekule_assignments {
        let alternative_orders: BTreeMap<_, _> = alternative.orders.iter().copied().collect();
        let mut adjacency: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for &(edge, order) in &alternative.orders {
            if matches!((chosen.get(&edge), order), (Some(1), 2) | (Some(2), 1)) {
                adjacency.entry(edge.0).or_default().push(edge.1);
                adjacency.entry(edge.1).or_default().push(edge.0);
            }
        }
        let mut seen = AtomSet::new();
        for &start in adjacency.keys() {
            if seen.contains(&start) {
                continue;
            }
            let mut component = AtomSet::new();
            let mut pending = vec![start];
            while let Some(atom) = pending.pop() {
                if component.insert(atom) {
                    pending.extend(adjacency[&atom].iter().copied());
                }
            }
            seen.extend(&component);
            if component.len() < 4
                || !component.len().is_multiple_of(4)
                || !component.is_disjoint(&protected)
                || component.iter().any(|id| {
                    let atom = &mol.atoms[id];
                    adjacency[id].len() != 2
                        || atom.charge != 0
                        || !matches!(atom.symbol.as_str(), "C" | "N")
                })
            {
                continue;
            }
            if !component.iter().all(|&atom| {
                adjacency[&atom]
                    .iter()
                    .filter(|&&other| chosen[&normalize_edge(atom, other)] == 2)
                    .count()
                    == 1
            }) {
                continue;
            }
            let localized: Vec<_> = component
                .iter()
                .flat_map(|&atom| adjacency[&atom].iter().map(move |&other| (atom, other)))
                .filter(|&(atom, other)| {
                    atom < other
                        && mol
                            .get_bond(atom, other)
                            .is_some_and(|bond| !bond.is_aromatic)
                })
                .collect();
            if !localized.is_empty()
                && [&chosen, &alternative_orders]
                    .into_iter()
                    .any(|assignment| {
                        localized.iter().all(|&(atom, other)| {
                            mol.get_bond(atom, other)
                                .is_some_and(|bond| bond.order == assignment[&(atom, other)])
                        })
                    })
            {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::super::super::graph_io::read_smiles;
    use super::*;

    #[test]
    fn protonated_fused_citations_preserve_carbon_hydrogenation() {
        for smiles in [
            "C1=CC2CCCCC2[NH2+]1",
            "C1=CCC2CC[NH2+]CC2CC1",
            "C1=CC2CCCC2C[NH2+]1",
            "C1=CC2CCCCCC2C[NH2+]1",
        ] {
            let mol = read_smiles(smiles).unwrap();
            let atoms = mol.atoms.keys().copied().collect();
            let error = plan_fusion_parent(&mol, &atoms).unwrap_err();
            assert!(error.contains("protonated"), "{smiles}: {error}");
        }
        for smiles in ["C1=CC2C[NH2+]CCC2CC1", "C1=CCC2CC[NH2+]CCC2CC1"] {
            let mol = read_smiles(smiles).unwrap();
            let atoms = mol.atoms.keys().copied().collect();
            assert!(
                plan_fusion_parent(&mol, &atoms).unwrap().is_some(),
                "{smiles}"
            );
        }
    }

    #[test]
    fn localized_nonaromatic_pi_alternatives_require_a_citation_witness() {
        for text in [
            "C1=CC2=CC3=C(C=CN4CN=CCC34)C2=N1",
            "ClC1=CC=CC2=CC=CC=CC2=C1",
            "c1cn2c3cc4ccnc-4c=3ccn2cn1",
        ] {
            let mol = read_smiles(text).unwrap();
            let atoms = mol
                .atoms
                .keys()
                .copied()
                .filter(|id| mol.atoms[id].symbol != "Cl")
                .collect();
            assert_eq!(
                plan_fusion_parent(&mol, &atoms).unwrap_err(),
                "openclatura: localized nonaromatic fused pi state not ported"
            );
        }
        for text in ["c1oc2ncccc2c1", "C1OCC2COCC12"] {
            let mol = read_smiles(text).unwrap();
            let atoms = mol.atoms.keys().copied().collect();
            assert!(
                plan_fusion_parent(&mol, &atoms).unwrap().is_some(),
                "{text}"
            );
        }
    }
}
