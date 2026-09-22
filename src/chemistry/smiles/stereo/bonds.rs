use super::*;
use std::collections::VecDeque;

#[derive(Clone, Copy)]
struct Neighbor {
    bond: usize,
    flipped: bool,
}
#[derive(Clone, Copy)]
struct Side {
    atom: usize,
    first: Neighbor,
    second: Option<Neighbor>,
    constrained: bool,
}

fn flip(dir: Direction) -> Direction {
    match dir {
        Direction::Up => Direction::Down,
        Direction::Down => Direction::Up,
        _ => dir,
    }
}
impl Context<'_> {
    fn canonical_bond(&self, b: usize) -> Result<bool, Error> {
        let meta = at(&self.output.metadata.bonds, b)?;
        Ok(at(&self.input.graph.bonds, b)?.order == 2
            && (2..=5).contains(&meta.stereo)
            && meta.stereo_atoms.len() == 2)
    }
    fn can_direct(&self, b: usize) -> Result<bool, Error> {
        Ok(matches!(at(&self.input.graph.bonds, b)?.order, 1 | 4))
    }
    pub(super) fn bonds(&mut self) -> Result<(), Error> {
        let walk = self.walk;
        let mut neighbors = vec![Vec::new(); self.input.graph.bonds.len()];
        let mut priority = Vec::new();
        for token in walk.tokens() {
            let Token::Bond { index: b, .. } = *token else {
                continue;
            };
            self.work.spend(1)?;
            if !self.canonical_bond(b)? {
                let meta = self.output.metadata.bonds.get_mut(b).ok_or(Error::Limit)?;
                meta.stereo = 0;
                meta.stereo_atoms.clear();
                continue;
            }
            let bond = at(&self.input.graph.bonds, b)?;
            let mut current = Vec::new();
            for atom in [bond.a, bond.b] {
                for &nbr in at(&self.adjacent, atom)? {
                    self.work.spend(1)?;
                    if !self.can_direct(nbr)? {
                        continue;
                    }
                    for &next in at(&self.adjacent, self.other(nbr, atom)?)? {
                        self.work.spend(1)?;
                        if next != nbr && self.canonical_bond(next)? {
                            current.push(next);
                            break;
                        }
                    }
                }
            }
            current.sort_by_key(|&b| self.bond_visit.get(b).copied());
            priority.push((
                std::cmp::Reverse(current.len()),
                *at(&self.bond_visit, b)?,
                b,
            ));
            put(&mut neighbors, b, current)?;
        }
        priority.sort_unstable();
        let mut seen = vec![false; self.input.graph.bonds.len()];
        for (_, _, root) in priority {
            let mut pending = VecDeque::from([root]);
            while let Some(bond) = pending.pop_front() {
                self.work.spend(1)?;
                if *at(&seen, bond)? {
                    continue;
                }
                self.double_bond(bond)?;
                put(&mut seen, bond, true)?;
                for &next in at(&neighbors, bond)? {
                    self.work.spend(1)?;
                    if !*at(&seen, next)? {
                        pending.push_back(next);
                    }
                }
            }
        }
        self.remove_unwanted()?;
        self.remove_redundant()
    }

    fn side(&self, atom: usize, bond: usize, left: bool) -> Result<Option<Side>, Error> {
        let mut eligible = Vec::new();
        let mut constrained = false;
        for &b in at(&self.adjacent, atom)? {
            if b == bond || !matches!(at(&self.input.graph.bonds, b)?.order, 1 | 4 | 5) {
                continue;
            }
            constrained |= *at(&self.bond_counts, b)? > 0;
            let other = self.other(b, atom)?;
            let forward = if left {
                at(&self.atom_visit, atom)? < at(&self.atom_visit, other)?
            } else {
                at(&self.atom_visit, other)? < at(&self.atom_visit, atom)?
            };
            eligible.push(Neighbor {
                bond: b,
                flipped: forward ^ *at(&self.closures, b)?,
            });
        }
        eligible.sort_by_key(|b| self.bond_visit.get(b.bond).copied());
        Ok(eligible.first().map(|&first| Side {
            atom,
            first,
            second: eligible.get(1).copied(),
            constrained,
        }))
    }
    fn direction(&self, n: Neighbor) -> Result<Direction, Error> {
        Ok(*at(&self.output.directions, n.bond)?)
    }
    fn same_side(&self, a: Neighbor, b: Neighbor) -> Result<bool, Error> {
        Ok((a.flipped != b.flipped) == (self.direction(a)? == self.direction(b)?))
    }
    fn copy_direction(&mut self, from: Neighbor, to: Neighbor) -> Result<(), Error> {
        let direction = self.direction(from)?;
        put(
            &mut self.output.directions,
            to.bond,
            if from.flipped == to.flipped {
                flip(direction)
            } else {
                direction
            },
        )
    }
    fn reference_direction(
        &self,
        bond: usize,
        source_atom: usize,
        target_atom: usize,
        from: Neighbor,
        to: Neighbor,
    ) -> Result<Direction, Error> {
        let meta = at(&self.output.metadata.bonds, bond)?;
        let mut dir = self.direction(from)?;
        if matches!(meta.stereo, 2 | 4) {
            dir = flip(dir);
        }
        if dir == Direction::None {
            return Err(invalid("Missing stereo reference direction"));
        }
        if at(&self.adjacent, source_atom)?.len() == 3
            && !meta
                .stereo_atoms
                .contains(&self.other(from.bond, source_atom)?)
        {
            dir = flip(dir);
        }
        if at(&self.adjacent, target_atom)?.len() == 3
            && !meta
                .stereo_atoms
                .contains(&self.other(to.bond, target_atom)?)
        {
            dir = flip(dir);
        }
        if from.flipped != to.flipped {
            dir = flip(dir);
        }
        Ok(dir)
    }
    fn complete_side(&mut self, side: Side) -> Result<bool, Error> {
        let mut consistent = true;
        if let Some(second) = side.second {
            if *at(&self.bond_counts, side.first.bond)? == 0 {
                self.copy_direction(second, side.first)?;
            } else if *at(&self.bond_counts, second.bond)? == 0 {
                self.copy_direction(side.first, second)?;
            } else {
                consistent = self.same_side(side.first, second)?;
            }
            self.increment(second.bond, side.atom)?;
        }
        self.increment(side.first.bond, side.atom)?;
        Ok(consistent)
    }
    fn controlling(&mut self, side: Side) -> Result<Neighbor, Error> {
        if *at(&self.bond_counts, side.first.bond)? > 0 {
            self.increment(side.first.bond, side.atom)?;
            if let Some(second) = side.second
                && *at(&self.bond_counts, second.bond)? != 0
            {
                self.increment(second.bond, side.atom)?;
            }
            Ok(side.first)
        } else {
            let second = side
                .second
                .ok_or_else(|| invalid("Missing controlling stereo bond"))?;
            self.copy_direction(second, side.first)?;
            self.increment(second.bond, side.atom)?;
            self.increment(side.first.bond, side.atom)?;
            Ok(second)
        }
    }
    fn double_bond(&mut self, b: usize) -> Result<(), Error> {
        let bond = at(&self.input.graph.bonds, b)?;
        let (mut a, mut z) = (bond.a, bond.b);
        if !(2..=3).contains(&at(&self.adjacent, a)?.len())
            || !(2..=3).contains(&at(&self.adjacent, z)?.len())
        {
            return Ok(());
        }
        if at(&self.atom_visit, a)? >= at(&self.atom_visit, z)? {
            std::mem::swap(&mut a, &mut z);
        }
        let (Some(left), Some(right)) = (self.side(a, b, true)?, self.side(z, b, false)?) else {
            return Ok(());
        };
        if left.constrained && right.constrained {
            let lc = self.complete_side(left)?;
            let rc = self.complete_side(right)?;
            self.conflicts(b, left, lc, right, rc)?;
            return Ok(());
        }
        if !right.constrained {
            let from = if left.constrained {
                self.controlling(left)?
            } else {
                put(&mut self.output.directions, left.first.bond, Direction::Up)?;
                self.increment(left.first.bond, left.atom)?;
                left.first
            };
            let dir = self.reference_direction(b, left.atom, right.atom, from, right.first)?;
            put(&mut self.output.directions, right.first.bond, dir)?;
            self.increment(right.first.bond, right.atom)?;
        } else {
            let from = self.controlling(right)?;
            let dir = self.reference_direction(b, right.atom, left.atom, from, left.first)?;
            put(&mut self.output.directions, left.first.bond, dir)?;
            self.increment(left.first.bond, left.atom)?;
        }
        for side in [left, right] {
            if let Some(second) = side.second
                && *at(&self.bond_counts, second.bond)? == 0
            {
                self.copy_direction(side.first, second)?;
                self.increment(second.bond, side.atom)?;
            }
        }
        Ok(())
    }
    fn remove_counted(&mut self, bond: usize, atom: usize, other: usize) -> Result<(), Error> {
        put(&mut self.bond_counts, bond, 0)?;
        count(&mut self.atom_counts, atom, -1)?;
        count(&mut self.atom_counts, other, -1)
    }
    fn fix_side(&mut self, b: usize, side: Side, reference: Side) -> Result<(), Error> {
        let second = side.second.ok_or(Error::Limit)?;
        for candidate in [side.first, second] {
            // The pinned implementation iterates copied Bond values: its
            // address comparison always selects first as the other bond.
            let other = self.other(side.first.bond, side.atom)?;
            if *at(&self.atom_counts, other)? == 2
                && self.reference_direction(
                    b,
                    reference.atom,
                    side.atom,
                    reference.first,
                    candidate,
                )? == self.direction(candidate)?
            {
                self.remove_counted(side.first.bond, side.atom, other)?;
                break;
            }
        }
        Ok(())
    }
    fn conflicts(
        &mut self,
        b: usize,
        left: Side,
        lc: bool,
        right: Side,
        rc: bool,
    ) -> Result<(), Error> {
        match (lc, rc) {
            (true, true) => (), // Native output retains an irreconcilable pair.
            (true, false) => self.fix_side(b, right, left)?,
            (false, true) => self.fix_side(b, left, right)?,
            (false, false) => {
                let ls = left.second.ok_or(Error::Limit)?;
                let rs = right.second.ok_or(Error::Limit)?;
                for l in [left.first, ls] {
                    for r in [right.first, rs] {
                        if self.reference_direction(b, left.atom, right.atom, l, r)?
                            != self.direction(r)?
                        {
                            continue;
                        }
                        let lb = if l.bond == left.first.bond {
                            ls.bond
                        } else {
                            left.first.bond
                        };
                        let rb = if r.bond == right.first.bond {
                            rs.bond
                        } else {
                            right.first.bond
                        };
                        let lo = self.other(lb, left.atom)?;
                        let ro = self.other(rb, right.atom)?;
                        if lo != ro
                            && *at(&self.atom_counts, lo)? == 2
                            && *at(&self.atom_counts, ro)? == 2
                        {
                            self.remove_counted(lb, left.atom, lo)?;
                            self.remove_counted(rb, right.atom, ro)?;
                            return Ok(());
                        }
                    }
                }
            }
        }
        Ok(())
    }
    fn remove_unwanted(&mut self) -> Result<(), Error> {
        let walk = self.walk;
        for token in walk.tokens() {
            let Token::Bond { index, .. } = *token else {
                continue;
            };
            self.work.spend(1)?;
            let b = at(&self.input.graph.bonds, index)?;
            if b.order != 2
                || at(&self.output.metadata.bonds, index)?.stereo > 1
                || at(&self.adjacent, b.a)?.len() == 1
                || at(&self.adjacent, b.b)?.len() == 1
            {
                continue;
            }
            let mut candidates = Vec::new();
            for &id in at(&self.adjacent, b.a)? {
                self.work.spend(1)?;
                if *at(&self.bond_counts, id)? != 0 {
                    candidates.push(id);
                }
            }
            if candidates.is_empty() {
                continue;
            }
            if *at(&self.atom_counts, b.a)? != 0 {
                candidates.clear();
            }
            let mut on_second = 0u8;
            for &id in at(&self.adjacent, b.b)? {
                self.work.spend(1)?;
                if *at(&self.bond_counts, id)? != 0 {
                    candidates.push(id);
                    on_second = on_second.wrapping_add(1);
                }
            }
            if on_second == 0 || *at(&self.atom_counts, b.b)? != 0 {
                continue;
            }
            candidates.sort_by_key(|&id| self.bond_visit.get(id).copied());
            for id in candidates {
                let c = at(&self.input.graph.bonds, id)?;
                let from = if [c.a, c.b].contains(&b.a) { b.a } else { b.b };
                let other = self.other(id, from)?;
                if *at(&self.atom_counts, other)? == 2 {
                    put(&mut self.bond_counts, id, 0)?;
                    put(&mut self.output.directions, id, Direction::None)?;
                    count(&mut self.atom_counts, other, -1)?;
                    break;
                }
            }
        }
        Ok(())
    }
    fn clear_direction(&mut self, bond: usize, atom: usize) -> Result<(), Error> {
        count(&mut self.bond_counts, bond, -1)?;
        if *at(&self.bond_counts, bond)? == 0 {
            put(&mut self.output.directions, bond, Direction::None)?;
            count(&mut self.atom_counts, atom, -1)?;
            let other = self.other(bond, atom)?;
            if *at(&self.atom_counts, other)? != 0 {
                count(&mut self.atom_counts, other, -1)?;
            }
        }
        Ok(())
    }
    fn clear_redundant(&mut self, bond: usize, atom: usize) -> Result<(), Error> {
        if *at(&self.atom_counts, atom)? < 2 {
            return Ok(());
        }
        let mut double = false;
        for &id in at(&self.adjacent, atom)? {
            self.work.spend(1)?;
            if id != bond
                && at(&self.input.graph.bonds, id)?.order == 2
                && at(&self.output.metadata.bonds, id)?.stereo > 1
            {
                double = true;
                break;
            }
        }
        if !double {
            return Ok(());
        }
        let target = at(&self.input.graph.bonds, bond)?;
        for &other in at(&self.adjacent, atom)? {
            self.work.spend(1)?;
            if other == bond || !self.can_direct(other)? {
                continue;
            }
            let b = at(&self.input.graph.bonds, other)?;
            if at(&self.bond_counts, other)? >= at(&self.bond_counts, bond)?
                && *at(&self.atom_counts, b.a)? != 1
                && *at(&self.atom_counts, b.b)? != 1
            {
                self.clear_direction(other, atom)?;
            } else if *at(&self.atom_counts, target.a)? != 1
                && *at(&self.atom_counts, target.b)? != 1
            {
                self.clear_direction(bond, atom)?;
            }
            break;
        }
        Ok(())
    }
    fn remove_redundant(&mut self) -> Result<(), Error> {
        let walk = self.walk;
        for token in walk.tokens() {
            let Token::Bond { index, left } = *token else {
                continue;
            };
            self.work.spend(1)?;
            if self.can_direct(index)? && *at(&self.bond_counts, index)? != 0 {
                self.clear_redundant(index, left)?;
                self.clear_redundant(index, self.other(index, left)?)?;
            } else {
                put(&mut self.output.directions, index, Direction::None)?;
            }
        }
        Ok(())
    }
}
