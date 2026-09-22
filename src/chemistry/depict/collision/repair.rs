use super::super::geometry::Transform;
use super::*;

impl Input<'_> {
    fn one_side(
        &self,
        start: usize,
        blocked: usize,
        session: &mut Session,
    ) -> Result<Vec<usize>, Error> {
        let mut result = Vec::new();
        let mut seen = BTreeSet::new();
        let mut stack = vec![start];
        while let Some(id) = stack.pop() {
            session.spend(1)?;
            if !seen.insert(id) {
                continue;
            }
            result.push(id);
            let neighbors = at(&self.neighbors, id)?;
            session.spend(neighbors.len())?;
            for &(neighbor, _) in neighbors.iter().rev() {
                if neighbor != blocked && !seen.contains(&neighbor) {
                    stack.push(neighbor);
                }
            }
        }
        Ok(result)
    }
    fn reflect(value: &mut EmbeddedAtom, first: Point, second: Point) -> Result<(), Error> {
        let mut temp = add(value.location, value.normal)?;
        value.location = geometry::reflect_point(value.location, first, second)?;
        temp = geometry::reflect_point(temp, first, second)?;
        value.normal = sub(temp, value.location)?;
        value.counter_clockwise = !value.counter_clockwise;
        Ok(())
    }
    fn flip_bond(
        &self,
        value: &mut Fragment,
        id: usize,
        flip_end: bool,
        session: &mut Session,
    ) -> Result<(), Error> {
        let bond = at(&self.graph.bonds, id)?;
        if *at(&self.ring_bonds, id)? {
            return Err(Error::Invalid("flip of ring bond"));
        }
        let (begin, end) = if flip_end {
            (bond.a, bond.b)
        } else {
            (bond.b, bond.a)
        };
        atom(value, begin)?;
        atom(value, end)?;
        let end_side = self.one_side(end, begin, session)?;
        session.spend(value.atoms.len() + end_side.len())?;
        let fixed = value.atoms.values().any(|a| a.fixed);
        if fixed {
            for &id in &end_side {
                if atom(value, id)?.fixed {
                    return Ok(());
                }
            }
        }
        let other_count = value
            .atoms
            .len()
            .checked_sub(end_side.len())
            .ok_or(Error::Invalid("fragment omits graph component"))?;
        let end_flip = other_count >= end_side.len();
        let selected = end_side.into_iter().collect::<BTreeSet<_>>();
        let ids = value.atoms.keys().copied().collect::<Vec<_>>();
        for id in ids {
            session.spend(1)?;
            if end_flip ^ !selected.contains(&id) {
                // Native axis endpoints are references into the same map.
                // Reflecting an endpoint can change the axis by roundoff,
                // including before the second reflection of loc + normal.
                let mut temp = add(atom(value, id)?.location, atom(value, id)?.normal)?;
                let location = geometry::reflect_point(
                    atom(value, id)?.location,
                    atom(value, begin)?.location,
                    atom(value, end)?.location,
                )?;
                atom_mut(value, id)?.location = location;
                temp = geometry::reflect_point(
                    temp,
                    atom(value, begin)?.location,
                    atom(value, end)?.location,
                )?;
                let a = atom_mut(value, id)?;
                a.normal = sub(temp, a.location)?;
                a.counter_clockwise = !a.counter_clockwise;
            }
        }
        Ok(())
    }
    fn spiro(&self, id: usize, session: &mut Session) -> Result<bool, Error> {
        let neighbors = at(&self.neighbors, id)?;
        let rings = at(&self.atom_rings, id)?;
        if neighbors.len() != 4 || rings.len() != 2 {
            return Ok(false);
        }
        let first = at(&self.rings.atoms, *at(rings, 0)?)?;
        let second = at(&self.rings.atoms, *at(rings, 1)?)?;
        session.spend(first.len() + second.len())?;
        let a = first.iter().copied().collect::<BTreeSet<_>>();
        let b = second.iter().copied().collect::<BTreeSet<_>>();
        if a.intersection(&b).count() != 1 {
            return Ok(false);
        }
        let (mut na, mut nb) = (0, 0);
        for &(neighbor, _) in neighbors {
            match (a.contains(&neighbor), b.contains(&neighbor)) {
                (true, false) => na += 1,
                (false, true) => nb += 1,
                _ => return Ok(false),
            }
        }
        Ok(na == 2 && nb == 2)
    }
    fn flip_spiro(
        &self,
        value: &mut Fragment,
        id: usize,
        session: &mut Session,
    ) -> Result<(), Error> {
        let ring = at(&self.rings.atoms, *at(at(&self.atom_rings, id)?, 0)?)?;
        session.spend(ring.len())?;
        let selected = ring.iter().copied().collect::<BTreeSet<_>>();
        let neighbors = at(&self.neighbors, id)?
            .iter()
            .filter_map(|&(n, _)| selected.contains(&n).then_some(n))
            .collect::<Vec<_>>();
        if neighbors.len() != 2 {
            return Err(Error::Invalid("spiro ring neighbors"));
        }
        let n1 = *at(&neighbors, 0)?;
        let n2 = *at(&neighbors, 1)?;
        let side = self.one_side(n1, id, session)?;
        let first = atom(value, id)?.location;
        let midpoint = scale(
            add(atom(value, n1)?.location, atom(value, n2)?.location)?,
            0.5,
        )?;
        session.spend(side.len() * 2)?;
        for &id in &side {
            if atom(value, id)?.fixed {
                return Ok(());
            }
        }
        for id in side {
            Self::reflect(atom_mut(value, id)?, first, midpoint)?;
        }
        Ok(())
    }
    fn rotatable(&self, a: usize, b: usize, session: &mut Session) -> Result<Vec<usize>, Error> {
        let path = self.shortest_path(a, b, session)?;
        if path.len() < 4 {
            return Ok(Vec::new());
        }
        let mut result = Vec::new();
        for pair in path
            .iter()
            .skip(1)
            .take(path.len() - 2)
            .copied()
            .collect::<Vec<_>>()
            .windows(2)
        {
            session.spend(1)?;
            let id = self.bond(*at(pair, 0)?, *at(pair, 1)?)?;
            if at(&self.metadata.bonds, id)?.stereo <= 1 && !*at(&self.ring_bonds, id)? {
                result.push(id);
            }
        }
        Ok(result)
    }
    fn try_bonds(
        &self,
        value: &mut Fragment,
        pair: (usize, usize),
        count: usize,
        density: f64,
        done: &mut BTreeMap<usize, usize>,
        session: &mut Session,
    ) -> Result<bool, Error> {
        for id in self.rotatable(pair.0, pair.1, session)? {
            let tries = done.entry(id).or_default();
            if *tries >= MAX_FLIPS {
                continue;
            }
            *tries += 1;
            for end in [true, false] {
                self.flip_bond(value, id, end, session)?;
                let pairs = self.find_in_place(value, true, session)?;
                let new_density = Self::density(value)?;
                if pairs.len() < count {
                    done.insert(id, MAX_FLIPS);
                    return Ok(true);
                }
                if pairs.len() == count && new_density < density {
                    return Ok(true);
                }
                // Native rejection reflects again; restoring a snapshot would
                // discard observable rounding in both coordinates and normals.
                self.flip_bond(value, id, end, session)?;
            }
        }
        Ok(false)
    }
    pub(super) fn bond_and_spiro(
        &self,
        value: &mut Fragment,
        session: &mut Session,
    ) -> Result<(), Error> {
        let mut spiros = BTreeSet::new();
        for id in 0..self.graph.atoms.len() {
            session.spend(1)?;
            if self.spiro(id, session)? {
                spiros.insert(id);
            }
        }
        let mut pairs = self.find_in_place(value, true, session)?;
        let mut bonds = BTreeMap::new();
        let mut done_spiros = BTreeMap::new();
        for _ in 0..MAX_ITERATIONS {
            let Some(&pair) = pairs.first() else {
                break;
            };
            let count = pairs.len();
            let density = Self::density(value)?;
            if !self.try_bonds(value, pair, count, density, &mut bonds, session)? {
                for id in self.shortest_path(pair.0, pair.1, session)? {
                    if !spiros.contains(&id)
                        || done_spiros.get(&id).copied().unwrap_or(0) >= MAX_FLIPS
                    {
                        continue;
                    }
                    self.flip_spiro(value, id, session)?;
                    let trial = self.find_in_place(value, true, session)?;
                    let new_density = Self::density(value)?;
                    if trial.len() < count {
                        done_spiros.insert(id, MAX_FLIPS);
                        break;
                    }
                    if trial.len() == count && new_density < density {
                        *done_spiros.entry(id).or_insert(0) += 1;
                        break;
                    }
                    self.flip_spiro(value, id, session)?;
                }
            }
            pairs = self.find_in_place(value, true, session)?;
        }
        Ok(())
    }
    fn degree(&self, id: usize) -> Result<usize, Error> {
        Ok(at(&self.neighbors, id)?.len())
    }
    fn only_neighbor(&self, id: usize) -> Result<usize, Error> {
        let neighbors = at(&self.neighbors, id)?;
        if neighbors.len() != 1 {
            return Err(Error::Invalid("terminal atom needs one neighbor"));
        }
        Ok(at(neighbors, 0)?.0)
    }
    fn closest_neighbor(
        &self,
        source: usize,
        other: usize,
        session: &mut Session,
    ) -> Result<usize, Error> {
        let mut result = 0;
        let mut minimum = 100_000_000;
        for &(neighbor, _) in at(&self.neighbors, other)? {
            session.spend(1)?;
            let distance = self.distance(source, neighbor, session)?;
            if distance < minimum {
                minimum = distance;
                result = neighbor;
            }
        }
        Ok(result)
    }
    pub(super) fn open_angles(
        &self,
        value: &mut Fragment,
        session: &mut Session,
    ) -> Result<(), Error> {
        for (a, b) in self.find_in_place(value, false, session)? {
            let (degree_a, degree_b) = (self.degree(a)?, self.degree(b)?);
            let (fixed_a, fixed_b) = (atom(value, a)?.fixed, atom(value, b)?.fixed);
            if (degree_a > 1 || fixed_a) && (degree_b > 1 || fixed_b) {
                continue;
            }
            let (first, second, kind) = if degree_a == 1 && !fixed_a && degree_b == 1 && !fixed_b {
                (self.only_neighbor(a)?, self.only_neighbor(b)?, 1)
            } else if degree_a == 1 && !fixed_a && (degree_b > 1 || fixed_b) {
                let first = self.only_neighbor(a)?;
                (first, self.closest_neighbor(first, b, session)?, 2)
            } else {
                let second = self.only_neighbor(b)?;
                (self.closest_neighbor(second, a, session)?, second, 3)
            };
            let v2 = sub(atom(value, a)?.location, atom(value, first)?.location)?;
            let v1 = sub(atom(value, second)?.location, atom(value, first)?.location)?;
            let mut angle = match kind {
                1 => 0.1222,
                2 => 2.0 * 0.1222,
                _ => -2.0 * 0.1222,
            };
            if cross(v1, v2)? < 0.0 {
                angle *= -1.0;
            }
            // Native constructs both transforms before either atom moves.
            // A terminal pair can use each other's position as the center.
            let first_transform = if kind != 3 {
                Some(Transform::rotate(atom(value, first)?.location, angle)?)
            } else {
                None
            };
            let second_transform = if kind != 2 {
                Some(Transform::rotate(
                    atom(value, second)?.location,
                    if kind == 1 { -angle } else { angle },
                )?)
            } else {
                None
            };
            if let Some(transform) = first_transform {
                let a = atom_mut(value, a)?;
                a.location = transform.apply(a.location)?;
            }
            if let Some(transform) = second_transform {
                let a = atom_mut(value, b)?;
                a.location = transform.apply(a.location)?;
            }
        }
        Ok(())
    }
    fn ring_path(
        &self,
        start: usize,
        session: &mut Session,
    ) -> Result<Vec<(usize, [usize; 2])>, Error> {
        let mut result = Vec::new();
        let mut seen = BTreeSet::new();
        let mut stack = vec![start];
        while let Some(id) = stack.pop() {
            session.spend(1)?;
            if seen.contains(&id) {
                continue;
            }
            let mut neighbors = Vec::new();
            for &(neighbor, bond) in at(&self.neighbors, id)? {
                session.spend(1)?;
                if *at(&self.ring_bonds, bond)? {
                    neighbors.push(neighbor);
                }
            }
            if neighbors.len() != 2 {
                continue;
            }
            let pair = [*at(&neighbors, 0)?, *at(&neighbors, 1)?];
            seen.insert(id);
            result.push((id, pair));
            for &neighbor in pair.iter().rev() {
                if !seen.contains(&neighbor) {
                    stack.push(neighbor);
                }
            }
        }
        Ok(result)
    }
    fn shorten_terminal(&self, value: &mut Fragment, id: usize) -> Result<(), Error> {
        let neighbor = self.only_neighbor(id)?;
        let relative = scale(
            sub(atom(value, id)?.location, atom(value, neighbor)?.location)?,
            0.9,
        )?;
        if squared(relative)?.sqrt() > 0.75 {
            let location = add(relative, atom(value, neighbor)?.location)?;
            atom_mut(value, id)?.location = location;
        }
        Ok(())
    }
    pub(super) fn shorten_bonds(
        &self,
        value: &mut Fragment,
        session: &mut Session,
    ) -> Result<(), Error> {
        let mut pairs = self.find_in_place(value, false, session)?;
        for _ in 0..MAX_ITERATIONS {
            let Some(&(mut a, mut b)) = pairs.first() else {
                break;
            };
            let (fixed_a, mut fixed_b) = (atom(value, a)?.fixed, atom(value, b)?.fixed);
            if fixed_a && fixed_b {
                pairs.remove(0);
                continue;
            }
            let (mut degree_a, mut degree_b) = (self.degree(a)?, self.degree(b)?);
            if fixed_a || (degree_b > degree_a && !fixed_b) {
                std::mem::swap(&mut a, &mut b);
                std::mem::swap(&mut degree_a, &mut degree_b);
                fixed_b = fixed_a;
            }
            let path = self.shortest_path(a, b, session)?;
            if path.is_empty() {
                pairs.remove(0);
                continue;
            }
            let mut open = false;
            for pair in path.windows(2) {
                session.spend(1)?;
                if !*at(&self.ring_bonds, self.bond(*at(pair, 0)?, *at(pair, 1)?)?)? {
                    open = true;
                }
            }
            if open {
                if degree_a == 1 {
                    self.shorten_terminal(value, a)?;
                }
                if degree_b == 1 && !fixed_b {
                    self.shorten_terminal(value, b)?;
                }
            } else {
                let mut path = self.ring_path(a, session)?;
                if path.is_empty() {
                    path = self.ring_path(b, session)?;
                }
                let mut moves = BTreeMap::new();
                for &(id, neighbors) in &path {
                    if atom(value, id)?.fixed {
                        continue;
                    }
                    let [first, second] = neighbors;
                    let midpoint = scale(
                        add(atom(value, first)?.location, atom(value, second)?.location)?,
                        0.5,
                    )?;
                    let movement = scale(
                        normalize(sub(midpoint, atom(value, id)?.location)?)?,
                        COLLISION_THRESHOLD,
                    )?;
                    moves.insert(id, movement);
                }
                for (id, _) in path {
                    let movement = moves.get(&id).copied().unwrap_or_default();
                    let a = atom_mut(value, id)?;
                    a.location = add(a.location, movement)?;
                }
            }
            pairs = self.find_in_place(value, false, session)?;
        }
        Ok(())
    }
}
