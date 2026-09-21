//! 3D atom perception adapted from RDKit Chirality.cpp (2026.03.6).
//! Copyright (C) 2004-2024 Greg Landrum and other RDKit contributors.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
use super::{Point3, wedging::Conformer};
use crate::chemistry::{graph::Graph, kekulize::Direction, ranking::Metadata};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum SpatialError {
    #[error("Invalid 3D stereo graph: {0}")]
    Graph(String),
    #[error("Invalid 3D stereo metadata: {0}")]
    Metadata(String),
    #[error("Invalid 3D stereo input: {0}")]
    Invalid(&'static str),
    #[error("3D stereo geometry: {0}")]
    Geometry(String),
}
type Result<T> = std::result::Result<T, SpatialError>;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpatialAnnotations {
    /// Preserve property absence separately from an explicit zero value.
    pub non_explicit: Vec<Option<i32>>,
    pub done: Option<bool>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpatialOptions {
    pub replace_existing: bool,
    pub allow_nontetrahedral: bool,
}
impl Default for SpatialOptions {
    fn default() -> Self {
        Self {
            replace_existing: true,
            allow_nontetrahedral: true,
        }
    }
}
#[derive(Debug, Serialize)]
pub struct SpatialStereo {
    pub metadata: Metadata,
    pub annotations: SpatialAnnotations,
}
fn at<T>(items: &[T], i: usize) -> Result<&T> {
    items
        .get(i)
        .ok_or(SpatialError::Invalid("Missing geometry item"))
}
fn volume(vectors: &[Point3], x: usize, y: usize, z: usize) -> Result<f64> {
    Ok(at(vectors, x)?.dot(at(vectors, y)?.cross(*at(vectors, z)?)))
}
fn permutation(
    vectors: &[Point3],
    triple: (usize, usize, usize),
    choices: (u32, u32),
) -> Result<u32> {
    Ok(if volume(vectors, triple.0, triple.1, triple.2)? >= 0. {
        choices.0
    } else {
        choices.1
    })
}
fn octahedral(pair: &[usize], v: &[Point3]) -> Result<u32> {
    let (triple, choices) = match (*at(pair, 0)?, *at(pair, 1)?, *at(pair, 2)?) {
        (2, _, 4) => ((0, 3, 4), (28, 27)),
        (2, _, 5) => ((0, 2, 3), (25, 30)),
        (2, _, _) => ((0, 2, 3), (26, 29)),
        (3, 4, _) => ((0, 3, 4), (22, 21)),
        (3, 5, _) => ((0, 1, 3), (19, 24)),
        (3, _, _) => ((0, 1, 3), (20, 23)),
        (4, 3, _) => ((0, 2, 4), (13, 12)),
        (4, 5, _) => ((0, 1, 2), (6, 18)),
        (4, _, _) => ((0, 1, 2), (7, 17)),
        (5, 3, _) => ((0, 2, 3), (11, 9)),
        (5, 4, _) => ((0, 1, 2), (3, 16)),
        (5, _, _) => ((0, 1, 2), (5, 15)),
        (_, 3, _) => ((0, 2, 3), (10, 8)),
        (_, 4, _) => ((0, 1, 2), (1, 2)),
        _ => ((0, 1, 2), (4, 14)),
    };
    permutation(v, triple, choices)
}
fn angle(a: Point3, b: Point3) -> f64 {
    (a.dot(b) / (a.squared() * b.squared()).sqrt())
        .clamp(-1., 1.)
        .acos()
}

fn non_tetrahedral(
    number: u8,
    center: Point3,
    neighbors: &[usize],
    positions: &[Point3],
) -> Result<Option<(u8, u32)>> {
    if number < 15 {
        return Ok(None);
    }
    let mut vectors = Vec::with_capacity(6);
    for &id in neighbors {
        // Preserve native validation order: a zero vector among the first six
        // is an error even when there are more neighbors than supported.
        if vectors.len() == 6 {
            return Ok(None);
        }
        vectors.push(
            at(positions, id)?
                .sub(center)
                .unit()
                .map_err(SpatialError::Geometry)?,
        );
    }
    let count = vectors.len();
    if count < 3 {
        return Ok(None);
    }
    let mut pair = [0usize; 6];
    let mut pairs = 0;
    for i in 0..count {
        for j in i + 1..count {
            if at(&vectors, i)?.dot(*at(&vectors, j)?) < -0.9 {
                if *at(&pair, i)? != 0 || *at(&pair, j)? != 0 {
                    return Ok(None);
                }
                *pair
                    .get_mut(i)
                    .ok_or(SpatialError::Invalid("Missing opposite ligand"))? = j + 1;
                *pair
                    .get_mut(j)
                    .ok_or(SpatialError::Invalid("Missing opposite ligand"))? = i + 1;
                pairs += 1;
            }
        }
    }
    let p0 = *at(&pair, 0)?;
    let p1 = *at(&pair, 1)?;
    let p2 = *at(&pair, 2)?;
    let result = match (pairs, count) {
        (1, 3) => (
            6,
            match p0 {
                0 => 3,
                2 => 2,
                _ => 1,
            },
        ),
        (1, 4) => {
            let (angle_indices, oct_triple, oct_perms, tri_triple, tri_perms) = match (p0, p1) {
                (2, _) => ((2, 3), (0, 2, 3), (25, 29), (0, 2, 3), (7, 8)),
                (3, _) => ((1, 3), (0, 1, 3), (19, 23), (0, 1, 3), (5, 6)),
                (4, _) => ((1, 2), (0, 1, 2), (6, 17), (0, 1, 2), (3, 4)),
                (_, 3) => ((0, 3), (0, 1, 3), (10, 8), (1, 0, 3), (13, 14)),
                (_, 4) => ((0, 2), (0, 1, 3), (1, 2), (1, 0, 2), (10, 12)),
                _ => ((0, 1), (0, 1, 3), (4, 14), (3, 0, 1), (16, 19)),
            };
            if angle(
                *at(&vectors, angle_indices.0)?,
                *at(&vectors, angle_indices.1)?,
            ) < 100. * std::f64::consts::PI / 180.
            {
                (8, permutation(&vectors, oct_triple, oct_perms)?)
            } else {
                (7, permutation(&vectors, tri_triple, tri_perms)?)
            }
        }
        (1, 5) => {
            let (triple, choices) = match (p0, p1, p2) {
                (2, _, _) => ((0, 2, 3), (7, 8)),
                (3, _, _) => ((0, 1, 3), (5, 6)),
                (4, _, _) => ((0, 1, 2), (3, 4)),
                (5, _, _) => ((0, 1, 2), (1, 2)),
                (_, 3, _) => ((1, 0, 3), (13, 14)),
                (_, 4, _) => ((1, 0, 2), (10, 12)),
                (_, 5, _) => ((1, 0, 2), (9, 11)),
                (_, _, 4) => ((2, 0, 1), (16, 19)),
                (_, _, 5) => ((2, 0, 1), (15, 20)),
                _ => ((3, 0, 1), (17, 18)),
            };
            (7, permutation(&vectors, triple, choices)?)
        }
        (2, 4) => (
            6,
            match p0 {
                2 => 2,
                3 => 1,
                _ => 3,
            },
        ),
        (2, 5) | (3, 6) => (8, octahedral(&pair, &vectors)?),
        _ => return Ok(None),
    };
    Ok(Some(result))
}

/// Infer atom winding and coordination permutations from a 3D conformer.
/// This pass neither assigns CIP labels nor edits hydrogens/connectivity.
/// A missing/2D conformer retains annotations. Errors never mutate inputs.
pub fn from_3d(
    graph: &Graph,
    metadata: &Metadata,
    directions: &[Direction],
    conformer: Option<&Conformer>,
    annotations: &SpatialAnnotations,
    options: SpatialOptions,
) -> Result<SpatialStereo> {
    let valences = graph.provisional_valences().map_err(SpatialError::Graph)?;
    metadata.validate(graph).map_err(SpatialError::Metadata)?;
    if directions.len() != graph.bonds.len() || annotations.non_explicit.len() != graph.atoms.len()
    {
        return Err(SpatialError::Invalid("Annotation dimensions changed"));
    }
    if conformer.is_some_and(|c| {
        c.positions.len() != graph.atoms.len() || c.positions.iter().any(|p| !p.valid())
    }) {
        return Err(SpatialError::Invalid(
            "Invalid or excessive conformer coordinates",
        ));
    }
    let mut result = SpatialStereo {
        metadata: metadata.clone(),
        annotations: annotations.clone(),
    };
    let Some(conf) = conformer.filter(|c| c.is_3d) else {
        return Ok(result);
    };
    result.annotations.done = None;
    let mut neighbors = vec![Vec::new(); graph.atoms.len()];
    let mut wiggly = vec![false; graph.atoms.len()];
    let mut explicit = metadata
        .atoms
        .iter()
        .map(|a| a.chiral_tag != 0)
        .collect::<Vec<_>>();
    for (i, bond) in graph.bonds.iter().enumerate() {
        for (atom, other) in [(bond.a, bond.b), (bond.b, bond.a)] {
            neighbors
                .get_mut(atom)
                .ok_or(SpatialError::Invalid("Missing atom neighbors"))?
                .push((i, other));
        }
        let dir = *at(directions, i)?;
        if matches!(dir, Direction::Wedge | Direction::Hash) {
            *explicit
                .get_mut(bond.a)
                .ok_or(SpatialError::Invalid("Missing wedge atom"))? = true;
        }
        if bond.order == 1 && (dir == Direction::Unknown || at(&metadata.bonds, i)?.unknown_stereo)
        {
            *wiggly
                .get_mut(bond.a)
                .ok_or(SpatialError::Invalid("Missing unknown-stereo atom"))? = true;
        }
    }
    for (i, atom) in graph.atoms.iter().enumerate() {
        let meta = result
            .metadata
            .atoms
            .get_mut(i)
            .ok_or(SpatialError::Invalid("Missing atom metadata"))?;
        if !options.replace_existing && meta.chiral_tag != 0 {
            continue;
        }
        meta.chiral_tag = 0;
        let adjacent = at(&neighbors, i)?;
        let mut affecting = Vec::new();
        for &(bond, other) in adjacent {
            let b = at(&graph.bonds, bond)?;
            // Hydrogen bonds affect this native geometry test; only an
            // outgoing dative bond is ignored among our supported bond types.
            if b.order != 5 || b.a != i {
                affecting.push(other);
            }
        }
        let degree = affecting.len();
        let total = degree
            + usize::from(atom.explicit_hydrogens)
            + at(&valences, i)?.implicit_hydrogens as usize;
        if degree < 3 || total > 6 || *at(&wiggly, i)? {
            continue;
        }
        let center = *at(&conf.positions, i)?;
        let coordination = if options.allow_nontetrahedral {
            non_tetrahedral(
                atom.atomic_number,
                center,
                &adjacent.iter().map(|&(_, a)| a).collect::<Vec<_>>(),
                &conf.positions,
            )?
        } else {
            None
        };
        if let Some((tag, permutation)) = coordination {
            meta.chiral_tag = tag;
            meta.chiral_permutation = Some(permutation);
        } else if total <= 4 && (matches!(atom.atomic_number, 16 | 34) || total == 4) {
            let vectors = affecting
                .iter()
                .map(|&a| Ok(at(&conf.positions, a)?.sub(center)))
                .collect::<Result<Vec<_>>>()?;
            let mut vol = volume(&vectors, 0, 1, 2)?;
            if (-0.1..=0.1).contains(&vol) && degree == 4 {
                vol = -volume(&vectors, 0, 1, 3)?;
            }
            meta.chiral_tag = if vol < -0.1 {
                1
            } else if vol > 0.1 {
                2
            } else {
                0
            };
        }
        if meta.chiral_tag != 0 && !*at(&explicit, i)? {
            *result
                .annotations
                .non_explicit
                .get_mut(i)
                .ok_or(SpatialError::Invalid("Missing 3D stereo flag"))? = Some(1);
        }
    }
    Ok(result)
}
