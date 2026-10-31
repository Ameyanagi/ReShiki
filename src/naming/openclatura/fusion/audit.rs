//! Independent graph reconstruction for systematic fusion candidates.
//!
//! This module consumes the typed proof, rather than running the planner or
//! component matcher again. A candidate reaches rendering only after replay.
use super::super::{
    locants::{parse_system_locant, retained_locant_sort_key as locant_key},
    molecule::{AtomSet, Molecule},
    retained_graph_model::RetainedGraphAtomTemplate,
    rules::multipliers,
};
use super::{
    descriptor::render_fusion_name,
    exocyclic::{is_neutral_external_pi_ligand, neutral_lambda_oxo_bonding_number},
    indicated_hydrogen::*,
    mancude::{self, CompareOptions, ParentDerivativeState},
    model::*,
    numbering::{self, parent_bond_model, validate_parent_bond_valence},
    registry::FusionComponentRegistry,
    rules::*,
    valence::FusionLambdaDescriptor,
};
use std::collections::{BTreeMap, BTreeSet};

type Node = (usize, String);
type Label = (String, i32);
type Specs = BTreeMap<usize, FusionComponentSpec>;
#[derive(Default)]
struct ReconstructedGraph {
    input_edges_by_occurrence: BTreeMap<usize, EdgeSet>,
    input_atoms: BTreeMap<usize, Label>,
    input_edges: EdgeSet,
    skeleton_atoms: BTreeMap<usize, Label>,
    skeleton_edges: BTreeMap<Edge, String>,
}
fn issue(errors: &mut Vec<String>, message: &str) {
    errors.push(message.into());
}
fn result(status: AuditStatus, checks: Vec<String>, errors: Vec<String>) -> FusionAuditResult {
    let mut unique = Vec::new();
    for error in errors {
        if !unique.contains(&error) {
            unique.push(error);
        }
    }
    FusionAuditResult {
        status,
        checks,
        errors: unique,
    }
}
fn check(checks: &mut Vec<String>, name: &str) {
    checks.push(name.into());
}
fn error(message: impl Into<String>, checks: Vec<String>) -> FusionAuditResult {
    result(AuditStatus::Error, checks, vec![message.into()])
}
fn molecular_edges(mol: &Molecule, atoms: &AtomSet) -> EdgeSet {
    mol.bonds
        .values()
        .filter(|b| atoms.contains(&b.u) && atoms.contains(&b.v))
        .map(|b| normalize_edge(b.u, b.v))
        .collect()
}
fn locant_map(numbering: &FusionNumberingProof) -> Result<BTreeMap<usize, String>, String> {
    numbering
        .input_locant_maps
        .first()
        .map(|m| m.iter().cloned().collect())
        .ok_or_else(|| "fusion numbering has no input locant map".into())
}
fn citation_plan(ast: &FusionNameAst) -> Result<FusionCitationPlan, String> {
    ast.citation_plan
        .clone()
        .or_else(|| {
            ast.citation_tree
                .clone()
                .map(|n| FusionCitationPlan::from_tree(n, &ast.joins))
        })
        .ok_or_else(|| "fusion AST has no citation structure".into())
}

#[allow(clippy::too_many_arguments)]
pub fn audit_fusion_plan(
    mol: &Molecule,
    parent_atoms: &AtomSet,
    ast: &FusionNameAst,
    abstract_parent_graph: &FusionGraph,
    numbering: &FusionNumberingProof,
    bond_model: &ParentBondModel,
    registry: &FusionComponentRegistry,
    indicated_hydrogens: &[String],
    charge_operations: &[FusionChargeOperation],
    derivative_state: &ParentDerivativeState,
    lambda_descriptors: &[FusionLambdaDescriptor],
    rendered_core_name: &str,
) -> Result<FusionAuditResult, String> {
    if parent_atoms.iter().any(|id| !mol.atoms.contains_key(id)) {
        return Ok(error("selected parent references unknown atom ids", vec![]));
    }
    if parent_atoms.is_empty() {
        return Ok(error("selected parent graph is empty", vec![]));
    }
    if !fusion_ring_size_gate(
        &numbering
            .selected_face_model
            .faces
            .iter()
            .map(|f| f.size)
            .collect::<Vec<_>>(),
    ) {
        return Ok(result(
            AuditStatus::Abstain,
            vec!["fusion_ring_size_gate".into()],
            vec!["fewer than two rings of size at least five".into()],
        ));
    }
    let mut specs = Specs::new();
    for matched in &ast.component_occurrences {
        match registry.spec_for_match(matched) {
            Ok(spec) => {
                specs.insert(matched.occurrence_id, spec);
            }
            Err(e) => return Ok(error(e, vec![])),
        }
    }
    // Production systematic fusion uses the audited preferred-name mode.
    let mut checks = vec![
        "fusion_ring_size_gate".into(),
        "pin_component_policy".into(),
    ];
    let non_pin: Vec<_> = specs
        .iter()
        .filter_map(|(&id, s)| (!s.pin_component).then_some(id))
        .collect();
    if !non_pin.is_empty() {
        return Ok(result(
            AuditStatus::Abstain,
            checks,
            vec![format!(
                "component occurrences are not PIN-eligible: {non_pin:?}"
            )],
        ));
    }
    let audit = (|| -> Result<FusionAuditResult, String> {
        let unresolved = unrepresented_component_indicated_hydrogens(
            ast,
            &specs,
            indicated_hydrogens,
            numbering,
            derivative_state,
        )?;
        if !unresolved.is_empty() {
            check(&mut checks, "component_indicated_h_composition");
            return Ok(result(
                AuditStatus::Abstain,
                checks.clone(),
                vec![format!(
                    "polycomponent fusion has unrepresented intrinsic indicated-hydrogen sites: {unresolved:?}"
                )],
            ));
        }
        let composition = audited_pin_composition_errors(
            mol,
            ast,
            &specs,
            numbering,
            indicated_hydrogens,
            derivative_state,
        )?;
        if !composition.is_empty() {
            check(&mut checks, "audited_composition_capability");
            return Ok(result(AuditStatus::Abstain, checks.clone(), composition));
        }
        let roles = component_role_errors(ast, &specs);
        if !roles.is_empty() {
            return Ok(result(
                AuditStatus::Abstain,
                vec!["component_role_policy".into()],
                roles,
            ));
        }
        let mut errors = Vec::new();
        let reconstructed = reconstruct(ast, &specs, &mut errors)?;
        audit_nomenclature_selection(ast, &specs, &mut errors)?;
        check(&mut checks, "nomenclature_selection");
        audit_component_and_face_coverage(
            mol,
            parent_atoms,
            ast,
            &specs,
            numbering,
            &reconstructed,
            &mut errors,
        )?;
        check(&mut checks, "component_coverage");
        audit_descriptors(mol, ast, &specs, &reconstructed, numbering, &mut errors)?;
        check(&mut checks, "descriptor_interfaces");
        audit_reconstructed_graph(
            mol,
            parent_atoms,
            abstract_parent_graph,
            &reconstructed,
            charge_operations,
            &mut errors,
        );
        check(&mut checks, "abstract_graph_reconstruction");
        check(&mut checks, "input_graph_identity");
        audit_component_pi_constraints(ast, &specs, abstract_parent_graph, &mut errors)?;
        audit_numbering(
            mol,
            parent_atoms,
            abstract_parent_graph,
            numbering,
            charge_operations,
            ast,
            &mut errors,
        )?;
        check(&mut checks, "completed_numbering");
        audit_charge_operations(
            mol,
            parent_atoms,
            abstract_parent_graph,
            numbering,
            charge_operations,
            &mut errors,
        )?;
        check(&mut checks, "charge_operations");
        let intrinsic_n_h = intrinsic_parent_lone_pair_sites(mol, abstract_parent_graph);
        let locants = locant_map(numbering)?;
        let cited_n_h: AtomSet = locants
            .iter()
            .filter_map(|(&id, l)| {
                let a = &mol.atoms[&id];
                (indicated_hydrogens.contains(l)
                    && a.symbol == "N"
                    && a.charge == 0
                    && a.total_h_count == 1)
                    .then_some(id)
            })
            .collect();
        let carbon_candidates = intrinsic_carbon_candidate_atoms(ast, &specs, mol)?;
        let released = released_fusion_carbon_sites(mol, abstract_parent_graph, &carbon_candidates);
        let mut intrinsic_c_h = AtomSet::new();
        if !intrinsic_n_h.is_empty() || !carbon_candidates.is_empty() || !released.is_empty() {
            let expected = if intrinsic_n_h.is_empty() {
                parent_bond_model(abstract_parent_graph)?
            } else {
                mancude::indicated_hydrogen_parent_bond_model(
                    abstract_parent_graph,
                    &intrinsic_n_h,
                )?
            };
            let (expected, carbon) = intrinsic_carbon_parent_model(
                mol,
                abstract_parent_graph,
                &expected,
                &locants,
                &carbon_candidates,
                &intrinsic_n_h,
                &cited_n_h,
            )?;
            intrinsic_c_h = carbon;
            if (!intrinsic_n_h.is_empty() || !intrinsic_c_h.is_empty() || !released.is_empty())
                && *bond_model != expected
            {
                issue(
                    &mut errors,
                    "parent bond model does not preserve the proved intrinsic-hydrogen sites",
                );
            }
        }
        audit_bond_model(abstract_parent_graph, bond_model, &mut errors);
        check(&mut checks, "parent_bond_model");
        audit_derivative_state(
            mol,
            parent_atoms,
            numbering,
            bond_model,
            indicated_hydrogens,
            derivative_state,
            &mut errors,
        )?;
        check(&mut checks, "parent_derivative_state");
        if super::external_pi_consumption::has_paired_external_pi_consumption(
            mol,
            parent_atoms,
            derivative_state,
        ) {
            // The additive handoff consumes this proved capability to avoid
            // counting the same external-pi saturation as another H operation.
            check(&mut checks, "paired_external_pi_consumption");
        }
        let mut hydro_owned = if indicated_hydrogens.is_empty()
            && mancude::has_complete_saturated_hydrogenation(mol, parent_atoms, derivative_state)
        {
            parent_atoms.clone()
        } else {
            AtomSet::new()
        };
        for op in derivative_state
            .hydro_operations
            .iter()
            .chain(derivative_state.added_hydrogen_operations())
        {
            hydro_owned.extend(op.atom_ids.iter().copied());
        }
        audit_indicated_hydrogens(
            mol,
            numbering,
            bond_model,
            indicated_hydrogens,
            &intrinsic_c_h,
            &hydro_owned,
            &mut errors,
        )?;
        check(&mut checks, "indicated_hydrogens");
        audit_lambda_descriptors(
            mol,
            parent_atoms,
            numbering,
            lambda_descriptors,
            &mut errors,
        )?;
        check(&mut checks, "lambda_descriptors");
        if render_fusion_name(ast, registry)? != rendered_core_name {
            issue(
                &mut errors,
                "rendered fusion parent does not reproduce its context-free AST",
            );
        }
        check(&mut checks, "context_free_rendering");
        Ok(result(
            if errors.is_empty() {
                AuditStatus::Confirmed
            } else {
                AuditStatus::Mismatch
            },
            checks.clone(),
            errors,
        ))
    })();
    Ok(match audit {
        Ok(value) => value,
        Err(e) if e.starts_with("openclatura:") && e.contains("not ported") => return Err(e),
        Err(e) => error(
            format!("fusion audit could not evaluate candidate: {e}"),
            checks,
        ),
    })
}

