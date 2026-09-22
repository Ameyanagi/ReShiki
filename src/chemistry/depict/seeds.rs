//! Initial coordination, coordinate-map and cis/trans fragments adapted from
//! RDKit 2026.03.6 RDDepictor.cpp, EmbeddedFrag.cpp and NontetrahedralStereo.cpp.
//! BSD-3-Clause; see licenses/rdkit/NOTICE. No layout orchestration or dispatch.

mod coordinates;
mod coordination;

use super::{
    arithmetic, attachment,
    geometry::{self, Bounds, Coordinates, Point},
    rings::Fragment,
};
use crate::chemistry::{
    graph::Graph,
    ranking::Metadata,
    stereo::perception::{RingCache, RingKind},
};
use std::collections::BTreeMap;

pub const MAX_WORK: usize = 50_000_000;
pub const MAX_ANGLE_PAIRS: usize = 1_000_000;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("Invalid depiction seed input: {0}")]
    Invalid(&'static str),
    #[error("Depiction seed work or storage limit exceeded")]
    Limit,
    #[error(transparent)]
    Geometry(#[from] geometry::Error),
}
impl From<attachment::Error> for Error {
    fn from(error: attachment::Error) -> Self {
        match error {
            attachment::Error::Invalid(reason) => Self::Invalid(reason),
            attachment::Error::Limit => Self::Limit,
            attachment::Error::Geometry(error) => Self::Geometry(error),
        }
    }
}

/// Each native coordination function captures BOND_LEN in a function-static
/// ideal-point array on its first invocation, even when its tag does not match.
/// The caller supplies those captured values explicitly; there are no globals.
/// Current cis/trans bond length is a separate argument.
#[derive(Debug, Clone, Copy)]
pub struct IdealLengths {
    pub square_planar: f64,
    pub trigonal_bipyramidal: f64,
    pub octahedral: f64,
}

struct Work(usize);
impl Work {
    fn spend(&mut self, amount: usize) -> Result<(), Error> {
        self.0 = self.0.checked_sub(amount).ok_or(Error::Limit)?;
        Ok(())
    }
}
fn at<T>(values: &[T], id: usize) -> Result<&T, Error> {
    values.get(id).ok_or(Error::Invalid("index outside input"))
}
fn empty() -> Fragment {
    Fragment {
        atoms: BTreeMap::new(),
        done: false,
        bounds: Bounds {
            positive_x: 0.0,
            negative_x: 0.0,
            positive_y: 0.0,
            negative_y: 0.0,
        },
        attachment_points: Vec::new(),
    }
}
fn input_number(value: f64) -> Result<f64, Error> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(geometry::Error::NonFinite.into())
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
fn normalize(p: Point) -> Result<Point, Error> {
    let length = number(arithmetic::squared_length(p.x, p.y))?.sqrt();
    if length < 1e-16 {
        return Err(geometry::Error::Numeric.into());
    }
    point(Point {
        x: p.x / length,
        y: p.y / length,
    })
}

/// Explicit graph, stereo properties and ring-cache state. Coordinate-map
/// construction reads ring counts only for an attachment with >=3 embedded
/// neighbors. All methods return new fragments and preserve every input.
pub struct Input<'a> {
    graph: &'a Graph,
    metadata: &'a Metadata,
    attachment: attachment::Input<'a>,
    ring_counts: Option<Vec<usize>>,
    work_limit: usize,
}
impl<'a> Input<'a> {
    pub fn new(
        graph: &'a Graph,
        metadata: &'a Metadata,
        data: &'a [attachment::AtomData],
        rings: &RingCache,
    ) -> Result<Self, Error> {
        let attachment = attachment::Input::new(graph, data)?;
        metadata
            .validate(graph)
            .map_err(|_| Error::Invalid("stereo metadata"))?;
        if rings.atoms.len() > 100_000 {
            return Err(Error::Limit);
        }
        if rings.kind == RingKind::None && !rings.atoms.is_empty() {
            return Err(Error::Invalid("uninitialized ring cache contains rings"));
        }
        let mut counts = vec![0; graph.atoms.len()];
        let mut entries = 0usize;
        for ring in &rings.atoms {
            entries = entries.checked_add(ring.len()).ok_or(Error::Limit)?;
            if entries > 1_000_000 {
                return Err(Error::Limit);
            }
            for &id in ring {
                *counts.get_mut(id).ok_or(Error::Invalid("ring atom"))? += 1;
            }
        }
        Ok(Self {
            graph,
            metadata,
            attachment,
            ring_counts: (rings.kind != RingKind::None).then_some(counts),
            work_limit: MAX_WORK,
        })
    }
    /// Lower the independent work bounds for neighbor setup and seed geometry.
    pub fn with_work_limit(mut self, limit: usize) -> Self {
        self.work_limit = limit.min(MAX_WORK);
        self.attachment = self.attachment.with_work_limit(limit);
        self
    }
    fn neighbors(&self, id: usize) -> Result<&[usize], Error> {
        Ok(self.attachment.adjacent(id)?)
    }
    fn work(&self) -> Work {
        Work(self.work_limit)
    }
    fn with_budget<T>(
        &self,
        remaining: &mut usize,
        run: impl FnOnce(&mut Work) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let mut work = Work((*remaining).min(self.work_limit));
        let result = run(&mut work);
        *remaining = work.0;
        result
    }
    /// Direct stereobond constructor: neighbor setup is deliberately separate,
    /// as in native embedCisTransSystems. Controls must be adjacent substituents.
    pub fn cis_trans(&self, bond: usize, current_length: f64) -> Result<Fragment, Error> {
        self.cis_trans_with_budget(bond, current_length, &mut { self.work_limit })
    }
    pub(super) fn cis_trans_with_budget(
        &self,
        bond: usize,
        current_length: f64,
        remaining: &mut usize,
    ) -> Result<Fragment, Error> {
        self.with_budget(remaining, |work| {
            self.cis_trans_work(bond, current_length, work)
        })
    }
    fn cis_trans_work(
        &self,
        bond: usize,
        current_length: f64,
        work: &mut Work,
    ) -> Result<Fragment, Error> {
        work.spend(2)?;
        let edge = at(&self.graph.bonds, bond)?;
        let stereo = at(&self.metadata.bonds, bond)?;
        let length = input_number(current_length)?;
        if edge.order != 2 || stereo.stereo <= 1 || stereo.stereo_atoms.len() != 2 {
            return Err(Error::Invalid("cis/trans bond or controls"));
        }
        for (position, center, other) in [(0, edge.a, edge.b), (1, edge.b, edge.a)] {
            let control = *at(&stereo.stereo_atoms, position)?;
            let adjacent = self.neighbors(center)?;
            work.spend(adjacent.len())?;
            if control == other || !adjacent.contains(&control) {
                return Err(Error::Invalid("nonadjacent cis/trans control"));
            }
        }
        let mut result = empty();
        let mut begin = attachment::fresh(edge.a);
        begin.neighbor1 = Some(edge.b);
        begin.normal = Point { x: 0.0, y: -1.0 };
        begin.counter_clockwise = false;
        begin.cis_trans_neighbor = Some(*at(&stereo.stereo_atoms, 0)?);
        let mut end = attachment::fresh(edge.b);
        end.location = Point { x: length, y: 0.0 };
        end.neighbor1 = Some(edge.a);
        end.cis_trans_neighbor = Some(*at(&stereo.stereo_atoms, 1)?);
        if matches!(stereo.stereo, 2 | 4) {
            end.normal = Point { x: 0.0, y: -1.0 };
        } else {
            end.normal = Point { x: 0.0, y: 1.0 };
            end.counter_clockwise = false;
        }
        result.atoms.insert(edge.a, begin);
        result.atoms.insert(edge.b, end);
        Ok(result)
    }
    /// Native coordinate-map constructor, including fixed flags, neighbor
    /// setup and attachment geometry. Input points are used without alignment.
    pub fn from_coordinates(&self, coordinates: &Coordinates) -> Result<Fragment, Error> {
        self.coordinates_with_budget(coordinates, &mut { self.work_limit })
    }
    pub(super) fn coordinates_with_budget(
        &self,
        coordinates: &Coordinates,
        remaining: &mut usize,
    ) -> Result<Fragment, Error> {
        self.with_budget(remaining, |work| {
            self.coordinate_fragment(coordinates, work)
        })
    }
}
