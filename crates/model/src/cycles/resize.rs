//! Resize an explicit cycle shape without replacing its retained affine frame.
use super::{Shape, center_of, outline};
use crate::{
    document::{Document, Point},
    graphics::Graphic,
};

#[cfg(test)]
#[path = "resize_tests.rs"]
mod tests;

pub(super) fn noncircle(doc: &mut Document, id: u64, radius: f32) {
    if !(radius.is_finite() && radius > 0.) {
        return;
    }
    let Some(g) = doc.graphics.iter().find(|g| g.id == id) else {
        return;
    };
    let (center, reported) = g.cycle_circle();
    if radius == reported || !center.is_finite() || outline(g).is_none() {
        return;
    }
    // Use the retained axes, not an axis-aligned box or a normalized angle.
    // f64 also lets a tiny valid frame be enlarged without overflowing its
    // scale ratio before that ratio is applied to the small coordinates.
    let norm = |p: Point| f64::from(p.x).hypot(f64::from(p.y));
    let mean = 0.25 * (norm(g.axis_x) + norm(g.axis_y));
    let scale = f64::from(radius) / mean;
    let vector = |p: Point| {
        Point::new(
            (f64::from(p.x) * scale) as f32,
            (f64::from(p.y) * scale) as f32,
        )
    };
    let mut frame = g.clone();
    frame.axis_x = vector(g.axis_x);
    frame.axis_y = vector(g.axis_y);
    // Anchor the converted axes to the retained center. Scaling the old
    // origin offset would amplify the center's earlier f32 rounding.
    frame.origin = Point::new(
        (f64::from(center.x) - 0.5 * (f64::from(frame.axis_x.x) + f64::from(frame.axis_y.x)))
            as f32,
        (f64::from(center.y) - 0.5 * (f64::from(frame.axis_x.y) + f64::from(frame.axis_y.y)))
            as f32,
    );
    let (next_center, next_radius) = frame.cycle_circle();
    let determinant =
        |x: Point, y: Point| f64::from(x.x) * f64::from(y.y) - f64::from(x.y) * f64::from(y.x);
    if !(next_center.is_finite() && next_radius.is_finite() && next_radius > 0.)
        || outline(&frame).is_none()
        || determinant(frame.axis_x, frame.axis_y).is_sign_negative()
            != determinant(g.axis_x, g.axis_y).is_sign_negative()
        || frame.cycle_counterclockwise() != g.cycle_counterclockwise()
    {
        return;
    }
    let parts: Vec<_> = g
        .cycle
        .iter()
        .flat_map(|c| c.parts().map(|(_, ids)| ids.clone()))
        .collect();
    let bounds = crate::scene::selections_bounds(doc, &parts);
    let mut moves = vec![];
    for (part, bounds) in parts.iter().zip(bounds) {
        let Some(bounds) = bounds else {
            continue;
        };
        let p = center_of(bounds);
        let delta = Point::new(
            ((f64::from(p.x) - f64::from(center.x)) * (scale - 1.)) as f32,
            ((f64::from(p.y) - f64::from(center.y)) * (scale - 1.)) as f32,
        );
        if !delta.is_finite() {
            return;
        }
        // Keep each part's own ink as the position reference. Linked name and
        // property captions follow that translation rather than shifting it.
        moves.push((crate::molecule_names::include_annotations(doc, part), delta));
    }
    let _ = apply(doc, id, &frame, moves);
}

