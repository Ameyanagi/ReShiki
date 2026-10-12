//! Closed-polygon helpers shared by scene ink, rear opacity and label clearance.
use crate::document::Point;

/// Twice the signed area relative to the first vertex, in f64.
pub(crate) fn signed_area(points: &[Point]) -> f64 {
    let Some(origin) = points.first() else {
        return 0.;
    };
    closed_edges(points)
        .map(|(a, b)| {
            (f64::from(a.x) - f64::from(origin.x)) * (f64::from(b.y) - f64::from(origin.y))
                - (f64::from(a.y) - f64::from(origin.y)) * (f64::from(b.x) - f64::from(origin.x))
        })
        .sum()
}

/// Each edge of a closed contour, last vertex back to the first; empty for an empty slice.
pub(crate) fn closed_edges<T: Copy>(points: &[T]) -> impl Iterator<Item = (T, T)> + '_ {
    points
        .iter()
        .copied()
        .zip(points.iter().copied().cycle().skip(1))
        .take(points.len())
}

#[cfg(test)]
mod tests;
