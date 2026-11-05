//! Bounded, graph-proved systematic fusion citations and pure rendering.
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use super::super::{
    locants::{LocantKey, retained_locant_sort_key},
    molecule::Molecule,
    rules::multipliers,
};
use super::{
    config::fusion_nomenclature_config,
    cover::{ComponentScope, CoverGraph, FusionInterface, audit_component_cover, component_scope},
    model::*,
    registry::FusionComponentRegistry,
    rules::{
        component_canonicalization_key, component_has_peri_fusion, component_parent_eligible,
        component_spec_seniority_key, component_variant_identity, multiplicative_attachment_key,
        multiplicative_member_order_key, uses_complex_multiplier,
    },
};

type JoinKey = (usize, usize, Vec<LocantKey>);
type Preference = (ComparisonKey, (usize, usize, Vec<i64>));
type OccurrenceIdentity = (BTreeSet<usize>, String, String, BTreeSet<usize>);
struct OccurrenceMappingGroup<'a> {
    face_ids: BTreeSet<usize>,
    atom_ids: BTreeSet<usize>,
    mappings: Vec<&'a FusionComponentMatch>,
}

#[derive(Clone, Debug)]
pub struct DescriptorOptions {
    pub cover_kinds: BTreeSet<String>,
    pub join_kinds: BTreeSet<String>,
    pub multiparent_parents: bool,
    pub enforce_interoperability_limits: bool,
}
impl Default for DescriptorOptions {
    fn default() -> Self {
        let support = &fusion_nomenclature_config().rules.support;
        Self {
            cover_kinds: support.cover_kinds.iter().cloned().collect(),
            join_kinds: support.join_kinds.iter().cloned().collect(),
            multiparent_parents: support.multiparent_parents,
            enforce_interoperability_limits: true,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentSide {
    pub letter: String,
    pub start_locant: String,
    pub end_locant: String,
}
#[derive(Clone)]
struct OccurrenceOption {
    face_ids: BTreeSet<usize>,
    atom_ids: BTreeSet<usize>,
    mappings: Vec<FusionComponentMatch>,
}
#[derive(Clone)]
struct PreparedSelection {
    options: Vec<OccurrenceOption>,
    matches: Vec<FusionComponentMatch>,
    specs: BTreeMap<usize, FusionComponentSpec>,
    graph: CoverGraph<usize>,
    hidden_parent_variants: BTreeSet<ComparisonKey>,
}
#[derive(Clone)]
struct CitationTopology {
    roots: Vec<usize>,
    parent_by_child: BTreeMap<usize, usize>,
    order_by_occurrence: BTreeMap<usize, usize>,
    interparent_pairs: Vec<Edge>,
    cycle_closing_pairs: Vec<Edge>,
    interparent_occurrences: Vec<usize>,
    location_key: ParentLocationKey,
}
#[derive(Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
struct FirstOrderLocantKey {
    parent_letters_set: Vec<(usize, String, usize)>,
    parent_letters_citation: Vec<(usize, String, usize)>,
    attached_locants_set: Vec<(LocantKey, usize)>,
    attached_locants_citation: Vec<(LocantKey, usize)>,
}
#[derive(Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
struct HigherOrderLocantKey {
    host_set: Vec<(LocantKey, usize)>,
    host_citation: Vec<(LocantKey, usize)>,
    attached_set: Vec<(LocantKey, usize)>,
    attached_citation: Vec<(LocantKey, usize)>,
}
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CandidateScore {
    benzoheterocycle_seniority: Vec<ComparisonKey>,
    seniority: ComparisonKey,
    location: ParentLocationKey,
    count: usize,
    maximum_order: usize,
    first_order_locants: FirstOrderLocantKey,
    higher_order_locants: Vec<HigherOrderLocantKey>,
    joins: Vec<JoinKey>,
    attached_seniority: Vec<ComparisonKey>,
}
#[derive(Clone)]
struct Candidate {
    score: CandidateScore,
    ast: FusionNameAst,
}

pub fn component_sides(spec: &FusionComponentSpec) -> Result<Vec<ComponentSide>, String> {
    let order = &spec.peripheral_order;
    if order.len() < 3 {
        return Err(format!(
            "component {:?} has no usable peripheral walk",
            spec.key
        ));
    }
    Ok(order
        .iter()
        .enumerate()
        .map(|(i, left)| ComponentSide {
            letter: alphabetic_index(i),
            start_locant: left.clone(),
            end_locant: order[(i + 1) % order.len()].clone(),
        })
        .collect())
}
pub fn component_bond_locant_sets(spec: &FusionComponentSpec) -> BTreeSet<BTreeSet<String>> {
    spec.bonds()
        .iter()
        .map(|bond| {
            [bond.locants.0.clone(), bond.locants.1.clone()]
                .into_iter()
                .collect()
        })
        .collect()
}
pub fn build_fusion_name_ast(
    mol: &Molecule,
    matches: &[FusionComponentMatch],
    registry: &FusionComponentRegistry,
    options: &DescriptorOptions,
) -> Result<FusionNameAst, String> {
    iter_fusion_name_asts(mol, matches, registry, options)?
        .next()
        .ok_or_else(|| "no exact supported-cover fusion citation was found".to_owned())?
}
/// Only the next intrinsic-parent/location tier is searched when requested.
pub struct FusionNameAstIter<'a> {
    mol: &'a Molecule,
    registry: &'a FusionComponentRegistry,
    options: DescriptorOptions,
    occurrences: Vec<OccurrenceOption>,
    prepared: Vec<PreparedSelection>,
    tiers: VecDeque<(Preference, Vec<usize>)>,
    pending: VecDeque<FusionNameAst>,
    emitted: Vec<FusionNameAst>,
    budget_exhausted: usize,
    terminal: bool,
}
pub fn iter_fusion_name_asts<'a>(
    mol: &'a Molecule,
    matches: &[FusionComponentMatch],
    registry: &'a FusionComponentRegistry,
    options: &DescriptorOptions,
) -> Result<FusionNameAstIter<'a>, String> {
    if options
        .cover_kinds
        .iter()
        .any(|k| !["tree", "multiparent"].contains(&k.as_str()))
    {
        return Err("unknown fusion cover kind".into());
    }
    if options
        .join_kinds
        .iter()
        .any(|k| !["ortho", "ortho_peri", "higher_order"].contains(&k.as_str()))
    {
        return Err("unknown fusion join kind".into());
    }
    // FR-3.4's note and FR-3.5 defer FR-2.2.8 benzo components until after
    // selection; retained components such as quinoline and indene stay eligible.
    let mut initial_matches = Vec::new();
    for matched in matches {
        let spec = registry.spec_for_match(matched)?;
        if !spec.benzoheterocycle_component || component_has_peri_fusion(matched, &spec, matches) {
            initial_matches.push(matched.clone());
        }
    }
    let occurrence_options = occurrence_options(&initial_matches, registry)?;
    let faces: BTreeSet<_> = matches
        .iter()
        .flat_map(|m| m.covered_face_ids.iter().copied())
        .collect();
    if faces.len() < 2 {
        return Err("a fusion citation requires at least two component faces".into());
    }
    if faces.len() > fusion_nomenclature_config().search.maximum_faces {
        return Err(format!(
            "face count {} exceeds bounded limit {}",
            faces.len(),
            fusion_nomenclature_config().search.maximum_faces
        ));
    }
    let selections = exact_component_covers(&occurrence_options, &faces)?;
    let mut parent_scopes: BTreeMap<ComparisonKey, BTreeSet<BTreeSet<usize>>> = BTreeMap::new();
    for option in &occurrence_options {
        let matched = &option.mappings[0];
        let spec = registry.spec_for_match(matched)?;
        if component_parent_eligible(matched, &spec, &initial_matches) {
            parent_scopes
                .entry(component_variant_identity(&spec))
                .or_default()
                .insert(option.atom_ids.clone());
        }
    }
    parent_scopes.retain(|_, scopes| scopes.len() > 1);
    let mut prepared = Vec::new();
    let mut tiers: BTreeMap<Preference, Vec<usize>> = BTreeMap::new();
    for selection in selections {
        let selected: Vec<_> = selection
            .into_iter()
            .map(|i| occurrence_options[i].clone())
            .collect();
        let Some(selection) =
            prepare_component_selection(mol, selected, registry, options, &parent_scopes)?
        else {
            continue;
        };
        for preference in selection_preference_tiers(&selection, options) {
            tiers.entry(preference).or_default().push(prepared.len());
        }
        prepared.push(selection);
    }
    Ok(FusionNameAstIter {
        mol,
        registry,
        options: options.clone(),
        occurrences: occurrence_options,
        prepared,
        tiers: tiers.into_iter().collect(),
        pending: VecDeque::new(),
        emitted: Vec::new(),
        budget_exhausted: 0,
        terminal: false,
    })
}
impl Iterator for FusionNameAstIter<'_> {
    type Item = Result<FusionNameAst, String>;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(ast) = self.pending.pop_front() {
                if self.emitted.contains(&ast) {
                    continue;
                }
                self.emitted.push(ast.clone());
                return Some(Ok(ast));
            }
            let Some(((seniority, _), indices)) = self.tiers.pop_front() else {
                if self.terminal || !self.emitted.is_empty() {
                    return None;
                }
                self.terminal = true;
                return Some(Err(if self.budget_exhausted > 0 {
                    format!(
                        "all viable component covers exceeded the locant-map budget of {} states",
                        fusion_nomenclature_config().search.locant_map_combinations
                    )
                } else {
                    format!(
                        "no exact {} fusion citation was found",
                        if self.options.cover_kinds == ["tree".to_string()].into_iter().collect() {
                            "tree-cover"
                        } else {
                            "supported-cover"
                        }
                    )
                }));
            };
            let mut candidates = Vec::new();
            for index in indices {
                match candidates_for_selection(
                    self.mol,
                    &self.prepared[index],
                    &self.options,
                    &seniority,
                ) {
                    Ok(mut result) => candidates.append(&mut result),
                    Err(error) if error.starts_with("locant-map budget:") => {
                        self.budget_exhausted += 1
                    }
                    Err(error) => {
                        self.terminal = true;
                        self.tiers.clear();
                        return Some(Err(error));
                    }
                }
            }
            candidates.sort_by(|left, right| left.score.cmp(&right.score));
            let mut index = 0;
            while index < candidates.len() {
                let mut end = index + 1;
                while end < candidates.len() && candidates[end].score == candidates[index].score {
                    end += 1;
                }
                let mut named = Vec::new();
                for candidate in &candidates[index..end] {
                    let regrouped = if self.mol.use_benzoheterocycle_names() {
                        match benzoheterocycle_candidates(
                            self.mol,
                            candidate,
                            &self.occurrences,
                            self.registry,
                            &self.options,
                        ) {
                            Ok(result) => result,
                            Err(error) => return Some(Err(error)),
                        }
                    } else {
                        vec![candidate.clone()]
                    };
                    for candidate in regrouped {
                        match render_fusion_name(&candidate.ast, self.registry) {
                            Ok(name) => named.push((candidate.score, name, candidate.ast)),
                            Err(error) => return Some(Err(error)),
                        }
                    }
                }
                named.sort_by(|left, right| (&left.0, &left.1).cmp(&(&right.0, &right.1)));
                self.pending
                    .extend(named.into_iter().map(|(_, _, ast)| ast));
                index = end;
            }
        }
    }
}