/// Width and height are the lengths of the retained local axes, not the
/// drawing's axis-aligned bounds. Only part centers undergo the affine map.
pub(super) fn dimensions(doc: &mut Document, id: u64, width: f32, height: f32) -> bool {
    if !(width.is_finite() && height.is_finite() && width > 0. && height > 0.) {
        return false;
    }
    let Some(g) = doc.graphics.iter().find(|g| g.id == id) else {
        return false;
    };
    if g.cycle.as_ref().is_none_or(|c| c.shape == Shape::Circle) || outline(g).is_none() {
        return false;
    }
    let center = g.cycle_circle().0;
    let norm = |p: Point| f64::from(p.x).hypot(f64::from(p.y));
    let (old_width, old_height) = (norm(g.axis_x), norm(g.axis_y));
    // Preserve an untouched axis exactly, including when its f64 norm rounds
    // to the dimension displayed by a native f32 control.
    let scale = |requested: f32, old: f64| {
        if requested == old as f32 {
            1.
        } else {
            f64::from(requested) / old
        }
    };
    let (sx, sy) = (scale(width, old_width), scale(height, old_height));
    if sx == 1. && sy == 1. {
        return false;
    }
    let scaled = |p: Point, scale| {
        Point::new(
            (f64::from(p.x) * scale) as f32,
            (f64::from(p.y) * scale) as f32,
        )
    };
    let mut frame = g.clone();
    frame.axis_x = scaled(g.axis_x, sx);
    frame.axis_y = scaled(g.axis_y, sy);
    frame.origin = Point::new(
        (f64::from(center.x) - 0.5 * (f64::from(frame.axis_x.x) + f64::from(frame.axis_y.x)))
            as f32,
        (f64::from(center.y) - 0.5 * (f64::from(frame.axis_x.y) + f64::from(frame.axis_y.y)))
            as f32,
    );
    let determinant =
        |x: Point, y: Point| f64::from(x.x) * f64::from(y.y) - f64::from(x.y) * f64::from(y.x);
    let det = determinant(g.axis_x, g.axis_y);
    let (next_center, next_radius) = frame.cycle_circle();
    let Some(perimeter) = outline(&frame) else {
        return false;
    };
    if !(center.is_finite()
        && next_center.is_finite()
        && next_radius.is_finite()
        && next_radius > 0.)
        // Finite independent axes can disappear when native f32 commands add
        // them to a translated origin. A painted convex loop must still have
        // its retained center strictly inside, rather than on a collapsed edge.
        || perimeter.contains(next_center) != Some(true)
        || determinant(frame.axis_x, frame.axis_y).is_sign_negative() != det.is_sign_negative()
        || frame.cycle_counterclockwise() != g.cycle_counterclockwise()
    {
        return false;
    }
    let parts: Vec<_> = g
        .cycle
        .iter()
        .flat_map(|c| c.parts().map(|(_, ids)| ids.clone()))
        .collect();
    let bounds = crate::scene::selections_bounds(doc, &parts);
    let mut moves = vec![];
    for (part, bounds) in parts.iter().zip(bounds) {
        let Some(bounds) = bounds else {
            continue;
        };
        let p = center_of(bounds);
        let (px, py) = (
            f64::from(p.x) - f64::from(center.x),
            f64::from(p.y) - f64::from(center.y),
        );
        let delta = if sx == sy {
            // Uniform resizing does not need an inverse, even on a frame
            // whose independent axes are nearly parallel.
            Point::new((px * (sx - 1.)) as f32, (py * (sy - 1.)) as f32)
        } else {
            let u = (px * f64::from(g.axis_y.y) - py * f64::from(g.axis_y.x)) / det;
            let v = (f64::from(g.axis_x.x) * py - f64::from(g.axis_x.y) * px) / det;
            Point::new(
                (u * (f64::from(frame.axis_x.x) - f64::from(g.axis_x.x))
                    + v * (f64::from(frame.axis_y.x) - f64::from(g.axis_y.x)))
                    as f32,
                (u * (f64::from(frame.axis_x.y) - f64::from(g.axis_x.y))
                    + v * (f64::from(frame.axis_y.y) - f64::from(g.axis_y.y)))
                    as f32,
            )
        };
        if !delta.is_finite() {
            return false;
        }
        let ids = crate::molecule_names::include_annotations(doc, part);
        if !rigid_bonds(doc, &ids, delta) {
            return false;
        }
        moves.push((ids, delta));
    }
    apply(doc, id, &frame, moves)
}

pub(super) fn rigid_bonds(doc: &Document, ids: &[u64], delta: Point) -> bool {
    let ids = crate::shape_text::with_holders(doc, ids);
    let ids: std::collections::HashSet<_> = doc
        .expand_abbreviation_selection(&ids)
        .into_iter()
        .collect();
    let positions: std::collections::HashMap<_, _> = doc
        .atoms
        .iter()
        .filter(|a| ids.contains(&a.id))
        .map(|a| (a.id, (a.position, a.position.offset(delta.x, delta.y))))
        .collect();
    if positions.values().any(|(_, new)| !new.is_finite()) {
        return false;
    }
    doc.bonds.iter().all(|bond| {
        let (Some(&(a, next_a)), Some(&(b, next_b))) =
            (positions.get(&bond.a), positions.get(&bond.b))
        else {
            return true;
        };
        let vector = |a: Point, b: Point| {
            (
                f64::from(b.x) - f64::from(a.x),
                f64::from(b.y) - f64::from(a.y),
            )
        };
        let (x, y) = vector(a, b);
        let (next_x, next_y) = vector(next_a, next_b);
        let length = x.hypot(y);
        let coordinates: f64 = [a, b, delta, delta]
            .into_iter()
            .map(|p| f64::from(p.x).abs() + f64::from(p.y).abs())
            .sum();
        // Allow ordinary f32 addition roundoff, capped at 0.1% of this bond's
        // length. A huge-coordinate error bound must not excuse a collapsed,
        // rotated or materially stretched intra-part bond vector.
        let tolerance = (4. * f64::from(f32::EPSILON) * coordinates).min(length * 0.001)
            + 16. * f64::EPSILON * length;
        (next_x - x).hypot(next_y - y) <= tolerance
    })
}

fn apply(doc: &mut Document, id: u64, frame: &Graphic, moves: Vec<(Vec<u64>, Point)>) -> bool {
    // Translation also expands abbreviations/shape-text holders and updates
    // attached arrows. Validate that complete proposal before changing the
    // source, so an unrepresentable later part cannot leave a partial resize.
    let mut candidate = doc.clone();
    for (ids, delta) in moves {
        candidate.translate(&ids, delta.x, delta.y);
    }
    if let Some(g) = candidate.graphics.iter_mut().find(|g| g.id == id) {
        g.origin = frame.origin;
        g.axis_x = frame.axis_x;
        g.axis_y = frame.axis_y;
    }
    // Fitted source families must be rebased on this complete resize
    // candidate before ordinary validation checks their nominal children.
    if super::perimeter_arcs::fits::edited(doc, &mut candidate, id, None).is_err() {
        return false;
    }
    if candidate.validate().is_ok() {
        let changed = candidate != *doc;
        *doc = candidate;
        changed
    } else {
        false
    }
}
