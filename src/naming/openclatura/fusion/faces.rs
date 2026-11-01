//! Bounded, graph-native chordless-face proofs. No drawing coordinates are read.
use super::{
    config::fusion_nomenclature_config,
    model::{Edge, EdgeSet, Face, FaceModel, normalize_edge},
};
use crate::naming::openclatura::{
    chains::canonical_ranks,
    graph_kernel::{adjacency_from_edges, connected_components},
    molecule::{AtomSet, Molecule},
    polycycle_topology::edges_within_atoms,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct GraphCycle {
    pub atoms: Vec<usize>,
    pub edges: EdgeSet,
}
impl GraphCycle {
    pub fn from_atoms(atoms: impl IntoIterator<Item = usize>) -> Result<Self, String> {
        let atoms = canonical_cycle(&atoms.into_iter().collect::<Vec<_>>());
        if atoms.len() < 3 || atoms.iter().copied().collect::<AtomSet>().len() != atoms.len() {
            return Err("A graph cycle needs at least three distinct atoms".into());
        }
        let edges = cycle_edges(&atoms);
        Ok(Self { atoms, edges })
    }
}
pub fn canonical_cycle(atoms: &[usize]) -> Vec<usize> {
    let mut best = None;
    for rev in [false, true] {
        let seq = if rev {
            atoms.iter().rev().copied().collect::<Vec<_>>()
        } else {
            atoms.to_vec()
        };
        for offset in 0..seq.len() {
            let mut value = seq.clone();
            value.rotate_left(offset);
            if best.as_ref().is_none_or(|b| &value < b) {
                best = Some(value)
            }
        }
    }
    best.unwrap_or_default()
}
pub fn cycle_edges(atoms: &[usize]) -> EdgeSet {
    cycle_edge_iter(atoms).collect()
}
pub(super) fn cycle_edge_iter(cycle: &[usize]) -> impl Iterator<Item = Edge> + '_ {
    cycle
        .iter()
        .copied()
        .zip(cycle.iter().copied().cycle().skip(1))
        .take(cycle.len())
        .map(|(u, v)| normalize_edge(u, v))
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FaceModelAudit {
    pub ok: bool,
    pub errors: Vec<String>,
    pub cycle_rank: usize,
    pub edge_multiplicity: Vec<(Edge, usize)>,
    pub dual_adjacency: Vec<(usize, Vec<usize>)>,
    pub outer_boundary: Vec<usize>,
    pub reconstructed_edges: EdgeSet,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundedFaceModel {
    pub atom_ids: AtomSet,
    pub edge_ids: EdgeSet,
    pub faces: Vec<GraphCycle>,
    pub outer_boundary: GraphCycle,
    pub cycle_rank: usize,
    pub audit: FaceModelAudit,
}
impl BoundedFaceModel {
    pub fn peripheral_atoms(&self) -> AtomSet {
        self.outer_boundary.atoms.iter().copied().collect()
    }
    pub fn interior_atoms(&self) -> AtomSet {
        self.atom_ids
            .difference(&self.peripheral_atoms())
            .copied()
            .collect()
    }
    pub fn face_membership_by_atom(&self) -> Vec<(usize, Vec<usize>)> {
        self.atom_ids
            .iter()
            .map(|&a| {
                (
                    a,
                    self.faces
                        .iter()
                        .enumerate()
                        .filter(|(_, f)| f.atoms.contains(&a))
                        .map(|(i, _)| i)
                        .collect(),
                )
            })
            .collect()
    }
    pub fn fusion_atoms(&self) -> AtomSet {
        self.face_membership_by_atom()
            .into_iter()
            .filter(|(_, v)| v.len() > 1)
            .map(|(a, _)| a)
            .collect()
    }
}
#[derive(Debug)]
struct Budget {
    phase: &'static str,
    limit: usize,
    used: usize,
}
impl Budget {
    fn new(phase: &'static str, limit: usize) -> Self {
        Self {
            phase,
            limit,
            used: 0,
        }
    }
    fn spend(&mut self) -> Result<(), String> {
        self.used += 1;
        if self.used > self.limit {
            Err(format!(
                "{} search exceeded its budget of {} states",
                self.phase, self.limit
            ))
        } else {
            Ok(())
        }
    }
}
// Every cycle edge was proved to be a molecular edge by the bounded-face audit.
#[allow(
    clippy::expect_used,
    reason = "bounded-face proofs contain only molecular edges"
)]
pub fn typed_face_model(mol: &Molecule, bounded: &BoundedFaceModel) -> FaceModel {
    let mut owners: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    let faces = bounded
        .faces
        .iter()
        .enumerate()
        .map(|(id, cycle)| {
            let edge_cycle = (0..cycle.atoms.len())
                .map(|i| {
                    mol.get_bond(cycle.atoms[i], cycle.atoms[(i + 1) % cycle.atoms.len()])
                        .expect("audited graph edge")
                        .idx
                })
                .collect::<Vec<_>>();
            for &edge in &edge_cycle {
                owners.entry(edge).or_default().push(id)
            }
            Face {
                id,
                atom_cycle: cycle.atoms.clone(),
                edge_cycle,
                size: cycle.atoms.len(),
            }
        })
        .collect();
    let mut face_adjacency = owners
        .iter()
        .filter(|(_, owners)| owners.len() == 2)
        .map(|(&edge, owners)| (owners[0], owners[1], edge))
        .collect::<Vec<_>>();
    face_adjacency.sort();
    FaceModel {
        faces,
        edge_to_faces: owners.iter().map(|(&e, v)| (e, v.clone())).collect(),
        perimeter_edges: owners
            .iter()
            .filter(|(_, v)| v.len() == 1)
            .map(|(&e, _)| e)
            .collect(),
        fusion_edges: owners
            .iter()
            .filter(|(_, v)| v.len() == 2)
            .map(|(&e, _)| e)
            .collect(),
        outer_boundary: bounded.outer_boundary.atoms.clone(),
        face_adjacency,
    }
}
pub fn enumerate_chordless_cycles_with_limits(
    mol: &Molecule,
    atoms: &AtomSet,
    min_size: usize,
    max_size: usize,
    search_budget: usize,
) -> Result<Vec<GraphCycle>, String> {
    let c = &fusion_nomenclature_config().search;
    if min_size < c.minimum_ring_size
        || max_size > c.maximum_ring_size
        || min_size > max_size
        || search_budget == 0
    {
        return Err("invalid cycle sizes or search budget".into());
    }
    if atoms.iter().any(|a| !mol.atoms.contains_key(a)) {
        return Err("Unknown atom ids".into());
    }
    let edges = edges_within_atoms(mol, atoms);
    let adjacency = adjacency_from_edges(atoms, &edges);
    let mut budget = Budget::new("cycle enumeration", search_budget);
    let mut found = BTreeMap::new();
    for &start in atoms {
        enumerate_from_start(
            start,
            &adjacency,
            min_size,
            max_size,
            &mut budget,
            &mut found,
            None,
        )?
    }
    let mut cycles = found.into_values().collect::<Vec<_>>();
    cycles.sort_by_key(|c| (c.atoms.len(), c.atoms.clone()));
    Ok(cycles)
}
fn enumerate_from_start(
    start: usize,
    adj: &BTreeMap<usize, AtomSet>,
    min: usize,
    max: usize,
    budget: &mut Budget,
    found: &mut BTreeMap<Vec<usize>, GraphCycle>,
    seed: Option<usize>,
) -> Result<(), String> {
    #[allow(
        clippy::too_many_arguments,
        reason = "DFS keeps the immutable graph and bounded mutable frontier explicit"
    )]
    fn visit(
        start: usize,
        adj: &BTreeMap<usize, AtomSet>,
        min: usize,
        max: usize,
        b: &mut Budget,
        found: &mut BTreeMap<Vec<usize>, GraphCycle>,
        seed: bool,
        path: &mut Vec<usize>,
        seen: &mut AtomSet,
    ) -> Result<(), String> {
        b.spend()?;
        let current = path[path.len() - 1];
        for &neighbor in &adj[&current] {
            if neighbor == start {
                if path.len() >= min {
                    let c = GraphCycle::from_atoms(path.iter().copied())?;
                    let induced = edges_from_adjacency(&c.atoms.iter().copied().collect(), adj);
                    if c.edges == induced {
                        found.insert(c.atoms.clone(), c);
                    }
                }
                continue;
            }
            if (!seed && neighbor < start) || seen.contains(&neighbor) || path.len() >= max {
                continue;
            }
            if seed
                && adj[&neighbor]
                    .iter()
                    .any(|a| *a != current && *a != start && seen.contains(a))
            {
                continue;
            }
            path.push(neighbor);
            seen.insert(neighbor);
            visit(start, adj, min, max, b, found, seed, path, seen)?;
            path.pop();
            seen.remove(&neighbor);
        }
        Ok(())
    }
    let mut path = vec![start];
    if let Some(n) = seed {
        path.push(n)
    }
    let mut seen = path.iter().copied().collect();
    visit(
        start,
        adj,
        min,
        max,
        budget,
        found,
        seed.is_some(),
        &mut path,
        &mut seen,
    )
}
fn edges_from_adjacency(atoms: &AtomSet, adj: &BTreeMap<usize, AtomSet>) -> EdgeSet {
    atoms
        .iter()
        .flat_map(|&a| {
            adj[&a]
                .iter()
                .filter(move |&&b| atoms.contains(&b) && a < b)
                .map(move |&b| (a, b))
        })
        .collect()
}
pub fn audit_bounded_face_model(
    mol: &Molecule,
    atoms: &AtomSet,
    faces: &[GraphCycle],
) -> FaceModelAudit {
    let graph_edges = edges_within_atoms(mol, atoms);
    let mut errors = Vec::new();
    let components = connected_components(atoms, &graph_edges).len();
    let rank = (graph_edges.len() + components).saturating_sub(atoms.len());
    if atoms.is_empty() {
        errors.push("ring graph is empty".into())
    } else if components != 1 {
        errors.push("ring graph is disconnected".into())
    }
    if faces.len() != rank {
        errors.push(format!(
            "bounded face count {} does not equal cycle rank {}",
            faces.len(),
            rank
        ))
    }
    let mut mult: BTreeMap<Edge, usize> = BTreeMap::new();
    for (i, f) in faces.iter().enumerate() {
        if f.atoms.iter().any(|a| !atoms.contains(a)) {
            errors.push(format!("face {i} contains atoms outside the ring graph"))
        }
        if !f.edges.is_subset(&graph_edges) {
            errors.push(format!("face {i} contains edges outside the ring graph"))
        }
        let f_atoms = f.atoms.iter().copied().collect::<AtomSet>();
        if graph_edges
            .iter()
            .filter(|(a, b)| f_atoms.contains(a) && f_atoms.contains(b))
            .copied()
            .collect::<EdgeSet>()
            != f.edges
        {
            errors.push(format!("face {i} is not chordless"))
        }
        for &e in &f.edges {
            *mult.entry(e).or_default() += 1
        }
    }
    let reconstructed = mult.keys().copied().collect::<EdgeSet>();
    if reconstructed != graph_edges {
        errors.push("bounded faces do not exactly reconstruct the ring-graph edges".into())
    }
    if mult.values().any(|&n| n > 2) {
        errors.push("a ring-graph edge belongs to more than two bounded faces".into())
    }
    let dual = (0..faces.len())
        .map(|i| {
            (
                i,
                (0..faces.len())
                    .filter(|&j| i != j && !faces[i].edges.is_disjoint(&faces[j].edges))
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();
    if !faces.is_empty() {
        let mut seen = AtomSet::new();
        let mut stack = vec![0];
        while let Some(n) = stack.pop() {
            if seen.insert(n) {
                stack.extend(&dual[n].1)
            }
        }
        if seen.len() != faces.len() {
            errors.push("bounded-face dual graph is disconnected".into())
        }
    }
    let boundary = mult
        .iter()
        .filter(|(_, n)| **n % 2 == 1)
        .map(|(&e, _)| e)
        .collect();
    let outer = simple_cycle_order(&boundary);
    if outer.is_none() {
        errors.push("XOR outer boundary is not a simple cycle".into())
    }
    FaceModelAudit {
        ok: errors.is_empty(),
        errors,
        cycle_rank: rank,
        edge_multiplicity: mult.into_iter().collect(),
        dual_adjacency: dual,
        outer_boundary: outer.unwrap_or_default(),
        reconstructed_edges: reconstructed,
    }
}
pub fn simple_cycle_order(edges: &EdgeSet) -> Option<Vec<usize>> {
    if edges.len() < 3 {
        return None;
    }
    let atoms = edges.iter().flat_map(|&(a, b)| [a, b]).collect::<AtomSet>();
    let adj = adjacency_from_edges(&atoms, edges);
    if adj.values().any(|n| n.len() != 2) || connected_components(&atoms, edges).len() != 1 {
        return None;
    }
    let start = *atoms.first()?;
    let mut order = vec![start];
    let mut previous = start;
    let mut current = *adj[&start].first()?;
    while current != start {
        if order.contains(&current) {
            return None;
        }
        order.push(current);
        let next = *adj[&current].iter().find(|&&a| a != previous)?;
        previous = current;
        current = next;
    }
    (order.len() == atoms.len()).then(|| canonical_cycle(&order))
}
#[derive(Clone, PartialEq, Eq)]
struct FaceCacheKey {
    atoms: AtomSet,
    molecule_atoms: Vec<(usize, String, i32, Option<u16>, bool, usize)>,
    bonds: Vec<(usize, usize, usize, usize)>,
}
thread_local! {
    static FACE_CACHE: super::ThreadLocalCache<FaceCacheKey, Option<BoundedFaceModel>> = const { super::ThreadLocalCache::new() };
}
pub fn cached_bounded_face_model(
    mol: &Molecule,
    atoms: &AtomSet,
) -> Result<Option<BoundedFaceModel>, String> {
    let key = FaceCacheKey {
        atoms: atoms.clone(),
        molecule_atoms: mol
            .atoms
            .values()
            .map(|a| {
                (
                    a.idx,
                    a.symbol.clone(),
                    a.charge,
                    a.isotope,
                    a.is_aromatic,
                    a.total_h_count,
                )
            })
            .collect(),
        bonds: mol
            .bonds
            .values()
            .map(|b| (b.idx, b.u, b.v, b.order))
            .collect(),
    };
    if let Some(model) = FACE_CACHE.with(|cache| cache.get(|previous| previous == &key)) {
        return Ok(model);
    }
    let model = select_bounded_face_model(mol, atoms)?;
    FACE_CACHE.with(|cache| cache.insert(key, model.clone()));
    Ok(model)
}
pub fn select_bounded_face_model(
    mol: &Molecule,
    atoms: &AtomSet,
) -> Result<Option<BoundedFaceModel>, String> {
    let c = &fusion_nomenclature_config().search;
    select_bounded_face_model_with_limits(
        mol,
        atoms,
        c.minimum_ring_size,
        None,
        c.cycle_states,
        c.face_model_states,
    )
}
pub fn select_bounded_face_model_with_limits(
    mol: &Molecule,
    atoms: &AtomSet,
    min: usize,
    max: Option<usize>,
    cycle_budget: usize,
    model_budget: usize,
) -> Result<Option<BoundedFaceModel>, String> {
    if cycle_budget == 0 || model_budget == 0 {
        return Err("face search budgets must be positive".into());
    }
    let edges = edges_within_atoms(mol, atoms);
    if atoms.is_empty() || connected_components(atoms, &edges).len() != 1 {
        return Ok(None);
    }
    let rank = (edges.len() + 1).saturating_sub(atoms.len());
    if rank < 1 {
        return Ok(None);
    }
    let config = fusion_nomenclature_config();
    if rank == 2
        && min == config.search.minimum_ring_size
        && max.is_none()
        && let Some(model) = larger_carbon_bicycle(mol, atoms, &edges)
    {
        return Ok(Some(model));
    }
    let mut cycles = enumerate_chordless_cycles_with_limits(
        mol,
        atoms,
        min,
        max.unwrap_or(config.search.maximum_ring_size),
        cycle_budget,
    )?;
    if rank > 2
        && min == config.search.minimum_ring_size
        && max.is_none()
        && let Some(&maximum_annulene_size) = config.annulene_ring_sizes.iter().max()
    {
        let covered = cycles
            .iter()
            .flat_map(|c| c.edges.iter().copied())
            .collect::<EdgeSet>();
        let missing = edges.difference(&covered).copied().collect::<EdgeSet>();
        let carbon = atoms
            .iter()
            .filter(|a| mol.atoms[a].symbol == "C" && mol.atoms[a].charge == 0)
            .copied()
            .collect::<AtomSet>();
        if !missing.is_empty()
            && missing
                .iter()
                .all(|(a, b)| carbon.contains(a) && carbon.contains(b))
        {
            let carbon_edges = edges
                .iter()
                .filter(|(a, b)| carbon.contains(a) && carbon.contains(b))
                .copied()
                .collect();
            let adj = adjacency_from_edges(&carbon, &carbon_edges);
            let mut budget = Budget::new("large carbon face completion", cycle_budget);
            let mut found = BTreeMap::new();
            for &(a, b) in &missing {
                enumerate_from_start(
                    a,
                    &adj,
                    config.search.maximum_ring_size + 1,
                    maximum_annulene_size,
                    &mut budget,
                    &mut found,
                    Some(b),
                )?
            }
            cycles.extend(found.into_values());
            cycles.sort_by_key(|c| (c.atoms.len(), c.atoms.clone()));
        }
    }
    let sets = independent_face_sets(
        &cycles,
        &edges,
        rank,
        &mut Budget::new("face-model selection", model_budget),
    )?;
    let ranks = canonical_ranks(mol, Some(atoms));
    let mut best_score = None;
    let mut best = None;
    let mut distinct = BTreeSet::new();
    for selected in sets {
        let faces = selected
            .into_iter()
            .map(|i| cycles[i].clone())
            .collect::<Vec<_>>();
        let audit = audit_bounded_face_model(mol, atoms, &faces);
        if !audit.ok {
            continue;
        }
        let mut signatures = faces
            .iter()
            .map(|f| canonical_cycle(&f.atoms.iter().map(|a| ranks[a]).collect::<Vec<_>>()))
            .collect::<Vec<_>>();
        signatures.sort();
        let score = (
            faces.iter().map(|f| f.atoms.len()).sum::<usize>(),
            signatures,
        );
        if best_score.as_ref().is_some_and(|b| &score > b) {
            continue;
        }
        if best_score.as_ref().is_none_or(|b| &score < b) {
            best_score = Some(score);
            best = None;
            distinct.clear()
        }
        distinct.insert(
            faces
                .iter()
                .map(|f| f.edges.clone())
                .collect::<BTreeSet<_>>(),
        );
        if best.is_none() {
            let outer_boundary = GraphCycle::from_atoms(audit.outer_boundary.iter().copied())?;
            best = Some(BoundedFaceModel {
                atom_ids: atoms.clone(),
                edge_ids: edges.clone(),
                faces,
                outer_boundary,
                cycle_rank: rank,
                audit,
            })
        }
    }
    Ok(if distinct.len() == 1 { best } else { None })
}
fn larger_carbon_bicycle(
    mol: &Molecule,
    atoms: &AtomSet,
    edges: &EdgeSet,
) -> Option<BoundedFaceModel> {
    let c = fusion_nomenclature_config();
    if c.annulene_ring_sizes.is_empty()
        || atoms
            .iter()
            .any(|a| mol.atoms[a].symbol != "C" || mol.atoms[a].charge != 0)
    {
        return None;
    }
    let adj = adjacency_from_edges(atoms, edges);
    let junctions = adj
        .iter()
        .filter(|(_, v)| v.len() == 3)
        .map(|(&a, _)| a)
        .collect::<Vec<_>>();
    if junctions.len() != 2 || adj.values().any(|v| v.len() != 2 && v.len() != 3) {
        return None;
    }
    let (start, end) = (junctions[0], junctions[1]);
    if !adj[&start].contains(&end) {
        return None;
    }
    let mut paths = Vec::new();
    let mut seen = AtomSet::from([start, end]);
    for &first in &adj[&start] {
        if first == end {
            continue;
        }
        let mut path = vec![start];
        let (mut prev, mut cur) = (start, first);
        while cur != end {
            if !seen.insert(cur) {
                return None;
            }
            path.push(cur);
            let following = adj[&cur]
                .iter()
                .filter(|&&a| a != prev)
                .copied()
                .collect::<Vec<_>>();
            if following.len() != 1 {
                return None;
            }
            prev = cur;
            cur = following[0]
        }
        path.push(end);
        paths.push(path)
    }
    if seen != *atoms || paths.len() != 2 {
        return None;
    }
    let sizes = paths.iter().map(Vec::len).collect::<Vec<_>>();
    if !c.annulene_ring_sizes.contains(sizes.iter().max()?)
        || *sizes.iter().max()? <= c.search.maximum_ring_size
        || *sizes.iter().min()? < c.rules.minimum_ring_size
    {
        return None;
    }
    let mut faces = paths
        .into_iter()
        .map(GraphCycle::from_atoms)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    faces.sort_by_key(|f| (f.atoms.len(), f.atoms.clone()));
    let audit = audit_bounded_face_model(mol, atoms, &faces);
    if !audit.ok {
        return None;
    }
    Some(BoundedFaceModel {
        atom_ids: atoms.clone(),
        edge_ids: edges.clone(),
        faces,
        outer_boundary: GraphCycle::from_atoms(audit.outer_boundary.iter().copied()).ok()?,
        cycle_rank: 2,
        audit,
    })
}
fn independent_face_sets(
    cycles: &[GraphCycle],
    edges: &EdgeSet,
    target: usize,
    budget: &mut Budget,
) -> Result<Vec<Vec<usize>>, String> {
    fn insert_basis(basis: &[EdgeSet], mut mask: EdgeSet) -> Option<Vec<EdgeSet>> {
        for row in basis {
            if mask.last() == row.last() {
                mask = mask.symmetric_difference(row).copied().collect()
            }
        }
        if mask.is_empty() {
            return None;
        }
        let mut result = basis.to_vec();
        result.push(mask);
        result.sort_by(|a, b| b.last().cmp(&a.last()));
        Some(result)
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "bounded GF(2) search keeps graph, frontier and proof accumulators separate"
    )]
    fn visit(
        cycles: &[GraphCycle],
        edges: &EdgeSet,
        target: usize,
        budget: &mut Budget,
        selected: &mut AtomSet,
        once: &EdgeSet,
        twice: &EdgeSet,
        basis: &[EdgeSet],
        visited: &mut BTreeSet<AtomSet>,
        results: &mut BTreeSet<Vec<usize>>,
    ) -> Result<(), String> {
        budget.spend()?;
        if !visited.insert(selected.clone()) {
            return Ok(());
        }
        if selected.len() == target {
            if once == edges {
                results.insert(selected.iter().copied().collect());
            }
            return Ok(());
        }
        let uncovered = edges.difference(once).copied().collect::<EdgeSet>();
        let choices = if !uncovered.is_empty() {
            #[allow(
                clippy::unwrap_used,
                reason = "the branch requires a nonempty uncovered edge set"
            )]
            let &pivot = uncovered
                .iter()
                .min_by_key(|&&e| {
                    (
                        cycles
                            .iter()
                            .enumerate()
                            .filter(|(i, c)| !selected.contains(i) && c.edges.contains(&e))
                            .count(),
                        e,
                    )
                })
                .unwrap();
            let possible = cycles
                .iter()
                .enumerate()
                .filter(|(i, _)| !selected.contains(i))
                .flat_map(|(_, c)| c.edges.iter().copied())
                .collect::<EdgeSet>();
            if !uncovered.is_subset(&possible) {
                return Ok(());
            }
            cycles
                .iter()
                .enumerate()
                .filter(|(i, c)| !selected.contains(i) && c.edges.contains(&pivot))
                .map(|(i, _)| i)
                .collect::<Vec<_>>()
        } else {
            (0..cycles.len())
                .filter(|i| !selected.contains(i))
                .collect()
        };
        for i in choices {
            let mask = &cycles[i].edges;
            if !twice.is_disjoint(mask) {
                continue;
            }
            let Some(next_basis) = insert_basis(basis, mask.clone()) else {
                continue;
            };
            let new_twice = twice
                .iter()
                .chain(once.intersection(mask))
                .copied()
                .collect();
            let new_once = once.union(mask).copied().collect::<EdgeSet>();
            if target - selected.len() == 1 && new_once != *edges {
                continue;
            }
            selected.insert(i);
            visit(
                cycles,
                edges,
                target,
                budget,
                selected,
                &new_once,
                &new_twice,
                &next_basis,
                visited,
                results,
            )?;
            selected.remove(&i);
        }
        Ok(())
    }
    if target == 0 || cycles.len() < target {
        return Ok(Vec::new());
    }
    let mut results = BTreeSet::new();
    visit(
        cycles,
        edges,
        target,
        budget,
        &mut AtomSet::new(),
        &EdgeSet::new(),
        &EdgeSet::new(),
        &[],
        &mut BTreeSet::new(),
        &mut results,
    )?;
    Ok(results.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::naming::openclatura::{
        graph_io::read_smiles,
        molecule::{Atom, Bond},
    };

    #[test]
    fn bounded_faces_reconstruct_naphthalene_and_reject_ambiguous_bridge() {
        let mol = read_smiles("c1ccc2ccccc2c1").unwrap();
        let atoms = mol.atoms.keys().copied().collect();
        let bounded = select_bounded_face_model(&mol, &atoms).unwrap().unwrap();
        assert_eq!(
            bounded
                .faces
                .iter()
                .map(|f| f.atoms.len())
                .collect::<Vec<_>>(),
            vec![6, 6]
        );
        assert!(bounded.audit.ok);
        assert_eq!(bounded.outer_boundary.atoms.len(), 10);
        assert!(bounded.interior_atoms().is_empty());
        assert_eq!(bounded.fusion_atoms().len(), 2);
        let typed = typed_face_model(&mol, &bounded);
        assert_eq!(typed.fusion_edges.len(), 1);
        assert_eq!(typed.perimeter_edges.len(), 10);

        let mut theta = Molecule::new();
        for a in 0..5 {
            theta.add_atom(Atom::new(a, "C")).unwrap()
        }
        for (i, (a, b)) in [(0, 2), (2, 1), (0, 3), (3, 1), (0, 4), (4, 1)]
            .into_iter()
            .enumerate()
        {
            theta.add_bond(Bond::new(i, a, b, 1)).unwrap()
        }
        let atoms = theta.atoms.keys().copied().collect();
        assert!(select_bounded_face_model(&theta, &atoms).unwrap().is_none());
    }

    #[test]
    fn budget_exhaustion_never_returns_partial_cycles() {
        let mol = read_smiles("c1ccc2ccccc2c1").unwrap();
        let atoms = mol.atoms.keys().copied().collect();
        assert!(
            enumerate_chordless_cycles_with_limits(&mol, &atoms, 3, 8, 1)
                .unwrap_err()
                .contains("cycle enumeration search exceeded")
        );
        assert!(
            select_bounded_face_model_with_limits(&mol, &atoms, 3, None, 10000, 1)
                .unwrap_err()
                .contains("face-model selection search exceeded")
        );
    }

    #[test]
    fn carbon_bicycle_proves_faces_larger_than_ordinary_cycle_limit() {
        let mut mol = Molecule::new();
        // A six-member ring and a ten-member ring share the edge 0--1.
        let left = vec![0, 1, 2, 3, 4, 5];
        let right = vec![0, 1, 6, 7, 8, 9, 10, 11, 12, 13];
        for a in 0..14 {
            mol.add_atom(Atom::new(a, "C")).unwrap()
        }
        let edges = cycle_edges(&left)
            .union(&cycle_edges(&right))
            .copied()
            .collect::<EdgeSet>();
        for (i, (a, b)) in edges.into_iter().enumerate() {
            mol.add_bond(Bond::new(i, a, b, 1)).unwrap()
        }
        let atoms = mol.atoms.keys().copied().collect();
        let bounded = select_bounded_face_model(&mol, &atoms).unwrap().unwrap();
        assert_eq!(
            bounded
                .faces
                .iter()
                .map(|f| f.atoms.len())
                .collect::<Vec<_>>(),
            vec![6, 10]
        );
        assert_eq!(bounded.outer_boundary.atoms.len(), 14);
    }
}