fn benzoheterocycle_candidates(
    mol: &Molecule,
    original: &Candidate,
    occurrences: &[OccurrenceOption],
    registry: &FusionComponentRegistry,
    policy: &DescriptorOptions,
) -> Result<Vec<Candidate>, String> {
    let ast = &original.ast;
    let selected: Vec<_> = ast
        .component_occurrences
        .iter()
        .map(|m| {
            occurrences
                .iter()
                .find(|o| {
                    o.face_ids == m.covered_face_ids
                        && o.mappings[0].spec_key == m.spec_key
                        && o.mappings[0].template_name == m.template_name
                })
                .cloned()
                .ok_or_else(|| "selected fusion component is absent from its cover".to_owned())
        })
        .collect::<Result<_, _>>()?;
    let faces_for = |ids: &[usize]| -> BTreeSet<BTreeSet<usize>> {
        ids.iter()
            .map(|id| ast.component_occurrences[*id].covered_face_ids.clone())
            .collect()
    };
    let parents = (ast.parent_occurrences.len() > 1).then(|| faces_for(&ast.parent_occurrences));
    let groups: Vec<_> = ast
        .multiplicative_groups
        .iter()
        .map(|g| faces_for(&g.occurrence_ids))
        .collect();
    let protected: BTreeSet<_> = parents
        .iter()
        .chain(&groups)
        .flat_map(|group| group.iter().cloned())
        .collect();
    let mut pending = vec![selected];
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    while let Some(selected) = pending.pop() {
        mol.spend_naming_work(1)?;
        let mut identity: Vec<_> = selected
            .iter()
            .map(|o| (o.face_ids.clone(), o.mappings[0].spec_key.clone()))
            .collect();
        identity.sort();
        if !seen.insert(identity) {
            continue;
        }
        let specs: Vec<_> = selected
            .iter()
            .map(|o| registry.spec_for_match(&o.mappings[0]))
            .collect::<Result<_, _>>()?;
        let mut merged = false;
        // FR-3.5 / P-25.3.5 acts on the components selected by FR-3.1–3.4.
        // Only an isolated benzo and an ortho-fused heteromonocycle may change.
        for (b, benzo) in selected.iter().enumerate() {
            if specs[b].key != "benzene" || protected.contains(&benzo.face_ids) {
                continue;
            }
            for (h, hetero) in selected.iter().enumerate() {
                if specs[h].rings.len() != 1
                    || specs[h].atoms().iter().all(|a| a.symbol == "C")
                    || protected.contains(&hetero.face_ids)
                {
                    continue;
                }
                let shared: Vec<_> = benzo
                    .atom_ids
                    .intersection(&hetero.atom_ids)
                    .copied()
                    .collect();
                if shared.len() != 2 || mol.get_bond(shared[0], shared[1]).is_none() {
                    continue;
                }
                let mut matches = Vec::new();
                for benzo_map in &benzo.mappings {
                    for hetero_map in &hetero.mappings {
                        matches.extend(registry.benzoheterocycle_matches(benzo_map, hetero_map)?);
                    }
                }
                if matches.is_empty() {
                    continue;
                }
                for option in occurrence_options(&matches, registry)? {
                    let mut next: Vec<_> = selected
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| *i != b && *i != h)
                        .map(|(_, o)| o.clone())
                        .collect();
                    next.push(option);
                    pending.push(next);
                }
                merged = true;
            }
        }
        if merged || selected.len() == ast.component_occurrences.len() {
            continue;
        }
        let mut benzo_seniority: Vec<_> = selected
            .iter()
            .zip(&specs)
            .filter(|(o, _)| {
                !ast.component_occurrences
                    .iter()
                    .any(|m| m.covered_face_ids == o.face_ids)
            })
            .map(|(_, spec)| component_spec_seniority_key(spec).as_tuple())
            .collect();
        benzo_seniority.sort();
        let Some(prepared) =
            prepare_component_selection(mol, selected, registry, policy, &BTreeMap::new())?
        else {
            continue;
        };
        for (seniority, _) in selection_preference_tiers(&prepared, policy) {
            for mut candidate in candidates_for_selection(mol, &prepared, policy, &seniority)? {
                let faces_for = |ids: &[usize]| -> BTreeSet<BTreeSet<usize>> {
                    ids.iter()
                        .map(|id| {
                            candidate.ast.component_occurrences[*id]
                                .covered_face_ids
                                .clone()
                        })
                        .collect()
                };
                // Rerooting must also preserve any unaffected FR-6 parents or FR-4.9 groups.
                if parents
                    .as_ref()
                    .is_some_and(|p| *p != faces_for(&candidate.ast.parent_occurrences))
                    || groups.iter().any(|group| {
                        !candidate
                            .ast
                            .multiplicative_groups
                            .iter()
                            .any(|g| *group == faces_for(&g.occurrence_ids))
                    })
                {
                    continue;
                }
                candidate.ast.benzoheterocycle_merge = true;
                candidate.score.benzoheterocycle_seniority = benzo_seniority.clone();
                result.push(candidate);
            }
        }
    }
    if result.is_empty() {
        result.push(original.clone());
    }
    Ok(result)
}
fn occurrence_option_key(option: &OccurrenceOption) -> (usize, i64, Vec<usize>, &str, Vec<usize>) {
    (
        *option.face_ids.first().unwrap_or(&0),
        -(option.face_ids.len() as i64),
        option.face_ids.iter().copied().collect(),
        option.mappings[0].template_name.as_str(),
        option.atom_ids.iter().copied().collect(),
    )
}
fn local_map_key(mapping: &[(String, usize)]) -> Vec<(LocantKey, usize)> {
    mapping
        .iter()
        .map(|(locant, atom)| (retained_locant_sort_key(locant), *atom))
        .collect()
}
fn occurrence_options(
    matches: &[FusionComponentMatch],
    registry: &FusionComponentRegistry,
) -> Result<Vec<OccurrenceOption>, String> {
    let mut group_indices: BTreeMap<OccurrenceIdentity, usize> = BTreeMap::new();
    let mut grouped: Vec<OccurrenceMappingGroup<'_>> = Vec::new();
    for matched in matches {
        if matched.covered_face_ids.is_empty() {
            continue;
        }
        let atoms = matched
            .local_to_input_atom
            .iter()
            .map(|(_, atom)| *atom)
            .collect();
        let key = (
            matched.covered_face_ids.clone(),
            matched.spec_key.clone(),
            matched.template_name.clone(),
            atoms,
        );
        let index = if let Some(&index) = group_indices.get(&key) {
            index
        } else {
            let index = grouped.len();
            let group = OccurrenceMappingGroup {
                face_ids: key.0.clone(),
                atom_ids: key.3.clone(),
                mappings: Vec::new(),
            };
            group_indices.insert(key, index);
            grouped.push(group);
            index
        };
        grouped[index].mappings.push(matched);
    }
    if grouped.is_empty() {
        return Err("no exact component matches were supplied".into());
    }
    let mut options = Vec::new();
    for OccurrenceMappingGroup {
        face_ids,
        atom_ids,
        mappings,
    } in grouped
    {
        registry.spec_for_match(mappings[0])?;
        let unique: BTreeMap<_, _> = mappings
            .into_iter()
            .map(|m| (m.local_to_input_atom.as_slice(), m))
            .collect();
        let mut mappings: Vec<_> = unique.into_values().cloned().collect();
        mappings.sort_by_key(|m| local_map_key(&m.local_to_input_atom));
        options.push(OccurrenceOption {
            face_ids,
            atom_ids,
            mappings,
        });
    }
    options.sort_by(|left, right| occurrence_option_key(left).cmp(&occurrence_option_key(right)));
    Ok(options)
}
fn exact_component_covers(
    options: &[OccurrenceOption],
    target: &BTreeSet<usize>,
) -> Result<Vec<Vec<usize>>, String> {
    let by_face: BTreeMap<usize, Vec<usize>> = target
        .iter()
        .map(|face| {
            (
                *face,
                options
                    .iter()
                    .enumerate()
                    .filter(|(_, o)| o.face_ids.contains(face))
                    .map(|(i, _)| i)
                    .collect(),
            )
        })
        .collect();
    if by_face.values().any(Vec::is_empty) {
        return Err("component matches do not cover every fusion face".into());
    }
    fn visit(
        options: &[OccurrenceOption],
        target: &BTreeSet<usize>,
        by_face: &BTreeMap<usize, Vec<usize>>,
        covered: BTreeSet<usize>,
        selected: &mut Vec<usize>,
        states: &mut usize,
        covers: &mut Vec<Vec<usize>>,
    ) -> Result<(), String> {
        let limits = &fusion_nomenclature_config().search;
        *states += 1;
        if *states > limits.component_selection_states {
            return Err(format!(
                "component cover search exceeds bounded limit {}",
                limits.component_selection_states
            ));
        }
        if &covered == target {
            covers.push(selected.clone());
            if covers.len() > limits.maximum_component_selections {
                return Err(format!(
                    "component selection count exceeds bounded limit {}",
                    limits.maximum_component_selections
                ));
            }
            return Ok(());
        }
        if selected.len() >= limits.maximum_component_occurrences {
            return Ok(());
        }
        // The unequal sets above and disjoint additions guarantee an uncovered face.
        let mut uncovered: Vec<_> = target.difference(&covered).copied().collect();
        uncovered.sort_by_key(|face| (by_face[face].len(), *face));
        let pivot = uncovered[0];
        for &index in &by_face[&pivot] {
            let option = &options[index];
            if !option.face_ids.is_disjoint(&covered) || !option.face_ids.is_subset(target) {
                continue;
            }
            selected.push(index);
            visit(
                options,
                target,
                by_face,
                covered.union(&option.face_ids).copied().collect(),
                selected,
                states,
                covers,
            )?;
            selected.pop();
        }
        Ok(())
    }
    let mut covers = Vec::new();
    visit(
        options,
        target,
        &by_face,
        BTreeSet::new(),
        &mut Vec::new(),
        &mut 0,
        &mut covers,
    )?;
    if covers.is_empty() {
        return Err("no bounded exact component cover was found".into());
    }
    Ok(covers)
}
fn scope_for_match(
    mol: &Molecule,
    matched: &FusionComponentMatch,
    spec: &FusionComponentSpec,
) -> Result<ComponentScope<usize>, String> {
    let atoms = matched.input_atom_by_locant();
    let mut edges = Vec::new();
    for bond in spec.bonds() {
        let edge = normalize_edge(atoms[&bond.locants.0], atoms[&bond.locants.1]);
        if mol.get_bond(edge.0, edge.1).is_none() {
            return Err(format!(
                "component {:?} maps local bond {:?} to a missing molecular edge",
                spec.key, bond.locants
            ));
        }
        edges.push(edge);
    }
    component_scope(matched.occurrence_id, atoms.into_values(), edges)
}
fn prepare_component_selection(
    mol: &Molecule,
    mut options: Vec<OccurrenceOption>,
    registry: &FusionComponentRegistry,
    policy: &DescriptorOptions,
    parent_scopes: &BTreeMap<ComparisonKey, BTreeSet<BTreeSet<usize>>>,
) -> Result<Option<PreparedSelection>, String> {
    if options.len()
        > fusion_nomenclature_config()
            .search
            .maximum_component_occurrences
    {
        return Ok(None);
    }
    options.sort_by(|left, right| occurrence_option_key(left).cmp(&occurrence_option_key(right)));
    let matches: Vec<_> = options
        .iter()
        .enumerate()
        .map(|(occurrence_id, o)| FusionComponentMatch {
            occurrence_id,
            ..o.mappings[0].clone()
        })
        .collect();
    let specs: BTreeMap<_, _> = matches
        .iter()
        .map(|m| registry.spec_for_match(m).map(|s| (m.occurrence_id, s)))
        .collect::<Result<_, _>>()?;
    let scopes: Vec<_> = matches
        .iter()
        .map(|m| scope_for_match(mol, m, &specs[&m.occurrence_id]))
        .collect::<Result<_, _>>()?;
    let target_atoms = scopes
        .iter()
        .flat_map(|s| s.atom_ids.iter().copied())
        .collect();
    let target_edges = scopes
        .iter()
        .flat_map(|s| s.edges.iter().copied())
        .collect();
    let audit = audit_component_cover(&scopes, &target_atoms, &target_edges)?;
    if !audit.ok
        || !policy.cover_kinds.contains(&audit.proof.kind)
        || audit.graph.interfaces.iter().any(|i| {
            !policy.join_kinds.contains(if i.shared_edges.len() == 1 {
                "ortho"
            } else {
                "ortho_peri"
            })
        })
        || (audit.proof.kind == "multiparent" && !policy.join_kinds.contains("higher_order"))
    {
        return Ok(None);
    }
    let mut selected: BTreeMap<ComparisonKey, BTreeSet<BTreeSet<usize>>> = BTreeMap::new();
    for (m, scope) in matches.iter().zip(&scopes) {
        selected
            .entry(component_variant_identity(&specs[&m.occurrence_id]))
            .or_default()
            .insert(scope.atom_ids.clone());
    }
    let hidden_parent_variants = parent_scopes
        .iter()
        .filter(|(variant, atoms)| {
            atoms
                .iter()
                .any(|a| !selected.get(*variant).is_some_and(|s| s.contains(a)))
        })
        .map(|(v, _)| v.clone())
        .collect();
    Ok(Some(PreparedSelection {
        options,
        matches,
        specs,
        graph: audit.graph,
        hidden_parent_variants,
    }))
}
fn cover_node_key(
    node: usize,
    adjacency: &BTreeMap<usize, Vec<usize>>,
    specs: &BTreeMap<usize, FusionComponentSpec>,
) -> (ComparisonKey, i64, ComparisonKey, usize) {
    (
        component_spec_seniority_key(&specs[&node]).as_tuple(),
        -(adjacency[&node].len() as i64),
        component_canonicalization_key(&specs[&node]),
        node,
    )
}
fn parent_location_key(
    orders: &BTreeMap<usize, usize>,
    roots: &[usize],
    interparents: &[usize],
    specs: &BTreeMap<usize, FusionComponentSpec>,
) -> ParentLocationKey {
    let maximum_attachment_order = orders.values().copied().max().unwrap_or(0);
    let attachment_count_by_order = (1..=maximum_attachment_order)
        .map(|level| -(orders.values().filter(|&&o| o == level).count() as i64))
        .collect();
    let variants: BTreeSet<_> = roots
        .iter()
        .map(|r| component_variant_identity(&specs[r]))
        .collect();
    let incomplete_system = usize::from(
        roots.len() == 1
            && specs.iter().any(|(i, s)| {
                !roots.contains(i) && variants.contains(&component_variant_identity(s))
            }),
    );
    let mut interparent: Vec<_> = interparents
        .iter()
        .map(|i| component_spec_seniority_key(&specs[i]).as_tuple())
        .collect();
    interparent.sort();
    let mut attached: Vec<_> = specs
        .iter()
        .filter(|(i, _)| !roots.contains(i) && !interparents.contains(i))
        .map(|(_, s)| component_spec_seniority_key(s).as_tuple())
        .collect();
    attached.sort();
    ParentLocationKey {
        incomplete_system,
        maximum_attachment_order,
        attachment_count_by_order,
        multiplicative_grouping_score: Vec::new(),
        interparent_seniority: ComparisonKey::tuple(interparent),
        attached_component_preference: ComparisonKey::tuple(attached),
    }
}
fn pre_mapping_location_key(key: &ParentLocationKey) -> (usize, usize, Vec<i64>) {
    (
        key.incomplete_system,
        key.maximum_attachment_order,
        key.attachment_count_by_order.clone(),
    )
}
fn citation_topology(
    adjacency: &BTreeMap<usize, Vec<usize>>,
    roots: Vec<usize>,
    specs: &BTreeMap<usize, FusionComponentSpec>,
    hidden: &BTreeSet<ComparisonKey>,
) -> CitationTopology {
    let mut parent_by_child = BTreeMap::new();
    let mut order_by_occurrence: BTreeMap<_, _> = roots.iter().map(|&r| (r, 0)).collect();
    let mut owner: BTreeMap<_, _> = roots.iter().map(|&r| (r, r)).collect();
    let mut ordered_roots = roots.clone();
    ordered_roots.sort_by_key(|&r| cover_node_key(r, adjacency, specs));
    let mut pending: VecDeque<_> = ordered_roots.into_iter().collect();
    while let Some(host) = pending.pop_front() {
        let mut children = adjacency[&host].clone();
        children.sort_by_key(|&node| cover_node_key(node, adjacency, specs));
        for child in children {
            if order_by_occurrence.contains_key(&child) {
                continue;
            }
            parent_by_child.insert(child, host);
            order_by_occurrence.insert(child, order_by_occurrence[&host] + 1);
            owner.insert(child, owner[&host]);
            pending.push_back(child);
        }
    }
    let primary: BTreeSet<_> = parent_by_child
        .iter()
        .map(|(&c, &h)| normalize_edge(c, h))
        .collect();
    let all: BTreeSet<_> = adjacency
        .iter()
        .flat_map(|(&l, neighbors)| {
            neighbors
                .iter()
                .filter(move |&&r| r != l)
                .map(move |&r| normalize_edge(l, r))
        })
        .collect();
    let mut closing: Vec<_> = all.difference(&primary).copied().collect();
    closing.sort_by_key(|&(l, r)| {
        let mut keys = vec![
            cover_node_key(l, adjacency, specs),
            cover_node_key(r, adjacency, specs),
        ];
        keys.sort();
        keys
    });
    let interparent_pairs: Vec<_> = closing
        .iter()
        .filter(|(l, r)| owner.get(l) != owner.get(r))
        .copied()
        .collect();
    let cycle_closing_pairs = closing
        .into_iter()
        .filter(|pair| !interparent_pairs.contains(pair))
        .collect();
    let mut interparent_occurrences: Vec<_> = interparent_pairs
        .iter()
        .flat_map(|&(l, r)| [l, r])
        .filter(|n| !roots.contains(n))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    interparent_occurrences.sort_by_key(|&n| cover_node_key(n, adjacency, specs));
    let mut location_key = parent_location_key(
        &order_by_occurrence,
        &roots,
        &interparent_occurrences,
        specs,
    );
    if roots
        .iter()
        .any(|root| hidden.contains(&component_variant_identity(&specs[root])))
    {
        location_key.incomplete_system = 1;
    }
    CitationTopology {
        roots,
        parent_by_child,
        order_by_occurrence,
        interparent_pairs,
        cycle_closing_pairs,
        interparent_occurrences,
        location_key,
    }
}
fn multiparent_root_sets(
    roots: &[usize],
    graph: &CoverGraph<usize>,
    specs: &BTreeMap<usize, FusionComponentSpec>,
) -> Vec<Vec<usize>> {
    if roots.len() < 2
        || roots
            .iter()
            .map(|r| component_variant_identity(&specs[r]))
            .collect::<BTreeSet<_>>()
            .len()
            != 1
    {
        return Vec::new();
    }
    let mut roots = roots.to_vec();
    roots.sort_by_key(|&r| cover_node_key(r, &graph.adjacency, specs));
    if roots.iter().enumerate().any(|(i, l)| {
        roots[i + 1..]
            .iter()
            .any(|r| graph.adjacency[l].contains(r))
    }) {
        return Vec::new();
    }
    let topology = citation_topology(&graph.adjacency, roots.clone(), specs, &BTreeSet::new());
    if topology.interparent_occurrences.is_empty() {
        Vec::new()
    } else {
        vec![roots]
    }
}
fn root_sets(
    roots: &[usize],
    prepared: &PreparedSelection,
    policy: &DescriptorOptions,
) -> Vec<Vec<usize>> {
    let mut sets: Vec<_> = roots.iter().map(|r| vec![*r]).collect();
    if policy.multiparent_parents {
        let mut variants: BTreeMap<ComparisonKey, Vec<usize>> = BTreeMap::new();
        for &r in roots {
            variants
                .entry(component_variant_identity(&prepared.specs[&r]))
                .or_default()
                .push(r);
        }
        for roots in variants.values() {
            sets.extend(multiparent_root_sets(
                roots,
                &prepared.graph,
                &prepared.specs,
            ));
        }
    }
    sets
}
fn citation_grammar_supported(
    topology: &CitationTopology,
    specs: &BTreeMap<usize, FusionComponentSpec>,
    enforce: bool,
) -> bool {
    if !enforce {
        return true;
    }
    let support = &fusion_nomenclature_config().rules.support;
    let maximum = if topology.roots.len() > 1 {
        support.maximum_multiparent_component_occurrences
    } else {
        support.maximum_tree_component_occurrences
    };
    if specs.len() > maximum {
        return false;
    }
    if topology.roots.len() == 1 {
        let mut children: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (&c, &h) in &topology.parent_by_child {
            children.entry(h).or_default().push(c);
        }
        for (&h, siblings) in &children {
            if h == topology.roots[0] {
                continue;
            }
            let branches: Vec<_> = siblings
                .iter()
                .filter(|c| children.contains_key(c))
                .collect();
            if branches.len() > 1 {
                return false;
            }
            if let Some(&branch) = branches.first() {
                let key = component_spec_seniority_key(&specs[branch]).as_tuple();
                if siblings
                    .iter()
                    .any(|c| component_spec_seniority_key(&specs[c]).as_tuple() < key)
                {
                    return false;
                }
            }
        }
    }
    true
}
fn eligible_roots(prepared: &PreparedSelection) -> BTreeMap<ComparisonKey, Vec<usize>> {
    let mut roots: BTreeMap<ComparisonKey, Vec<usize>> = BTreeMap::new();
    for matched in &prepared.matches {
        let spec = &prepared.specs[&matched.occurrence_id];
        if component_parent_eligible(matched, spec, &prepared.matches) {
            roots
                .entry(component_spec_seniority_key(spec).as_tuple())
                .or_default()
                .push(matched.occurrence_id);
        }
    }
    roots
}
fn selection_preference_tiers(
    prepared: &PreparedSelection,
    policy: &DescriptorOptions,
) -> Vec<Preference> {
    let mut result = Vec::new();
    for (seniority, roots) in eligible_roots(prepared) {
        let supported: Vec<_> = root_sets(&roots, prepared, policy)
            .into_iter()
            .map(|r| {
                citation_topology(
                    &prepared.graph.adjacency,
                    r,
                    &prepared.specs,
                    &prepared.hidden_parent_variants,
                )
            })
            .filter(|t| {
                citation_grammar_supported(
                    t,
                    &prepared.specs,
                    policy.enforce_interoperability_limits,
                )
            })
            .collect();
        if let Some(location) = supported
            .iter()
            .map(|t| pre_mapping_location_key(&t.location_key))
            .min()
        {
            result.push((seniority, location));
        }
    }
    result.sort();
    result
}
fn candidates_for_selection(
    mol: &Molecule,
    prepared: &PreparedSelection,
    policy: &DescriptorOptions,
    required_seniority: &ComparisonKey,
) -> Result<Vec<Candidate>, String> {
    let mut mapping_sets = Vec::new();
    for (occurrence_id, option) in prepared.options.iter().enumerate() {
        mapping_sets.push(
            option
                .mappings
                .iter()
                .cloned()
                .map(|m| FusionComponentMatch { occurrence_id, ..m })
                .collect::<Vec<_>>(),
        );
    }
    for (seniority, roots) in eligible_roots(prepared) {
        if &seniority != required_seniority {
            continue;
        }
        let topologies: Vec<_> = root_sets(&roots, prepared, policy)
            .into_iter()
            .map(|r| {
                citation_topology(
                    &prepared.graph.adjacency,
                    r,
                    &prepared.specs,
                    &prepared.hidden_parent_variants,
                )
            })
            .filter(|t| {
                citation_grammar_supported(
                    t,
                    &prepared.specs,
                    policy.enforce_interoperability_limits,
                )
            })
            .collect();
        let Some(preferred_location) = topologies
            .iter()
            .map(|t| pre_mapping_location_key(&t.location_key))
            .min()
        else {
            continue;
        };
        let interfaces: BTreeMap<_, _> = prepared
            .graph
            .interfaces
            .iter()
            .cloned()
            .map(|i| (normalize_edge(i.left, i.right), i))
            .collect();
        let mut candidates = Vec::new();
        for topology in topologies
            .into_iter()
            .filter(|t| pre_mapping_location_key(&t.location_key) == preferred_location)
        {
            if topology
                .parent_by_child
                .keys()
                .any(|c| !prepared.specs[c].usable_as_attached)
            {
                continue;
            }
            if topology.roots.len() == 1
                && topology.interparent_pairs.is_empty()
                && topology.cycle_closing_pairs.is_empty()
            {
                if let Some(candidate) = best_tree_mapping_candidate(
                    &mapping_sets,
                    &prepared.specs,
                    &topology,
                    &interfaces,
                    mol,
                    policy,
                )? {
                    candidates.push(candidate);
                }
            } else {
                candidates.extend(compatible_mapping_candidates(
                    &mapping_sets,
                    &prepared.specs,
                    &topology,
                    &interfaces,
                    mol,
                    policy,
                )?);
            }
        }
        if !candidates.is_empty() {
            return Ok(candidates);
        }
    }
    Ok(Vec::new())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostSideProjection {
    host_map: BTreeMap<String, usize>,
    sides: Vec<ComponentSide>,
    side_edges: Vec<Edge>,
}
pub fn host_side_projection(
    host: &FusionComponentMatch,
    spec: &FusionComponentSpec,
) -> Result<HostSideProjection, String> {
    let atoms = host.input_atom_by_locant();
    let sides = component_sides(spec)?;
    let side_edges = sides
        .iter()
        .map(|side| normalize_edge(atoms[&side.start_locant], atoms[&side.end_locant]))
        .collect();
    Ok(HostSideProjection {
        host_map: atoms,
        sides,
        side_edges,
    })
}
pub fn classify_ordered_fusion_interface(
    attached: &FusionComponentMatch,
    host: &FusionComponentMatch,
    attached_spec: &FusionComponentSpec,
    host_spec: &FusionComponentSpec,
    interface: &FusionInterface<usize>,
    mol: &Molecule,
) -> Option<(OrderedFusionInterface, usize)> {
    let projection = host_side_projection(host, host_spec).ok()?;
    classify_with_projection(attached, host, attached_spec, interface, mol, &projection)
}
fn classify_with_projection(
    attached: &FusionComponentMatch,
    host: &FusionComponentMatch,
    attached_spec: &FusionComponentSpec,
    interface: &FusionInterface<usize>,
    mol: &Molecule,
    projection: &HostSideProjection,
) -> Option<(OrderedFusionInterface, usize)> {
    if interface.shared_edges.is_empty() {
        return None;
    }
    let HostSideProjection {
        host_map,
        sides,
        side_edges,
    } = projection;
    let indices: BTreeSet<_> = side_edges
        .iter()
        .enumerate()
        .filter(|(_, e)| interface.shared_edges.contains(e))
        .map(|(i, _)| i)
        .collect();
    if indices.len() != interface.shared_edges.len() || indices.len() == sides.len() {
        return None;
    }
    let starts: Vec<_> = indices
        .iter()
        .copied()
        .filter(|i| !indices.contains(&((i + sides.len() - 1) % sides.len())))
        .collect();
    if starts.len() != 1 {
        return None;
    }
    let start = starts[0];
    let ordered: Vec<_> = (0..indices.len())
        .map(|offset| (start + offset) % sides.len())
        .collect();
    if ordered.iter().copied().collect::<BTreeSet<_>>() != indices {
        return None;
    }
    let mut host_text = vec![sides[ordered[0]].start_locant.clone()];
    host_text.extend(ordered.iter().map(|&i| sides[i].end_locant.clone()));
    let ordered_input_atoms: Vec<_> = host_text.iter().map(|l| host_map[l]).collect();
    let ordered_input_edges: Vec<_> = ordered_input_atoms
        .windows(2)
        .map(|p| normalize_edge(p[0], p[1]))
        .collect();
    if ordered_input_edges.iter().copied().collect::<BTreeSet<_>>() != interface.shared_edges
        || ordered_input_atoms.iter().copied().collect::<BTreeSet<_>>() != interface.shared_atom_ids
    {
        return None;
    }
    let inverse: BTreeMap<_, _> = attached
        .local_to_input_atom
        .iter()
        .map(|(l, a)| (*a, l.clone()))
        .collect();
    let attached_text: Vec<_> = ordered_input_atoms
        .iter()
        .map(|a| inverse.get(a).cloned())
        .collect::<Option<_>>()?;
    let bonds = component_bond_locant_sets(attached_spec);
    if attached_text
        .windows(2)
        .any(|p| !bonds.contains(&p.iter().cloned().collect()))
    {
        return None;
    }
    let ordered_input_bonds = ordered_input_edges
        .iter()
        .map(|&(u, v)| mol.get_bond(u, v).map(|b| b.idx))
        .collect::<Option<Vec<_>>>()?;
    let kind = if ordered_input_edges.len() == 1 {
        FusionJoinKind::Ortho
    } else {
        FusionJoinKind::OrthoPeri
    };
    let attached_path: Vec<_> = attached_text
        .into_iter()
        .map(|text| ComponentLocant {
            component_id: attached.occurrence_id,
            text,
            prime_depth: 0,
        })
        .collect();
    Some((
        OrderedFusionInterface {
            kind,
            attached_occurrence: attached.occurrence_id,
            host_occurrence: host.occurrence_id,
            cited_attached_locants: attached_path.clone(),
            attached_path,
            host_path: host_text
                .into_iter()
                .map(|text| ComponentLocant {
                    component_id: host.occurrence_id,
                    text,
                    prime_depth: 0,
                })
                .collect(),
            host_sides: ordered
                .into_iter()
                .map(|i| FusionSide {
                    component_id: host.occurrence_id,
                    letter: sides[i].letter.clone(),
                    prime_depth: 0,
                })
                .collect(),
            host_locants: Vec::new(),
            ordered_input_atoms,
            ordered_input_edges,
            ordered_input_bonds,
        },
        start,
    ))
}
fn classified_join(
    attached: &FusionComponentMatch,
    host: &FusionComponentMatch,
    specs: &BTreeMap<usize, FusionComponentSpec>,
    interface: &FusionInterface<usize>,
    order: usize,
    mol: &Molecule,
) -> Option<(FusionJoin, usize)> {
    classify_ordered_fusion_interface(
        attached,
        host,
        &specs[&attached.occurrence_id],
        &specs[&host.occurrence_id],
        interface,
        mol,
    )
    .map(|(interface, rank)| (FusionJoin { order, interface }, rank))
}
fn deduplicate_interface_equivalent_maps(
    mapping_sets: &[Vec<FusionComponentMatch>],
    interfaces: &BTreeMap<Edge, FusionInterface<usize>>,
) -> Vec<Vec<FusionComponentMatch>> {
    let mut atoms: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    for interface in interfaces.values() {
        atoms
            .entry(interface.left)
            .or_default()
            .extend(&interface.shared_atom_ids);
        atoms
            .entry(interface.right)
            .or_default()
            .extend(&interface.shared_atom_ids);
    }
    mapping_sets
        .iter()
        .enumerate()
        .map(|(occurrence, mappings)| {
            let relevant = atoms.get(&occurrence);
            let mut projected: BTreeMap<Vec<(usize, &str)>, FusionComponentMatch> = BTreeMap::new();
            for mapping in mappings {
                let inverse: BTreeMap<_, _> = mapping
                    .local_to_input_atom
                    .iter()
                    .map(|(l, a)| (*a, l.as_str()))
                    .collect();
                let projection = relevant
                    .into_iter()
                    .flatten()
                    .map(|a| (*a, inverse[a]))
                    .collect();
                projected
                    .entry(projection)
                    .or_insert_with(|| mapping.clone());
            }
            projected.into_values().collect()
        })
        .collect()
}
fn attached_locant_key(join: &FusionJoin) -> Vec<LocantKey> {
    join.attached_locants()
        .iter()
        .map(|l| retained_locant_sort_key(&l.text))
        .collect()
}
fn join_preference_key(join: &FusionJoin, side_rank: usize) -> JoinKey {
    (join.order, side_rank, attached_locant_key(join))
}
/// FR-4.5(a-d) / P-25.3.4.2.4(a-d) compare each complete locant tier
/// before considering the next one. Sets retain repeated locants, and numeric
/// locants retain multiplicative primes. FR-4.6 omission happens in rendering.
fn first_order_locant_key(
    citation_order: &[usize],
    joins: &BTreeMap<usize, FusionJoin>,
    depths: &BTreeMap<usize, usize>,
) -> FirstOrderLocantKey {
    let first_order: Vec<_> = citation_order
        .iter()
        .filter_map(|id| joins.get(id))
        .filter(|join| join.order == 1)
        .collect();
    let parent_letters_citation: Vec<_> = first_order
        .iter()
        .flat_map(|join| join.host_sides())
        .map(|side| {
            (
                side.letter.len(),
                side.letter.clone(),
                depths
                    .get(&side.component_id)
                    .copied()
                    .unwrap_or(side.prime_depth),
            )
        })
        .collect();
    let attached_locants_citation: Vec<_> = first_order
        .iter()
        .flat_map(|join| join.attached_locants())
        .map(|locant| {
            (
                retained_locant_sort_key(&locant.text),
                depths
                    .get(&locant.component_id)
                    .copied()
                    .unwrap_or(locant.prime_depth),
            )
        })
        .collect();
    let mut parent_letters_set = parent_letters_citation.clone();
    parent_letters_set.sort();
    let mut attached_locants_set = attached_locants_citation.clone();
    attached_locants_set.sort();
    FirstOrderLocantKey {
        parent_letters_set,
        parent_letters_citation,
        attached_locants_set,
        attached_locants_citation,
    }
}
fn higher_order_locant_key(
    citation_order: &[usize],
    joins: &BTreeMap<usize, FusionJoin>,
    depths: &BTreeMap<usize, usize>,
) -> Vec<HigherOrderLocantKey> {
    let mut orders: BTreeMap<usize, HigherOrderLocantKey> = BTreeMap::new();
    let locant_key = |locant: &ComponentLocant| {
        (
            retained_locant_sort_key(&locant.text),
            depths
                .get(&locant.component_id)
                .copied()
                .unwrap_or(locant.prime_depth),
        )
    };
    // FR-4.5(e-h) is applied outwards, before moving to the next order.
    for join in citation_order.iter().filter_map(|id| joins.get(id)) {
        if join.order > 1 {
            let key = orders.entry(join.order).or_default();
            key.host_citation
                .extend(join.interface.host_path.iter().map(locant_key));
            key.attached_citation
                .extend(join.attached_locants().iter().map(locant_key));
        }
    }
    orders
        .into_values()
        .map(|mut key| {
            key.host_set = key.host_citation.clone();
            key.host_set.sort();
            key.attached_set = key.attached_citation.clone();
            key.attached_set.sort();
            key
        })
        .collect()
}
fn ordered_children(
    parents: &BTreeMap<usize, usize>,
    specs: &BTreeMap<usize, FusionComponentSpec>,
    joins: &BTreeMap<usize, FusionJoin>,
    ranks: &BTreeMap<usize, usize>,
) -> BTreeMap<usize, Vec<usize>> {
    let mut children: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (&c, &p) in parents {
        children.entry(p).or_default().push(c);
    }
    for values in children.values_mut() {
        values.sort_by_key(|c| {
            (
                component_spec_seniority_key(&specs[c]).as_tuple(),
                ranks[c],
                attached_locant_key(&joins[c]),
                component_canonicalization_key(&specs[c]),
            )
        });
    }
    children
}
#[derive(Clone)]
enum TreeWork {
    VisitNode(usize),
    SelectChildren(usize, usize),
    TryChild(usize, usize, usize),
    EmitJoin(usize),
    Complete,
}
#[derive(Clone)]
struct TreeState {
    maps: BTreeMap<usize, FusionComponentMatch>,
    map_indices: BTreeMap<usize, usize>,
    joins: BTreeMap<usize, FusionJoin>,
    ranks: BTreeMap<usize, usize>,
    first_order_locants: Option<FirstOrderLocantKey>,
    prefix: Vec<JoinKey>,
    todo: Vec<TreeWork>,
}
fn best_tree_mapping_candidate(
    mapping_sets: &[Vec<FusionComponentMatch>],
    specs: &BTreeMap<usize, FusionComponentSpec>,
    topology: &CitationTopology,
    interfaces: &BTreeMap<Edge, FusionInterface<usize>>,
    mol: &Molecule,
    policy: &DescriptorOptions,
) -> Result<Option<Candidate>, String> {
    // Choose every direct child's map before visiting its descendants. Join
    // prefixes may prune only in the final citation order: chains and distinct
    // leaf prefixes have a fixed order, whereas multipart branch keys and
    // tied-prefix multiplicative groups are finalized in build_candidate.
    let projected = deduplicate_interface_equivalent_maps(mapping_sets, interfaces);
    let root = topology.roots[0];
    let children: BTreeMap<usize, Vec<usize>> = (0..projected.len())
        .map(|p| {
            (
                p,
                topology
                    .parent_by_child
                    .iter()
                    .filter(|(_, host)| **host == p)
                    .map(|(&c, _)| c)
                    .collect(),
            )
        })
        .collect();
    let prune_citation_prefix = topology
        .order_by_occurrence
        .values()
        .all(|&order| order <= 1)
        && children.values().all(|ids| {
            ids.len() < 2
                || (ids.iter().all(|child| children[child].is_empty())
                    && ids
                        .iter()
                        .map(|child| fusion_prefix_citation_key(&specs[child].attached_prefix))
                        .collect::<BTreeSet<_>>()
                        .len()
                        == ids.len())
        });
    let mut best: Option<Candidate> = None;
    let mut states = 0;
    let mut proved: BTreeMap<(usize, usize, usize), Option<(FusionJoin, usize)>> = BTreeMap::new();
    let mut projections: BTreeMap<(usize, usize), Option<HostSideProjection>> = BTreeMap::new();
    for (root_index, root_mapping) in projected[root].iter().enumerate() {
        let mut pending = vec![TreeState {
            maps: [(root, root_mapping.clone())].into_iter().collect(),
            map_indices: [(root, root_index)].into_iter().collect(),
            joins: BTreeMap::new(),
            ranks: BTreeMap::new(),
            first_order_locants: None,
            prefix: Vec::new(),
            todo: vec![TreeWork::Complete, TreeWork::VisitNode(root)],
        }];
        while let Some(mut state) = pending.pop() {
            let Some(work) = state.todo.pop() else {
                continue;
            };
            match work {
                TreeWork::VisitNode(node) => {
                    state.todo.push(TreeWork::SelectChildren(node, 0));
                    pending.push(state);
                }
                TreeWork::SelectChildren(node, position) => {
                    let ids = &children[&node];
                    if position == ids.len() {
                        let mut ordered = ids.clone();
                        ordered.sort_by_key(|c| {
                            (
                                fusion_prefix_citation_key(&specs[c].attached_prefix),
                                component_spec_seniority_key(&specs[c]).as_tuple(),
                                state.ranks[c],
                                attached_locant_key(&state.joins[c]),
                                component_canonicalization_key(&specs[c]),
                            )
                        });
                        if node == root && prune_citation_prefix {
                            state.first_order_locants = Some(first_order_locant_key(
                                &ordered,
                                &state.joins,
                                &BTreeMap::new(),
                            ));
                        }
                        for child in ordered.into_iter().rev() {
                            state.todo.push(TreeWork::EmitJoin(child));
                            state.todo.push(TreeWork::VisitNode(child));
                        }
                        pending.push(state);
                    } else {
                        let child = ids[position];
                        for index in (0..projected[child].len()).rev() {
                            let mut branch = state.clone();
                            branch.todo.push(TreeWork::TryChild(node, position, index));
                            pending.push(branch);
                        }
                    }
                }
                TreeWork::TryChild(node, position, index) => {
                    states += 1;
                    if states > fusion_nomenclature_config().search.locant_map_combinations {
                        return Err(format!(
                            "locant-map budget: tree search exceeds {} states",
                            fusion_nomenclature_config().search.locant_map_combinations
                        ));
                    }
                    let child = children[&node][position];
                    let host_index = state.map_indices[&node];
                    let key = (child, index, host_index);
                    let classified = if let Some(value) = proved.get(&key) {
                        value.clone()
                    } else {
                        let projection =
                            projections.entry((node, host_index)).or_insert_with(|| {
                                host_side_projection(&state.maps[&node], &specs[&node]).ok()
                            });
                        let result = if let Some(projection) = projection {
                            classify_with_projection(
                                &projected[child][index],
                                &state.maps[&node],
                                &specs[&child],
                                &interfaces[&normalize_edge(child, node)],
                                mol,
                                projection,
                            )
                            .map(|(interface, rank)| {
                                (
                                    FusionJoin {
                                        order: topology.order_by_occurrence[&child],
                                        interface,
                                    },
                                    rank,
                                )
                            })
                        } else {
                            None
                        };
                        proved.insert(key, result.clone());
                        result
                    };
                    if let Some((join, rank)) = classified {
                        state.maps.insert(child, projected[child][index].clone());
                        state.map_indices.insert(child, index);
                        state.joins.insert(child, join);
                        state.ranks.insert(child, rank);
                        state
                            .todo
                            .push(TreeWork::SelectChildren(node, position + 1));
                        pending.push(state);
                    }
                }
                TreeWork::EmitJoin(child) => {
                    if prune_citation_prefix {
                        state.prefix.push(join_preference_key(
                            &state.joins[&child],
                            state.ranks[&child],
                        ));
                    }
                    if !prune_citation_prefix
                        || !best.as_ref().is_some_and(|b| {
                            state
                                .first_order_locants
                                .as_ref()
                                .is_some_and(|first_order| {
                                    first_order > &b.score.first_order_locants
                                        || (first_order == &b.score.first_order_locants
                                            && state.prefix.as_slice()
                                                > &b.score.joins[..state.prefix.len()])
                                })
                        })
                    {
                        pending.push(state);
                    }
                }
                TreeWork::Complete => {
                    let matches: Vec<_> = (0..projected.len())
                        .map(|i| state.maps[&i].clone())
                        .collect();
                    let classified = state
                        .joins
                        .iter()
                        .map(|(&c, j)| (c, (j.clone(), state.ranks[&c])))
                        .collect();
                    if let Some(candidate) = build_candidate(
                        &matches,
                        specs,
                        topology,
                        interfaces,
                        mol,
                        policy,
                        &classified,
                    ) && best.as_ref().is_none_or(|b| candidate.score < b.score)
                    {
                        best = Some(candidate);
                    }
                }
            }
        }
    }
    Ok(best)
}
fn compatible_mapping_candidates(
    mapping_sets: &[Vec<FusionComponentMatch>],
    specs: &BTreeMap<usize, FusionComponentSpec>,
    topology: &CitationTopology,
    interfaces: &BTreeMap<Edge, FusionInterface<usize>>,
    mol: &Molecule,
    policy: &DescriptorOptions,
) -> Result<Vec<Candidate>, String> {
    let projected = deduplicate_interface_equivalent_maps(mapping_sets, interfaces);
    let mut order: Vec<_> = (0..mapping_sets.len()).collect();
    order.sort_by_key(|i| (topology.order_by_occurrence[i], *i));
    let leading: BTreeSet<_> = order
        .iter()
        .filter(|i| topology.order_by_occurrence[i] == 0)
        .copied()
        .collect();
    if order.is_empty()
        || topology.roots.is_empty()
        || leading != topology.roots.iter().copied().collect()
    {
        return Err("component cover graph has inconsistent numbered roots".into());
    }
    let mut pending = vec![(
        0usize,
        BTreeMap::<usize, FusionComponentMatch>::new(),
        BTreeMap::<usize, (FusionJoin, usize)>::new(),
    )];
    let mut states = 0;
    let mut result = Vec::new();
    while let Some((position, selected, classified)) = pending.pop() {
        states += 1;
        if states > fusion_nomenclature_config().search.locant_map_combinations {
            return Err("locant-map budget: compatible search exhausted".into());
        }
        if position == order.len() {
            let matches = (0..projected.len())
                .map(|i| selected[&i].clone())
                .collect::<Vec<_>>();
            if let Some(candidate) = build_candidate(
                &matches,
                specs,
                topology,
                interfaces,
                mol,
                policy,
                &classified,
            ) {
                result.push(candidate);
            }
            continue;
        }
        let occurrence = order[position];
        let host = topology.parent_by_child.get(&occurrence).copied();
        for candidate in projected[occurrence].iter().rev() {
            let mut classified = classified.clone();
            if let Some(host) = host {
                let Some(join) = classified_join(
                    candidate,
                    &selected[&host],
                    specs,
                    &interfaces[&normalize_edge(occurrence, host)],
                    topology.order_by_occurrence[&occurrence],
                    mol,
                ) else {
                    continue;
                };
                classified.insert(occurrence, join);
            }
            let mut selected = selected.clone();
            selected.insert(occurrence, candidate.clone());
            pending.push((position + 1, selected, classified));
        }
    }
    Ok(result)
}
fn multiplicative_groups(
    children: &BTreeMap<usize, Vec<usize>>,
    specs: &BTreeMap<usize, FusionComponentSpec>,
    joins: &BTreeMap<usize, FusionJoin>,
) -> (BTreeMap<usize, usize>, Vec<FusionMultiplicityGroup>) {
    let mut depths = BTreeMap::new();
    let mut groups = Vec::new();
    for siblings in children.values() {
        let mut patterns: BTreeMap<ComparisonKey, Vec<usize>> = BTreeMap::new();
        for child in siblings {
            if !children.contains_key(child) {
                patterns
                    .entry(multiplicative_attachment_key(&specs[child], &joins[child]))
                    .or_default()
                    .push(*child);
            }
        }
        for mut members in patterns.into_values() {
            if !(2..=1000).contains(&members.len()) {
                continue;
            }
            members.sort_by_key(|c| multiplicative_member_order_key(&joins[c]));
            for (depth, &occurrence) in members.iter().enumerate() {
                depths.insert(occurrence, depth);
            }
            groups.push(FusionMultiplicityGroup {
                multiplier: multipliers::basic(members.len()),
                occurrence_ids: members,
            });
        }
    }
    groups.sort_by(|a, b| a.occurrence_ids.cmp(&b.occurrence_ids));
    (depths, groups)
}
fn build_citation_tree(root: usize, children: &BTreeMap<usize, Vec<usize>>) -> FusionCitationNode {
    FusionCitationNode {
        occurrence_id: root,
        children: children
            .get(&root)
            .into_iter()
            .flatten()
            .map(|&child| build_citation_tree(child, children))
            .collect(),
    }
}
fn postorder_children(root: &FusionCitationNode) -> Vec<usize> {
    let mut result = Vec::new();
    for child in &root.children {
        result.extend(postorder_children(child));
        result.push(child.occurrence_id);
    }
    result
}
fn tree_citation_scope_supported(
    root: &FusionCitationNode,
    groups: &[FusionMultiplicityGroup],
) -> bool {
    fn visit<'a>(
        node: &FusionCitationNode,
        depth: usize,
        group_by_member: &BTreeMap<usize, &'a [usize]>,
        visited: &mut BTreeSet<&'a [usize]>,
        level: &mut usize,
    ) -> bool {
        for child in node.children.iter().rev() {
            if let Some(group) = group_by_member.get(&child.occurrence_id)
                && !visited.insert(*group)
            {
                continue;
            }
            let order = depth + 1;
            if order != 1 && order != *level && order != *level + 1 {
                return false;
            }
            *level = order;
            if !visit(child, order, group_by_member, visited, level) {
                return false;
            }
        }
        true
    }
    let group_by_member = groups
        .iter()
        .flat_map(|g| {
            g.occurrence_ids
                .iter()
                .map(|&id| (id, g.occurrence_ids.as_slice()))
        })
        .collect();
    visit(root, 0, &group_by_member, &mut BTreeSet::new(), &mut 0)
}
fn with_prime_depths(mut join: FusionJoin, depths: &BTreeMap<usize, usize>) -> FusionJoin {
    let a = depths
        .get(&join.attached_occurrence())
        .copied()
        .unwrap_or(0);
    let h = depths.get(&join.host_occurrence()).copied().unwrap_or(0);
    for locant in join
        .interface
        .attached_path
        .iter_mut()
        .chain(join.interface.cited_attached_locants.iter_mut())
    {
        locant.prime_depth = a;
    }
    for locant in join
        .interface
        .host_path
        .iter_mut()
        .chain(join.interface.host_locants.iter_mut())
    {
        locant.prime_depth = h;
    }
    for side in &mut join.interface.host_sides {
        side.prime_depth = h;
    }
    join
}
fn closing_join_direction(
    pair: Edge,
    topology: &CitationTopology,
    specs: &BTreeMap<usize, FusionComponentSpec>,
) -> (usize, usize) {
    let mut nodes = [pair.0, pair.1];
    nodes.sort_by_key(|i| {
        (
            component_spec_seniority_key(&specs[i]).as_tuple(),
            component_canonicalization_key(&specs[i]),
            *i,
        )
    });
    let (left, right) = (nodes[0], nodes[1]);
    if topology.roots.contains(&left) != topology.roots.contains(&right) {
        return if topology.roots.contains(&left) {
            (right, left)
        } else {
            (left, right)
        };
    }
    nodes.sort_by_key(|i| {
        (
            topology.order_by_occurrence[i],
            component_spec_seniority_key(&specs[i]).as_tuple(),
            component_canonicalization_key(&specs[i]),
            *i,
        )
    });
    (nodes[1], nodes[0])
}
fn build_candidate(
    matches: &[FusionComponentMatch],
    specs: &BTreeMap<usize, FusionComponentSpec>,
    topology: &CitationTopology,
    interfaces: &BTreeMap<Edge, FusionInterface<usize>>,
    mol: &Molecule,
    policy: &DescriptorOptions,
    classified: &BTreeMap<usize, (FusionJoin, usize)>,
) -> Option<Candidate> {
    let by_id: BTreeMap<_, _> = matches.iter().map(|m| (m.occurrence_id, m)).collect();
    let mut primary_joins = BTreeMap::new();
    let mut ranks = BTreeMap::new();
    let mut interparent = Vec::new();
    let mut closing = Vec::new();
    for (&child, &host) in &topology.parent_by_child {
        let (mut join, rank) = classified.get(&child).cloned().or_else(|| {
            classified_join(
                by_id[&child],
                by_id[&host],
                specs,
                &interfaces[&normalize_edge(child, host)],
                topology.order_by_occurrence[&child],
                mol,
            )
        })?;
        if topology.order_by_occurrence[&child] > 1 {
            if !policy.join_kinds.contains("higher_order") {
                return None;
            }
            join.interface.kind = FusionJoinKind::HigherOrder;
            join.interface.host_sides.clear();
            join.interface.host_locants = join.interface.host_path.clone();
        }
        primary_joins.insert(child, join);
        ranks.insert(child, rank);
    }
    for pair in topology
        .interparent_pairs
        .iter()
        .chain(&topology.cycle_closing_pairs)
    {
        let (attached, host) = closing_join_direction(*pair, topology, specs);
        let (mut interface, _) = classify_ordered_fusion_interface(
            by_id[&attached],
            by_id[&host],
            &specs[&attached],
            &specs[&host],
            &interfaces[pair],
            mol,
        )?;
        let is_interparent = topology.interparent_pairs.contains(pair);
        if !is_interparent {
            interface.kind = FusionJoinKind::HigherOrder;
            interface.host_sides.clear();
            interface.host_locants = interface.host_path.clone();
        }
        let join = FusionJoin {
            order: topology.order_by_occurrence[&attached].max(1) + 1,
            interface,
        };
        if is_interparent {
            interparent.push(join);
        } else {
            closing.push(join);
        }
    }
    let children = ordered_children(&topology.parent_by_child, specs, &primary_joins, &ranks);
    let (mut depths, mut groups) = multiplicative_groups(&children, specs, &primary_joins);
    if topology.roots.len() > 1 {
        groups.clear();
    }
    let mut roots: Vec<_> = topology
        .roots
        .iter()
        .map(|&root| build_citation_tree(root, &children))
        .collect();
    let mut selection_order: Vec<_> = roots.iter().flat_map(postorder_children).collect();
    let mut opsin_citation_order = false;
    if roots.len() == 1 && interparent.is_empty() && closing.is_empty() {
        let group_by_member = groups
            .iter()
            .flat_map(|g| g.occurrence_ids.iter().map(move |&id| (id, g)))
            .collect();
        let mut alphabetical = roots[0].clone();
        order_citation_tree(&mut alphabetical, specs, &primary_joins, &group_by_member);
        // FR-4.5 / P-25.3.4.2.4 compare fusion locants in FR-3.6 citation
        // order. Select the maps in that order even if OPSIN later requires
        // the explicitly unverified seniority-order rendering below.
        selection_order = postorder_children(&alphabetical);
        if policy.enforce_interoperability_limits
            && !tree_citation_scope_supported(&alphabetical, &groups)
        {
            // Keep the seniority tree when OPSIN cannot read FR-3.6 order.
            // Validate that tree below, and mark the emitted name unverified:
            // a usable fusion citation must not become a preferred von Baeyer
            // fallback merely because of parser scope limits (P-25.5).
            opsin_citation_order = true;
        } else {
            roots[0] = alphabetical;
        }
    }
    if policy.enforce_interoperability_limits
        && roots.len() == 1
        && !tree_citation_scope_supported(&roots[0], &groups)
    {
        return None;
    }
    for root in &roots {
        add_citation_prime_depths(root, 0, &mut depths);
    }
    if roots.len() > 1 {
        for (depth, root) in roots.iter().enumerate() {
            depths.insert(root.occurrence_id, depth);
        }
    }
    let citation_children: Vec<_> = roots.iter().flat_map(postorder_children).collect();
    let primary: Vec<_> = citation_children
        .iter()
        .map(|c| with_prime_depths(primary_joins[c].clone(), &depths))
        .collect();
    let interparent: Vec<_> = interparent
        .into_iter()
        .map(|j| with_prime_depths(j, &depths))
        .collect();
    let closing: Vec<_> = closing
        .into_iter()
        .map(|j| with_prime_depths(j, &depths))
        .collect();
    let primary_count = primary.len();
    let interparent_count = interparent.len();
    let joins: Vec<_> = primary
        .into_iter()
        .chain(interparent)
        .chain(closing)
        .collect();
    let descriptors = joins
        .iter()
        .map(|j| FusionDescriptor::from_interface(&j.interface))
        .collect();
    let citation_plan = FusionCitationPlan {
        roots,
        primary_join_indices: (0..primary_count).collect(),
        interparent_join_indices: (primary_count..primary_count + interparent_count).collect(),
        cycle_closing_join_indices: (primary_count + interparent_count..joins.len()).collect(),
        interparent_occurrences: topology.interparent_occurrences.clone(),
        render_order: citation_children,
    };
    let plan_kind = if topology.roots.len() > 1 {
        "multiparent"
    } else if !topology.cycle_closing_pairs.is_empty() {
        "cyclic_component_cover"
    } else if !groups.is_empty() {
        "multiplicative_tree"
    } else if matches.len() == 2 {
        "two_component"
    } else {
        "polycomponent_tree"
    };
    let mut exact_location = topology.location_key.clone();
    let mut sorted_groups: Vec<_> = groups.iter().collect();
    sorted_groups.sort_by_key(|g| (-(g.occurrence_ids.len() as i64), g.occurrence_ids.clone()));
    exact_location.multiplicative_grouping_score = sorted_groups
        .iter()
        .map(|g| -(g.occurrence_ids.len() as i64))
        .collect();
    let score = CandidateScore {
        benzoheterocycle_seniority: Vec::new(),
        seniority: component_spec_seniority_key(&specs[&topology.roots[0]]).as_tuple(),
        location: exact_location,
        count: matches.len(),
        maximum_order: topology
            .order_by_occurrence
            .values()
            .copied()
            .max()
            .unwrap_or(0),
        first_order_locants: if topology.roots.len() == 1
            && topology.interparent_pairs.is_empty()
            && topology.cycle_closing_pairs.is_empty()
        {
            first_order_locant_key(&selection_order, &primary_joins, &depths)
        } else {
            FirstOrderLocantKey::default()
        },
        higher_order_locants: higher_order_locant_key(&selection_order, &primary_joins, &depths),
        joins: selection_order
            .iter()
            .map(|c| join_preference_key(&primary_joins[c], ranks[c]))
            .collect(),
        attached_seniority: selection_order
            .iter()
            .map(|c| component_spec_seniority_key(&specs[c]).as_tuple())
            .collect(),
    };
    let mut component_occurrences = matches.to_vec();
    component_occurrences.sort_by_key(|m| m.occurrence_id);
    let ast = FusionNameAst {
        benzoheterocycle_merge: false,
        opsin_citation_order,
        plan_kind: plan_kind.into(),
        parent_occurrences: topology.roots.clone(),
        component_occurrences,
        joins,
        citation_tree: if citation_plan.roots.len() == 1 {
            Some(citation_plan.roots[0].clone())
        } else {
            None
        },
        multiplicative_groups: groups,
        descriptors,
        citation_plan: Some(citation_plan),
    };
    Some(Candidate { score, ast })
}
fn alphabetic_index(index: usize) -> String {
    let mut value = index + 1;
    let mut letters = Vec::new();
    while value > 0 {
        letters.push((b'a' + ((value - 1) % 26) as u8) as char);
        value = (value - 1) / 26;
    }
    letters.into_iter().rev().collect()
}

