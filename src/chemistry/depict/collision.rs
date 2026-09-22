//! Deterministic collision correction from RDKit 2026.03.6 EmbeddedFrag.cpp,
//! DepictUtils.cpp and Matrices.cpp. BSD-3-Clause; see licenses/rdkit/NOTICE.
//! Operations return detached state, including the native density side effects.

mod repair;

use super::{
    geometry::{self, Point},
    rings::{EmbeddedAtom, Fragment},
};
use crate::chemistry::{graph::Graph, ranking::Metadata, stereo::perception::RingCache};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub const MAX_WORK: usize = 50_000_000;
const MAX_DISTANCES: usize = 4_000_000;
const MAX_PAIRS: usize = 1_000_000;
const COLLISION_THRESHOLD: f64 = 0.70;
const HETEROATOM_SCALE: f64 = 1.3;
const MAX_ITERATIONS: usize = 15;
const MAX_FLIPS: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("Invalid depiction collision input: {0}")]
    Invalid(&'static str),
    #[error("Depiction collision work or storage limit exceeded")]
    Limit,
    #[error(transparent)]
    Geometry(#[from] geometry::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    BondAndSpiroFlip,
    OpenAngles,
    ShortenBonds,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub fragment: Fragment,
    pub pairs: Vec<(usize, usize)>,
    pub total_density: f64,
}

pub(super) struct Session {
    remaining: usize,
    distances: BTreeMap<usize, Vec<usize>>,
}
impl Session {
    fn spend(&mut self, count: usize) -> Result<(), Error> {
        self.remaining = self.remaining.checked_sub(count).ok_or(Error::Limit)?;
        Ok(())
    }
}
fn at<T>(values: &[T], id: usize) -> Result<&T, Error> {
    values.get(id).ok_or(Error::Invalid("index outside input"))
}
fn atom(value: &Fragment, id: usize) -> Result<&EmbeddedAtom, Error> {
    value
        .atoms
        .get(&id)
        .ok_or(Error::Invalid("missing embedded atom"))
}
fn atom_mut(value: &mut Fragment, id: usize) -> Result<&mut EmbeddedAtom, Error> {
    value
        .atoms
        .get_mut(&id)
        .ok_or(Error::Invalid("missing embedded atom"))
}
fn number(value: f64) -> Result<f64, Error> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(geometry::Error::Numeric.into())
    }
}
fn point(x: f64, y: f64) -> Result<Point, Error> {
    Ok(Point {
        x: number(x)?,
        y: number(y)?,
    })
}
fn add(a: Point, b: Point) -> Result<Point, Error> {
    point(a.x + b.x, a.y + b.y)
}
fn sub(a: Point, b: Point) -> Result<Point, Error> {
    point(a.x - b.x, a.y - b.y)
}
fn scale(a: Point, b: f64) -> Result<Point, Error> {
    point(a.x * b, a.y * b)
}
fn squared(a: Point) -> Result<f64, Error> {
    number(super::arithmetic::squared_length(a.x, a.y))
}
fn cross(a: Point, b: Point) -> Result<f64, Error> {
    number(super::arithmetic::cross(a.x, a.y, b.x, b.y))
}
fn normalize(a: Point) -> Result<Point, Error> {
    let length = squared(a)?.sqrt();
    if length < 1.0e-16 {
        return Err(geometry::Error::Numeric.into());
    }
    point(a.x / length, a.y / length)
}

