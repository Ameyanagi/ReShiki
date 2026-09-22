//! Ordered ring selection and the no-template native ring constructor.
//!
//! Adapted from RDKit 2026.03.6 DepictUtils.cpp, EmbeddedFrag.cpp/.h and
//! RDGeneral/types.cpp. See licenses/rdkit/NOTICE for BSD-3-Clause attribution.
//! Ring perception, templates, neighbor setup and final layout are later stages.

mod embed;

use super::geometry::{self, Bounds, Point};
use crate::chemistry::{graph::Graph, ranking::Metadata, stereo::perception::RingCache};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_WORK: usize = 50_000_000;
pub const MAX_RING_ATOMS: usize = 1_000_000;
pub const MAX_RINGS: usize = 100_000;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("Invalid depiction ring input: {0}")]
    Invalid(&'static str),
    #[error("Depiction ring work or storage limit exceeded")]
    Limit,
    #[error(transparent)]
    Geometry(#[from] geometry::Error),
}

/// Ring indices in this API refer to positions in the selected ordered list,
/// not the source cache's indices. Common atoms retain native traversal order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NextRing {
    pub ring: usize,
    pub common_atoms: Vec<usize>,
}

/// Native EmbeddedAtom state shared by ring construction and attachment stages.
/// The map key is authoritative: a native no-angle attachment leaves `id` at 0.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddedAtom {
    pub id: usize,
    pub location: Point,
    pub normal: Point,
    pub angle: f64,
    pub neighbor1: Option<usize>,
    pub neighbor2: Option<usize>,
    pub cis_trans_neighbor: Option<usize>,
    pub counter_clockwise: bool,
    pub rotation_direction: i32,
    pub neighbors: Vec<usize>,
    pub density: f64,
    pub fixed: bool,
}

/// Ordered embedded fragment shared by depiction stages. The ring constructor
/// leaves bounds, neighbors and attachment points at their initial values;
/// neighbor setup and non-ring attachment fill the latter two separately.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fragment {
    pub atoms: BTreeMap<usize, EmbeddedAtom>,
    pub done: bool,
    pub bounds: Bounds,
    pub attachment_points: Vec<usize>,
}

struct Work(usize);
impl Work {
    fn spend(&mut self, amount: usize) -> Result<(), Error> {
        self.0 = self.0.checked_sub(amount).ok_or(Error::Limit)?;
        Ok(())
    }
    fn contains(&mut self, values: &[usize], value: usize) -> Result<bool, Error> {
        self.spend(values.len())?;
        Ok(values.contains(&value))
    }
}
fn at<T>(values: &[T], index: usize) -> Result<&T, Error> {
    values
        .get(index)
        .ok_or(Error::Invalid("index outside input"))
}

/// Borrow an explicit chemical graph, stereo metadata and ordered ring cache.
/// Only selected rings are inspected. No cache, chemistry or input is changed.
/// The cache kind is not consulted by the native no-template constructor.
pub struct Input<'a> {
    graph: &'a Graph,
    metadata: &'a Metadata,
    cache: &'a RingCache,
    selected: Vec<usize>,
    degrees: Vec<usize>,
    bonds: BTreeMap<(usize, usize), usize>,
    work_limit: usize,
}
impl<'a> Input<'a> {
    pub fn new(
        graph: &'a Graph,
        metadata: &'a Metadata,
        cache: &'a RingCache,
        selected: &[usize],
    ) -> Result<Self, Error> {
        if selected.len() > MAX_RINGS
            || cache.atoms.len() > MAX_RINGS
            || graph.atoms.len() > geometry::MAX_POINTS
            || graph.bonds.len() > 300_000
        {
            return Err(Error::Limit);
        }
        graph
            .validate()
            .map_err(|_| Error::Invalid("chemical graph"))?;
        metadata
            .validate(graph)
            .map_err(|_| Error::Invalid("stereo metadata"))?;
        let mut degrees = vec![0; graph.atoms.len()];
        let mut bonds = BTreeMap::new();
        for (id, bond) in graph.bonds.iter().enumerate() {
            for atom in [bond.a, bond.b] {
                let degree = degrees
                    .get_mut(atom)
                    .ok_or(Error::Invalid("bond endpoint"))?;
                *degree += 1;
            }
            bonds.insert((bond.a.min(bond.b), bond.a.max(bond.b)), id);
        }
        let mut storage = 0usize;
        for &id in selected {
            let ring = at(&cache.atoms, id)?;
            storage = storage.checked_add(ring.len()).ok_or(Error::Limit)?;
            if storage > MAX_RING_ATOMS {
                return Err(Error::Limit);
            }
            if ring.len() < 3 {
                return Err(Error::Invalid("ring needs at least three atoms"));
            }
            let mut seen = BTreeSet::new();
            let mut previous = *ring.last().ok_or(Error::Invalid("empty ring"))?;
            for &atom in ring {
                if atom >= graph.atoms.len() || !seen.insert(atom) {
                    return Err(Error::Invalid("invalid or repeated ring atom"));
                }
                if !bonds.contains_key(&(previous.min(atom), previous.max(atom))) {
                    return Err(Error::Invalid("ring is not in bond traversal order"));
                }
                previous = atom;
            }
        }
        Ok(Self {
            graph,
            metadata,
            cache,
            selected: selected.to_vec(),
            degrees,
            bonds,
            work_limit: MAX_WORK,
        })
    }