pub fn render_fusion_name(
    ast: &FusionNameAst,
    registry: &FusionComponentRegistry,
) -> Result<String, String> {
    Ok(render_fusion_name_parts(ast, registry)?.concat())
}

/// FR-3.6 / P-25.3.4.2.3 compare the names of complete attached branches.
/// Bracketed locants and fusion descriptors do not contribute to the initial
/// alphabetic comparison. Keep this separate from component seniority, which
/// still selects the components and the completed numbering.
#[derive(Default, PartialEq, Eq, PartialOrd, Ord)]
struct FusionPrefixCitationKey {
    name: String,
    internal_fusion_locants: Vec<Vec<LocantKey>>,
    heteroatom_locants: Vec<Vec<LocantKey>>,
    indacene_markers: Vec<String>,
}

fn fusion_prefix_citation_key(prefix: &str) -> FusionPrefixCitationKey {
    let mut key = FusionPrefixCitationKey::default();
    let (prefix, marker) = prefix
        .strip_prefix("as-")
        .filter(|stem| *stem == "indaceno")
        .map(|stem| (stem, "as"))
        .or_else(|| {
            prefix
                .strip_prefix("s-")
                .filter(|stem| *stem == "indaceno")
                .map(|stem| (stem, "s"))
        })
        .unwrap_or((prefix, ""));
    key.indacene_markers.push(marker.into());
    // Some retained components spell heteroatom locants without brackets.
    let prefix = if prefix.starts_with(|c: char| c.is_ascii_digit()) {
        if let Some((locants, stem)) = prefix.split_once('-') {
            key.heteroatom_locants
                .push(locants.split(',').map(retained_locant_sort_key).collect());
            stem
        } else {
            prefix
        }
    } else {
        prefix
    };
    let mut bracket = None;
    for (index, c) in prefix.char_indices() {
        match c {
            '[' => bracket = Some(index + 1),
            ']' => {
                if let Some(start) = bracket.take() {
                    key.heteroatom_locants.push(
                        prefix[start..index]
                            .split(',')
                            .map(retained_locant_sort_key)
                            .collect(),
                    );
                }
            }
            _ if bracket.is_none() && c.is_alphabetic() => {
                key.name.extend(c.to_lowercase());
            }
            _ => {}
        }
    }
    key
}