fn unrepresented_component_indicated_hydrogens(
    ast: &FusionNameAst,
    specs: &Specs,
    cited: &[String],
    numbering: &FusionNumberingProof,
    state: &ParentDerivativeState,
) -> Result<Vec<(usize, String)>, String> {
    let consumed = super::component_hydrogen::component_hydrogen_consumption(ast, specs)?
        .map(|v| v.default_sites)
        .unwrap_or_default();
    let fusion: AtomSet = ast
        .joins
        .iter()
        .flat_map(FusionJoin::shared_input_atoms)
        .collect();
    let converted: AtomSet = state
        .intrinsic_hydro_operations()
        .iter()
        .flat_map(|o| o.atom_ids.iter().copied())
        .collect();
    let locants = locant_map(numbering)?;
    let mut unresolved = Vec::new();
    for matched in &ast.component_occurrences {
        let input = matched.input_atom_by_locant();
        for l in &specs[&matched.occurrence_id].template.default_indicated_h {
            let atom = *input
                .get(l)
                .ok_or("component indicated hydrogen has no mapped atom")?;
            if !consumed.contains(&(matched.occurrence_id, l.clone()))
                && !fusion.contains(&atom)
                && !converted.contains(&atom)
                && !locants.get(&atom).is_some_and(|l| cited.contains(l))
            {
                unresolved.push((matched.occurrence_id, l.clone()));
            }
        }
    }
    unresolved.sort();
    Ok(unresolved)
}
fn component_role_errors(ast: &FusionNameAst, specs: &Specs) -> Vec<String> {
    let mut errors = Vec::new();
    for matched in &ast.component_occurrences {
        let spec = &specs[&matched.occurrence_id];
        if ast.parent_occurrences.contains(&matched.occurrence_id) {
            if !component_parent_eligible(matched, spec, &ast.component_occurrences) {
                errors.push(format!(
                    "component occurrence {} is not allowed as a fusion parent",
                    matched.occurrence_id
                ));
            }
        } else if !spec.usable_as_attached {
            errors.push(format!(
                "component occurrence {} is not allowed as an attached component",
                matched.occurrence_id
            ));
        }
    }
    errors
}
fn audit_nomenclature_selection(
    ast: &FusionNameAst,
    specs: &Specs,
    errors: &mut Vec<String>,
) -> Result<(), String> {
    let citation = citation_plan(ast)?;
    if citation.parent_occurrences() != ast.parent_occurrences {
        issue(
            errors,
            "fusion citation roots do not match the declared parent occurrences",
        );
    }
    if let Some(tree) = &ast.citation_tree
        && citation.roots != vec![tree.clone()]
    {
        issue(
            errors,
            "legacy citation tree does not match the citation plan",
        );
    }
    for &i in &citation.primary_join_indices {
        let join = ast
            .joins
            .get(i)
            .ok_or("citation primary join index is out of range")?;
        if citation
            .parent_occurrences()
            .contains(&join.attached_occurrence())
        {
            issue(errors, "a citation root is attached by a primary join");
        }
    }
    for &i in &citation.cycle_closing_join_indices {
        if ast
            .joins
            .get(i)
            .ok_or("citation closing join index is out of range")?
            .kind()
            != FusionJoinKind::HigherOrder
        {
            issue(
                errors,
                "a cycle-closing join lacks a higher-order descriptor",
            );
        }
    }
    if ast
        .parent_occurrences
        .iter()
        .any(|p| !specs.contains_key(p))
    {
        issue(
            errors,
            "a declared fusion parent is absent from component occurrences",
        );
        return Ok(());
    }
    let eligible: Vec<_> = ast
        .component_occurrences
        .iter()
        .filter(|m| {
            component_parent_eligible(m, &specs[&m.occurrence_id], &ast.component_occurrences)
        })
        .collect();
    let Some(preferred) = eligible
        .iter()
        .map(|m| component_spec_seniority_key(&specs[&m.occurrence_id]))
        .min()
    else {
        issue(errors, "fusion plan has no component eligible as a parent");
        return Ok(());
    };
    if ast
        .parent_occurrences
        .iter()
        .any(|p| component_spec_seniority_key(&specs[p]) != preferred)
    {
        issue(
            errors,
            "declared fusion parent is not the intrinsically senior eligible component",
        );
    }
    if ast.parent_occurrences.len() > 1 {
        let variants: BTreeSet<_> = ast
            .parent_occurrences
            .iter()
            .map(|p| component_variant_identity(&specs[p]))
            .collect();
        if variants.len() != 1 {
            issue(
                errors,
                "multiparent citation does not use identical parent components",
            );
        }
        if citation.interparent_occurrences.is_empty() {
            issue(
                errors,
                "multiparent citation has no explicit interparent component",
            );
        }
    }
    Ok(())
}

fn find(parent: &mut BTreeMap<Node, Node>, value: &Node) -> Result<Node, String> {
    let p = parent
        .get(value)
        .ok_or("fusion join references an unknown component locant")?
        .clone();
    if &p == value {
        Ok(p)
    } else {
        let root = find(parent, &p)?;
        parent.insert(value.clone(), root.clone());
        Ok(root)
    }
}
fn reconstruct(
    ast: &FusionNameAst,
    specs: &Specs,
    errors: &mut Vec<String>,
) -> Result<ReconstructedGraph, String> {
    let mut parent = BTreeMap::new();
    let mut labels = BTreeMap::new();
    let mut input = BTreeMap::new();
    let mut skeleton = BTreeMap::new();
    let mut local_edges = Vec::new();
    let mut output = ReconstructedGraph::default();
    for matched in &ast.component_occurrences {
        let occurrence = matched.occurrence_id;
        let spec = &specs[&occurrence];
        let input_map = matched.input_atom_by_locant();
        let skeleton_map = matched.skeleton_atom_by_locant();
        let expected: BTreeSet<_> = spec.locants().iter().cloned().collect();
        if input_map.keys().cloned().collect::<BTreeSet<_>>() != expected
            || skeleton_map.keys().cloned().collect::<BTreeSet<_>>() != expected
        {
            errors.push(format!(
                "component occurrence {occurrence} does not map every spec locant exactly once"
            ));
            continue;
        }
        let atoms = spec.template.atom_by_locant();
        for l in spec.locants() {
            let node = (occurrence, l.clone());
            let a = atoms
                .get(l)
                .ok_or("component spec has an undefined locant")?;
            parent.insert(node.clone(), node.clone());
            labels.insert(node.clone(), (a.symbol.clone(), a.charge));
            input.insert(node.clone(), input_map[l]);
            skeleton.insert(node, skeleton_map[l]);
        }
        let mut edges = EdgeSet::new();
        for bond in spec.bonds() {
            let (left, right) = &bond.locants;
            local_edges.push((
                (occurrence, left.clone()),
                (occurrence, right.clone()),
                bond.bond_class.clone(),
            ));
            edges.insert(normalize_edge(input_map[left], input_map[right]));
        }
        output.input_edges_by_occurrence.insert(occurrence, edges);
    }
    for join in &ast.joins {
        if !specs.contains_key(&join.attached_occurrence())
            || !specs.contains_key(&join.host_occurrence())
        {
            issue(
                errors,
                "fusion join references an unknown component occurrence",
            );
            continue;
        }
        let attached: Vec<_> = join
            .interface
            .attached_path
            .iter()
            .map(|l| (join.attached_occurrence(), l.text.clone()))
            .collect();
        let host: Vec<_> = join
            .interface
            .host_path
            .iter()
            .map(|l| (join.host_occurrence(), l.text.clone()))
            .collect();
        if attached
            .iter()
            .map(|n| input.get(n).copied())
            .collect::<Option<Vec<_>>>()
            .as_ref()
            != Some(&join.interface.ordered_input_atoms)
        {
            issue(
                errors,
                "fusion join attached path disagrees with its ordered input atoms",
            );
            continue;
        }
        if host
            .iter()
            .map(|n| input.get(n).copied())
            .collect::<Option<Vec<_>>>()
            .as_ref()
            != Some(&join.interface.ordered_input_atoms)
        {
            issue(
                errors,
                "fusion join host path disagrees with its ordered input atoms",
            );
            continue;
        }
        if attached.len() != host.len() {
            return Err("fusion join component paths have different lengths".into());
        }
        for (a, h) in attached.iter().zip(&host) {
            let ar = find(&mut parent, a)?;
            let hr = find(&mut parent, h)?;
            parent.insert(hr, ar);
        }
    }
    let mut classes: BTreeMap<Node, Vec<Node>> = BTreeMap::new();
    for node in labels.keys() {
        classes
            .entry(find(&mut parent, node)?)
            .or_default()
            .push(node.clone());
    }
    let mut root_input = BTreeMap::new();
    let mut root_skeleton = BTreeMap::new();
    for (root, members) in classes {
        let member_labels: BTreeSet<_> = members.iter().map(|n| labels[n].clone()).collect();
        let input_ids: AtomSet = members.iter().map(|n| input[n]).collect();
        let skeleton_ids: AtomSet = members.iter().map(|n| skeleton[n]).collect();
        if member_labels.len() != 1 {
            issue(
                errors,
                "merged component atoms have incompatible element or charge definitions",
            );
            continue;
        }
        if input_ids.len() != 1 {
            issue(
                errors,
                "merged component atoms map to different input atoms",
            );
            continue;
        }
        if skeleton_ids.len() != 1 {
            issue(
                errors,
                "merged component atoms map to different abstract atoms",
            );
            continue;
        }
        let label = member_labels
            .into_iter()
            .next()
            .ok_or("empty merged component label class")?;
        let input_id = *input_ids
            .first()
            .ok_or("empty merged component input class")?;
        let skeleton_id = *skeleton_ids
            .first()
            .ok_or("empty merged component abstract class")?;
        if output
            .input_atoms
            .get(&input_id)
            .is_some_and(|v| v != &label)
        {
            issue(
                errors,
                "input atom receives incompatible component definitions",
            );
        }
        if output
            .skeleton_atoms
            .get(&skeleton_id)
            .is_some_and(|v| v != &label)
        {
            issue(
                errors,
                "abstract atom receives incompatible component definitions",
            );
        }
        output.input_atoms.insert(input_id, label.clone());
        output.skeleton_atoms.insert(skeleton_id, label);
        root_input.insert(root.clone(), input_id);
        root_skeleton.insert(root, skeleton_id);
    }
    for (left, right, class) in local_edges {
        let l = find(&mut parent, &left)?;
        let r = find(&mut parent, &right)?;
        if l == r {
            issue(errors, "fusion collapse creates a self bond");
            continue;
        }
        let (Some(&li), Some(&ri), Some(&ls), Some(&rs)) = (
            root_input.get(&l),
            root_input.get(&r),
            root_skeleton.get(&l),
            root_skeleton.get(&r),
        ) else {
            continue;
        };
        output.input_edges.insert(normalize_edge(li, ri));
        let edge = normalize_edge(ls, rs);
        if let Some(previous) = output.skeleton_edges.get(&edge) {
            if previous == &class {
                continue;
            }
            if ["aromatic", "mancude", "fusion"].contains(&previous.as_str())
                && ["aromatic", "mancude", "fusion"].contains(&class.as_str())
            {
                output.skeleton_edges.insert(edge, "mancude".into());
            } else {
                issue(errors, "shared abstract edge has incompatible bond classes");
            }
        } else {
            output.skeleton_edges.insert(edge, class);
        }
    }
    Ok(output)
}

