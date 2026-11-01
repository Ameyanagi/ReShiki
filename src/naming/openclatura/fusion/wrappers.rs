//! Typed composition of independently proved fused parents with path bridges and spiro sides.
use super::super::{
    assembly_utils::needs_hyphen,
    chains::canonical_ranks,
    graph_kernel::connected_components,
    locants::{parse_system_locant, retained_locant_sort_key as locant_key},
    molecule::{AtomSet, Molecule, bond_ids_within},
    polycycle_topology::{edges_within_atoms, internal_degrees},
    retained_fused_templates::{
        MatchOptions, match_retained_graph_templates_with_options, retained_parent_metadata,
    },
    retained_graph_model::RetainedGraphTemplateMatch,
    retained_name_policy::{
        render_retained_hydrogen_state, retained_parent_name_policy, retained_parent_output_name,
    },
    ring_parent::RingParent,
    rules::{multipliers, stems},
};
use super::{
    composite_bridges::{
        CompositeBridgeConstruction, composite_bridge_constructions,
        localized_carbon_bridge_bond_orders,
    },
    config::fusion_nomenclature_config,
    exocyclic::{is_neutral_external_pi_ligand, neutral_lambda_oxo_bonding_number},
    faces::cached_bounded_face_model,
    indicated_hydrogen::component_h_locants,
    mancude::{
        ParentDerivativeOptions, ParentDerivativeState, UnsaturationOperation,
        parent_derivative_state,
    },
    model::{ComparisonKey, FusionParentPlan, ParentBondModel},
    numbering::retained_template_parent_bond_model,
    planner::plan_fusion_parent,
    registry::fusion_component_registry,
    rules::fusion_ring_size_gate,
};
use std::collections::{BTreeMap, BTreeSet};

