//! Component pi budgets and completed-system intrinsic hydrogen witnesses.
use super::super::{
    locants::retained_locant_sort_key as locant_key,
    molecule::{AtomSet, Molecule},
    retained_graph_model::RetainedGraphAtomTemplate,
};
use super::{
    charges::{fusion_charge_lone_pair_sites, protonated_pi_nitrogen_atoms},
    exocyclic::{is_neutral_external_pi_ligand, neutral_lambda_oxo_bonding_number},
    mancude::*,
    model::*,
    numbering::{parent_bond_model, parent_pi_capable_atom_ids},
};
use std::collections::{BTreeMap, BTreeSet};
fn pi_class(class: &str) -> bool {
    matches!(class, "aromatic" | "mancude" | "fusion")
}
fn paired(assignment: &BondAssignment) -> AtomSet {
    assignment
        .orders
        .iter()
        .filter(|(_, o)| *o == 2)
        .flat_map(|(e, _)| [e.0, e.1])
        .collect()
}
fn all_single(mol: &Molecule, id: usize) -> bool {
    mol.get_neighbors(id)
        .iter()
        .all(|&n| mol.get_bond(id, n).is_some_and(|b| b.order == 1))
}
fn parent_degree(mol: &Molecule, atoms: &AtomSet, id: usize) -> usize {
    mol.get_neighbors(id)
        .iter()
        .filter(|a| atoms.contains(a))
        .count()
}
pub fn component_graph(
    spec: &FusionComponentSpec,
    atoms: &[RetainedGraphAtomTemplate],
) -> FusionGraph {
    let ids: BTreeMap<_, _> = atoms
        .iter()
        .enumerate()
        .map(|(i, a)| (a.locant.as_str(), i))
        .collect();
    FusionGraph {
        atoms: atoms
            .iter()
            .enumerate()
            .map(|(id, a)| FusionGraphAtom {
                id,
                symbol: a.symbol.clone(),
                formal_charge: a.charge,
                pi_capacity: a.resolved_pi_capacity(),
                forced_single: a.forced_single,
                indicated_h_site: false,
                saturated: a.saturated,
            })
            .collect(),
        bonds: spec
            .bonds()
            .iter()
            .map(|b| FusionGraphBond {
                atoms: (ids[b.locants.0.as_str()], ids[b.locants.1.as_str()]),
                bond_class: b.bond_class.clone(),
            })
            .collect(),
    }
}
pub fn component_parent_atoms(
    spec: &FusionComponentSpec,
) -> Result<Vec<RetainedGraphAtomTemplate>, String> {
    let atoms = spec.atoms();
    let Some(declared) = spec.template.mancude_double_bonds else {
        return Ok(atoms.to_vec());
    };
    let movable: BTreeSet<_> = atoms
        .iter()
        .filter(|a| {
            spec.template.default_indicated_h.contains(&a.locant)
                && a.symbol == "C"
                && a.charge == 0
                && a.saturated
                && !a.forced_single
                && a.pi_capacity != Some(0)
        })
        .map(|a| a.locant.clone())
        .collect();
    if movable.is_empty() || spec.bonds().iter().any(|b| !pi_class(&b.bond_class)) {
        return Ok(atoms.to_vec());
    }
    let relaxed: Vec<_> = atoms
        .iter()
        .map(|a| {
            let mut a = a.clone();
            if movable.contains(&a.locant) {
                a.saturated = false;
                a.default_h = false;
            }
            a
        })
        .collect();
    let original = parent_bond_model(&component_graph(spec, atoms))?;
    let model = parent_bond_model(&component_graph(spec, &relaxed))?;
    if original.maximum_non_cumulative_double_bonds != declared
        || model.maximum_non_cumulative_double_bonds != declared
    {
        return Ok(atoms.to_vec());
    }
    let ids: AtomSet = atoms
        .iter()
        .enumerate()
        .filter(|(_, a)| movable.contains(&a.locant))
        .map(|(i, _)| i)
        .collect();
    if !model.allowed_kekule_assignments.iter().any(|a| {
        a.orders
            .iter()
            .all(|(e, o)| *o == 1 || (!ids.contains(&e.0) && !ids.contains(&e.1)))
    }) {
        return Ok(atoms.to_vec());
    }
    Ok(relaxed)
}
pub fn component_h_locants(
    spec: &FusionComponentSpec,
    symbol: &str,
) -> Result<BTreeSet<String>, String> {
    let atoms = component_parent_atoms(spec)?;
    let graph = component_graph(spec, &atoms);
    let model = parent_bond_model(&graph)?;
    if spec
        .template
        .mancude_double_bonds
        .is_some_and(|n| n != model.maximum_non_cumulative_double_bonds)
    {
        return Ok(BTreeSet::new());
    }
    let mut candidates = BTreeSet::new();
    for assignment in &model.allowed_kekule_assignments {
        let occupied = paired(assignment);
        for a in &graph.atoms {
            if a.symbol == symbol
                && a.pi_capacity != 0
                && !a.forced_single
                && !a.saturated
                && !occupied.contains(&a.id)
            {
                candidates.insert(atoms[a.id].locant.clone());
            }
        }
    }
    Ok(candidates)
}
fn component_carbon_pi_locants(spec: &FusionComponentSpec) -> Result<BTreeSet<String>, String> {
    let atoms = component_parent_atoms(spec)?;
    let model = parent_bond_model(&component_graph(spec, &atoms))?;
    if spec
        .template
        .mancude_double_bonds
        .is_some_and(|n| n != model.maximum_non_cumulative_double_bonds)
    {
        return Ok(BTreeSet::new());
    }
    Ok(atoms
        .into_iter()
        .filter(|a| {
            a.symbol == "C"
                && a.charge == 0
                && a.resolved_pi_capacity() != 0
                && !a.saturated
                && !a.forced_single
        })
        .map(|a| a.locant)
        .collect())
}
fn has_component_pi_budget(spec: &FusionComponentSpec) -> bool {
    spec.template.mancude_double_bonds.is_some()
        || (spec.atoms().iter().all(|a| {
            a.symbol == "C"
                && a.charge == 0
                && a.resolved_pi_capacity() == 1
                && !a.saturated
                && !a.forced_single
        }) && spec.bonds().iter().all(|b| pi_class(&b.bond_class)))
}
pub fn intrinsic_carbon_fusion_scope(
    ast: &FusionNameAst,
    specs: &BTreeMap<usize, FusionComponentSpec>,
) -> bool {
    let occurrences: AtomSet = ast
        .component_occurrences
        .iter()
        .map(|m| m.occurrence_id)
        .collect();
    if occurrences.len() < 2
        || occurrences != specs.keys().copied().collect()
        || !specs.values().all(has_component_pi_budget)
    {
        return false;
    }
    let mut neighbors: BTreeMap<_, AtomSet> =
        occurrences.iter().map(|&o| (o, AtomSet::new())).collect();
    for join in &ast.joins {
        let h = join.host_occurrence();
        let a = join.attached_occurrence();
        if !neighbors.contains_key(&h) || !neighbors.contains_key(&a) {
            return false;
        }
        neighbors.entry(h).or_default().insert(a);
        neighbors.entry(a).or_default().insert(h);
    }
    let mut seen = AtomSet::new();
    let mut pending = occurrences.first().copied().into_iter().collect::<Vec<_>>();
    while let Some(o) = pending.pop() {
        if seen.insert(o) {
            pending.extend(neighbors[&o].difference(&seen).copied());
        }
    }
    seen == occurrences
}
pub fn component_carbon_h_relocation_scope(
    ast: &FusionNameAst,
    specs: &BTreeMap<usize, FusionComponentSpec>,
) -> bool {
    intrinsic_carbon_fusion_scope(ast, specs)
        || specs
            .values()
            .all(|s| s.atoms().iter().all(|a| a.symbol == "C" && a.charge == 0))
}
pub fn component_parent_graph(
    ast: &FusionNameAst,
    specs: &BTreeMap<usize, FusionComponentSpec>,
    relocate_carbon_h: bool,
) -> Result<FusionGraph, String> {
    let mut labels: BTreeMap<usize, FusionGraphAtom> = BTreeMap::new();
    let mut edges: BTreeMap<Edge, String> = BTreeMap::new();
    for m in &ast.component_occurrences {
        let spec = &specs[&m.occurrence_id];
        let local = m.input_atom_by_locant();
        let atoms = if relocate_carbon_h {
            component_parent_atoms(spec)?
        } else {
            spec.atoms().to_vec()
        };
        for a in atoms {
            let id = local[&a.locant];
            let mut site = FusionGraphAtom {
                id,
                symbol: a.symbol.clone(),
                formal_charge: a.charge,
                pi_capacity: a.resolved_pi_capacity(),
                forced_single: a.forced_single,
                indicated_h_site: a.indicated_h_site || a.default_h,
                saturated: a.saturated,
            };
            if let Some(previous) = labels.get(&id) {
                if previous.symbol != site.symbol || previous.formal_charge != site.formal_charge {
                    return Err("component atom identities disagree at a shared fusion site".into());
                }
                site.pi_capacity = site.pi_capacity.min(previous.pi_capacity);
                site.forced_single |= previous.forced_single;
                site.indicated_h_site |= previous.indicated_h_site;
                site.saturated |= previous.saturated;
            }
            labels.insert(id, site);
        }
        for b in spec.bonds() {
            let edge = normalize_edge(local[&b.locants.0], local[&b.locants.1]);
            let class = if let Some(previous) = edges.get(&edge) {
                if previous == &b.bond_class {
                    previous.clone()
                } else if pi_class(previous) && pi_class(&b.bond_class) {
                    "mancude".into()
                } else {
                    return Err("component bond classes disagree on a shared fusion edge".into());
                }
            } else {
                b.bond_class.clone()
            };
            edges.insert(edge, class);
        }
    }
    Ok(FusionGraph {
        atoms: labels.into_values().collect(),
        bonds: edges
            .into_iter()
            .map(|(atoms, bond_class)| FusionGraphBond { atoms, bond_class })
            .collect(),
    })
}
pub fn intrinsic_carbon_candidate_atoms(
    ast: &FusionNameAst,
    specs: &BTreeMap<usize, FusionComponentSpec>,
    mol: &Molecule,
) -> Result<AtomSet, String> {
    let parent_atoms: AtomSet = ast
        .component_occurrences
        .iter()
        .flat_map(|m| m.local_to_input_atom.iter().map(|(_, a)| *a))
        .collect();
    let mut blocked = AtomSet::new();
    let mut movable = AtomSet::new();
    for m in &ast.component_occurrences {
        let spec = &specs[&m.occurrence_id];
        let relaxed: BTreeMap<_, _> = component_parent_atoms(spec)?
            .into_iter()
            .map(|a| (a.locant.clone(), a))
            .collect();
        let local = m.input_atom_by_locant();
        for a in spec.atoms() {
            let id = local[&a.locant];
            let role = &relaxed[&a.locant];
            if role.resolved_pi_capacity() == 0 || role.saturated || role.forced_single {
                blocked.insert(id);
            } else if a.saturated && !role.saturated {
                movable.insert(id);
            }
        }
    }
    let available: AtomSet = movable.difference(&blocked).copied().collect();
    let mut junctions: AtomSet = available
        .iter()
        .copied()
        .filter(|&a| is_pi_bearing_carbon_junction(mol, a, &parent_atoms))
        .collect();
    junctions.extend(hydrogenation_junctions(mol, &parent_atoms, &available));
    if !intrinsic_carbon_fusion_scope(ast, specs) {
        return Ok(junctions);
    }
    if !component_carbon_h_relocation_scope(ast, specs) {
        if junctions.is_empty() {
            return Ok(junctions);
        }
        if parent_atoms.iter().any(|&a| {
            mol.get_neighbors(a).iter().any(|&n| {
                !parent_atoms.contains(&n) && mol.get_bond(a, n).is_some_and(|b| b.order > 1)
            })
        }) {
            let mut relocated = AtomSet::new();
            for m in &ast.component_occurrences {
                let local = m.input_atom_by_locant();
                if local.values().any(|a| junctions.contains(a)) {
                    for l in component_h_locants(&specs[&m.occurrence_id], "C")? {
                        relocated.insert(local[&l]);
                    }
                }
            }
            junctions.extend(relocated.difference(&blocked).copied());
            return Ok(junctions);
        }
    }
    if !parent_atoms
        .iter()
        .any(|&a| is_intrinsic_carbon_h_site(mol, a, &parent_atoms))
    {
        return Ok(junctions);
    }
    let mut candidates = AtomSet::new();
    for m in &ast.component_occurrences {
        let local = m.input_atom_by_locant();
        for l in component_carbon_pi_locants(&specs[&m.occurrence_id])? {
            candidates.insert(local[&l]);
        }
    }
    junctions.extend(candidates.difference(&blocked).copied());
    Ok(junctions)
}
pub fn is_intrinsic_carbon_h_site(mol: &Molecule, id: usize, atoms: &AtomSet) -> bool {
    let a = &mol.atoms[&id];
    if a.symbol != "C" || a.charge != 0 || a.is_aromatic {
        return false;
    }
    let ns = mol.get_neighbors(id);
    let external = ns.iter().filter(|n| !atoms.contains(n)).count();
    let degree = ns.len() - external;
    matches!(degree, 2 | 3) && a.total_h_count + external == 4 - degree && all_single(mol, id)
}
fn is_pi_bearing_carbon_junction(mol: &Molecule, id: usize, atoms: &AtomSet) -> bool {
    let a = &mol.atoms[&id];
    a.symbol == "C"
        && a.charge == 0
        && parent_degree(mol, atoms, id) == 3
        && (a.is_aromatic
            || mol
                .get_neighbors(id)
                .iter()
                .any(|&n| atoms.contains(&n) && mol.get_bond(id, n).is_some_and(|b| b.order == 2)))
}
pub fn pi_bearing_fusion_carbon_sites(
    mol: &Molecule,
    graph: &FusionGraph,
    candidates: &AtomSet,
) -> AtomSet {
    let atoms = graph.atoms.iter().map(|a| a.id).collect();
    graph
        .atoms
        .iter()
        .filter(|a| {
            candidates.contains(&a.id)
                && a.symbol == "C"
                && a.formal_charge == 0
                && a.saturated
                && !a.forced_single
                && is_pi_bearing_carbon_junction(mol, a.id, &atoms)
        })
        .map(|a| a.id)
        .collect()
}
fn hydrogenation_junctions(mol: &Molecule, atoms: &AtomSet, candidates: &AtomSet) -> AtomSet {
    if candidates.is_empty()
        || atoms.iter().any(|&a| {
            mol.atoms[&a].charge != 0
                || mol.get_neighbors(a).iter().any(|&n| {
                    !atoms.contains(&n) && mol.get_bond(a, n).is_some_and(|b| b.order > 1)
                })
        })
    {
        return AtomSet::new();
    }
    candidates
        .iter()
        .copied()
        .filter(|&id| {
            let a = &mol.atoms[&id];
            a.symbol == "C"
                && !a.is_aromatic
                && parent_degree(mol, atoms, id) == 3
                && a.total_h_count + mol.degree(id) == 4
                && all_single(mol, id)
        })
        .collect()
}
pub fn released_fusion_carbon_sites(
    mol: &Molecule,
    graph: &FusionGraph,
    candidates: &AtomSet,
) -> AtomSet {
    let movable = graph
        .atoms
        .iter()
        .filter(|a| {
            candidates.contains(&a.id) && a.saturated && !a.forced_single && a.formal_charge == 0
        })
        .map(|a| a.id)
        .collect();
    let atoms = graph.atoms.iter().map(|a| a.id).collect();
    let mut sites = pi_bearing_fusion_carbon_sites(mol, graph, candidates);
    sites.extend(hydrogenation_junctions(mol, &atoms, &movable));
    sites
}
pub fn intrinsic_parent_lone_pair_sites(mol: &Molecule, graph: &FusionGraph) -> AtomSet {
    let mut sites = aromatic_nitrogen_lone_pair_sites(mol, graph);
    sites.extend(fusion_charge_lone_pair_sites(mol, graph));
    sites
}
#[cfg(test)]
pub fn aromatic_nitrogen_hydrogen_atoms(mol: &Molecule, graph: &FusionGraph) -> AtomSet {
    aromatic_nitrogen_lone_pair_sites(mol, graph)
        .into_iter()
        .filter(|a| mol.atoms[a].total_h_count == 1)
        .collect()
}
pub fn aromatic_nitrogen_lone_pair_sites(mol: &Molecule, graph: &FusionGraph) -> AtomSet {
    let atoms: AtomSet = graph.atoms.iter().map(|a| a.id).collect();
    let mut external = Vec::new();
    for &a in &atoms {
        for &n in mol.get_neighbors(a) {
            if !atoms.contains(&n) && mol.get_bond(a, n).is_some_and(|b| b.order > 1) {
                external.push((a, n));
            }
        }
    }
    if (!external.is_empty() && atoms.iter().any(|a| mol.atoms[a].charge != 0))
        || external.iter().any(|&(a, n)| {
            !is_neutral_external_pi_ligand(mol, a, n)
                || mol.atoms[&a].total_h_count
                    + mol
                        .get_neighbors(a)
                        .iter()
                        .filter_map(|&n| mol.get_bond(a, n))
                        .map(|b| b.order)
                        .sum::<usize>()
                    != neutral_lambda_oxo_bonding_number(mol, a).unwrap_or_else(|| {
                        mol.atoms[&a].element().map_or(0, |e| e.standard_valence)
                    })
        })
    {
        return AtomSet::new();
    }
    graph
        .atoms
        .iter()
        .filter(|a| {
            let observed = &mol.atoms[&a.id];
            let degree = parent_degree(mol, &atoms, a.id);
            a.symbol == "N"
                && observed.is_aromatic
                && observed.charge == 0
                && mol.degree(a.id) + observed.total_h_count == 3
                && matches!(degree, 2 | 3)
                && (external.is_empty() || degree == 2)
                && all_single(mol, a.id)
        })
        .map(|a| a.id)
        .collect()
}
/// Choose intrinsic carbon hydrogen sites against one completed maximum matching.
pub fn intrinsic_carbon_parent_model(
    mol: &Molecule,
    graph: &FusionGraph,
    model: &ParentBondModel,
    locants: &BTreeMap<usize, String>,
    candidates: &AtomSet,
    intrinsic_h: &AtomSet,
    cited_n_h: &AtomSet,
) -> Result<(ParentBondModel, AtomSet), String> {
    let released = released_fusion_carbon_sites(mol, graph, candidates);
    let mut graph = graph.clone();
    let mut model = model.clone();
    if !released.is_empty() {
        for a in &mut graph.atoms {
            if released.contains(&a.id) {
                a.pi_capacity = 1;
                a.saturated = false;
                a.indicated_h_site = false;
            }
        }
        model = indicated_hydrogen_parent_bond_model(&graph, intrinsic_h)?;
    }
    let hydrogen_atoms: AtomSet = intrinsic_h.union(cited_n_h).copied().collect();
    let mut owned = hydrogen_atoms.clone();
    owned.extend(protonated_pi_nitrogen_atoms(mol, &graph));
    let atoms: AtomSet = locants.keys().copied().collect();
    let hydro_n_h: AtomSet = atoms
        .iter()
        .copied()
        .filter(|a| {
            mol.atoms[a].symbol != "C" && mol.atoms[a].total_h_count != 0 && !owned.contains(a)
        })
        .collect();
    if candidates.is_empty()
        || hydro_n_h.iter().any(|&a| {
            mol.atoms[&a].total_h_count != 1 || !is_added_hydrogen_nitrogen(mol, &atoms, a)
        })
    {
        return Ok((model, AtomSet::new()));
    }
    if !saturated_nitrogen_hydrogen_sites(mol, &atoms, &hydrogen_atoms).is_empty() {
        return Ok((model, AtomSet::new()));
    }
    let original = model.clone();
    model = nitrogen_composition_parent_model(mol, &atoms, &model, &hydrogen_atoms)?;
    let no_carbon_model = if !cited_n_h.is_subset(intrinsic_h) {
        original
    } else {
        model.clone()
    };
    let mut eligible: AtomSet = candidates
        .iter()
        .copied()
        .filter(|&a| is_intrinsic_carbon_h_site(mol, a, &atoms))
        .collect();
    let oxo: AtomSet = candidates
        .iter()
        .copied()
        .filter(|&a| {
            let atom = &mol.atoms[&a];
            atom.symbol == "C"
                && atom.charge == 0
                && parent_degree(mol, &atoms, a) == 2
                && mol.get_neighbors(a).iter().all(|&n| {
                    !atoms.contains(&n)
                        || mol.get_bond(a, n).is_some_and(|b| b.order == 1)
                        || (atom.is_aromatic && mol.atoms[&n].is_aromatic)
                })
                && mol
                    .get_neighbors(a)
                    .iter()
                    .any(|&n| !atoms.contains(&n) && is_neutral_external_pi_ligand(mol, a, n))
        })
        .collect();
    eligible.extend(&oxo);
    let capable = parent_pi_capable_atom_ids(&graph)?;
    let externally_owned: AtomSet =
        oxo.iter()
            .copied()
            .filter(|&a| {
                mol.get_neighbors(a).iter().any(|n| intrinsic_h.contains(n))
                    && mol.get_neighbors(a).iter().any(|n| {
                        atoms.contains(n) && capable.contains(n) && !intrinsic_h.contains(n)
                    })
            })
            .collect();
    eligible = eligible.difference(&externally_owned).copied().collect();
    let carbon: AtomSet = graph
        .atoms
        .iter()
        .filter(|a| a.symbol == "C" && capable.contains(&a.id) && !externally_owned.contains(&a.id))
        .map(|a| a.id)
        .collect();
    let mut occupied = carbon.clone();
    for &e in model
        .pi_eligible_edges
        .iter()
        .chain(&model.required_double_bonds)
    {
        if mol.get_bond(e.0, e.1).is_some_and(|b| b.order > 1) {
            occupied.extend([e.0, e.1]);
        }
    }
    let unresolved = atoms.iter().any(|&a| {
        let atom = &mol.atoms[&a];
        atom.symbol == "N"
            && atom.charge == 0
            && !atom.is_aromatic
            && atom.total_h_count == 0
            && mol.degree(a) == 3
            && parent_degree(mol, &atoms, a) == 2
            && all_single(mol, a)
            && model.pi_eligible_edges.iter().any(|e| e.0 == a || e.1 == a)
    });
    let mut groups = BTreeSet::new();
    for assignment in &model.allowed_kekule_assignments {
        let p = paired(assignment);
        let unpaired: AtomSet = carbon.difference(&p).copied().collect();
        if !intrinsic_h.is_empty() && occupied.is_subset(&p) && !unresolved {
            return Ok((no_carbon_model, AtomSet::new()));
        }
        if !unpaired.is_empty() && unpaired.is_subset(&eligible) {
            groups.insert(unpaired);
        }
    }
    let mut choices = Vec::new();
    for sites in groups {
        let constrained = single_site_parent_model(&model, &sites)?;
        if constrained.maximum_non_cumulative_double_bonds
            != model.maximum_non_cumulative_double_bonds
        {
            continue;
        }
        let Some(delta) = compare_actual_parent_to_implied_parent(
            mol,
            &atoms,
            &constrained,
            &CompareOptions {
                atom_to_locant: Some(locants),
                indicated_hydrogen_atom_ids: &hydrogen_atoms,
                externally_unsaturated_atom_ids: &oxo,
                ..Default::default()
            },
        )?
        else {
            continue;
        };
        if !delta.compatible {
            continue;
        }
        let redistribution = if !delta.additional_multiple_bond_ids.is_empty()
            || delta
                .hydrogenated_atom_ids()
                .iter()
                .any(|a| mol.atoms[a].is_aromatic)
        {
            let indicated: AtomSet = hydrogen_atoms.union(&sites).copied().collect();
            prove_pi_redistribution(mol, &atoms, &constrained, &delta, &indicated, &[])?
        } else {
            None
        };
        if !delta.additional_multiple_bond_ids.is_empty() && redistribution.is_none() {
            continue;
        }
        if delta
            .assignment
            .orders
            .iter()
            .filter(|(_, o)| *o == 2)
            .count()
            != model.maximum_non_cumulative_double_bonds
        {
            continue;
        }
        let p = paired(&delta.assignment);
        if carbon.difference(&p).copied().collect::<AtomSet>() != sites
            || !occupied.difference(&sites).all(|a| p.contains(a))
        {
            continue;
        }
        let hydro = redistribution.as_ref().map_or_else(
            || delta.hydrogenated_atom_ids(),
            |r| r.hydrogenated_atom_ids.clone(),
        );
        if !hydro_n_h.is_subset(&hydro)
            || hydro.iter().any(|a| mol.atoms[a].is_aromatic)
            || (!sites.is_disjoint(&oxo) && delta.intrinsic_hydro_operations.is_empty())
        {
            continue;
        }
        let mut keys: Vec<_> = sites.iter().map(|a| locant_key(&locants[a])).collect();
        keys.sort();
        let rank = (delta.hydrogenated_edges.len(), keys);
        choices.push((
            rank,
            constrained,
            if delta.intrinsic_hydro_operations.is_empty() {
                sites
            } else {
                AtomSet::new()
            },
        ));
    }
    Ok(choices
        .into_iter()
        .min_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, m, s)| (m, s))
        .unwrap_or((no_carbon_model, AtomSet::new())))
}

