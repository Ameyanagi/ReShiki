//! Default graph sanitization order from RDKit MolOps.cpp (2026.03.6).
//! Copyright (C) 2001-2024 Greg Landrum and other RDKit contributors.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
//!
//! Operates on a detached graph. Parsing, stereo perception, canonical
//! identifiers and layout remain separate operations. No drawing is edited.
use super::{
    aromaticity,
    electronic::{self, Hybridization},
    graph::{Graph, Valence},
    kekulize::{self, Direction},
    normalize,
    ranking::{self, Metadata},
    rings, stereo,
};
use serde::Serialize;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Input,
    Cleanup,
    Metals,
    Properties,
    Rings,
    Kekulize,
    Radicals,
    Aromaticity,
    Conjugation,
    Hybridization,
    Atropisomers,
    Chirality,
    Hydrogens,
}

#[derive(Debug)]
pub struct Error {
    pub stage: Stage,
    pub message: String,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.stage, self.message)
    }
}
impl std::error::Error for Error {}
fn at_stage<T>(stage: Stage, result: Result<T, String>) -> Result<T, Error> {
    result.map_err(|message| Error { stage, message })
}

#[derive(Debug, Serialize)]
pub struct Sanitized {
    pub graph: Graph,
    pub metadata: Metadata,
    pub directions: Vec<Direction>,
    pub conjugated: Vec<bool>,
    pub hybridizations: Vec<Hybridization>,
    pub valences: Vec<Valence>,
    pub rings: Vec<Vec<usize>>,
    #[serde(skip)]
    pub canonical_retry: bool,
}

/// Validate and normalize a graph in the default sanitization order. Results
/// are returned only after every stage succeeds; inputs remain unchanged.
/// Ambiguous dense-ring pruning fails explicitly, preserving the current
/// backend boundary until a portable replacement is available.
pub fn sanitize(
    graph: &Graph,
    metadata: &Metadata,
    directions: &[Direction],
) -> Result<Sanitized, Error> {
    use Stage::*;
    at_stage(Input, graph.validate())?;
    at_stage(Input, metadata.validate(graph))?;
    if directions.len() != graph.bonds.len() {
        return Err(Error {
            stage: Input,
            message: "Invalid sanitization bond directions".into(),
        });
    }
    let graph = at_stage(Cleanup, normalize::functional_groups(graph))?;
    let graph = at_stage(Metals, normalize::organometallics(&graph, metadata, None))?;
    let cache = at_stage(Properties, graph.valences())?;
    let rings = rings::perceive(&graph, rings::Options::default())
        .map_err(|error| Error {
            stage: Rings,
            message: error.to_string(),
        })?
        .atoms;
    let mut attempt = at_stage(
        Kekulize,
        kekulize::if_possible_cached(
            &graph,
            &rings,
            directions,
            kekulize::Options::default(),
            Some(&cache),
        ),
    )?;
    let canonical_retry = !attempt.success;
    if canonical_retry {
        let cache = at_stage(
            Kekulize,
            attempt.assignment.graph.refresh_implicit(&attempt.cache),
        )?;
        let ranks = at_stage(
            Kekulize,
            ranking::rank_cached(
                &attempt.assignment.graph,
                &rings,
                metadata,
                ranking::Options {
                    fragment: true,
                    ..ranking::Options::default()
                },
                &cache,
            ),
        )?;
        attempt = at_stage(
            Kekulize,
            kekulize::assign_cached(
                &attempt.assignment.graph,
                &rings,
                &attempt.assignment.directions,
                kekulize::Options {
                    ranks: Some(&ranks),
                    ..kekulize::Options::default()
                },
                &cache,
            ),
        )?;
    }
    let mut graph = attempt.assignment.graph;
    let cache = attempt.cache;
    let radicals = at_stage(Radicals, graph.assign_radicals())?;
    for (atom, count) in graph.atoms.iter_mut().zip(radicals) {
        atom.radical_electrons = count;
    }
    let graph = at_stage(
        Aromaticity,
        aromaticity::perceive_cached(&graph, &rings, &cache),
    )?
    .graph;
    let conjugated = at_stage(
        Conjugation,
        electronic::conjugation_cached(&graph, Some(&cache)),
    )?;
    let tags = metadata
        .atoms
        .iter()
        .map(|a| a.chiral_tag)
        .collect::<Vec<_>>();
    let hybridizations = at_stage(
        Hybridization,
        electronic::hybridization_cached(&graph, &tags, &conjugated, Some(&cache)),
    )?;
    let metadata = at_stage(
        Atropisomers,
        stereo::atropisomers(&graph, metadata, &hybridizations, &rings),
    )?;
    let metadata = at_stage(
        Chirality,
        stereo::chirality_cached(&graph, &metadata, &hybridizations, Some(&cache)),
    )?;
    let graph = at_stage(Hydrogens, aromaticity::adjust_hydrogens(&graph, &cache))?;
    let valences = at_stage(Properties, graph.valences())?;
    Ok(Sanitized {
        graph,
        metadata,
        directions: attempt.assignment.directions,
        conjugated,
        hybridizations,
        valences,
        rings,
        canonical_retry,
    })
}
