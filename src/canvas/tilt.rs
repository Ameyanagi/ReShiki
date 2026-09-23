//! Screen-space tilt gestures share the same projection as the inspector buttons.
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
mod tests {
    use super::*;

    #[test]
    fn screen_angles_snap_and_bound_each_gesture_without_nonfinite_edits() {
        let drag = TiltDrag {
            ids: vec![],
            start: Point::new(100., 100.),
        };
        assert_eq!(drag.angles(Point::new(136., 74.), false), (13., 18.));
        assert_eq!(drag.angles(Point::new(136., 74.), true), (15., 15.));
        assert_eq!(drag.angles(Point::new(1000., -1000.), true), (75., 75.));
        assert_eq!(drag.angles(Point::new(101., 101.), false), (0., 0.));
        assert_eq!(drag.angles(Point::new(f32::NAN, 100.), false), (0., 0.));
        let mut doc = reshiki::rings::Preset::Regular.document(42., false);
        let before = doc.clone();
        let ids = doc.all_ids();
        for (x, y) in [(f32::NAN, 15.), (15., f32::INFINITY), (76., 15.)] {
            apply(&mut doc, &ids, x, y);
            assert_eq!(doc, before);
        }
    }
}
