//! Ring construction shared by the ordinary and template-seeded constructors.
use super::{EmbeddedAtom, Error, Fragment, Input, Work, at};
use crate::chemistry::depict::{
    arithmetic,
    geometry::{self, Bounds, Coordinates, Point, Transform},
};
use std::{collections::BTreeMap, f64::consts::PI};

fn number(value: f64) -> Result<f64, Error> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(geometry::Error::Numeric.into())
    }
}
fn point(value: Point) -> Result<Point, Error> {
    number(value.x)?;
    number(value.y)?;
    Ok(value)
}
fn add(a: Point, b: Point) -> Result<Point, Error> {
    point(Point {
        x: a.x + b.x,
        y: a.y + b.y,
    })
}
fn sub(a: Point, b: Point) -> Result<Point, Error> {
    point(Point {
        x: a.x - b.x,
        y: a.y - b.y,
    })
}
fn length(value: Point) -> Result<f64, Error> {
    number(arithmetic::squared_length(value.x, value.y)).map(f64::sqrt)
}
fn atom(atoms: &BTreeMap<usize, EmbeddedAtom>, id: usize) -> Result<&EmbeddedAtom, Error> {
    atoms
        .get(&id)
        .ok_or(Error::Invalid("missing embedded atom"))
}

impl EmbeddedAtom {
    fn new(id: usize, location: Point, angle: f64, previous: usize, next: usize) -> Self {
        Self {
            id,
            location,
            normal: Point::default(),
            angle,
            neighbor1: Some(previous),
            neighbor2: Some(next),
            cis_trans_neighbor: None,
            counter_clockwise: true,
            rotation_direction: 0,
            neighbors: Vec::new(),
            density: -1.0,
            fixed: false,
        }
    }
    fn transform(&mut self, transform: Transform) -> Result<(), Error> {
        let temp = add(self.location, self.normal)?;
        self.location = transform.apply(self.location)?;
        self.normal = sub(transform.apply(temp)?, self.location)?;
        Ok(())
    }
    fn reflect(&mut self, first: Point, second: Point) -> Result<(), Error> {
        let temp = add(self.location, self.normal)?;
        self.location = geometry::reflect_point(self.location, first, second)?;
        self.normal = sub(geometry::reflect_point(temp, first, second)?, self.location)?;
        self.counter_clockwise = !self.counter_clockwise;
        Ok(())
    }
}

fn from_ring(
    ring: &[usize],
    coordinates: &Coordinates,
) -> Result<BTreeMap<usize, EmbeddedAtom>, Error> {
    let count = u32::try_from(ring.len()).map_err(|_| Error::Limit)?;
    let angle = PI * (1.0 - (2.0 / f64::from(count)));
    let mut atoms = BTreeMap::new();
    for (index, &id) in ring.iter().enumerate() {
        let previous = if index == 0 {
            *ring.last().ok_or(Error::Invalid("empty ring"))?
        } else {
            *at(ring, index - 1)?
        };
        let next = *at(ring, (index + 1) % ring.len())?;
        let location = *coordinates
            .get(&id)
            .ok_or(Error::Invalid("missing ring coordinate"))?;
        atoms.insert(id, EmbeddedAtom::new(id, location, angle, previous, next));
    }
    Ok(atoms)
}

