//! Atomic full CIP labeling adapted from RDKit CIPLabeler.cpp (2026.03.6).
//! Copyright (C) 2020 Schrödinger, LLC. BSD-3-Clause; see licenses/rdkit/.
use super::{
    Error, Molecule, at, at_mut,
    configuration::{Configuration, PrimaryLabel, Target},
    digraph::{Descriptor, Digraph},
    invalid,
    rules::{Iterations, Rules},
};
use crate::{native_order, stereo::perception::State};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Options {
    /// Two absent or empty selections mean the whole molecule. A nonempty
    /// selection restricts labeling to the supplied indices on each side,
    /// matching the native Python wrapper's truthiness-based selection.
    pub atoms: Option<Vec<usize>>,
    pub bonds: Option<Vec<usize>>,
    /// Shared second-pass comparison budget; zero has the native unlimited meaning.
    pub max_iterations: u32,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            atoms: None,
            bonds: None,
            max_iterations: 1_250_000,
        }
    }
}

#[derive(Debug)]
pub struct Assignment {
    pub state: State,
    /// Computed neighbor orders and labels written by this pass. Unselected
    /// centers retain their existing labels; unresolved selected labels clear.
    pub labels: Vec<PrimaryLabel>,
}

fn selected(selection: Option<&[usize]>, size: usize) -> Result<Vec<usize>, Error> {
    let Some(selection) = selection else {
        return Ok((0..size).collect());
    };
    if selection.len() > 300_000 {
        return Err(Error::Limit);
    }
    let mut flags = vec![false; size];
    for &index in selection {
        *at_mut(&mut flags, index)? = true;
    }
    Ok(flags
        .into_iter()
        .enumerate()
        .filter_map(|(i, yes)| yes.then_some(i))
        .collect())
}

fn configurations(molecule: &Molecule<'_>, options: &Options) -> Result<Vec<Configuration>, Error> {
    let mut configurations = Vec::new();
    let state = molecule.state;
    let all = options.atoms.as_ref().is_none_or(Vec::is_empty)
        && options.bonds.as_ref().is_none_or(Vec::is_empty);
    let atoms = if all {
        None
    } else {
        Some(options.atoms.as_deref().unwrap_or(&[]))
    };
    let bonds = if all {
        None
    } else {
        Some(options.bonds.as_deref().unwrap_or(&[]))
    };
    for atom in selected(atoms, state.graph.atoms.len())? {
        if matches!(at(&state.metadata.atoms, atom)?.chiral_tag, 1 | 2) {
            configurations.push(Configuration::tetrahedral(molecule, atom)?);
        }
    }
    for bond in selected(bonds, state.graph.bonds.len())? {
        let endpoints = at(&state.graph.bonds, bond)?;
        let foci = [endpoints.a, endpoints.b];
        match at(&state.metadata.bonds, bond)?.stereo {
            2 | 4 => configurations.push(Configuration::sp2(molecule, bond, foci, 4)?),
            3 | 5 => configurations.push(Configuration::sp2(molecule, bond, foci, 5)?),
            cfg @ (6 | 7) => {
                configurations.push(Configuration::atropisomer(molecule, bond, foci, cfg)?)
            }
            _ => (),
        }
    }
    Ok(configurations)
}

// Bound each centre's graph storage as well as its node, visit-map and work
// limits. Only one graph is alive at a time.
const STORAGE_UNITS: usize = 16_000_000;
fn bounded(graph: &Digraph<'_>) -> Result<(), Error> {
    if graph.storage_units() > STORAGE_UNITS {
        return Err(Error::Limit);
    }
    Ok(())
}

fn auxiliary<'a>(
    configurations: &mut [Configuration],
    center: usize,
    molecule: &mut Molecule<'a>,
    graph: &mut Digraph<'a>,
    rules: &Rules,
    iterations: &mut Iterations,
) -> Result<(), Error> {
    let mut candidates = Vec::new();
    let mut keys = Vec::new();
    for (index, configuration) in configurations.iter().enumerate() {
        if index == center {
            continue;
        }
        let mut seen = false;
        for &atom in configuration.foci() {
            seen |= graph.seen_atom(atom)?;
        }
        if !seen {
            continue;
        }
        for node in graph.nodes_for_atom(molecule, configuration.focus()?)? {
            if graph.node(node)?.is_duplicate() {
                continue;
            }
            let mut low = node;
            if let Some(&other) = configuration.foci().get(1) {
                for edge in graph.edges_to_atom(molecule, node, other)? {
                    let other = graph.edge(edge)?.other(node)?;
                    if graph.node(other)?.distance < graph.node(node)?.distance {
                        low = other;
                    }
                }
            }
            if !graph.node(low)?.is_duplicate() {
                if candidates.len() >= 2_000_000 {
                    return Err(Error::Limit);
                }
                let distance = graph.node(low)?.distance;
                candidates.push((low, index, distance));
                keys.push(distance.checked_neg().ok_or(Error::Limit)?);
            }
        }
    }
    let order = native_order::indices(&keys).map_err(|e| match e {
        native_order::Error::Limit => Error::Limit,
        e => invalid(e.to_string()),
    })?;
    let mut queue = BTreeMap::new();
    let mut previous = i32::MAX;
    for index in order {
        let &(node, configuration, distance) = at(&candidates, index)?;
        if distance < previous {
            for (node, label) in std::mem::take(&mut queue) {
                graph.set_node_aux(node, label)?;
            }
            previous = distance;
        }
        let label = at_mut(configurations, configuration)?
            .label_at(molecule, graph, node, rules, iterations)?;
        // Native emplace keeps the first candidate for a node at this depth.
        queue.entry(node).or_insert(label);
    }
    for (node, label) in queue {
        graph.set_node_aux(node, label)?;
    }
    Ok(())
}