#[allow(clippy::too_many_arguments)]
fn audit_component_and_face_coverage(
    mol: &Molecule,
    atoms: &AtomSet,
    ast: &FusionNameAst,
    specs: &Specs,
    numbering: &FusionNumberingProof,
    reconstructed: &ReconstructedGraph,
    errors: &mut Vec<String>,
) -> Result<(), String> {
    let faces: BTreeMap<_, _> = numbering
        .selected_face_model
        .faces
        .iter()
        .map(|f| (f.id, f))
        .collect();
    let mut face_owners: BTreeMap<usize, usize> = BTreeMap::new();
    let mut all_atoms = AtomSet::new();
    let mut all_edges = EdgeSet::new();
    let mut scopes = Vec::new();
    let mut edge_counts: BTreeMap<Edge, usize> = BTreeMap::new();
    for matched in &ast.component_occurrences {
        let occurrence = matched.occurrence_id;
        let input = matched.input_atom_by_locant();
        let spec = &specs[&occurrence];
        let scope_atoms: AtomSet = input.values().copied().collect();
        let mut edges = EdgeSet::new();
        for b in spec.bonds() {
            if let (Some(&l), Some(&r)) = (input.get(&b.locants.0), input.get(&b.locants.1)) {
                edges.insert(normalize_edge(l, r));
            }
        }
        all_atoms.extend(&scope_atoms);
        all_edges.extend(&edges);
        for &edge in &edges {
            *edge_counts.entry(edge).or_default() += 1;
        }
        scopes.push((occurrence, edges));
        for &face in &matched.covered_face_ids {
            *face_owners.entry(face).or_default() += 1;
        }
        if matched
            .covered_face_ids
            .iter()
            .any(|id| !faces.contains_key(id))
        {
            errors.push(format!(
                "component occurrence {occurrence} covers unknown faces"
            ));
            continue;
        }
        let mapped: BTreeSet<AtomSet> = spec
            .rings
            .iter()
            .filter_map(|ring| {
                ring.iter()
                    .map(|l| input.get(l).copied())
                    .collect::<Option<AtomSet>>()
            })
            .collect();
        let selected: BTreeSet<AtomSet> = matched
            .covered_face_ids
            .iter()
            .map(|id| faces[id].atom_cycle.iter().copied().collect())
            .collect();
        if mapped != selected {
            errors.push(format!(
                "component occurrence {occurrence} does not reconstruct its declared faces"
            ));
        }
    }
    if face_owners.keys().copied().collect::<AtomSet>() != faces.keys().copied().collect()
        || face_owners.values().any(|&count| count != 1)
    {
        issue(
            errors,
            "component occurrences do not cover every selected face exactly once",
        );
    }
    let target = molecular_edges(mol, atoms);
    if scopes.len() < 2 {
        issue(errors, "a fusion cover requires at least two components");
    }
    if &all_atoms != atoms {
        issue(
            errors,
            "component atoms do not exactly reconstruct the target",
        );
    }
    if all_edges != target {
        issue(
            errors,
            "component edges do not exactly reconstruct the target",
        );
    }
    let mut visited = AtomSet::new();
    let mut pending: Vec<_> = scopes.first().map(|s| vec![s.0]).unwrap_or_default();
    while let Some(id) = pending.pop() {
        if !visited.insert(id) {
            continue;
        }
        let edges = &scopes
            .iter()
            .find(|s| s.0 == id)
            .ok_or("missing component scope")?
            .1;
        for (other, other_edges) in &scopes {
            if !edges.is_disjoint(other_edges) && !visited.contains(other) {
                pending.push(*other);
            }
        }
    }
    if visited.len() != scopes.len() {
        issue(errors, "component overlap graph is disconnected");
    }
    if edge_counts.values().any(|&n| n > 2) {
        issue(
            errors,
            "a molecular edge belongs to more than two components",
        );
    }
    if reconstructed.input_edges != target {
        issue(
            errors,
            "component reconstruction does not exactly cover selected parent edges",
        );
    }
    Ok(())
}
fn host_path_from_sides(
    join: &FusionJoin,
    host: &FusionComponentSpec,
    errors: &mut Vec<String>,
) -> Vec<ComponentLocant> {
    if !join.host_locants().is_empty() {
        return join.host_locants().to_vec();
    }
    let mut edges = Vec::new();
    for side in join.host_sides() {
        for letter in side.letter.chars() {
            let Some(index) = (letter as u32)
                .checked_sub('a' as u32)
                .map(|n| n as usize)
                .filter(|&n| n < host.peripheral_order.len())
            else {
                issue(errors, "host side is not defined by its component");
                return vec![];
            };
            edges.push((
                host.peripheral_order[index].clone(),
                host.peripheral_order[(index + 1) % host.peripheral_order.len()].clone(),
            ));
        }
    }
    let mut path = Vec::new();
    for (left, right) in edges {
        if path.is_empty() {
            path.push(left);
        } else if path.last() != Some(&left) {
            issue(
                errors,
                "host sides do not form one ordered interface on component",
            );
            return vec![];
        }
        path.push(right);
    }
    if path.is_empty() {
        issue(
            errors,
            "host sides do not form one ordered interface on component",
        );
        return vec![];
    }
    let depths: BTreeSet<_> = join.host_sides().iter().map(|s| s.prime_depth).collect();
    if depths.len() != 1 {
        issue(
            errors,
            "host sides use inconsistent prime depths on component",
        );
        return vec![];
    }
    let Some(&prime_depth) = depths.first() else {
        return vec![];
    };
    path.into_iter()
        .map(|text| ComponentLocant {
            component_id: join.host_occurrence(),
            text,
            prime_depth,
        })
        .collect()
}
fn audit_descriptors(
    mol: &Molecule,
    ast: &FusionNameAst,
    specs: &Specs,
    reconstructed: &ReconstructedGraph,
    numbering: &FusionNumberingProof,
    errors: &mut Vec<String>,
) -> Result<(), String> {
    let matches: BTreeMap<_, _> = ast
        .component_occurrences
        .iter()
        .map(|m| (m.occurrence_id, m))
        .collect();
    let mut by_pair = BTreeMap::new();
    for (index, join) in ast.joins.iter().enumerate() {
        let a = join.attached_occurrence();
        let h = join.host_occurrence();
        let pair = normalize_edge(a, h);
        if by_pair.insert(pair, join).is_some() {
            issue(errors, "component pair has more than one fusion join");
        }
        let attached = matches
            .get(&a)
            .ok_or("fusion descriptor references unknown attached occurrence")?
            .input_atom_by_locant();
        let host = matches
            .get(&h)
            .ok_or("fusion descriptor references unknown host occurrence")?
            .input_atom_by_locant();
        let evidence = &join.interface;
        let attached_path = evidence
            .attached_path
            .iter()
            .map(|l| attached.get(&l.text).copied())
            .collect::<Option<Vec<_>>>();
        let host_path = evidence
            .host_path
            .iter()
            .map(|l| host.get(&l.text).copied())
            .collect::<Option<Vec<_>>>();
        if attached_path.as_ref() != Some(&evidence.ordered_input_atoms)
            || host_path.as_ref() != Some(&evidence.ordered_input_atoms)
        {
            issue(
                errors,
                "fusion component paths do not preserve the ordered input interface",
            );
            continue;
        }
        let attached_path = attached_path.ok_or("fusion interface has no attached atom path")?;
        let ordered_edges: Vec<_> = attached_path
            .windows(2)
            .map(|p| normalize_edge(p[0], p[1]))
            .collect();
        let edges: EdgeSet = ordered_edges.iter().copied().collect();
        let actual: EdgeSet = reconstructed
            .input_edges_by_occurrence
            .get(&a)
            .ok_or("missing attached reconstructed edges")?
            .intersection(
                reconstructed
                    .input_edges_by_occurrence
                    .get(&h)
                    .ok_or("missing host reconstructed edges")?,
            )
            .copied()
            .collect();
        let derived_atoms: AtomSet = attached_path.iter().copied().collect();
        if evidence.ordered_input_edges != ordered_edges {
            issue(errors, "fusion join stores a wrong ordered edge path");
        }
        if edges != actual {
            issue(
                errors,
                "fusion join does not cite every and only shared edge",
            );
        }
        if derived_atoms != actual.iter().flat_map(|&(l, r)| [l, r]).collect() {
            issue(errors, "fusion join cites wrong shared atoms");
        }
        let bond_ids: AtomSet = actual
            .iter()
            .filter_map(|&(l, r)| mol.get_bond(l, r).map(|b| b.idx))
            .collect();
        if evidence.shared_input_atoms() != derived_atoms {
            issue(errors, "fusion join stores wrong shared atoms");
        }
        if evidence.shared_input_bonds() != bond_ids {
            issue(errors, "fusion join stores wrong shared bonds");
        }
        if !evidence
            .shared_input_bonds()
            .is_subset(&numbering.selected_face_model.fusion_edges)
        {
            issue(
                errors,
                "fusion join is not a selected-layout fusion interface",
            );
        }
        let expected = host_path_from_sides(join, &specs[&h], errors);
        if !expected.is_empty() && expected != evidence.host_path {
            issue(
                errors,
                "fusion join host sides disagree with its typed host path",
            );
        }
        let expected_kind = if !evidence.host_locants.is_empty() {
            FusionJoinKind::HigherOrder
        } else if ordered_edges.len() == 1 {
            FusionJoinKind::Ortho
        } else {
            FusionJoinKind::OrthoPeri
        };
        if evidence.kind != expected_kind {
            issue(
                errors,
                "fusion join has the wrong fusion kind for its ordered interface",
            );
        }
        if !ast.descriptors.is_empty()
            && ast.descriptors.get(index) != Some(&FusionDescriptor::from_interface(evidence))
        {
            errors.push(format!(
                "descriptor {index} does not preserve its fusion join"
            ));
        }
    }
    let occurrences: Vec<_> = matches.keys().copied().collect();
    for (i, &l) in occurrences.iter().enumerate() {
        for &r in &occurrences[i + 1..] {
            let overlap = !reconstructed
                .input_edges_by_occurrence
                .get(&l)
                .ok_or("missing component edges")?
                .is_disjoint(
                    reconstructed
                        .input_edges_by_occurrence
                        .get(&r)
                        .ok_or("missing component edges")?,
                );
            if overlap != by_pair.contains_key(&normalize_edge(l, r)) {
                issue(
                    errors,
                    "shared interface between component occurrences is not described exactly once",
                );
            }
        }
    }
    let citation = citation_plan(ast)?;
    let mut children: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    let mut attached = BTreeMap::new();
    for &index in &citation.primary_join_indices {
        let join = ast
            .joins
            .get(index)
            .ok_or("primary join index out of range")?;
        children
            .entry(join.host_occurrence())
            .or_default()
            .push(join.attached_occurrence());
        attached.insert(join.attached_occurrence(), join);
    }
    let audit_multiplicity =
        ast.plan_kind == "multiplicative_tree" || !ast.multiplicative_groups.is_empty();
    let mut expected_groups = Vec::new();
    let mut offsets = BTreeMap::new();
    for siblings in children.values() {
        let mut identity: BTreeMap<_, Vec<usize>> = BTreeMap::new();
        for &child in siblings {
            if !children.contains_key(&child) {
                identity
                    .entry(multiplicative_attachment_key(
                        &specs[&child],
                        attached[&child],
                    ))
                    .or_default()
                    .push(child);
            }
        }
        for members in identity.values_mut() {
            if members.len() < 2 {
                continue;
            }
            members.sort_by_key(|id| multiplicative_member_order_key(attached[id]));
            if audit_multiplicity {
                expected_groups.push((members.clone(), multipliers::basic(members.len())));
                for (depth, &id) in members.iter().enumerate() {
                    offsets.insert(id, depth);
                    let expected = depth + attached[&id].order.saturating_sub(1);
                    let depths: BTreeSet<_> = attached[&id]
                        .interface
                        .attached_path
                        .iter()
                        .map(|l| l.prime_depth)
                        .collect();
                    if depths != BTreeSet::from([expected]) {
                        issue(
                            errors,
                            "multiplicative occurrence has incorrect prime depth",
                        );
                    }
                }
            }
        }
    }
    let mut actual_groups: Vec<_> = ast
        .multiplicative_groups
        .iter()
        .map(|g| (g.occurrence_ids.clone(), g.multiplier.clone()))
        .collect();
    actual_groups.sort();
    expected_groups.sort();
    if audit_multiplicity && actual_groups != expected_groups {
        issue(
            errors,
            "multiplicative groups do not match exact sibling interface orbits",
        );
    }
    for &i in &citation.cycle_closing_join_indices {
        if ast.joins.get(i).ok_or("closing join out of range")?.kind()
            != FusionJoinKind::HigherOrder
        {
            issue(
                errors,
                "citation-plan join classes disagree with descriptor kinds",
            );
        }
    }
    fn primary(node: &FusionCitationNode, depth: usize, out: &mut BTreeMap<usize, (usize, usize)>) {
        for child in &node.children {
            out.insert(child.occurrence_id, (node.occurrence_id, depth + 1));
            primary(child, depth + 1, out);
        }
    }
    let mut expected_primary = BTreeMap::new();
    for root in &citation.roots {
        primary(root, 0, &mut expected_primary);
    }
    for &i in &citation.primary_join_indices {
        let join = &ast.joins[i];
        let Some(&(host, order)) = expected_primary.get(&join.attached_occurrence()) else {
            issue(errors, "primary join is absent from the citation tree");
            continue;
        };
        if join.host_occurrence() != host || join.order != order {
            issue(
                errors,
                "primary join disagrees with its citation-tree order",
            );
        }
        if order == 1 {
            if join.kind() == FusionJoinKind::HigherOrder {
                issue(
                    errors,
                    "first-order primary join incorrectly uses numeric host locants",
                );
            }
            continue;
        }
        if join.kind() != FusionJoinKind::HigherOrder {
            issue(
                errors,
                "higher-order primary join lacks numeric host locants",
            );
            continue;
        }
        let a: BTreeSet<_> = join
            .attached_locants()
            .iter()
            .map(|l| l.prime_depth)
            .collect();
        let h: BTreeSet<_> = join.host_locants().iter().map(|l| l.prime_depth).collect();
        if a != BTreeSet::from([order - 1
            + offsets
                .get(&join.attached_occurrence())
                .copied()
                .unwrap_or(0)])
            || h != BTreeSet::from([order - 2])
        {
            issue(
                errors,
                "higher-order primary join has incorrect intermediate-component prime depths",
            );
        }
    }
    for &i in &citation.interparent_join_indices {
        if ast
            .joins
            .get(i)
            .ok_or("interparent join out of range")?
            .kind()
            == FusionJoinKind::HigherOrder
        {
            issue(
                errors,
                "interparent joins must retain parent-side descriptors",
            );
        }
    }
    let parents: AtomSet = citation.parent_occurrences().into_iter().collect();
    for &id in &citation.interparent_occurrences {
        let hosts: AtomSet = ast
            .joins
            .iter()
            .filter(|j| j.attached_occurrence() == id)
            .map(FusionJoin::host_occurrence)
            .collect();
        if hosts.intersection(&parents).count() < 2 {
            issue(
                errors,
                "interparent component occurrence does not join multiple parents",
            );
        }
    }
    Ok(())
}
fn audit_reconstructed_graph(
    mol: &Molecule,
    atoms: &AtomSet,
    graph: &FusionGraph,
    reconstructed: &ReconstructedGraph,
    operations: &[FusionChargeOperation],
    errors: &mut Vec<String>,
) {
    let actual: BTreeMap<_, _> = atoms
        .iter()
        .map(|&id| (id, (mol.atoms[&id].symbol.clone(), mol.atoms[&id].charge)))
        .collect();
    let mut observed = reconstructed.input_atoms.clone();
    for operation in operations {
        if let Some((symbol, charge)) = observed.get_mut(&operation.atom_id)
            && *symbol == operation.symbol
            && *charge == operation.base_charge
        {
            *charge = operation.observed_charge;
        }
    }
    if observed != actual {
        issue(
            errors,
            "reconstructed component atoms differ from selected input elements or formal charges",
        );
    }
    if reconstructed.input_edges != molecular_edges(mol, atoms) {
        issue(
            errors,
            "reconstructed component connectivity differs from the selected input parent",
        );
    }
    let abstract_labels: BTreeMap<_, _> = graph
        .atoms
        .iter()
        .map(|a| (a.id, (a.symbol.clone(), a.formal_charge)))
        .collect();
    let abstract_edges: BTreeMap<_, _> = graph
        .bonds
        .iter()
        .map(|b| (normalize_edge(b.atoms.0, b.atoms.1), b.bond_class.clone()))
        .collect();
    if reconstructed.skeleton_atoms != abstract_labels {
        issue(
            errors,
            "reconstructed component atoms differ from the declared abstract parent graph",
        );
    }
    if reconstructed.skeleton_edges != abstract_edges {
        issue(
            errors,
            "reconstructed component bonds differ from the declared abstract parent graph",
        );
    }
}
fn audit_component_pi_constraints(
    ast: &FusionNameAst,
    specs: &Specs,
    graph: &FusionGraph,
    errors: &mut Vec<String>,
) -> Result<(), String> {
    let relocate = component_carbon_h_relocation_scope(ast, specs);
    let mut expected: BTreeMap<usize, Vec<RetainedGraphAtomTemplate>> = BTreeMap::new();
    for matched in &ast.component_occurrences {
        let spec = &specs[&matched.occurrence_id];
        let atoms = if relocate {
            component_parent_atoms(spec)?
        } else {
            spec.atoms().to_vec()
        };
        let input = matched.input_atom_by_locant();
        for atom in atoms {
            expected
                .entry(
                    *input
                        .get(&atom.locant)
                        .ok_or("component constraint has unknown locant")?,
                )
                .or_default()
                .push(atom);
        }
    }
    for atom in &graph.atoms {
        let components = expected
            .get(&atom.id)
            .ok_or("abstract atom has no component pi constraint")?;
        if atom.pi_capacity
            != components
                .iter()
                .map(RetainedGraphAtomTemplate::resolved_pi_capacity)
                .min()
                .unwrap_or(0)
            || atom.forced_single != components.iter().any(|a| a.forced_single)
            || atom.saturated != components.iter().any(|a| a.saturated)
        {
            issue(
                errors,
                "abstract parent changes a component's fixed pi-capacity or saturation constraint",
            );
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn audit_numbering(
    mol: &Molecule,
    atoms: &AtomSet,
    graph: &FusionGraph,
    numbering: &FusionNumberingProof,
    operations: &[FusionChargeOperation],
    ast: &FusionNameAst,
    errors: &mut Vec<String>,
) -> Result<(), String> {
    let abstract_map: BTreeMap<_, _> = numbering.abstract_atom_to_locant.iter().cloned().collect();
    let labels: BTreeMap<_, _> = graph
        .atoms
        .iter()
        .map(|a| (a.id, (&a.symbol, a.formal_charge)))
        .collect();
    let abstract_edges: EdgeSet = graph
        .bonds
        .iter()
        .map(|b| normalize_edge(b.atoms.0, b.atoms.1))
        .collect();
    if abstract_map.keys().collect::<BTreeSet<_>>() != labels.keys().collect::<BTreeSet<_>>()
        || abstract_map.values().collect::<BTreeSet<_>>().len() != abstract_map.len()
    {
        issue(
            errors,
            "completed abstract numbering is not a bijection over the parent graph",
        );
    }
    audit_face_model(mol, atoms, numbering, errors);
    audit_layout_numbering_compatibility(atoms, numbering, ast, errors)?;
    let mut membership: BTreeMap<usize, usize> = BTreeMap::new();
    for face in &numbering.selected_face_model.faces {
        for &id in &face.atom_cycle {
            *membership.entry(id).or_default() += 1;
        }
    }
    let fusion: AtomSet = membership
        .iter()
        .filter_map(|(&id, &n)| (n > 1).then_some(id))
        .collect();
    let abstract_by_locant: BTreeMap<_, _> =
        abstract_map.iter().map(|(&a, l)| (l.clone(), a)).collect();
    let by_atom: BTreeMap<_, _> = operations.iter().map(|o| (o.atom_id, o)).collect();
    for items in &numbering.input_locant_maps {
        let input: BTreeMap<_, _> = items.iter().cloned().collect();
        if input.keys().copied().collect::<AtomSet>() != *atoms
            || input.values().collect::<BTreeSet<_>>().len() != input.len()
        {
            issue(
                errors,
                "completed input numbering is not a bijection over the selected parent",
            );
            continue;
        }
        let mapped: Option<BTreeMap<_, _>> = input
            .iter()
            .map(|(&id, l)| abstract_by_locant.get(l).map(|&a| (id, a)))
            .collect();
        let Some(mapped) = mapped else {
            issue(
                errors,
                "input numbering uses locants absent from the abstract numbering",
            );
            continue;
        };
        for (&input_atom, &abstract_atom) in &mapped {
            let &(symbol, charge) = labels
                .get(&abstract_atom)
                .ok_or("input numbering references an unknown abstract atom")?;
            let charge = by_atom
                .get(&input_atom)
                .map_or(charge, |o| o.observed_charge);
            let observed = &mol.atoms[&input_atom];
            if observed.symbol != *symbol || observed.charge != charge {
                issue(
                    errors,
                    "input locant map does not preserve element and formal-charge labels",
                );
                break;
            }
        }
        let mapped_edges: EdgeSet = molecular_edges(mol, atoms)
            .iter()
            .map(|&(l, r)| normalize_edge(mapped[&l], mapped[&r]))
            .collect();
        if mapped_edges != abstract_edges {
            issue(errors, "input locant map is not graph preserving");
        }
        for (&id, l) in &input {
            let locant = parse_system_locant(l)?;
            if locant.interior_distance.is_some() {
                continue;
            }
            let carbon = fusion.contains(&id) && mol.atoms[&id].symbol == "C";
            if carbon != !locant.fusion_suffix.is_empty() {
                issue(
                    errors,
                    "fusion-carbon suffix locants do not match the selected face model",
                );
                break;
            }
            if fusion.contains(&id)
                && mol.atoms[&id].symbol != "C"
                && !locant.fusion_suffix.is_empty()
            {
                issue(
                    errors,
                    "fusion heteroatoms must receive integer completed-system locants",
                );
                break;
            }
        }
    }
    Ok(())
}
fn cyclic_pairs(values: &[usize]) -> impl Iterator<Item = Edge> + '_ {
    values
        .iter()
        .copied()
        .zip(values.iter().copied().cycle().skip(1))
        .take(values.len())
}
fn audit_face_model(
    mol: &Molecule,
    atoms: &AtomSet,
    numbering: &FusionNumberingProof,
    errors: &mut Vec<String>,
) {
    let model = &numbering.selected_face_model;
    let mut owners: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    let mut reconstructed = AtomSet::new();
    for face in &model.faces {
        if face.atom_cycle.iter().any(|a| !atoms.contains(a)) {
            errors.push(format!(
                "numbering face {} contains atoms outside the selected parent",
                face.id
            ));
            continue;
        }
        let mut expected = Vec::new();
        for (l, r) in cyclic_pairs(&face.atom_cycle) {
            if let Some(bond) = mol.get_bond(l, r) {
                expected.push(bond.idx);
                owners.entry(bond.idx).or_default().push(face.id);
                reconstructed.insert(bond.idx);
            } else {
                issue(errors, "numbering face contains a nonexistent input edge");
            }
        }
        if expected != face.edge_cycle {
            issue(
                errors,
                "numbering face edge cycle does not follow its atom cycle",
            );
        }
    }
    let expected: AtomSet = mol
        .bonds
        .values()
        .filter(|b| atoms.contains(&b.u) && atoms.contains(&b.v))
        .map(|b| b.idx)
        .collect();
    if reconstructed != expected {
        issue(
            errors,
            "numbering face model does not reconstruct every selected parent edge",
        );
    }
    let expected_owners: Vec<_> = owners
        .iter()
        .map(|(&e, fs)| {
            let mut fs = fs.clone();
            fs.sort();
            (e, fs)
        })
        .collect();
    if model.edge_to_faces != expected_owners {
        issue(errors, "numbering face edge ownership is inconsistent");
    }
    let fusion: AtomSet = owners
        .iter()
        .filter_map(|(&e, fs)| (fs.len() == 2).then_some(e))
        .collect();
    let perimeter: AtomSet = owners
        .iter()
        .filter_map(|(&e, fs)| (fs.len() == 1).then_some(e))
        .collect();
    if model.fusion_edges != fusion || model.perimeter_edges != perimeter {
        issue(
            errors,
            "numbering face perimeter or fusion edge classification is inconsistent",
        );
    }
    let mut adjacency: Vec<_> = owners
        .iter()
        .filter(|(_, fs)| fs.len() == 2)
        .map(|(&e, fs)| (fs[0].min(fs[1]), fs[0].max(fs[1]), e))
        .collect();
    adjacency.sort();
    if model.face_adjacency != adjacency {
        issue(
            errors,
            "numbering face adjacency is inconsistent with shared edges",
        );
    }
    let mut boundary = AtomSet::new();
    for (l, r) in cyclic_pairs(&model.outer_boundary) {
        if let Some(b) = mol.get_bond(l, r) {
            boundary.insert(b.idx);
        } else {
            issue(errors, "numbering outer boundary is not a molecular cycle");
            return;
        }
    }
    if boundary != perimeter {
        issue(
            errors,
            "numbering outer boundary does not equal the face-model perimeter",
        );
    }
}
fn audit_layout_numbering_compatibility(
    atoms: &AtomSet,
    numbering: &FusionNumberingProof,
    ast: &FusionNameAst,
    errors: &mut Vec<String>,
) -> Result<(), String> {
    let layout = &numbering.selected_layout;
    if layout.atom_positions.is_empty() {
        return Ok(());
    }
    let positions: BTreeMap<_, _> = layout
        .atom_positions
        .iter()
        .map(|&(id, x, y)| (id, (x, y)))
        .collect();
    let centers: AtomSet = layout.face_positions.iter().map(|&(id, _, _)| id).collect();
    let faces: AtomSet = numbering
        .selected_face_model
        .faces
        .iter()
        .map(|f| f.id)
        .collect();
    if positions.keys().copied().collect::<AtomSet>() != *atoms {
        issue(
            errors,
            "selected layout does not position every and only parent atom",
        );
        return Ok(());
    }
    if centers != faces {
        issue(
            errors,
            "selected layout does not position every and only selected face",
        );
    }
    if positions.values().collect::<BTreeSet<_>>().len() != positions.len() {
        issue(
            errors,
            "selected layout assigns the same position to multiple parent atoms",
        );
    }
    let directed: BTreeSet<_> = super::config::fusion_nomenclature_config()
        .ring_shapes
        .iter()
        .filter(|s| s.directed_entry_port.is_some())
        .map(|s| s.shape_id.as_str())
        .collect();
    if layout
        .face_shapes
        .iter()
        .any(|(_, s)| directed.contains(s.as_str()))
        && layout.component_entry_edge.is_none()
    {
        issue(errors, "directed layout is missing its component entry");
    }
    if let Some((l, r)) = layout.component_entry_edge {
        let left = positions
            .get(&l)
            .ok_or("component entry atom is not positioned")?;
        let right = positions
            .get(&r)
            .ok_or("component entry atom is not positioned")?;
        if left.1 != right.1 || left.0 <= right.0 {
            issue(
                errors,
                "component entry direction is incompatible with the layout orientation",
            );
        }
        let ownership = numbering
            .selected_face_model
            .faces
            .iter()
            .filter(|f| {
                cyclic_pairs(&f.atom_cycle)
                    .any(|e| normalize_edge(e.0, e.1) == normalize_edge(l, r))
            })
            .count();
        if ownership != 2 {
            issue(errors, "component entry is not a shared fusion edge");
        }
        if !ast
            .joins
            .iter()
            .any(|j| j.interface.ordered_input_atoms == vec![l, r])
        {
            issue(
                errors,
                "component entry does not match an ordered fusion interface",
            );
        }
    }
    let boundary = &numbering.selected_face_model.outer_boundary;
    let mut area = 0i128;
    for (l, r) in cyclic_pairs(boundary) {
        let (x, y) = positions
            .get(&l)
            .ok_or("outer boundary atom is not positioned")?;
        let (u, v) = positions
            .get(&r)
            .ok_or("outer boundary atom is not positioned")?;
        area += i128::from(*x) * i128::from(*v) - i128::from(*u) * i128::from(*y);
    }
    if area == 0 {
        issue(errors, "selected layout has a degenerate outer boundary");
        return Ok(());
    }
    let mut clockwise = boundary.clone();
    if area > 0 {
        clockwise.reverse();
    }
    let locants: BTreeMap<_, _> = numbering.abstract_atom_to_locant.iter().cloned().collect();
    if locants.keys().copied().collect::<AtomSet>() != *atoms {
        return Ok(());
    }
    let mut expected: Vec<_> = boundary
        .iter()
        .copied()
        .collect::<AtomSet>()
        .into_iter()
        .collect();
    expected.sort_by_key(|id| locant_key(&locants[id]));
    if let Some(first) = expected.first() {
        let start = clockwise
            .iter()
            .position(|id| id == first)
            .ok_or("layout perimeter has an unknown start atom")?;
        clockwise.rotate_left(start);
        if clockwise != expected {
            issue(
                errors,
                "completed locant order is incompatible with the selected layout perimeter",
            );
        }
    }
    Ok(())
}
fn audit_charge_operations(
    mol: &Molecule,
    atoms: &AtomSet,
    graph: &FusionGraph,
    numbering: &FusionNumberingProof,
    operations: &[FusionChargeOperation],
    errors: &mut Vec<String>,
) -> Result<(), String> {
    let locants = locant_map(numbering)?;
    let base: BTreeMap<_, _> = graph.atoms.iter().map(|a| (a.id, a)).collect();
    let mut expected = BTreeMap::new();
    for &id in atoms {
        let a = base
            .get(&id)
            .ok_or("parent charge atom is absent from abstract graph")?;
        if mol.atoms[&id].charge != a.formal_charge {
            expected.insert(id, (a.formal_charge, mol.atoms[&id].charge));
        }
    }
    let mut observed = BTreeMap::new();
    for op in operations {
        if !atoms.contains(&op.atom_id) || !base.contains_key(&op.atom_id) {
            issue(
                errors,
                "fusion charge operation targets an atom outside the parent graph",
            );
            continue;
        }
        if observed.contains_key(&op.atom_id) {
            issue(errors, "fusion charge operations duplicate a charged atom");
            continue;
        }
        if locants.get(&op.atom_id) != Some(&op.locant) {
            issue(
                errors,
                "fusion charge operation locant does not map to its charged atom",
            );
        }
        let atom = &mol.atoms[&op.atom_id];
        if op.symbol != atom.symbol {
            issue(
                errors,
                "fusion charge operation does not preserve the charged atom element",
            );
        }
        match op.operation_kind {
            FusionChargeOperationKind::Deprotonation => {
                let valence = atom.total_h_count
                    + mol
                        .get_neighbors(atom.idx)
                        .iter()
                        .filter_map(|&n| mol.get_bond(atom.idx, n))
                        .map(|b| b.order)
                        .sum::<usize>();
                let target = match atom.symbol.as_str() {
                    "C" => Some(3),
                    "N" => Some(2),
                    _ => None,
                };
                if (op.base_charge, op.observed_charge) != (0, -1) || Some(valence) != target {
                    issue(
                        errors,
                        "fusion deprotonation does not preserve proton-removal valence",
                    );
                }
            }
            FusionChargeOperationKind::HeteroatomCationization => {
                if !["N", "O"].contains(&op.symbol.as_str())
                    || (op.base_charge, op.observed_charge) != (0, 1)
                {
                    issue(
                        errors,
                        "fusion cationization is not a neutral N/O to +1 operation",
                    );
                }
            }
        }
        observed.insert(op.atom_id, (op.base_charge, op.observed_charge));
    }
    if observed != expected {
        issue(
            errors,
            "fusion charge operations do not exactly represent parent formal-charge changes",
        );
    }
    Ok(())
}
fn audit_lambda_descriptors(
    mol: &Molecule,
    atoms: &AtomSet,
    numbering: &FusionNumberingProof,
    descriptors: &[FusionLambdaDescriptor],
    errors: &mut Vec<String>,
) -> Result<(), String> {
    let locants = locant_map(numbering)?;
    let mut expected = BTreeMap::new();
    for &id in atoms {
        let atom = &mol.atoms[&id];
        if atom.charge != 0 || atom.is_carbon() {
            continue;
        }
        let hydrogen_count = if atom.total_h_count != 0 {
            atom.total_h_count
        } else {
            atom.explicit_h_count
        };
        let bonding = hydrogen_count
            + mol
                .get_neighbors(id)
                .iter()
                .filter_map(|&n| mol.get_bond(id, n))
                .map(|b| b.order)
                .sum::<usize>();
        let standard = atom
            .element()
            .ok_or("unknown parent element")?
            .standard_valence;
        if bonding > standard {
            expected.insert(
                id,
                (
                    locants.get(&id).ok_or("lambda atom has no locant")?.clone(),
                    bonding,
                    standard,
                ),
            );
        }
    }
    let observed: BTreeMap<_, _> = descriptors
        .iter()
        .map(|d| {
            (
                d.atom_id,
                (d.locant.clone(), d.bonding_number, d.standard_valence),
            )
        })
        .collect();
    if observed != expected {
        issue(
            errors,
            "lambda descriptors do not represent every neutral nonstandard-valence parent atom",
        );
    }
    Ok(())
}
fn audit_bond_model(graph: &FusionGraph, model: &ParentBondModel, errors: &mut Vec<String>) {
    let edges: EdgeSet = graph
        .bonds
        .iter()
        .map(|b| normalize_edge(b.atoms.0, b.atoms.1))
        .collect();
    if model
        .edges()
        .iter()
        .map(|&(l, r)| normalize_edge(l, r))
        .collect::<EdgeSet>()
        != edges
    {
        issue(
            errors,
            "parent bond model does not cover every and only abstract parent edge",
        );
        return;
    }
    let mut maximum = 0;
    for assignment in &model.allowed_kekule_assignments {
        if let Err(e) = validate_parent_bond_valence(graph, assignment) {
            errors.push(format!(
                "parent bond assignment violates fixed valence: {e}"
            ));
        }
        let orders: BTreeMap<_, _> = assignment
            .orders
            .iter()
            .map(|&(e, o)| (normalize_edge(e.0, e.1), o))
            .collect();
        if orders.keys().copied().collect::<EdgeSet>() != edges {
            issue(errors, "parent bond assignment is incomplete");
            continue;
        }
        if orders.values().any(|o| ![1, 2].contains(o)) {
            issue(
                errors,
                "fusion parent bond assignments may contain only single and double bonds",
            );
        }
        let doubles: Vec<_> = orders
            .iter()
            .filter_map(|(&e, &o)| (o == 2).then_some(e))
            .collect();
        maximum = maximum.max(doubles.len());
        if doubles
            .iter()
            .flat_map(|&(l, r)| [l, r])
            .collect::<AtomSet>()
            .len()
            != 2 * doubles.len()
        {
            issue(
                errors,
                "parent bond assignment contains cumulative double bonds",
            );
        }
        if model
            .required_single_bonds
            .iter()
            .any(|e| orders.get(e) != Some(&1))
        {
            issue(
                errors,
                "parent bond assignment violates a required-single edge",
            );
        }
        if model
            .required_double_bonds
            .iter()
            .any(|e| orders.get(e) != Some(&2))
        {
            issue(
                errors,
                "parent bond assignment violates a required-double edge",
            );
        }
    }
    if maximum != model.maximum_non_cumulative_double_bonds {
        issue(
            errors,
            "parent bond model double-bond maximum is inconsistent with its assignments",
        );
    }
    if model.allowed_kekule_assignments.is_empty() {
        issue(
            errors,
            "parent bond model has no allowed bonding assignment",
        );
    }
}
#[allow(clippy::too_many_arguments)]
fn audit_indicated_hydrogens(
    mol: &Molecule,
    numbering: &FusionNumberingProof,
    model: &ParentBondModel,
    cited: &[String],
    intrinsic: &AtomSet,
    hydro_owned: &AtomSet,
    errors: &mut Vec<String>,
) -> Result<(), String> {
    if cited.iter().collect::<BTreeSet<_>>().len() != cited.len() {
        issue(
            errors,
            "fusion indicated-hydrogen citations contain duplicate locants",
        );
        return Ok(());
    }
    let selected = locant_map(numbering)?;
    let by_locant: BTreeMap<_, _> = selected.iter().map(|(&a, l)| (l.clone(), a)).collect();
    let Some(cited_atoms) = cited
        .iter()
        .map(|l| by_locant.get(l).copied())
        .collect::<Option<AtomSet>>()
    else {
        issue(
            errors,
            "fusion indicated-hydrogen citation is outside the completed numbering",
        );
        return Ok(());
    };
    let mut candidates: AtomSet = numbering::indicated_hydrogen_candidate_atoms(mol, &selected)
        .into_iter()
        .collect();
    candidates.extend(intrinsic);
    if !cited_atoms.is_subset(&candidates) {
        issue(
            errors,
            "fusion indicated-hydrogen citation points to an ineligible graph atom",
        );
    }
    let mut required: AtomSet = candidates
        .iter()
        .copied()
        .filter(|id| mol.atoms[id].symbol != "C")
        .collect();
    required.extend(numbering::bond_model_indicated_hydrogen_atoms(
        mol,
        model,
        &candidates,
    ));
    required.extend(intrinsic);
    if !required.is_subset(&cited_atoms.union(hydro_owned).copied().collect()) {
        issue(
            errors,
            "fusion indicated-hydrogen citations omit a graph-required site",
        );
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn audit_derivative_state(
    mol: &Molecule,
    atoms: &AtomSet,
    numbering: &FusionNumberingProof,
    model: &ParentBondModel,
    cited: &[String],
    state: &ParentDerivativeState,
    errors: &mut Vec<String>,
) -> Result<(), String> {
    let locants = locant_map(numbering)?;
    let by_locant: BTreeMap<_, _> = locants.iter().map(|(&id, l)| (l.clone(), id)).collect();
    let indicated: AtomSet = cited
        .iter()
        .map(|l| {
            by_locant
                .get(l)
                .copied()
                .ok_or_else(|| "indicated hydrogen is outside completed numbering".to_string())
        })
        .collect::<Result<_, _>>()?;
    let external: Vec<_> = state.external_pi_operations().cloned().collect();
    let external_atoms: AtomSet = external.iter().map(|o| o.parent_atom_id).collect();
    let delta = mancude::compare_actual_parent_to_implied_parent(
        mol,
        atoms,
        model,
        &CompareOptions {
            externally_unsaturated_atom_ids: &external_atoms,
            indicated_hydrogen_atom_ids: &indicated,
            atom_to_locant: Some(&locants),
            ..Default::default()
        },
    )?;
    let Some(delta) = delta.filter(|d| d.compatible) else {
        issue(
            errors,
            "observed parent bond state is incompatible with the parent hydride",
        );
        return Ok(());
    };
    if state.bond_delta != delta {
        issue(
            errors,
            "typed derivative state does not carry the selected parent bond delta",
        );
    }
    let redistribution =
        mancude::prove_pi_redistribution(mol, atoms, model, &delta, &indicated, &external)?;
    if state.pi_redistribution != redistribution {
        issue(
            errors,
            "pi redistribution does not reconstruct the raw delta and final implicit parent state",
        );
    }
    let mut hydrogenated: Vec<_> = redistribution
        .as_ref()
        .map_or_else(
            || delta.hydrogenated_atom_ids(),
            |p| p.hydrogenated_atom_ids.clone(),
        )
        .into_iter()
        .collect();
    hydrogenated.sort_by_key(|id| locant_key(&locants[id]));
    let mut hydrogenated_bonds = Vec::new();
    if let Some(p) = &redistribution {
        hydrogenated_bonds.extend(p.removed_bond_ids.union(&p.added_bond_ids).copied());
    } else {
        for &(l, r) in &delta.hydrogenated_edges {
            hydrogenated_bonds.push(
                mol.get_bond(l, r)
                    .ok_or("hydrogenated edge is not a molecular bond")?
                    .idx,
            );
        }
    }
    hydrogenated_bonds.sort();
    if !hydrogenated.is_empty() {
        if state.hydro_operations.len() != 1 {
            issue(
                errors,
                "hydrogenated parent edges require one typed hydro operation",
            );
        } else {
            let o = &state.hydro_operations[0];
            if o.operation_kind != "additive_hydrogen"
                || o.atom_ids != hydrogenated
                || o.locants
                    != hydrogenated
                        .iter()
                        .map(|a| locants[a].clone())
                        .collect::<Vec<_>>()
                || o.bond_ids != hydrogenated_bonds
            {
                issue(
                    errors,
                    "typed hydro operation does not exactly represent hydrogenated parent edges",
                );
            }
        }
    } else if !state.hydro_operations.is_empty() {
        issue(
            errors,
            "typed hydro operations exist without hydrogenated parent edges",
        );
    }
    let mut expected_unsaturation = BTreeSet::new();
    if redistribution.is_none() {
        for id in &delta.additional_multiple_bond_ids {
            let b = mol
                .bonds
                .get(id)
                .ok_or("unsaturation delta has an unknown bond")?;
            expected_unsaturation.insert((*id, b.order, BTreeSet::from([b.u, b.v])));
        }
    }
    let observed_unsaturation: BTreeSet<_> = state
        .unsaturation_operations
        .iter()
        .map(|o| {
            (
                o.bond_id,
                o.bond_order,
                o.atom_ids.iter().copied().collect::<AtomSet>(),
            )
        })
        .collect();
    if observed_unsaturation.len() != state.unsaturation_operations.len() {
        issue(
            errors,
            "typed unsaturation operations duplicate a parent bond",
        );
    }
    if observed_unsaturation != expected_unsaturation {
        issue(
            errors,
            "typed unsaturation operations do not represent the parent bond delta",
        );
    }
    if state.unsaturation_operations.iter().any(|o| {
        o.atom_ids
            .iter()
            .map(|a| locants.get(a).cloned())
            .collect::<Option<Vec<_>>>()
            .as_ref()
            != Some(&o.locants)
    }) {
        issue(
            errors,
            "typed unsaturation operation locants do not match completed-system numbering",
        );
    }
    let mut expected_oxo = BTreeSet::new();
    for &parent in atoms {
        for &neighbor in mol.get_neighbors(parent) {
            if !atoms.contains(&neighbor)
                && mol.atoms[&neighbor].symbol == "O"
                && let Some(b) = mol.get_bond(parent, neighbor).filter(|b| b.order == 2)
            {
                expected_oxo.insert((parent, neighbor, b.idx, locants[&parent].clone()));
            }
        }
    }
    let observed_oxo: BTreeSet<_> = state
        .oxo_operations
        .iter()
        .map(|o| {
            (
                o.parent_atom_id,
                o.external_atom_id,
                o.bond_id,
                o.locant.clone(),
            )
        })
        .collect();
    if observed_oxo.len() != state.oxo_operations.len() {
        issue(
            errors,
            "typed oxo operations duplicate an exocyclic parent oxo group",
        );
    }
    if observed_oxo != expected_oxo {
        issue(
            errors,
            "typed oxo operations do not represent every exocyclic parent oxo group",
        );
    }
    for (symbol, operations) in [
        ("N", &state.imino_operations),
        ("C", &state.alkylidene_operations),
    ] {
        let mut expected = BTreeSet::new();
        for &parent in atoms {
            for &neighbor in mol.get_neighbors(parent) {
                if !atoms.contains(&neighbor)
                    && mol.atoms[&neighbor].symbol == symbol
                    && is_neutral_external_pi_ligand(mol, parent, neighbor)
                {
                    let b = mol
                        .get_bond(parent, neighbor)
                        .ok_or("external pi ligand has no parent bond")?;
                    expected.insert((parent, neighbor, b.idx, locants[&parent].clone()));
                }
            }
        }
        let observed: BTreeSet<_> = operations
            .iter()
            .map(|o| {
                (
                    o.parent_atom_id,
                    o.external_atom_id,
                    o.bond_id,
                    o.locant.clone(),
                )
            })
            .collect();
        if observed.len() != operations.len() || observed != expected {
            errors.push(format!(
                "typed external {symbol} operations do not represent every supported exocyclic bond"
            ));
        }
    }
    Ok(())
}
fn operation_atoms(operations: &[super::super::assembly_parts::HydroOperation]) -> AtomSet {
    operations
        .iter()
        .flat_map(|o| o.atom_ids.iter().copied())
        .collect()
}
fn audited_pin_composition_errors(
    mol: &Molecule,
    ast: &FusionNameAst,
    specs: &Specs,
    numbering: &FusionNumberingProof,
    cited: &[String],
    state: &ParentDerivativeState,
) -> Result<Vec<String>, String> {
    let consistent =
        has_consistent_derivative_operations(mol, ast, specs, numbering, cited, state)?;
    let locants = locant_map(numbering)?;
    let by_locant: BTreeMap<_, _> = locants.iter().map(|(&a, l)| (l.clone(), a)).collect();
    let intrinsic_carbon = consistent
        && cited
            .iter()
            .any(|l| by_locant.get(l).is_some_and(|a| mol.atoms[a].symbol == "C"));
    if !specs.is_empty()
        && specs
            .values()
            .all(|s| s.template.family == "generated_hw_monocycle")
        && !intrinsic_carbon
    {
        return Ok(vec!["fusion of exclusively generated Hantzsch-Widman components lacks an audited orientation anchor".into()]);
    }
    if consistent || has_complete_hydrogenation(mol, numbering, cited, state)? {
        return Ok(vec![]);
    }
    if !cited.is_empty()
        && (!state.hydro_operations.is_empty()
            || !state.unsaturation_operations.is_empty()
            || state.external_pi_operations().next().is_some())
    {
        return Ok(vec![
            "combined indicated-hydrogen and bond/oxo derivative fusion grammar is not audited"
                .into(),
        ]);
    }
    Ok(vec![
        "combined fusion derivative operations lack independent applicable atom scopes".into(),
    ])
}
fn has_consistent_derivative_operations(
    mol: &Molecule,
    ast: &FusionNameAst,
    specs: &Specs,
    numbering: &FusionNumberingProof,
    cited: &[String],
    state: &ParentDerivativeState,
) -> Result<bool, String> {
    let locants = locant_map(numbering)?;
    let by_locant: BTreeMap<_, _> = locants.iter().map(|(&a, l)| (l.clone(), a)).collect();
    let atoms: AtomSet = locants.keys().copied().collect();
    for operation in state
        .imino_operations
        .iter()
        .chain(&state.alkylidene_operations)
    {
        let p = operation.parent_atom_id;
        let e = operation.external_atom_id;
        if !atoms.contains(&p)
            || !mol.atoms.contains_key(&e)
            || atoms.contains(&e)
            || mol.atoms[&e].symbol != operation.external_atom_symbol
            || !is_neutral_external_pi_ligand(mol, p, e)
            || mol
                .get_bond(p, e)
                .is_none_or(|b| b.idx != operation.bond_id)
            || locants.get(&p) != Some(&operation.locant)
        {
            return Ok(false);
        }
    }
    if !state.bond_delta.hydrogenated_edges.is_empty()
        && state.hydro_operations.is_empty()
        && state
            .pi_redistribution
            .as_ref()
            .is_none_or(|p| !p.hydrogenated_atom_ids.is_empty())
    {
        return Ok(false);
    }
    let Some(h) = cited
        .iter()
        .map(|l| by_locant.get(l).copied())
        .collect::<Option<AtomSet>>()
    else {
        return Ok(false);
    };
    let hydro = operation_atoms(&state.hydro_operations);
    let oxo: AtomSet = state
        .external_pi_operations()
        .map(|o| o.parent_atom_id)
        .collect();
    let added = operation_atoms(state.added_hydrogen_operations());
    let intrinsic = operation_atoms(state.intrinsic_hydro_operations());
    let unsaturated: AtomSet = state
        .unsaturation_operations
        .iter()
        .flat_map(|o| o.atom_ids.iter().copied())
        .collect();
    if [&h, &hydro, &oxo, &added, &intrinsic, &unsaturated]
        .iter()
        .any(|s| !s.is_subset(&atoms))
    {
        return Ok(false);
    }
    if hydro.is_empty()
        && oxo.is_empty()
        && added.is_empty()
        && intrinsic.is_empty()
        && unsaturated.is_empty()
        && h.iter().all(|id| mol.atoms[id].symbol != "C")
    {
        return Ok(true);
    }
    if !h.is_disjoint(&hydro)
        || !h.is_disjoint(&oxo)
        || !h.is_disjoint(&added)
        || !h.is_disjoint(&intrinsic)
        || !hydro.is_disjoint(&oxo)
        || !hydro.is_disjoint(&added)
        || !hydro.is_disjoint(&intrinsic)
        || !added.is_disjoint(&oxo)
        || !added.is_disjoint(&intrinsic)
    {
        return Ok(false);
    }
    if intrinsic.iter().any(|id| mol.atoms[id].symbol != "C") {
        return Ok(false);
    }
    let single_assignment = |id: usize| {
        state
            .bond_delta
            .assignment
            .orders
            .iter()
            .all(|&(edge, o)| !([edge.0, edge.1].contains(&id)) || o == 1)
    };
    let witnesses: AtomSet = h
        .union(&added)
        .copied()
        .filter(|id| {
            let a = &mol.atoms[id];
            let neighbors = mol.get_neighbors(*id);
            a.symbol == "N"
                && a.charge == 0
                && a.total_h_count == 1
                && neighbors.len() == 2
                && neighbors.iter().all(|n| {
                    atoms.contains(n) && mol.get_bond(*id, *n).is_some_and(|b| b.order == 1)
                })
                && single_assignment(*id)
        })
        .collect();
    let added_n: AtomSet = added
        .iter()
        .copied()
        .filter(|&id| mancude::is_added_hydrogen_nitrogen(mol, &atoms, id) && single_assignment(id))
        .collect();
    if added
        .iter()
        .any(|id| mol.atoms[id].symbol != "C" && !added_n.contains(id))
    {
        return Ok(false);
    }
    for &id in &hydro {
        let a = &mol.atoms[&id];
        if a.symbol == "C" {
            continue;
        }
        let neighbors = mol.get_neighbors(id);
        if a.symbol != "N"
            || a.charge != 0
            || a.is_aromatic
            || neighbors
                .iter()
                .any(|&n| mol.get_bond(id, n).is_none_or(|b| b.order != 1))
            || a.total_h_count + neighbors.len() != 3
        {
            return Ok(false);
        }
    }
    if added.union(&intrinsic).any(|id| mol.atoms[id].charge != 0) {
        return Ok(false);
    }
    if !state.unsaturation_operations.is_empty()
        && [&h, &hydro, &oxo, &added, &intrinsic]
            .iter()
            .any(|s| !s.is_empty())
    {
        return Ok(false);
    }
    let carbon: AtomSet = h
        .iter()
        .copied()
        .filter(|id| mol.atoms[id].symbol == "C")
        .collect();
    let nitrogen: AtomSet = h.difference(&carbon).copied().collect();
    if nitrogen.iter().any(|id| mol.atoms[id].symbol != "N") {
        return Ok(false);
    }
    if nitrogen.iter().any(|id| {
        let a = &mol.atoms[id];
        !a.is_aromatic && a.charge == 0 && !witnesses.contains(id)
    }) {
        return Ok(false);
    }
    if !carbon.is_empty() {
        if !added.is_empty() || !intrinsic.is_empty() || !nitrogen.is_subset(&witnesses) {
            return Ok(false);
        }
        if !carbon.is_subset(&intrinsic_carbon_candidate_atoms(ast, specs, mol)?) {
            return Ok(false);
        }
        if carbon
            .iter()
            .any(|&id| !is_intrinsic_carbon_h_site(mol, id, &atoms) || !single_assignment(id))
        {
            return Ok(false);
        }
        if !oxo.is_empty() {
            let graph = component_parent_graph(ast, specs, true)?;
            let donors = intrinsic_parent_lone_pair_sites(mol, &graph);
            if !has_carbon_h_oxo_bond_consumption(mol, &atoms, state, &donors)? {
                return Ok(false);
            }
        }
    }
    let represented: AtomSet = state.external_pi_operations().map(|o| o.bond_id).collect();
    let mut external = Vec::new();
    for &id in &atoms {
        for &n in mol.get_neighbors(id) {
            if !atoms.contains(&n) {
                let b = mol.get_bond(id, n).ok_or("parent neighbor has no bond")?;
                if b.order > 1 && !represented.contains(&b.idx) {
                    external.push((id, n));
                }
            }
        }
    }
    if external.is_empty() {
        return Ok(true);
    }
    if hydro.is_empty()
        || [&h, &added, &intrinsic, &unsaturated]
            .iter()
            .any(|s| !s.is_empty())
    {
        return Ok(false);
    }
    Ok(external.into_iter().all(|(id, n)| {
        let a = &mol.atoms[&id];
        let b = &mol.atoms[&n];
        a.symbol == "C"
            && a.charge == 0
            && !hydro.contains(&id)
            && !oxo.contains(&id)
            && mol.get_bond(id, n).is_some_and(|e| e.order == 2)
            && ["C", "N"].contains(&b.symbol.as_str())
            && b.charge == 0
            && b.total_h_count
                + mol
                    .get_neighbors(n)
                    .iter()
                    .filter_map(|&v| mol.get_bond(n, v))
                    .map(|e| e.order)
                    .sum::<usize>()
                == b.element().map_or(0, |e| e.standard_valence)
    }))
}
fn has_carbon_h_oxo_bond_consumption(
    mol: &Molecule,
    atoms: &AtomSet,
    state: &ParentDerivativeState,
    donors: &AtomSet,
) -> Result<bool, String> {
    if super::external_pi_consumption::has_paired_external_pi_consumption(mol, atoms, state) {
        return Ok(true);
    }
    let expected: BTreeMap<_, _> = state.bond_delta.assignment.orders.iter().copied().collect();
    for operation in state.external_pi_operations() {
        let id = operation.parent_atom_id;
        if neutral_lambda_oxo_bonding_number(mol, id).is_some() {
            let neighbors: Vec<_> = mol
                .get_neighbors(id)
                .iter()
                .copied()
                .filter(|n| atoms.contains(n))
                .collect();
            if !state.oxo_operations.contains(operation)
                || neighbors.len() != mol.atoms[&id].element().map_or(0, |e| e.standard_valence)
                || neighbors.iter().any(|&n| {
                    expected.get(&normalize_edge(id, n)) != Some(&1)
                        || mol.get_bond(id, n).is_none_or(|b| b.order != 1)
                })
            {
                return Ok(false);
            }
            let actual: BTreeMap<_, _> = mol
                .get_neighbors(id)
                .iter()
                .filter(|n| !atoms.contains(n))
                .filter_map(|&n| {
                    mol.get_bond(id, n)
                        .filter(|b| b.order == 2)
                        .map(|b| (b.idx, n))
                })
                .collect();
            let cited: Vec<_> = state
                .oxo_operations
                .iter()
                .filter(|o| o.parent_atom_id == id)
                .map(|o| (o.bond_id, o.external_atom_id))
                .collect();
            if cited.len() != actual.len()
                || cited.into_iter().collect::<BTreeMap<_, _>>() != actual
            {
                return Ok(false);
            }
            continue;
        }
        if mol.atoms[&id].symbol != "C"
            || mol.atoms[&id].charge != 0
            || mol
                .get_neighbors(id)
                .iter()
                .filter(|n| atoms.contains(n))
                .count()
                != 2
            || mol
                .get_neighbors(id)
                .iter()
                .filter(|n| atoms.contains(n))
                .any(|&n| mol.get_bond(id, n).is_none_or(|b| b.order != 1))
        {
            return Ok(false);
        }
        let consumed: Vec<_> = state
            .bond_delta
            .assignment
            .orders
            .iter()
            .filter_map(|&(e, o)| (o == 2 && [e.0, e.1].contains(&id)).then_some(e))
            .collect();
        if consumed.is_empty() && mol.get_neighbors(id).iter().any(|n| donors.contains(n)) {
            continue;
        }
        if consumed.len() != 1 {
            return Ok(false);
        }
        let edge = consumed[0];
        let other = if edge.0 == id { edge.1 } else { edge.0 };
        if !is_intrinsic_carbon_h_site(mol, other, atoms) {
            return Ok(false);
        }
    }
    Ok(state.external_pi_operations().next().is_some())
}
fn has_complete_hydrogenation(
    mol: &Molecule,
    numbering: &FusionNumberingProof,
    cited: &[String],
    state: &ParentDerivativeState,
) -> Result<bool, String> {
    if !state.unsaturation_operations.is_empty()
        || state.external_pi_operations().next().is_some()
        || state.hydro_operations.is_empty()
    {
        return Ok(false);
    }
    let locants = locant_map(numbering)?;
    let atoms: AtomSet = locants.keys().copied().collect();
    let by_locant: BTreeMap<_, _> = locants.iter().map(|(&id, l)| (l.clone(), id)).collect();
    let hydrogenated: EdgeSet = state
        .bond_delta
        .hydrogenated_edges
        .iter()
        .copied()
        .collect();
    if !cited.is_empty() {
        let Some(indicated) = cited
            .iter()
            .map(|l| by_locant.get(l).copied())
            .collect::<Option<AtomSet>>()
        else {
            return Ok(false);
        };
        let sites = mancude::saturated_nitrogen_hydrogen_sites(mol, &atoms, &indicated);
        if sites.is_empty()
            || atoms
                .iter()
                .any(|id| mol.atoms[id].symbol == "N" && !sites.contains(id))
        {
            return Ok(false);
        }
        let covered: AtomSet = hydrogenated.iter().flat_map(|&(l, r)| [l, r]).collect();
        if covered.len() != 2 * hydrogenated.len()
            || atoms
                .difference(&sites)
                .filter(|id| !covered.contains(id))
                .count()
                > 1
            || !sites.is_disjoint(&covered)
        {
            return Ok(false);
        }
        if state
            .bond_delta
            .assignment
            .orders
            .iter()
            .any(|&(e, o)| o != 1 && (sites.contains(&e.0) || sites.contains(&e.1)))
        {
            return Ok(false);
        }
    } else if !mancude::has_complete_saturated_hydrogenation(mol, &atoms, state)
        && !atoms.iter().all(|id| {
            mol.atoms[id].charge == 0
                && ["C", "N", "O", "S"].contains(&mol.atoms[id].symbol.as_str())
        })
    {
        return Ok(false);
    }
    Ok(!atoms.iter().any(|&id| {
        mol.get_neighbors(id)
            .iter()
            .any(|&n| !atoms.contains(&n) && mol.get_bond(id, n).is_some_and(|b| b.order > 1))
    }) && state.bond_delta.assignment.orders.iter().all(|&(e, o)| {
        mol.get_bond(e.0, e.1).is_some_and(|b| b.order == 1)
            && atoms.contains(&e.0)
            && atoms.contains(&e.1)
            && (o != 2 || hydrogenated.contains(&e))
    }))
}

#[cfg(test)]
mod tests {
    use super::super::super::{
        molecule::{Atom, Bond},
        retained_graph_model::{RetainedGraphBondTemplate, RetainedGraphTemplate},
    };
    use super::*;

    fn fused_pair() -> (FusionNameAst, Specs) {
        let locants: Vec<_> = (1..=5).map(|n| n.to_string()).collect();
        let template = RetainedGraphTemplate {
            locants: locants.clone(),
            atoms: locants
                .iter()
                .map(|locant| RetainedGraphAtomTemplate {
                    locant: locant.clone(),
                    symbol: "C".into(),
                    ..Default::default()
                })
                .collect(),
            bonds: (0..5)
                .map(|i| RetainedGraphBondTemplate {
                    locants: (locants[i].clone(), locants[(i + 1) % 5].clone()),
                    bond_class: "mancude".into(),
                })
                .collect(),
            ..Default::default()
        };
        let spec = FusionComponentSpec {
            template,
            ..Default::default()
        };
        let mapping = |ids: [usize; 5]| locants.iter().cloned().zip(ids).collect::<Vec<_>>();
        let components = vec![
            FusionComponentMatch {
                occurrence_id: 0,
                local_to_input_atom: mapping([0, 1, 2, 3, 4]),
                local_to_skeleton_atom: mapping([0, 1, 2, 3, 4]),
                ..Default::default()
            },
            FusionComponentMatch {
                occurrence_id: 1,
                local_to_input_atom: mapping([1, 0, 5, 6, 7]),
                local_to_skeleton_atom: mapping([1, 0, 5, 6, 7]),
                ..Default::default()
            },
        ];
        let locant = |component_id, text: &str| ComponentLocant {
            component_id,
            text: text.into(),
            prime_depth: 0,
        };
        let ast = FusionNameAst {
            component_occurrences: components,
            joins: vec![FusionJoin {
                order: 1,
                interface: OrderedFusionInterface {
                    attached_occurrence: 1,
                    host_occurrence: 0,
                    attached_path: vec![locant(1, "1"), locant(1, "2")],
                    host_path: vec![locant(0, "2"), locant(0, "1")],
                    ordered_input_atoms: vec![1, 0],
                    ordered_input_edges: vec![(0, 1)],
                    ..Default::default()
                },
            }],
            ..Default::default()
        };
        (ast, BTreeMap::from([(0, spec.clone()), (1, spec)]))
    }

    #[test]
    fn reconstruction_detects_interface_corruption_independently() {
        let (mut ast, specs) = fused_pair();
        let mut errors = Vec::new();
        let reconstructed = reconstruct(&ast, &specs, &mut errors).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(reconstructed.input_atoms.len(), 8);
        assert_eq!(reconstructed.input_edges.len(), 9);
        // The graph coverage is unchanged, but the cited host path has been
        // reversed without changing its ordered interface witness.
        ast.joins[0].interface.host_path.reverse();
        let mut errors = Vec::new();
        reconstruct(&ast, &specs, &mut errors).unwrap();
        assert!(
            errors.iter().any(|e| e.contains("host path disagrees")),
            "{errors:?}"
        );
    }

    #[test]
    fn input_numbering_must_reconstruct_connectivity() {
        let mut mol = Molecule::new();
        for id in 0..4 {
            mol.add_atom(Atom::new(id, "C")).unwrap();
        }
        for (id, (u, v)) in [(0, 1), (1, 2), (2, 3), (3, 0)].into_iter().enumerate() {
            mol.add_bond(Bond::new(id, u, v, 1)).unwrap();
        }
        let atoms = (0..4).collect();
        let graph = FusionGraph {
            atoms: (0..4).map(|id| FusionGraphAtom::new(id, "C")).collect(),
            bonds: [(0, 1), (1, 2), (2, 3), (3, 0)]
                .into_iter()
                .map(|atoms| FusionGraphBond {
                    atoms,
                    bond_class: "mancude".into(),
                })
                .collect(),
        };
        let face = Face {
            id: 0,
            atom_cycle: vec![0, 1, 2, 3],
            edge_cycle: vec![0, 1, 2, 3],
            size: 4,
        };
        let mut proof = FusionNumberingProof {
            abstract_atom_to_locant: vec![
                (0, "1".into()),
                (1, "2".into()),
                (2, "3".into()),
                (3, "4".into()),
            ],
            input_locant_maps: vec![vec![
                (0, "1".into()),
                (1, "2".into()),
                (2, "3".into()),
                (3, "4".into()),
            ]],
            selected_face_model: FaceModel {
                faces: vec![face],
                outer_boundary: vec![0, 1, 2, 3],
                edge_to_faces: (0..4).map(|id| (id, vec![0])).collect(),
                perimeter_edges: (0..4).collect(),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut errors = Vec::new();
        audit_numbering(
            &mol,
            &atoms,
            &graph,
            &proof,
            &[],
            &FusionNameAst::default(),
            &mut errors,
        )
        .unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        proof.input_locant_maps[0].swap(1, 2);
        // Swapping tuple positions preserves the map. Corrupt the assignments
        // themselves while retaining a formally bijective set of locants.
        proof.input_locant_maps[0][1].1 = "2".into();
        proof.input_locant_maps[0][2].1 = "3".into();
        let mut errors = Vec::new();
        audit_numbering(
            &mol,
            &atoms,
            &graph,
            &proof,
            &[],
            &FusionNameAst::default(),
            &mut errors,
        )
        .unwrap();
        assert!(
            errors.iter().any(|e| e.contains("not graph preserving")),
            "{errors:?}"
        );
    }
}