impl Input<'_> {
    /// Exact no-template constructor stage. Errors leave inputs unchanged.
    pub fn embed_without_templates(&self, bond_length: f64) -> Result<Fragment, Error> {
        self.begin(bond_length)?.finish(None)
    }
    /// Preserve native coordinate construction/mirroring before a core-template
    /// attempt. Full-system templates bypass this stage in EmbeddedFrag.
    pub(in crate::chemistry::depict) fn begin(
        &self,
        bond_length: f64,
    ) -> Result<Construction<'_, '_>, Error> {
        if self.selected.is_empty() {
            return Err(Error::Invalid("empty ring system"));
        }
        let mut work = self.work();
        let mut coordinates = Vec::new();
        let mut union = Vec::new();
        for index in 0..self.selected.len() {
            let ring = self.ring(index)?;
            work.spend(ring.len())?;
            let mut positions = geometry::embed_ring(ring, bond_length)?;
            self.mirror_trans(ring, &mut positions, &mut work)?;
            coordinates.push(positions);
            for &id in ring {
                if !work.contains(&union, id)? {
                    union.push(id);
                }
            }
        }
        Ok(Construction {
            input: self,
            work,
            coordinates,
            union,
        })
    }

    // Native traversal order matters when adjacent trans bonds successively
    // mirror the same neighboring coordinates. Do not collect then transform.
    fn mirror_trans(
        &self,
        ring: &[usize],
        coordinates: &mut Coordinates,
        work: &mut Work,
    ) -> Result<(), Error> {
        for (index, &first) in ring.iter().enumerate() {
            work.spend(1)?;
            let second = *at(ring, (index + 1) % ring.len())?;
            let id = self
                .bond(first, second)
                .ok_or(Error::Invalid("missing ring bond"))?;
            if at(&self.graph.bonds, id)?.order != 2 {
                continue;
            }
            let stereo = at(&self.metadata.bonds, id)?;
            if stereo.stereo <= 1 || stereo.stereo_atoms.len() != 2 {
                continue;
            }
            let left_in = work.contains(ring, *at(&stereo.stereo_atoms, 0)?)?;
            let right_in = work.contains(ring, *at(&stereo.stereo_atoms, 1)?)?;
            let trans = if matches!(stereo.stereo, 3 | 5) {
                left_in == right_in
            } else {
                left_in != right_in
            };
            if !trans {
                continue;
            }
            let previous = if index == 0 {
                *ring.last().ok_or(Error::Invalid("empty ring"))?
            } else {
                *at(ring, index - 1)?
            };
            let last = *coordinates
                .get(&previous)
                .ok_or(Error::Invalid("missing mirror anchor"))?;
            let reference = *coordinates
                .get(&second)
                .ok_or(Error::Invalid("missing mirror anchor"))?;
            let interest = *coordinates
                .get(&first)
                .ok_or(Error::Invalid("missing mirror atom"))?;
            let d = sub(last, reference)?;
            let denominator = number(arithmetic::squared_length(d.x, d.y))?;
            let a = number(arithmetic::multiply_subtract(d.x, d.x, d.y, d.y) / denominator)?;
            let b = number(2.0 * d.x * d.y / denominator)?;
            let result = point(Point {
                x: arithmetic::dot(a, b, interest.x - reference.x, interest.y - reference.y)
                    + reference.x,
                y: arithmetic::multiply_subtract(
                    b,
                    interest.x - reference.x,
                    a,
                    interest.y - reference.y,
                ) + reference.y,
            })?;
            coordinates.insert(first, result);
        }
        Ok(())
    }
}

/// Detached continuation shared by ordinary and template-seeded ring assembly.
pub(in crate::chemistry::depict) struct Construction<'a, 'g> {
    input: &'a Input<'g>,
    work: Work,
    coordinates: Vec<Coordinates>,
    union: Vec<usize>,
}
impl Construction<'_, '_> {
    pub(in crate::chemistry::depict) fn finish(
        self,
        seed: Option<(Fragment, Vec<usize>)>,
    ) -> Result<Fragment, Error> {
        let Self {
            input,
            mut work,
            coordinates,
            union,
        } = self;
        let (mut atoms, mut done, attachment_points) = if let Some((fragment, done)) = seed {
            if done.is_empty() || done.iter().any(|&i| i >= input.selected.len()) {
                return Err(Error::Invalid("invalid template ring seed"));
            }
            (fragment.atoms, done, fragment.attachment_points)
        } else {
            let first = input
                .first(&mut work)?
                .ok_or(Error::Invalid("empty ring system"))?;
            (
                from_ring(input.ring(first)?, at(&coordinates, first)?)?,
                vec![first],
                Vec::new(),
            )
        };
        while atoms.len() < union.len() {
            work.spend(1)?;
            let next = input.next_with_work(&done, &mut work)?;
            if work.contains(&done, next.ring)? {
                return Err(Error::Invalid("ring selection made no progress"));
            }
            let mut other = from_ring(input.ring(next.ring)?, at(&coordinates, next.ring)?)?;
            let common = &next.common_atoms;
            let first = *common
                .first()
                .ok_or(Error::Invalid("disconnected ring system"))?;
            let (transform, pins) = if common.len() == 1 {
                (one_atom_transform(&atoms, &other, first)?, vec![first])
            } else {
                let last = *common.last().ok_or(Error::Invalid("missing common atom"))?;
                (
                    Transform::align(
                        atom(&atoms, first)?.location,
                        atom(&atoms, last)?.location,
                        atom(&other, first)?.location,
                        atom(&other, last)?.location,
                    )?,
                    vec![first, last],
                )
            };
            work.spend(other.len())?;
            for other_atom in other.values_mut() {
                other_atom.transform(transform)?;
            }
            if common.len() > 1 {
                reflect_density(&atoms, &mut other, &pins, &mut work)?;
            }
            merge(&mut atoms, other, common.len(), &pins, &mut work)?;
            done.push(next.ring);
            if done.len() > input.selected.len() {
                return Err(Error::Invalid("ring selection made no progress"));
            }
        }
        Ok(Fragment {
            atoms,
            done: false,
            bounds: Bounds {
                positive_x: 0.0,
                negative_x: 0.0,
                positive_y: 0.0,
                negative_y: 0.0,
            },
            attachment_points,
        })
    }
}