#[cfg(test)]
mod tests {
    use super::super::super::molecule::{Atom, Bond};
    use super::*;

    #[test]
    fn aromatic_donor_hydrogen_and_sigma_substitution_share_the_lone_pair_role() {
        let mut mol = super::super::super::graph_io::read_smiles("c1cc[nH]c1").unwrap();
        let nitrogen = *mol.atoms.iter().find(|(_, a)| a.symbol == "N").unwrap().0;
        let graph = FusionGraph {
            atoms: mol
                .atoms
                .values()
                .map(|a| FusionGraphAtom::new(a.idx, &a.symbol))
                .collect(),
            bonds: Vec::new(),
        };
        assert_eq!(
            aromatic_nitrogen_hydrogen_atoms(&mol, &graph),
            AtomSet::from([nitrogen])
        );
        mol.atoms.get_mut(&nitrogen).unwrap().total_h_count = 0;
        let mut methyl = Atom::new(5, "C");
        methyl.total_h_count = 3;
        mol.add_atom(methyl).unwrap();
        mol.add_bond(Bond::new(99, nitrogen, 5, 1)).unwrap();
        assert!(aromatic_nitrogen_hydrogen_atoms(&mol, &graph).is_empty());
        assert_eq!(
            aromatic_nitrogen_lone_pair_sites(&mol, &graph),
            AtomSet::from([nitrogen])
        );
    }

