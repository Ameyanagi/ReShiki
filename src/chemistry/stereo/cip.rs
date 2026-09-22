//! Full CIP molecular preparation adapted from RDKit CIPMol.cpp/Mancude.cpp.
//! Copyright (C) 2020 Schrödinger, LLC. BSD-3-Clause; see licenses/rdkit/.
//! The labeling pass is separate; this adapter never edits the input state.
pub mod digraph;
mod mancude;
use super::perception::{RingCache, RingKind, State};
use crate::chemistry::{graph::Graph, kekulize, ranking, rings};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

#[cfg(test)]
mod tests;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid CIP molecule: {0}")]
    Invalid(String),
    #[error("CIP resource limit exceeded")]
    Limit,
    #[error("CIP graph expansion reached the 100,000-node limit")]
    Nodes,
    #[error("CIP bond {index} has unsupported noninteger order {order}")]
    BondOrder { index: usize, order: u8 },
    #[error(transparent)]
    Rings(#[from] rings::RingError),
}
fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}
fn at<T>(values: &[T], index: usize) -> Result<&T, Error> {
    values
        .get(index)
        .ok_or_else(|| invalid("Missing molecule index"))
}
fn at_mut<T>(values: &mut [T], index: usize) -> Result<&mut T, Error> {
    values
        .get_mut(index)
        .ok_or_else(|| invalid("Missing molecule index"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Fraction(pub u32, pub u32);
impl Fraction {
    fn new(numerator: u32, denominator: u32) -> Self {
        if denominator == 0 {
            return Self(0, 1);
        }
        let (mut a, mut b) = (numerator, denominator);
        while b != 0 {
            (a, b) = (b, a % b);
        }
        Self(numerator / a, denominator / a)
    }
}

/// Read-only molecular context for full CIP graph expansion. Ring and Kekulé
/// caches belong to this context; a caller's drawing and caches remain intact.
pub struct Molecule<'a> {
    state: &'a State,
    adjacent: Vec<Vec<(usize, usize)>>,
    ring_cache: RingCache,
    ring_bonds: Option<Vec<bool>>,
    bond_types: Option<Vec<u8>>,
    fractions: Option<Vec<Fraction>>,
}
impl<'a> Molecule<'a> {
    pub fn new(state: &'a State) -> Result<Self, Error> {
        let graph = &state.graph;
        graph
            .cached_valences(Some(&state.valences))
            .map_err(invalid)?;
        state.metadata.validate(graph).map_err(invalid)?;
        if state.directions.len() != graph.bonds.len()
            || state.conjugated.len() != graph.bonds.len()
            || state.hybridizations.len() != graph.atoms.len()
            || state.properties.atoms.len() != graph.atoms.len()
            || state.properties.bond_codes.len() != graph.bonds.len()
            || state.rings.kind == RingKind::None && !state.rings.atoms.is_empty()
        {
            return Err(invalid("Molecular state dimensions changed"));
        }
        let ring_bonds = if state.rings.kind == RingKind::None {
            None
        } else {
            Some(ring_flags(graph, &state.rings.atoms)?)
        };
        let mut adjacent = vec![Vec::new(); graph.atoms.len()];
        for (i, b) in graph.bonds.iter().enumerate() {
            at_mut(&mut adjacent, b.a)?.push((b.b, i));
            at_mut(&mut adjacent, b.b)?.push((b.a, i));
        }
        Ok(Self {
            state,
            adjacent,
            ring_cache: state.rings.clone(),
            ring_bonds,
            bond_types: None,
            fractions: None,
        })
    }
    fn prepare_bonds(&mut self) -> Result<(), Error> {
        if self.bond_types.is_some() {
            return Ok(());
        }
        let state = self.state;
        let graph = &state.graph;
        if !graph.atoms.iter().any(|a| a.aromatic)
            && !graph.bonds.iter().any(|b| b.aromatic || b.order == 4)
        {
            self.bond_types = Some(graph.bonds.iter().map(|b| b.order).collect());
            return Ok(());
        }
        let cache = graph.refresh_implicit(&state.valences).map_err(invalid)?;
        let ranking_rings = if self.ring_cache.kind == RingKind::None {
            rings::fast(graph).map_err(invalid)?.atoms
        } else {
            self.ring_cache.atoms.clone()
        };
        let ranks = ranking::rank_cached(
            graph,
            &ranking_rings,
            &state.metadata,
            ranking::Options {
                fragment: true,
                ..Default::default()
            },
            &cache,
        )
        .map_err(invalid)?;
        let assignment_rings = if self.ring_cache.kind == RingKind::None {
            let mut basis = rings::perceive(graph, Default::default())?;
            basis.atoms.truncate(basis.basis_count);
            basis.atoms
        } else {
            self.ring_cache.atoms.clone()
        };
        let partial = kekulize::partial_cached(
            graph,
            &assignment_rings,
            &state.directions,
            kekulize::Options {
                ranks: Some(&ranks),
                ..Default::default()
            },
            &cache,
        )
        .map_err(invalid)?;
        self.bond_types = Some(
            partial
                .assignment
                .graph
                .bonds
                .iter()
                .map(|b| b.order)
                .collect(),
        );
        Ok(())
    }
    pub fn bond_order(&mut self, index: usize) -> Result<u8, Error> {
        at(&self.state.graph.bonds, index)?;
        self.prepare_bonds()?;
        let order = *at(
            self.bond_types
                .as_deref()
                .ok_or_else(|| invalid("Missing bond orders"))?,
            index,
        )?;
        match order {
            0 | 5 => Ok(0),
            1 | 4 => Ok(1),
            2 | 3 => Ok(order),
            6 => Ok(4),
            _ => Err(Error::BondOrder { index, order }),
        }
    }
    pub fn is_in_ring(&mut self, index: usize) -> Result<bool, Error> {
        at(&self.state.graph.bonds, index)?;
        if self.ring_bonds.is_none() {
            if self.ring_cache.kind == RingKind::None {
                self.ring_cache = RingCache {
                    kind: RingKind::Fast,
                    atoms: rings::fast(&self.state.graph).map_err(invalid)?.atoms,
                };
            }
            self.ring_bonds = Some(ring_flags(&self.state.graph, &self.ring_cache.atoms)?);
        }
        at(
            self.ring_bonds
                .as_deref()
                .ok_or_else(|| invalid("Missing ring flags"))?,
            index,
        )
        .copied()
    }
    pub fn fraction(&mut self, index: usize) -> Result<Fraction, Error> {
        at(&self.state.graph.atoms, index)?;
        if self.fractions.is_none() {
            self.fractions = Some(mancude::calculate(self)?);
        }
        at(
            self.fractions
                .as_deref()
                .ok_or_else(|| invalid("Missing CIP fractions"))?,
            index,
        )
        .copied()
    }
}

fn ring_flags(graph: &Graph, rings: &[Vec<usize>]) -> Result<Vec<bool>, Error> {
    let pair = |a: usize, b: usize| (a.min(b), a.max(b));
    let lookup: HashMap<_, _> = graph
        .bonds
        .iter()
        .enumerate()
        .map(|(i, b)| (pair(b.a, b.b), i))
        .collect();
    let mut flags = vec![false; graph.bonds.len()];
    let mut total = 0_usize;
    for ring in rings {
        total = total.checked_add(ring.len()).ok_or(Error::Limit)?;
        if total > 2_000_000 {
            return Err(Error::Limit);
        }
        if ring.len() < 3 || ring.iter().collect::<HashSet<_>>().len() != ring.len() {
            return Err(invalid("Ring must contain at least three distinct atoms"));
        }
        for (&a, &b) in ring.iter().zip(ring.iter().cycle().skip(1)) {
            let edge = *lookup
                .get(&pair(a, b))
                .ok_or_else(|| invalid("Missing ring bond"))?;
            *at_mut(&mut flags, edge)? = true;
        }
    }
    Ok(flags)
}
