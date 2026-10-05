//! Shared, bounded corners for normal, bold and tapered bond outlines.
use crate::document::{Bond, Document, Point};
#[cfg(test)]
mod double_tests;
#[cfg(test)]
thread_local! {
    static CONSTRUCTIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
#[cfg(test)]
pub(crate) fn construction_count() -> usize {
    CONSTRUCTIONS.with(std::cell::Cell::get)
}

fn eligible(doc: &Document, b: &Bond) -> bool {
    (b.order == 1
        || b.order == 4 && b.projection
        || matches!(b.order, 2 | 7)
            && match crate::scene::effective_double_position(doc, b) {
                crate::bonds::DoublePosition::Auto => false,
                crate::bonds::DoublePosition::Left | crate::bonds::DoublePosition::Right => true,
                crate::bonds::DoublePosition::Center => false,
            })
        && matches!(
            b.display.as_str(),
            "plain" | "bold" | "wedge" | "hollow_wedge"
        )
}
fn other(b: &Bond, id: u64) -> u64 {
    if b.a == id { b.b } else { b.a }
}
/// Per-scene graph facts. Rebuilt from a borrowed document, so no persistent
/// invalidation or stale geometry is involved when atoms or bonds change.
pub struct Joins<'a> {
    doc: &'a Document,
    atoms: std::collections::HashMap<u64, (&'a crate::document::Atom, bool)>,
    incident: std::collections::HashMap<u64, Vec<(usize, &'a Bond)>>,
    eligible: std::collections::HashSet<(u64, u64)>,
}
impl<'a> Joins<'a> {
    pub fn new(doc: &'a Document) -> Self {
        #[cfg(test)]
        CONSTRUCTIONS.with(|count| count.set(count.get() + 1));
        let atoms = doc
            .atoms
            .iter()
            .map(|a| (a.id, (a, crate::atom_labels::visible(a, doc))))
            .collect();
        let mut incident: std::collections::HashMap<u64, Vec<(usize, &'a Bond)>> =
            Default::default();
        let mut eligible_bonds = std::collections::HashSet::new();
        for (index, bond) in doc.bonds.iter().enumerate() {
            if eligible(doc, bond) {
                eligible_bonds.insert((bond.a, bond.b));
                if doc.bond_visible(bond.a, bond.b) {
                    for id in [bond.a, bond.b] {
                        incident.entry(id).or_default().push((index, bond));
                    }
                }
            }
        }
        Self {
            doc,
            atoms,
            incident,
            eligible: eligible_bonds,
        }
    }
    fn neighbors(&self, bond: &Bond, id: u64) -> impl Iterator<Item = &'a Bond> {
        self.incident
            .get(&id)
            .into_iter()
            .flatten()
            .filter_map(move |(_, b)| (!std::ptr::eq(*b, bond)).then_some(*b))
    }
    pub fn needed(&self, b: &Bond) -> bool {
        self.eligible.contains(&(b.a, b.b))
            && (b.display != "plain"
                || [b.a, b.b]
                    .iter()
                    .any(|id| self.neighbors(b, *id).next().is_some()))
    }
}
#[cfg(test)]
pub fn needed(doc: &Document, b: &Bond) -> bool {
    Joins::new(doc).needed(b)
}
fn half_width(doc: &Document, b: &Bond) -> f32 {
    doc.drawing_style.world(
        if b.display == "bold" || matches!(b.display.as_str(), "wedge" | "hollow_wedge") {
            doc.drawing_style.bold_width_pt
        } else {
            doc.drawing_style.line_width_pt
        },
    ) / 2.
}
fn end_width(doc: &Document, b: &Bond, id: u64) -> f32 {
    if matches!(b.display.as_str(), "wedge" | "hollow_wedge") && id == b.a {
        doc.drawing_style.line_width() / 2.
    } else {
        half_width(doc, b)
    }
}
fn cross(a: Point, b: Point) -> f32 {
    a.x * b.y - a.y * b.x
}
fn subtract(a: Point, b: Point) -> Point {
    Point::new(a.x - b.x, a.y - b.y)
}
impl Joins<'_> {
    fn cap(&self, b: &Bond, id: u64, point: Point, opposite: Point) -> [Point; 2] {
        let doc = self.doc;
        let length = point.distance(opposite).max(0.001);
        let u = Point::new(
            (opposite.x - point.x) / length,
            (opposite.y - point.y) / length,
        );
        let width = end_width(doc, b, id);
        let far_width = end_width(doc, b, other(b, id));
        let corner = |side: f32| {
            let start = point.offset(-u.y * width * side, u.x * width * side);
            if self
                .atoms
                .get(&id)
                .is_none_or(|(a, visible)| a.position.distance(point) > 0.001 || *visible)
            {
                return start;
            }
            // Each edge meets its angular neighbour, including three-way junctions.
            let adjacent = self
                .neighbors(b, id)
                .filter_map(|adj| {
                    let end = self.atoms.get(&other(adj, id))?.0.position;
                    let v = subtract(end, point);
                    let angle = (side * cross(u, v).atan2(u.x * v.x + u.y * v.y))
                        .rem_euclid(std::f32::consts::TAU);
                    (v.distance(Point::default()) > 0.001 && angle > 0.001)
                        .then_some((adj, end, angle))
                })
                .min_by(|a, b| a.2.total_cmp(&b.2));
            let Some((adj, end, _)) = adjacent else {
                return start;
            };
            let adjacent_length = point.distance(end).max(0.001);
            let v = Point::new(
                (end.x - point.x) / adjacent_length,
                (end.y - point.y) / adjacent_length,
            );
            let w = end_width(doc, adj, id);
            let far_w = end_width(doc, adj, other(adj, id));
            let a = point.offset(v.y * w * side, -v.x * w * side);
            let direction = subtract(
                opposite.offset(-u.y * far_width * side, u.x * far_width * side),
                start,
            );
            let adjacent_direction =
                subtract(end.offset(v.y * far_w * side, -v.x * far_w * side), a);
            let denominator = cross(direction, adjacent_direction);
            if denominator.abs() < 0.001 {
                return start;
            }
            let t = cross(subtract(a, start), adjacent_direction) / denominator;
            let intersection = start.offset(direction.x * t, direction.y * t);
            let offset = subtract(intersection, point);
            let limit = (4. * width.max(w)).min(0.45 * length.min(adjacent_length));
            let scale = (limit / offset.distance(Point::default()).max(0.001)).min(1.);
            point.offset(offset.x * scale, offset.y * scale)
        };
        [corner(1.), corner(-1.)]
    }
    pub fn polygon(&self, b: &Bond, start: Point, end: Point) -> Vec<Point> {
        let [al, ar] = self.cap(b, b.a, start, end);
        let [bl, br] = self.cap(b, b.b, end, start);
        // A three-way cap passes through the atom between its two shared corners.
        // Joining those corners directly would cut off a colored branch's root.
        let joint = |id, point: Point| {
            b.display != "hollow_wedge"
                && self.atoms.get(&id).is_some_and(|(a, visible)| {
                    a.position.distance(point) < 0.001
                        && !*visible
                        && self.neighbors(b, id).take(2).count() == 2
                })
        };
        let mut points = vec![al, br];
        if joint(b.b, end) {
            points.push(end);
        }
        points.extend([bl, ar]);
        if joint(b.a, start) {
            points.push(start);
        }
        points
    }
}
#[cfg(test)]
fn cap(doc: &Document, b: &Bond, id: u64, point: Point, opposite: Point) -> [Point; 2] {
    Joins::new(doc).cap(b, id, point, opposite)
}
#[cfg(test)]
pub fn polygon(doc: &Document, b: &Bond, start: Point, end: Point) -> Vec<Point> {
    Joins::new(doc).polygon(b, start, end)
}

pub struct Junction {
    pub color: crate::palette::Color,
    pub parts: Vec<(usize, Vec<Point>)>,
    pub underlay: bool,
}

/// Mixed-color sectors share exact edges but are antialiased separately.
/// Paint their union underneath the color paths to close transparent seams.
/// Keep the concave boundaries: a convex hull would fill the notch between
/// the thin ring edge and the substituent at a thick/thin/branch junction.
impl Joins<'_> {
    pub fn junctions(&self) -> Vec<Junction> {
        let doc = self.doc;
        let mut result = Vec::new();
        for atom in &doc.atoms {
            if !doc.atom_visible(atom.id)
                || self.atoms.get(&atom.id).is_none_or(|(_, visible)| *visible)
            {
                continue;
            }
            let incident = self
                .incident
                .get(&atom.id)
                .map(Vec::as_slice)
                .unwrap_or_default();
            let Some((_, first)) = incident.first() else {
                continue;
            };
            let underlay = incident.iter().any(|(_, b)| b.color != first.color);
            if incident.len() < 3
                || !underlay && incident.iter().all(|(_, b)| b.display != "hollow_wedge")
            {
                continue;
            }
            let mut parts = Vec::new();
            for (index, bond) in incident {
                let Some((end, _)) = self.atoms.get(&other(bond, atom.id)) else {
                    continue;
                };
                let length = atom.position.distance(end.position);
                if length < 0.001 {
                    continue;
                }
                let [left, right] = self.cap(bond, atom.id, atom.position, end.position);
                if !underlay || bond.display == "hollow_wedge" {
                    // A hollow wedge keeps its open interior; only its root joins
                    // the shared atom, not a filled segment along its length.
                    let mut triangle = vec![left, right, atom.position];
                    if cross(subtract(right, left), subtract(atom.position, left)) > 0. {
                        triangle.reverse();
                    }
                    parts.push((*index, triangle));
                    continue;
                }
                let width = end_width(doc, bond, atom.id);
                let u = Point::new(
                    (end.position.x - atom.position.x) / length,
                    (end.position.y - atom.position.y) / length,
                );
                let along =
                    |p: Point| (p.x - atom.position.x) * u.x + (p.y - atom.position.y) * u.y;
                // Extend beyond both miter corners to avoid self-intersecting
                // short strips, while staying in the bond's near half.
                let reach = (2. * half_width(doc, bond))
                    .max(along(left).max(along(right)) + width)
                    .min(length * 0.5);
                let far_width = width + (end_width(doc, bond, end.id) - width) * reach / length;
                let far = atom.position.offset(u.x * reach, u.y * reach);
                parts.push((
                    *index,
                    vec![
                        left,
                        far.offset(-u.y * far_width, u.x * far_width),
                        far.offset(u.y * far_width, -u.x * far_width),
                        right,
                        atom.position,
                    ],
                ));
            }
            result.push(Junction {
                color: incident
                    .iter()
                    .map(|(_, b)| b.color)
                    .min()
                    .unwrap_or(first.color),
                parts,
                underlay,
            });
        }
        result
    }
}
#[cfg(test)]
mod tests;
