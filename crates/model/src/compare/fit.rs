//! The proper 2D rigid fit (Kabsch): the rotation and translation that best
//! carry one set of points onto another, with what is left over. No
//! reflection and no scale.

/// A point as `(x, y)` in drawing units.
pub type Xy = (f64, f64);

/// `to ≈ R · from + t`, with the residuals left after the fit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fit {
    pub cos: f64,
    pub sin: f64,
    pub dx: f64,
    pub dy: f64,
    /// Root mean square and largest distance left after the fit.
    pub rms: f64,
    pub max: f64,
}

impl Fit {
    pub const IDENTITY: Self = Self {
        cos: 1.,
        sin: 0.,
        dx: 0.,
        dy: 0.,
        rms: 0.,
        max: 0.,
    };

    pub fn apply(&self, (x, y): Xy) -> Xy {
        (
            self.cos * x - self.sin * y + self.dx,
            self.sin * x + self.cos * y + self.dy,
        )
    }

    /// The rotation alone, for directions such as a graphic's axes.
    pub fn rotate(&self, (x, y): Xy) -> Xy {
        (self.cos * x - self.sin * y, self.sin * x + self.cos * y)
    }

    /// Clockwise on screen is positive, as drawing y grows downward.
    pub fn degrees(&self) -> f64 {
        self.sin.atan2(self.cos).to_degrees()
    }

    /// The rigid fit of `pairs` (from, to). One point, or points that sit
    /// on one spot, fit by translation alone. `None` without points.
    pub fn rigid(pairs: &[(Xy, Xy)]) -> Option<Self> {
        let (from, to) = centroids(pairs)?;
        let (mut dot, mut cross) = (0., 0.);
        for ((ax, ay), (bx, by)) in pairs {
            let (ax, ay, bx, by) = (ax - from.0, ay - from.1, bx - to.0, by - to.1);
            dot += ax * bx + ay * by;
            cross += ax * by - ay * bx;
        }
        let length = dot.hypot(cross);
        let (cos, sin) = if length > 1e-9 {
            (dot / length, cross / length)
        } else {
            (1., 0.)
        };
        Some(Self::through(cos, sin, from, to).with_residuals(pairs))
    }

    /// Translation alone, centroid onto centroid.
    pub fn translation(pairs: &[(Xy, Xy)]) -> Option<Self> {
        let (from, to) = centroids(pairs)?;
        Some(Self::through(1., 0., from, to).with_residuals(pairs))
    }

    fn through(cos: f64, sin: f64, from: Xy, to: Xy) -> Self {
        let mut fit = Self {
            cos,
            sin,
            ..Self::IDENTITY
        };
        let turned = fit.rotate(from);
        fit.dx = to.0 - turned.0;
        fit.dy = to.1 - turned.1;
        fit
    }

    /// The same transform with the residuals of `pairs`.
    pub fn with_residuals(mut self, pairs: &[(Xy, Xy)]) -> Self {
        let (rms, max) = self.residuals(pairs);
        self.rms = rms;
        self.max = max;
        self
    }

    /// Root mean square and largest distance between each moved `from` and
    /// its `to`.
    pub fn residuals(&self, pairs: &[(Xy, Xy)]) -> (f64, f64) {
        if pairs.is_empty() {
            return (0., 0.);
        }
        let (mut sum, mut max) = (0_f64, 0_f64);
        for (from, to) in pairs {
            let moved = self.apply(*from);
            let distance = (moved.0 - to.0).hypot(moved.1 - to.1);
            sum += distance * distance;
            max = max.max(distance);
        }
        ((sum / pairs.len() as f64).sqrt(), max)
    }

    /// The furthest any of `points` goes under this transform.
    pub fn reach(&self, points: impl IntoIterator<Item = Xy>) -> f64 {
        points
            .into_iter()
            .map(|p| {
                let moved = self.apply(p);
                (moved.0 - p.0).hypot(moved.1 - p.1)
            })
            .fold(0., f64::max)
    }
}

fn centroids(pairs: &[(Xy, Xy)]) -> Option<(Xy, Xy)> {
    if pairs.is_empty() {
        return None;
    }
    let n = pairs.len() as f64;
    let (mut from, mut to) = ((0., 0.), (0., 0.));
    for ((ax, ay), (bx, by)) in pairs {
        from = (from.0 + ax, from.1 + ay);
        to = (to.0 + bx, to.1 + by);
    }
    Some(((from.0 / n, from.1 / n), (to.0 / n, to.1 / n)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(points: &[Xy], degrees: f64, dx: f64, dy: f64) -> Vec<(Xy, Xy)> {
        let (sin, cos) = degrees.to_radians().sin_cos();
        points
            .iter()
            .map(|&(x, y)| ((x, y), (cos * x - sin * y + dx, sin * x + cos * y + dy)))
            .collect()
    }

    const SHAPE: [Xy; 4] = [(0., 0.), (30., 0.), (45., 26.), (10., 40.)];

    #[test]
    fn a_rotation_and_translation_are_recovered_exactly() {
        let pairs = turn(&SHAPE, 37., 120., -45.);
        let fit = Fit::rigid(&pairs).unwrap();
        assert!((fit.degrees() - 37.).abs() < 1e-9);
        assert!(fit.max < 1e-9 && fit.rms < 1e-9);
        let back = fit.apply((10., 40.));
        let want = pairs[3].1;
        assert!((back.0 - want.0).abs() < 1e-9 && (back.1 - want.1).abs() < 1e-9);
    }

    #[test]
    fn a_mirror_image_is_not_fitted_by_a_reflection() {
        let pairs: Vec<_> = SHAPE.iter().map(|&(x, y)| ((x, y), (-x, y))).collect();
        let fit = Fit::rigid(&pairs).unwrap();
        assert!(fit.max > 5., "a reflection stays as residual: {fit:?}");
    }

    #[test]
    fn one_point_fits_by_translation() {
        let fit = Fit::rigid(&[((1., 2.), (4., 6.))]).unwrap();
        assert_eq!((fit.cos, fit.sin), (1., 0.));
        assert_eq!((fit.dx, fit.dy), (3., 4.));
        assert_eq!(Fit::rigid(&[]), None);
    }

    #[test]
    fn a_reshaped_figure_leaves_residuals() {
        let mut pairs = turn(&SHAPE, 0., 0., 0.);
        pairs[2].1 = (60., 26.);
        let fit = Fit::rigid(&pairs).unwrap();
        assert!(fit.max > 5.);
        assert!(Fit::translation(&pairs).unwrap().max >= fit.max - 1e-9);
    }
}
