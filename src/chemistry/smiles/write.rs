//! Plain molecular SMILES from RDKit SmilesWrite.cpp (2026.03.6).
//! Copyright (C) 2002-2025 Greg Landrum and other RDKit contributors.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
mod fragments;
use super::{stereo, symbols, traversal};
use crate::chemistry::{
    kekulize::{self, Direction},
    ranking, rings,
    stereo::perception::{self, RingCache, RingKind, State},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid molecular SMILES state: {0}")]
    Invalid(String),
    #[error("Molecular SMILES resource limit exceeded")]
    Limit,
    #[error(transparent)]
    Rings(#[from] rings::RingError),
    #[error(transparent)]
    Traversal(#[from] traversal::Error),
    #[error(transparent)]
    Stereo(#[from] stereo::Error),
    #[error(transparent)]
    Symbols(#[from] symbols::Error),
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Options {
    pub canonical: bool,
    pub root: Option<usize>,
    pub clean_stereo: bool,
    pub ignore_maps: bool,
    pub include_dative: bool,
    pub symbols: symbols::Options,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            canonical: true,
            root: None,
            clean_stereo: true,
            ignore_maps: false,
            include_dative: true,
            symbols: symbols::Options::default(),
        }
    }
}
fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}
fn at<T>(values: &[T], id: usize) -> Result<&T, Error> {
    values
        .get(id)
        .ok_or_else(|| invalid("Missing molecular item"))
}
fn put<T>(values: &mut [T], id: usize) -> Result<&mut T, Error> {
    values
        .get_mut(id)
        .ok_or_else(|| invalid("Missing molecular item"))
}

/// Serialize an entire molecular state without changing it. Stereo caches use
/// the computed-property semantics of preparation/perception. Random traversal,
/// queries and CX extensions are not part of this plain SMILES interface.
pub fn write(input: &State, options: Options) -> Result<traversal::Output, Error> {
    input.graph.validate().map_err(invalid)?;
    let properties = vec![symbols::AtomProperties::default(); input.graph.atoms.len()];
    with_symbols(input, &properties, options)
}

/// Preserve custom atom symbols and supplemental labels when explicitly supplied.
pub fn with_symbols(
    input: &State,
    properties: &[symbols::AtomProperties<'_>],
    options: Options,
) -> Result<traversal::Output, Error> {
    validate(input)?;
    if properties.len() != input.graph.atoms.len() {
        return Err(invalid("Atom symbol property count changed"));
    }
    if !input.graph.atoms.is_empty() && options.root.is_some_and(|a| a >= input.graph.atoms.len()) {
        return Err(invalid("Root atom is outside the molecule"));
    }
    let mut output = Vec::new();
    let mut output_size = 0usize;
    for fragment in fragments::split(input)? {
        let props = fragment
            .atoms
            .iter()
            .map(|&a| at(properties, a).copied())
            .collect::<Result<Vec<_>, _>>()?;
        // Preserve the pinned native root translation, including its rejection
        // of an out-of-range root in an interleaved disconnected component.
        let root = options
            .root
            .filter(|a| fragment.atoms.binary_search(a).is_ok())
            .map(|a| {
                a.checked_sub(*fragment.atoms.first().ok_or(Error::Limit)?)
                    .ok_or(Error::Limit)
            })
            .transpose()?;
        let mut part = component(fragment.state, &props, Options { root, ..options })?;
        part.atom_order = part
            .atom_order
            .iter()
            .map(|&a| at(&fragment.atoms, a).copied())
            .collect::<Result<_, _>>()?;
        part.bond_order = part
            .bond_order
            .iter()
            .map(|&b| at(&fragment.bonds, b).copied())
            .collect::<Result<_, _>>()?;
        output_size = output_size
            .checked_add(part.text.len())
            .and_then(|size| size.checked_add(1))
            .ok_or(Error::Limit)?;
        if output_size > 16 * 1024 * 1024 {
            return Err(Error::Limit);
        }
        output.push(part);
    }
    if options.canonical {
        output.sort_unstable_by(|a, b| {
            (&a.text, &a.atom_order, &a.bond_order).cmp(&(&b.text, &b.atom_order, &b.bond_order))
        });
    }
    let mut result = traversal::Output {
        text: String::new(),
        atom_order: Vec::new(),
        bond_order: Vec::new(),
    };
    for (index, part) in output.into_iter().enumerate() {
        let size = result
            .text
            .len()
            .checked_add(part.text.len())
            .and_then(|s| s.checked_add(1))
            .ok_or(Error::Limit)?;
        if size > 16 * 1024 * 1024 {
            return Err(Error::Limit);
        }
        if index > 0 {
            result.text.push('.');
        }
        result.text.push_str(&part.text);
        result.atom_order.extend(part.atom_order);
        result.bond_order.extend(part.bond_order);
    }
    Ok(result)
}

fn component(
    mut state: State,
    props: &[symbols::AtomProperties<'_>],
    options: Options,
) -> Result<traversal::Output, Error> {
    state.valences = state.graph.provisional_valences().map_err(invalid)?;
    let maps = state
        .metadata
        .atoms
        .iter()
        .map(|a| a.map_number)
        .collect::<Vec<_>>();
    if options.ignore_maps {
        for atom in &mut state.metadata.atoms {
            atom.map_number = 0;
            atom.map_present = false;
        }
    }
    if options.symbols.isomeric && state.properties.done.is_none() {
        state = perception::perceive(
            &state,
            perception::Options {
                clean: options.clean_stereo,
                force: false,
                flag_possible: false,
            },
        )
        .map_err(invalid)?;
    }
    state.metadata.groups.clear();
    for (direction, meta) in state.directions.iter_mut().zip(&mut state.metadata.bonds) {
        if matches!(direction, Direction::Unknown | Direction::EitherDouble) {
            *direction = Direction::None;
        }
        if meta.stereo == 1 {
            meta.stereo = 0;
        }
    }
    if !options.include_dative {
        let mut changed = vec![false; state.graph.atoms.len()];
        for bond in &mut state.graph.bonds {
            if bond.order == 5 {
                bond.order = 1;
                *put(&mut changed, bond.a)? = true;
            }
        }
        // Only the explicit cache changes; the old implicit count is retained.
        if changed.contains(&true) {
            let refreshed = state.graph.provisional_valences().map_err(invalid)?;
            for (i, valence) in state.valences.iter_mut().enumerate() {
                if *at(&changed, i)? {
                    valence.explicit_valence = at(&refreshed, i)?.explicit_valence;
                }
            }
        }
    }
    let ranks = if options.canonical {
        let was_empty = state.rings.kind == RingKind::None;
        let ranking_rings = if was_empty {
            rings::fast(&state.graph).map_err(invalid)?.atoms
        } else {
            state.rings.atoms.clone()
        };
        ranking::rank_cached(
            &state.graph,
            &ranking_rings,
            &state.metadata,
            ranking::Options {
                include_chirality: options.symbols.isomeric,
                include_isotopes: options.symbols.isomeric,
                include_stereo_groups: options.symbols.isomeric,
                ..ranking::Options::default()
            },
            &state.valences,
        )
        .map_err(invalid)?
    } else {
        (0..u32::try_from(state.graph.atoms.len()).map_err(|_| Error::Limit)?).collect()
    };
    if options.canonical && options.ignore_maps {
        for (atom, number) in state.metadata.atoms.iter_mut().zip(maps) {
            atom.map_number = number;
            atom.map_present = number != 0;
        }
    }
    let start = options
        .root
        .or_else(|| {
            ranks
                .iter()
                .enumerate()
                .min_by_key(|&(_, r)| r)
                .map(|(i, _)| i)
        })
        .ok_or_else(|| invalid("Empty component"))?;
    if options.symbols.kekule {
        state.valences = state
            .graph
            .refresh_implicit(&state.valences)
            .map_err(invalid)?;
        let kekule_ranks = if state.graph.atoms.iter().any(|a| a.aromatic)
            || state.graph.bonds.iter().any(|b| b.aromatic)
        {
            let ranking_rings = if state.rings.kind == RingKind::None {
                rings::fast(&state.graph).map_err(invalid)?.atoms
            } else {
                state.rings.atoms.clone()
            };
            let ranks = ranking::rank_cached(
                &state.graph,
                &ranking_rings,
                &state.metadata,
                ranking::Options {
                    fragment: true,
                    ..ranking::Options::default()
                },
                &state.valences,
            )
            .map_err(invalid)?;
            Some(ranks)
        } else {
            None
        };
        if state.rings.kind == RingKind::None {
            ensure_basis(&mut state)?;
        }
        let assigned = kekulize::assign_cached(
            &state.graph,
            &state.rings.atoms,
            &state.directions,
            kekulize::Options {
                ranks: kekule_ranks.as_deref(),
                ..kekulize::Options::default()
            },
            &state.valences,
        )
        .map_err(invalid)?;
        state.graph = assigned.assignment.graph;
        state.directions = assigned.assignment.directions;
        state.valences = assigned.cache;
    }
    if !options.canonical {
        state.properties.done = Some(true);
    }
    if state.properties.done.is_none() {
        state = perception::perceive(
            &state,
            perception::Options {
                clean: false,
                force: false,
                flag_possible: false,
            },
        )
        .map_err(invalid)?;
    }
    ensure_basis(&mut state)?;
    let ring_bonds = ring_bonds(&state)?;
    let walk = traversal::build(&state.graph, &ring_bonds, &ranks, start)?;
    let broken = props.iter().map(|p| p.broken_chirality).collect::<Vec<_>>();
    let adjusted = stereo::canonicalize(&state, &walk, &broken, options.symbols.isomeric)?;
    let writer = symbols::Writer::new(&state.graph, &adjusted.metadata, Some(&state.valences))?;
    Ok(walk.render(&writer, &adjusted.directions, props, options.symbols)?)
}
fn ensure_basis(state: &mut State) -> Result<(), Error> {
    if !matches!(state.rings.kind, RingKind::Symmetric | RingKind::Basis) {
        let mut found = rings::perceive(&state.graph, rings::Options::default())?;
        found.atoms.truncate(found.basis_count);
        state.rings = RingCache {
            kind: RingKind::Basis,
            atoms: found.atoms,
        };
    }
    Ok(())
}
fn ring_bonds(state: &State) -> Result<Vec<bool>, Error> {
    let edges = state
        .graph
        .bonds
        .iter()
        .enumerate()
        .map(|(i, b)| ((b.a.min(b.b), b.a.max(b.b)), i))
        .collect::<HashMap<_, _>>();
    let mut result = vec![false; state.graph.bonds.len()];
    let mut storage = 0usize;
    for ring in &state.rings.atoms {
        storage = storage.checked_add(ring.len()).ok_or(Error::Limit)?;
        if ring.len() < 3 || storage > 2_000_000 {
            return Err(invalid("Invalid ring storage"));
        }
        for (&a, &b) in ring.iter().zip(ring.iter().cycle().skip(1)) {
            let edge = *edges
                .get(&(a.min(b), a.max(b)))
                .ok_or_else(|| invalid("Ring edge is missing"))?;
            *put(&mut result, edge)? = true;
        }
    }
    Ok(result)
}
fn validate(state: &State) -> Result<(), Error> {
    state
        .graph
        .cached_valences(Some(&state.valences))
        .map_err(invalid)?;
    state.metadata.validate(&state.graph).map_err(invalid)?;
    let (n, e) = (state.graph.atoms.len(), state.graph.bonds.len());
    if state.directions.len() != e
        || state.conjugated.len() != e
        || state.hybridizations.len() != n
        || state.properties.atoms.len() != n
        || state.properties.bond_codes.len() != e
    {
        return Err(invalid("Molecular annotation count changed"));
    }
    let mut storage = 0usize;
    for (props, meta) in state.properties.atoms.iter().zip(&state.metadata.atoms) {
        if props.ring_members.is_some() != meta.ring_stereo {
            return Err(invalid("Ring stereo cache changed"));
        }
        if let Some(members) = &props.ring_members {
            storage = storage.checked_add(members.len()).ok_or(Error::Limit)?;
            if storage > 2_000_000
                || members
                    .iter()
                    .any(|&v| v == 0 || v.unsigned_abs() as usize > n)
            {
                return Err(invalid("Invalid ring stereo members"));
            }
        }
    }
    ring_bonds(state)?;
    Ok(())
}
