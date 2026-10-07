//! Screen-space tilt gestures share the same projection as the inspector tilt fields.
use iced::Point;
use reshiki::document::Document;

pub(crate) fn available(doc: &Document, ids: &[u64]) -> bool {
    let ids = doc.expand_abbreviation_selection(ids);
    doc.atoms
        .iter()
        .filter(|a| ids.contains(&a.id) && (a.centroid.is_empty() || a.attachment.is_some()))
        .take(2)
        .count()
        >= 2
        || doc.graphics.iter().any(|g| ids.contains(&g.id))
}

#[derive(Debug)]
pub(super) struct TiltDrag {
    pub ids: Vec<u64>,
    pub start: Point,
}

impl TiltDrag {
    pub fn angles(&self, end: Point, snap: bool) -> (f32, f32) {
        let dx = end.x - self.start.x;
        let dy = end.y - self.start.y;
        if !dx.is_finite() || !dy.is_finite() || dx.hypot(dy) < 3. {
            return (0., 0.);
        }
        let angle = |distance: f32| {
            let degrees = distance * 0.5;
            let degrees = if snap {
                (degrees / 15.).round() * 15.
            } else {
                degrees
            };
            degrees.clamp(-75., 75.)
        };
        (angle(-dy), angle(dx))
    }
}

pub(crate) fn apply(doc: &mut Document, ids: &[u64], x: f32, y: f32) {
    if !x.is_finite() || !y.is_finite() || x.abs() > 75. || y.abs() > 75. {
        return;
    }
    reshiki::projection::tilt(doc, ids, x, true);
    reshiki::projection::tilt(doc, ids, y, false);
}

#[cfg(test)]
mod tests;
