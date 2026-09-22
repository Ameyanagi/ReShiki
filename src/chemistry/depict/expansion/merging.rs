use super::geometry::{Point, Transform};
use super::rings::EmbeddedAtom;
use super::*;
use crate::chemistry::depict::arithmetic;
use std::f64::consts::PI;

fn atom(f: &Fragment, id: usize) -> Result<&EmbeddedAtom> {
    f.atoms
        .get(&id)
        .ok_or(Error::Invalid("missing embedded atom"))
}
fn atom_mut(f: &mut Fragment, id: usize) -> Result<&mut EmbeddedAtom> {
    f.atoms
        .get_mut(&id)
        .ok_or(Error::Invalid("missing embedded atom"))
}
fn number(v: f64) -> Result<f64> {
    if v.is_finite() {
        Ok(v)
    } else {
        Err(geometry::Error::Numeric.into())
    }
}
fn point(p: Point) -> Result<Point> {
    number(p.x)?;
    number(p.y)?;
    Ok(p)
}
fn sub(a: Point, b: Point) -> Result<Point> {
    point(Point {
        x: a.x - b.x,
        y: a.y - b.y,
    })
}
fn add(a: Point, b: Point) -> Result<Point> {
    point(Point {
        x: a.x + b.x,
        y: a.y + b.y,
    })
}
fn dot(a: Point, b: Point) -> Result<f64> {
    number(arithmetic::dot(a.x, a.y, b.x, b.y))
}
fn length(p: Point) -> Result<f64> {
    number(number(arithmetic::squared_length(p.x, p.y))?.sqrt())
}
fn transform(f: &mut Fragment, t: Transform, work: &mut Budget) -> Result<()> {
    work.spend(f.atoms.len())?;
    for a in f.atoms.values_mut() {
        let temp = add(a.location, a.normal)?;
        a.location = t.apply(a.location)?;
        a.normal = sub(t.apply(temp)?, a.location)?;
    }
    Ok(())
}
fn reflect(f: &mut Fragment, first: Point, second: Point, work: &mut Budget) -> Result<()> {
    work.spend(f.atoms.len())?;
    for a in f.atoms.values_mut() {
        let temp = add(a.location, a.normal)?;
        a.location = geometry::reflect_point(a.location, first, second)?;
        a.normal = sub(geometry::reflect_point(temp, first, second)?, a.location)?;
        a.counter_clockwise = !a.counter_clockwise;
    }
    Ok(())
}
fn one_atom_transform(f: &Fragment, other: &Fragment, id: usize) -> Result<Transform> {
    let current = atom(f, id)?;
    let incoming = atom(other, id)?;
    let [Some(a), Some(b), Some(c), Some(d)] = [
        incoming.neighbor1,
        incoming.neighbor2,
        current.neighbor1,
        current.neighbor2,
    ] else {
        return Err(Error::Invalid("missing single-anchor merge neighbors"));
    };
    let mut midpoint = add(atom(other, a)?.location, atom(other, b)?.location)?;
    midpoint.x *= 0.5;
    midpoint.y *= 0.5;
    let bisect = geometry::bisect_point(
        current.location,
        2.0 * PI - current.angle,
        atom(f, c)?.location,
        atom(f, d)?.location,
    )?;
    Ok(Transform::align(
        current.location,
        bisect,
        incoming.location,
        midpoint,
    )?)
}
fn density_reflection(
    f: &Fragment,
    incoming: &mut Fragment,
    a: usize,
    b: usize,
    work: &mut Budget,
) -> Result<()> {
    let (first, second) = (atom(f, a)?.location, atom(f, b)?.location);
    let (mut normal, mut mirrored) = (0.0, 0.0);
    for (&id, a) in &incoming.atoms {
        work.spend(1)?;
        if f.atoms.contains_key(&id) {
            continue;
        }
        let reflected = geometry::reflect_point(a.location, first, second)?;
        for b in f.atoms.values() {
            work.spend(1)?;
            let d = length(sub(b.location, a.location)?)?;
            let r = length(sub(b.location, reflected)?)?;
            normal += if d > 1.0e-3 { 1.0 / d } else { 1000.0 };
            mirrored += if r > 1.0e-3 { 1.0 / r } else { 1000.0 };
        }
    }
    if number(normal - mirrored)? > 1.0e-4 {
        reflect(incoming, first, second, work)?;
    }
    Ok(())
}
fn third_reflection(
    f: &Fragment,
    incoming: &mut Fragment,
    a: usize,
    b: usize,
    c: usize,
    work: &mut Budget,
) -> Result<()> {
    let (first, second) = (atom(f, a)?.location, atom(f, b)?.location);
    let mut normal = sub(second, first)?;
    normal = Point {
        x: -normal.y,
        y: normal.x,
    };
    let other = sub(atom(incoming, c)?.location, first)?;
    let third = sub(atom(f, c)?.location, first)?;
    let (dot1, dot2) = (dot(normal, third)?, dot(normal, other)?);
    if number(dot1 * dot2)? < 0.0 {
        reflect(incoming, first, second, work)?;
    }
    Ok(())
}
fn stereo_reflection(
    f: &Fragment,
    incoming: &mut Fragment,
    case: u8,
    a: usize,
    b: usize,
    work: &mut Budget,
) -> Result<()> {
    let first = atom(f, a)?.location;
    let (normal, reference) = if case == 1 {
        let control = atom(incoming, a)?;
        let id = control
            .cis_trans_neighbor
            .ok_or(Error::Invalid("missing stereo control"))?;
        let Some(reference) = f.atoms.get(&id) else {
            return Ok(());
        };
        (control.normal, reference.location)
    } else {
        let control = atom(f, a)?;
        let id = control
            .cis_trans_neighbor
            .ok_or(Error::Invalid("missing stereo control"))?;
        (control.normal, {
            if !incoming.atoms.contains_key(&id) {
                work.reserve(1)?;
            }
            incoming
                .atoms
                .entry(id)
                .or_insert_with(|| attachment::fresh(0))
                .location
        })
    };
    if dot(sub(reference, first)?, normal)? < 0.0 {
        reflect(incoming, first, atom(f, b)?.location, work)?;
    }
    Ok(())
}
impl Input<'_> {
    pub(super) fn add_atom(
        &self,
        fragment: &mut Fragment,
        id: usize,
        target: usize,
        length: f64,
        work: &mut Budget,
    ) -> Result<()> {
        work.reserve(
            self.attachment
                .adjacent(id)?
                .len()
                .checked_add(2)
                .ok_or(Error::Limit)?,
        )?;
        self.attachment
            .add_in_place(fragment, id, target, length, work)?;
        Ok(())
    }
    pub fn merge_with_common(
        &self,
        fragment: &Fragment,
        incoming: &Fragment,
        common: &[usize],
        bond_length: f64,
    ) -> Result<Merged> {
        let mut work = Budget::new(self.work_limit);
        self.attachment.validate(fragment, &mut work)?;
        self.attachment.validate(incoming, &mut work)?;
        work.fragment(fragment)?;
        work.fragment(incoming)?;
        if common.len() > self.graph.atoms.len() {
            return Err(Error::Limit);
        }
        work.reserve(common.len())?;
        let mut result = Merged {
            fragment: fragment.clone(),
            incoming: incoming.clone(),
            common: common.to_vec(),
        };
        self.merge_common(
            &mut result.fragment,
            &mut result.incoming,
            &mut result.common,
            bond_length,
            &mut work,
        )?;
        Ok(result)
    }
    pub fn merge_no_common(
        &self,
        fragment: &Fragment,
        incoming: &Fragment,
        target: usize,
        neighbor: usize,
        bond_length: f64,
    ) -> Result<Merged> {
        let mut work = Budget::new(self.work_limit);
        self.attachment.validate(fragment, &mut work)?;
        self.attachment.validate(incoming, &mut work)?;
        work.fragment(fragment)?;
        work.fragment(incoming)?;
        let mut result = Merged {
            fragment: fragment.clone(),
            incoming: incoming.clone(),
            common: vec![target, neighbor],
        };
        self.merge_separate(
            &mut result.fragment,
            &mut result.incoming,
            target,
            neighbor,
            bond_length,
            &mut work,
        )?;
        Ok(result)
    }
    pub(super) fn merge_common(
        &self,
        fragment: &mut Fragment,
        incoming: &mut Fragment,
        common: &mut Vec<usize>,
        bond_length: f64,
        work: &mut Budget,
    ) -> Result<()> {
        if common.is_empty() || common.len() > self.graph.atoms.len() {
            return Err(Error::Invalid("common atom count"));
        }
        let mut unique = BTreeSet::new();
        for &id in common.iter() {
            work.spend(1)?;
            atom(fragment, id)?;
            atom(incoming, id)?;
            if !unique.insert(id) {
                return Err(Error::Invalid("repeated common atom"));
            }
        }
        let mut case = 0;
        if common.len() == 1 {
            let id = *at(common, 0)?;
            let other = if atom(fragment, id)?.cis_trans_neighbor.is_some() {
                case = 2;
                let other = atom(fragment, id)?
                    .neighbor1
                    .ok_or(Error::Invalid("stereo endpoint"))?;
                self.add_atom(incoming, other, id, bond_length, work)?;
                Some(other)
            } else if atom(incoming, id)?.cis_trans_neighbor.is_some() {
                case = 1;
                let other = atom(incoming, id)?
                    .neighbor1
                    .ok_or(Error::Invalid("stereo endpoint"))?;
                self.add_atom(fragment, other, id, bond_length, work)?;
                Some(other)
            } else {
                let other = atom(fragment, id)?.neighbor1;
                if let Some(other) = other {
                    self.add_atom(incoming, other, id, bond_length, work)?;
                }
                other
            };
            if let Some(other) = other {
                common.push(other);
                unique.insert(other);
            }
        }
        let trans = if common.len() == 1 {
            one_atom_transform(fragment, incoming, *at(common, 0)?)?
        } else {
            let (a, b) = (*at(common, 0)?, *at(common, 1)?);
            Transform::align(
                atom(fragment, a)?.location,
                atom(fragment, b)?.location,
                atom(incoming, a)?.location,
                atom(incoming, b)?.location,
            )?
        };
        transform(incoming, trans, work)?;
        if common.len() >= 2 {
            let (a, b) = (*at(common, 0)?, *at(common, 1)?);
            if case > 0 {
                stereo_reflection(fragment, incoming, case, a, b, work)?;
            } else if common.len() == 2 {
                density_reflection(fragment, incoming, a, b, work)?;
            } else {
                third_reflection(fragment, incoming, a, b, *at(common, 2)?, work)?;
            }
        }
        for (&id, a) in &incoming.atoms {
            work.spend(1)?;
            if !unique.contains(&id) {
                work.reserve(a.neighbors.len().checked_add(2).ok_or(Error::Limit)?)?;
                fragment.atoms.insert(id, a.clone());
                work.spend(fragment.attachment_points.len())?;
                if !a.neighbors.is_empty() && !fragment.attachment_points.contains(&id) {
                    fragment.attachment_points.push(id);
                }
            } else {
                let old = atom_mut(fragment, id)?;
                if a.cis_trans_neighbor.is_some() {
                    old.cis_trans_neighbor = a.cis_trans_neighbor;
                    old.normal = a.normal;
                    old.counter_clockwise = a.counter_clockwise;
                }
                if a.angle > 0.0 {
                    old.angle = a.angle;
                    old.neighbor1 = a.neighbor1;
                    old.neighbor2 = a.neighbor2;
                }
            }
        }
        for &id in common.iter() {
            self.attachment.update(fragment, id, work)?;
        }
        Ok(())
    }
    pub(super) fn merge_separate(
        &self,
        fragment: &mut Fragment,
        incoming: &mut Fragment,
        target: usize,
        neighbor: usize,
        bond_length: f64,
        work: &mut Budget,
    ) -> Result<()> {
        self.add_atom(fragment, neighbor, target, bond_length, work)?;
        self.add_atom(incoming, target, neighbor, bond_length, work)?;
        self.merge_common(
            fragment,
            incoming,
            &mut vec![target, neighbor],
            bond_length,
            work,
        )
    }
}
