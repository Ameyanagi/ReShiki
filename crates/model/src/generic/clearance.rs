//! Conservative separating-axis clearance certificates.
use crate::document::Point;

// A successful separating-axis test is a conservative clearance certificate
// for padded convex footprints. Touching/collinear/uncertain pairs refuse;
// there is no positive-tolerance strip that could admit a crossing. Extra
// endpoint axes improve capsule clearance without a geometry solver/retry.
#[derive(Clone, Copy)]
pub(super) struct Footprint {
    points: [Point; 4],
    count: usize,
    radius: f64,
}

impl Footprint {
    pub(super) fn point(p: Point, radius: f64) -> Self {
        Self {
            points: [p; 4],
            count: 1,
            radius,
        }
    }
    pub(super) fn segment(a: Point, b: Point, radius: f64) -> Self {
        Self {
            points: [a, b, b, a],
            count: 2,
            radius,
        }
    }
    pub(super) fn rectangle((lo, hi): (Point, Point), radius: f64) -> Self {
        Self {
            points: [lo, Point::new(hi.x, lo.y), hi, Point::new(lo.x, hi.y)],
            count: 4,
            radius,
        }
    }
    pub(super) fn separated(self, other: Self) -> bool {
        let (Some(points), Some(other_points)) = (
            self.points.get(..self.count),
            other.points.get(..other.count),
        ) else {
            return false;
        };
        let Some(&origin) = points.first() else {
            return false;
        };
        if other_points.is_empty() {
            return false;
        }
        let axis = |x: f64, y: f64| {
            let length = x.hypot(y);
            if !length.is_finite() || length == 0. {
                return false;
            }
            let project = |points: &[Point]| {
                let mut lo = f64::INFINITY;
                let mut hi = f64::NEG_INFINITY;
                let mut error = 0_f64;
                for p in points {
                    let px = (f64::from(p.x) - f64::from(origin.x)) * x;
                    let py = (f64::from(p.y) - f64::from(origin.y)) * y;
                    lo = lo.min(px + py);
                    hi = hi.max(px + py);
                    error = error.max(
                        (f64::from(p.x).abs() + f64::from(origin.x).abs()) * x.abs()
                            + (f64::from(p.y).abs() + f64::from(origin.y).abs()) * y.abs()
                            + px.abs()
                            + py.abs(),
                    );
                }
                (lo, hi, error)
            };
            let (alo, ahi, ae) = project(points);
            let (blo, bhi, be) = project(other_points);
            let pad = (self.radius + other.radius) * length;
            let error = (ae + be + pad.abs()) * (f64::EPSILON * 64.);
            let gap = (blo - ahi).max(alo - bhi);
            pad.is_finite() && gap > pad + error
        };
        if axis(1., 0.) || axis(0., 1.) {
            return true;
        }
        for points in [points, other_points] {
            for (a, b) in points.iter().zip(points.iter().cycle().skip(1)) {
                if axis(
                    f64::from(a.y) - f64::from(b.y),
                    f64::from(b.x) - f64::from(a.x),
                ) {
                    return true;
                }
            }
        }
        points.iter().any(|a| {
            other_points.iter().any(|b| {
                axis(
                    f64::from(a.x) - f64::from(b.x),
                    f64::from(a.y) - f64::from(b.y),
                )
            })
        })
    }
}

#[cfg(test)]
mod tests;