fn fusion_group_multiplier(
    group: &FusionMultiplicityGroup,
    specs: &BTreeMap<usize, FusionComponentSpec>,
) -> String {
    if group
        .occurrence_ids
        .iter()
        .any(|id| uses_complex_multiplier(&specs[id]))
    {
        multipliers::complex_(group.occurrence_ids.len())
    } else {
        group.multiplier.clone()
    }
}

/// Finalize FR-3.6 order before scope validation, priming, and rendering. Each
/// branch is compared as a whole, after ordering its own attached components.
fn order_citation_tree(
    node: &mut FusionCitationNode,
    specs: &BTreeMap<usize, FusionComponentSpec>,
    joins: &BTreeMap<usize, FusionJoin>,
    groups: &BTreeMap<usize, &FusionMultiplicityGroup>,
) -> FusionPrefixCitationKey {
    let child_keys: BTreeMap<_, _> = node
        .children
        .iter_mut()
        .map(|child| {
            (
                child.occurrence_id,
                order_citation_tree(child, specs, joins, groups),
            )
        })
        .collect();
    node.children.sort_by(|left, right| {
        child_keys[&left.occurrence_id].cmp(&child_keys[&right.occurrence_id])
    });
    let mut key = FusionPrefixCitationKey::default();
    let mut included_groups = BTreeSet::new();
    for child in &node.children {
        let group = groups.get(&child.occurrence_id);
        if let Some(group) = group {
            if !included_groups.insert(&group.occurrence_ids) {
                continue;
            }
            // Ignore this branch's own multiplier when its parent compares it;
            // a multiplier inside this multipart branch is integral to its name.
            key.name.push_str(&fusion_group_multiplier(group, specs));
        }
        let child_key = &child_keys[&child.occurrence_id];
        key.name.push_str(&child_key.name);
        key.internal_fusion_locants
            .extend_from_slice(&child_key.internal_fusion_locants);
        let members = group.map_or_else(
            || vec![child.occurrence_id],
            |group| group.occurrence_ids.clone(),
        );
        for member in members {
            let join = &joins[&member];
            let mut locants: Vec<_> = join
                .attached_locants()
                .iter()
                .chain(join.host_locants())
                .map(|l| retained_locant_sort_key(&l.text))
                .collect();
            locants.sort();
            key.internal_fusion_locants.push(locants);
        }
        key.heteroatom_locants
            .extend_from_slice(&child_key.heteroatom_locants);
        key.indacene_markers
            .extend_from_slice(&child_key.indacene_markers);
    }
    let mut own = fusion_prefix_citation_key(&specs[&node.occurrence_id].attached_prefix);
    key.name.push_str(&own.name);
    key.heteroatom_locants.append(&mut own.heteroatom_locants);
    key.indacene_markers.append(&mut own.indacene_markers);
    key
}

