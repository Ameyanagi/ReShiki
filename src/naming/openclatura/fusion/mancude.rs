//! Compare observed bonding with an exhaustive parent-hydride bond model.
use super::super::{
    assembly_parts::HydroOperation,
    locants::retained_locant_sort_key as locant_key,
    molecule::{AtomSet, Molecule},
};
use super::{
    exocyclic::{is_neutral_external_pi_ligand, neutral_lambda_oxo_bonding_number},
    model::*,
    numbering::parent_bond_model,
};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParentBondDelta {
    pub assignment: BondAssignment,
    // The requested shared delta retains parent-pi identity for a later audit.
    #[allow(dead_code)]
    pub implied_multiple_bond_ids: AtomSet,
    pub hydrogenated_edges: Vec<Edge>,
    pub additional_multiple_bond_ids: AtomSet,
    pub compatible: bool,
    pub added_hydrogen_operations: Vec<HydroOperation>,
    pub intrinsic_hydro_operations: Vec<HydroOperation>,
    pub composition_model: Option<ParentBondModel>,
}
impl ParentBondDelta {
    pub fn hydrogenated_atom_ids(&self) -> AtomSet {
        self.hydrogenated_edges
            .iter()
            .flat_map(|&(u, v)| [u, v])
            .collect()
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PiRedistribution {
    pub removed_bond_ids: AtomSet,
    pub added_bond_ids: AtomSet,
    pub hydrogenated_atom_ids: AtomSet,
    // Keep the exact witness that proves the returned redistribution.
    #[allow(dead_code)]
    pub final_assignment: BondAssignment,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalPiOperation {
    // The shared derivative proof retains typed operation labels and locants;
    // retained assembly currently consumes only their graph identities.
    #[allow(dead_code)]
    pub key: String,
    #[allow(dead_code)]
    pub locant: String,
    pub parent_atom_id: usize,
    pub external_atom_id: usize,
    pub bond_id: usize,
    pub external_atom_symbol: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
// The explicitly requested derivative-state API includes unsaturation evidence.
// Its detailed graph binding is read by the systematic audit in the next port.
#[allow(dead_code)]
pub struct UnsaturationOperation {
    pub key: String,
    pub locants: Vec<String>,
    pub atom_ids: Vec<usize>,
    pub bond_id: usize,
    pub bond_order: usize,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParentDerivativeState {
    pub bond_delta: ParentBondDelta,
    pub hydro_operations: Vec<HydroOperation>,
    pub unsaturation_operations: Vec<UnsaturationOperation>,
    // Preserve all exact derivative witnesses even when a retained caller
    // only needs their hydrogen projection.
    #[allow(dead_code)]
    pub oxo_operations: Vec<ExternalPiOperation>,
    #[allow(dead_code)]
    pub pi_redistribution: Option<PiRedistribution>,
    // Retained assembly currently renders oxo; preserve the other proved
    // external-pi operations in the requested shared derivative-state value.
    #[allow(dead_code)]
    pub imino_operations: Vec<ExternalPiOperation>,
    #[allow(dead_code)]
    pub alkylidene_operations: Vec<ExternalPiOperation>,
}
impl ParentDerivativeState {
    pub fn external_pi_operations(&self) -> impl Iterator<Item = &ExternalPiOperation> {
        self.oxo_operations
            .iter()
            .chain(&self.imino_operations)
            .chain(&self.alkylidene_operations)
    }
    pub fn added_hydrogen_operations(&self) -> &[HydroOperation] {
        &self.bond_delta.added_hydrogen_operations
    }
    pub fn intrinsic_hydro_operations(&self) -> &[HydroOperation] {
        &self.bond_delta.intrinsic_hydro_operations
    }
}
static EMPTY_ATOMS: AtomSet = AtomSet::new();
#[derive(Clone, Debug)]
pub struct CompareOptions<'a> {
    pub externally_unsaturated_atom_ids: &'a AtomSet,
    pub indicated_hydrogen_atom_ids: &'a AtomSet,
    pub atom_to_locant: Option<&'a BTreeMap<usize, String>>,
    pub preserve_retained_parent_state: bool,
    pub compose_retained_external_pi: bool,
}
impl Default for CompareOptions<'_> {
    fn default() -> Self {
        Self {
            externally_unsaturated_atom_ids: &EMPTY_ATOMS,
            indicated_hydrogen_atom_ids: &EMPTY_ATOMS,
            atom_to_locant: None,
            preserve_retained_parent_state: false,
            compose_retained_external_pi: false,
        }
    }
}
#[derive(Clone, Debug)]
pub struct ParentDerivativeOptions<'a> {
    pub indicated_hydrogen_atom_ids: &'a AtomSet,
    pub preserve_retained_parent_state: bool,
    pub allow_pi_redistribution: Option<bool>,
    pub compose_retained_external_pi: bool,
}
impl Default for ParentDerivativeOptions<'_> {
    fn default() -> Self {
        Self {
            indicated_hydrogen_atom_ids: &EMPTY_ATOMS,
            preserve_retained_parent_state: false,
            allow_pi_redistribution: None,
            compose_retained_external_pi: false,
        }
    }
}

fn touches(edge: Edge, sites: &AtomSet) -> bool {
    sites.contains(&edge.0) || sites.contains(&edge.1)
}
fn parent_degree(mol: &Molecule, atoms: &AtomSet, id: usize) -> usize {
    mol.get_neighbors(id)
        .iter()
        .filter(|n| atoms.contains(n))
        .count()
}
fn all_single(mol: &Molecule, id: usize) -> bool {
    mol.get_neighbors(id)
        .iter()
        .all(|&n| mol.get_bond(id, n).is_some_and(|b| b.order == 1))
}
fn bonding_number(mol: &Molecule, id: usize) -> usize {
    mol.atoms[&id].total_h_count
        + mol
            .get_neighbors(id)
            .iter()
            .map(|&n| mol.get_bond(id, n).map_or(0, |b| b.order))
            .sum::<usize>()
}

pub fn indicated_hydrogen_parent_bond_model(
    graph: &FusionGraph,
    sites: &AtomSet,
) -> Result<ParentBondModel, String> {
    let domain: AtomSet = graph.atoms.iter().map(|a| a.id).collect();
    if !sites.is_subset(&domain) {
        return Err("indicated-hydrogen site is outside the parent graph".into());
    }
    if graph
        .bonds
        .iter()
        .any(|b| b.bond_class == "double" && touches(b.atoms, sites))
    {
        return Err("indicated-hydrogen site conflicts with a required parent double bond".into());
    }
    let mut constrained = graph.clone();
    for atom in &mut constrained.atoms {
        if sites.contains(&atom.id) {
            atom.forced_single = true;
        }
    }
    parent_bond_model(&constrained)
}
pub fn single_site_parent_model(
    model: &ParentBondModel,
    sites: &AtomSet,
) -> Result<ParentBondModel, String> {
    let edges = model.edges();
    let atoms: AtomSet = edges.iter().flat_map(|&(u, v)| [u, v]).collect();
    let graph = FusionGraph {
        atoms: atoms
            .iter()
            .map(|&id| FusionGraphAtom::new(id, "C"))
            .collect(),
        bonds: edges
            .iter()
            .map(|&e| FusionGraphBond {
                atoms: e,
                bond_class: if model.required_double_bonds.contains(&e) {
                    "double"
                } else if model.required_single_bonds.contains(&e) {
                    "single"
                } else {
                    "mancude"
                }
                .into(),
            })
            .collect(),
    };
    indicated_hydrogen_parent_bond_model(&graph, sites)
}
pub fn saturated_nitrogen_hydrogen_sites(
    mol: &Molecule,
    atoms: &AtomSet,
    indicated: &AtomSet,
) -> AtomSet {
    if indicated.len() < 2
        || !indicated.is_subset(atoms)
        || indicated.iter().any(|id| mol.atoms[id].symbol != "N")
    {
        return AtomSet::new();
    }
    for &id in atoms {
        let atom = &mol.atoms[&id];
        if atom.is_aromatic
            || !matches!(
                (atom.symbol.as_str(), atom.charge),
                ("C", 0) | ("N", 0) | ("N", 1)
            )
            || !all_single(mol, id)
        {
            return AtomSet::new();
        }
        let valence = if atom.symbol == "C" {
            4
        } else {
            3 + atom.charge as usize
        };
        if mol.degree(id) > valence || atom.total_h_count != valence - mol.degree(id) {
            return AtomSet::new();
        }
    }
    indicated.clone()
}
pub fn has_complete_saturated_hydrogenation(
    mol: &Molecule,
    atoms: &AtomSet,
    state: &ParentDerivativeState,
) -> bool {
    let nitrogens = atoms
        .iter()
        .copied()
        .filter(|a| mol.atoms[a].symbol == "N")
        .collect();
    if saturated_nitrogen_hydrogen_sites(mol, atoms, &nitrogens).is_empty()
        || state.external_pi_operations().next().is_some()
        || !state.unsaturation_operations.is_empty()
        || state.hydro_operations.is_empty()
    {
        return false;
    }
    let edges: EdgeSet = state
        .bond_delta
        .hydrogenated_edges
        .iter()
        .copied()
        .collect();
    state.bond_delta.compatible
        && state.added_hydrogen_operations().is_empty()
        && state.intrinsic_hydro_operations().is_empty()
        && edges.len() * 2 == atoms.len()
        && state.bond_delta.hydrogenated_atom_ids() == *atoms
        && state
            .bond_delta
            .assignment
            .orders
            .iter()
            .all(|(e, o)| *o == 1 || (*o == 2 && edges.contains(e)))
}
/// A charged sigma N citation must preserve ring pi count and hydrogen sites.
pub fn protonated_hydrogenation_is_complete(
    mol: &Molecule,
    atoms: &AtomSet,
    model: &ParentBondModel,
    state: &ParentDerivativeState,
    indicated: &AtomSet,
) -> Result<bool, String> {
    if !atoms.iter().any(|&id| {
        let atom = &mol.atoms[&id];
        atom.symbol == "N"
            && atom.charge == 1
            && atom.total_h_count > 0
            && atom.total_h_count + mol.degree(id) == 4
            && all_single(mol, id)
    }) || !mol.bonds.values().any(|bond| {
        atoms.contains(&bond.u) && atoms.contains(&bond.v) && bond.order == 2 && !bond.is_aromatic
    }) || state.external_pi_operations().next().is_some()
        || !state.unsaturation_operations.is_empty()
    {
        return Ok(true);
    }
    let sites = indicated
        .union(&state.bond_delta.hydrogenated_atom_ids())
        .copied()
        .collect();
    let remaining = match single_site_parent_model(model, &sites) {
        Ok(remaining) => remaining,
        Err(error) if super::numbering::mancude_search_budget_exceeded(&error) => {
            return Err(error);
        }
        Err(_) => return Ok(false),
    };
    let observed = mol
        .bonds
        .values()
        .filter(|bond| atoms.contains(&bond.u) && atoms.contains(&bond.v) && bond.order == 2)
        .count();
    Ok(remaining.maximum_non_cumulative_double_bonds == observed
        && remaining
            .allowed_kekule_assignments
            .iter()
            .all(|assignment| {
                atoms
                    .iter()
                    .filter(|id| {
                        matches!(mol.atoms[id].symbol.as_str(), "C" | "N")
                            && !mol.atoms[id].is_aromatic
                    })
                    .all(|id| {
                        let expected_pi: usize = assignment
                            .orders
                            .iter()
                            .filter(|((u, v), _)| u == id || v == id)
                            .map(|(_, order)| order - 1)
                            .sum();
                        let actual_pi: usize = mol
                            .get_neighbors(*id)
                            .iter()
                            .filter(|neighbor| atoms.contains(neighbor))
                            .filter_map(|neighbor| mol.get_bond(*id, *neighbor))
                            .map(|bond| bond.order - 1)
                            .sum();
                        expected_pi == actual_pi
                    })
            }))
}

fn single_bonded_parent_nitrogen(mol: &Molecule, atoms: &AtomSet, id: usize) -> bool {
    let atom = &mol.atoms[&id];
    atom.symbol == "N"
        && atom.charge == 0
        && parent_degree(mol, atoms, id) == 2
        && atom.total_h_count + mol.degree(id) == 3
        && all_single(mol, id)
}
fn five_membered_single_nitrogen(mol: &Molecule, atoms: &AtomSet, id: usize) -> bool {
    if !single_bonded_parent_nitrogen(mol, atoms, id) {
        return false;
    }
    let neighbors: Vec<_> = mol
        .get_neighbors(id)
        .iter()
        .copied()
        .filter(|n| atoms.contains(n))
        .collect();
    let (first, last) = (neighbors[0], neighbors[1]);
    mol.get_neighbors(first)
        .iter()
        .copied()
        .filter(|n| atoms.contains(n) && *n != id && *n != last)
        .any(|second| {
            mol.get_neighbors(last)
                .iter()
                .copied()
                .filter(|n| atoms.contains(n) && *n != id && *n != first && *n != second)
                .any(|third| mol.get_bond(second, third).is_some())
        })
}
pub fn is_added_hydrogen_nitrogen(mol: &Molecule, atoms: &AtomSet, id: usize) -> bool {
    atoms.contains(&id)
        && !mol.atoms[&id].is_aromatic
        && single_bonded_parent_nitrogen(mol, atoms, id)
}
pub fn nitrogen_composition_parent_model(
    mol: &Molecule,
    atoms: &AtomSet,
    model: &ParentBondModel,
    indicated: &AtomSet,
) -> Result<ParentBondModel, String> {
    let mut sites = saturated_nitrogen_hydrogen_sites(mol, atoms, indicated);
    for &id in atoms {
        let atom = &mol.atoms[&id];
        let degree = parent_degree(mol, atoms, id);
        if atom.symbol == "N"
            && atom.charge == 0
            && (degree == 3
                || ((indicated.contains(&id)
                    || (atom.total_h_count == 0
                        && mol.degree(id) == 3
                        && degree == 2
                        && all_single(mol, id)))
                    && !atom.is_aromatic
                    && single_bonded_parent_nitrogen(mol, atoms, id)
                    && (indicated.contains(&id) || five_membered_single_nitrogen(mol, atoms, id))))
        {
            sites.insert(id);
        }
    }
    if sites.is_empty()
        || !model
            .pi_eligible_edges
            .iter()
            .chain(&model.required_double_bonds)
            .any(|&e| touches(e, &sites))
    {
        return Ok(model.clone());
    }
    let constrained = single_site_parent_model(model, &sites)?;
    let aromatic: AtomSet = sites
        .iter()
        .copied()
        .filter(|a| mol.atoms[a].is_aromatic)
        .collect();
    let boundary: EdgeSet = if aromatic.is_empty() {
        EdgeSet::new()
    } else {
        mol.bonds
            .values()
            .filter(|b| {
                atoms.contains(&b.u)
                    && atoms.contains(&b.v)
                    && b.order == 1
                    && mol.atoms[&b.u].is_aromatic != mol.atoms[&b.v].is_aromatic
            })
            .map(|b| normalize_edge(b.u, b.v))
            .collect()
    };
    if !aromatic.is_empty()
        && constrained.allowed_kekule_assignments.iter().all(|a| {
            a.orders
                .iter()
                .any(|&(e, o)| o == 2 && boundary.contains(&e))
        })
    {
        let remaining = sites.difference(&aromatic).copied().collect::<AtomSet>();
        return if remaining.is_empty() {
            Ok(model.clone())
        } else {
            single_site_parent_model(model, &remaining)
        };
    }
    Ok(constrained)
}

pub fn compare_actual_parent_to_implied_parent(
    mol: &Molecule,
    atoms: &AtomSet,
    model: &ParentBondModel,
    options: &CompareOptions<'_>,
) -> Result<Option<ParentBondDelta>, String> {
    let observed: BTreeMap<_, _> = mol
        .bonds
        .values()
        .filter(|b| atoms.contains(&b.u) && atoms.contains(&b.v))
        .map(|b| (normalize_edge(b.u, b.v), b))
        .collect();
    if observed.keys().copied().collect::<EdgeSet>() != model.edges() {
        return Ok(None);
    }
    let resolved;
    let model = if options.preserve_retained_parent_state {
        model
    } else {
        resolved = match nitrogen_composition_parent_model(
            mol,
            atoms,
            model,
            options.indicated_hydrogen_atom_ids,
        ) {
            Ok(m) => m,
            Err(e) if super::numbering::mancude_search_budget_exceeded(&e) => return Err(e),
            Err(_) => return Ok(None),
        };
        &resolved
    };
    let mut numbered_edges: Vec<_> = observed.keys().copied().collect();
    if let Some(locants) = options.atom_to_locant {
        numbered_edges.sort_by_key(|&(u, v)| {
            let mut keys = [locant_key(&locants[&u]), locant_key(&locants[&v])];
            keys.sort();
            keys
        });
    }
    type DeltaRank = (usize, usize, usize, usize, usize, Vec<usize>);
    let mut best: Option<(DeltaRank, ParentBondDelta)> = None;
    for assignment in &model.allowed_kekule_assignments {
        let expected: BTreeMap<_, _> = assignment
            .orders
            .iter()
            .map(|&(e, o)| (normalize_edge(e.0, e.1), o))
            .collect();
        if expected.keys().ne(observed.keys()) {
            continue;
        }
        let mut implied = AtomSet::new();
        let mut hydrogenated = Vec::new();
        let mut additional = AtomSet::new();
        let mut incompatible = 0;
        for (&edge, bond) in &observed {
            let order = expected[&edge];
            let aromatic = model.pi_eligible_edges.contains(&edge)
                && mol.atoms[&edge.0].is_aromatic
                && mol.atoms[&edge.1].is_aromatic;
            if aromatic {
                if bond.order > 1 {
                    implied.insert(bond.idx);
                }
                continue;
            }
            if bond.order == order {
                if bond.order > 1 {
                    implied.insert(bond.idx);
                }
            } else if bond.order == 1 && order == 2 {
                hydrogenated.push(edge);
            } else if bond.order > order {
                additional.insert(bond.idx);
            } else {
                incompatible += 1;
            }
        }
        let after: Vec<_> = hydrogenated
            .iter()
            .copied()
            .filter(|&e| !touches(e, options.indicated_hydrogen_atom_ids))
            .collect();
        let residual: Vec<_> = after
            .iter()
            .copied()
            .filter(|&e| !touches(e, options.externally_unsaturated_atom_ids))
            .collect();
        // Without a numbered map the sorted edge domain is common to all assignments.
        let tie = if options.atom_to_locant.is_some() {
            numbered_edges.iter().map(|e| expected[e]).collect()
        } else {
            assignment.orders.iter().map(|(_, o)| *o).collect()
        };
        let rank = (
            incompatible,
            additional.len(),
            hydrogenated.len(),
            after.len(),
            after.len() - residual.len(),
            tie,
        );
        let delta = ParentBondDelta {
            assignment: assignment.clone(),
            implied_multiple_bond_ids: implied,
            hydrogenated_edges: residual,
            additional_multiple_bond_ids: additional,
            compatible: incompatible == 0,
            ..Default::default()
        };
        if best.as_ref().is_none_or(|(r, _)| rank < *r) {
            best = Some((rank, delta));
        }
    }
    let Some((_, mut delta)) = best else {
        return Ok(None);
    };
    let Some(locants) = options.atom_to_locant else {
        return Ok(Some(delta));
    };
    if !delta.compatible {
        return Ok(Some(delta));
    }
    if options.preserve_retained_parent_state {
        if options.compose_retained_external_pi
            && !options.externally_unsaturated_atom_ids.is_empty()
        {
            let mut selected = model.clone();
            selected.allowed_kekule_assignments = vec![delta.assignment.clone()];
            if let Some(composed) = external_pi_parent_delta(
                mol,
                atoms,
                &selected,
                options.externally_unsaturated_atom_ids,
                locants,
                true,
            )? {
                return Ok(Some(composed));
            }
        }
        return Ok(Some(delta));
    }
    let donor_constrained = !options.indicated_hydrogen_atom_ids.is_empty()
        && options
            .indicated_hydrogen_atom_ids
            .iter()
            .all(|a| mol.atoms[a].symbol == "N");
    let mut consumed = delta.assignment.orders.iter().any(|&(e, o)| {
        o == 2
            && observed[&normalize_edge(e.0, e.1)].order == 1
            && touches(e, options.externally_unsaturated_atom_ids)
    });
    if consumed {
        let pi = model.pi_atoms();
        consumed = atoms
            .iter()
            .all(|a| mol.atoms[a].symbol != "C" || pi.contains(a));
    }
    let paired_external: AtomSet = delta
        .assignment
        .orders
        .iter()
        .filter(|&&(e, o)| o == 2 && touches(e, options.indicated_hydrogen_atom_ids))
        .flat_map(|&(e, _)| [e.0, e.1])
        .filter(|a| options.externally_unsaturated_atom_ids.contains(a))
        .collect();
    if !options
        .externally_unsaturated_atom_ids
        .is_subset(&paired_external)
        && (!delta.additional_multiple_bond_ids.is_empty()
            || !delta.hydrogenated_edges.is_empty()
            || donor_constrained
            || consumed)
        && let Some(composed) = external_pi_parent_delta(
            mol,
            atoms,
            model,
            options.externally_unsaturated_atom_ids,
            locants,
            false,
        )?
    {
        return Ok(Some(composed));
    }
    delta.intrinsic_hydro_operations = fixed_carbon_hydro_operations(
        mol,
        atoms,
        model,
        &delta,
        options.externally_unsaturated_atom_ids,
        options.indicated_hydrogen_atom_ids,
        locants,
    );
    Ok(Some(delta))
}

pub fn prove_pi_redistribution(
    mol: &Molecule,
    atoms: &AtomSet,
    model: &ParentBondModel,
    delta: &ParentBondDelta,
    indicated: &AtomSet,
    external: &[ExternalPiOperation],
) -> Result<Option<PiRedistribution>, String> {
    let model = delta.composition_model.as_ref().unwrap_or(model);
    if !indicated.is_subset(atoms)
        || !delta.compatible
        || delta.hydrogenated_edges.is_empty()
        || (!delta.added_hydrogen_operations.is_empty() && delta.composition_model.is_none())
        || !delta.intrinsic_hydro_operations.is_empty()
        || !model.allowed_kekule_assignments.contains(&delta.assignment)
    {
        return Ok(None);
    }
    let observed: BTreeMap<_, _> = mol
        .bonds
        .values()
        .filter(|b| atoms.contains(&b.u) && atoms.contains(&b.v))
        .map(|b| (normalize_edge(b.u, b.v), b))
        .collect();
    let expected: BTreeMap<_, _> = delta.assignment.orders.iter().copied().collect();
    if observed.keys().ne(expected.keys()) || observed.values().any(|b| !matches!(b.order, 1 | 2)) {
        return Ok(None);
    }
    let mut oxo_bonds = AtomSet::new();
    let mut oxo_sites = AtomSet::new();
    for op in external {
        let parent = op.parent_atom_id;
        let ligand = op.external_atom_id;
        if !atoms.contains(&parent)
            || atoms.contains(&ligand)
            || !mol.atoms.contains_key(&ligand)
            || !mol.bonds.get(&op.bond_id).is_some_and(|b| {
                b.order == 2 && normalize_edge(b.u, b.v) == normalize_edge(parent, ligand)
            })
            || oxo_bonds.contains(&op.bond_id)
            || mol.atoms[&ligand].symbol != op.external_atom_symbol
            || !is_neutral_external_pi_ligand(mol, parent, ligand)
            || indicated.contains(&parent)
        {
            return Ok(None);
        }
        oxo_bonds.insert(op.bond_id);
        oxo_sites.insert(parent);
    }
    for &id in atoms {
        let value = &mol.atoms[&id];
        let degree = parent_degree(mol, atoms, id);
        let neighbors = mol.get_neighbors(id);
        if value.charge != 0
            || !matches!(degree, 2..=4)
            || neighbors.iter().any(|&n| {
                !atoms.contains(&n)
                    && mol
                        .get_bond(id, n)
                        .is_some_and(|b| b.order != 1 && !oxo_bonds.contains(&b.idx))
            })
            || bonding_number(mol, id)
                != neutral_lambda_oxo_bonding_number(mol, id)
                    .unwrap_or_else(|| value.element().map_or(0, |e| e.standard_valence))
        {
            return Ok(None);
        }
        if degree == 4
            && (value.symbol != "C"
                || neighbors
                    .iter()
                    .any(|&n| atoms.contains(&n) && expected[&normalize_edge(id, n)] != 1))
        {
            return Ok(None);
        }
        if oxo_sites.contains(&id)
            && neighbors.iter().any(|&n| {
                atoms.contains(&n)
                    && (expected[&normalize_edge(id, n)] != 1
                        || mol.get_bond(id, n).is_none_or(|b| b.order != 1))
            })
        {
            return Ok(None);
        }
        if indicated.contains(&id)
            && neighbors.iter().any(|&n| {
                atoms.contains(&n)
                    && mol
                        .get_bond(id, n)
                        .is_none_or(|b| b.order != expected[&normalize_edge(id, n)])
            })
        {
            return Ok(None);
        }
    }
    let mut parent_pi: BTreeMap<_, usize> = atoms.iter().map(|&a| (a, 0)).collect();
    let mut actual_pi = parent_pi.clone();
    let mut removed = AtomSet::new();
    let mut added = AtomSet::new();
    for (&edge, bond) in &observed {
        if !matches!(expected[&edge], 1 | 2) {
            return Ok(None);
        }
        for id in [edge.0, edge.1] {
            *parent_pi.entry(id).or_default() += expected[&edge] - 1;
            *actual_pi.entry(id).or_default() += bond.order - 1;
        }
        if expected[&edge] > bond.order {
            removed.insert(bond.idx);
        } else if expected[&edge] < bond.order {
            added.insert(bond.idx);
        }
    }
    if atoms
        .iter()
        .any(|a| parent_pi[a] > 1 || actual_pi[a] > parent_pi[a])
    {
        return Ok(None);
    }
    let sites: AtomSet = atoms
        .iter()
        .copied()
        .filter(|a| parent_pi[a] - actual_pi[a] == 1)
        .collect();
    if sites.is_empty()
        || !sites.len().is_multiple_of(2)
        || removed.len() < added.len()
        || 2 * (removed.len() - added.len()) != sites.len()
    {
        return Ok(None);
    }
    for &id in &sites {
        let value = &mol.atoms[&id];
        if value.symbol == "C" {
            continue;
        }
        if value.symbol != "N"
            || value.charge != 0
            || value.is_aromatic
            || parent_degree(mol, atoms, id) != 2
            || value.total_h_count + mol.degree(id) != 3
            || !all_single(mol, id)
        {
            return Ok(None);
        }
    }
    if added.is_empty()
        || (sites == delta.hydrogenated_atom_ids() && delta.additional_multiple_bond_ids.is_empty())
        || !delta.additional_multiple_bond_ids.is_subset(&added)
    {
        return Ok(None);
    }
    let implicit = match single_site_parent_model(model, &sites) {
        Ok(m) => m,
        Err(e) if super::numbering::mancude_search_budget_exceeded(&e) => return Err(e),
        Err(_) => return Ok(None),
    };
    let final_assignment = BondAssignment {
        orders: observed.iter().map(|(&e, b)| (e, b.order)).collect(),
    };
    if !implicit
        .allowed_kekule_assignments
        .contains(&final_assignment)
        || implicit.maximum_non_cumulative_double_bonds + sites.len() / 2
            != model.maximum_non_cumulative_double_bonds
    {
        return Ok(None);
    }
    Ok(Some(PiRedistribution {
        removed_bond_ids: removed,
        added_bond_ids: added,
        hydrogenated_atom_ids: sites,
        final_assignment,
    }))
}
fn spiro_carbon_sites(mol: &Molecule, atoms: &AtomSet) -> AtomSet {
    let mut sites = AtomSet::new();
    for &id in atoms {
        let atom = &mol.atoms[&id];
        let outside: AtomSet = mol
            .get_neighbors(id)
            .iter()
            .copied()
            .filter(|n| !atoms.contains(n))
            .collect();
        if atom.symbol != "C"
            || atom.charge != 0
            || parent_degree(mol, atoms, id) != 2
            || outside.len() != 2
            || !all_single(mol, id)
        {
            continue;
        }
        let mut reached = AtomSet::new();
        let mut boundary = AtomSet::new();
        let mut pending = outside.first().copied().into_iter().collect::<Vec<_>>();
        while let Some(current) = pending.pop() {
            if !reached.insert(current) {
                continue;
            }
            for &n in mol.get_neighbors(current) {
                if atoms.contains(&n) {
                    boundary.insert(n);
                } else if !reached.contains(&n) {
                    pending.push(n);
                }
            }
        }
        if outside.is_subset(&reached) && boundary == AtomSet::from([id]) {
            sites.insert(id);
        }
    }
    sites
}
fn external_pi_parent_delta(
    mol: &Molecule,
    atoms: &AtomSet,
    model: &ParentBondModel,
    external: &AtomSet,
    locants: &BTreeMap<usize, String>,
    preserve_assignment: bool,
) -> Result<Option<ParentBondDelta>, String> {
    if external.iter().any(|&id| {
        mol.atoms[&id].charge != 0
            || (mol.atoms[&id].symbol != "C"
                && neutral_lambda_oxo_bonding_number(mol, id).is_none())
    }) {
        return Ok(None);
    }
    let mut forced = external.clone();
    forced.extend(spiro_carbon_sites(mol, atoms));
    forced.extend(atoms.iter().copied().filter(|&id| {
        let a = &mol.atoms[&id];
        a.symbol == "N"
            && a.charge == 0
            && a.total_h_count == 0
            && mol.degree(id) == 3
            && parent_degree(mol, atoms, id) == 3
            && all_single(mol, id)
    }));
    let constrained = match single_site_parent_model(model, &forced) {
        Ok(m) => m,
        Err(e) if super::numbering::mancude_search_budget_exceeded(&e) => return Err(e),
        Err(_) => return Ok(None),
    };
    let pi_atoms = model.pi_atoms();
    let mut assignments = std::collections::BTreeSet::new();
    for assignment in &model.allowed_kekule_assignments {
        assignments.insert(BondAssignment {
            orders: assignment
                .orders
                .iter()
                .map(|&(e, o)| (e, if touches(e, &forced) { 1 } else { o }))
                .collect(),
        });
    }
    if !preserve_assignment {
        assignments.extend(constrained.allowed_kekule_assignments.iter().cloned());
    }
    let mut candidates = Vec::new();
    for assignment in assignments {
        let paired: AtomSet = assignment
            .orders
            .iter()
            .filter(|(_, o)| *o == 2)
            .flat_map(|&(e, _)| [e.0, e.1])
            .collect();
        let added: AtomSet = pi_atoms
            .difference(&paired)
            .copied()
            .filter(|a| !forced.contains(a))
            .collect();
        if added.iter().any(|&id| {
            let a = &mol.atoms[&id];
            !matches!(a.symbol.as_str(), "C" | "N")
                || (a.symbol == "N" && !is_added_hydrogen_nitrogen(mol, atoms, id))
                || a.charge != 0
                || a.is_aromatic
                || !matches!(parent_degree(mol, atoms, id), 2 | 3)
                || !all_single(mol, id)
        }) {
            continue;
        }
        let mut sites = forced.clone();
        sites.extend(&added);
        let candidate_model = if added.is_empty() {
            constrained.clone()
        } else {
            single_site_parent_model(model, &sites)?
        };
        let mut witness = candidate_model;
        witness.allowed_kekule_assignments = vec![assignment];
        let Some(state) = parent_derivative_state(
            mol,
            atoms,
            &witness,
            locants,
            &ParentDerivativeOptions {
                preserve_retained_parent_state: true,
                allow_pi_redistribution: Some(true),
                ..Default::default()
            },
        )?
        else {
            continue;
        };
        if !state.unsaturation_operations.is_empty() {
            continue;
        }
        let mut delta = state.bond_delta;
        let mut ordered: Vec<_> = added.iter().copied().collect();
        ordered.sort_by_key(|a| locant_key(&locants[a]));
        if !ordered.is_empty() {
            let bonds: AtomSet = ordered
                .iter()
                .flat_map(|&a| {
                    mol.get_neighbors(a)
                        .iter()
                        .copied()
                        .filter(|n| atoms.contains(n))
                        .filter_map(move |n| mol.get_bond(a, n).map(|b| b.idx))
                })
                .collect();
            delta.added_hydrogen_operations = vec![HydroOperation {
                key: "added_hydrogen".into(),
                locants: ordered.iter().map(|a| locants[a].clone()).collect(),
                atom_ids: ordered.clone(),
                bond_ids: bonds.into_iter().collect(),
                operation_kind: "indicated_hydrogen".into(),
            }];
        }
        let mut hydro_keys: Vec<_> = delta
            .hydrogenated_atom_ids()
            .iter()
            .map(|a| locant_key(&locants[a]))
            .collect();
        hydro_keys.sort();
        let rank = (
            !delta.additional_multiple_bond_ids.is_empty(),
            delta.hydrogenated_edges.len(),
            ordered
                .iter()
                .map(|a| locant_key(&locants[a]))
                .collect::<Vec<_>>(),
            hydro_keys,
        );
        candidates.push((rank, delta));
    }
    let Some((_, mut selected)) = candidates.into_iter().min_by(|a, b| a.0.cmp(&b.0)) else {
        return Ok(None);
    };
    if !selected.hydrogenated_edges.is_empty() {
        let added: AtomSet = selected
            .added_hydrogen_operations
            .iter()
            .flat_map(|op| op.atom_ids.iter().copied())
            .collect();
        let mut sites = forced;
        sites.extend(added);
        let composition = single_site_parent_model(model, &sites)?;
        if composition
            .allowed_kekule_assignments
            .contains(&selected.assignment)
        {
            selected.composition_model = Some(composition);
        }
    }
    Ok(Some(selected))
}
fn fixed_carbon_hydro_operations(
    mol: &Molecule,
    atoms: &AtomSet,
    model: &ParentBondModel,
    delta: &ParentBondDelta,
    oxo: &AtomSet,
    indicated: &AtomSet,
    locants: &BTreeMap<usize, String>,
) -> Vec<HydroOperation> {
    if oxo.is_empty()
        || !indicated.is_empty()
        || !delta.hydrogenated_edges.is_empty()
        || !delta.additional_multiple_bond_ids.is_empty()
    {
        return Vec::new();
    }
    let sites: AtomSet = atoms
        .iter()
        .copied()
        .filter(|&id| {
            mol.atoms[&id].symbol == "C"
                && mol.atoms[&id].charge == 0
                && parent_degree(mol, atoms, id) == 2
                && mol.get_neighbors(id).iter().all(|&n| {
                    !atoms.contains(&n)
                        || model.required_single_bonds.contains(&normalize_edge(id, n))
                })
        })
        .collect();
    if sites.len() != 2 || sites.is_disjoint(oxo) {
        return Vec::new();
    }
    let mut ordered: Vec<_> = sites.into_iter().collect();
    ordered.sort_by_key(|a| locant_key(&locants[a]));
    vec![HydroOperation {
        key: "intrinsic_parent_hydrogen".into(),
        locants: ordered.iter().map(|a| locants[a].clone()).collect(),
        atom_ids: ordered,
        operation_kind: "additive_hydrogen".into(),
        ..Default::default()
    }]
}
pub fn parent_derivative_state(
    mol: &Molecule,
    atoms: &AtomSet,
    model: &ParentBondModel,
    locants: &BTreeMap<usize, String>,
    options: &ParentDerivativeOptions<'_>,
) -> Result<Option<ParentDerivativeState>, String> {
    if locants.keys().copied().collect::<AtomSet>() != *atoms {
        return Ok(None);
    }
    let mut oxo = Vec::new();
    let mut imino = Vec::new();
    let mut alkylidene = Vec::new();
    let mut ordered: Vec<_> = atoms.iter().copied().collect();
    ordered.sort_by_key(|a| locant_key(&locants[a]));
    for id in ordered {
        for &n in mol.get_neighbors(id) {
            if atoms.contains(&n) || !matches!(mol.atoms[&n].symbol.as_str(), "O" | "N" | "C") {
                continue;
            }
            let Some(bond) = mol.get_bond(id, n).filter(|b| b.order == 2) else {
                continue;
            };
            let symbol = &mol.atoms[&n].symbol;
            if symbol != "O" && !is_neutral_external_pi_ligand(mol, id, n) {
                continue;
            }
            let op = ExternalPiOperation {
                key: match symbol.as_str() {
                    "O" => "oxo",
                    "N" => "imino",
                    _ => "alkylidene",
                }
                .into(),
                locant: locants[&id].clone(),
                parent_atom_id: id,
                external_atom_id: n,
                bond_id: bond.idx,
                external_atom_symbol: symbol.clone(),
            };
            match symbol.as_str() {
                "O" => oxo.push(op),
                "N" => imino.push(op),
                _ => alkylidene.push(op),
            }
        }
    }
    let external: Vec<_> = oxo
        .iter()
        .chain(&imino)
        .chain(&alkylidene)
        .cloned()
        .collect();
    let external_sites = external.iter().map(|op| op.parent_atom_id).collect();
    let Some(delta) = compare_actual_parent_to_implied_parent(
        mol,
        atoms,
        model,
        &CompareOptions {
            externally_unsaturated_atom_ids: &external_sites,
            indicated_hydrogen_atom_ids: options.indicated_hydrogen_atom_ids,
            atom_to_locant: Some(locants),
            preserve_retained_parent_state: options.preserve_retained_parent_state,
            compose_retained_external_pi: options.compose_retained_external_pi,
        },
    )?
    else {
        return Ok(None);
    };
    if !delta.compatible {
        return Ok(None);
    }
    let redistribution = if options
        .allow_pi_redistribution
        .unwrap_or(!options.preserve_retained_parent_state)
    {
        prove_pi_redistribution(
            mol,
            atoms,
            model,
            &delta,
            options.indicated_hydrogen_atom_ids,
            &external,
        )?
    } else {
        None
    };
    let hydro_atoms = redistribution
        .as_ref()
        .map(|r| r.hydrogenated_atom_ids.clone())
        .unwrap_or_else(|| delta.hydrogenated_atom_ids());
    let mut ordered: Vec<_> = hydro_atoms.into_iter().collect();
    ordered.sort_by_key(|a| locant_key(&locants[a]));
    let hydro_bonds: AtomSet = if let Some(r) = &redistribution {
        r.removed_bond_ids
            .union(&r.added_bond_ids)
            .copied()
            .collect()
    } else {
        delta
            .hydrogenated_edges
            .iter()
            .filter_map(|&(u, v)| mol.get_bond(u, v).map(|b| b.idx))
            .collect()
    };
    let hydro = if ordered.is_empty() {
        Vec::new()
    } else {
        vec![HydroOperation {
            key: "additive_hydrogen".into(),
            locants: ordered.iter().map(|a| locants[a].clone()).collect(),
            atom_ids: ordered,
            bond_ids: hydro_bonds.into_iter().collect(),
            operation_kind: "additive_hydrogen".into(),
        }]
    };
    let mut unsaturation = Vec::new();
    if redistribution.is_none() {
        for id in &delta.additional_multiple_bond_ids {
            let bond = &mol.bonds[id];
            let mut ordered = vec![bond.u, bond.v];
            ordered.sort_by_key(|a| locant_key(&locants[a]));
            unsaturation.push(UnsaturationOperation {
                key: "additional_unsaturation".into(),
                locants: ordered.iter().map(|a| locants[a].clone()).collect(),
                atom_ids: ordered,
                bond_id: *id,
                bond_order: bond.order,
            });
        }
    }
    Ok(Some(ParentDerivativeState {
        bond_delta: delta,
        hydro_operations: hydro,
        unsaturation_operations: unsaturation,
        oxo_operations: oxo,
        pi_redistribution: redistribution,
        imino_operations: imino,
        alkylidene_operations: alkylidene,
    }))
}

#[cfg(test)]
mod tests {
    use super::super::super::molecule::{Atom, Bond};
    use super::*;

