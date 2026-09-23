//! Aromatic bond order is retained in the graph; circles follow its closed rings.
use crate::{
    document::{Document, Point},
    graphics::{Graphic, GraphicKind, GraphicStyle},
};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

pub struct Circle {
    pub atoms: Vec<u64>,
    pub center: Point,
    pub radius: f32,
    pub color: [u8; 3],
    pub width_pt: f32,
    pub projected_axes: Option<[Point; 2]>,
}
impl Circle {
    pub fn graphic(&self) -> Graphic {
        let mut graphic = Graphic::dragged(
            0,
            GraphicKind::Ellipse,
            self.center.offset(-self.radius, -self.radius),
            self.center.offset(self.radius, self.radius),
            GraphicStyle {
                stroke: self.color,
                width_pt: self.width_pt,
                ..Default::default()
            },
            Default::default(),
            false,
        );
        if let Some([u, v]) = self.projected_axes {
            graphic.origin = self
                .center
                .offset(-self.radius * (u.x + v.x), -self.radius * (u.y + v.y));
            graphic.axis_x = Point::new(2. * self.radius * u.x, 2. * self.radius * u.y);
            graphic.axis_y = Point::new(2. * self.radius * v.x, 2. * self.radius * v.y);
        }
        graphic
    }
    pub fn contains_bond(&self, a: u64, b: u64) -> bool {
        self.atoms
            .iter()
            .zip(self.atoms.iter().cycle().skip(1))
            .any(|(x, y)| (*x == a && *y == b) || (*x == b && *y == a))
    }
}

pub fn circles(doc: &Document) -> Vec<Circle> {
    let mut neighbors: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
    for b in doc
        .bonds
        .iter()
        .filter(|b| b.order == 4 && doc.bond_visible(b.a, b.b))
    {
        neighbors.entry(b.a).or_default().push(b.b);
        neighbors.entry(b.b).or_default().push(b.a);
    }
    for n in neighbors.values_mut() {
        n.sort_unstable();
    }
    let mut seen = BTreeSet::new();
    let mut result = vec![];
    let mut budget = 200_000_usize;
    for b in doc.bonds.iter().filter(|b| b.order == 4) {
        if budget == 0 {
            break;
        }
        let mut previous = HashMap::from([(b.a, b.a)]);
        let mut queue = VecDeque::from([(b.a, 0)]);
        let mut found = false;
        while let Some((id, depth)) = queue.pop_front() {
            if budget == 0 {
                break;
            }
            budget -= 1;
            if depth >= 24 {
                continue;
            }
            for next in neighbors.get(&id).into_iter().flatten().copied() {
                if (id == b.a && next == b.b)
                    || (id == b.b && next == b.a)
                    || previous.contains_key(&next)
                {
                    continue;
                }
                previous.insert(next, id);
                if next == b.b {
                    found = true;
                    break;
                }
                queue.push_back((next, depth + 1));
            }
            if found {
                break;
            }
        }
        if !found {
            continue;
        }
        let mut atoms = vec![b.b];
        let mut id = b.b;
        while id != b.a {
            let Some(parent) = previous.get(&id).copied() else {
                break;
            };
            id = parent;
            atoms.push(id);
        }
        let key: BTreeSet<_> = atoms.iter().copied().collect();
        if !seen.insert(key) {
            continue;
        }
        let points: Option<Vec<_>> = atoms
            .iter()
            .map(|id| doc.atom(*id).map(|a| a.position))
            .collect();
        let Some(points) = points.filter(|p| p.len() >= 3) else {
            continue;
        };
        let center = Point::new(
            points.iter().map(|p| p.x).sum::<f32>() / points.len() as f32,
            points.iter().map(|p| p.y).sum::<f32>() / points.len() as f32,
        );
        let projected = ring_plane(doc, &atoms);
        let projected_axes = projected.as_ref().map(|(_, axes)| *axes);
        let planar_points = projected
            .as_ref()
            .map(|(points, _)| points.as_slice())
            .unwrap_or(&points);
        let planar_center = if projected.is_some() {
            Point::default()
        } else {
            center
        };
        let mut clearance = f32::INFINITY;
        let mut length = 0.;
        for (a, z) in planar_points
            .iter()
            .zip(planar_points.iter().cycle().skip(1))
        {
            let l = a.distance(*z);
            if l < 0.1 {
                clearance = 0.;
                break;
            }
            let t = (((planar_center.x - a.x) * (z.x - a.x)
                + (planar_center.y - a.y) * (z.y - a.y))
                / (l * l))
                .clamp(0., 1.);
            clearance = clearance.min(
                planar_center.distance(Point::new(a.x + t * (z.x - a.x), a.y + t * (z.y - a.y))),
            );
            length += l;
        }
        let radius =
            clearance - length / points.len() as f32 * doc.drawing_style.bond_spacing_ratio;
        if radius.is_finite() && radius > doc.drawing_style.line_width() * 2. {
            result.push(Circle {
                atoms,
                center,
                radius,
                color: b.color,
                width_pt: doc.drawing_style.line_width_pt,
                projected_axes,
            });
        }
    }
    result
}

