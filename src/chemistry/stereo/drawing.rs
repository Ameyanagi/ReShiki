//! Wedge perception from RDKit Chirality.cpp (2026.03.6).
//! Copyright (C) 2004-2024 Greg Landrum and other RDKit contributors.
//! Point arithmetic from Geometry/point.h, Copyright (C) 2003-2025.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
use super::at;
use crate::chemistry::{
    graph::{Graph, Valence},
    kekulize::Direction,
    ranking::Metadata,
};
use serde::{Deserialize, Serialize};
mod bonds;
pub mod wedging;
pub use bonds::{BondGeometry, detect_bond_stereo, double_bond_directions};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Point3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}
impl Point3 {
    fn sub(self, other: Self) -> Self {
        Self {
            x: self.x - other.x,
            y: self.y - other.y,
            z: self.z - other.z,
        }
    }
    fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }
    fn cross(self, other: Self) -> Self {
        Self {
            x: self.y * other.z - self.z * other.y,
            y: self.z * other.x - self.x * other.z,
            z: self.x * other.y - self.y * other.x,
        }
    }
    fn squared(self) -> f64 {
        self.dot(self)
    }
    fn unit(self) -> Result<Self, String> {
        let length = self.squared().sqrt();
        if length < 1e-16 {
            return Err("Cannot normalize a zero-length stereo vector".into());
        }
        Ok(Self {
            x: self.x / length,
            y: self.y / length,
            z: self.z / length,
        })
    }
    fn valid(self) -> bool {
        [self.x, self.y, self.z]
            .iter()
            .all(|v| v.is_finite() && v.abs() <= 1e100)
    }
}

#[derive(Debug, Serialize)]
pub struct DrawnStereo {
    pub graph: Graph,
    pub metadata: Metadata,
    pub valences: Vec<Valence>,
}

/// Perceive atom winding from wedge/hash bonds and drawing coordinates.
/// Coordinates use the chemistry convention (Y up), not screen coordinates.
/// None represents no conformer. Existing tags are retained unless requested;
/// ambiguous geometry clears newly considered tags as in the reference.
/// CIP labels and double-bond stereochemistry are separate passes.
pub fn from_directions(
    graph: &Graph,
    metadata: &Metadata,
    directions: &[Direction],
    positions: Option<&[Point3]>,
    replace_existing: bool,
) -> Result<DrawnStereo, String> {
    let cache = graph.provisional_valences()?;
    metadata.validate(graph)?;
    if directions.len() != graph.bonds.len() {
        return Err("Invalid stereo direction count".into());
    }
    if positions.is_some_and(|p| p.len() != graph.atoms.len() || p.iter().any(|&p| !p.valid())) {
        return Err("Invalid or excessive stereo coordinates".into());
    }
    let mut result = DrawnStereo {
        graph: graph.clone(),
        metadata: metadata.clone(),
        valences: cache,
    };
    let Some(positions) = positions else {
        return Ok(result);
    };
    let mut neighbors = vec![Vec::new(); graph.atoms.len()];
    for (id, bond) in graph.bonds.iter().enumerate() {
        for atom in [bond.a, bond.b] {
            neighbors
                .get_mut(atom)
                .ok_or("Missing stereo endpoint")?
                .push(id);
        }
    }
    let mut assigned = vec![false; graph.atoms.len()];
    let mut promoted = Vec::new();
    for (id, bond) in graph.bonds.iter().enumerate() {
        let atom = bond.a;
        let existing = at(&result.metadata.atoms, atom)?.chiral_tag;
        match *at(directions, id)? {
            Direction::Unknown if *at(&assigned, atom)? || replace_existing => {
                result
                    .metadata
                    .atoms
                    .get_mut(atom)
                    .ok_or("Missing stereo atom")?
                    .chiral_tag = 0;
                *assigned.get_mut(atom).ok_or("Missing stereo assignment")? = true;
            }
            Direction::Wedge | Direction::Hash => {
                if *at(&assigned, atom)? || !replace_existing && existing != 0 {
                    continue;
                }
                let code = pseudo3d(graph, directions, positions, at(&neighbors, atom)?, id)?;
                result
                    .metadata
                    .atoms
                    .get_mut(atom)
                    .ok_or("Missing stereo atom")?
                    .chiral_tag = code;
                if code != 0 {
                    *assigned.get_mut(atom).ok_or("Missing stereo assignment")? = true;
                }
                let edited = result
                    .graph
                    .atoms
                    .get_mut(atom)
                    .ok_or("Missing stereo atom")?;
                if at(&neighbors, atom)?.len() == 3
                    && edited.explicit_hydrogens == 0
                    && at(&result.valences, atom)?.implicit_hydrogens == 1
                {
                    edited.explicit_hydrogens = 1;
                    promoted.push(atom);
                }
            }
            _ => {}
        }
    }
    // Geometry does not read valences. Each atom is promoted at most once;
    // defer these independent refreshes to one linear environment traversal.
    result.valences = result
        .graph
        .refresh_atoms(&result.valences, &promoted, true)?;
    Ok(result)
}

