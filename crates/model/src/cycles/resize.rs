//! Resize an explicit cycle shape without replacing its retained affine frame.
use super::{center_of, outline};
use crate::document::{Document, Point};

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
    if candidate.validate().is_ok() {
        *doc = candidate;
    }
}
