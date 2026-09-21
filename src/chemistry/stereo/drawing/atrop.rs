//! Atropisomer detection adapted from RDKit Atropisomers.cpp (2026.03.6).
//! Copyright (C) 2004-2021 Tad hurst/CDD and other RDKit contributors.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
use super::{Point3, wedging::Conformer};
use crate::chemistry::{
    electronic::{self, Hybridization},
    graph::Graph,
    kekulize::Direction,
    ranking::Metadata,
};

const SMALL: f64 = 1e-7;

#[derive(Debug, thiserror::Error)]
pub enum AtropError {
    #[error("Invalid atropisomer graph: {0}")]
    Graph(String),
    #[error("Invalid atropisomer metadata: {0}")]
    Metadata(String),
    #[error("Invalid atropisomer input: {0}")]
    Invalid(&'static str),
    #[error("Atropisomer geometry: {0}")]
    Geometry(String),
}
type Result<T> = std::result::Result<T, AtropError>;
fn at<T>(items: &[T], i: usize) -> Result<&T> {
    items
        .get(i)
        .ok_or(AtropError::Invalid("Missing graph item"))
}

#[derive(Default)]
struct Neighbors {
    bonds: Vec<(usize, usize)>,
    wedges: usize,
    unknown: usize,
}
struct End {
    atom: usize,
    count: usize,
    first: (usize, usize),
    second: Option<(usize, usize)>,
}
impl End {
    fn direction(&self, directions: &[Direction]) -> Result<Option<Direction>> {
        let wedge = |id| -> Result<Direction> {
            Ok(match *at(directions, id)? {
                d @ (Direction::Wedge | Direction::Hash) => d,
                _ => Direction::None,
            })
        };
        let first = wedge(self.first.0)?;
        let second = self
            .second
            .map(|(id, _)| wedge(id))
            .transpose()?
            .unwrap_or(Direction::None);
        if first != Direction::None && first == second {
            return Ok(None);
        }
        Ok(Some(
            if first == Direction::Wedge || second == Direction::Hash {
                Direction::Wedge
            } else if first == Direction::Hash || second == Direction::Wedge {
                Direction::Hash
            } else {
                Direction::None
            },
        ))
    }

