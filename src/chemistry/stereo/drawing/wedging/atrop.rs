//! Atropisomer wedging from RDKit Atropisomers.cpp (2026.03.6).
//! Copyright (C) 2004-2021 Tad hurst/CDD and other RDKit contributors.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
use super::*;
const SMALL: f64 = 1e-7;

impl Context<'_> {
    fn end_vector(
        &self,
        atom: usize,
        bonds: &[usize],
        y: Point3,
        work: &mut Work,
    ) -> Result<Option<Point3>, String> {
        work.spend(bonds.len())?;
        if !(1..=2).contains(&bonds.len()) {
            return Err("Invalid atropisomer neighbors".into());
        }
        let project = |b| -> Result<Point3, String> {
            let v = self.point(self.other(b, atom)?)?.sub(self.point(atom)?);
            Ok(Point3 {
                x: 0.0,
                y: v.dot(y),
                z: v.z,
            })
        };
        let mut v = project(*at(bonds, 0)?)?;
        if bonds.len() == 2 {
            let other = project(*at(bonds, 1)?)?;
            if v.squared().sqrt() < SMALL {
                v = Point3 {
                    x: 0.0,
                    y: -other.y,
                    z: -other.z,
                };
            } else if v.dot(other) > SMALL {
                return Ok(None);
            }
        }
        if v.squared().sqrt() < SMALL {
            return Ok(None);
        }
        Ok(Some(v.unit()?))
    }
    fn atrop_direction(
        &self,
        bond: usize,
        stereo: u8,
        end: usize,
        number: usize,
        vectors: &[Point3; 2],
    ) -> Result<Direction, String> {
        if self.conf.is_3d {
            let b = at(&self.state.graph.bonds, bond)?;
            Ok(if self.point(b.b)?.z - self.point(b.a)?.z > SMALL {
                Direction::Wedge
            } else {
                Direction::Hash
            })
        } else {
            let odd = (stereo == 7) ^ (number == 1) ^ (end == 1) ^ (at(vectors, 1 - end)?.y < 0.0);
            Ok(if odd {
                Direction::Wedge
            } else {
                Direction::Hash
            })
        }
    }
    fn atrop(&mut self, index: usize, work: &mut Work) -> Result<(), String> {
        let main = at(&self.state.graph.bonds, index)?.clone();
        let atoms = [main.a, main.b];
        let stereo = at(&self.state.metadata.bonds, index)?.stereo;
        let mut ends = [Vec::new(), Vec::new()];
        for (end, &atom) in atoms.iter().enumerate() {
            for &b in at(&self.adjacent, atom)? {
                work.spend(1)?;
                if b != index {
                    ends.get_mut(end).ok_or("Missing atropisomer end")?.push(b);
                }
            }
            let bonds = ends.get_mut(end).ok_or("Missing atropisomer end")?;
            if bonds.is_empty() {
                return Ok(());
            }
            if bonds.len() > 2 {
                return Err("Excess atropisomer neighbors".into());
            }
            if bonds.len() == 2
                && self.other(*at(bonds, 1)?, atom)? < self.other(*at(bonds, 0)?, atom)?
            {
                bonds.swap(0, 1);
            }
            for &b in bonds.iter() {
                if self.direction(b)? == Direction::Unknown {
                    return Ok(());
                }
            }
        }
        let mut vectors = [Point3::default(); 2];
        if !self.conf.is_3d {
            let axis = self.point(main.b)?.sub(self.point(main.a)?);
            if axis.squared().sqrt() < SMALL {
                return Ok(());
            }
            let x = axis.unit()?;
            let y = Point3 {
                x: -x.y,
                y: x.x,
                z: 0.0,
            }
            .unit()?;
            for end in 0..2 {
                let Some(v) = self.end_vector(*at(&atoms, end)?, at(&ends, end)?, y, work)? else {
                    return Ok(());
                };
                put(&mut vectors, end, v)?;
            }
        }
        let mut existing = Vec::new();
        for (end, (&atom, bonds)) in atoms.iter().zip(&ends).enumerate() {
            for (number, &bond) in bonds.iter().enumerate() {
                work.spend(1)?;
                let b = at(&self.state.graph.bonds, bond)?;
                // The 2D reference checks the axis order here; 3D checks the
                // candidate order. Preserve that difference for imported data.
                if wedged(self.direction(bond)?)
                    && b.a == atom
                    && can_direct(if self.conf.is_3d { b.order } else { main.order })
                {
                    existing.push((end, number, bond));
                }
            }
        }
        if !existing.is_empty() {
            for (end, number, bond) in existing {
                let d = self.atrop_direction(bond, stereo, end, number, &vectors)?;
                put(&mut self.state.directions, bond, d)?;
            }
            return Ok(());
        }
        let (mut best_count, mut largest, mut best_single, mut best_dir) =
            (usize::MAX, 0usize, false, Direction::None);
        let mut best = None;
        for (end, (&atom, bonds)) in atoms.iter().zip(&ends).enumerate() {
            for (number, &bond) in bonds.iter().enumerate() {
                work.spend(1)?;
                let single = at(&self.state.graph.bonds, bond)?.order == 1;
                if !can_direct(at(&self.state.graph.bonds, bond)?.order)
                    || self
                        .choices
                        .contains_key(&if self.conf.is_3d { index } else { bond })
                {
                    continue;
                }
                // The 3D reference orients every considered candidate, even
                // if it later chooses another bond or abandons this axis.
                if self.conf.is_3d {
                    self.orient(bond, atom)?;
                }
                let direction = self.direction(bond)?;
                if direction != Direction::None {
                    if at(&self.state.graph.bonds, bond)?.a == atom
                        && (self.conf.is_3d || wedged(direction))
                    {
                        return Ok(());
                    }
                    continue;
                }
                let mut count = *at(&self.bond_rings, bond)?;
                let size = if count == 0 {
                    count = 10;
                    0
                } else {
                    let size = *at(&self.minimum, bond)?;
                    if size > 8 { 0 } else { size }
                };
                if count > best_count {
                    continue;
                }
                let better_ring = count < best_count || size > largest;
                if !better_ring && best_single && !single {
                    continue;
                }
                let dir = self.atrop_direction(bond, stereo, end, number, &vectors)?;
                if better_ring
                    || (!best_single && single)
                    || best_dir == Direction::None
                    || (best_dir == Direction::Hash && dir == Direction::Wedge)
                {
                    best = Some((bond, atom));
                    best_count = count;
                    best_single = single;
                    best_dir = dir;
                    if better_ring {
                        largest = size;
                    }
                }
            }
        }
        if let Some((bond, atom)) = best {
            self.orient(bond, atom)?;
            put(&mut self.state.directions, bond, best_dir)?;
            self.choices.insert(bond, Choice::Atrop(best_dir));
        }
        Ok(())
    }
    pub(super) fn choose_atrop(&mut self, work: &mut Work) -> Result<(), String> {
        for i in 0..self.state.graph.bonds.len() {
            work.spend(1)?;
            let b = at(&self.state.graph.bonds, i)?;
            if b.order == 1
                && matches!(at(&self.state.metadata.bonds, i)?.stereo, 6 | 7)
                && (2..=3).contains(&self.total_degree(b.a)?)
                && (2..=3).contains(&self.total_degree(b.b)?)
            {
                self.atrop(i, work)?;
            }
        }
        Ok(())
    }
}
