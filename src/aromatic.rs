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
}
impl Circle {
    pub fn graphic(&self) -> Graphic {
        Graphic::dragged(
            0,
            GraphicKind::Ellipse,
            self.center.offset(-self.radius, -self.radius),
            self.center.offset(self.radius, self.radius),
            GraphicStyle {
                stroke: self.color,
                ..Default::default()
            },
            Default::default(),
            false,
        )
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
        let mut clearance = f32::INFINITY;
        let mut length = 0.;
        for (a, z) in points.iter().zip(points.iter().cycle().skip(1)) {
            let l = a.distance(*z);
            if l < 0.1 {
                clearance = 0.;
                break;
            }
            let t = (((center.x - a.x) * (z.x - a.x) + (center.y - a.y) * (z.y - a.y)) / (l * l))
                .clamp(0., 1.);
            clearance = clearance
                .min(center.distance(Point::new(a.x + t * (z.x - a.x), a.y + t * (z.y - a.y))));
            length += l;
        }
        let radius =
            clearance - length / points.len() as f32 * crate::style::DEFAULT.bond_spacing_ratio;
        if radius.is_finite() && radius > crate::style::DEFAULT.line_width() * 2. {
            result.push(Circle {
                atoms,
                center,
                radius,
                color: b.color,
            });
        }
    }
    result
}