/// Native graph adjacency retains bond insertion order. Distance rows are
/// computed lazily by unit-edge BFS: exactly the integer distances used by
/// native Floyd-Warshall, without an unbounded dense matrix allocation.
pub struct Input<'a> {
    graph: &'a Graph,
    metadata: &'a Metadata,
    rings: &'a RingCache,
    neighbors: Vec<Vec<(usize, usize)>>,
    atom_rings: Vec<Vec<usize>>,
    ring_bonds: Vec<bool>,
    bonds: BTreeMap<(usize, usize), usize>,
    work_limit: usize,
}
impl<'a> Input<'a> {
    pub fn new(
        graph: &'a Graph,
        metadata: &'a Metadata,
        rings: &'a RingCache,
    ) -> Result<Self, Error> {
        if graph.atoms.len() > geometry::MAX_POINTS
            || graph.bonds.len() > 300_000
            || rings.atoms.len() > 100_000
        {
            return Err(Error::Limit);
        }
        graph
            .validate()
            .map_err(|_| Error::Invalid("chemical graph"))?;
        metadata
            .validate(graph)
            .map_err(|_| Error::Invalid("stereo metadata"))?;
        let mut neighbors = vec![Vec::new(); graph.atoms.len()];
        let mut bonds = BTreeMap::new();
        for (id, bond) in graph.bonds.iter().enumerate() {
            for (a, b) in [(bond.a, bond.b), (bond.b, bond.a)] {
                neighbors
                    .get_mut(a)
                    .ok_or(Error::Invalid("bond endpoint"))?
                    .push((b, id));
            }
            bonds.insert((bond.a.min(bond.b), bond.a.max(bond.b)), id);
        }
        let mut atom_rings = vec![Vec::new(); graph.atoms.len()];
        let mut ring_bonds = vec![false; graph.bonds.len()];
        let mut storage = 0usize;
        for (id, ring) in rings.atoms.iter().enumerate() {
            storage = storage.checked_add(ring.len()).ok_or(Error::Limit)?;
            if storage > 1_000_000 {
                return Err(Error::Limit);
            }
            if ring.len() < 3 {
                return Err(Error::Invalid("ring needs three atoms"));
            }
            let mut seen = BTreeSet::new();
            let mut previous = *ring.last().ok_or(Error::Invalid("empty ring"))?;
            for &a in ring {
                if !seen.insert(a) {
                    return Err(Error::Invalid("repeated ring atom"));
                }
                atom_rings
                    .get_mut(a)
                    .ok_or(Error::Invalid("ring atom"))?
                    .push(id);
                let bond = *bonds
                    .get(&(a.min(previous), a.max(previous)))
                    .ok_or(Error::Invalid("ring traversal"))?;
                *ring_bonds
                    .get_mut(bond)
                    .ok_or(Error::Invalid("ring bond"))? = true;
                previous = a;
            }
        }
        Ok(Self {
            graph,
            metadata,
            rings,
            neighbors,
            atom_rings,
            ring_bonds,
            bonds,
            work_limit: MAX_WORK,
        })
    }
    pub fn with_work_limit(mut self, limit: usize) -> Self {
        self.work_limit = limit.min(MAX_WORK);
        self
    }
    pub(super) fn session(&self) -> Session {
        Session {
            remaining: self.work_limit,
            distances: BTreeMap::new(),
        }
    }
    fn validate(&self, value: &Fragment, session: &mut Session) -> Result<(), Error> {
        if value.atoms.len() > self.graph.atoms.len()
            || value.attachment_points.len() > self.graph.atoms.len()
        {
            return Err(Error::Limit);
        }
        let mut pending = 0usize;
        for (&id, a) in &value.atoms {
            session.spend(1)?;
            at(&self.graph.atoms, id)?;
            at(&self.graph.atoms, a.id)?;
            if !(-1..=1).contains(&a.rotation_direction) {
                return Err(Error::Invalid("rotation direction"));
            }
            for v in [
                a.location.x,
                a.location.y,
                a.normal.x,
                a.normal.y,
                a.angle,
                a.density,
            ] {
                if !v.is_finite() {
                    return Err(geometry::Error::NonFinite.into());
                }
            }
            for id in [a.neighbor1, a.neighbor2, a.cis_trans_neighbor]
                .into_iter()
                .flatten()
            {
                at(&self.graph.atoms, id)?;
            }
            pending = pending.checked_add(a.neighbors.len()).ok_or(Error::Limit)?;
            if pending > 600_000 {
                return Err(Error::Limit);
            }
            session.spend(a.neighbors.len())?;
            let mut seen = BTreeSet::new();
            for &id in &a.neighbors {
                at(&self.graph.atoms, id)?;
                if !seen.insert(id) {
                    return Err(Error::Invalid("repeated pending neighbor"));
                }
            }
        }
        let mut seen = BTreeSet::new();
        for &id in &value.attachment_points {
            session.spend(1)?;
            atom(value, id)?;
            if !seen.insert(id) {
                return Err(Error::Invalid("repeated attachment point"));
            }
        }
        for v in [
            value.bounds.positive_x,
            value.bounds.negative_x,
            value.bounds.positive_y,
            value.bounds.negative_y,
        ] {
            if !v.is_finite() {
                return Err(geometry::Error::NonFinite.into());
            }
        }
        Ok(())
    }
    pub fn find(&self, fragment: &Fragment, include_bonds: bool) -> Result<Report, Error> {
        let mut session = self.session();
        self.validate(fragment, &mut session)?;
        let mut fragment = fragment.clone();
        let pairs = self.find_in_place(&mut fragment, include_bonds, &mut session)?;
        let total_density = Self::density(&fragment)?;
        Ok(Report {
            fragment,
            pairs,
            total_density,
        })
    }
    pub fn apply(&self, fragment: &Fragment, stage: Stage) -> Result<Fragment, Error> {
        self.apply_with_work(fragment, stage, &mut { self.work_limit })
    }
    pub fn repair(&self, fragment: &Fragment) -> Result<Fragment, Error> {
        self.repair_with_work(fragment, &mut { self.work_limit })
    }
    /// Shared-budget entry point for the solver's all-fragment stage ordering.
    pub(super) fn apply_with_work(
        &self,
        fragment: &Fragment,
        stage: Stage,
        remaining: &mut usize,
    ) -> Result<Fragment, Error> {
        self.with_work(fragment, remaining, |value, session| {
            self.apply_in_place(value, stage, session)
        })
    }
    pub(super) fn repair_with_work(
        &self,
        fragment: &Fragment,
        remaining: &mut usize,
    ) -> Result<Fragment, Error> {
        self.with_work(fragment, remaining, |value, session| {
            for stage in [
                Stage::BondAndSpiroFlip,
                Stage::OpenAngles,
                Stage::ShortenBonds,
            ] {
                self.apply_in_place(value, stage, session)?;
            }
            Ok(())
        })
    }
    fn with_work(
        &self,
        fragment: &Fragment,
        remaining: &mut usize,
        operation: impl FnOnce(&mut Fragment, &mut Session) -> Result<(), Error>,
    ) -> Result<Fragment, Error> {
        let mut session = self.session();
        session.remaining = session.remaining.min(*remaining);
        let result = (|| {
            self.validate(fragment, &mut session)?;
            self.complete(fragment, &mut session)?;
            let mut value = fragment.clone();
            operation(&mut value, &mut session)?;
            Ok(value)
        })();
        *remaining = session.remaining;
        result
    }
    fn complete(&self, value: &Fragment, session: &mut Session) -> Result<(), Error> {
        for &id in value.atoms.keys() {
            for &(neighbor, _) in at(&self.neighbors, id)? {
                session.spend(1)?;
                atom(value, neighbor)?;
            }
        }
        Ok(())
    }
    /// The caller owns a detached validated fragment and discards it on error.
    pub(super) fn apply_in_place(
        &self,
        value: &mut Fragment,
        stage: Stage,
        session: &mut Session,
    ) -> Result<(), Error> {
        match stage {
            Stage::BondAndSpiroFlip => self.bond_and_spiro(value, session),
            Stage::OpenAngles => self.open_angles(value, session),
            Stage::ShortenBonds => self.shorten_bonds(value, session),
        }
    }
    fn distance(&self, a: usize, b: usize, session: &mut Session) -> Result<usize, Error> {
        if !session.distances.contains_key(&a) {
            let count = self.graph.atoms.len();
            if session
                .distances
                .len()
                .checked_add(1)
                .and_then(|n| n.checked_mul(count))
                .ok_or(Error::Limit)?
                > MAX_DISTANCES
            {
                return Err(Error::Limit);
            }
            session.spend(count)?;
            let mut row = vec![100_000_000; count];
            *row.get_mut(a).ok_or(Error::Invalid("distance source"))? = 0;
            let mut queue = VecDeque::from([a]);
            while let Some(id) = queue.pop_front() {
                let next = *at(&row, id)? + 1;
                for &(neighbor, _) in at(&self.neighbors, id)? {
                    session.spend(1)?;
                    let target = row
                        .get_mut(neighbor)
                        .ok_or(Error::Invalid("distance neighbor"))?;
                    if *target == 100_000_000 {
                        *target = next;
                        queue.push_back(neighbor);
                    }
                }
            }
            session.distances.insert(a, row);
        }
        Ok(*at(
            session
                .distances
                .get(&a)
                .ok_or(Error::Invalid("distance row"))?,
            b,
        )?)
    }
    fn shortest_path(
        &self,
        a: usize,
        b: usize,
        session: &mut Session,
    ) -> Result<Vec<usize>, Error> {
        if a == b {
            return Err(Error::Invalid("path endpoints coincide"));
        }
        session.spend(self.graph.atoms.len())?;
        let mut previous = vec![None; self.graph.atoms.len()];
        *previous.get_mut(a).ok_or(Error::Invalid("path source"))? = Some(a);
        at(&previous, b)?;
        let mut queue = VecDeque::from([a]);
        while let Some(id) = queue.pop_front() {
            for &(neighbor, _) in at(&self.neighbors, id)? {
                session.spend(1)?;
                let pred = previous
                    .get_mut(neighbor)
                    .ok_or(Error::Invalid("path neighbor"))?;
                if pred.is_none() {
                    *pred = Some(id);
                    if neighbor == b {
                        let mut result = vec![b];
                        let mut end = b;
                        while end != a {
                            session.spend(1)?;
                            end = at(&previous, end)?.ok_or(Error::Invalid("path predecessor"))?;
                            result.push(end);
                        }
                        result.reverse();
                        return Ok(result);
                    }
                    queue.push_back(neighbor);
                }
            }
        }
        Ok(Vec::new())
    }
    fn bond(&self, a: usize, b: usize) -> Result<usize, Error> {
        self.bonds
            .get(&(a.min(b), a.max(b)))
            .copied()
            .ok_or(Error::Invalid("missing path bond"))
    }
    fn density(value: &Fragment) -> Result<f64, Error> {
        value
            .atoms
            .values()
            .try_fold(0.0, |sum, a| number(a.density + sum))
    }
    fn find_in_place(
        &self,
        value: &mut Fragment,
        include_bonds: bool,
        session: &mut Session,
    ) -> Result<Vec<(usize, usize)>, Error> {
        session.spend(value.atoms.len())?;
        for a in value.atoms.values_mut() {
            a.density = 0.0;
        }
        let mut result = Vec::new();
        let ids = value.atoms.keys().copied().collect::<Vec<_>>();
        for (i, &a) in ids.iter().enumerate() {
            let factor1 = if at(&self.graph.atoms, a)?.atomic_number == 6 {
                1.0
            } else {
                HETEROATOM_SCALE
            };
            for &b in ids.iter().take(i) {
                session.spend(1)?;
                let factor2 = if at(&self.graph.atoms, b)?.atomic_number == 6 {
                    1.0
                } else {
                    HETEROATOM_SCALE
                };
                let mut d2 = squared(sub(atom(value, b)?.location, atom(value, a)?.location)?)?;
                let density = if d2 > 1.0e-3 { 1.0 / d2 } else { 1000.0 };
                for id in [a, b] {
                    let a = atom_mut(value, id)?;
                    a.density = number(a.density + density)?;
                }
                d2 /= factor1 * factor2;
                if d2 < COLLISION_THRESHOLD * COLLISION_THRESHOLD {
                    if result.len() >= MAX_PAIRS {
                        return Err(Error::Limit);
                    }
                    result.push((a, b));
                }
            }
        }
        if include_bonds {
            for (id, b1) in self.graph.bonds.iter().enumerate() {
                session.spend(1)?;
                let (Some(a1), Some(a2)) = (value.atoms.get(&b1.a), value.atoms.get(&b1.b)) else {
                    continue;
                };
                let v1 = sub(a2.location, a1.location)?;
                let avg1 = scale(add(a2.location, a1.location)?, 0.5)?;
                for b2 in self.graph.bonds.iter().skip(id + 1) {
                    session.spend(1)?;
                    let (Some(a3), Some(a4)) = (value.atoms.get(&b2.a), value.atoms.get(&b2.b))
                    else {
                        continue;
                    };
                    let avg2 = sub(scale(add(a4.location, a3.location)?, 0.5)?, avg1)?;
                    let avg_sq = squared(avg2)?;
                    // Native also tests < 0.5; the stricter squared threshold implies it.
                    if avg_sq < 0.5 * 0.5 {
                        let v2 = sub(a3.location, a1.location)?;
                        let v3 = sub(a4.location, a1.location)?;
                        if number(cross(v1, v2)? * cross(v1, v3)?)? < -1e-6 {
                            let mut best = (usize::MAX, (b1.a, b2.a));
                            for (a, b) in [(b1.a, b2.a), (b1.a, b2.b), (b1.b, b2.a), (b1.b, b2.b)] {
                                let distance = self.distance(a, b, session)?;
                                if distance < best.0 {
                                    best = (distance, (a, b));
                                }
                            }
                            if result.len() >= MAX_PAIRS {
                                return Err(Error::Limit);
                            }
                            result.push(best.1);
                        }
                    }
                }
            }
        }
        Ok(result)
    }
}
