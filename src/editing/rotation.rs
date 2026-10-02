//! Reference sites that follow rotation, unlike axis-aligned drawing bounds.
use crate::{
    document::{Document, Point},
    graphics::{Graphic, GraphicKind, PathCommand},
};
use std::collections::HashSet;

/// The mean of selected visible atom sites and one stable site per drawing object.
///
/// Captions contribute their upright text's insertion point, arrows their endpoint
/// midpoint, and graphics a point in their retained affine frame. A rotation about
/// this mean leaves it fixed, so recomputing it for the next command does not move
/// the pivot. Hidden abbreviation members move with their visible anchor but do
/// not add weight. Derived drawing centroids likewise contribute no extra site.
/// Bounds-based layout and explicit-pivot transforms use their own centers.
pub fn center(doc: &Document, ids: &[u64]) -> Option<Point> {
    let selected: HashSet<_> = doc.expand_abbreviation_selection(ids).into_iter().collect();
    let (mut x, mut y, mut count) = (0_f64, 0_f64, 0_u64);
    let mut add = |px: f64, py: f64| {
        x += px;
        y += py;
        count += 1;
    };
    // Document order makes duplicate/reordered selection IDs immaterial.
    for atom in &doc.atoms {
        if selected.contains(&atom.id)
            && doc.atom_visible(atom.id)
            && (atom.centroid.is_empty() || atom.attachment.is_some())
        {
            add(f64::from(atom.position.x), f64::from(atom.position.y));
        }
    }
    for caption in &doc.annotations {
        if selected.contains(&caption.id) {
            add(f64::from(caption.position.x), f64::from(caption.position.y));
        }
    }
    for arrow in &doc.arrows {
        if selected.contains(&arrow.id) {
            add(
                (f64::from(arrow.start.x) + f64::from(arrow.end.x)) * 0.5,
                (f64::from(arrow.start.y) + f64::from(arrow.end.y)) * 0.5,
            );
        }
    }
    for graphic in &doc.graphics {
        if selected.contains(&graphic.id) {
            let (x, y) = graphic_anchor(graphic);
            add(x, y);
        }
    }
    if count == 0 {
        return None;
    }
    let point = Point::new((x / count as f64) as f32, (y / count as f64) as f32);
    (point.x.is_finite() && point.y.is_finite()).then_some(point)
}

fn graphic_anchor(graphic: &Graphic) -> (f64, f64) {
    let (x, y) = match graphic.kind {
        GraphicKind::Symbol(_) | GraphicKind::Orbital(_) => (0., 0.),
        GraphicKind::Line | GraphicKind::Curve => (0.5, 0.),
        // Legacy half arcs have a half-height frame; both forms use the ellipse center.
        GraphicKind::Arc if graphic.arc.is_none() => (0.5, 1.),
        GraphicKind::Path => {
            // These are stored local coordinates, not bounds rebuilt in world axes.
            // They remain fixed while map_positions transforms the affine frame.
            let bounds = graphic.path.iter().flat_map(PathCommand::points).fold(
                None,
                |bounds: Option<(Point, Point)>, p| {
                    Some(match bounds {
                        None => (p, p),
                        Some((lo, hi)) => (
                            Point::new(lo.x.min(p.x), lo.y.min(p.y)),
                            Point::new(hi.x.max(p.x), hi.y.max(p.y)),
                        ),
                    })
                },
            );
            bounds
                .map(|(lo, hi)| {
                    (
                        (f64::from(lo.x) + f64::from(hi.x)) * 0.5,
                        (f64::from(lo.y) + f64::from(hi.y)) * 0.5,
                    )
                })
                .unwrap_or_default()
        }
        GraphicKind::Picture
        | GraphicKind::Rectangle
        | GraphicKind::RoundedRectangle
        | GraphicKind::Ellipse
        | GraphicKind::Arc
        | GraphicKind::Brackets
        | GraphicKind::Parentheses
        | GraphicKind::Braces => (0.5, 0.5),
    };
    (
        f64::from(graphic.origin.x)
            + f64::from(graphic.axis_x.x) * x
            + f64::from(graphic.axis_y.x) * y,
        f64::from(graphic.origin.y)
            + f64::from(graphic.axis_x.y) * x
            + f64::from(graphic.axis_y.y) * y,
    )
}
