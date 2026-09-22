//! Native coordination lookup tables and RDDepictor initial point selection.
use super::{Error, IdealLengths, Input, at, input_number, point};
use crate::chemistry::{
    depict::{
        geometry::{Coordinates, Point},
        rings::Fragment,
    },
    native_order,
};

// Pinned RDKit uses the rounded decimal 0.707107. This exact rational spelling
// preserves that value without substituting the more precise square-root constant.
const ISQRT2: f64 = 707_107.0 / 1_000_000.0;

impl Input<'_> {
    /// DepictorLocal uses supplied depict ranks, distinct from CIP properties.
    /// Equal keys retain the pinned platform's comparison-sort policy.
    pub fn ranked_neighbors(&self, center: usize, ranks: &[i32]) -> Result<Vec<usize>, Error> {
        if ranks.len() != self.graph.atoms.len() {
            return Err(Error::Invalid("depict rank count"));
        }
        let neighbors = self.neighbors(center)?;
        self.work().spend(neighbors.len())?;
        let keys = neighbors
            .iter()
            .map(|&id| at(ranks, id).copied())
            .collect::<Result<Vec<_>, _>>()?;
        let order = native_order::indices(&keys).map_err(|error| match error {
            native_order::Error::Limit => Error::Limit,
            _ => Error::Invalid("native rank sort"),
        })?;
        order
            .into_iter()
            .map(|id| at(neighbors, id).copied())
            .collect()
    }
    /// Direct getChiralAcrossAtom semantics, including absent/invalid
    /// permutations and truncated coordination spheres.
    pub fn across(&self, center: usize, ligand: usize) -> Result<Option<usize>, Error> {
        at(&self.graph.atoms, ligand)?;
        let meta = at(&self.metadata.atoms, center)?;
        let neighbors = self.neighbors(center)?;
        let Some(position) = neighbors.iter().position(|&id| id == ligand) else {
            return Ok(None);
        };
        let permutation = meta.chiral_permutation.unwrap_or(0) as usize;
        if permutation == 0 {
            return Ok(None);
        }
        let index = match meta.chiral_tag {
            6 if neighbors.len() <= 4 => SP.get(permutation).and_then(|row| row.get(position)),
            7 if neighbors.len() <= 5 => TB.get(permutation).and_then(|row| row.get(position)),
            8 if neighbors.len() <= 6 => OH.get(permutation).and_then(|row| row.get(position)),
            _ => None,
        };
        Ok(index
            .and_then(|&index| neighbors.get(usize::from(index)))
            .copied())
    }
    /// Forward/reverse TBP axial query in bond-insertion order.
    pub fn axial(&self, center: usize, reverse: bool) -> Result<Option<usize>, Error> {
        let meta = at(&self.metadata.atoms, center)?;
        let neighbors = self.neighbors(center)?;
        if meta.chiral_tag != 7 || neighbors.len() > 5 {
            return Ok(None);
        }
        let permutation = meta.chiral_permutation.unwrap_or(0) as usize;
        let index = AXIAL
            .get(permutation)
            .and_then(|row| row.get(usize::from(reverse)));
        Ok(index
            .and_then(|&index| neighbors.get(usize::from(index)))
            .copied())
    }
    /// Native ideal ligand angle in degrees. Source behavior for identical or
    /// nonadjacent queried atoms is preserved; these are queries, not geometry.
    pub fn ideal_angle(&self, center: usize, first: usize, second: usize) -> Result<f64, Error> {
        at(&self.graph.atoms, first)?;
        at(&self.graph.atoms, second)?;
        let tag = at(&self.metadata.atoms, center)?.chiral_tag;
        Ok(match tag {
            6 | 8 => {
                if self.across(center, first)? == Some(second) {
                    180.0
                } else {
                    90.0
                }
            }
            7 => {
                if self.across(center, first)? == Some(second) {
                    180.0
                } else {
                    let forward = self.axial(center, false)?;
                    let reverse = self.axial(center, true)?;
                    if [forward, reverse]
                        .into_iter()
                        .any(|id| id == Some(first) || id == Some(second))
                    {
                        90.0
                    } else {
                        120.0
                    }
                }
            }
            _ => 0.0,
        })
    }
    /// One SP/TBP/OH seed. Other tags yield None. Captured per-constructor
    /// lengths are explicit; the current global bond length is not used here.
    pub fn coordination(
        &self,
        center: usize,
        ranks: &[i32],
        lengths: &IdealLengths,
    ) -> Result<Option<Fragment>, Error> {
        let tag = at(&self.metadata.atoms, center)?.chiral_tag;
        let length = match tag {
            6 => lengths.square_planar,
            7 => lengths.trigonal_bipyramidal,
            8 => lengths.octahedral,
            _ => return Ok(None),
        };
        let length = input_number(length)?;
        let neighbors = self.ranked_neighbors(center, ranks)?;
        if tag == 6 && neighbors.is_empty() {
            return Err(Error::Invalid("coordination degree"));
        }
        let mut points = Coordinates::new();
        points.insert(center, Point::default());
        let mut work = self.work();
        work.spend(
            neighbors
                .len()
                .checked_mul(neighbors.len())
                .ok_or(Error::Limit)?,
        )?;
        match tag {
            6 => {
                let ideal = [
                    Point {
                        x: ISQRT2 * length,
                        y: ISQRT2 * length,
                    },
                    Point {
                        x: ISQRT2 * length,
                        y: -ISQRT2 * length,
                    },
                    Point {
                        x: -ISQRT2 * length,
                        y: -ISQRT2 * length,
                    },
                    Point {
                        x: -ISQRT2 * length,
                        y: ISQRT2 * length,
                    },
                ];
                let first = *at(&neighbors, 0)?;
                points.insert(first, point(*at(&ideal, 0)?)?);
                let mut full = false;
                for &id in neighbors.iter().skip(1) {
                    let angle = self.ideal_angle(center, first, id)?;
                    let index = if (angle - 180.0).abs() < 0.1 {
                        2
                    } else if !full {
                        full = true;
                        1
                    } else {
                        3
                    };
                    points.insert(id, point(*at(&ideal, index)?)?);
                }
            }
            7 => {
                let ideal = [
                    Point { x: 0.0, y: length },
                    Point { x: 0.0, y: -length },
                    Point {
                        x: -0.866025 * length,
                        y: length / 2.0,
                    },
                    Point {
                        x: -0.866025 * length,
                        y: -length / 2.0,
                    },
                    Point { x: length, y: 0.0 },
                ];
                let first = self.axial(center, false)?;
                let second = self.axial(center, true)?;
                if let Some(id) = first {
                    points.insert(id, point(*at(&ideal, 0)?)?);
                }
                if let Some(id) = second {
                    points.insert(id, point(*at(&ideal, 1)?)?);
                }
                let mut index = 2;
                for &id in &neighbors {
                    if Some(id) != first && Some(id) != second {
                        points.insert(
                            id,
                            point(
                                *at(&ideal, index)
                                    .map_err(|_| Error::Invalid("TBP ideal-point overflow"))?,
                            )?,
                        );
                        index += 1;
                    }
                }
            }
            8 => {
                let ideal = [
                    Point { x: 0.0, y: length },
                    Point { x: 0.0, y: -length },
                    Point {
                        x: 0.866025 * length,
                        y: length / 2.0,
                    },
                    Point {
                        x: 0.866025 * length,
                        y: -length / 2.0,
                    },
                    Point {
                        x: -0.866025 * length,
                        y: -length / 2.0,
                    },
                    Point {
                        x: -0.866025 * length,
                        y: length / 2.0,
                    },
                ];
                let mut first = None;
                let mut second = None;
                for (index, &id) in neighbors.iter().enumerate() {
                    let mut all90 = true;
                    for &other in neighbors.iter().skip(index + 1) {
                        let angle = self.ideal_angle(center, id, other)?;
                        if (angle - 180.0).abs() < 0.1 {
                            first = Some(id);
                            second = Some(other);
                            all90 = false;
                            break;
                        } else if (angle - 90.0).abs() > 0.1 {
                            all90 = false;
                        }
                    }
                    if all90 {
                        first = Some(id);
                    }
                    if first.is_some() {
                        break;
                    }
                }
                if let Some(id) = first {
                    points.insert(id, point(*at(&ideal, 0)?)?);
                }
                if let Some(id) = second {
                    points.insert(id, point(*at(&ideal, 1)?)?);
                }
                let mut equatorial = None;
                let mut opposite = None;
                for &id in &neighbors {
                    if Some(id) == first || Some(id) == second {
                        continue;
                    }
                    if equatorial.is_none() {
                        equatorial = Some(id);
                        points.insert(id, point(*at(&ideal, 2)?)?);
                        opposite = self.across(center, id)?;
                        if let Some(id) = opposite {
                            points.insert(id, point(*at(&ideal, 4)?)?);
                        }
                    } else {
                        if Some(id) == opposite || Some(id) == equatorial {
                            continue;
                        }
                        points.insert(id, point(*at(&ideal, 3)?)?);
                        if let Some(other) = self.across(center, id)? {
                            points.insert(other, point(*at(&ideal, 5)?)?);
                        }
                        break;
                    }
                }
            }
            _ => return Err(Error::Invalid("coordination tag")),
        }
        Ok(Some(self.coordinate_fragment(&points, &mut work)?))
    }
}

