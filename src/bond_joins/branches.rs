//! Preserve the silhouette of an unambiguous thick/thin ring junction.
use super::{cap, cross, eligible, end_width, other, subtract};
use crate::document::{Bond, Document, Point};

pub(super) struct Backbone<'a> {
    edges: [&'a Bond; 2],
    center: Point,
}
impl Backbone<'_> {
    pub(super) fn contains(&self, bond: &Bond) -> bool {
        self.edges.iter().any(|edge| std::ptr::eq(*edge, bond))
    }
    pub(super) fn boundary(&self, doc: &Document, id: u64) -> Option<[(Point, Point, f32); 2]> {
        let point = doc.atom(id)?.position;
        let [first, second] = self.edges;
        let first_end = doc.atom(other(first, id))?.position;
        let corners = cap(doc, first, id, point, first_end);
        let outer = corners
            .into_iter()
            .max_by(|a, b| a.distance(self.center).total_cmp(&b.distance(self.center)))?;
        let edge = |bond: &Bond| {
            let end = doc.atom(other(bond, id))?.position;
            let length = point.distance(end);
            if length < 0.001 {
                return None;
            }
            let u = Point::new((end.x - point.x) / length, (end.y - point.y) / length);
            let side = cross(u, subtract(outer, point)).signum();
            let width = end_width(doc, bond, other(bond, id));
            let direction = subtract(end.offset(-u.y * width * side, u.x * width * side), outer);
            let inside = cross(direction, subtract(self.center, outer)).signum();
            Some((outer, direction, inside))
        };
        Some([edge(first)?, edge(second)?])
    }
}

pub(super) fn backbone(doc: &Document, id: u64) -> Option<Backbone<'_>> {
    let atom = doc.atom(id)?;
    if crate::atom_labels::visible(atom, doc) {
        return None;
    }
    let incident: Vec<_> = doc
        .bonds
        .iter()
        .filter(|b| (b.a == id || b.b == id) && doc.bond_visible(b.a, b.b))
        .collect();
    let [a, b, c] = incident.as_slice() else {
        return None;
    };
    if incident
        .iter()
        .any(|b| !eligible(doc, b) || b.display == "hollow_wedge")
        || (end_width(doc, a, id) - end_width(doc, b, id)).abs() < 0.001
            && (end_width(doc, a, id) - end_width(doc, c, id)).abs() < 0.001
    {
        return None;
    }
    let rings = crate::aromatic::ring_circles(doc, false);
    let mut rings = rings.iter().filter(|r| r.atoms.contains(&id));
    let ring = rings.next()?;
    if rings.next().is_some() {
        return None;
    }
    let mut edges = incident
        .iter()
        .copied()
        .filter(|b| ring.contains_bond(b.a, b.b));
    let first = edges.next()?;
    let second = edges.next()?;
    let branch = incident
        .iter()
        .copied()
        .find(|b| !ring.contains_bond(b.a, b.b))?;
    if branch.display != "plain" || branch.order != 1 {
        return None;
    }
    let outward = subtract(atom.position, ring.center);
    let direction = subtract(doc.atom(other(branch, id))?.position, atom.position);
    if outward.x * direction.x + outward.y * direction.y <= 0. {
        return None;
    }
    let result = Backbone {
        edges: [first, second],
        center: ring.center,
    };
    // The two ring rays must surround their interior; edge-on/folded outlines
    // keep the general bounded junction rather than extrapolating a corner.
    let u = subtract(doc.atom(other(first, id))?.position, atom.position);
    let v = subtract(doc.atom(other(second, id))?.position, atom.position);
    if cross(u, v).abs() < 0.001 * u.distance(Point::default()) * v.distance(Point::default()) {
        return None;
    }
    Some(result)
}

fn clip(points: &[Point], line: &(Point, Point, f32), inside: bool) -> Vec<Point> {
    let (origin, direction, sign) = *line;
    let distance = |p: Point| cross(direction, subtract(p, origin)) * sign;
    let keep = |d: f32| if inside { d >= 0. } else { d <= 0. };
    let mut result = Vec::new();
    for (a, b) in points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
    {
        let da = distance(*a);
        let db = distance(*b);
        if keep(da) {
            result.push(*a);
        }
        if keep(da) != keep(db) {
            let t = da / (da - db);
            result.push(a.offset((b.x - a.x) * t, (b.y - a.y) * t));
        }
    }
    result
}

pub(super) fn outside(
    mut polygon: Vec<Point>,
    boundary: &[(Point, Point, f32); 2],
) -> Vec<Vec<Point>> {
    let mut result = Vec::new();
    // Subtract the intersection of the two inward half-planes. The remaining
    // pieces meet the ring outline exactly, including a root spanning its tip.
    for line in boundary {
        let part = clip(&polygon, line, false);
        if part.len() >= 3 {
            result.push(part);
        }
        polygon = clip(&polygon, line, true);
    }
    result
}
