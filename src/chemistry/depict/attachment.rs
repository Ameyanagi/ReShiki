//! Ordered neighbor setup and direct non-ring attachment from RDKit 2026.03.6
//! EmbeddedFrag.cpp and DepictUtils.cpp/.h. BSD-3-Clause; see licenses/rdkit/NOTICE.
//!
//! This is a library stage. Expansion, cis/trans and non-tetrahedral initial
//! fragment construction, collision repair and runtime dispatch are separate.
//! Every operation returns a new fragment and leaves all supplied state intact.

use super::{
    arithmetic,
    geometry::{self, Bounds, Point, Transform},
    rings::{EmbeddedAtom, Fragment},
};
use crate::chemistry::{electronic::Hybridization, graph::Graph};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, f64::consts::PI};

pub const MAX_WORK: usize = 50_000_000;
const MAX_NEIGHBORS: usize = 600_000;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("Invalid depiction attachment input: {0}")]
    Invalid(&'static str),
    #[error("Depiction attachment work or storage limit exceeded")]
    Limit,
    #[error(transparent)]
    Geometry(#[from] geometry::Error),
}

/// Cached native properties are explicit. CIPRank takes precedence over
/// ChiralAtomRank (the literal property `_chiralAtomRank`); absent properties use the element/degree/index fallback.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AtomData {
    pub hybridization: Hybridization,
    pub cip_rank: Option<u32>,
    pub chiral_rank: Option<u32>,
}

struct Work(usize);
impl Work {
    fn spend(&mut self, amount: usize) -> Result<(), Error> {
        self.0 = self.0.checked_sub(amount).ok_or(Error::Limit)?;
        Ok(())
    }
}
fn at<T>(items: &[T], id: usize) -> Result<&T, Error> {
    items.get(id).ok_or(Error::Invalid("index outside input"))
}
fn atom(fragment: &Fragment, id: usize) -> Result<&EmbeddedAtom, Error> {
    fragment
        .atoms
        .get(&id)
        .ok_or(Error::Invalid("missing embedded atom"))
}
fn atom_mut(fragment: &mut Fragment, id: usize) -> Result<&mut EmbeddedAtom, Error> {
    fragment
        .atoms
        .get_mut(&id)
        .ok_or(Error::Invalid("missing embedded atom"))
}
pub(super) fn fresh(id: usize) -> EmbeddedAtom {
    EmbeddedAtom {
        id,
        location: Point::default(),
        normal: Point::default(),
        angle: -1.0,
        neighbor1: None,
        neighbor2: None,
        cis_trans_neighbor: None,
        counter_clockwise: true,
        rotation_direction: 0,
        neighbors: Vec::new(),
        density: -1.0,
        fixed: false,
    }
}
fn number(value: f64) -> Result<f64, Error> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(geometry::Error::Numeric.into())
    }
}
fn point(value: Point) -> Result<Point, Error> {
    number(value.x)?;
    number(value.y)?;
    Ok(value)
}
fn sub(a: Point, b: Point) -> Result<Point, Error> {
    point(Point {
        x: a.x - b.x,
        y: a.y - b.y,
    })
}
fn length(p: Point) -> Result<f64, Error> {
    number(number(arithmetic::squared_length(p.x, p.y))?.sqrt())
}
fn normalize(p: Point) -> Result<Point, Error> {
    let size = length(p)?;
    if size < 1.0e-16 {
        return Err(geometry::Error::Numeric.into());
    }
    point(Point {
        x: p.x / size,
        y: p.y / size,
    })
}