    fn carbon_h_domains() -> (Molecule, FusionGraph, BTreeMap<usize, String>) {
        let mut edges = Vec::new();
        for offset in [0, 5] {
            for id in 0..5 {
                edges.push(normalize_edge(offset + id, offset + (id + 1) % 5));
            }
        }
        edges.push((2, 7));
        let doubles = EdgeSet::from([(1, 2), (3, 4), (6, 7), (8, 9)]);
        let mut mol = Molecule::new();
        for id in 0..10 {
            let mut a = Atom::new(id, "C");
            a.total_h_count = if [0, 5].contains(&id) {
                2
            } else if [2, 7].contains(&id) {
                0
            } else {
                1
            };
            mol.add_atom(a).unwrap();
        }
        for (id, &(u, v)) in edges.iter().enumerate() {
            mol.add_bond(Bond::new(
                id,
                u,
                v,
                if doubles.contains(&(u, v)) { 2 } else { 1 },
            ))
            .unwrap();
        }
        let graph = FusionGraph {
            atoms: (0..10).map(|id| FusionGraphAtom::new(id, "C")).collect(),
            bonds: edges
                .into_iter()
                .map(|e| FusionGraphBond {
                    atoms: e,
                    bond_class: if e == (2, 7) { "single" } else { "mancude" }.into(),
                })
                .collect(),
        };
        (
            mol,
            graph,
            (0..10).map(|a| (a, (a + 1).to_string())).collect(),
        )
    }