fn add_citation_prime_depths(
    node: &FusionCitationNode,
    order: usize,
    depths: &mut BTreeMap<usize, usize>,
) {
    if order > 1 {
        *depths.entry(node.occurrence_id).or_default() += order - 1;
    }
    for child in &node.children {
        add_citation_prime_depths(child, order + 1, depths);
    }
}

/// Ordered citation pieces. Atom/bond bindings are not consumed by the plain-name route.
pub fn render_fusion_name_parts(
    ast: &FusionNameAst,
    registry: &FusionComponentRegistry,
) -> Result<Vec<String>, String> {
    let matches: BTreeMap<_, _> = ast
        .component_occurrences
        .iter()
        .map(|m| (m.occurrence_id, m))
        .collect();
    let specs: BTreeMap<_, _> = matches
        .iter()
        .map(|(&i, m)| registry.spec_for_match(m).map(|s| (i, s)))
        .collect::<Result<_, _>>()?;
    let omit_monocyclic_join = matches.len() == 2
        && ast.joins.len() == 1
        && ast.joins[0].kind() == FusionJoinKind::Ortho
        && specs
            .values()
            .all(|s| s.rings.len() == 1 && s.atoms().iter().all(|a| a.symbol == "C"));
    let mut descriptors: BTreeMap<usize, Vec<&FusionDescriptor>> = BTreeMap::new();
    if ast.joins.len() != ast.descriptors.len() {
        return Err("descriptor count must match join count".into());
    }
    for (join, descriptor) in ast.joins.iter().zip(&ast.descriptors) {
        descriptors
            .entry(join.attached_occurrence())
            .or_default()
            .push(descriptor);
    }
    let groups: BTreeMap<_, _> = ast
        .multiplicative_groups
        .iter()
        .flat_map(|g| g.occurrence_ids.iter().map(move |&i| (i, g)))
        .collect();
    let fallback_plan;
    let plan = if let Some(plan) = &ast.citation_plan {
        plan
    } else {
        fallback_plan = ast
            .citation_tree
            .clone()
            .map(|r| FusionCitationPlan::from_tree(r, &ast.joins))
            .ok_or("fusion AST has no citation plan")?;
        &fallback_plan
    };
    if plan.roots.is_empty() {
        return Err("fusion AST has no citation root".into());
    }
    if plan.roots.len() > 1
        || !plan.interparent_join_indices.is_empty()
        || !plan.cycle_closing_join_indices.is_empty()
    {
        let mut pieces = Vec::new();
        for occurrence in &plan.render_order {
            let spec = &specs[occurrence];
            let joined = descriptors.get(occurrence).ok_or_else(|| {
                format!("nonparent component occurrence {occurrence} has no fusion descriptor")
            })?;
            pieces.push(spec.attached_prefix.clone());
            pieces.push(combine_rendered_descriptors(
                &joined
                    .iter()
                    .map(|d| render_descriptor(d, false))
                    .collect::<Vec<_>>(),
                joined.iter().any(|d| d.kind == FusionJoinKind::HigherOrder),
                false,
            ));
        }
        let root_specs: Vec<_> = plan
            .roots
            .iter()
            .map(|root| &specs[&root.occurrence_id])
            .collect();
        let parent_names: BTreeSet<_> = root_specs
            .iter()
            .map(|s| parent_component_name(s))
            .collect();
        if parent_names.len() != 1 {
            return Err("multiparent rendering requires identical parent components".into());
        }
        let styles: BTreeSet<_> = root_specs
            .iter()
            .map(|s| s.multiplicative_prefix_style.as_str())
            .collect();
        if styles.len() != 1 {
            return Err("multiparent components require one multiplier style".into());
        }
        let name = parent_names.into_iter().next().unwrap_or_default();
        if root_specs.len() == 1 {
            pieces.push(name);
        } else {
            if root_specs.len() > 1000 {
                return Err("unsupported multiparent multiplicity".into());
            }
            let multiplier = if styles.contains("complex") {
                multipliers::complex_(root_specs.len())
            } else {
                multipliers::basic(root_specs.len())
            };
            pieces.push(format!("{multiplier}{name}"));
        }
        return Ok(pieces);
    }
    struct Renderer<'a> {
        registry: &'a FusionComponentRegistry,
        specs: &'a BTreeMap<usize, FusionComponentSpec>,
        descriptors: &'a BTreeMap<usize, Vec<&'a FusionDescriptor>>,
        groups: &'a BTreeMap<usize, &'a FusionMultiplicityGroup>,
        root: &'a FusionCitationNode,
        rendered_groups: BTreeSet<Vec<usize>>,
        omit_join: bool,
    }
    impl Renderer<'_> {
        fn attachment(&mut self, node: &FusionCitationNode) -> Result<Vec<String>, String> {
            let mut pieces = self.children(node)?;
            let spec = &self.specs[&node.occurrence_id];
            if self.omit_join {
                return Ok(vec![spec.attached_prefix.clone()]);
            }
            let descriptors = self
                .descriptors
                .get(&node.occurrence_id)
                .ok_or("attached component has no descriptor")?;
            let texts = descriptors
                .iter()
                .map(|d| {
                    render_descriptor(
                        d,
                        descriptors.len() == 1
                            && node.children.is_empty()
                            && omit_attached_locants(self.registry, &spec.key, d.kind),
                    )
                })
                .collect::<Vec<_>>();
            pieces.push(spec.attached_prefix.clone());
            pieces.push(combine_rendered_descriptors(
                &texts,
                descriptors
                    .iter()
                    .any(|d| d.kind == FusionJoinKind::HigherOrder),
                false,
            ));
            Ok(pieces)
        }
        fn children(&mut self, node: &FusionCitationNode) -> Result<Vec<String>, String> {
            let mut pieces = Vec::new();
            for child in &node.children {
                let Some(group) = self.groups.get(&child.occurrence_id).copied() else {
                    pieces.extend(self.attachment(child)?);
                    continue;
                };
                if !self.rendered_groups.insert(group.occurrence_ids.clone()) {
                    continue;
                }
                let members: Vec<_> = group
                    .occurrence_ids
                    .iter()
                    .map(|&id| {
                        citation_node(self.root, id)
                            .ok_or("multiplicative member absent from citation")
                    })
                    .collect::<Result<_, _>>()?;
                if members.iter().any(|m| !m.children.is_empty()) {
                    return Err("multiplicative rendering is limited to leaf components".into());
                }
                let prefixes: BTreeSet<_> = members
                    .iter()
                    .map(|m| self.specs[&m.occurrence_id].attached_prefix.clone())
                    .collect();
                if prefixes.len() != 1 {
                    return Err("a multiplicative group must use one attached prefix".into());
                }
                let multiplier = fusion_group_multiplier(group, self.specs);
                let mut texts = Vec::new();
                let mut higher = false;
                let mut side_only = true;
                for member in members {
                    let descriptor = self
                        .descriptors
                        .get(&member.occurrence_id)
                        .and_then(|d| d.first())
                        .ok_or("multiplicative member has no descriptor")?;
                    let omit = omit_attached_locants(
                        self.registry,
                        &self.specs[&member.occurrence_id].key,
                        descriptor.kind,
                    );
                    texts.push(render_descriptor(descriptor, omit));
                    higher |= descriptor.kind == FusionJoinKind::HigherOrder;
                    side_only &= omit;
                }
                pieces.push(multiplier);
                pieces.push(prefixes.into_iter().next().unwrap_or_default());
                pieces.push(combine_rendered_descriptors(&texts, higher, side_only));
            }
            Ok(pieces)
        }
    }
    let root = &plan.roots[0];
    let mut renderer = Renderer {
        registry,
        specs: &specs,
        descriptors: &descriptors,
        groups: &groups,
        root,
        rendered_groups: BTreeSet::new(),
        omit_join: omit_monocyclic_join,
    };
    let mut pieces = renderer.children(root)?;
    pieces.push(parent_component_name(&specs[&root.occurrence_id]));
    Ok(pieces)
}
fn parent_component_name(spec: &FusionComponentSpec) -> String {
    let locants = &spec.template.default_indicated_h;
    if locants.is_empty() {
        return spec.parent_name.clone();
    }
    let prefix = locants
        .iter()
        .map(|l| format!("{l}H"))
        .collect::<Vec<_>>()
        .join(",")
        + "-";
    spec.parent_name
        .strip_prefix(&prefix)
        .unwrap_or(&spec.parent_name)
        .to_owned()
}
fn citation_node(root: &FusionCitationNode, occurrence: usize) -> Option<&FusionCitationNode> {
    if root.occurrence_id == occurrence {
        return Some(root);
    }
    root.children
        .iter()
        .find_map(|c| citation_node(c, occurrence))
}
fn combine_rendered_descriptors(
    descriptors: &[String],
    higher_order: bool,
    side_only: bool,
) -> String {
    let interiors: Vec<_> = descriptors.iter().map(|d| &d[1..d.len() - 1]).collect();
    let separator = if higher_order {
        ";"
    } else if side_only {
        ","
    } else {
        ":"
    };
    format!("[{}]", interiors.join(separator))
}
fn render_descriptor(descriptor: &FusionDescriptor, omit: bool) -> String {
    if omit
        && matches!(
            descriptor.kind,
            FusionJoinKind::Ortho | FusionJoinKind::OrthoPeri
        )
        && !descriptor.parent_sides.is_empty()
    {
        format!(
            "[{}]",
            descriptor
                .parent_sides
                .iter()
                .map(ToString::to_string)
                .collect::<String>()
        )
    } else {
        descriptor.render()
    }
}
fn omit_attached_locants(
    registry: &FusionComponentRegistry,
    key: &str,
    kind: FusionJoinKind,
) -> bool {
    registry.get(key).is_some_and(|component| match kind {
        FusionJoinKind::Ortho => component.omit_attached_locants,
        FusionJoinKind::OrthoPeri => component.omit_ortho_peri_attached_locants,
        FusionJoinKind::HigherOrder => false,
    })
}

