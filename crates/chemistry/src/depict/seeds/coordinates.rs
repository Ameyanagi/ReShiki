use super::{Error, Input, MAX_ANGLE_PAIRS, Work, at, empty, input_number, normalize, number, sub};
use crate::depict::{
    arithmetic, attachment,
    geometry::{self, Coordinates, Point},
    rings::{EmbeddedAtom, Fragment},
};
use std::f64::consts::PI;

#[cfg(test)]
mod tests;

fn atom(value: &Fragment, id: usize) -> Result<&EmbeddedAtom, Error> {
    value
        .atoms
        .get(&id)
        .ok_or(Error::Invalid("missing coordinate atom"))
}
fn atom_mut(value: &mut Fragment, id: usize) -> Result<&mut EmbeddedAtom, Error> {
    value
        .atoms
        .get_mut(&id)
        .ok_or(Error::Invalid("missing coordinate atom"))
}
fn angle(center: Point, first: Point, second: Point) -> Result<f64, Error> {
    let first = normalize(sub(first, center)?)?;
    let second = normalize(sub(second, center)?)?;
    Ok(
        number(arithmetic::dot(first.x, first.y, second.x, second.y))?
            .clamp(-1.0, 1.0)
            .acos(),
    )
}
impl Input<'_> {
    pub(super) fn coordinate_fragment(
        &self,
        coordinates: &Coordinates,
        work: &mut Work,
    ) -> Result<Fragment, Error> {
        if coordinates.len() > geometry::MAX_POINTS {
            return Err(Error::Limit);
        }
        work.spend(coordinates.len())?;
        let mut fragment = empty();
        for (&id, &location) in coordinates {
            at(&self.graph.atoms, id)?;
            input_number(location.x)?;
            input_number(location.y)?;
            let mut atom = attachment::fresh(id);
            atom.location = location;
            atom.fixed = true;
            fragment.atoms.insert(id, atom);
        }
        fragment = self.attachment.setup_with_budget(&fragment, &mut work.0)?;
        for id in fragment.attachment_points.clone() {
            let pending = &atom(&fragment, id)?.neighbors;
            let adjacent = self.neighbors(id)?;
            work.spend(
                adjacent
                    .len()
                    .checked_mul(pending.len() + 1)
                    .ok_or(Error::Limit)?,
            )?;
            let done: Vec<_> = adjacent
                .iter()
                .copied()
                .filter(|id| !pending.contains(id))
                .collect();
            let center = atom(&fragment, id)?.location;
            match done.as_slice() {
                [] => {
                    let a = atom_mut(&mut fragment, id)?;
                    a.normal = Point { x: 1.0, y: 0.0 };
                    a.angle = -1.0;
                }
                [neighbor] => {
                    let mut normal = normalize(sub(atom(&fragment, *neighbor)?.location, center)?)?;
                    std::mem::swap(&mut normal.x, &mut normal.y);
                    normal.x *= -1.0;
                    let a = atom_mut(&mut fragment, id)?;
                    a.neighbor1 = Some(*neighbor);
                    a.normal = normal;
                }
                [first, second] => {
                    let angle = angle(
                        center,
                        atom(&fragment, *first)?.location,
                        atom(&fragment, *second)?.location,
                    )?;
                    let a = atom_mut(&mut fragment, id)?;
                    a.neighbor1 = Some(*first);
                    a.neighbor2 = Some(*second);
                    a.angle = angle;
                }
                _ => self.neighbor_angles(&mut fragment, id, &done, work)?,
            }
        }
        Ok(fragment)
    }
    fn neighbor_angles(
        &self,
        fragment: &mut Fragment,
        id: usize,
        done: &[usize],
        work: &mut Work,
    ) -> Result<(), Error> {
        let count = done.len();
        if count > MAX_ANGLE_PAIRS {
            return Err(Error::Limit);
        }
        let log = usize::try_from(usize::BITS - count.leading_zeros()).map_err(|_| Error::Limit)?;
        work.spend(count.checked_mul(2 * log + 3).ok_or(Error::Limit)?)?;
        let center = atom(fragment, id)?.location;
        // Consecutive bond directions bound gaps without another bond in
        // them, including exterior gaps larger than pi (RDKit #9650).
        let mut directions = Vec::with_capacity(count);
        for &neighbor in done {
            let delta = sub(atom(fragment, neighbor)?.location, center)?;
            directions.push((number(delta.y.atan2(delta.x))?, neighbor));
        }
        directions.sort_by(|a, b| {
            if a.0 == b.0 {
                a.1.cmp(&b.1)
            } else {
                a.0.total_cmp(&b.0)
            }
        });
        let mut pairs = Vec::with_capacity(count);
        for (index, start) in directions.iter().enumerate() {
            let end = at(&directions, (index + 1) % count)?;
            let mut gap = number(end.0 - start.0)?;
            if index + 1 == count {
                gap = number(gap + 2.0 * PI)?;
            }
            pairs.push((gap, (start.1, end.1)));
        }
        pairs.sort_by(|a, b| {
            if a.0 == b.0 {
                a.1.cmp(&b.1)
            } else {
                a.0.total_cmp(&b.0)
            }
        });
        let mut winner = *pairs.last().ok_or(Error::Invalid("no angle pair"))?;
        let rings = self
            .ring_counts
            .as_ref()
            .ok_or(Error::Invalid("ring cache is uninitialized"))?;
        for &pair in pairs.iter().rev() {
            if *at(rings, pair.1.0)? <= 1 && *at(rings, pair.1.1)? <= 1 {
                winner = pair;
                break;
            }
        }
        // Rotate neighbor2 towards neighbor1 through the selected gap.
        let target = atom_mut(fragment, id)?;
        target.rotation_direction = 1;
        target.neighbor1 = Some(winner.1.1);
        target.neighbor2 = Some(winner.1.0);
        target.angle = 2.0 * PI - winner.0;
        Ok(())
    }
}