    #[test]
    fn intrinsic_carbon_sites_preserve_the_joint_parent_pi_maximum() {
        let (mol, graph, locants) = carbon_h_domains();
        let original = parent_bond_model(&graph).unwrap();
        let (model, sites) = intrinsic_carbon_parent_model(
            &mol,
            &graph,
            &original,
            &locants,
            &AtomSet::from([0, 5]),
            &AtomSet::new(),
            &AtomSet::new(),
        )
        .unwrap();
        assert_eq!(sites, AtomSet::from([0, 5]));
        assert_eq!(model.maximum_non_cumulative_double_bonds, 4);
        let atoms = locants.keys().copied().collect();
        let state = parent_derivative_state(&mol, &atoms, &model, &locants, &Default::default())
            .unwrap()
            .unwrap();
        assert!(state.hydro_operations.is_empty());
        assert!(state.unsaturation_operations.is_empty());
        let (declined, sites) = intrinsic_carbon_parent_model(
            &mol,
            &graph,
            &original,
            &locants,
            &AtomSet::from([0]),
            &AtomSet::new(),
            &AtomSet::new(),
        )
        .unwrap();
        assert!(sites.is_empty());
        assert_eq!(declined, original);
    }

    #[test]
    fn oxo_composes_intrinsic_hydrogen_without_spending_a_parent_pi_pair() {
        let (mut mol, graph, locants) = carbon_h_domains();
        mol.atoms.get_mut(&0).unwrap().total_h_count = 0;
        mol.add_atom(Atom::new(10, "O")).unwrap();
        mol.add_bond(Bond::new(11, 0, 10, 2)).unwrap();
        let original = parent_bond_model(&graph).unwrap();
        let (model, sites) = intrinsic_carbon_parent_model(
            &mol,
            &graph,
            &original,
            &locants,
            &AtomSet::from([0, 5]),
            &AtomSet::new(),
            &AtomSet::new(),
        )
        .unwrap();
        assert!(sites.is_empty());
        assert_eq!(model.maximum_non_cumulative_double_bonds, 4);
        let state = parent_derivative_state(
            &mol,
            &locants.keys().copied().collect(),
            &model,
            &locants,
            &Default::default(),
        )
        .unwrap()
        .unwrap();
        assert!(state.hydro_operations.is_empty());
        assert!(state.added_hydrogen_operations().is_empty());
        assert_eq!(state.intrinsic_hydro_operations()[0].atom_ids, [0, 5]);
        assert_eq!(state.oxo_operations[0].parent_atom_id, 0);
    }
}
