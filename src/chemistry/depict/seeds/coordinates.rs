use super::{Error, Input, MAX_ANGLE_PAIRS, Work, at, empty, input_number, normalize, number, sub};
use crate::chemistry::depict::{
    arithmetic, attachment,
    geometry::{self, Coordinates, Point},
    rings::{EmbeddedAtom, Fragment},
};
use std::f64::consts::PI;

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
        fragment = self.attachment.setup_neighbors(&fragment)?;
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
        let count = done
            .len()
            .checked_mul(
                done.len()
                    .checked_sub(1)
                    .ok_or(Error::Invalid("angle neighbor count"))?,
            )
            .ok_or(Error::Limit)?
            / 2;
        if count > MAX_ANGLE_PAIRS {
            return Err(Error::Limit);
        }
        let log = usize::try_from(usize::BITS - count.leading_zeros()).map_err(|_| Error::Limit)?;
        work.spend(count.checked_mul(log + 1).ok_or(Error::Limit)?)?;
        let center = atom(fragment, id)?.location;
        let mut pairs = Vec::with_capacity(count);
        for (index, &first) in done.iter().enumerate() {
            for &second in done.iter().skip(index + 1) {
                pairs.push((
                    angle(
                        center,
                        atom(fragment, first)?.location,
                        atom(fragment, second)?.location,
                    )?,
                    (first, second),
                ));
            }
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
        let (w1, w2) = winner.1;
        let mut anchors = None;
        for &(_, (a, b)) in &pairs {
            anchors = if a == w1 {
                Some((b, w1))
            } else if b == w1 {
                Some((a, w1))
            } else if a == w2 {
                Some((b, w2))
            } else if b == w2 {
                Some((a, w2))
            } else {
                None
            };
            if anchors.is_some() {
                break;
            }
        }
        let (first, second) = anchors.ok_or(Error::Invalid("no angle anchors"))?;
        let a = sub(atom(fragment, first)?.location, center)?;
        let b = sub(atom(fragment, second)?.location, center)?;
        let cross = number(arithmetic::cross(a.x, a.y, b.x, b.y))?;
        let direction = if number(cross * (PI - winner.0))? >= 0.0 {
            -1
        } else {
            1
        };
        let target = atom_mut(fragment, id)?;
        target.rotation_direction = direction;
        target.neighbor1 = Some(first);
        target.neighbor2 = Some(second);
        target.angle = 2.0 * PI - winner.0;
        Ok(())
    }
}