fn swap(order: &mut [usize], a: usize, b: usize) -> Result<(), String> {
    let (av, bv) = (*at(order, a)?, *at(order, b)?);
    *order.get_mut(a).ok_or("Missing stereo permutation")? = bv;
    *order.get_mut(b).ok_or("Missing stereo permutation")? = av;
    Ok(())
}

/// Assign cis/trans references from the first neighboring up/down bond on
/// each side, preserving graph insertion order. Explicit "any" stereo remains
/// unchanged. This is not geometric double-bond perception or CIP assignment.
pub fn bond_stereo_from_directions(
    graph: &Graph,
    metadata: &Metadata,
    directions: &[Direction],
) -> Result<Metadata, String> {
    graph.validate()?;
    metadata.validate(graph)?;
    if directions.len() != graph.bonds.len() {
        return Err("Invalid stereo direction count".into());
    }
    // Cache each first neighbor once: repeated scans would become quadratic
    // for high-degree atoms with many incident double bonds.
    let mut first = vec![None; graph.atoms.len()];
    for (id, bond) in graph.bonds.iter().enumerate() {
        if bond.order != 2 && matches!(*at(directions, id)?, Direction::Up | Direction::Down) {
            for atom in [bond.a, bond.b] {
                first
                    .get_mut(atom)
                    .ok_or("Missing stereo endpoint")?
                    .get_or_insert(id);
            }
        }
    }
    let mut result = metadata.clone();
    for (id, bond) in graph.bonds.iter().enumerate() {
        if bond.order != 2 || at(&metadata.bonds, id)?.stereo == 1 {
            continue;
        }
        let (Some(left), Some(right)) = (*at(&first, bond.a)?, *at(&first, bond.b)?) else {
            continue;
        };
        let (lb, rb) = (at(&graph.bonds, left)?, at(&graph.bonds, right)?);
        let a = if lb.a == bond.a { lb.b } else { lb.a };
        let b = if rb.a == bond.b { rb.b } else { rb.a };
        let left_up = (*at(directions, left)? == Direction::Up) ^ (lb.a == bond.a);
        let right_up = (*at(directions, right)? == Direction::Up) ^ (rb.b == bond.b);
        let target = result.bonds.get_mut(id).ok_or("Missing stereo bond")?;
        target.stereo_atoms = vec![a, b];
        target.stereo = if left_up == right_up { 5 } else { 4 };
    }
    Ok(result)
}

fn needs_swap(cp1: Point3, cp2: Point3, dp1: f64, dp2: f64) -> bool {
    if dp1.abs() - 1.0 > -0.001 {
        return cp2.z < 0.0;
    }
    if dp2.abs() - 1.0 > -0.001 && cp1.z < 0.0 {
        return true;
    }
    if cp1.z * cp2.z < -0.001 {
        return cp1.z < cp2.z;
    }
    if dp1 * dp2 < -0.001 {
        return dp1 < dp2;
    }
    dp1.abs() > dp2.abs()
}

