//! Shared checked drawing indices and finite bounds geometry.
use crate::document::Point;

pub(super) fn finite(p: Point) -> Result<(), String> {
    if p.x.is_finite() && p.y.is_finite() {
        Ok(())
    } else {
        Err("Generated layout produced nonfinite coordinates".into())
    }
}

pub(super) fn at<T>(values: &[T], i: usize) -> Result<&T, String> {
    values
        .get(i)
        .ok_or_else(|| "Generated layout index is outside the drawing".into())
}

pub(super) fn at_mut<T>(values: &mut [T], i: usize) -> Result<&mut T, String> {
    values
        .get_mut(i)
        .ok_or_else(|| "Generated layout index is outside the drawing".into())
}

pub(super) fn include(bounds: &mut Option<(Point, Point)>, lo: Point, hi: Point) {
    *bounds = Some(bounds.map_or((lo, hi), |(a, b)| {
        (
            Point::new(a.x.min(lo.x), a.y.min(lo.y)),
            Point::new(b.x.max(hi.x), b.y.max(hi.y)),
        )
    }));
}