fn primary(
    configuration: &Configuration,
    descriptor: Descriptor,
    molecule: &Molecule<'_>,
) -> Result<PrimaryLabel, Error> {
    let label = configuration.primary_label(descriptor)?;
    if let (Target::Bond(bond), Some((_, atoms))) = (label.target, label.bond_stereo) {
        let endpoints = at(&molecule.state.graph.bonds, bond)?;
        for (endpoint, control) in [endpoints.a, endpoints.b].into_iter().zip(atoms) {
            if !at(&molecule.adjacent, endpoint)?
                .iter()
                .any(|&(neighbor, _)| neighbor == control)
            {
                return Err(invalid(
                    "CIP control atom is not adjacent to its bond endpoint",
                ));
            }
        }
    }
    Ok(label)
}

/// Evaluate on detached graphs and publish only a complete successful result.
/// Input chemical state, cached labels and ring order remain untouched on error.
pub fn assign(input: &State, options: &Options) -> Result<Assignment, Error> {
    let mut molecule = Molecule::new(input)?;
    let mut configurations = configurations(&molecule, options)?;
    let count = configurations.len();
    let seen_units = input
        .graph
        .atoms
        .len()
        .div_ceil(std::mem::size_of::<usize>());
    // Every centre builds a graph with an atom-sized visit map: bound the total.
    if count
        .checked_mul(seen_units + 64)
        .is_none_or(|n| n > STORAGE_UNITS)
    {
        return Err(Error::Limit);
    }
    let mut labels = Vec::new();
    let constitutional = Rules::constitutional();
    let full = Rules::full();
    let mut iterations = Iterations::new(options.max_iterations);
    // Each centre needs only its own graph (auxiliary labels are computed on
    // it), so build it here and drop it once the centre is labelled. A fast
    // pass with a small private budget comes first; centres it cannot decide
    // continue on the same graph with the shared budget and the full rules.
    for i in 0..count {
        let mut graph = at(&configurations, i)?.make_digraph(&molecule)?;
        bounded(&graph)?;
        let fast = at_mut(&mut configurations, i)?.label(
            &mut molecule,
            &mut graph,
            &constitutional,
            &mut Iterations::new(2000),
        );
        bounded(&graph)?;
        let descriptor = match fast {
            Ok(Descriptor::Unknown) | Err(Error::Iterations) => {
                let mut descriptor = at_mut(&mut configurations, i)?.label(
                    &mut molecule,
                    &mut graph,
                    &constitutional,
                    &mut iterations,
                )?;
                bounded(&graph)?;
                if descriptor == Descriptor::Unknown {
                    auxiliary(
                        &mut configurations,
                        i,
                        &mut molecule,
                        &mut graph,
                        &full,
                        &mut iterations,
                    )?;
                    bounded(&graph)?;
                    descriptor = at_mut(&mut configurations, i)?.label(
                        &mut molecule,
                        &mut graph,
                        &full,
                        &mut iterations,
                    )?;
                    bounded(&graph)?;
                }
                descriptor
            }
            Ok(descriptor) => descriptor,
            Err(error) => return Err(error),
        };
        if descriptor != Descriptor::Unknown {
            labels.push(primary(at(&configurations, i)?, descriptor, &molecule)?);
        }
    }
    let mut state = input.clone();
    state.rings = molecule.ring_cache.clone();
    for configuration in &configurations {
        match configuration.target() {
            Target::Atom(atom) => at_mut(&mut state.properties.atoms, atom)?.cip_code = None,
            Target::Bond(bond) => *at_mut(&mut state.properties.bond_codes, bond)? = None,
        }
    }
    for label in &labels {
        let code = match label.descriptor {
            Descriptor::R => "R",
            Descriptor::S => "S",
            Descriptor::PseudoR => "r",
            Descriptor::PseudoS => "s",
            Descriptor::E => "E",
            Descriptor::Z => "Z",
            Descriptor::SeqTrans => "e",
            Descriptor::SeqCis => "z",
            Descriptor::M => "M",
            Descriptor::P => "P",
            Descriptor::PseudoM => "m",
            Descriptor::PseudoP => "p",
            _ => return Err(invalid("Unsupported completed CIP descriptor")),
        }
        .to_owned();
        match label.target {
            Target::Atom(atom) => at_mut(&mut state.properties.atoms, atom)?.cip_code = Some(code),
            Target::Bond(bond) => {
                *at_mut(&mut state.properties.bond_codes, bond)? = Some(code);
                if let Some((stereo, atoms)) = label.bond_stereo {
                    let metadata = at_mut(&mut state.metadata.bonds, bond)?;
                    metadata.stereo = stereo;
                    metadata.stereo_atoms = atoms.to_vec();
                }
            }
        }
    }
    Ok(Assignment { state, labels })
}
