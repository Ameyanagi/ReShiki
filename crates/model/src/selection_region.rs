//! Freeform selection shared by lasso and rectangular selection.
use crate::{
    document::{Document, Point},
    graphics::{flattened, segment_distance},
};

const BOUNDARY_TOLERANCE: f32 = 0.001;

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
        if segment_distance(p, *a, *b) < BOUNDARY_TOLERANCE {
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
    let straddles = |a: Point, b: Point, c: Point, d: Point| {
        // Clipping and affine frame round trips can put a boundary point a
        // few ulps across an edge. Use contains' distance tolerance here too:
        // a point accepted on that edge must not become a strict crossing.
        let tolerance = BOUNDARY_TOLERANCE * a.distance(b);
        let (c, d) = (cross(a, b, c), cross(a, b, d));
        (c < -tolerance && d > tolerance) || (c > tolerance && d < -tolerance)
    };
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
            .any(|(c, d)| straddles(a, b, *c, *d) && straddles(*c, *d, a, b))
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
        if g.table.is_some() {
            let footprints: Vec<_> = crate::graphics::table::cell_frames(g)
                .map(|frame| frame.corners())
                .collect();
            if !footprints.is_empty()
                && footprints
                    .iter()
                    .all(|corners| encloses(polygon, corners, true))
            {
                result.push(g.id);
            }
            continue;
        }
        if g.tlc.is_some() {
            let paths = flattened(&g.commands());
            let labels = g.tlc_labels();
            let visible: Vec<_> = paths
                .iter()
                .filter_map(|path| visible_path(g.id, path))
                .chain(labels.iter().filter_map(|label| {
                    let corners = cells.polygon(g.id, &label.corners());
                    (!corners.is_empty()).then(|| encloses(polygon, &corners, true))
                }))
                .collect();
            if !visible.is_empty() && visible.into_iter().all(|inside| inside) {
                result.push(g.id);
            }
            continue;
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enclosure_uses_the_same_boundary_tolerance_as_point_containment() {
        let polygon = [
            Point::default(),
            Point::new(100., 0.),
            Point::new(100., 100.),
            Point::new(0., 100.),
        ];
        let just_outside = f32::from_bits(100.0_f32.to_bits() + 2);
        let clipped = [Point::new(20., 10.), Point::new(just_outside, 10.)];
        assert!(clipped.iter().all(|p| contains(&polygon, *p)));
        assert!(encloses(&polygon, &clipped, false));
        assert!(!encloses(
            &polygon,
            &[clipped[0], Point::new(100.01, 10.)],
            false,
        ));
    }

    #[test]
    fn enclosure_rejects_a_path_crossing_a_visible_concave_gap() {
        let polygon = [
            Point::default(),
            Point::new(100., 0.),
            Point::new(100., 100.),
            Point::new(60., 100.),
            Point::new(60., 40.),
            Point::new(40., 40.),
            Point::new(40., 100.),
            Point::new(0., 100.),
        ];
        let crossing = [Point::new(20., 80.), Point::new(80., 80.)];
        assert!(crossing.iter().all(|p| contains(&polygon, *p)));
        assert!(!encloses(&polygon, &crossing, false));
        assert!(!encloses(&polygon, &crossing, true));
    }
}