#[cfg(test)]
mod tests {
    use super::super::super::molecule::{Atom, Bond};
    use super::super::registry::fusion_component_registry;
    use super::*;

    fn fixture(components: &[(&str, &[usize])]) -> (Molecule, Vec<FusionComponentMatch>) {
        let registry = fusion_component_registry();
        let mut mol = Molecule::new();
        let mut matches = Vec::new();
        for (face, (key, ids)) in components.iter().enumerate() {
            let spec = registry.get(key).unwrap().spec;
            let mapping: Vec<_> = spec
                .locants()
                .iter()
                .cloned()
                .zip(ids.iter().copied())
                .collect();
            let atoms: BTreeMap<_, _> = mapping.iter().cloned().collect();
            for atom in spec.atoms() {
                let id = atoms[&atom.locant];
                if !mol.atoms.contains_key(&id) {
                    let mut atom = Atom::new(id, &atom.symbol);
                    atom.is_aromatic = !["O", "S", "Se", "Te"].contains(&atom.symbol.as_str());
                    mol.add_atom(atom).unwrap();
                }
            }
            for bond in spec.bonds() {
                let (u, v) = (atoms[&bond.locants.0], atoms[&bond.locants.1]);
                if mol.get_bond(u, v).is_none() {
                    mol.add_bond(Bond::new(mol.bonds.len(), u, v, 1)).unwrap();
                }
            }
            matches.push(FusionComponentMatch {
                occurrence_id: face,
                spec_key: (*key).into(),
                covered_face_ids: [face].into_iter().collect(),
                local_to_input_atom: mapping.clone(),
                local_to_skeleton_atom: mapping,
                template_name: spec.template.name.clone(),
                ..Default::default()
            });
        }
        (mol, matches)
    }