    /// Reduce the independent per-operation work budget; it cannot exceed the
    /// hard bound. Construction already enforces graph and ring storage limits.
    pub fn with_work_limit(mut self, limit: usize) -> Self {
        self.work_limit = limit.min(MAX_WORK);
        self
    }
    pub fn source_ring_ids(&self) -> &[usize] {
        &self.selected
    }
    fn ring(&self, index: usize) -> Result<&[usize], Error> {
        Ok(at(&self.cache.atoms, *at(&self.selected, index)?)?.as_slice())
    }
    fn bond(&self, a: usize, b: usize) -> Option<usize> {
        self.bonds.get(&(a.min(b), a.max(b))).copied()
    }
    fn work(&self) -> Work {
        Work(self.work_limit)
    }

    /// Fewest substituted atoms (degree > 2), then largest ring, then first.
    /// An empty ring selection has no first ring, matching native -1.
    pub fn pick_first(&self) -> Result<Option<usize>, Error> {
        self.first(&mut self.work())
    }
    fn first(&self, work: &mut Work) -> Result<Option<usize>, Error> {
        let mut result = None;
        let (mut minimum, mut size) = (100_000_000usize, 0usize);
        for index in 0..self.selected.len() {
            let ring = self.ring(index)?;
            work.spend(ring.len())?;
            let mut substitutions = 0;
            for &atom in ring {
                if *at(&self.degrees, atom)? > 2 {
                    substitutions += 1;
                }
            }
            if substitutions < minimum || (substitutions == minimum && ring.len() > size) {
                result = Some(index);
                minimum = substitutions;
                size = ring.len();
            }
        }
        Ok(result)
    }

    /// Ordered survivors of native one-at-a-time core-ring pruning.
    /// Preserve its two remembered atom IDs, including revisits after the
    /// intersection count exceeds two; a set-based rewrite changes behavior.
    pub fn core(&self) -> Result<Vec<usize>, Error> {
        self.core_with_budget(&mut { self.work_limit })
    }
    pub(super) fn core_with_budget(&self, remaining: &mut usize) -> Result<Vec<usize>, Error> {
        let mut work = Work((*remaining).min(self.work_limit));
        let result = self.core_work(&mut work);
        *remaining = work.0;
        result
    }
    fn core_work(&self, work: &mut Work) -> Result<Vec<usize>, Error> {
        let mut removed = vec![false; self.selected.len()];
        loop {
            let mut changed = false;
            for current in 0..self.selected.len() {
                work.spend(1)?;
                if *at(&removed, current)? {
                    continue;
                }
                let (mut count, mut first, mut second) = (0usize, None, None);
                for other in 0..self.selected.len() {
                    work.spend(1)?;
                    if other == current || *at(&removed, other)? {
                        continue;
                    }
                    for &atom in self.ring(current)? {
                        if !work.contains(self.ring(other)?, atom)? {
                            continue;
                        }
                        if Some(atom) != first && Some(atom) != second {
                            count += 1;
                            if first.is_none() {
                                first = Some(atom);
                            } else {
                                second = Some(atom);
                            }
                            if count == 2 {
                                break;
                            }
                        }
                    }
                }
                let adjacent = match (first, second) {
                    (Some(a), Some(b)) => self.bond(a, b).is_some(),
                    _ => false,
                };
                if count == 1 || (count == 2 && adjacent) {
                    *removed
                        .get_mut(current)
                        .ok_or(Error::Invalid("core ring index"))? = true;
                    changed = true;
                    break;
                }
            }
            if !changed {
                break;
            }
        }
        Ok(removed
            .iter()
            .enumerate()
            .filter_map(|(i, &removed)| (!removed).then_some(i))
            .collect())
    }

    /// Prefer the first ring with exactly two shared atoms; otherwise maximize
    /// overlap. Rotate wrapped common chains only in the latter branch.
    pub fn next(&self, done: &[usize]) -> Result<NextRing, Error> {
        self.next_with_work(done, &mut self.work())
    }
    fn next_with_work(&self, done: &[usize], work: &mut Work) -> Result<NextRing, Error> {
        if done.len() > MAX_RINGS {
            return Err(Error::Limit);
        }
        if done.is_empty()
            || self.selected.len() < 2
            || done.iter().any(|&id| id >= self.selected.len())
        {
            return Err(Error::Invalid("completed ring indices"));
        }
        let mut done_atoms = Vec::new();
        for index in 0..self.selected.len() {
            if !work.contains(done, index)? {
                continue;
            }
            for &atom in self.ring(index)? {
                if !work.contains(&done_atoms, atom)? {
                    done_atoms.push(atom);
                }
            }
        }
        let mut result = None;
        let mut common = Vec::new();
        for index in 0..self.selected.len() {
            if work.contains(done, index)? {
                continue;
            }
            let mut candidate = Vec::new();
            for &atom in self.ring(index)? {
                if work.contains(&done_atoms, atom)? {
                    candidate.push(atom);
                }
            }
            if candidate.len() == 2 {
                return Ok(NextRing {
                    ring: index,
                    common_atoms: candidate,
                });
            }
            if candidate.len() > common.len() {
                result = Some(index);
                common = candidate;
            }
        }
        let ring = result.ok_or(Error::Invalid("no remaining intersecting ring"))?;
        let source = self.ring(ring)?;
        let mut prefix = 0;
        for (&atom, &original) in common.iter().zip(source) {
            work.spend(1)?;
            if atom != original {
                break;
            }
            prefix += 1;
        }
        if prefix > 0 && prefix < common.len() {
            common.rotate_left(prefix);
        }
        Ok(NextRing {
            ring,
            common_atoms: common,
        })
    }
}
