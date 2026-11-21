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
    let cells = crate::scene::table::Layout::new(doc);
    let visible_path = |id, points: &[Point]| -> Option<bool> {
        if points.len() > 2 && points.first() == points.last() {
            let clipped = cells.polygon(id, points);
            return (!clipped.is_empty()).then(|| encloses(polygon, &clipped, true));
        }
        let segments: Vec<_> = points
            .windows(2)
            .filter_map(|pair| {
                let &[a, b] = pair else { return None };
                cells.segment(id, a, b)
            })
            .collect();
        (!segments.is_empty()).then(|| {
            segments
                .iter()
                .all(|(a, b)| encloses(polygon, &[*a, *b], false))
        })
    };
    let mut result: Vec<_> = doc
        .atoms
        .iter()
        .filter(|a| {
            doc.atom_drawn(a.id)
                && cells.contains(a.id, a.position)
                && contains(polygon, a.position)
        })
        .map(|a| a.id)
        .collect();
    // A 3D model inside the region selects its molecule.
    for model in &doc.models {
        let ids = model.linked_ids(doc);
        if model.display != crate::models3d::Display::TwoD
            && crate::models3d::frame_corners(doc, model).is_some_and(|corners| {
                let corners = ids
                    .first()
                    .map_or_else(|| corners.to_vec(), |id| cells.polygon(*id, &corners));
                !corners.is_empty() && encloses(polygon, &corners, true)
            })
        {
            for id in ids {
                if !result.contains(&id) {
                    result.push(id);
                }
            }
        }
    }
    for a in &doc.annotations {
        let corners = cells.polygon(a.id, &cells.annotation(a).corners());
        if !corners.is_empty() && encloses(polygon, &corners, true) {
            result.push(a.id);
        }
    }
    for a in &doc.arrows {
        let visible: Vec<_> = a
            .paths()
            .iter()
            .flat_map(|part| flattened(&part.commands))
            .filter_map(|points| visible_path(a.id, &points))
            .collect();
        if !visible.is_empty() && visible.into_iter().all(|inside| inside) {
            result.push(a.id);
        }
    }
    for g in &doc.graphics {
        let commands = g.commands();
        let fill_only = g.is_fill_only_path();
        let paths = if fill_only {
            crate::graphics::fill_boundaries(&commands)
        } else {
            flattened(&commands)
        };
        let visible: Vec<_> = paths
            .iter()
            .filter_map(|path| visible_path(g.id, path))
            .collect();
        if !visible.is_empty() && visible.into_iter().all(|inside| inside) {
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