type LocantMap = BTreeMap<usize, String>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WrapperParentKind {
    Retained,
    SystematicFusion,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NondetachableBridgeKind {
    Carbo,
    Epoxy,
    Epithio,
    Epimino,
    Composite,
}

#[derive(Clone, Debug)]
pub struct WrapperParentPlan {
    pub hydride: RingParent,
    pub bond_models: Vec<ParentBondModel>,
    pub selected_locant_map: Option<LocantMap>,
    pub selected_bond_model: Option<ParentBondModel>,
}
impl WrapperParentPlan {
    pub fn kind(&self) -> WrapperParentKind {
        if self.hydride.is_systematic_fusion() {
            WrapperParentKind::SystematicFusion
        } else {
            WrapperParentKind::Retained
        }
    }
    pub fn name(&self) -> &str {
        self.hydride.base_name().unwrap_or("")
    }
    pub fn atom_ids(&self) -> &AtomSet {
        &self.hydride.atoms
    }
    pub fn locant_maps(&self) -> Vec<LocantMap> {
        self.selected_locant_map
            .as_ref()
            .map_or_else(|| self.hydride.proof_locant_maps(), |m| vec![m.clone()])
    }
    pub fn fusion_plan(&self) -> Option<&FusionParentPlan> {
        self.hydride.fusion_plan.as_ref()
    }
    pub fn validate(&self) -> Result<(), String> {
        let maps = self.locant_maps();
        if self.name().is_empty() || self.atom_ids().is_empty() || maps.is_empty() {
            return Err("wrapper parent requires a name, atoms, and locant maps".into());
        }
        if maps.iter().any(|m| {
            m.keys().copied().collect::<AtomSet>() != *self.atom_ids()
                || m.values().collect::<BTreeSet<_>>().len() != m.len()
        }) {
            return Err("wrapper parent locant maps must be complete and bijective".into());
        }
        if self.bond_models.len() != maps.len() {
            return Err("wrapper parent requires one bond model per locant map".into());
        }
        if self.selected_locant_map.is_some() != self.selected_bond_model.is_some() {
            return Err(
                "wrapper parent locant and bond-model selections must be made together".into(),
            );
        }
        if self.kind() == WrapperParentKind::SystematicFusion && self.fusion_plan().is_none() {
            return Err("systematic-fusion wrapper parent requires its fusion proof".into());
        }
        Ok(())
    }
    pub fn select(&self, index: usize) -> Result<Self, String> {
        let entries = self
            .locant_maps()
            .into_iter()
            .nth(index)
            .ok_or("invalid wrapper locant selection")?;
        let model = self
            .bond_models
            .get(index)
            .cloned()
            .ok_or("invalid wrapper bond-model selection")?;
        let mut hydride = self.hydride.clone();
        if let Some(plan) = self
            .fusion_plan()
            .filter(|p| !p.numbering_variants.is_empty())
        {
            let variant = plan
                .numbering_variants
                .get(index)
                .ok_or("invalid wrapper fusion selection")?;
            if variant.numbering.string_input_locant_maps().first() != Some(&entries)
                || variant.bond_model != model
            {
                return Err(
                    "wrapper numbering must select the aligned fusion proof and bond model".into(),
                );
            }
            hydride = RingParent::from_fusion_plan(variant.clone())?;
        }
        hydride.parent_bond_model = Some(model.clone());
        let selected = Self {
            hydride,
            bond_models: vec![model.clone()],
            selected_locant_map: Some(entries),
            selected_bond_model: Some(model),
        };
        selected.validate()?;
        Ok(selected)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NondetachableBridgeOperation {
    pub kind: NondetachableBridgeKind,
    pub prefix: String,
    pub atom_ids: Vec<usize>,
    pub endpoint_atom_ids: (usize, usize),
    pub endpoint_locants: (String, String),
    pub bond_ids: AtomSet,
    pub internal_bond_orders: Vec<usize>,
    pub unsaturation_locants: Vec<String>,
}
impl NondetachableBridgeOperation {
    pub fn rendered(&self) -> String {
        format!(
            "{},{}-{}",
            self.endpoint_locants.0, self.endpoint_locants.1, self.prefix
        )
    }
    fn validate(&self) -> Result<(), String> {
        if self.internal_bond_orders.len() != self.atom_ids.len().saturating_sub(1) {
            return Err("bridge bond orders must describe each internal path edge".into());
        }
        if self
            .internal_bond_orders
            .iter()
            .any(|o| ![1, 2].contains(o))
        {
            return Err(
                "the audited bridge tier supports only single and double internal bonds".into(),
            );
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct BridgedFusionWrapperPlan {
    pub parent: WrapperParentPlan,
    pub bridges: Vec<NondetachableBridgeOperation>,
    pub derivative_state: ParentDerivativeState,
    pub rendered_name: String,
    // Upstream divergence: graph-bound rendered token metadata is omitted with the trace layer.
    pub audit_ok: bool,
    pub audit_checks: Vec<String>,
    pub search_states: usize,
    pub bridge_unsaturation_operations: Vec<UnsaturationOperation>,
    pub saturated_bridge_precursor: Option<NondetachableBridgeOperation>,
}
impl BridgedFusionWrapperPlan {
    pub fn atom_to_locant(&self) -> LocantMap {
        let mut locants = self
            .parent
            .locant_maps()
            .into_iter()
            .next()
            .unwrap_or_default();
        let mut next = locants
            .values()
            .filter_map(|l| parse_system_locant(l).ok().map(|l| l.base))
            .max()
            .unwrap_or(0)
            + 1;
        let mut bridges: Vec<_> = self.bridges.iter().collect();
        bridges.sort_by_key(|b| {
            let mut ends = vec![
                locant_key(&b.endpoint_locants.0),
                locant_key(&b.endpoint_locants.1),
            ];
            ends.sort_by(|a, b| b.cmp(a));
            std::cmp::Reverse(ends)
        });
        for bridge in bridges {
            let mut path = bridge.atom_ids.clone();
            if locant_key(&bridge.endpoint_locants.0) < locant_key(&bridge.endpoint_locants.1) {
                path.reverse();
            }
            for atom in path {
                locants.insert(atom, next.to_string());
                next += 1;
            }
        }
        locants
    }
    fn validate(&self) -> Result<(), String> {
        if self.bridges.is_empty() {
            return Err("bridged fusion wrapper requires at least one bridge".into());
        }
        if !self.audit_ok || self.audit_checks.is_empty() {
            return Err(
                "bridged fusion wrapper must record its successful graph partition audits".into(),
            );
        }
        if self.search_states == 0 {
            return Err("bridged fusion wrapper must record bounded search effort".into());
        }
        if !self.bridge_unsaturation_operations.is_empty()
            != self.saturated_bridge_precursor.is_some()
        {
            return Err(
                "bridge dehydrogenation requires both a saturated precursor and operations".into(),
            );
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct FusionSpiroSidePlan {
    pub parent: WrapperParentPlan,
    pub junction_atom_id: usize,
    pub junction_locant: String,
}
impl FusionSpiroSidePlan {
    pub fn validate(&self) -> Result<(), String> {
        if self.parent.kind() != WrapperParentKind::SystematicFusion {
            return Err("FusionSpiroSidePlan requires a systematic fusion parent".into());
        }
        if !self.parent.atom_ids().contains(&self.junction_atom_id) {
            return Err("spiro junction lies outside the fused side parent".into());
        }
        if self
            .parent
            .locant_maps()
            .first()
            .and_then(|m| m.get(&self.junction_atom_id))
            != Some(&self.junction_locant)
        {
            return Err("spiro junction locant must come from the fusion proof map".into());
        }
        Ok(())
    }
}

// Upstream divergence: production exposes only the audited fusion mode; disabled and legacy modes have no public Rust naming route.
pub fn plan_fusion_spiro_side(
    mol: &Molecule,
    atoms: &AtomSet,
    junction: usize,
) -> Result<Option<FusionSpiroSidePlan>, String> {
    if !atoms.contains(&junction) || atoms.iter().any(|a| mol.atoms[a].charge != 0) {
        return Ok(None);
    }
    let Some(parent) = systematic_fusion_parent(mol, atoms)? else {
        return Ok(None);
    };
    let Some(junction_locant) = parent
        .locant_maps()
        .first()
        .and_then(|m| m.get(&junction))
        .cloned()
    else {
        return Ok(None);
    };
    let plan = FusionSpiroSidePlan {
        parent,
        junction_atom_id: junction,
        junction_locant,
    };
    plan.validate()?;
    Ok(Some(plan))
}

pub fn plan_bridged_fusion_wrapper(
    mol: &Molecule,
    atoms: &AtomSet,
) -> Result<Option<BridgedFusionWrapperPlan>, String> {
    plan_bridged_fusion_wrapper_with_options(mol, atoms, None)
}
pub fn plan_bridged_fusion_wrapper_with_options(
    mol: &Molecule,
    atoms: &AtomSet,
    maximum_bridge_atoms: Option<usize>,
) -> Result<Option<BridgedFusionWrapperPlan>, String> {
    // Upstream divergence: charged ligands on parent heteroatoms require a
    // joint parent-charge proof that the neutral wrapper tier does not provide.
    if maximum_bridge_atoms == Some(0)
        || atoms.iter().any(|a| {
            mol.atoms[a].charge != 0
                || (!mol.atoms[a].is_carbon()
                    && mol
                        .get_neighbors(*a)
                        .iter()
                        .any(|n| !atoms.contains(n) && mol.atoms[n].charge != 0))
        })
    {
        return Ok(None);
    }
    let edges = edges_within_atoms(mol, atoms);
    let degrees = internal_degrees(atoms, &edges);
    if (edges.len() + 1).saturating_sub(atoms.len()) < 3 || !degrees.values().any(|d| *d == 3) {
        return Ok(None);
    }
    let segments = junction_path_interiors(mol, atoms, &degrees);
    if segments.is_empty() {
        return Ok(None);
    }
    let limit = fusion_nomenclature_config()
        .search
        .component_selection_states;
    let mut visited_states = 0;
    for parent_kind in [
        WrapperParentKind::Retained,
        WrapperParentKind::SystematicFusion,
    ] {
        let mut candidates = Vec::new();
        for removed_candidates in removal_groups(&segments, maximum_bridge_atoms, limit).values() {
            for removed in removed_candidates {
                visited_states += 1;
                if visited_states > limit {
                    return Ok(None);
                }
                let parent_atoms: AtomSet = atoms.difference(removed).copied().collect();
                // Upstream divergence: prove necessary path/bridge applicability before
                // standalone parent planning, whose Rust safety errors must not escape
                // candidates that cannot produce any wrapper. This preserves state
                // accounting and accepted candidate, locant-map and tie order.
                let Some(paths) = bridge_path_components(mol, atoms, &parent_atoms, removed) else {
                    continue;
                };
                let provisional_locants: LocantMap = parent_atoms
                    .iter()
                    .map(|&atom| (atom, (atom + 1).to_string()))
                    .collect();
                if bridge_operations(mol, &paths, &parent_atoms, &provisional_locants).is_none() {
                    continue;
                }
                let parent = if parent_kind == WrapperParentKind::Retained {
                    retained_wrapper_parent(mol, &parent_atoms)?
                } else {
                    systematic_fusion_parent(mol, &parent_atoms)?
                };
                let Some(parent) = parent else { continue };
                let prefer_dehydro = prefer_completed_system_bridge_unsaturation(&parent)?;
                for (map_index, entries) in parent.locant_maps().into_iter().enumerate() {
                    let selected_parent = parent.select(map_index)?;
                    let derivative_state = wrapper_parent_derivative_state(
                        mol,
                        &parent_atoms,
                        selected_parent
                            .selected_bond_model
                            .as_ref()
                            .ok_or("missing selected parent model")?,
                        &entries,
                        selected_parent.fusion_plan(),
                        true,
                    )?;
                    let Some(derivative_state) = derivative_state else {
                        continue;
                    };
                    let Some(mut operations) =
                        bridge_operations(mol, &paths, &parent_atoms, &entries)
                    else {
                        continue;
                    };
                    operations.sort_by(|a, b| {
                        (a.prefix.trim_start_matches('('), &a.endpoint_locants)
                            .cmp(&(b.prefix.trim_start_matches('('), &b.endpoint_locants))
                    });
                    let unlocalized = operations.len() == 1
                        && operations[0].kind == NondetachableBridgeKind::Carbo
                        && !operations[0].unsaturation_locants.is_empty()
                        && !localized_carbon_bridge_bond_orders()
                            .contains(&operations[0].internal_bond_orders);
                    let bridge_unsaturation = if prefer_dehydro || unlocalized {
                        bridge_dehydrogenation(mol, &operations, &entries).unwrap_or_default()
                    } else {
                        Vec::new()
                    };
                    let precursor = if bridge_unsaturation.is_empty() {
                        None
                    } else {
                        Some(saturated_bridge_precursor(&operations[0]))
                    };
                    let Some(audit_checks) = audit_bridge_plan(
                        mol,
                        atoms,
                        &parent_atoms,
                        &entries,
                        &operations,
                        selected_parent
                            .selected_bond_model
                            .as_ref()
                            .ok_or("missing selected parent model")?,
                        &derivative_state,
                        &bridge_unsaturation,
                        precursor.as_ref(),
                        selected_parent.fusion_plan(),
                        selected_parent.fusion_plan().is_none(),
                    )?
                    else {
                        continue;
                    };
                    let rendered_name =
                        render_bridge_name(&selected_parent, &operations, &bridge_unsaturation);
                    let rank = wrapper_rank(
                        parent_atoms.len(),
                        &operations,
                        &derivative_state,
                        map_index,
                        &rendered_name,
                    );
                    let plan = BridgedFusionWrapperPlan {
                        parent: selected_parent,
                        bridges: operations,
                        derivative_state,
                        rendered_name,
                        audit_ok: true,
                        audit_checks,
                        search_states: visited_states,
                        bridge_unsaturation_operations: bridge_unsaturation,
                        saturated_bridge_precursor: precursor,
                    };
                    plan.validate()?;
                    candidates.push((rank, plan));
                }
            }
            if !candidates.is_empty() {
                break;
            }
        }
        if !candidates.is_empty() {
            candidates.sort_by(|a, b| a.0.cmp(&b.0));
            let best = candidates[0].0.clone();
            let mut tied: Vec<_> = candidates
                .into_iter()
                .take_while(|(rank, _)| *rank == best)
                .map(|(_, plan)| plan)
                .collect();
            if tied.len() > 1 {
                let ranks = canonical_ranks(mol, None);
                tied.sort_by_key(|plan| {
                    let mut locants: Vec<_> = plan.atom_to_locant().into_iter().collect();
                    locants.sort_by_key(|(_, l)| locant_key(l));
                    locants.iter().map(|(a, _)| ranks[a]).collect::<Vec<_>>()
                });
            }
            let selected = tied.into_iter().next();
            // Upstream divergence: the wrapper's retained-like citation renders
            // its parent hydride, hydro operations and bridge unsaturation, but
            // has no grammar for additional multiple bonds inside the parent.
            // After preserving upstream's exact best choice, reject a name that
            // would erase those graph-proved bonds rather than try another name.
            if selected
                .as_ref()
                .is_some_and(|plan| !plan.derivative_state.unsaturation_operations.is_empty())
            {
                return Err(
                    "openclatura: bridged fusion parent additional unsaturation is not ported"
                        .into(),
                );
            }
            return Ok(selected);
        }
    }
    Ok(None)
}

fn key_locant(l: &str) -> ComparisonKey {
    let (base, suffix, text, distance) = locant_key(l);
    ComparisonKey::tuple([
        base.into(),
        (suffix as usize).into(),
        text.into(),
        distance.into(),
    ])
}
fn wrapper_rank(
    parent_len: usize,
    operations: &[NondetachableBridgeOperation],
    state: &ParentDerivativeState,
    map_index: usize,
    rendered: &str,
) -> ComparisonKey {
    let mut all_endpoints: Vec<_> = operations
        .iter()
        .flat_map(|o| [&o.endpoint_locants.0, &o.endpoint_locants.1])
        .map(|l| key_locant(l))
        .collect();
    all_endpoints.sort();
    ComparisonKey::tuple([
        ComparisonKey::Int(-(parent_len as i64)),
        ComparisonKey::tuple(all_endpoints),
        ComparisonKey::tuple(operations.iter().map(|o| {
            ComparisonKey::tuple([
                key_locant(&o.endpoint_locants.0),
                key_locant(&o.endpoint_locants.1),
            ])
        })),
        ComparisonKey::tuple(
            operations.iter().map(|o| {
                ComparisonKey::tuple(o.unsaturation_locants.iter().cloned().map(Into::into))
            }),
        ),
        ComparisonKey::tuple(
            state
                .hydro_operations
                .iter()
                .flat_map(|o| &o.locants)
                .map(|l| key_locant(l)),
        ),
        ComparisonKey::tuple(operations.iter().map(|o| o.prefix.clone().into())),
        map_index.into(),
        rendered.into(),
    ])
}

fn prefer_completed_system_bridge_unsaturation(parent: &WrapperParentPlan) -> Result<bool, String> {
    if parent.kind() == WrapperParentKind::Retained {
        return Ok(false);
    }
    let Some(plan) = parent.fusion_plan() else {
        return Ok(false);
    };
    let registry = fusion_component_registry();
    for occurrence in &plan.ast.component_occurrences {
        let spec = registry.spec_for_match(occurrence)?;
        if spec.rings.len() == 1 && !component_h_locants(&spec, "C")?.is_empty() {
            return Ok(true);
        }
    }
    Ok(false)
}
fn bridge_dehydrogenation(
    mol: &Molecule,
    bridges: &[NondetachableBridgeOperation],
    parent_locants: &LocantMap,
) -> Option<Vec<UnsaturationOperation>> {
    let [bridge] = bridges else { return None };
    if bridge.kind != NondetachableBridgeKind::Carbo || bridge.unsaturation_locants.is_empty() {
        return None;
    }
    let next = parent_locants.values().map(|l| locant_key(l).0).max()? + 1;
    let mut path = bridge.atom_ids.clone();
    if locant_key(&bridge.endpoint_locants.0) < locant_key(&bridge.endpoint_locants.1) {
        path.reverse();
    }
    let locants: LocantMap = path
        .iter()
        .enumerate()
        .map(|(i, a)| (*a, (next + i).to_string()))
        .collect();
    let mut result = Vec::new();
    let mut unsaturated = AtomSet::new();
    for pair in path.windows(2) {
        let bond = mol.get_bond(pair[0], pair[1])?;
        if bond.order != 2 {
            continue;
        }
        if pair.iter().any(|a| unsaturated.contains(a)) {
            return None;
        }
        unsaturated.extend(pair);
        result.push(UnsaturationOperation {
            key: format!(
                "fusion:bridge:dehydro:{},{}",
                locants[&pair[0]], locants[&pair[1]]
            ),
            locants: vec![locants[&pair[0]].clone(), locants[&pair[1]].clone()],
            atom_ids: pair.to_vec(),
            bond_id: bond.idx,
            bond_order: 2,
        });
    }
    Some(result)
}
fn saturated_bridge_precursor(
    bridge: &NondetachableBridgeOperation,
) -> NondetachableBridgeOperation {
    let mut precursor = bridge.clone();
    precursor.prefix = carbo_bridge_prefix(bridge.atom_ids.len(), &[]);
    precursor.internal_bond_orders.fill(1);
    precursor.unsaturation_locants.clear();
    precursor
}

fn removal_groups(
    segments: &[AtomSet],
    maximum: Option<usize>,
    limit: usize,
) -> BTreeMap<usize, BTreeSet<AtomSet>> {
    struct CandidateGroups {
        generated: usize,
        limit: usize,
        maximum: Option<usize>,
        groups: BTreeMap<usize, BTreeSet<AtomSet>>,
    }
    fn choose(
        segments: &[AtomSet],
        start: usize,
        left: usize,
        current: AtomSet,
        state: &mut CandidateGroups,
    ) {
        if state.generated > state.limit {
            return;
        }
        if left == 0 {
            state.generated += 1;
            if state.generated <= state.limit
                && state.maximum.is_none_or(|max| current.len() <= max)
            {
                state
                    .groups
                    .entry(current.len())
                    .or_default()
                    .insert(current);
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
                current.union(&segments[index]).copied().collect(),
                state,
            );
            if state.generated > state.limit {
                break;
            }
        }
    }
    let mut state = CandidateGroups {
        generated: 0,
        limit,
        maximum,
        groups: BTreeMap::new(),
    };
    for count in 1..=segments.len() {
        choose(segments, 0, count, AtomSet::new(), &mut state);
        if state.generated > limit {
            break;
        }
    }
    state.groups
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
    let mut interiors = BTreeSet::new();
    for &junction in &junctions {
        let mut neighbors: Vec<_> = mol
            .get_neighbors(junction)
            .iter()
            .filter(|n| atoms.contains(n))
            .copied()
            .collect();
        neighbors.sort();
        for neighbor in neighbors {
            if junctions.contains(&neighbor) {
                continue;
            }
            let mut path = AtomSet::new();
            let (mut previous, mut current) = (junction, neighbor);
            while !junctions.contains(&current) {
                if degrees.get(&current) != Some(&2) || !path.insert(current) {
                    path.clear();
                    break;
                }
                let following: Vec<_> = mol
                    .get_neighbors(current)
                    .iter()
                    .filter(|&&a| atoms.contains(&a) && a != previous)
                    .copied()
                    .collect();
                let [next] = following.as_slice() else {
                    path.clear();
                    break;
                };
                (previous, current) = (current, *next);
            }
            if !path.is_empty() && current != junction {
                interiors.insert(path);
            }
        }
    }
    let mut result: Vec<_> = interiors.into_iter().collect();
    result.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
    result
}

fn retained_matches(
    mol: &Molecule,
    atoms: &AtomSet,
    allow_nonaromatic: bool,
    allow_relocated_indicated_h: bool,
) -> Vec<RetainedGraphTemplateMatch> {
    match_retained_graph_templates_with_options(
        mol,
        atoms,
        MatchOptions {
            family: Some("fused"),
            allow_nonaromatic,
            allow_relocated_indicated_h,
            ..Default::default()
        },
    )
}
fn oxo_wrapper_hydrogen_state(
    mol: &Molecule,
    matched: &mut RetainedGraphTemplateMatch,
) -> Result<(), String> {
    if matched.indicated_h.is_empty() {
        return Ok(());
    }
    let oxo: Vec<_> = matched
        .locant_to_atom
        .iter()
        .filter(|(_, a)| {
            mol.atoms[a].is_carbon()
                && mol.get_neighbors(**a).iter().any(|n| {
                    !matched.matched_atoms.contains(n)
                        && mol.atoms[n].symbol == "O"
                        && mol.get_bond(**a, *n).is_some_and(|b| b.order == 2)
                        && is_neutral_external_pi_ligand(mol, **a, *n)
                })
        })
        .map(|(l, _)| l.clone())
        .collect();
    if oxo.is_empty() {
        return Ok(());
    }
    let additional_pi = |citation: &[String]| -> Result<Option<usize>, String> {
        let Ok(model) = retained_template_parent_bond_model(
            &matched.template,
            &matched.locant_to_atom,
            Some(citation),
        ) else {
            return Ok(None);
        };
        Ok(parent_derivative_state(
            mol,
            &matched.matched_atoms,
            &model,
            &matched.atom_to_locant,
            &ParentDerivativeOptions {
                preserve_retained_parent_state: true,
                allow_pi_redistribution: Some(true),
                ..Default::default()
            },
        )?
        .map(|state| state.unsaturation_operations.len()))
    };
    let Some(mut best) = additional_pi(&matched.indicated_h)? else {
        return Ok(());
    };
    if best == 0 {
        return Ok(());
    }
    let mut selected = matched.indicated_h.clone();
    for old in &matched.indicated_h {
        if !mol.atoms[&matched.locant_to_atom[old]].is_carbon() {
            continue;
        }
        for new in &oxo {
            if matched.indicated_h.contains(new) {
                continue;
            }
            let mut citation: Vec<_> = matched
                .indicated_h
                .iter()
                .filter(|l| *l != old)
                .cloned()
                .chain(std::iter::once(new.clone()))
                .collect();
            citation.sort_by_key(|l| locant_key(l));
            if let Some(count) = additional_pi(&citation)?
                && count < best
            {
                selected = citation;
                best = count;
            }
        }
    }
    matched.indicated_h = selected;
    Ok(())
}
fn retained_wrapper_parent(
    mol: &Molecule,
    atoms: &AtomSet,
) -> Result<Option<WrapperParentPlan>, String> {
    let mut matches = retained_matches(mol, atoms, false, false);
    if matches.is_empty() {
        matches = retained_matches(mol, atoms, true, true);
    }
    let registry = fusion_component_registry();
    matches.retain(|m| {
        // Upstream divergence: retained Rust templates omit duplicate ring arrays; use registered rings or the same bounded-face model for the eligibility gate.
        let ring_gate = registry.template_spec(&m.template.name).map_or_else(
            || {
                cached_bounded_face_model(mol, atoms)
                    .ok()
                    .flatten()
                    .is_some_and(|model| {
                        fusion_ring_size_gate(
                            &model
                                .faces
                                .iter()
                                .map(|f| f.atoms.len())
                                .collect::<Vec<_>>(),
                        )
                    })
            },
            |spec| fusion_ring_size_gate(&spec.ring_sizes()),
        );
        ring_gate
            && !(m.indicated_h != m.template.default_indicated_h
                && retained_parent_name_policy(&m.template.name)
                    .is_some_and(|p| p.hydrogenation.is_some()))
    });
    if matches.is_empty() {
        return Ok(None);
    }
    for matched in &mut matches {
        let by_locant = matched.template.atom_by_locant();
        if matched.template.default_indicated_h.is_empty()
            || matched
                .template
                .default_indicated_h
                .iter()
                .chain(&matched.indicated_h)
                .any(|l| {
                    by_locant[l].symbol != "C" && !mol.atoms[&matched.locant_to_atom[l]].is_aromatic
                })
        {
            matched.indicated_h = matched.template.default_indicated_h.clone();
        }
        oxo_wrapper_hydrogen_state(mol, matched)?;
    }
    let mut first = matches[0].clone();
    let mut same_parent: Vec<_> = matches
        .into_iter()
        .filter(|m| m.template.name == first.template.name && m.indicated_h == first.indicated_h)
        .collect();
    let first_model = match retained_template_parent_bond_model(
        &first.template,
        &first.locant_to_atom,
        Some(&first.indicated_h),
    ) {
        Ok(model) => model,
        Err(error) => {
            if first.indicated_h == first.template.default_indicated_h {
                return Err(error);
            }
            for matched in &mut same_parent {
                matched.indicated_h = matched.template.default_indicated_h.clone();
            }
            first = same_parent[0].clone();
            retained_template_parent_bond_model(&first.template, &first.locant_to_atom, None)?
        }
    };
    // Preserve insertion order: the first retained match is the first wrapper map.
    let mut candidates = vec![(first.atom_to_locant.clone(), first_model)];
    for matched in same_parent {
        if !candidates.iter().any(|(m, _)| *m == matched.atom_to_locant) {
            let model = retained_template_parent_bond_model(
                &matched.template,
                &matched.locant_to_atom,
                Some(&matched.indicated_h),
            )?;
            candidates.push((matched.atom_to_locant, model));
        }
    }
    let metadata = retained_parent_metadata(&first.template.name).map(|mut metadata| {
        metadata.default_indicated_h = first.indicated_h.clone();
        if let Some(stem) = &metadata.derivative_stem {
            metadata.derivative_stem = Some(render_retained_hydrogen_state(
                stem,
                &first.template.default_indicated_h,
                Some(&first.indicated_h),
            ));
        }
        if first.indicated_h != first.template.default_indicated_h {
            let by_locant = first.template.atom_by_locant();
            let mut inherent: Vec<_> = metadata
                .inherent_saturated_locants
                .iter()
                .filter(|l| !first.template.default_indicated_h.contains(l))
                .cloned()
                .chain(
                    first
                        .indicated_h
                        .iter()
                        .filter(|l| by_locant[*l].symbol == "C")
                        .cloned(),
                )
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            inherent.sort_by_key(|l| locant_key(l));
            metadata.inherent_saturated_locants = inherent;
        }
        metadata.relocated_indicated_h = first.indicated_h != first.template.default_indicated_h;
        metadata
    });
    let (locant_maps, bond_models) = candidates.into_iter().unzip();
    let parent = WrapperParentPlan {
        hydride: RingParent::from_retained_locant_maps(
            atoms.clone(),
            locant_maps,
            retained_parent_output_name(
                &first.template.name,
                "wrapped_parent",
                &first.template.default_indicated_h,
                Some(&first.indicated_h),
            ),
            metadata,
        )?,
        bond_models,
        selected_locant_map: None,
        selected_bond_model: None,
    };
    parent.validate()?;
    Ok(Some(parent))
}
fn systematic_fusion_parent(
    mol: &Molecule,
    atoms: &AtomSet,
) -> Result<Option<WrapperParentPlan>, String> {
    let Some(plan) = plan_fusion_parent(mol, atoms)? else {
        return Ok(None);
    };
    let bond_models = if plan.numbering_variants.is_empty() {
        vec![plan.bond_model.clone()]
    } else {
        plan.numbering_variants
            .iter()
            .map(|v| v.bond_model.clone())
            .collect()
    };
    let parent = WrapperParentPlan {
        hydride: RingParent::from_fusion_plan(plan)?,
        bond_models,
        selected_locant_map: None,
        selected_bond_model: None,
    };
    parent.validate()?;
    Ok(Some(parent))
}

fn bridge_path_components(
    mol: &Molecule,
    all_atoms: &AtomSet,
    parent: &AtomSet,
    removed: &AtomSet,
) -> Option<Vec<Vec<usize>>> {
    let removed_edges = edges_within_atoms(mol, all_atoms)
        .into_iter()
        .filter(|(u, v)| removed.contains(u) && removed.contains(v))
        .collect();
    let mut paths = Vec::new();
    for component in connected_components(removed, &removed_edges) {
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
        if degrees.values().any(|d| *d > 2) {
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
        let start = *degrees.iter().find(|(_, d)| **d <= 1)?.0;
        let path = walk_path(mol, &component, start);
        if path.iter().copied().collect::<AtomSet>() != component {
            return None;
        }
        paths.push(path);
    }
    paths.sort();
    Some(paths)
}
fn walk_path(mol: &Molecule, atoms: &AtomSet, start: usize) -> Vec<usize> {
    let mut path = vec![start];
    let mut previous = None;
    while path.len() < atoms.len() {
        let Some(next) = mol
            .get_neighbors(*path.last().unwrap_or(&start))
            .iter()
            .filter(|&&n| atoms.contains(&n) && Some(n) != previous)
            .min()
            .copied()
        else {
            break;
        };
        previous = path.last().copied();
        path.push(next);
    }
    path
}
fn bridge_operations(
    mol: &Molecule,
    paths: &[Vec<usize>],
    parent: &AtomSet,
    locants: &LocantMap,
) -> Option<Vec<NondetachableBridgeOperation>> {
    let mut operations = Vec::new();
    for path in paths {
        let OrientedBridgePath {
            atoms: path,
            endpoints,
        } = orient_bridge_path(mol, path, parent, locants)?;
        if mol.get_bond(path[0], endpoints.0)?.order != 1
            || mol.get_bond(*path.last()?, endpoints.1)?.order != 1
            || mol.get_bond(endpoints.0, endpoints.1).is_some()
        {
            return None;
        }
        let (kind, prefix, orders, unsaturation) = bridge_class(mol, &path)?;
        let mut scope: AtomSet = path.iter().copied().collect();
        scope.extend([endpoints.0, endpoints.1]);
        let endpoint_atoms = AtomSet::from([endpoints.0, endpoints.1]);
        let operation = NondetachableBridgeOperation {
            kind,
            prefix,
            atom_ids: path,
            endpoint_atom_ids: endpoints,
            endpoint_locants: (
                locants.get(&endpoints.0)?.clone(),
                locants.get(&endpoints.1)?.clone(),
            ),
            bond_ids: bond_ids_within(mol, &scope)
                .difference(&bond_ids_within(mol, &endpoint_atoms))
                .copied()
                .collect(),
            internal_bond_orders: orders,
            unsaturation_locants: unsaturation,
        };
        operation.validate().ok()?;
        operations.push(operation);
    }
    Some(operations)
}
struct OrientedBridgePath {
    atoms: Vec<usize>,
    endpoints: (usize, usize),
}

fn orient_bridge_path(
    mol: &Molecule,
    path: &[usize],
    parent: &AtomSet,
    locants: &LocantMap,
) -> Option<OrientedBridgePath> {
    let attachments = |a: usize| {
        let mut neighbors: Vec<_> = mol
            .get_neighbors(a)
            .iter()
            .filter(|n| parent.contains(n))
            .copied()
            .collect();
        neighbors.sort();
        neighbors
    };
    if path.len() == 1 {
        let mut endpoints = attachments(path[0]);
        if endpoints.len() != 2 {
            return None;
        }
        endpoints.sort_by_key(|a| locant_key(&locants[a]));
        return Some(OrientedBridgePath {
            atoms: path.to_vec(),
            endpoints: (endpoints[0], endpoints[1]),
        });
    }
    if path.is_empty()
        || path[1..path.len() - 1]
            .iter()
            .any(|a| !attachments(*a).is_empty())
    {
        return None;
    }
    let (left, right) = (attachments(path[0]), attachments(*path.last()?));
    if left.len() != 1 || right.len() != 1 {
        return None;
    }
    let forward = OrientedBridgePath {
        atoms: path.to_vec(),
        endpoints: (left[0], right[0]),
    };
    let reverse = OrientedBridgePath {
        atoms: path.iter().rev().copied().collect(),
        endpoints: (right[0], left[0]),
    };
    if composite_bridge_construction(mol, &forward.atoms).is_some() {
        return Some(forward);
    }
    if composite_bridge_construction(mol, &reverse.atoms).is_some() {
        return Some(reverse);
    }
    let orientation_key = |candidate: &OrientedBridgePath| {
        (
            candidate
                .atoms
                .windows(2)
                .enumerate()
                .filter_map(|(i, p)| {
                    mol.get_bond(p[0], p[1])
                        .filter(|b| b.order == 2)
                        .map(|_| i + 1)
                })
                .collect::<Vec<_>>(),
            locant_key(&locants[&candidate.endpoints.0]),
            locant_key(&locants[&candidate.endpoints.1]),
        )
    };
    Some(if orientation_key(&forward) <= orientation_key(&reverse) {
        forward
    } else {
        reverse
    })
}
fn composite_bridge_construction(
    mol: &Molecule,
    path: &[usize],
) -> Option<&'static CompositeBridgeConstruction> {
    if path.len() < 2
        || mol.atoms[&path[0]].symbol == "C"
        || path.iter().any(|a| mol.atoms[a].charge != 0)
    {
        return None;
    }
    let symbols: Vec<_> = path.iter().map(|a| mol.atoms[a].symbol.clone()).collect();
    let orders: Vec<_> = path
        .windows(2)
        .map(|p| mol.get_bond(p[0], p[1]).map(|b| b.order))
        .collect::<Option<_>>()?;
    composite_bridge_constructions().get(&(symbols, orders))
}
type BridgeClass = (NondetachableBridgeKind, String, Vec<usize>, Vec<String>);
fn bridge_class(mol: &Molecule, path: &[usize]) -> Option<BridgeClass> {
    // Upstream divergence: bridge prefixes describe hydride atoms, including
    // composite bridges. Prove each atom's full bonding number and H state
    // before choosing a prefix from element identity and internal bonds.
    for &id in path {
        let atom = &mol.atoms[&id];
        let bonding_number = atom.total_h_count
            + mol
                .get_neighbors(id)
                .iter()
                .map(|&n| mol.get_bond(id, n).map_or(0, |b| b.order))
                .sum::<usize>();
        if atom.charge != 0
            || (bonding_number != atom.element()?.standard_valence
                && neutral_lambda_oxo_bonding_number(mol, id) != Some(bonding_number))
        {
            return None;
        }
    }
    let orders: Vec<_> = path
        .windows(2)
        .map(|p| mol.get_bond(p[0], p[1]).map(|b| b.order))
        .collect::<Option<_>>()?;
    if path.iter().all(|a| mol.atoms[a].symbol == "C") {
        if orders.iter().any(|o| ![1, 2].contains(o)) {
            return None;
        }
        let double_locants: Vec<_> = orders
            .iter()
            .enumerate()
            .filter(|(_, o)| **o == 2)
            .map(|(i, _)| (i + 1).to_string())
            .collect();
        return Some((
            NondetachableBridgeKind::Carbo,
            carbo_bridge_prefix(path.len(), &double_locants),
            orders,
            double_locants,
        ));
    }
    if let Some(composite) = composite_bridge_construction(mol, path) {
        let unsaturation = composite
            .carbon_bond_orders
            .iter()
            .enumerate()
            .filter(|(_, o)| **o == 2)
            .map(|(i, _)| (i + 1).to_string())
            .collect();
        return Some((
            NondetachableBridgeKind::Composite,
            format!("({}{})", composite.hetero_prefix, composite.carbon_prefix),
            orders,
            unsaturation,
        ));
    }
    if !orders.is_empty() {
        return None;
    }
    let (kind, prefix) = match path.first().map(|a| mol.atoms[a].symbol.as_str())? {
        "O" => (NondetachableBridgeKind::Epoxy, "epoxy"),
        "S" => (NondetachableBridgeKind::Epithio, "epithio"),
        "N" => (NondetachableBridgeKind::Epimino, "epimino"),
        _ => return None,
    };
    Some((kind, prefix.into(), Vec::new(), Vec::new()))
}

fn wrapper_parent_derivative_state(
    mol: &Molecule,
    atoms: &AtomSet,
    model: &ParentBondModel,
    locants: &LocantMap,
    fusion_plan: Option<&FusionParentPlan>,
    retained_redistribution: bool,
) -> Result<Option<ParentDerivativeState>, String> {
    let indicated: AtomSet = fusion_plan
        .into_iter()
        .flat_map(|plan| &plan.indicated_hydrogens)
        .filter_map(|locant| {
            locants
                .iter()
                .find(|(_, value)| *value == locant)
                .map(|(&atom, _)| atom)
        })
        .collect();
    if let Some(plan) = fusion_plan {
        let state = &plan.derivative_state;
        if state.external_pi_operations().next().is_none()
            || !state.bond_delta.added_hydrogen_operations.is_empty()
            || !state.hydro_operations.is_empty()
        {
            return Ok(Some(state.clone()));
        }
        // Upstream divergence: the standalone systematic-parent comparison may
        // suppress external-pi composition in the presence of an intrinsic
        // saturated carbon. Reprove composition for the wrapper so an oxo,
        // imino or alkylidene operation retains its required added-H citation.
        if let Some(composed) = parent_derivative_state(
            mol,
            atoms,
            model,
            locants,
            &ParentDerivativeOptions {
                indicated_hydrogen_atom_ids: Some(&indicated),
                preserve_retained_parent_state: true,
                allow_pi_redistribution: Some(true),
                compose_retained_external_pi: true,
            },
        )? && !composed.bond_delta.added_hydrogen_operations.is_empty()
        {
            // Upstream divergence: the alkylidene prefix assembler separately
            // hydrogenates its attachment edge. Combining that operation with
            // this added-H citation is not yet a proved wrapper composition.
            if !state.alkylidene_operations.is_empty() {
                return Err(
                    "openclatura: bridged fusion alkylidene added hydrogen is not ported".into(),
                );
            }
            return Ok(Some(composed));
        }
        return Ok(Some(state.clone()));
    }
    parent_derivative_state(
        mol,
        atoms,
        model,
        locants,
        &ParentDerivativeOptions {
            preserve_retained_parent_state: true,
            allow_pi_redistribution: retained_redistribution.then_some(true),
            compose_retained_external_pi: true,
            ..Default::default()
        },
    )
}
fn carbo_bridge_prefix(length: usize, double_locants: &[String]) -> String {
    let stem = stems::stem_for(length);
    if double_locants.is_empty() {
        return format!("{stem}ano");
    }
    if length == 2 && double_locants == ["1"] {
        return "etheno".into();
    }
    let multiplier = if double_locants.len() == 1 {
        String::new()
    } else {
        multipliers::basic(double_locants.len())
    };
    format!(
        "{stem}{}[{}]{multiplier}eno",
        if double_locants.len() > 1 { "a" } else { "" },
        double_locants.join(",")
    )
}
fn render_bridge_name(
    parent: &WrapperParentPlan,
    operations: &[NondetachableBridgeOperation],
    unsaturation: &[UnsaturationOperation],
) -> String {
    let mut name = String::new();
    if !unsaturation.is_empty() {
        let locants: Vec<_> = unsaturation
            .iter()
            .flat_map(|o| o.locants.iter().cloned())
            .collect();
        name.push_str(&format!(
            "{}-{}dehydro-",
            locants.join(","),
            multipliers::basic(locants.len())
        ));
    }
    for (index, operation) in operations.iter().enumerate() {
        let prefix = if unsaturation.is_empty() {
            operation.prefix.clone()
        } else {
            carbo_bridge_prefix(operation.atom_ids.len(), &[])
        };
        if unsaturation.is_empty() {
            name.push_str(&operation.rendered());
        } else {
            name.push_str(&format!(
                "{},{}-{prefix}",
                operation.endpoint_locants.0, operation.endpoint_locants.1
            ));
        }
        if index + 1 < operations.len() || needs_hyphen(&prefix, parent.name()) {
            name.push('-');
        }
    }
    name.push_str(parent.name());
    name
}

#[allow(clippy::too_many_arguments)]
fn audit_bridge_plan(
    mol: &Molecule,
    all_atoms: &AtomSet,
    parent: &AtomSet,
    locants: &LocantMap,
    operations: &[NondetachableBridgeOperation],
    model: &ParentBondModel,
    state: &ParentDerivativeState,
    unsaturation: &[UnsaturationOperation],
    precursor: Option<&NondetachableBridgeOperation>,
    fusion_plan: Option<&FusionParentPlan>,
    retained_redistribution: bool,
) -> Result<Option<Vec<String>>, String> {
    if locants.keys().copied().collect::<AtomSet>() != *parent
        || locants.values().collect::<BTreeSet<_>>().len() != parent.len()
    {
        return Ok(None);
    }
    let bridge_atoms: AtomSet = operations
        .iter()
        .flat_map(|o| o.atom_ids.iter().copied())
        .collect();
    if operations.iter().map(|o| o.atom_ids.len()).sum::<usize>() != bridge_atoms.len()
        || !parent.is_disjoint(&bridge_atoms)
        || parent.union(&bridge_atoms).copied().collect::<AtomSet>() != *all_atoms
    {
        return Ok(None);
    }
    let mut expected_edges = edges_within_atoms(mol, parent);
    for operation in operations {
        let path = &operation.atom_ids;
        if path.is_empty() || path.windows(2).any(|p| mol.get_bond(p[0], p[1]).is_none()) {
            return Ok(None);
        }
        let attachments: AtomSet = path
            .iter()
            .flat_map(|a| mol.get_neighbors(*a).iter().copied())
            .filter(|a| parent.contains(a))
            .collect();
        let endpoints =
            AtomSet::from([operation.endpoint_atom_ids.0, operation.endpoint_atom_ids.1]);
        if attachments != endpoints
            || (&operation.endpoint_locants.0, &operation.endpoint_locants.1)
                != (
                    &locants[&operation.endpoint_atom_ids.0],
                    &locants[&operation.endpoint_atom_ids.1],
                )
        {
            return Ok(None);
        }
        let Some((kind, prefix, orders, unsaturation_locants)) = bridge_class(mol, path) else {
            return Ok(None);
        };
        if operation.kind != kind
            || operation.prefix != prefix
            || operation.internal_bond_orders != orders
            || operation.unsaturation_locants != unsaturation_locants
        {
            return Ok(None);
        }
        let scope: AtomSet = path
            .iter()
            .copied()
            .chain(endpoints.iter().copied())
            .collect();
        expected_edges.extend(
            edges_within_atoms(mol, &scope)
                .difference(&edges_within_atoms(mol, &endpoints))
                .copied(),
        );
        if operation.bond_ids
            != bond_ids_within(mol, &scope)
                .difference(&bond_ids_within(mol, &endpoints))
                .copied()
                .collect()
        {
            return Ok(None);
        }
    }
    if expected_edges != edges_within_atoms(mol, all_atoms) {
        return Ok(None);
    }
    if let Some(plan) = fusion_plan {
        if plan.bond_model != *model
            || plan.numbering.string_input_locant_maps().first() != Some(locants)
        {
            return Ok(None);
        }
        let indicated: AtomSet = locants
            .iter()
            .filter(|(_, locant)| plan.indicated_hydrogens.contains(locant))
            .map(|(&atom, _)| atom)
            .collect();
        let base_state = parent_derivative_state(
            mol,
            parent,
            model,
            locants,
            &ParentDerivativeOptions {
                indicated_hydrogen_atom_ids: Some(&indicated),
                allow_pi_redistribution: retained_redistribution.then_some(true),
                ..Default::default()
            },
        )?;
        if base_state.as_ref() != Some(&plan.derivative_state) {
            return Ok(None);
        }
    }
    let replayed = wrapper_parent_derivative_state(
        mol,
        parent,
        model,
        locants,
        fusion_plan,
        retained_redistribution,
    )?;
    if replayed.as_ref() != Some(state) || !unsaturation.is_empty() != precursor.is_some() {
        return Ok(None);
    }
    if !unsaturation.is_empty() {
        if operations.len() != 1
            || precursor != Some(&saturated_bridge_precursor(&operations[0]))
            || bridge_dehydrogenation(mol, operations, locants).as_deref() != Some(unsaturation)
        {
            return Ok(None);
        }
        let mut degrees: BTreeMap<_, usize> = all_atoms.iter().map(|a| (*a, 0)).collect();
        let mut double_edges: Vec<_> = state
            .bond_delta
            .assignment
            .orders
            .iter()
            .filter(|(_, o)| *o == 2)
            .map(|(edge, _)| *edge)
            .collect();
        double_edges.extend(
            unsaturation
                .iter()
                .filter_map(|o| match o.atom_ids.as_slice() {
                    [u, v] => Some((*u, *v)),
                    _ => None,
                }),
        );
        for (u, v) in double_edges {
            *degrees.entry(u).or_default() += 1;
            *degrees.entry(v).or_default() += 1;
        }
        let aromatic_carbon = all_atoms
            .iter()
            .all(|a| mol.atoms[a].is_carbon() && mol.atoms[a].is_aromatic);
        if degrees.values().any(|d| *d > 1) || aromatic_carbon && degrees.values().any(|d| *d != 1)
        {
            return Ok(None);
        }
    }
    let mut checks: Vec<_> = [
        "complete_bijective_parent_locants",
        "disjoint_parent_and_bridge_atoms",
        "simple_bridge_paths",
        "exact_bridge_endpoints_and_locants",
        "exact_bridge_bond_ownership",
        "typed_bridge_bond_and_prefix_model",
        "parent_derivative_state",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    if !unsaturation.is_empty() {
        checks.extend([
            "graph_bound_bridge_dehydrogenation".into(),
            "complete_bridge_pi_assignment".into(),
        ]);
    }
    checks.push("complete_wrapper_graph_reconstruction".into());
    Ok(Some(checks))
}

#[cfg(test)]
mod tests {
    use super::super::super::graph_io::read_smiles;
    use super::*;

    #[test]
    fn external_pi_wrappers_keep_required_added_hydrogen() {
        for (smiles, suffix) in [
            ("O=C1Cc2cc3c(csc3o2)C1", "one"),
            ("N=C1Cc2cc3c(csc3o2)C1", "imine"),
        ] {
            let mol = read_smiles(smiles).unwrap();
            let atoms = super::super::super::chains::get_cyclic_atoms(&mol, &AtomSet::new());
            let plan = plan_bridged_fusion_wrapper(&mol, &atoms).unwrap().unwrap();
            assert_eq!(
                plan.derivative_state.bond_delta.added_hydrogen_operations[0].locants,
                ["6"]
            );
            assert_eq!(
                super::super::super::name_smiles(smiles).unwrap().text,
                format!("1,7-epoxy-4H-cyclohepta[c]thiophen-5(6H)-{suffix}")
            );
        }
        assert_eq!(
            super::super::super::name_smiles("C12=CC=C(C3=CC=CC=C13)CCCCCC2")
                .map(|name| name.text)
                .as_deref(),
            Ok("1,4-hexanonaphthalene")
        );
        assert!(super::super::super::name_smiles("C=C1Cc2cc3c(csc3[nH]2)C1").is_err());
    }

    #[test]
    fn bridge_prefixes_require_hydride_valence_and_neutral_boundary() {
        for smiles in ["[SH2]1c2ccc3cc1ccc3c2", "[O-]n1c2cc3c(occ31)s2"] {
            let mol = read_smiles(smiles).unwrap();
            let atoms = super::super::super::chains::get_cyclic_atoms(&mol, &AtomSet::new());
            assert!(
                plan_bridged_fusion_wrapper(&mol, &atoms).unwrap().is_none(),
                "{smiles}"
            );
        }
        for smiles in [
            "S1c2ccc3cc1ccc3c2",
            "N1c2ccc3cc1ccc3c2",
            "O=S1c2ccc3cc1ccc3c2",
        ] {
            let mol = read_smiles(smiles).unwrap();
            let atoms = super::super::super::chains::get_cyclic_atoms(&mol, &AtomSet::new());
            assert!(
                plan_bridged_fusion_wrapper(&mol, &atoms).unwrap().is_some(),
                "{smiles}"
            );
        }
    }

    #[test]
    fn retained_fused_bridge_wrappers_preserve_typed_graph_proofs() {
        for (smiles, expected) in [
            ("C12=CC=C(C3=CC=CC=C13)O2", "1,4-epoxynaphthalene"),
            (
                "C12=CC=C(C=3C4=CC=C(C13)C4)O2",
                "1,4-epoxy-5,8-methanonaphthalene",
            ),
            (
                "C12=CC(=CC3=CC=4C5=CC=C(C4C=C13)O5)C2",
                "5,8-epoxy-1,3-methanoanthracene",
            ),
            ("N12C=CC3=CC(=CC=C13)C2", "1,5-methanoindole"),
            ("C12=CC=C(C3=CC=CC=C13)CC=C2", "1,4-prop[1]enonaphthalene"),
            ("C12=CC=C(C3=CC=CC=C13)C=CC2", "1,4-prop[1]enonaphthalene"),
            (
                "C12=CC=C(C3=CC=CC=C13)C=CC=C2",
                "1,4-buta[1,3]dienonaphthalene",
            ),
            ("C12=CC=C(C3=CC=CC=C13)C=C2", "1,4-ethenonaphthalene"),
            ("C12=CC=C(C3=CC=CC=C13)CCCCCC2", "1,4-hexanonaphthalene"),
        ] {
            let mol = read_smiles(smiles).unwrap();
            let atoms = mol.atoms.keys().copied().collect();
            let plan = plan_bridged_fusion_wrapper(&mol, &atoms).unwrap().unwrap();
            assert_eq!(plan.rendered_name, expected, "{smiles}");
            assert!(plan.audit_ok);
            assert_eq!(
                plan.atom_to_locant().keys().copied().collect::<AtomSet>(),
                atoms
            );
            assert!(!plan.audit_checks.is_empty());
        }
    }
    #[test]
    fn wrapper_bounds_and_annulation_gate_are_graph_based() {
        let mol = read_smiles("C12=CC=C(C3=CC=CC=C13)CCCCCC2").unwrap();
        let atoms = mol.atoms.keys().copied().collect();
        assert!(
            plan_bridged_fusion_wrapper_with_options(&mol, &atoms, Some(0))
                .unwrap()
                .is_none()
        );
        assert!(
            plan_bridged_fusion_wrapper_with_options(&mol, &atoms, Some(5))
                .unwrap()
                .is_none()
        );
        for smiles in ["C1CC2CCC1C2", "C1C2CC3CC1CC(C2)C3", "c1ccc2ccccc2c1"] {
            let mol = read_smiles(smiles).unwrap();
            let atoms = mol.atoms.keys().copied().collect();
            assert!(
                plan_bridged_fusion_wrapper(&mol, &atoms).unwrap().is_none(),
                "{smiles}"
            );
        }
    }
    #[test]
    fn inapplicable_bridge_candidates_do_not_propagate_standalone_parent_errors() {
        for smiles in [
            "CC1=CNN=C2CC3C=CC=CNC3[SH]12",
            "BrC1=CNC=C[C@@H]2N=C3C(=CCN4C=CCC[C@H]34)CC=C12",
        ] {
            let mol = read_smiles(smiles).unwrap();
            let atoms = super::super::super::chains::find_ring_systems(&mol, &AtomSet::new())
                .unwrap()
                .into_iter()
                .max_by_key(|system| system.atoms.len())
                .unwrap()
                .atoms;
            assert!(
                plan_bridged_fusion_wrapper(&mol, &atoms).unwrap().is_none(),
                "{smiles}"
            );
        }
    }
    #[test]
    fn composite_bridges_follow_senior_fragment_and_carbon_internal_locants() {
        let mol = read_smiles("COCC=C").unwrap();
        let path = vec![1, 2, 3, 4];
        let (_, prefix, orders, locants) = bridge_class(&mol, &path).unwrap();
        assert_eq!(prefix, "(epoxyprop[2]eno)");
        assert_eq!(orders, vec![1, 1, 2]);
        assert_eq!(locants, vec!["2"]);
        assert_eq!(carbo_bridge_prefix(2, &["1".into()]), "etheno");
        assert_eq!(
            carbo_bridge_prefix(4, &["1".into(), "3".into()]),
            "buta[1,3]dieno"
        );
    }
    #[test]
    fn systematic_fused_spiro_and_bridge_sides_keep_numbering_proof_alignment() {
        use super::super::super::molecule::{Atom, Bond};
        let mut mol = read_smiles("O1C2=C(C=C1)C=CS2").unwrap();
        let atoms = mol.atoms.keys().copied().collect();
        let side = plan_fusion_spiro_side(&mol, &atoms, 4).unwrap().unwrap();
        assert_eq!(side.parent.name(), "thieno[2,3-b]furan");
        assert_eq!(side.junction_locant, "2");
        let fusion = plan_fusion_parent(&mol, &atoms).unwrap().unwrap();
        let by_locant: BTreeMap<_, _> = fusion.numbering.string_input_locant_maps()[0]
            .iter()
            .map(|(a, l)| (l.clone(), *a))
            .collect();
        let atom = mol.atoms.keys().max().unwrap() + 1;
        let bond = mol.bonds.keys().max().unwrap() + 1;
        mol.add_atom(Atom::new(atom, "O")).unwrap();
        mol.add_bond(Bond::new(bond, by_locant["2"], atom, 1))
            .unwrap();
        mol.add_bond(Bond::new(bond + 1, atom, by_locant["3a"], 1))
            .unwrap();
        let all = mol.atoms.keys().copied().collect();
        let plan = plan_bridged_fusion_wrapper(&mol, &all).unwrap().unwrap();
        assert_eq!(plan.parent.kind(), WrapperParentKind::SystematicFusion);
        assert_eq!(plan.rendered_name, "2,3a-epoxythieno[2,3-b]furan");
    }
    #[test]
    fn completed_system_bridge_unsaturation_is_proved_and_rejects_corruption() {
        let mol = read_smiles("c1cc2ccc3ccc4ccc5ccc6ccc1c2c3c4c56").unwrap();
        let atoms = mol.atoms.keys().copied().collect();
        let plan = plan_bridged_fusion_wrapper(&mol, &atoms).unwrap().unwrap();
        assert_eq!(
            plan.rendered_name,
            "13,14-didehydro-9,10-ethanodicyclopenta[c,g]phenanthrene"
        );
        assert_eq!(plan.search_states, 69);
        assert_eq!(
            plan.saturated_bridge_precursor
                .as_ref()
                .unwrap()
                .internal_bond_orders,
            [1]
        );
        assert_eq!(plan.bridge_unsaturation_operations.len(), 1);
        let mut corrupted = plan.bridge_unsaturation_operations.clone();
        corrupted[0].bond_order = 1;
        assert!(
            audit_bridge_plan(
                &mol,
                &atoms,
                plan.parent.atom_ids(),
                &plan.parent.locant_maps()[0],
                &plan.bridges,
                plan.parent.selected_bond_model.as_ref().unwrap(),
                &plan.derivative_state,
                &corrupted,
                plan.saturated_bridge_precursor.as_ref(),
                plan.parent.fusion_plan(),
                false
            )
            .unwrap()
            .is_none()
        );
    }
    #[test]
    fn selected_wrapper_declines_parent_pi_bonds_omitted_by_its_citation() {
        use super::super::super::{chains::find_ring_systems, name_smiles};
        let smiles = r"C/C=C\[C@@H]1CCCC2=C([N+](=O)[O-])C[C@H]3NC[C@]1(N)C[C@H]23";
        let mol = read_smiles(smiles).unwrap();
        let atoms = find_ring_systems(&mol, &AtomSet::new())
            .unwrap()
            .into_iter()
            .max_by_key(|system| system.atoms.len())
            .unwrap()
            .atoms;
        let expected = "openclatura: bridged fusion parent additional unsaturation is not ported";
        assert_eq!(
            plan_bridged_fusion_wrapper(&mol, &atoms).unwrap_err(),
            expected
        );
        assert_eq!(name_smiles(smiles).unwrap_err(), expected);
    }
    #[test]
    fn removal_enumeration_counts_before_filter_and_preserves_size_order() {
        let segments = vec![
            AtomSet::from([0]),
            AtomSet::from([1, 2]),
            AtomSet::from([3]),
        ];
        let groups = removal_groups(&segments, Some(2), 4);
        assert_eq!(
            groups[&1],
            BTreeSet::from([AtomSet::from([0]), AtomSet::from([3])])
        );
        assert_eq!(groups[&2], BTreeSet::from([AtomSet::from([1, 2])]));
    }
}
