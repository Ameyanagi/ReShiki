//! Double-bond direction propagation from RDKit Chirality.cpp (2026.03.6).
//! Copyright (C) 2004-2024 Greg Landrum and other RDKit contributors.
//! Dihedral arithmetic from Geometry/point.cpp and point.h, Copyright (C)
//! 2003-2025 Greg Landrum and other RDKit contributors.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
use super::{Point3, at};
use crate::chemistry::{graph::Graph, kekulize::Direction, ranking::Metadata};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

#[cfg(test)]
mod tests;

#[derive(Debug, Serialize)]
pub struct BondGeometry {
    pub metadata: Metadata,
    pub directions: Vec<Direction>,
}

struct Work {
    remaining: usize,
    stored: usize,
}
impl Default for Work {
    fn default() -> Self {
        Self {
            remaining: 50_000_000,
            stored: 0,
        }
    }
}
impl Work {
    fn spend(&mut self, amount: usize) -> Result<(), String> {
        self.remaining = self
            .remaining
            .checked_sub(amount)
            .ok_or("Double-bond stereo work limit exceeded")?;
        Ok(())
    }
    fn store(&mut self, amount: usize) -> Result<(), String> {
        self.stored = self
            .stored
            .checked_add(amount)
            .ok_or("Double-bond stereo storage overflow")?;
        if self.stored > 2_000_000 {
            return Err("Double-bond stereo storage limit exceeded".into());
        }
        Ok(())
    }
}

fn validate(
    graph: &Graph,
    metadata: &Metadata,
    directions: &[Direction],
    positions: Option<&[Point3]>,
    bounds: super::CoordinateBounds,
) -> Result<(), String> {
    graph.validate()?;
    metadata.validate(graph)?;
    if directions.len() != graph.bonds.len() {
        return Err("Invalid double-bond direction count".into());
    }
    // Dihedral norms multiply four squared bond lengths. This range keeps all
    // intermediate products finite without rescaling native tolerance tests.
    if positions
        .is_some_and(|p| p.len() != graph.atoms.len() || p.iter().any(|&p| !bounds.allows(p, 1e37)))
    {
        return Err("Invalid or excessive double-bond coordinates".into());
    }
    Ok(())
}

/// Detect directions from geometry; no conformer leaves annotations unchanged.
/// Rings must be this graph's symmetric SSSR. No absolute E/Z ranking is done.
pub fn detect_bond_stereo(
    graph: &Graph,
    metadata: &Metadata,
    directions: &[Direction],
    positions: Option<&[Point3]>,
    rings: &[Vec<usize>],
) -> Result<BondGeometry, String> {
    validate(
        graph,
        metadata,
        directions,
        positions,
        super::CoordinateBounds::Drawing,
    )?;
    if positions.is_none() {
        return Ok(BondGeometry {
            metadata: metadata.clone(),
            directions: directions.to_vec(),
        });
    }
    double_bond_directions(graph, metadata, directions, positions, rings)
}

/// Assign neighboring single/aromatic bond directions from coordinates, or
/// from existing cis/trans or E/Z tags when coordinates are absent. Preserve
/// the reference traversal order while replacing recursion with a bounded stack.
pub fn double_bond_directions(
    graph: &Graph,
    metadata: &Metadata,
    directions: &[Direction],
    positions: Option<&[Point3]>,
    rings: &[Vec<usize>],
) -> Result<BondGeometry, String> {
    double_bond_directions_with_bounds(
        graph,
        metadata,
        directions,
        positions,
        rings,
        super::CoordinateBounds::Drawing,
    )
}

pub(crate) fn double_bond_directions_with_bounds(
    graph: &Graph,
    metadata: &Metadata,
    directions: &[Direction],
    positions: Option<&[Point3]>,
    rings: &[Vec<usize>],
    bounds: super::CoordinateBounds,
) -> Result<BondGeometry, String> {
    with_work(
        graph,
        metadata,
        directions,
        positions,
        rings,
        &mut Work::default(),
        bounds,
    )
}

struct Context<'a> {
    graph: &'a Graph,
    positions: Option<&'a [Point3]>,
    result: BondGeometry,
    adjacent: Vec<Vec<usize>>,
    needs: Vec<bool>,
    counts: Vec<usize>,
    follows: Vec<Vec<usize>>,
}

