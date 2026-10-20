//! Screen-space tilt gestures share the same projection as the inspector tilt fields.
use iced::Point;
use reshiki::document::Document;

/// Whether the selection holds something that tilts: two atoms shown in 2D,
/// a shape, or a molecule shown only as a 3D model, which turns its model.
pub(crate) fn available(doc: &Document, ids: &[u64]) -> bool {
    let ids = doc.expand_abbreviation_selection(ids);
    doc.atoms
        .iter()
        .filter(|a| {
            ids.contains(&a.id)
                && (a.centroid.is_empty() || a.attachment.is_some())
                && !reshiki::models3d::hides(doc, a.id)
        })
        .take(2)
        .count()
        >= 2
        || doc.graphics.iter().any(|g| ids.contains(&g.id))
        || ids.iter().any(|id| reshiki::models3d::hides(doc, *id))
}

#[derive(Debug)]
pub(super) struct TiltDrag {
    pub ids: Vec<u64>,
    pub start: Point,
}

impl TiltDrag {
    /// Total tilt of the drag so far: 0.5° per pixel, unbounded, so a long
    /// drag keeps turning past 90° and 180°.
    pub fn angles(&self, end: Point, snap: bool) -> (f32, f32) {
        let dx = end.x - self.start.x;
        let dy = end.y - self.start.y;
        if !dx.is_finite() || !dy.is_finite() || dx.hypot(dy) < 3. {
            return (0., 0.);
        }
        let angle = |distance: f32| {
            let degrees = distance * 0.5;
            if snap {
                (degrees / 15.).round() * 15.
            } else {
                degrees
            }
        };
        (angle(-dy), angle(dx))
    }
}

/// The equivalent angle in −180…+180°, as shown in the live readout
/// (never −0, so an untouched axis reads +0°).
pub(crate) fn wrap(degrees: f32) -> f32 {
    let degrees = degrees.rem_euclid(360.) + 0.;
    if degrees > 180. {
        degrees - 360.
    } else {
        degrees
    }
}

pub(crate) fn apply(doc: &mut Document, ids: &[u64], x: f32, y: f32) {
    if !x.is_finite() || !y.is_finite() {
        return;
    }
    reshiki::projection::tilt(doc, ids, x, true);
    reshiki::projection::tilt(doc, ids, y, false);
}

/// A C–O molecule with a model in `display` (ids 0 and 1 of the result) and
/// a drawn C–N pair beside it.
#[cfg(test)]
pub(crate) fn shown(display: reshiki::models3d::Display) -> (Document, [u64; 2], [u64; 2]) {
    use reshiki::document::Point;
    use reshiki::models3d::{ForceField, Model3d, Placed, set_display};
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(-20., 0.));
    let b = doc.add_atom("O", Point::new(20., 15.));
    doc.add_bond(a, b, 1, "plain");
    let c = doc.add_atom("C", Point::new(200., 0.));
    let d = doc.add_atom("N", Point::new(240., 15.));
    doc.add_bond(c, d, 1, "plain");
    let atoms = [(a, [-0.7, 0., 0.]), (b, [0.7, 0.2, 0.])]
        .map(|(id, xyz)| Placed { id, xyz })
        .to_vec();
    let model = Model3d::new(&doc, atoms, vec![], ForceField::Mmff94s);
    doc.models.push(model);
    assert!(set_display(&mut doc, 0, display) || display == reshiki::models3d::Display::TwoD);
    (doc, [a, b], [c, d])
}

#[cfg(test)]
mod tests;