    fn vector(&self, conf: &Conformer, y: Point3, z: Point3) -> Result<Option<Point3>> {
        if !conf.is_3d && self.count > 2 {
            return Err(AtropError::Invalid("More than two 2D defining bonds"));
        }
        let center = *at(&conf.positions, self.atom)?;
        let project = |id| -> Result<Point3> {
            let v = at(&conf.positions, id)?.sub(center);
            Ok(Point3 {
                x: 0.,
                y: v.dot(y),
                z: v.dot(z),
            })
        };
        let mut result = project(self.first.1)?;
        if let Some((_, atom)) = self.second {
            let other = project(atom)?;
            if result.squared().sqrt() < SMALL {
                result = Point3::default().sub(other);
            } else if result.dot(other) > SMALL {
                return Ok(None);
            }
        }
        if result.squared().sqrt() < SMALL {
            return Ok(None);
        }
        if !conf.is_3d {
            result = result.unit().map_err(AtropError::Geometry)?;
        }
        Ok(Some(result))
    }
}

fn end(
    neighbors: &[Neighbors],
    atom: usize,
    axis: usize,
    directions: &[Direction],
) -> Result<Option<End>> {
    let adjacent = at(neighbors, atom)?;
    let count = adjacent.bonds.len().saturating_sub(1);
    if count == 0 || adjacent.unknown > usize::from(*at(directions, axis)? == Direction::Unknown) {
        return Ok(None);
    }
    let mut others = adjacent.bonds.iter().copied().filter(|&(id, _)| id != axis);
    let mut first = others
        .next()
        .ok_or(AtropError::Invalid("Missing defining bond"))?;
    let second = if count == 2 {
        let mut second = others
            .next()
            .ok_or(AtropError::Invalid("Missing defining bond"))?;
        if second.1 < first.1 {
            std::mem::swap(&mut first, &mut second);
        }
        Some(second)
    } else {
        None
    };
    Ok(Some(End {
        atom,
        count,
        first,
        second,
    }))
}

fn frame(conf: &Conformer, a: usize, b: usize) -> Result<Option<(Point3, Point3)>> {
    let x = at(&conf.positions, b)?.sub(*at(&conf.positions, a)?);
    if x.squared().sqrt() < SMALL {
        return Ok(None);
    }
    let x = x.unit().map_err(AtropError::Geometry)?;
    if !conf.is_3d {
        let y = Point3 {
            x: -x.y,
            y: x.x,
            z: 0.,
        }
        .unit()
        .map_err(AtropError::Geometry)?;
        return Ok(Some((
            y,
            Point3 {
                x: 0.,
                y: 0.,
                z: 1.,
            },
        )));
    }
    let z = if x.x.abs() > SMALL || x.y.abs() > SMALL {
        Point3 {
            x: 0.,
            y: 0.,
            z: 1.,
        }
    } else {
        Point3 {
            x: 1.,
            y: 0.,
            z: 0.,
        }
    };
    let y = z.cross(x);
    let z = x.cross(y);
    Ok(Some((
        y.unit().map_err(AtropError::Geometry)?,
        z.unit().map_err(AtropError::Geometry)?,
    )))
}

fn perceive(
    ends: [End; 2],
    directions: &[Direction],
    conf: Option<&Conformer>,
) -> Result<Option<u8>> {
    let [left, right] = ends;
    let Some(conf) = conf else {
        let (Some(a), Some(b)) = (left.direction(directions)?, right.direction(directions)?) else {
            return Ok(None);
        };
        return Ok(if a == b {
            None
        } else if a == Direction::Wedge || b == Direction::Hash {
            Some(7)
        } else if a == Direction::Hash || b == Direction::Wedge {
            Some(6)
        } else {
            None
        });
    };
    let Some((y, z)) = frame(conf, left.atom, right.atom)? else {
        return Ok(None);
    };
    let mut vectors = [Point3::default(); 2];
    for (end, target) in [left, right].iter().zip(&mut vectors) {
        // In 3D, wedging only selects candidate axes; it does not change geometry.
        let direction = if conf.is_3d {
            Some(Direction::None)
        } else {
            end.direction(directions)?
        };
        let Some(direction) = direction else {
            return Ok(None);
        };
        let Some(mut vector) = end.vector(conf, y, z)? else {
            return Ok(None);
        };
        if matches!(direction, Direction::Wedge | Direction::Hash) {
            vector.y *= 0.707;
            vector.z = vector.y.abs()
                * if direction == Direction::Wedge {
                    1.
                } else {
                    -1.
                };
        }
        *target = vector;
    }
    let [a, b] = vectors;
    let cross = b.cross(a).x;
    Ok(if cross > SMALL {
        Some(7)
    } else if cross < -SMALL {
        Some(6)
    } else {
        None
    })
}

/// Detect axial chirality from a freshly parsed molecule and optional geometry.
/// Derives the provisional valences and hybridizations used by file imports.
/// Only bond stereo tags change; ambiguous cases retain their previous tags.
/// Geometry uses chemistry coordinates (Y up). Errors never change the caller.
pub fn detect_atropisomers(
    graph: &Graph,
    metadata: &Metadata,
    directions: &[Direction],
    conformer: Option<&Conformer>,
) -> Result<Metadata> {
    detect_atropisomers_with_bounds(
        graph,
        metadata,
        directions,
        conformer,
        super::CoordinateBounds::Drawing,
    )
}

pub(crate) fn detect_atropisomers_with_bounds(
    graph: &Graph,
    metadata: &Metadata,
    directions: &[Direction],
    conformer: Option<&Conformer>,
    bounds: super::CoordinateBounds,
) -> Result<Metadata> {
    graph.validate().map_err(AtropError::Graph)?;
    metadata.validate(graph).map_err(AtropError::Metadata)?;
    if directions.len() != graph.bonds.len()
        || conformer.is_some_and(|c| {
            c.positions.len() != graph.atoms.len()
                || c.positions.iter().any(|&p| !bounds.allows(p, 1e100))
        })
    {
        return Err(AtropError::Invalid("Invalid directions or coordinates"));
    }
    let mut neighbors: Vec<Neighbors> = (0..graph.atoms.len())
        .map(|_| Neighbors::default())
        .collect();
    for (id, (bond, dir)) in graph.bonds.iter().zip(directions).enumerate() {
        for (a, b) in [(bond.a, bond.b), (bond.b, bond.a)] {
            let adjacent = neighbors
                .get_mut(a)
                .ok_or(AtropError::Invalid("Missing endpoint"))?;
            adjacent.bonds.push((id, b));
            adjacent.unknown += usize::from(*dir == Direction::Unknown);
        }
        if matches!(bond.order, 1 | 4) && matches!(dir, Direction::Wedge | Direction::Hash) {
            neighbors
                .get_mut(bond.a)
                .ok_or(AtropError::Invalid("Missing wedge atom"))?
                .wedges += 1;
        }
    }
    let mut candidates = Vec::new();
    for (id, (bond, dir)) in graph.bonds.iter().zip(directions).enumerate() {
        if bond.order != 1 || at(&metadata.bonds, id)?.stereo == 1 {
            continue;
        }
        let own = usize::from(matches!(dir, Direction::Wedge | Direction::Hash));
        if at(&neighbors, bond.a)?.wedges > own || at(&neighbors, bond.b)?.wedges > 0 {
            candidates.push(id);
        }
    }
    let mut result = metadata.clone();
    if candidates.is_empty() {
        return Ok(result);
    }
    let cache = graph.provisional_valences().map_err(AtropError::Graph)?;
    let degree = |id| -> Result<usize> {
        Ok(at(&neighbors, id)?.bonds.len()
            + usize::from(at(&graph.atoms, id)?.explicit_hydrogens)
            + at(&cache, id)?.implicit_hydrogens as usize)
    };
    let mut possible = false;
    for &id in &candidates {
        let bond = at(&graph.bonds, id)?;
        possible |= (2..=3).contains(&degree(bond.a)?) && (2..=3).contains(&degree(bond.b)?);
    }
    if !possible {
        return Ok(result);
    }
    let conjugated =
        electronic::conjugation_cached(graph, Some(&cache)).map_err(AtropError::Graph)?;
    let tags = metadata
        .atoms
        .iter()
        .map(|a| a.chiral_tag)
        .collect::<Vec<_>>();
    let hybrids = electronic::hybridization_cached(graph, &tags, &conjugated, Some(&cache))
        .map_err(AtropError::Graph)?;
    for id in candidates {
        let bond = at(&graph.bonds, id)?;
        if *at(&hybrids, bond.a)? != Hybridization::Sp2
            || *at(&hybrids, bond.b)? != Hybridization::Sp2
        {
            continue;
        }
        let (Some(a), Some(b)) = (
            end(&neighbors, bond.a, id, directions)?,
            end(&neighbors, bond.b, id, directions)?,
        ) else {
            continue;
        };
        if let Some(stereo) = perceive([a, b], directions, conformer)? {
            result
                .bonds
                .get_mut(id)
                .ok_or(AtropError::Invalid("Missing axis"))?
                .stereo = stereo;
        }
    }
    Ok(result)
}