fn one_atom_transform(
    atoms: &BTreeMap<usize, EmbeddedAtom>,
    other: &BTreeMap<usize, EmbeddedAtom>,
    id: usize,
) -> Result<Transform, Error> {
    let current = atom(atoms, id)?;
    let incoming = atom(other, id)?;
    let first = incoming
        .neighbor1
        .ok_or(Error::Invalid("missing incoming ring neighbor"))?;
    let second = incoming
        .neighbor2
        .ok_or(Error::Invalid("missing incoming ring neighbor"))?;
    let mut midpoint = add(atom(other, first)?.location, atom(other, second)?.location)?;
    midpoint.x *= 0.5;
    midpoint.y *= 0.5;
    let first = current
        .neighbor1
        .ok_or(Error::Invalid("missing embedded ring neighbor"))?;
    let second = current
        .neighbor2
        .ok_or(Error::Invalid("missing embedded ring neighbor"))?;
    let largest_angle = 2.0 * PI - current.angle;
    let bisector = geometry::bisect_point(
        current.location,
        largest_angle,
        atom(atoms, first)?.location,
        atom(atoms, second)?.location,
    )?;
    Ok(Transform::align(
        current.location,
        bisector,
        incoming.location,
        midpoint,
    )?)
}

fn reflect_density(
    atoms: &BTreeMap<usize, EmbeddedAtom>,
    other: &mut BTreeMap<usize, EmbeddedAtom>,
    pins: &[usize],
    work: &mut Work,
) -> Result<(), Error> {
    let first = atom(atoms, *at(pins, 0)?)?.location;
    let second = atom(atoms, *at(pins, 1)?)?.location;
    let (mut normal, mut reflected) = (0.0, 0.0);
    for (&id, incoming) in other.iter() {
        work.spend(1)?;
        if atoms.contains_key(&id) {
            continue;
        }
        let reflected_location = geometry::reflect_point(incoming.location, first, second)?;
        for current in atoms.values() {
            work.spend(1)?;
            let distance = length(sub(current.location, incoming.location)?)?;
            let reflected_distance = length(sub(current.location, reflected_location)?)?;
            normal += if distance > 1e-3 {
                1.0 / distance
            } else {
                1000.0
            };
            reflected += if reflected_distance > 1e-3 {
                1.0 / reflected_distance
            } else {
                1000.0
            };
        }
    }
    number(normal)?;
    number(reflected)?;
    if normal - reflected > 1e-4 {
        work.spend(other.len())?;
        for incoming in other.values_mut() {
            incoming.reflect(first, second)?;
        }
    }
    Ok(())
}

fn merge(
    atoms: &mut BTreeMap<usize, EmbeddedAtom>,
    other: BTreeMap<usize, EmbeddedAtom>,
    common: usize,
    pins: &[usize],
    work: &mut Work,
) -> Result<(), Error> {
    for (id, incoming) in other {
        work.spend(1)?;
        if let Some(current) = atoms.get_mut(&id) {
            if common <= 2 && pins.contains(&id) {
                current.angle = number(current.angle + incoming.angle)?;
                if current.neighbor1 == incoming.neighbor1 {
                    current.neighbor1 = incoming.neighbor2;
                } else if current.neighbor1 == incoming.neighbor2 {
                    current.neighbor1 = incoming.neighbor1;
                } else if current.neighbor2 == incoming.neighbor1 {
                    current.neighbor2 = incoming.neighbor2;
                } else if current.neighbor2 == incoming.neighbor2 {
                    current.neighbor2 = incoming.neighbor1;
                }
            }
        } else {
            atoms.insert(id, incoming);
        }
    }
    Ok(())
}
