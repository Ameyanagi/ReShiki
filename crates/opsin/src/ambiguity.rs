//! AmbiguityChecker from OPSIN 2.9.0, b91b610af5ab07560fedb20730d7aef46bb2bca0.
//! MIT, Daniel Lowe and contributors. Environment analysis includes transient
//! substitutable-hydrogen ghosts and preserves the source's out-valency split.
use crate::fragment_tools::calculate_substitutable_hydrogen_atoms;
use crate::graph::{AtomId, BondId, Element, Graph, GraphError};
use crate::stereo_analyser::{self, StereoAnalysis};
use std::collections::{BTreeMap, HashSet};

pub fn analyse_relevant_atoms_and_bonds(
    graph: &Graph,
    starting_atoms: &[AtomId],
) -> Result<StereoAnalysis, GraphError> {
    let mut atoms = Vec::new();
    let mut atom_set = HashSet::new();
    let mut bonds = Vec::new();
    let mut bond_set = HashSet::new();
    let mut stack = starting_atoms.to_vec();
    while let Some(atom) = stack.pop() {
        if atom_set.insert(atom) {
            atoms.push(atom);
            for &bond in &graph.atom(atom).bonds {
                if bond_set.insert(bond) {
                    bonds.push(bond);
                }
                stack.push(graph.bond(bond).other_atom(atom).unwrap());
            }
        }
    }
    let mut expanded = graph.clone();
    let mut ghost_atoms = Vec::new();
    for &atom in &atoms {
        for _ in 0..calculate_substitutable_hydrogen_atoms(&expanded, atom) {
            let fragment = expanded.atom(atom).fragment;
            let hydrogen = expanded.add_atom(fragment, Element::H);
            // In Java these bonds are present on atoms, but not in the supplied
            // bond collection. Including single bonds here adds no multiple-
            // bond ghosts and gives exactly the same neighbour-colour graph.
            bonds.push(expanded.add_bond(hydrogen, atom, 1)?);
            ghost_atoms.push(hydrogen);
        }
    }
    atoms.extend(ghost_atoms);
    stereo_analyser::analyse_atoms_and_bonds(&expanded, &atoms, &bonds)
}

pub fn atom_environment(
    graph: &Graph,
    analysis: &StereoAnalysis,
    atom: AtomId,
) -> Result<String, GraphError> {
    let environment = analysis
        .atom_environment_number(atom)
        .ok_or_else(|| GraphError("OPSIN Bug: Atom was not part of ambiguity analysis".into()))?;
    Ok(format!("{environment}\t{}", graph.atom(atom).out_valency))
}
pub fn all_atoms_equivalent(graph: &Graph, atoms: &[AtomId]) -> Result<bool, GraphError> {
    let analysis = analyse_relevant_atoms_and_bonds(graph, atoms)?;
    let environments: HashSet<_> = atoms
        .iter()
        .map(|atom| atom_environment(graph, &analysis, *atom))
        .collect::<Result<_, _>>()?;
    Ok(environments.len() == 1)
}
pub fn all_bonds_equivalent(graph: &Graph, bonds: &[BondId]) -> Result<bool, GraphError> {
    let atoms: Vec<_> = bonds
        .iter()
        .flat_map(|bond| {
            let bond = graph.bond(*bond);
            [bond.from, bond.to]
        })
        .collect();
    let analysis = analyse_relevant_atoms_and_bonds(graph, &atoms)?;
    let mut environments = HashSet::new();
    for bond in bonds {
        let bond = graph.bond(*bond);
        let first = atom_environment(graph, &analysis, bond.from)?;
        let second = atom_environment(graph, &analysis, bond.to)?;
        environments.insert(if first > second {
            format!("{first}{second}")
        } else {
            format!("{second}{first}")
        });
    }
    Ok(environments.len() == 1)
}
fn check_substitution_input(atoms: &[AtomId], required: usize) -> Result<(), GraphError> {
    if atoms.is_empty() {
        return Err(GraphError(
            "OPSIN Bug: Must provide at least one substituable atom".into(),
        ));
    }
    if atoms.len() < required {
        return Err(GraphError(
            "OPSIN Bug: substitutableAtoms must be >= numberToBeSubstituted".into(),
        ));
    }
    Ok(())
}
pub fn is_substitution_ambiguous(
    graph: &Graph,
    atoms: &[AtomId],
    required: usize,
) -> Result<bool, GraphError> {
    check_substitution_input(atoms, required)?;
    if atoms.len() == required {
        return Ok(false);
    }
    if graph
        .fragment(graph.atom(atoms[0]).fragment)
        .default_in_atom
        .is_some_and(|preferred| atoms.iter().take(required).all(|atom| *atom == preferred))
    {
        return Ok(false);
    }
    let unique: HashSet<_> = atoms.iter().copied().collect();
    if unique.len() == 1 {
        return Ok(false);
    }
    if all_atoms_equivalent(graph, &unique.into_iter().collect::<Vec<_>>())?
        && (required == 1 || required == atoms.len() - 1)
    {
        return Ok(false);
    }
    Ok(true)
}
pub fn use_atom_environments_to_give_plausible_substitution(
    graph: &Graph,
    atoms: &[AtomId],
    required: usize,
) -> Result<Option<Vec<AtomId>>, GraphError> {
    check_substitution_input(atoms, required)?;
    if atoms.len() == required {
        return Ok(Some(atoms.to_vec()));
    }
    if let Some(preferred) = plausible_by_symmetry(graph, atoms, required)? {
        return Ok(Some(preferred));
    }
    let mut local = BTreeMap::<String, Vec<AtomId>>::new();
    for &atom in atoms {
        let a = graph.atom(atom);
        let valency = graph.determine_valency(atom, true);
        let current = graph.incoming_valency(atom) + a.out_valency;
        let number_bonds = valency - current + a.bonds.len() as i32;
        local
            .entry(format!(
                "{}\t{valency}\t{number_bonds}\t{}",
                a.element, a.spare_valency
            ))
            .or_default()
            .push(atom);
    }
    Ok(unique_matching_group(local.values(), required))
}
fn plausible_by_symmetry(
    graph: &Graph,
    atoms: &[AtomId],
    required: usize,
) -> Result<Option<Vec<AtomId>>, GraphError> {
    let analysis = analyse_relevant_atoms_and_bonds(graph, atoms)?;
    let mut environments = BTreeMap::<String, Vec<AtomId>>::new();
    for &atom in atoms {
        environments
            .entry(atom_environment(graph, &analysis, atom)?)
            .or_default()
            .push(atom);
    }
    let matches = environments
        .values()
        .filter(|group| group.len() == required)
        .count();
    if matches > 1 {
        return Ok(None);
    }
    if matches == 1 {
        return Ok(unique_matching_group(environments.values(), required));
    }
    let mut preferred = None;
    for group in environments
        .values()
        .filter(|group| group.len() == required * 2)
    {
        let mut seen = HashSet::new();
        let unique: Vec<_> = group
            .iter()
            .copied()
            .filter(|atom| seen.insert(*atom))
            .collect();
        if unique.len() == required {
            if preferred.is_some() {
                return Ok(None);
            }
            preferred = Some(unique);
        }
    }
    Ok(preferred)
}
fn unique_matching_group<'a>(
    groups: impl Iterator<Item = &'a Vec<AtomId>>,
    required: usize,
) -> Option<Vec<AtomId>> {
    let mut preferred = None;
    for group in groups.filter(|group| group.len() == required) {
        if preferred.is_some() {
            return None;
        }
        preferred = Some(group.clone());
    }
    preferred
}