fn with_work(
    graph: &Graph,
    metadata: &Metadata,
    directions: &[Direction],
    positions: Option<&[Point3]>,
    rings: &[Vec<usize>],
    work: &mut Work,
    bounds: super::CoordinateBounds,
) -> Result<BondGeometry, String> {
    validate(graph, metadata, directions, positions, bounds)?;
    let mut ctx = Context {
        graph,
        positions,
        result: BondGeometry {
            metadata: metadata.clone(),
            directions: directions.to_vec(),
        },
        adjacent: vec![Vec::new(); graph.atoms.len()],
        needs: vec![false; graph.bonds.len()],
        counts: vec![0; graph.bonds.len()],
        follows: vec![Vec::new(); graph.bonds.len()],
    };
    let mut pairs = HashMap::new();
    for (id, bond) in graph.bonds.iter().enumerate() {
        for atom in [bond.a, bond.b] {
            ctx.adjacent
                .get_mut(atom)
                .ok_or("Missing stereo endpoint")?
                .push(id);
        }
        pairs.insert((bond.a.min(bond.b), bond.a.max(bond.b)), id);
    }
    let mut minimum = vec![usize::MAX; graph.bonds.len()];
    for ring in rings {
        work.store(ring.len())?;
        work.spend(ring.len())?;
        if ring.len() < 3 || ring.iter().collect::<HashSet<_>>().len() != ring.len() {
            return Err("Invalid double-bond ring data".into());
        }
        for (&a, &b) in ring.iter().zip(ring.iter().cycle().skip(1)) {
            let &id = pairs
                .get(&(a.min(b), a.max(b)))
                .ok_or("Missing stereo ring bond")?;
            let size = minimum.get_mut(id).ok_or("Missing ring size")?;
            *size = (*size).min(ring.len());
        }
    }
    let mut priorities = Vec::new();
    for (id, bond) in graph.bonds.iter().enumerate() {
        work.spend(1)?;
        if bond.order != 2
            || at(&metadata.bonds, id)?.stereo == 1
            || *at(directions, id)? == Direction::EitherDouble
            || at(&ctx.adjacent, bond.a)?.len() <= 1
            || at(&ctx.adjacent, bond.b)?.len() <= 1
            || *at(&minimum, id)? < 8
        {
            continue;
        }
        let mut candidate = true;
        let mut score = 0u32;
        for atom in [bond.a, bond.b] {
            for &neighbor in at(&ctx.adjacent, atom)? {
                work.spend(1)?;
                let nb = at(&graph.bonds, neighbor)?;
                if matches!(nb.order, 1 | 4) {
                    *ctx.counts.get_mut(neighbor).ok_or("Missing stereo count")? += 1;
                    let dir = *at(directions, neighbor)?;
                    if nb.a == atom
                        && dir == Direction::Unknown
                        && at(&metadata.bonds, neighbor)?.unknown_stereo
                    {
                        candidate = false;
                    } else {
                        *ctx.needs.get_mut(id).ok_or("Missing stereo flag")? = true;
                        if matches!(dir, Direction::None | Direction::Down | Direction::Up) {
                            *ctx.needs.get_mut(neighbor).ok_or("Missing stereo flag")? = true;
                            // Native priority sums neighbor INDICES, not counts.
                            score = score
                                .checked_add(neighbor as u32)
                                .filter(|s| *s <= i32::MAX as u32)
                                .ok_or("Stereo priority exceeds signed reference range")?;
                            work.store(1)?;
                            // A simple graph cannot have this single bond at
                            // both ends of the same double bond; no duplicates.
                            ctx.follows
                                .get_mut(neighbor)
                                .ok_or("Missing stereo follow-up")?
                                .push(id);
                        }
                    }
                }
                if !candidate {
                    break;
                }
            }
            if !candidate {
                break;
            }
        }
        if candidate {
            if *at(&minimum, id)? == usize::MAX {
                score = score.wrapping_mul(10);
            }
            priorities.push((score, id));
        }
    }
    priorities.sort_unstable_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    for (_, id) in priorities {
        let mut stack = vec![id];
        while let Some(id) = stack.pop() {
            work.spend(1)?;
            let follow = ctx.update(id, work)?;
            if stack.len().saturating_add(follow.len()) > 2_000_000 {
                return Err("Stereo traversal stack limit exceeded".into());
            }
            stack.extend(follow.into_iter().rev());
        }
    }
    Ok(ctx.result)
}

