//! Detached native initial-coordinate construction and fragment expansion.
//!
//! RDKit 2026.03.6 RDDepictor.cpp and EmbeddedFrag.cpp, BSD-3-Clause;
//! see licenses/rdkit/NOTICE. Collision correction and final coordinate output
//! are subsequent stages and are deliberately not approximated here.
#[cfg(test)]
mod budget_tests;
mod initial;
mod merging;
mod traversal;

use super::{
    attachment::{self, AtomData, Work},
    geometry::{self, Coordinates},
    rings::{self, Fragment},
    seeds, templates,
};
use crate::chemistry::{
    graph::Graph,
    ranking::Metadata,
    stereo::perception::{RingCache, State},
};
use serde::Serialize;
use std::{
    collections::BTreeSet,
    ops::{Deref, DerefMut},
};

pub const MAX_WORK: usize = 50_000_000;
pub const MAX_STORAGE: usize = 2_000_000;
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid depiction expansion input: {0}")]
    Invalid(&'static str),
    #[error("Depiction expansion work or storage limit exceeded")]
    Limit,
    #[error("Depiction chemical preparation failed: {0}")]
    Chemistry(String),
    #[error(transparent)]
    Perception(#[from] crate::chemistry::rings::RingError),
    #[error(transparent)]
    Attachment(#[from] attachment::Error),
    #[error(transparent)]
    Geometry(#[from] geometry::Error),
    #[error(transparent)]
    Rings(#[from] rings::Error),
    #[error(transparent)]
    Seeds(#[from] seeds::Error),
    #[error(transparent)]
    Templates(#[from] templates::Error),
}
/// A single monotonically consumed allocation/work allowance per detached
/// operation. Charging new records before insertion also bounds transient
/// copies; releasing a fragment does not replenish the allocation allowance.
struct Budget {
    work: Work,
    storage: usize,
}
impl Budget {
    fn new(work: usize) -> Self {
        Self {
            work: Work(work),
            storage: MAX_STORAGE,
        }
    }
    fn reserve(&mut self, count: usize) -> Result<()> {
        self.storage = self.storage.checked_sub(count).ok_or(Error::Limit)?;
        Ok(())
    }
    fn fragment(&mut self, fragment: &Fragment) -> Result<()> {
        self.reserve(
            fragment
                .atoms
                .len()
                .checked_add(fragment.attachment_points.len())
                .ok_or(Error::Limit)?,
        )?;
        for atom in fragment.atoms.values() {
            self.spend(1)?;
            self.reserve(atom.neighbors.len())?;
        }
        Ok(())
    }
}
// Primitive mutation sessions consume this same work counter; only expansion
// owns the additional aggregate allocation allowance.
impl Deref for Budget {
    type Target = Work;
    fn deref(&self) -> &Work {
        &self.work
    }
}
impl DerefMut for Budget {
    fn deref_mut(&mut self) -> &mut Work {
        &mut self.work
    }
}
fn depict_ranks(graph: &Graph) -> Result<Vec<i32>> {
    if graph.atoms.len() > geometry::MAX_POINTS || graph.bonds.len() > 300_000 {
        return Err(Error::Limit);
    }
    let mut ranks = graph
        .atoms
        .iter()
        .map(|a| {
            100 * if a.atomic_number == 1 {
                1000
            } else {
                i32::from(a.atomic_number)
            }
        })
        .collect::<Vec<_>>();
    for bond in &graph.bonds {
        for id in [bond.a, bond.b] {
            let rank = ranks.get_mut(id).ok_or(Error::Invalid("bond endpoint"))?;
            *rank = rank.checked_add(1).ok_or(Error::Limit)?;
        }
    }
    Ok(ranks)
}
type Result<T> = std::result::Result<T, Error>;
fn at<T>(values: &[T], index: usize) -> Result<&T> {
    values
        .get(index)
        .ok_or(Error::Invalid("index outside input"))
}

#[derive(Debug, Clone, Copy)]
pub struct Options {
    pub bond_length: f64,
    pub ideal_lengths: seeds::IdealLengths,
    pub use_ring_templates: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            bond_length: 1.5,
            ideal_lengths: seeds::IdealLengths {
                square_planar: 1.5,
                trigonal_bipyramidal: 1.5,
                octahedral: 1.5,
            },
            use_ring_templates: true,
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct Initial {
    /// Original getAtomDepictRank values, captured before ring/stereo preparation.
    pub depict_ranks: Vec<i32>,
    /// Detached initial-stage state. The original chemical state stays borrowed.
    pub state: State,
    pub fragments: Vec<Fragment>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Seeded {
    pub fragments: Vec<Fragment>,
    pub non_embedded: Vec<usize>,
    pub pre_specified: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct Merged {
    pub fragment: Fragment,
    pub incoming: Fragment,
    pub common: Vec<usize>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Expanded {
    pub fragments: Vec<Fragment>,
    pub non_embedded: Vec<usize>,
}
/// Post-stereo prepared properties for direct seed and merge observations.
/// The higher compute_initial wrapper performs ring and stereo preparation.
pub struct Input<'a> {
    graph: &'a Graph,
    metadata: &'a Metadata,
    cache: &'a RingCache,
    attachment: attachment::Input<'a>,
    seeds: seeds::Input<'a>,
    templates: templates::Input<'a>,
    ranks: Vec<i32>,
    work_limit: usize,
}
impl<'a> Input<'a> {
    pub fn new(
        graph: &'a Graph,
        metadata: &'a Metadata,
        cache: &'a RingCache,
        data: &'a [AtomData],
    ) -> Result<Self> {
        let attachment = attachment::Input::new(graph, data)?;
        let seeds = seeds::Input::new(graph, metadata, data, cache)?;
        let templates = templates::Input::new(graph, metadata, cache, data)?;
        let ranks = depict_ranks(graph)?;
        Ok(Self {
            graph,
            metadata,
            cache,
            attachment,
            seeds,
            templates,
            ranks,
            work_limit: MAX_WORK,
        })
    }
    fn with_rank_properties(mut self, ranks: super::ranks::Input<'_>) -> Result<Self> {
        self.attachment = self.attachment.with_rank_properties(ranks)?;
        self.seeds = self.seeds.with_rank_properties(ranks)?;
        self.templates = self.templates.with_rank_properties(ranks)?;
        Ok(self)
    }
    pub fn with_work_limit(mut self, limit: usize) -> Self {
        self.work_limit = limit.min(MAX_WORK);
        self.attachment = self.attachment.with_work_limit(self.work_limit);
        self.seeds = self.seeds.with_work_limit(self.work_limit);
        self.templates = self.templates.with_work_limit(self.work_limit);
        self
    }
    fn validate_fragments(&self, fragments: &[Fragment], work: &mut Budget) -> Result<()> {
        if fragments.len() > self.graph.atoms.len().saturating_mul(3).saturating_add(1) {
            return Err(Error::Limit);
        }
        let mut stored = 0usize;
        for f in fragments {
            stored = stored
                .checked_add(f.atoms.len() + f.attachment_points.len())
                .ok_or(Error::Limit)?;
            for a in f.atoms.values() {
                stored = stored.checked_add(a.neighbors.len()).ok_or(Error::Limit)?;
            }
            if stored > MAX_STORAGE {
                return Err(Error::Limit);
            }
            self.attachment.validate(f, work)?;
        }
        Ok(())
    }
    fn remaining(&self, ids: &[usize]) -> Result<BTreeSet<usize>> {
        if ids.len() > self.graph.atoms.len() {
            return Err(Error::Limit);
        }
        let mut remaining = BTreeSet::new();
        for &id in ids {
            at(&self.graph.atoms, id)?;
            if !remaining.insert(id) {
                return Err(Error::Invalid("repeated unembedded atom"));
            }
        }
        Ok(remaining)
    }
}

/// Native computeInitialCoords, ending before collision correction. The source
/// state is never changed, including ring/stereo preparation failures.
pub fn compute_initial(
    input: &State,
    chiral_ranks: &[Option<u32>],
    coordinates: Option<&Coordinates>,
    options: Options,
) -> Result<Initial> {
    compute_initial_with_work(input, chiral_ranks, coordinates, options, &mut { MAX_WORK })
}

/// One allowance covers chemical preparation, all seeds, and expansion.
/// The caller's excess allowance above the stage cap is retained; failures
/// consume completed work while leaving the source state untouched.
pub(crate) fn compute_initial_with_work(
    input: &State,
    chiral_ranks: &[Option<u32>],
    coordinates: Option<&Coordinates>,
    options: Options,
    remaining: &mut usize,
) -> Result<Initial> {
    compute_initial_ranks_with_work(
        input,
        super::ranks::Input::Numeric(chiral_ranks),
        coordinates,
        options,
        remaining,
    )
}

pub(crate) fn compute_initial_ranks_with_work(
    input: &State,
    rank_properties: super::ranks::Input<'_>,
    coordinates: Option<&Coordinates>,
    options: Options,
    remaining: &mut usize,
) -> Result<Initial> {
    let initial = (*remaining).min(MAX_WORK);
    let mut work = Budget::new(initial);
    let result = compute_initial_inner(input, rank_properties, coordinates, options, &mut work);
    let used = initial.checked_sub(work.0).ok_or(Error::Limit)?;
    *remaining = remaining.checked_sub(used).ok_or(Error::Limit)?;
    result
}

fn compute_initial_inner(
    input: &State,
    rank_properties: super::ranks::Input<'_>,
    coordinates: Option<&Coordinates>,
    options: Options,
    work: &mut Budget,
) -> Result<Initial> {
    if rank_properties.len() != input.graph.atoms.len() {
        return Err(Error::Invalid("chiral rank count"));
    }
    if input.graph.atoms.len() > geometry::MAX_POINTS || input.graph.bonds.len() > 300_000 {
        return Err(Error::Limit);
    }
    work.spend(input.graph.atoms.len())?;
    work.spend(input.graph.bonds.len())?;
    for group in &input.metadata.groups {
        work.spend(1)?;
        work.spend(group.atoms.len())?;
        work.spend(group.bonds.len())?;
    }
    let ranks = depict_ranks(&input.graph)?;
    input.graph.validate().map_err(Error::Chemistry)?;
    input
        .metadata
        .validate(&input.graph)
        .map_err(Error::Chemistry)?;
    let (atoms, bonds) = (input.graph.atoms.len(), input.graph.bonds.len());
    if input.valences.len() != atoms
        || input.hybridizations.len() != atoms
        || input.properties.atoms.len() != atoms
        || input.properties.bond_codes.len() != bonds
        || input.directions.len() != bonds
        || input.conjugated.len() != bonds
    {
        return Err(Error::Invalid("chemical annotation count"));
    }
    let mut members = 0usize;
    for properties in &input.properties.atoms {
        work.spend(1)?;
        if properties.cip_code.as_ref().is_some_and(|s| s.len() > 1024) {
            return Err(Error::Limit);
        }
        members = members
            .checked_add(properties.ring_members.as_ref().map_or(0, Vec::len))
            .ok_or(Error::Limit)?;
        if members > MAX_STORAGE {
            return Err(Error::Limit);
        }
        work.spend(properties.ring_members.as_ref().map_or(0, Vec::len))?;
        work.spend(properties.cip_code.as_ref().map_or(0, String::len))?;
    }
    if input
        .properties
        .bond_codes
        .iter()
        .flatten()
        .any(|s| s.len() > 1024)
    {
        return Err(Error::Limit);
    }
    for code in &input.properties.bond_codes {
        work.spend(1)?;
        work.spend(code.as_ref().map_or(0, String::len))?;
    }
    let rings = crate::chemistry::rings::perceive_with_work(
        &input.graph,
        crate::chemistry::rings::Options {
            include_dative: true,
            include_hydrogen: false,
        },
        &mut work.0,
    )?;
    // Original cached rings are replaced before stereo assignment. Do not clone
    // an arbitrarily large discarded cache merely to overwrite it afterwards.
    let mut state = State {
        graph: input.graph.clone(),
        metadata: input.metadata.clone(),
        directions: input.directions.clone(),
        valences: input.valences.clone(),
        conjugated: input.conjugated.clone(),
        hybridizations: input.hybridizations.clone(),
        properties: input.properties.clone(),
        rings: RingCache {
            kind: crate::chemistry::stereo::perception::RingKind::Symmetric,
            atoms: rings.atoms,
        },
    };
    state = crate::chemistry::stereo::perception::perceive_prepared_with_work(
        &state,
        crate::chemistry::stereo::perception::Options {
            clean: false,
            force: false,
            flag_possible: false,
        },
        &mut work.0,
    )
    .map_err(Error::Chemistry)?;
    // Account for data collection and the bounded constructor index passes.
    work.spend(state.graph.atoms.len())?;
    work.spend(state.graph.bonds.len())?;
    for ring in &state.rings.atoms {
        work.spend(1)?;
        work.spend(ring.len())?;
    }
    let data = state
        .hybridizations
        .iter()
        .enumerate()
        .map(|(i, &hybridization)| {
            Ok(AtomData {
                hybridization,
                cip_rank: at(&state.properties.atoms, i)?.cip_rank,
                chiral_rank: None,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    work.spend(state.graph.atoms.len().checked_mul(3).ok_or(Error::Limit)?)?;
    let mut prepared = Input::new(&state.graph, &state.metadata, &state.rings, &data)?
        .with_rank_properties(rank_properties)?;
    prepared.ranks = ranks;
    let fragments = prepared.initial_with_budget(coordinates, options, work)?;
    Ok(Initial {
        depict_ranks: prepared.ranks,
        state,
        fragments,
    })
}
