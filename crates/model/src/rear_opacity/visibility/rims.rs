use super::triangle::Triangle;
use super::vector::cross2;
use super::{Document, Point};
use crate::polygon::closed_edges;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn on_segment(p: Point, a: Point, b: Point, epsilon: f64) -> bool {
    let (x, y) = (
        f64::from(b.x) - f64::from(a.x),
        f64::from(b.y) - f64::from(a.y),
    );
    let length = x.hypot(y);
    if length < epsilon {
        return p.distance(a) <= epsilon as f32;
    }
    if cross2(a, b, p).abs() > epsilon * length {
        return false;
    }
    let along = (f64::from(p.x) - f64::from(a.x)) * x + (f64::from(p.y) - f64::from(a.y)) * y;
    along >= -epsilon * length && along <= length * length + epsilon * length
}
pub(super) type RimAtoms = BTreeMap<u64, (u64, Point)>;
pub(super) type RimBonds = BTreeMap<(u64, u64), (u64, Point, Point)>;
pub(super) fn rims(
    doc: &Document,
    triangles: &[Triangle],
    epsilon: f64,
) -> (RimAtoms, RimBonds, BTreeMap<u64, u64>) {
    let mut groups: BTreeMap<u64, BTreeSet<u64>> = BTreeMap::new();
    for triangle in triangles {
        groups
            .entry(triangle.shell)
            .or_default()
            .extend(&triangle.face);
    }
    let mut atoms = BTreeMap::new();
    let mut bonds = BTreeMap::new();
    let mut memberships = BTreeMap::new();
    for (shell, ids) in groups {
        let mut points: Vec<_> = ids
            .iter()
            .filter_map(|id| doc.atom(*id).map(|a| (a.position, *id)))
            .collect();
        let low = convex_hull(&mut points);
        if low.len() < 3 {
            continue;
        }
        for (p, id) in &points {
            memberships.insert(*id, shell);
            if closed_edges(&low).any(|(a, b)| on_segment(*p, a.0, b.0, epsilon)) {
                atoms.insert(*id, (shell, *p));
            }
        }
        for bond in doc
            .bonds
            .iter()
            .filter(|b| ids.contains(&b.a) && ids.contains(&b.b))
        {
            let (Some(a), Some(b)) = (doc.atom(bond.a), doc.atom(bond.b)) else {
                continue;
            };
            let (a, b) = (a.position, b.position);
            if closed_edges(&low)
                .any(|(x, y)| on_segment(a, x.0, y.0, epsilon) && on_segment(b, x.0, y.0, epsilon))
            {
                bonds.insert((bond.a.min(bond.b), bond.a.max(bond.b)), (shell, a, b));
            }
        }
    }
    (atoms, bonds, memberships)
}
fn last_turn(stack: &[(Point, u64)], p: Point) -> Option<f64> {
    let mut tail = stack.iter().rev();
    let b = tail.next()?.0;
    let a = tail.next()?.0;
    Some(cross2(a, b, p))
}
pub(super) fn convex_hull(points: &mut [(Point, u64)]) -> Vec<(Point, u64)> {
    points.sort_by(|a, b| a.0.x.total_cmp(&b.0.x).then(a.0.y.total_cmp(&b.0.y)));
    let mut low: Vec<(Point, u64)> = vec![];
    let mut high: Vec<(Point, u64)> = vec![];
    for &p in points.iter() {
        while last_turn(&low, p.0).is_some_and(|turn| turn <= 0.) {
            low.pop();
        }
        low.push(p);
    }
    for &p in points.iter().rev() {
        while last_turn(&high, p.0).is_some_and(|turn| turn <= 0.) {
            high.pop();
        }
        high.push(p);
    }
    low.pop();
    high.pop();
    low.extend(high);
    low
}
