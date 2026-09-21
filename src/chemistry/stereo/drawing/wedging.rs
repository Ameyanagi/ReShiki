//! Bond wedging adapted from RDKit WedgeBonds.cpp (2026.03.6).
//! Copyright (C) 2023 Greg Landrum and other RDKit contributors.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
mod atrop;
#[cfg(test)]
mod tests;
use super::{Point3, at};
use crate::chemistry::{
    graph::{Graph, Valence},
    kekulize::Direction,
    ranking::Metadata,
    rings,
    stereo::perception::{RingCache, RingKind},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    f64::consts::PI,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WedgeState {
    pub graph: Graph,
    pub metadata: Metadata,
    pub directions: Vec<Direction>,
    pub rings: RingCache,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Conformer {
    pub positions: Vec<Point3>,
    pub is_3d: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WedgeProperties {
    pub valences: Vec<Valence>,
    /// Presence of an attachment-point marker, regardless of its stored value.
    pub attachment_points: Vec<bool>,
}
struct Work(usize);
impl Work {
    fn spend(&mut self, n: usize) -> Result<(), String> {
        self.0 = self
            .0
            .checked_sub(n)
            .ok_or("Bond wedging work limit exceeded")?;
        Ok(())
    }
}
#[derive(Clone, Copy)]
enum Choice {
    Atom(usize),
    Atrop(Direction),
}
struct Context<'a> {
    state: WedgeState,
    properties: &'a WedgeProperties,
    conf: &'a Conformer,
    adjacent: Vec<Vec<usize>>,
    atom_rings: Vec<usize>,
    bond_rings: Vec<usize>,
    minimum: Vec<usize>,
    choices: BTreeMap<usize, Choice>,
}
fn tetrahedral(tag: u8) -> bool {
    matches!(tag, 1 | 2)
}
fn wedged(direction: Direction) -> bool {
    matches!(direction, Direction::Wedge | Direction::Hash)
}
fn can_direct(order: u8) -> bool {
    matches!(order, 1 | 4)
}
fn put<T>(items: &mut [T], index: usize, value: T) -> Result<(), String> {
    *items.get_mut(index).ok_or("Invalid wedging index")? = value;
    Ok(())
}
fn planar(mut point: Point3) -> Point3 {
    point.z = 0.0;
    point
}
fn angle(a: Point3, b: Point3) -> Result<f64, String> {
    let value = (a.dot(b) / (a.squared() * b.squared()).sqrt())
        .clamp(-1.0, 1.0)
        .acos();
    if !value.is_finite() {
        return Err("Invalid wedging angle".into());
    }
    Ok(value)
}
impl<'a> Context<'a> {
    fn new(
        input: &WedgeState,
        properties: &'a WedgeProperties,
        conf: &'a Conformer,
        initialize_rings: bool,
        work: &mut Work,
    ) -> Result<Self, String> {
        input.graph.cached_valences(Some(&properties.valences))?;
        input.metadata.validate(&input.graph)?;
        let (n, e) = (input.graph.atoms.len(), input.graph.bonds.len());
        if input.directions.len() != e
            || properties.attachment_points.len() != n
            || conf.positions.len() != n
            || conf.positions.iter().any(|p| !p.valid())
        {
            return Err("Invalid bond wedging annotations or coordinates".into());
        }
        let mut state = input.clone();
        if initialize_rings && !matches!(state.rings.kind, RingKind::Basis | RingKind::Symmetric) {
            let mut found = rings::perceive(&state.graph, rings::Options::default())
                .map_err(|e| e.to_string())?;
            found.atoms.truncate(found.basis_count);
            state.rings = RingCache {
                kind: RingKind::Basis,
                atoms: found.atoms,
            };
        }
        let mut adjacent = vec![Vec::new(); n];
        let mut by_pair = HashMap::new();
        for (i, b) in state.graph.bonds.iter().enumerate() {
            work.spend(1)?;
            for a in [b.a, b.b] {
                adjacent.get_mut(a).ok_or("Missing wedging atom")?.push(i);
            }
            by_pair.insert((b.a.min(b.b), b.a.max(b.b)), i);
        }
        let mut result = Self {
            state,
            properties,
            conf,
            adjacent,
            atom_rings: vec![0; n],
            bond_rings: vec![0; e],
            minimum: vec![0; e],
            choices: BTreeMap::new(),
        };
        if result.state.rings.kind == RingKind::None && !result.state.rings.atoms.is_empty() {
            return Err("Uninitialized wedging ring cache contains cycles".into());
        }
        let mut storage = 0usize;
        for ring in &result.state.rings.atoms {
            storage = storage
                .checked_add(ring.len())
                .ok_or("Wedging ring storage overflow")?;
            if storage > 2_000_000
                || ring.len() < 3
                || ring.iter().collect::<HashSet<_>>().len() != ring.len()
            {
                return Err("Invalid wedging ring cache".into());
            }
            work.spend(ring.len())?;
            for (&a, &b) in ring.iter().zip(ring.iter().cycle().skip(1)) {
                let edge = *by_pair
                    .get(&(a.min(b), a.max(b)))
                    .ok_or("Missing wedging ring edge")?;
                *result.atom_rings.get_mut(a).ok_or("Missing ring atom")? += 1;
                *result.bond_rings.get_mut(edge).ok_or("Missing ring bond")? += 1;
                let min = result.minimum.get_mut(edge).ok_or("Missing minimum ring")?;
                if *min == 0 || ring.len() < *min {
                    *min = ring.len();
                }
            }
        }
        Ok(result)
    }
    fn other(&self, bond: usize, atom: usize) -> Result<usize, String> {
        let b = at(&self.state.graph.bonds, bond)?;
        if b.a == atom {
            Ok(b.b)
        } else if b.b == atom {
            Ok(b.a)
        } else {
            Err("Unconnected wedging atom".into())
        }
    }
    fn orient(&mut self, bond: usize, atom: usize) -> Result<(), String> {
        let other = self.other(bond, atom)?;
        let b = self
            .state
            .graph
            .bonds
            .get_mut(bond)
            .ok_or("Missing wedging bond")?;
        b.a = atom;
        b.b = other;
        Ok(())
    }
    fn tag(&self, atom: usize) -> Result<u8, String> {
        Ok(at(&self.state.metadata.atoms, atom)?.chiral_tag)
    }
    fn point(&self, atom: usize) -> Result<Point3, String> {
        Ok(*at(&self.conf.positions, atom)?)
    }
    fn direction(&self, bond: usize) -> Result<Direction, String> {
        Ok(*at(&self.state.directions, bond)?)
    }
    fn total_degree(&self, atom: usize) -> Result<usize, String> {
        Ok(at(&self.adjacent, atom)?.len()
            + usize::from(at(&self.state.graph.atoms, atom)?.explicit_hydrogens)
            + at(&self.properties.valences, atom)?.implicit_hydrogens as usize)
    }
    fn choose_tetrahedral(&mut self, work: &mut Work) -> Result<(), String> {
        let n = self.state.graph.atoms.len();
        let mut counts = vec![100i64; n];
        for (i, b) in self.state.graph.bonds.iter().enumerate() {
            work.spend(1)?;
            if wedged(self.direction(i)?) || self.direction(i)? == Direction::Unknown {
                if tetrahedral(self.tag(b.a)?) {
                    put(&mut counts, b.a, 101)?;
                } else if tetrahedral(self.tag(b.b)?) {
                    put(&mut counts, b.b, 101)?;
                }
            }
        }
        let mut any = false;
        for (i, count) in counts.iter_mut().enumerate() {
            work.spend(1)?;
            if *count > 100 || !tetrahedral(self.tag(i)?) {
                continue;
            }
            *count = 0;
            any = true;
            for &bond in at(&self.adjacent, i)? {
                work.spend(1)?;
                let other = self.other(bond, i)?;
                if at(&self.state.graph.atoms, other)?.atomic_number == 1 {
                    *count -= 10;
                } else if tetrahedral(self.tag(other)?) {
                    *count -= 1;
                }
            }
        }
        let mut order: Vec<_> = (0..n).collect();
        if any {
            work.spend(n.saturating_mul(n.max(1).ilog2() as usize + 1))?;
            // Equal scores have no chemical preference; retain atom order.
            order.sort_by_key(|&i| counts.get(i).copied());
        }
        let mut double_scores = vec![0i64; n];
        for (i, bond) in self.state.graph.bonds.iter().enumerate() {
            work.spend(1)?;
            if bond.order == 2 {
                let stereo = at(&self.state.metadata.bonds, i)?.stereo;
                let score = 11000
                    + if stereo == 1 {
                        23000
                    } else if stereo > 1 {
                        12000
                    } else {
                        0
                    };
                for atom in [bond.a, bond.b] {
                    *double_scores.get_mut(atom).ok_or("Missing double score")? += score;
                }
            }
        }
        for i in order {
            work.spend(1)?;
            if *at(&counts, i)? > 100 {
                continue;
            }
            if !tetrahedral(self.tag(i)?) {
                break;
            }
            let mut best: Option<(i64, usize)> = None;
            for &b in at(&self.adjacent, i)? {
                work.spend(1)?;
                if at(&self.state.graph.bonds, b)?.order != 1 || self.choices.contains_key(&b) {
                    continue;
                }
                let other = self.other(b, i)?;
                let number = at(&self.state.graph.atoms, other)?.atomic_number;
                let score = if number == 1 {
                    -1_000_000
                } else {
                    let count = *at(&counts, other)?;
                    i64::from(number)
                        + 100 * at(&self.adjacent, other)?.len() as i64
                        + 1000 * i64::from(self.tag(other)? != 0)
                        - if count < 100 { 100000 * count } else { 0 }
                        + 10000 * (*at(&self.atom_rings, other)? as i64)
                        + 20000 * (*at(&self.bond_rings, b)? as i64)
                        + *at(&double_scores, other)?
                        + 500000 * i64::from(*at(&self.properties.attachment_points, other)?)
                };
                if i32::try_from(score).is_err() {
                    return Err("Wedging score exceeds reference integer range".into());
                }
                if best.is_none_or(|v| (score, b) < v) {
                    best = Some((score, b));
                }
            }
            if let Some((_, b)) = best {
                self.choices.insert(b, Choice::Atom(i));
            }
        }
        Ok(())
    }
    fn wedge_direction(
        &self,
        bond: usize,
        atom: usize,
        work: &mut Work,
    ) -> Result<Direction, String> {
        if at(&self.state.graph.bonds, bond)?.order != 1 {
            return Err("Only single bonds support tetrahedral wedging".into());
        }
        let tag = self.tag(atom)?;
        if !tetrahedral(tag) {
            return Err("Wedging requires a tetrahedral center".into());
        }
        let original = self.direction(bond)?;
        let center = planar(self.point(atom)?);
        let Ok(reference) = planar(self.point(self.other(bond, atom)?)?)
            .sub(center)
            .unit()
        else {
            return Ok(original);
        };
        let neighbors = at(&self.adjacent, atom)?;
        let reference_index = neighbors
            .iter()
            .position(|&b| b == bond)
            .ok_or("Missing wedge reference")?;
        let mut ordered = vec![(0.0, reference_index, 0usize)];
        for (position, &b) in neighbors.iter().enumerate() {
            work.spend(1)?;
            if b == bond {
                continue;
            }
            let Ok(vector) = planar(self.point(self.other(b, atom)?)?).sub(center).unit() else {
                return Ok(original);
            };
            let mut a = angle(reference, vector)?;
            if reference.x * vector.y - reference.y * vector.x < -1e-16 {
                a = 2.0 * PI - a;
            }
            ordered.push((a, position, ordered.len()));
        }
        work.spend(
            ordered
                .len()
                .saturating_mul(ordered.len().max(1).ilog2() as usize + 1),
        )?;
        // Native inserts a later equal-angle neighbor before earlier ones.
        ordered.sort_by(|a, b| a.0.total_cmp(&b.0).then(b.2.cmp(&a.2)));
        let mut tree = vec![0usize; ordered.len() + 1];
        let mut odd = false;
        for &(_, p, _) in ordered.iter().rev() {
            let mut i = p;
            while i > 0 {
                work.spend(1)?;
                odd ^= (*at(&tree, i)? & 1) != 0;
                i -= i & i.wrapping_neg();
            }
            i = p + 1;
            while i < tree.len() {
                work.spend(1)?;
                *tree.get_mut(i).ok_or("Missing wedge parity slot")? += 1;
                i += i & i.wrapping_neg();
            }
        }
        if ordered.len() == 3 && at(&ordered, 2)?.0 - at(&ordered, 1)?.0 >= PI - PI * 1.9 / 180.0 {
            odd = !odd;
        }
        Ok(if (tag == 2) ^ odd {
            Direction::Wedge
        } else {
            Direction::Hash
        })
    }
    fn second_wedge(&mut self, bond: usize, work: &mut Work) -> Result<(), String> {
        let b = at(&self.state.graph.bonds, bond)?.clone();
        let atom = b.a;
        if at(&self.adjacent, atom)?.len() < 4 {
            return Ok(());
        }
        let center = planar(self.point(atom)?);
        let reference = planar(self.point(b.b)?).sub(center).unit()?;
        let (mut min_angle, mut best_degree, mut best) = (10000.0, 100usize, None);
        for &candidate in at(&self.adjacent, atom)? {
            work.spend(1)?;
            let other = self.other(candidate, atom)?;
            if candidate == bond
                || at(&self.state.graph.bonds, candidate)?.order != 1
                || self.direction(candidate)? != Direction::None
                || self.tag(other)? != 0
                || *at(&self.bond_rings, candidate)? != 0
            {
                continue;
            }
            let vector = planar(self.point(other)?).sub(center).unit()?;
            let a = angle(reference, vector)?;
            let degree = at(&self.adjacent, other)?.len();
            if a - min_angle < 5.0 * PI / 180.0 && degree <= best_degree {
                best = Some(candidate);
                min_angle = a;
                best_degree = degree;
            }
        }
        if let Some(candidate) = best
            && min_angle < 2.0 * PI / 3.0
        {
            let direction = if self.direction(bond)? == Direction::Hash {
                Direction::Wedge
            } else {
                Direction::Hash
            };
            put(&mut self.state.directions, candidate, direction)?;
            self.orient(candidate, atom)?;
        }
        Ok(())
    }
}

/// Choose and draw wedge/hash bonds without moving atoms. Coordinates use Y up.
/// A conformer is required. All changes, including endpoint reversals, are atomic.
pub fn wedge_molecule(
    input: &WedgeState,
    properties: &WedgeProperties,
    conformer: Option<&Conformer>,
    two_bonds: bool,
) -> Result<WedgeState, String> {
    with_work(
        input,
        properties,
        conformer,
        two_bonds,
        &mut Work(50_000_000),
    )
}
fn with_work(
    input: &WedgeState,
    properties: &WedgeProperties,
    conformer: Option<&Conformer>,
    two_bonds: bool,
    work: &mut Work,
) -> Result<WedgeState, String> {
    let conf = conformer.ok_or("No conformer available for bond wedging")?;
    let mut ctx = Context::new(input, properties, conf, true, work)?;
    ctx.choose_tetrahedral(work)?;
    ctx.choose_atrop(work)?;
    for (bond, choice) in ctx.choices.clone() {
        work.spend(1)?;
        match choice {
            Choice::Atrop(direction) => put(&mut ctx.state.directions, bond, direction)?,
            Choice::Atom(atom) => {
                let direction = ctx.wedge_direction(bond, atom, work)?;
                if wedged(direction) {
                    put(&mut ctx.state.directions, bond, direction)?;
                    ctx.orient(bond, atom)?;
                }
            }
        }
    }
    if two_bonds {
        for atom in 0..ctx.state.graph.atoms.len() {
            work.spend(1)?;
            if !tetrahedral(ctx.tag(atom)?) {
                continue;
            }
            let mut bonds = Vec::new();
            for &b in at(&ctx.adjacent, atom)? {
                work.spend(1)?;
                let bond = at(&ctx.state.graph.bonds, b)?;
                if bond.a == atom && bond.order == 1 && wedged(ctx.direction(b)?) {
                    bonds.push(b);
                }
            }
            if bonds.len() == 1 {
                ctx.second_wedge(*at(&bonds, 0)?, work)?;
            }
        }
    }
    Ok(ctx.state)
}

/// Set one bond's wedge direction relative to the requested tetrahedral atom.
/// Like the reference's single-bond API, this does not reverse its endpoints.
pub fn wedge_bond(
    input: &WedgeState,
    properties: &WedgeProperties,
    conformer: &Conformer,
    bond: usize,
    atom: usize,
) -> Result<WedgeState, String> {
    let mut work = Work(50_000_000);
    let mut ctx = Context::new(input, properties, conformer, false, &mut work)?;
    ctx.other(bond, atom)?;
    if at(&ctx.state.graph.bonds, bond)?.order == 1 {
        let direction = ctx.wedge_direction(bond, atom, &mut work)?;
        if wedged(direction) {
            put(&mut ctx.state.directions, bond, direction)?;
        }
    }
    Ok(ctx.state)
}
