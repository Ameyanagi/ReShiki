//! Freeform selection shared by lasso and rectangular selection.
use crate::{
    document::{Document, Point},
    graphics::{flattened, segment_distance},
};

pub fn contains(polygon: &[Point], p: Point) -> bool {
    if polygon.len() < 3 {
        return false;
    }
    let mut inside = false;
    for (a, b) in polygon
        .iter()
        .zip(polygon.iter().cycle().skip(1))
        .take(polygon.len())
    {
        if segment_distance(p, *a, *b) < 0.001 {
            return true;
        }
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
    }
    inside
}
fn encloses(polygon: &[Point], points: &[Point], closed: bool) -> bool {
    if !points.iter().all(|p| contains(polygon, *p)) {
        return false;
    }
    let cross =
        |a: Point, b: Point, c: Point| (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    let edges = points
        .windows(2)
        .filter_map(|p| match p {
            [a, b] => Some((*a, *b)),
            _ => None,
        })
        .chain(
            points
                .last()
                .zip(points.first())
                .filter(|_| closed)
                .map(|(a, b)| (*a, *b)),
        );
    !edges.into_iter().any(|(a, b)| {
        polygon
            .iter()
            .zip(polygon.iter().cycle().skip(1))
            .take(polygon.len())
            .any(|(c, d)| {
                cross(a, b, *c) * cross(a, b, *d) < -0.0001
                    && cross(*c, *d, a) * cross(*c, *d, b) < -0.0001
            })
    })
}
pub fn objects(doc: &Document, polygon: &[Point]) -> Vec<u64> {
    if polygon.len() < 3 {
        return vec![];
    }
    let mut result: Vec<_> = doc
        .atoms
        .iter()
        .filter(|a| doc.atom_visible(a.id) && contains(polygon, a.position))
        .map(|a| a.id)
        .collect();
    for a in &doc.annotations {
        let (w, h) = a.size();
        let p = a.position;
        if encloses(
            polygon,
            &[p, p.offset(w, 0.), p.offset(w, h), p.offset(0., h)],
            true,
        ) {
            result.push(a.id);
        }
    }
    for a in &doc.arrows {
        if a.paths().iter().all(|part| {
            flattened(&part.commands)
                .iter()
                .all(|points| encloses(polygon, points, false))
        }) {
            result.push(a.id);
        }
    }
    for g in &doc.graphics {
        if flattened(&g.commands())
            .iter()
            .all(|path| !path.is_empty() && encloses(polygon, path, false))
        {
            result.push(g.id);
        }
    }
    result
}

pub fn combine(existing: &[u64], hits: &[u64], add: bool, subtract: bool) -> Vec<u64> {
    if subtract {
        return existing
            .iter()
            .filter(|id| !hits.contains(id))
            .copied()
            .collect();
    }
    if !add {
        return hits.to_vec();
    }
    let mut ids = existing.to_vec();
    for id in hits {
        if !ids.contains(id) {
            ids.push(*id);
        }
    }
    ids
}