/// Source adjacency follows bond insertion order. The borrowed graph and
/// property arrays are bounded and validated once; methods have separate work
/// budgets. Rank arithmetic preserves native unsigned wrapping followed by
/// the signed 32-bit pair conversion used by rankAtomsByRank.
pub struct Input<'a> {
    graph: &'a Graph,
    data: &'a [AtomData],
    neighbors: Vec<Vec<usize>>,
    ranks: Vec<i32>,
    work_limit: usize,
}
impl<'a> Input<'a> {
    pub fn new(graph: &'a Graph, data: &'a [AtomData]) -> Result<Self, Error> {
        if graph.atoms.len() > geometry::MAX_POINTS
            || graph.bonds.len() > 300_000
            || data.len() > geometry::MAX_POINTS
        {
            return Err(Error::Limit);
        }
        if data.len() != graph.atoms.len() {
            return Err(Error::Invalid("atom property count"));
        }
        graph
            .validate()
            .map_err(|_| Error::Invalid("chemical graph"))?;
        let mut neighbors = vec![Vec::new(); graph.atoms.len()];
        for bond in &graph.bonds {
            for (a, b) in [(bond.a, bond.b), (bond.b, bond.a)] {
                neighbors
                    .get_mut(a)
                    .ok_or(Error::Invalid("bond endpoint"))?
                    .push(b);
            }
        }
        let count = u32::try_from(graph.atoms.len()).map_err(|_| Error::Limit)?;
        let mut ranks = Vec::with_capacity(data.len());
        for (id, properties) in data.iter().enumerate() {
            let rank = if let Some(rank) = properties.cip_rank {
                rank
            } else {
                let rank = properties
                    .chiral_rank
                    .map_or(id as u32, |rank| count.wrapping_sub(rank));
                let atomic_number = u32::from(at(&graph.atoms, id)?.atomic_number);
                let element = if atomic_number == 1 {
                    1000
                } else {
                    atomic_number
                };
                let degree = u32::try_from(at(&neighbors, id)?.len()).map_err(|_| Error::Limit)?;
                rank.wrapping_add(count.wrapping_mul(100 * element + degree))
            };
            ranks.push(rank as i32);
        }
        Ok(Self {
            graph,
            data,
            neighbors,
            ranks,
            work_limit: MAX_WORK,
        })
    }
    pub(super) fn adjacent(&self, id: usize) -> Result<&[usize], Error> {
        Ok(at(&self.neighbors, id)?.as_slice())
    }
    pub fn with_work_limit(mut self, limit: usize) -> Self {
        self.work_limit = limit.min(MAX_WORK);
        self
    }
    fn work(&self) -> Work {
        Work(self.work_limit)
    }
    fn rank(&self, ids: &[usize], ascending: bool, work: &mut Work) -> Result<Vec<usize>, Error> {
        if ids.len() > MAX_NEIGHBORS {
            return Err(Error::Limit);
        }
        let log =
            usize::try_from(usize::BITS - ids.len().leading_zeros()).map_err(|_| Error::Limit)?;
        work.spend(ids.len().checked_mul(log + 1).ok_or(Error::Limit)?)?;
        let mut keys = Vec::with_capacity(ids.len());
        for &id in ids {
            keys.push((*at(&self.ranks, id)?, id));
        }
        if ascending {
            keys.sort();
        } else {
            keys.sort_by(|a, b| b.cmp(a));
        }
        Ok(keys.into_iter().map(|(_, id)| id).collect())
    }
    pub fn ranked_atoms(&self, ids: &[usize], ascending: bool) -> Result<Vec<usize>, Error> {
        self.rank(ids, ascending, &mut self.work())
    }
    fn validate(&self, value: &Fragment, work: &mut Work) -> Result<(), Error> {
        if value.atoms.len() > self.graph.atoms.len()
            || value.attachment_points.len() > self.graph.atoms.len()
        {
            return Err(Error::Limit);
        }
        let mut storage = 0usize;
        for (&id, a) in &value.atoms {
            work.spend(1)?;
            at(self.data, id)?;
            at(self.data, a.id)?;
            if !(-1..=1).contains(&a.rotation_direction) {
                return Err(Error::Invalid("rotation direction"));
            }
            for coordinate in [
                a.location.x,
                a.location.y,
                a.normal.x,
                a.normal.y,
                a.angle,
                a.density,
            ] {
                if !coordinate.is_finite() {
                    return Err(geometry::Error::NonFinite.into());
                }
            }
            for index in [a.neighbor1, a.neighbor2, a.cis_trans_neighbor]
                .into_iter()
                .flatten()
            {
                at(self.data, index)?;
            }
            storage = storage.checked_add(a.neighbors.len()).ok_or(Error::Limit)?;
            if storage > MAX_NEIGHBORS {
                return Err(Error::Limit);
            }
            work.spend(a.neighbors.len())?;
            let mut seen = BTreeSet::new();
            for &index in &a.neighbors {
                at(self.data, index)?;
                if !seen.insert(index) {
                    return Err(Error::Invalid("repeated unembedded neighbor"));
                }
            }
        }
        let mut seen = BTreeSet::new();
        work.spend(value.attachment_points.len())?;
        for &id in &value.attachment_points {
            atom(value, id)?;
            if !seen.insert(id) {
                return Err(Error::Invalid("repeated attachment point"));
            }
        }
        for coordinate in [
            value.bounds.positive_x,
            value.bounds.negative_x,
            value.bounds.positive_y,
            value.bounds.negative_y,
        ] {
            if !coordinate.is_finite() {
                return Err(geometry::Error::NonFinite.into());
            }
        }
        Ok(())
    }
    fn order_neighbors(
        &self,
        id: usize,
        ids: &[usize],
        work: &mut Work,
    ) -> Result<Vec<usize>, Error> {
        let neighbors = at(&self.neighbors, id)?;
        let selected: BTreeSet<_> = ids.iter().copied().collect();
        work.spend(neighbors.len() + ids.len())?;
        let reference = neighbors.iter().copied().rfind(|id| !selected.contains(id));
        let mut ordered = ids.to_vec();
        if let Some(id) = reference {
            ordered.push(id);
        }
        if ordered.len() < 4 {
            return Err(Error::Invalid("neighbor ordering needs four atoms"));
        }
        ordered = self.rank(&ordered, true, work)?;
        let size = ordered.len();
        // Both positions exist after the length check; checked access avoids
        // indexing assumptions even for hostile externally supplied fragments.
        let left = *at(&ordered, size - 3)?;
        let right = *at(&ordered, size - 2)?;
        *ordered
            .get_mut(size - 3)
            .ok_or(Error::Invalid("neighbor swap"))? = right;
        *ordered
            .get_mut(size - 2)
            .ok_or(Error::Invalid("neighbor swap"))? = left;
        if let Some(id) = reference {
            let position = ordered
                .iter()
                .position(|&other| other == id)
                .ok_or(Error::Invalid("missing neighbor reference"))?;
            ordered.rotate_left(position + 1);
            ordered.pop();
        }
        Ok(ordered)
    }
    fn update(&self, value: &mut Fragment, id: usize, work: &mut Work) -> Result<(), Error> {
        atom(value, id)?;
        let adjacent = at(&self.neighbors, id)?;
        work.spend(adjacent.len())?;
        let mut ids = Vec::new();
        let mut hydrogens = Vec::new();
        for &other in adjacent {
            if !value.atoms.contains_key(&other) {
                if at(&self.graph.atoms, other)?.atomic_number == 1 {
                    hydrogens.push(other);
                } else {
                    ids.push(other);
                }
            }
        }
        ids.extend(hydrogens);
        if !ids.is_empty() {
            ids = if adjacent.len() < 4 || ids.len() < 3 {
                self.rank(&ids, true, work)?
            } else {
                self.order_neighbors(id, &ids, work)?
            };
            work.spend(value.attachment_points.len())?;
            if !value.attachment_points.contains(&id) {
                value.attachment_points.push(id);
            }
        }
        atom_mut(value, id)?.neighbors = ids;
        Ok(())
    }
    pub fn single_atom(&self, id: usize) -> Result<Fragment, Error> {
        at(self.data, id)?;
        let mut a = fresh(id);
        a.normal = Point { x: 1.0, y: 0.0 };
        let mut value = Fragment {
            atoms: [(id, a)].into(),
            done: false,
            bounds: Bounds {
                positive_x: 0.0,
                negative_x: 0.0,
                positive_y: 0.0,
                negative_y: 0.0,
            },
            attachment_points: Vec::new(),
        };
        self.update(&mut value, id, &mut self.work())?;
        Ok(value)
    }
    /// Rebuild one neighbor list. Native stale attachment entries are retained.
    pub fn update_neighbors(&self, fragment: &Fragment, id: usize) -> Result<Fragment, Error> {
        let mut work = self.work();
        self.validate(fragment, &mut work)?;
        let mut value = fragment.clone();
        self.update(&mut value, id, &mut work)?;
        Ok(value)
    }
    /// Rebuild every list in ascending map order, then rank attachment points.
    pub fn setup_neighbors(&self, fragment: &Fragment) -> Result<Fragment, Error> {
        let mut work = self.work();
        self.validate(fragment, &mut work)?;
        let mut value = fragment.clone();
        value.attachment_points.clear();
        for &id in fragment.atoms.keys() {
            self.update(&mut value, id, &mut work)?;
        }
        value.attachment_points = self.rank(&value.attachment_points, true, &mut work)?;
        Ok(value)
    }
    pub fn add_non_ring_atom(
        &self,
        fragment: &Fragment,
        id: usize,
        target: usize,
        bond_length: f64,
    ) -> Result<Fragment, Error> {
        let mut work = self.work();
        self.validate(fragment, &mut work)?;
        at(self.data, id)?;
        if !bond_length.is_finite() {
            return Err(geometry::Error::NonFinite.into());
        }
        if fragment.atoms.contains_key(&id) {
            return Err(Error::Invalid("atom already embedded"));
        }
        let reference = atom(fragment, target)?;
        let adjacent = at(&self.neighbors, target)?;
        work.spend(adjacent.len() + reference.neighbors.len())?;
        if !adjacent.contains(&id) || !reference.neighbors.contains(&id) {
            return Err(Error::Invalid("atom is not a pending bonded neighbor"));
        }
        let mut value = fragment.clone();
        if reference.angle > 0.0 {
            self.with_angle(&mut value, id, target, &mut work)?;
        } else {
            self.without_angle(&mut value, id, target, bond_length)?;
        }
        atom_mut(&mut value, target)?
            .neighbors
            .retain(|&other| other != id);
        self.update(&mut value, id, &mut work)?;
        Ok(value)
    }
    fn nearby(value: &Fragment, p: Point, radius: f64, work: &mut Work) -> Result<usize, Error> {
        work.spend(value.atoms.len())?;
        let mut result = 0;
        for a in value.atoms.values() {
            if length(sub(a.location, p)?)? < radius {
                result += 1;
            }
        }
        Ok(result)
    }
    fn with_angle(
        &self,
        value: &mut Fragment,
        id: usize,
        target: usize,
        work: &mut Work,
    ) -> Result<(), Error> {
        let reference = atom(value, target)?.clone();
        let remaining = number(2.0 * PI - reference.angle)?;
        let count = u32::try_from(reference.neighbors.len() + 1).map_err(|_| Error::Limit)?;
        let mut angle = remaining / f64::from(count);
        atom_mut(value, target)?.angle = number(reference.angle + angle)?;
        let first = atom(
            value,
            reference
                .neighbor1
                .ok_or(Error::Invalid("first angle anchor"))?,
        )?
        .location;
        let second = atom(
            value,
            reference
                .neighbor2
                .ok_or(Error::Invalid("second angle anchor"))?,
        )?
        .location;
        if reference.rotation_direction == 0 {
            let a = sub(first, reference.location)?;
            let b = sub(second, reference.location)?;
            let cross = number(arithmetic::cross(a.x, a.y, b.x, b.y))?;
            let direction = if number(cross * (PI - remaining))? >= 0.0 {
                -1
            } else {
                1
            };
            atom_mut(value, target)?.rotation_direction = direction;
        }
        angle *= f64::from(atom(value, target)?.rotation_direction);
        let mut location = Transform::rotate(reference.location, angle)?.apply(second)?;
        if remaining.abs() - PI < 1.0e-3 {
            let alternate = Transform::rotate(reference.location, -angle)?.apply(second)?;
            if Self::nearby(value, location, 0.5, work)?
                > Self::nearby(value, alternate, 0.5, work)?
            {
                location = alternate;
            }
        }
        atom_mut(value, target)?.neighbor2 = Some(id);
        let direction = sub(location, reference.location)?;
        let normal = Point {
            x: -direction.y,
            y: direction.x,
        };
        let left = point(Point {
            x: location.x + normal.x,
            y: location.y + normal.y,
        })?;
        let right = sub(location, normal)?;
        let ccw = Self::nearby(value, left, 2.5, work)?;
        let cw = Self::nearby(value, right, 2.5, work)?;
        let mut normal = normalize(normal)?;
        let mut added = fresh(id);
        added.location = location;
        added.neighbor1 = Some(target);
        if ccw < cw {
            added.counter_clockwise = false;
        } else {
            normal.x *= -1.0;
            normal.y *= -1.0;
        }
        added.normal = normal;
        value.atoms.insert(id, added);
        Ok(())
    }
    fn without_angle(
        &self,
        value: &mut Fragment,
        id: usize,
        target: usize,
        bond_length: f64,
    ) -> Result<(), Error> {
        let reference = atom(value, target)?.clone();
        let mut ccw = reference.counter_clockwise;
        let mut location = reference.normal;
        if reference
            .cis_trans_neighbor
            .is_some_and(|control| control != id)
        {
            ccw = !ccw;
            location.x *= -1.0;
            location.y *= -1.0;
        }
        if number(arithmetic::squared_length(location.x, location.y))? <= 1.0e-8 {
            return Err(Error::Invalid("attachment normal is too short"));
        }
        let degree = at(&self.neighbors, target)?.len();
        let mut angle = match at(self.data, target)?.hybridization {
            Hybridization::Unspecified | Hybridization::Sp3 => {
                if degree == 4 {
                    PI / 2.0
                } else {
                    2.0 * PI / 3.0
                }
            }
            Hybridization::Sp2 => 2.0 * PI / 3.0,
            _ => 2.0 * PI / f64::from(u32::try_from(degree).map_err(|_| Error::Limit)?),
        };
        let flip = reference.neighbor1.is_none();
        if !flip {
            let a = atom_mut(value, target)?;
            a.angle = angle;
            a.neighbor2 = Some(id);
        } else {
            let normal = Transform::rotate(Point::default(), angle)?.apply(reference.normal)?;
            let a = atom_mut(value, target)?;
            a.normal = normal;
            a.neighbor1 = Some(id);
        }
        angle -= PI / 2.0;
        if !ccw {
            angle *= -1.0;
        }
        location = Transform::rotate(Point::default(), angle)?.apply(location)?;
        location.x *= bond_length;
        location.y *= bond_length;
        location.x += reference.location.x;
        location.y += reference.location.y;
        location = point(location)?;
        let direction = sub(reference.location, location)?;
        let mut normal = Point {
            x: -direction.y,
            y: direction.x,
        };
        if ccw ^ flip {
            normal.x *= -1.0;
            normal.y *= -1.0;
        }
        let mut added = fresh(0); // Native no-angle helper leaves aid at its default.
        added.location = location;
        added.normal = normalize(normal)?;
        added.neighbor1 = Some(target);
        added.counter_clockwise = (!ccw) ^ flip;
        value.atoms.insert(id, added);
        Ok(())
    }
}