    #[test]
    fn ranks_local_maps_and_omits_certified_benzo_locants() {
        let (mol, mut matches) = fixture(&[
            ("pyridine", &[0, 1, 2, 3, 4, 5]),
            ("benzene", &[1, 2, 6, 7, 8, 9]),
        ]);
        mol.set_benzoheterocycle_names(false);
        let mut reflected = matches[0].clone();
        for (_, atom) in &mut reflected.local_to_input_atom {
            *atom = match *atom {
                1 => 5,
                2 => 4,
                4 => 2,
                5 => 1,
                other => other,
            };
        }
        reflected.local_to_skeleton_atom = reflected.local_to_input_atom.clone();
        matches.insert(0, reflected);
        let registry = fusion_component_registry();
        let ast =
            build_fusion_name_ast(&mol, &matches, registry, &DescriptorOptions::default()).unwrap();
        assert_eq!(
            render_fusion_name(&ast, registry).unwrap(),
            "benzo[b]pyridine"
        );
        assert_eq!(ast.joins[0].interface.ordered_input_edges, vec![(1, 2)]);
    }

    #[test]
    fn combines_equivalent_leaf_components_and_primes_their_maps() {
        let (mol, matches) = fixture(&[
            ("pyridine", &[0, 1, 2, 3, 4, 5]),
            ("benzene", &[1, 2, 6, 7, 8, 9]),
            ("benzene", &[3, 4, 10, 11, 12, 13]),
        ]);
        let registry = fusion_component_registry();
        let ast =
            build_fusion_name_ast(&mol, &matches, registry, &DescriptorOptions::default()).unwrap();
        assert_eq!(ast.plan_kind, "multiplicative_tree");
        assert_eq!(ast.multiplicative_groups.len(), 1);
        assert_eq!(
            render_fusion_name(&ast, registry).unwrap(),
            "dibenzo[b,d]pyridine"
        );
        let depths: BTreeSet<_> = ast
            .joins
            .iter()
            .map(|j| j.attached_locants()[0].prime_depth)
            .collect();
        assert_eq!(depths, [0, 1].into_iter().collect());
    }

    #[test]
    fn higher_order_descriptors_preserve_both_numeric_paths() {
        let descriptor = FusionDescriptor {
            attached_locants: vec![
                ComponentLocant {
                    component_id: 2,
                    text: "1".into(),
                    prime_depth: 1,
                },
                ComponentLocant {
                    component_id: 2,
                    text: "2".into(),
                    prime_depth: 1,
                },
            ],
            parent_locants: vec![
                ComponentLocant {
                    component_id: 1,
                    text: "3".into(),
                    prime_depth: 0,
                },
                ComponentLocant {
                    component_id: 1,
                    text: "4".into(),
                    prime_depth: 0,
                },
            ],
            kind: FusionJoinKind::HigherOrder,
            ..Default::default()
        };
        assert_eq!(render_descriptor(&descriptor, true), "[1',2':3,4]");
        assert_eq!(
            combine_rendered_descriptors(&[descriptor.render(), "[2',3':4,5]".into()], true, false),
            "[1',2':3,4;2',3':4,5]"
        );
    }

    fn rendering_fixture(
        components: &[&str],
        links: &[(usize, usize, &[&str], &[&str])],
    ) -> FusionNameAst {
        let parents: BTreeMap<_, _> = links.iter().map(|&(c, p, _, _)| (c, p)).collect();
        let mut children: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        let mut joins = Vec::new();
        for &(child, host, attached, parent) in links {
            children.entry(host).or_default().push(child);
            let mut order = 1;
            let mut ancestor = host;
            while let Some(&parent) = parents.get(&ancestor) {
                order += 1;
                ancestor = parent;
            }
            let locants = |id, depth, texts: &[&str]| {
                texts
                    .iter()
                    .map(|text| ComponentLocant {
                        component_id: id,
                        text: (*text).into(),
                        prime_depth: depth,
                    })
                    .collect()
            };
            joins.push(FusionJoin {
                order,
                interface: OrderedFusionInterface {
                    attached_occurrence: child,
                    host_occurrence: host,
                    cited_attached_locants: locants(child, order - 1, attached),
                    host_sides: if order == 1 {
                        parent
                            .iter()
                            .map(|side| FusionSide {
                                component_id: host,
                                letter: (*side).into(),
                                ..Default::default()
                            })
                            .collect()
                    } else {
                        Vec::new()
                    },
                    host_locants: if order > 1 {
                        locants(host, order - 2, parent)
                    } else {
                        Vec::new()
                    },
                    kind: if order == 1 {
                        FusionJoinKind::Ortho
                    } else {
                        FusionJoinKind::HigherOrder
                    },
                    ..Default::default()
                },
            });
        }
        let root = build_citation_tree(0, &children);
        let mut ast = FusionNameAst {
            component_occurrences: components
                .iter()
                .enumerate()
                .map(|(occurrence_id, key)| FusionComponentMatch {
                    occurrence_id,
                    spec_key: (*key).into(),
                    ..Default::default()
                })
                .collect(),
            descriptors: joins
                .iter()
                .map(|join| FusionDescriptor::from_interface(&join.interface))
                .collect(),
            citation_plan: Some(FusionCitationPlan::from_tree(root.clone(), &joins)),
            citation_tree: Some(root),
            joins,
            ..Default::default()
        };
        order_rendering_fixture(&mut ast);
        ast
    }

    fn order_rendering_fixture(ast: &mut FusionNameAst) {
        let registry = fusion_component_registry();
        let specs = ast
            .component_occurrences
            .iter()
            .map(|m| (m.occurrence_id, registry.spec_for_match(m).unwrap()))
            .collect();
        let joins = ast
            .joins
            .iter()
            .map(|join| (join.attached_occurrence(), join.clone()))
            .collect();
        let groups = ast
            .multiplicative_groups
            .iter()
            .flat_map(|g| g.occurrence_ids.iter().map(move |&id| (id, g)))
            .collect();
        let root = ast.citation_tree.as_mut().unwrap();
        order_citation_tree(root, &specs, &joins, &groups);
        ast.citation_plan = Some(FusionCitationPlan::from_tree(root.clone(), &ast.joins));
    }

    #[test]
    fn citation_keys_ignore_locants_and_indacene_italics_until_names_tie() {
        let mut prefixes = [
            "[1,2,4]triazolo",
            "[1,3]oxazolo",
            "[1]benzothieno",
            "[1,2]oxazolo",
        ];
        prefixes.sort_by_cached_key(|prefix| fusion_prefix_citation_key(prefix));
        assert_eq!(
            prefixes,
            [
                "[1]benzothieno",
                "[1,2]oxazolo",
                "[1,3]oxazolo",
                "[1,2,4]triazolo",
            ]
        );
        let mut prefixes = ["s-indaceno", "imidazo", "as-indaceno", "naphtho"];
        prefixes.sort_by_cached_key(|prefix| fusion_prefix_citation_key(prefix));
        assert_eq!(
            prefixes,
            ["imidazo", "as-indaceno", "s-indaceno", "naphtho"]
        );
        assert!(
            fusion_prefix_citation_key("1,5-naphthyridino")
                < fusion_prefix_citation_key("1,10-naphthyridino")
        );
    }

    #[test]
    fn rendering_orders_complete_branches_and_retains_bound_descriptors() {
        let registry = fusion_component_registry();
        let ast = rendering_fixture(
            &["pyrazine", "imidazole", "pyrrole", "furan"],
            &[
                (1, 0, &["4", "5"], &["e"]),
                (2, 0, &["2", "3"], &["b"]),
                (3, 2, &["2", "3"], &["4", "5"]),
            ],
        );
        let original = ast.clone();
        assert_eq!(
            render_fusion_name(&ast, registry).unwrap(),
            "furo[2',3':4,5]pyrrolo[2,3-b]imidazo[4,5-e]pyrazine"
        );
        assert_eq!(ast, original);
        assert_eq!(ast.citation_plan.unwrap().render_order, [3, 2, 1]);

        let ast = rendering_fixture(
            &["pyridine", "oxazole", "furan", "isoxazole"],
            &[
                (1, 0, &["4", "5"], &["a"]),
                (2, 0, &["2", "3"], &["b"]),
                (3, 0, &["5", "4"], &["e"]),
            ],
        );
        assert_eq!(
            render_fusion_name(&ast, registry).unwrap(),
            "furo[2,3-b][1,2]oxazolo[5,4-e][1,3]oxazolo[4,5-a]pyridine"
        );

        // The descriptors inside otherwise identical branches break a tie;
        // their descriptors for attachment to pyridine remain bound to them.
        let ast = rendering_fixture(
            &["pyridine", "pyran", "pyran", "benzene", "benzene"],
            &[
                (1, 0, &["2", "3"], &["b"]),
                (2, 0, &["2", "3"], &["e"]),
                (3, 1, &["1", "2"], &["5", "6"]),
                (4, 2, &["1", "2"], &["3", "4"]),
            ],
        );
        assert_eq!(
            render_fusion_name(&ast, registry).unwrap(),
            "benzo[1',2':3,4]pyrano[2,3-e]benzo[1',2':5,6]pyrano[2,3-b]pyridine"
        );
    }

    #[test]
    fn multipliers_contribute_only_inside_a_multipart_branch_citation() {
        let registry = fusion_component_registry();
        let mut ast = rendering_fixture(
            &[
                "pyridine",
                "pyran",
                "benzene",
                "benzene",
                "cycloheptatriene",
            ],
            &[
                (1, 0, &["2", "3"], &["b"]),
                (2, 1, &["1", "2"], &["4", "5"]),
                (3, 1, &["1", "2"], &["5", "6"]),
                (4, 0, &["1", "2"], &["e"]),
            ],
        );
        ast.multiplicative_groups.push(FusionMultiplicityGroup {
            occurrence_ids: vec![2, 3],
            multiplier: "di".into(),
        });
        let depths = [(2, 1), (3, 2)].into_iter().collect();
        for join in &mut ast.joins {
            *join = with_prime_depths(join.clone(), &depths);
        }
        ast.descriptors = ast
            .joins
            .iter()
            .map(|join| FusionDescriptor::from_interface(&join.interface))
            .collect();
        order_rendering_fixture(&mut ast);
        assert_eq!(
            render_fusion_name(&ast, registry).unwrap(),
            "cyclohepta[e]dibenzo[1',2':4,5;1'',2'':5,6]pyrano[2,3-b]pyridine"
        );

        let mut ast = rendering_fixture(
            &["pyridine", "pyran", "pyran", "furan"],
            &[
                (1, 0, &["2", "3"], &["b"]),
                (2, 0, &["4", "5"], &["d"]),
                (3, 0, &["2", "3"], &["e"]),
            ],
        );
        let depths = [(2, 1)].into_iter().collect();
        for join in &mut ast.joins {
            *join = with_prime_depths(join.clone(), &depths);
        }
        ast.descriptors = ast
            .joins
            .iter()
            .map(|join| FusionDescriptor::from_interface(&join.interface))
            .collect();
        ast.multiplicative_groups.push(FusionMultiplicityGroup {
            occurrence_ids: vec![1, 2],
            multiplier: "di".into(),
        });
        order_rendering_fixture(&mut ast);
        assert_eq!(
            render_fusion_name(&ast, registry).unwrap(),
            "furo[2,3-e]dipyrano[2,3-b:4',5'-d]pyridine"
        );
    }