// A rigidly tilted planar ring retains its unprojected circle as an ellipse.
fn ring_plane(doc: &Document, ids: &[u64]) -> Option<(Vec<Point>, [Point; 2])> {
    let atoms: Option<Vec<_>> = ids.iter().map(|id| doc.atom(*id)).collect();
    let atoms = atoms?;
    if !atoms.iter().any(|a| a.depth.abs() > 0.001) {
        return None;
    }
    let n = atoms.len() as f32;
    let center = [
        atoms.iter().map(|a| a.position.x).sum::<f32>() / n,
        atoms.iter().map(|a| a.position.y).sum::<f32>() / n,
        atoms.iter().map(|a| a.depth).sum::<f32>() / n,
    ];
    let [cx, cy, cz] = center;
    let points: Vec<_> = atoms
        .iter()
        .map(|a| [a.position.x - cx, a.position.y - cy, a.depth - cz])
        .collect();
    let dot = |[x, y, z]: [f32; 3], [a, b, c]: [f32; 3]| x * a + y * b + z * c;
    let cross =
        |[x, y, z]: [f32; 3], [a, b, c]: [f32; 3]| [y * c - z * b, z * a - x * c, x * b - y * a];
    let unit = |v: [f32; 3]| {
        let length = dot(v, v).sqrt();
        (length > 0.001).then(|| v.map(|n| n / length))
    };
    let u = unit(*points.first()?)?;
    let normal = points.iter().find_map(|v| unit(cross(u, *v)))?;
    if points.iter().any(|v| dot(*v, normal).abs() > 0.01) {
        return None;
    }
    let v = cross(normal, u);
    let projected = points
        .iter()
        .map(|p| Point::new(dot(*p, u), dot(*p, v)))
        .collect();
    let [ux, uy, _] = u;
    let [vx, vy, _] = v;
    Some((projected, [Point::new(ux, uy), Point::new(vx, vy)]))
}
#[cfg(test)]
mod projection_tests {
    use super::*;
    #[test]
    fn aromatic_circle_tilts_with_ring_without_losing_radius_or_connectivity() {
        let mut doc = Document::default();
        let ids = crate::editing::ring(&mut doc, Point::default(), 6, true, 5.);
        let original = circles(&doc);
        let radius = original[0].radius;
        let bonds = doc.bonds.clone();
        crate::projection::tilt(&mut doc, &ids, 60., true);
        let tilted = circles(&doc);
        assert_eq!(tilted.len(), 1);
        assert!((tilted[0].radius - radius).abs() < 0.001);
        let ellipse = tilted[0].graphic();
        // Measure the ellipse itself: drawing bounds also include the stroke
        // and Bézier control points, which are not its projected radii.
        let width = ellipse.axis_x.x.hypot(ellipse.axis_y.x);
        let height = ellipse.axis_x.y.hypot(ellipse.axis_y.y);
        assert!((height / width - 0.5).abs() < 0.001);
        assert_eq!(doc.bonds, bonds);
    }
}
