//! Noncircle Tidy preserves ordered roles on equal perimeter distances.
use super::{Drawn, Outline, boxes, center_of, outline, perimeter_arcs, tidy_phase};
use crate::{
    document::{Document, Point},
    style::DEFAULT,
};

const NO_MOVE: f32 = 0.05;

fn shift(doc: &mut Document, ids: &[u64], from: Point, to: Point) -> bool {
    if from.distance(to) <= NO_MOVE {
        return false;
    }
    let ids = crate::molecule_names::include_annotations(doc, ids);
    doc.translate(&ids, to.x - from.x, to.y - from.y);
    true
}

pub(super) fn tidy(doc: &mut Document, id: u64) -> bool {
    let Some(g) = doc.graphics.iter().find(|g| g.id == id) else {
        return false;
    };
    let Some(cycle) = g.cycle.as_deref().cloned() else {
        return false;
    };
    let Some(perimeter) = outline(g) else {
        return false;
    };
    let n = cycle.intermediates.len();
    if n == 0 {
        return false;
    }
    let bond = doc.drawing_style.bond_length_world;
    if !(bond.is_finite() && bond > 0.) {
        return false;
    }
    let dir = if g.cycle_counterclockwise() { -1. } else { 1. };
    let all = boxes(doc, &cycle);
    let centers: Vec<_> = all.iter().take(n).map(|b| b.map(center_of)).collect();
    let Some(phase) = tidy_phase::phase(&perimeter, &centers, dir) else {
        return false;
    };
    // Plan every intermediate before mutating, preserving missing part slots.
    let targets: Option<Vec<_>> = (0..n)
        .map(|i| perimeter.point(phase + dir * perimeter.length() * i as f64 / n as f64))
        .collect();
    let Some(targets) = targets else {
        return false;
    };
    let mut moved = false;
    for ((ids, from), to) in cycle.intermediates.iter().zip(&centers).zip(targets) {
        if let Some(from) = from {
            moved |= shift(doc, ids, *from, to);
        }
    }
    let Some(g) = doc.graphics.iter().find(|g| g.id == id) else {
        return moved;
    };
    let gap = DEFAULT.world(cycle.gap_pt) + bond;
    let mut side = n;
    let mut moves = vec![];
    perimeter_arcs::resolve(doc, g, &perimeter, &mut Drawn::default(), |s, resolved| {
        let Some(step) = cycle.steps.get(s) else {
            return;
        };
        for (parts, lo, hi) in [(&step.reagents, 0.2, 0.4), (&step.products, 0.6, 0.8)] {
            for (i, ids) in parts.iter().enumerate() {
                let b = all.get(side).copied().flatten();
                side += 1;
                let Some(b) = b else {
                    continue;
                };
                let fraction = if parts.len() == 1 {
                    (lo + hi) * 0.5
                } else {
                    lo + (hi - lo) * i as f64 / (parts.len() - 1) as f64
                };
                let from = center_of(b);
                let Some(inside) = perimeter.contains(from) else {
                    continue;
                };
                let fallback = resolved.first + resolved.span * fraction;
                let lanes = [
                    resolved.lanes.first().and_then(Option::as_ref),
                    resolved.lanes.get(1).and_then(Option::as_ref),
                ];
                let frame =
                    perimeter_arcs::Frame::at(lanes, &perimeter, fallback, fraction, dir as f32);
                let (w, h) = (b.1.x - b.0.x, b.1.y - b.0.y);
                let to = frame
                    .and_then(|frame| place(frame, &perimeter, (w, h), gap, bond, inside))
                    .or_else(|| {
                        // Hidden steps retain virtual shafts. Only a missing or
                        // unreachable painted frame needs the actual outline.
                        let frame = perimeter_arcs::Frame::at(
                            [None, None],
                            &perimeter,
                            fallback,
                            fraction,
                            dir as f32,
                        )?;
                        place(frame, &perimeter, (w, h), gap, bond, inside)
                    });
                if let Some(to) = to {
                    moves.push((ids, from, to));
                }
            }
        }
    });
    for (ids, from, to) in moves {
        moved |= shift(doc, ids, from, to);
    }
    moved
}

pub(super) fn place(
    frame: perimeter_arcs::Frame,
    outline: &Outline,
    (w, h): (f32, f32),
    gap: f32,
    bond: f32,
    inside: bool,
) -> Option<Point> {
    // Fixed candidates depend on role/ink and the final shaft, never on the
    // prior distance from it. A second Tidy therefore picks the same position.
    for depth in [1., 1.8, 3., 5., 8., 0.5, 0.25, 0.125, 0.0625] {
        for offset in frame.offsets(w, h, gap, inside) {
            let sign = if inside { -1. } else { 1. };
            let mut offset = Point::new(offset.x * sign * depth, offset.y * sign * depth);
            if !inside {
                // As for Circle, large ink may not fit its full clearance.
                // Keep the center inside the membership reach for later edits.
                let distance = offset.distance(Point::default());
                let scale = (3.5 * bond / distance).min(1.);
                offset.x *= scale;
                offset.y *= scale;
            }
            let candidate = frame.point.offset(offset.x, offset.y);
            let fits_inside = || {
                let padding = (gap - bond).max(0.);
                let (x, y) = (w * 0.5 + padding, h * 0.5 + padding);
                [(-x, -y), (x, -y), (x, y), (-x, y)]
                    .into_iter()
                    .all(|(x, y)| outline.contains(candidate.offset(x, y)) == Some(true))
            };
            if candidate.is_finite()
                && outline.contains(candidate) == Some(inside)
                // Convex native outlines contain the whole box exactly when
                // all four corners fit. A center near a turn is insufficient.
                && (!inside || fits_inside())
            {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(test)]
#[path = "tidy_shape_tests.rs"]
mod tests;