const SP: [[u8; 4]; 4] = [[4, 4, 4, 4], [2, 3, 0, 1], [1, 0, 3, 2], [3, 2, 1, 0]];

const TB: [[u8; 5]; 21] = [
    [5, 5, 5, 5, 5],
    [4, 5, 5, 5, 0],
    [4, 5, 5, 5, 0],
    [3, 5, 5, 0, 5],
    [3, 5, 5, 0, 5],
    [2, 5, 0, 5, 5],
    [2, 5, 0, 5, 5],
    [1, 0, 5, 5, 5],
    [1, 0, 5, 5, 5],
    [5, 4, 5, 5, 1],
    [5, 3, 5, 1, 5],
    [5, 4, 5, 5, 1],
    [5, 3, 5, 1, 5],
    [5, 2, 1, 5, 5],
    [5, 2, 1, 5, 5],
    [5, 5, 4, 5, 2],
    [5, 5, 3, 2, 5],
    [5, 5, 5, 4, 3],
    [5, 5, 5, 4, 3],
    [5, 5, 3, 2, 5],
    [5, 5, 4, 5, 2],
];

const OH: [[u8; 6]; 31] = [
    [6, 6, 6, 6, 6, 6],
    [5, 3, 4, 1, 2, 0],
    [5, 3, 4, 1, 2, 0],
    [4, 3, 5, 1, 0, 2],
    [5, 4, 3, 2, 1, 0],
    [4, 5, 3, 2, 0, 1],
    [3, 4, 5, 0, 1, 2],
    [3, 5, 4, 0, 2, 1],
    [5, 2, 1, 4, 3, 0],
    [4, 2, 1, 5, 0, 3],
    [5, 2, 1, 4, 3, 0],
    [4, 2, 1, 5, 0, 3],
    [3, 2, 1, 0, 5, 4],
    [3, 2, 1, 0, 5, 4],
    [5, 4, 3, 2, 1, 0],
    [4, 5, 3, 2, 0, 1],
    [4, 3, 5, 1, 0, 2],
    [3, 5, 4, 0, 2, 1],
    [3, 4, 5, 0, 1, 2],
    [2, 4, 0, 5, 1, 3],
    [2, 5, 0, 4, 3, 1],
    [2, 3, 0, 1, 5, 4],
    [2, 3, 0, 1, 5, 4],
    [2, 5, 0, 4, 3, 1],
    [2, 4, 0, 5, 1, 3],
    [1, 0, 4, 5, 2, 3],
    [1, 0, 5, 4, 3, 2],
    [1, 0, 3, 2, 5, 4],
    [1, 0, 3, 2, 5, 4],
    [1, 0, 5, 4, 3, 2],
    [1, 0, 4, 5, 2, 3],
];

const AXIAL: [[u8; 2]; 21] = [
    [5, 5],
    [0, 4],
    [0, 4],
    [0, 3],
    [0, 3],
    [0, 2],
    [0, 2],
    [0, 1],
    [0, 1],
    [1, 4],
    [1, 4],
    [1, 3],
    [1, 3],
    [1, 2],
    [1, 2],
    [2, 4],
    [2, 3],
    [3, 4],
    [3, 4],
    [2, 3],
    [2, 4],
];
