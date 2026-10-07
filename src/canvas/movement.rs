//! A shared constraint for the drag preview and the committed movement.
use reshiki::{
    chains::BondDrawing,
    document::{Document, Point},
};
use std::collections::{HashMap, HashSet};

/// Shift-drag keeps only the component the pointer has moved farther along.
pub(super) fn axis_locked(requested: Point) -> Point {
    if horizontal(requested) {
        Point::new(requested.x, 0.)
    } else {
        Point::new(0., requested.y)
    }
}

/// Whether a Shift-drag locks to the horizontal axis.
pub(super) fn horizontal(requested: Point) -> bool {
    requested.x.abs() >= requested.y.abs()
}

/// Shift-drag under bond constraints: a move they would push off the axis is refused.
pub(super) fn axis_delta(
    doc: &Document,
    ids: &[u64],
    requested: Point,
    drawing: BondDrawing,
) -> Point {
    let locked = axis_locked(requested);
    let d = delta(doc, ids, locked, drawing);
    let off_axis = if locked.y == 0. { d.y } else { d.x };
    if off_axis.abs() > 0.001 {
        return Point::default();
    }
    d
}

pub(super) fn delta(doc: &Document, ids: &[u64], requested: Point, drawing: BondDrawing) -> Point {
    if !requested.x.is_finite() || !requested.y.is_finite() {
        return Point::default();
    }
    if !drawing.fixed_length && !drawing.fixed_angles {
        return requested;
    }
    if !drawing.length.is_finite() || drawing.length <= 0. {
        return Point::default();
    }
    let selected: HashSet<_> = doc.expand_abbreviation_selection(ids).into_iter().collect();
    let positions: HashMap<_, _> = doc.atoms.iter().map(|a| (a.id, a.position)).collect();
    // Internal bonds move rigidly. Only bonds to fixed atoms constrain translation.
    let mut centers = Vec::new();
    for bond in &doc.bonds {
        let (moving, fixed) = match (selected.contains(&bond.a), selected.contains(&bond.b)) {
            (true, false) => (bond.a, bond.b),
            (false, true) => (bond.b, bond.a),
            _ => continue,
        };
        let Some((moving, fixed)) = positions.get(&moving).zip(positions.get(&fixed)) else {
            return Point::default();
        };
        centers.push(Point::new(fixed.x - moving.x, fixed.y - moving.y));
    }
    let Some(&center) = centers.first() else {
        return requested;
    };
    let tolerance = drawing.length * 0.0001;
    let valid = |d: Point| {
        centers.iter().all(|c| {
            let angle = (d.y - c.y).atan2(d.x - c.x);
            (!drawing.fixed_length || (c.distance(d) - drawing.length).abs() <= tolerance)
                && (!drawing.fixed_angles || (angle - drawing.angle(angle)).abs() < 0.0001)
        })
    };
    let candidate = drawing.endpoint(center, requested);
    if valid(candidate) {
        return candidate;
    }
    let mut candidates = Vec::new();
    if drawing.fixed_angles {
        for step in 0..24 {
            let angle = step as f32 * std::f32::consts::PI / 12.;
            let length = if drawing.fixed_length {
                drawing.length
            } else {
                ((requested.x - center.x) * angle.cos() + (requested.y - center.y) * angle.sin())
                    .max(0.)
            };
            candidates.push(center.offset(length * angle.cos(), length * angle.sin()));
        }
    } else if let Some(other) = centers.iter().find(|c| c.distance(center) > tolerance) {
        // Equal-radius circle intersections preserve both bonds at a ring vertex.
        let distance = center.distance(*other);
        if distance <= 2. * drawing.length {
            let middle = Point::new((center.x + other.x) / 2., (center.y + other.y) / 2.);
            let height = (drawing.length.powi(2) - (distance / 2.).powi(2))
                .max(0.)
                .sqrt();
            let x = -(other.y - center.y) / distance * height;
            let y = (other.x - center.x) / distance * height;
            candidates.extend([middle.offset(x, y), middle.offset(-x, -y)]);
        }
    }
    // Conflicting constraints keep geometry intact; Option/Alt explicitly frees it.
    candidates
        .into_iter()
        .filter(|d| valid(*d))
        .min_by(|a, b| a.distance(requested).total_cmp(&b.distance(requested)))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