    fn carbon_ring(size: usize, doubles: &EdgeSet) -> Molecule {
        let mut mol = Molecule::new();
        for id in 0..size {
            mol.add_atom(Atom::new(id, "C")).unwrap();
        }
        for id in 0..size {
            let next = (id + 1) % size;
            let order = if doubles.contains(&normalize_edge(id, next)) {
                2
            } else {
                1
            };
            mol.add_bond(Bond::new(id, id, next, order)).unwrap();
        }
        for id in 0..size {
            let load: usize = mol
                .get_neighbors(id)
                .iter()
                .map(|&n| mol.get_bond(id, n).unwrap().order)
                .sum();
            mol.atoms.get_mut(&id).unwrap().total_h_count = 4 - load;
        }
        mol
    }

    #[test]
    fn pi_redistribution_cites_only_net_hydrogen_endpoints() {
        let mol = carbon_ring(6, &EdgeSet::from([(1, 2), (4, 5)]));
        let atoms = (0..6).collect();
        let graph = super::super::numbering::parent_bond_graph(&mol, &atoms);
        let mut model = parent_bond_model(&graph).unwrap();
        model.allowed_kekule_assignments = vec![BondAssignment {
            orders: model
                .edges()
                .iter()
                .map(|&e| {
                    (
                        e,
                        if [(0, 1), (2, 3), (4, 5)].contains(&e) {
                            2
                        } else {
                            1
                        },
                    )
                })
                .collect(),
        }];
        let locants = (0..6).map(|a| (a, (a + 1).to_string())).collect();
        let state = parent_derivative_state(&mol, &atoms, &model, &locants, &Default::default())
            .unwrap()
            .unwrap();
        assert!(state.pi_redistribution.is_some());
        assert_eq!(state.hydro_operations[0].locants, ["1", "4"]);
        assert!(state.unsaturation_operations.is_empty());
        assert_eq!(
            state.bond_delta.hydrogenated_atom_ids(),
            AtomSet::from([0, 1, 2, 3])
        );
    }

    #[test]
    fn retained_external_pi_composition_owns_added_hydrogen() {
        let mut mol = carbon_ring(4, &EdgeSet::new());
        mol.atoms.get_mut(&0).unwrap().total_h_count = 0;
        mol.add_atom(Atom::new(4, "O")).unwrap();
        mol.add_bond(Bond::new(4, 0, 4, 2)).unwrap();
        let atoms = (0..4).collect();
        let model =
            parent_bond_model(&super::super::numbering::parent_bond_graph(&mol, &atoms)).unwrap();
        let locants = (0..4).map(|a| (a, (a + 1).to_string())).collect();
        let state = parent_derivative_state(
            &mol,
            &atoms,
            &model,
            &locants,
            &ParentDerivativeOptions {
                preserve_retained_parent_state: true,
                compose_retained_external_pi: true,
                ..Default::default()
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(state.oxo_operations.len(), 1);
        assert_eq!(state.bond_delta.added_hydrogen_operations.len(), 1);
        assert_eq!(
            state.bond_delta.added_hydrogen_operations[0].atom_ids.len(),
            1
        );
        assert_eq!(state.hydro_operations[0].atom_ids.len(), 2);
    }
}