fn pseudo3d(
    graph: &Graph,
    directions: &[Direction],
    positions: &[Point3],
    adjacent: &[usize],
    reference: usize,
) -> Result<u8, String> {
    if adjacent.len() > 4 {
        return Ok(0);
    }
    let bond = at(&graph.bonds, reference)?;
    let mut center = *at(positions, bond.a)?;
    center.z = 0.0;
    let mut tip = *at(positions, bond.b)?;
    let length = center.sub(tip).squared().sqrt();
    let offset = 0.1 * if length != 0.0 { length } else { 1.0 };
    tip.z = if *at(directions, reference)? == Direction::Wedge {
        offset
    } else {
        -offset
    };
    let mut vectors = Vec::with_capacity(adjacent.len());
    let mut ref_index = None;
    let mut all_single = true;
    for (index, &id) in adjacent.iter().enumerate() {
        let neighbor = at(&graph.bonds, id)?;
        let other = if neighbor.a == bond.a {
            neighbor.b
        } else {
            neighbor.a
        };
        let mut point = *at(positions, other)?;
        if id == reference {
            ref_index = Some(index);
            point = tip;
        } else {
            point.z = match *at(directions, id)? {
                Direction::Wedge if neighbor.a == bond.a => offset,
                Direction::Hash if neighbor.a == bond.a => -offset,
                _ => 0.0,
            };
            if center.sub(point).squared() < 0.001 {
                return Ok(0);
            }
        }
        all_single &= neighbor.order == 1;
        vectors.push(point.sub(center).unit()?);
    }
    let n = vectors.len();
    if !(3..=4).contains(&n) {
        return Ok(0);
    }
    for i in 0..n {
        for j in 0..i {
            if at(&vectors, i)?.sub(*at(&vectors, j)?).squared() < 0.001 {
                return Ok(0);
            }
        }
    }
    if !all_single && !matches!(at(&graph.atoms, bond.a)?.atomic_number, 15 | 16) {
        return Ok(0);
    }
    let mut order = [0, 1, 2, 3];
    let mut prefactor = 1.0;
    let ref_index = ref_index.ok_or("Missing reference stereo bond")?;
    if ref_index != 0 {
        swap(&mut order, 0, ref_index)?;
        prefactor *= -1.0;
    }
    let [i0, i1, i2, _] = order;
    if n == 4
        && at(&vectors, i1)?.cross(*at(&vectors, i2)?).squared() < 0.01
        && at(&vectors, i1)?.cross(*at(&vectors, i0)?).squared() > 0.01
    {
        let z = -at(&vectors, i0)?.z;
        vectors.get_mut(i1).ok_or("Missing stereo vector")?.z = z;
    }
    if n == 3 {
        let (v0, v1, v2) = (*at(&vectors, i0)?, *at(&vectors, i1)?, *at(&vectors, i2)?);
        if needs_swap(v0.cross(v1), v0.cross(v2), v0.dot(v1), v0.dot(v2)) {
            swap(&mut order, 1, 2)?;
            prefactor *= -1.0;
        }
    } else {
        let mut sorted = Vec::new();
        for &id in order.iter().skip(1) {
            let (v0, vi) = (*at(&vectors, i0)?, *at(&vectors, id)?);
            let sign = if v0.cross(vi).z < -0.001 { -1 } else { 1 };
            sorted.push((sign, f64::from(sign) * v0.dot(vi), id));
        }
        // C++ tuple order treats signed zeros as equal. All values are finite.
        sorted.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| {
                    if b.1 < a.1 {
                        std::cmp::Ordering::Less
                    } else if b.1 > a.1 {
                        std::cmp::Ordering::Greater
                    } else {
                        std::cmp::Ordering::Equal
                    }
                })
                .then_with(|| b.2.cmp(&a.2))
        });
        let mut changed = 0;
        for (old, (_, _, new)) in order.iter_mut().skip(1).zip(sorted) {
            changed += usize::from(*old != new);
            *old = new;
        }
        if changed == 2 {
            prefactor *= -1.0;
        }
    }
    let mut ordered = order
        .iter()
        .take(n)
        .map(|&i| at(&vectors, i).copied())
        .collect::<Result<Vec<_>, _>>()?;
    for i in 0..n {
        for j in i + 1..n {
            let (a, b) = (*at(&ordered, i)?, *at(&ordered, j)?);
            if a.z * b.z < -0.001 && a.cross(b).squared() < 0.01 {
                if n == 4 && (a.dot(b) + 1.0).abs() < 0.001 && (j - i == 1 || i == 0 && j == 3) {
                    ordered.get_mut(j).ok_or("Missing stereo vector")?.z = 0.0;
                } else {
                    return Ok(0);
                }
            }
        }
    }
    let (v0, v1, v2) = (*at(&ordered, 0)?, *at(&ordered, 1)?, *at(&ordered, 2)?);
    if n == 3 {
        let conflict = if v1.z * v0.z < -0.0001 && v2.z.abs() < 0.0001 {
            v2.cross(v0).z * v2.cross(v1).z < -0.0001
        } else if v2.z * v0.z < -0.0001 && v1.z.abs() < 0.0001 {
            v1.cross(v0).z * v1.cross(v2).z < -0.0001
        } else {
            false
        };
        if conflict {
            return Ok(0);
        }
    }
    let (mut flat1, mut flat2) = (v1, v2);
    flat1.z = 0.0;
    flat2.z = 0.0;
    let mut cross = flat1.cross(flat2);
    if n == 3 {
        if cross.squared() < 0.00031 {
            flat1.z = -v0.z;
            flat2.z = -v0.z;
            cross = flat1.cross(flat2);
        }
    } else if cross.squared() < 0.01 && at(&ordered, 3)?.z.abs() < 0.0001 {
        ordered.get_mut(3).ok_or("Missing stereo vector")?.z = -v0.z;
    }
    let mut volume = cross.dot(v0);
    if n == 4 {
        let v3 = *at(&ordered, 3)?;
        let mut flat3 = v3;
        flat3.z = 0.0;
        let volume2 = flat1.cross(flat3).dot(v0);
        if volume.abs() < 0.001 {
            if volume2.abs() < 0.001 {
                return Ok(0);
            }
            volume = volume2;
            prefactor *= -1.0;
        } else if volume * volume2 > 0.0 && volume2.abs() > 0.00174 && v1.dot(v2) < v1.dot(v3) {
            volume = volume2;
            prefactor *= -1.0;
        } else if volume.abs() < 0.00174 && volume2.abs() > 0.00174 {
            if volume * volume2 < 0.0 {
                prefactor *= -1.0;
            }
            volume = volume2;
        }
    }
    volume *= prefactor;
    Ok(if volume > 0.00174 {
        2
    } else if volume < -0.00174 {
        1
    } else {
        0
    })
}