    #[test]
    fn built_plan_and_bound_joins_follow_complete_branch_order() {
        let (mol, matches) = rematched_fixture(&[
            ("pyrazine", &[0, 1, 2, 3, 4, 5]),
            ("imidazole", &[6, 7, 8, 1, 2]),
            ("pyrrole", &[9, 4, 5, 10, 11]),
            ("furan", &[12, 10, 11, 13, 14]),
        ]);
        let registry = fusion_component_registry();
        let ast =
            build_fusion_name_ast(&mol, &matches, registry, &DescriptorOptions::default()).unwrap();
        let plan = ast.citation_plan.as_ref().unwrap();
        let specs: Vec<_> = plan
            .render_order
            .iter()
            .map(|id| &ast.component_occurrences[*id].spec_key)
            .collect();
        assert_eq!(specs, ["furan", "pyrrole", "imidazole"]);
        assert_eq!(plan.render_order, postorder_children(&plan.roots[0]));
        assert_eq!(
            ast.joins
                .iter()
                .map(FusionJoin::attached_occurrence)
                .collect::<Vec<_>>(),
            plan.render_order
        );
        assert!(tree_citation_scope_supported(
            &plan.roots[0],
            &ast.multiplicative_groups
        ));
        let name = render_fusion_name(&ast, registry).unwrap();
        assert!(name.starts_with("furo["), "{name}");
        assert!(name.find("pyrrolo[").unwrap() < name.find("imidazo[").unwrap());
        assert!(name.ends_with("pyrazine"), "{name}");
        let furan_join = &ast.joins[0];
        assert!(
            furan_join
                .attached_locants()
                .iter()
                .all(|l| l.prime_depth == 1)
        );
        assert!(furan_join.host_locants().iter().all(|l| l.prime_depth == 0));
    }

    #[test]
    fn citation_scope_checks_the_alphabetical_tree_used_for_rendering() {
        let ast = rendering_fixture(
            &["quinoline", "pyridine", "imidazole", "thiophene", "pyrrole"],
            &[
                (1, 0, &["3", "4"], &["b"]),
                (2, 1, &["1", "2"], &["1", "2"]),
                (3, 2, &["2", "3"], &["4", "5"]),
                (4, 1, &["2", "3"], &["5", "6"]),
            ],
        );
        let root = ast.citation_tree.as_ref().unwrap();
        let plan = ast.citation_plan.as_ref().unwrap();
        assert_eq!(postorder_children(root), [4, 3, 2, 1]);
        assert_eq!(plan.render_order, postorder_children(root));
        assert_eq!(plan.roots, std::slice::from_ref(root));
        assert_eq!(
            render_fusion_name(&ast, fusion_component_registry()).unwrap(),
            "pyrrolo[2',3':5,6]thieno[2'',3'':4',5']imidazo[1',2':1,2]pyrido[3,4-b]quinoline"
        );
        assert!(!tree_citation_scope_supported(root, &[]));

        // The previous seniority order is accepted by OPSIN's scope rule, but
        // it must not validate a different, alphabetically rendered traversal.
        let mut seniority_tree = root.clone();
        seniority_tree.children[0].children.reverse();
        assert!(tree_citation_scope_supported(&seniority_tree, &[]));
        assert_eq!(postorder_children(&seniority_tree), [3, 2, 4, 1]);
        let mut depths = BTreeMap::new();
        add_citation_prime_depths(root, 0, &mut depths);
        assert_eq!(depths, [(2, 1), (3, 2), (4, 1)].into_iter().collect());
    }

    #[test]
    fn fusion_locants_follow_alphabetical_citation_order() -> Result<(), String> {
        for (smiles, expected) in [
            (
                // Pyrrole is senior to furan, but furo is cited first. The
                // tied parent letter set {b,e} is minimized in citation order.
                "n1c2[nH]ccc2nc2occc12",
                "5H-furo[2,3-b]pyrrolo[2,3-e]pyrazine",
            ),
            (
                // Furan is senior to thiophene and is also cited first.
                "O1C=CC2=NC3=C(C=C21)SC=C3",
                "furo[3,2-b]thieno[2,3-e]pyridine",
            ),
        ] {
            let name = crate::naming::openclatura::name_smiles(smiles)?;
            assert_eq!(name.text, expected);
            assert_eq!(name.status, crate::naming::NameStatus::Preferred);
            assert_strict_fusion_name_identity(smiles, &name)?;
        }
        Ok(())
    }

    #[test]
    fn identical_leaf_components_multiply_across_attached_interface_orbits() -> Result<(), String> {
        for (smiles, expected) in [
            ("O1C=CC2=NC=3C(C=C21)=COC3", "difuro[3,2-b:3',4'-e]pyridine"),
            (
                "O1CC=CC=2C1=C1C(=NC2)C=COC1",
                "2H,10H-dipyrano[4,3-b:2',3'-d]pyridine",
            ),
            ("c1oc2nc3occc3cc2c1", "difuro[2,3-b:3',2'-e]pyridine"),
            ("c1sc2nc3sccc3nc2c1", "dithieno[2,3-b:3',2'-e]pyrazine"),
            ("O1C=CC2=C1C=C1C(=N2)C=CO1", "difuro[3,2-b:2',3'-e]pyridine"),
            (
                "C1OC=C2C3=C1C=1C(=NC3=CC=C2)OC=CC1",
                "1H-dipyrano[2,3-b:3',4',5'-de]quinoline",
            ),
        ] {
            let name = crate::naming::openclatura::name_smiles(smiles)?;
            assert_eq!(name.text, expected);
            assert_eq!(name.status, crate::naming::NameStatus::Preferred);
            assert_strict_fusion_name_identity(smiles, &name)?;
        }
        Ok(())
    }

    fn assert_strict_fusion_name_identity(
        smiles: &str,
        name: &crate::naming::openclatura::Name,
    ) -> Result<(), String> {
        let parser = opsin::Parser::new().map_err(|error| error.to_string())?;
        for text in std::iter::once(&name.text).chain(name.alternatives.iter().map(|n| &n.name)) {
            let parsed = parser.parse(text, &opsin::ParseOptions::strict());
            assert_eq!(
                parsed.status,
                opsin::Status::Success,
                "{text}: {}",
                parsed.message
            );
            assert!(parsed.warnings.is_empty(), "{text}: {:?}", parsed.warnings);
            let round_trip = parsed
                .structure
                .ok_or("Missing parsed fusion graph")?
                .semantic_cxsmiles()
                .map_err(|error| error.to_string())?;
            crate::naming::verify_identity(smiles, &round_trip)?;
        }
        Ok(())
    }

    #[test]
    fn naphthyridine_parent_simplifies_alphabetized_fusion_name() -> Result<(), String> {
        let smiles = "S1CC=C2C1=NC=1N2C=2C(=C3C1N=C1C=CC=CC1=C3)N=CC2";
        let name = crate::naming::openclatura::name_smiles(smiles)?;
        assert_eq!(
            name.text,
            "2H-benzo[b]pyrrolo[2,3-f]thieno[2',3':4,5]imidazo[1,2-h][1,7]naphthyridine"
        );
        assert_eq!(name.status, crate::naming::NameStatus::Preferred);
        assert_strict_fusion_name_identity(smiles, &name)
    }

    #[test]
    fn alphabetized_corpus_prefixes_preserve_numbering_and_round_trip() -> Result<(), String> {
        let smiles = "CC1=C(SC(=N1)NC(=O)CSC2=NN=C3N2C(=NC4=C3C5=C(S4)CCCC5)C(C)C)C";
        let name = crate::naming::openclatura::name_smiles(smiles)?;
        assert_eq!(
            name.text,
            "N-(4,5-dimethyl-1,3-thiazol-2-yl)-2-{[5-(propan-2-yl)-8,9,10,11-tetrahydro[1]benzothieno[3,2-e][1,2,4]triazolo[4,3-c]pyrimidin-3-yl]sulfanyl}acetamide"
        );
        assert_eq!(
            name.status,
            crate::naming::NameStatus::Unverified(crate::naming::NameReason::OpsinFusionLayout)
        );
        assert_strict_fusion_name_identity(smiles, &name)
    }

    fn rematched_fixture(components: &[(&str, &[usize])]) -> (Molecule, Vec<FusionComponentMatch>) {
        let (mol, known) = fixture(components);
        let registry = fusion_component_registry();
        let mut faces = Vec::new();
        for matched in known {
            let spec = registry.spec_for_match(&matched).unwrap();
            let atoms = matched.input_atom_by_locant();
            for ring in spec.rings {
                let atom_cycle: Vec<_> = ring.iter().map(|l| atoms[l]).collect();
                let edge_cycle = (0..atom_cycle.len())
                    .map(|i| {
                        mol.get_bond(atom_cycle[i], atom_cycle[(i + 1) % atom_cycle.len()])
                            .unwrap()
                            .idx
                    })
                    .collect();
                faces.push(Face {
                    id: faces.len(),
                    size: atom_cycle.len(),
                    atom_cycle,
                    edge_cycle,
                });
            }
        }
        let matches = registry.match_faces(&mol, &faces, None).unwrap();
        (mol, matches)
    }

    #[test]
    fn graph_matched_citations_prioritize_first_order_fusion_locants() -> Result<(), String> {
        type CitationCase<'a> = (Vec<(&'a str, Vec<usize>)>, &'a str);
        let cases: Vec<CitationCase<'_>> = vec![
            (
                vec![
                    ("furan", vec![0, 1, 2, 3, 4]),
                    ("thiophene", vec![5, 1, 2, 6, 7]),
                ],
                "thieno[2,3-b]furan",
            ),
            (
                vec![
                    ("pyridine", vec![0, 1, 2, 3, 4, 5]),
                    ("furan", vec![6, 2, 1, 7, 8]),
                    ("thiophene", vec![9, 4, 5, 10, 11]),
                ],
                "furo[3,2-b]thieno[2,3-e]pyridine",
            ),
            (
                vec![
                    ("pyridine", vec![0, 1, 2, 3, 4, 5]),
                    ("thiophene", vec![6, 1, 2, 7, 8]),
                    ("furan", vec![9, 7, 8, 10, 11]),
                ],
                // FR-4.5(c): first-order {2,3} wins over {4,5} before
                // comparing the locants of the higher-order furo component.
                "furo[2',3':4,5]thieno[2,3-b]pyridine",
            ),
            (
                vec![
                    ("pyridine", vec![0, 1, 2, 3, 4, 5]),
                    ("furan", vec![6, 2, 1, 7, 8]),
                    ("furan", vec![9, 4, 5, 10, 11]),
                ],
                "difuro[3,2-b:2',3'-e]pyridine",
            ),
            (
                vec![
                    ("pyridine", vec![0, 1, 2, 3, 4, 5]),
                    ("furan", vec![6, 2, 1, 7, 8]),
                    ("furan", vec![9, 10, 4, 5, 11]),
                ],
                "difuro[3,2-b:3',4'-e]pyridine",
            ),
            (
                vec![
                    ("pyridine", vec![0, 1, 2, 3, 4, 5]),
                    ("pyran", vec![6, 7, 2, 1, 8, 9]),
                    ("pyran", vec![10, 3, 4, 11, 12, 13]),
                ],
                "dipyrano[4,3-b:2',3'-d]pyridine",
            ),
        ];
        let registry = fusion_component_registry();
        for (components, expected) in cases {
            for renumber in [false, true] {
                let components: Vec<_> = components
                    .iter()
                    .map(|(key, ids)| {
                        (
                            *key,
                            ids.iter()
                                .map(|&id| if renumber { 1000 - 17 * id } else { id })
                                .collect::<Vec<_>>(),
                        )
                    })
                    .collect();
                let refs: Vec<_> = components
                    .iter()
                    .map(|(key, ids)| (*key, ids.as_slice()))
                    .collect();
                let (mol, matches) = rematched_fixture(&refs);
                let ast =
                    build_fusion_name_ast(&mol, &matches, registry, &DescriptorOptions::default())
                        .unwrap();
                assert_eq!(render_fusion_name(&ast, registry).unwrap(), expected);
            }
        }
        let parser = opsin::Parser::new().map_err(|error| error.to_string())?;
        let mut graphs = Vec::new();
        for name in [
            "furo[2',3':4,5]thieno[2,3-b]pyridine",
            "furo[3',2':2,3]thieno[5,4-b]pyridine",
        ] {
            let parsed = parser.parse(name, &opsin::ParseOptions::strict());
            assert_eq!(parsed.status, opsin::Status::Success, "{name}");
            assert!(parsed.warnings.is_empty(), "{name}: {:?}", parsed.warnings);
            graphs.push(
                parsed
                    .structure
                    .ok_or("Missing parsed first-order locant control graph")?
                    .semantic_cxsmiles()
                    .map_err(|error| error.to_string())?,
            );
        }
        crate::naming::verify_identity(&graphs[0], &graphs[1])?;
        Ok(())
    }
}