#[derive(Default)]
struct Control {
    chosen: Option<usize>,
    other: Option<usize>,
    unknown: bool,
}
impl Context<'_> {
    fn other(&self, bond: usize, atom: usize) -> Result<usize, String> {
        let bond = at(&self.graph.bonds, bond)?;
        if bond.a == atom {
            Ok(bond.b)
        } else if bond.b == atom {
            Ok(bond.a)
        } else {
            Err("Unconnected stereo controller".into())
        }
    }
    fn controller(&self, id: usize, atom: usize, work: &mut Work) -> Result<Control, String> {
        let mut control = Control::default();
        for &neighbor in at(&self.adjacent, atom)? {
            work.spend(1)?;
            if neighbor == id {
                continue;
            }
            let nb = at(&self.graph.bonds, neighbor)?;
            let dir = *at(&self.result.directions, neighbor)?;
            if matches!(nb.order, 1 | 4)
                && matches!(dir, Direction::None | Direction::Down | Direction::Up)
            {
                if let Some(chosen) = control.chosen {
                    if *at(&self.needs, neighbor)? {
                        if at(&self.counts, neighbor)? > at(&self.counts, chosen)? {
                            control.other = Some(chosen);
                            control.chosen = Some(neighbor);
                        } else {
                            control.other = Some(neighbor);
                        }
                    } else {
                        control.other = Some(chosen);
                        control.chosen = Some(neighbor);
                    }
                } else {
                    control.chosen = Some(neighbor);
                }
            }
            if matches!(nb.order, 1 | 4)
                && (dir == Direction::Unknown
                    || at(&self.result.metadata.bonds, neighbor)?.unknown_stereo)
            {
                control.unknown = true;
                break;
            }
        }
        Ok(control)
    }
    fn mark_unknown(&mut self, id: usize, work: &mut Work) -> Result<(), String> {
        let bond = at(&self.graph.bonds, id)?;
        let (begin, end) = (bond.a.min(bond.b), bond.a.max(bond.b));
        if at(&self.adjacent, begin)?.len() <= 1 || at(&self.adjacent, end)?.len() <= 1 {
            return Ok(());
        }
        let mut first = None;
        let mut second = None;
        for &neighbor in at(&self.adjacent, begin)? {
            work.spend(1)?;
            let other = self.other(neighbor, begin)?;
            if other != end {
                first = Some(first.map_or(other, |old: usize| old.min(other)));
            }
        }
        for &neighbor in at(&self.adjacent, end)? {
            work.spend(1)?;
            let other = self.other(neighbor, end)?;
            if other != begin {
                second = Some(second.map_or(other, |old: usize| old.max(other)));
            }
        }
        let (first, second) = (
            first.ok_or("Missing first stereo reference")?,
            second.ok_or("Missing second stereo reference")?,
        );
        let atom_order = if bond.a == begin {
            vec![first, second]
        } else {
            vec![second, first]
        };
        let target = self
            .result
            .metadata
            .bonds
            .get_mut(id)
            .ok_or("Missing stereo bond")?;
        target.stereo = 1;
        target.stereo_atoms = atom_order;
        Ok(())
    }
    fn set_direction(
        &mut self,
        id: usize,
        atom: usize,
        direction: Direction,
        mut reverse: bool,
    ) -> Result<(), String> {
        if !matches!(direction, Direction::Down | Direction::Up) {
            return Err("Invalid controlling stereo direction".into());
        }
        self.other(id, atom)?;
        if at(&self.graph.bonds, id)?.a != atom {
            reverse = !reverse;
        }
        *self
            .result
            .directions
            .get_mut(id)
            .ok_or("Missing stereo direction")? = if reverse {
            if direction == Direction::Down {
                Direction::Up
            } else {
                Direction::Down
            }
        } else {
            direction
        };
        Ok(())
    }
    fn update(&mut self, id: usize, work: &mut Work) -> Result<Vec<usize>, String> {
        if !*at(&self.needs, id)? {
            return Ok(Vec::new());
        }
        *self.needs.get_mut(id).ok_or("Missing stereo flag")? = false;
        let bond = at(&self.graph.bonds, id)?;
        if bond.order != 2 {
            return Err("Non-double stereo traversal bond".into());
        }
        let (a, b) = (bond.a, bond.b);
        let mut left = self.controller(id, a, work)?;
        if left.unknown {
            self.mark_unknown(id, work)?;
            return Ok(Vec::new());
        }
        let Some(mut first) = left.chosen else {
            return Ok(Vec::new());
        };
        let mut right = self.controller(id, b, work)?;
        if right.unknown {
            self.mark_unknown(id, work)?;
            return Ok(Vec::new());
        }
        let Some(mut second) = right.chosen else {
            return Ok(Vec::new());
        };
        let mut reverse;
        if let Some(positions) = self.positions {
            let (begin, end) = (*at(positions, a)?, *at(positions, b)?);
            let mut p1 = *at(positions, self.other(first, a)?)?;
            let mut p2 = *at(positions, self.other(second, b)?)?;
            if linear(p1.sub(begin), end.sub(begin)) {
                let Some(other) = left.other else {
                    self.mark_unknown(id, work)?;
                    return Ok(Vec::new());
                };
                left.other = Some(first);
                first = other;
                p1 = *at(positions, self.other(first, a)?)?;
                if linear(p1.sub(begin), end.sub(begin)) {
                    self.mark_unknown(id, work)?;
                    return Ok(Vec::new());
                }
            }
            if linear(p2.sub(end), begin.sub(end)) {
                let Some(other) = right.other else {
                    self.mark_unknown(id, work)?;
                    return Ok(Vec::new());
                };
                right.other = Some(second);
                second = other;
                p2 = *at(positions, self.other(second, b)?)?;
                // Preserve the native second-end retry's use of begin here.
                if linear(p2.sub(begin), begin.sub(end)) {
                    self.mark_unknown(id, work)?;
                    return Ok(Vec::new());
                }
            }
            reverse = dihedral(p1, begin, end, p2) >= std::f64::consts::FRAC_PI_2;
        } else {
            let meta = at(&self.result.metadata.bonds, id)?;
            reverse = match meta.stereo {
                2 | 4 => false,
                3 | 5 => true,
                _ => return Ok(Vec::new()),
            };
            if meta.stereo_atoms.len() != 2 {
                return Err("Missing double-bond stereo references".into());
            }
            for other in [self.other(first, a)?, self.other(second, b)?] {
                if !meta.stereo_atoms.contains(&other) {
                    reverse = !reverse;
                }
            }
        }
        let mut follow = Vec::new();
        for neighbor in [first, second] {
            if *at(&self.needs, neighbor)? {
                for &other in at(&self.follows, neighbor)? {
                    work.spend(1)?;
                    if *at(&self.needs, other)? {
                        work.store(1)?;
                        follow.push(other);
                    }
                }
            }
        }
        if !*at(&self.needs, first)? {
            if *at(&self.needs, second)? {
                reverse ^= at(&self.graph.bonds, first)?.a != a;
                self.set_direction(second, b, *at(&self.result.directions, first)?, reverse)?;
            }
        } else if !*at(&self.needs, second)? {
            reverse ^= at(&self.graph.bonds, second)?.a != b;
            self.set_direction(first, a, *at(&self.result.directions, second)?, reverse)?;
        } else {
            self.set_direction(first, a, Direction::Down, false)?;
            self.set_direction(second, b, Direction::Down, reverse)?;
        }
        *self.needs.get_mut(first).ok_or("Missing stereo flag")? = false;
        *self.needs.get_mut(second).ok_or("Missing stereo flag")? = false;
        for (other, selected, atom) in [(left.other, first, a), (right.other, second, b)] {
            if let Some(other) = other
                && *at(&self.needs, other)?
            {
                self.set_direction(
                    other,
                    atom,
                    *at(&self.result.directions, selected)?,
                    at(&self.graph.bonds, selected)?.a == atom,
                )?;
                *self.needs.get_mut(other).ok_or("Missing stereo flag")? = false;
            }
        }
        Ok(follow)
    }
}

fn linear(a: Point3, b: Point3) -> bool {
    let norm = a.squared() * b.squared();
    norm < 1e-6 || a.dot(b) < -0.999388 * norm.sqrt()
}
fn dihedral(a: Point3, b: Point3, c: Point3, d: Point3) -> f64 {
    let axis = c.sub(b);
    let first = a.sub(b).cross(axis);
    let second = d.sub(c).cross(axis);
    let cosine = first.dot(second) / (first.squared() * second.squared()).sqrt();
    if cosine <= -1.0 {
        std::f64::consts::PI
    } else if cosine >= 1.0 {
        0.0
    } else {
        cosine.acos()
    }
}
