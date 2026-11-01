//! Reflection fitting for a connected path, including its Bézier controls.
use super::{Graphic, Path};
use crate::document::Point;

struct Samples {
    points: Vec<Point>,
    owners: Vec<usize>,
    controls: Vec<Option<usize>>,
    centre: (f64, f64),
    tolerance: f64,
    roundoff: f64,
}
impl Samples {
    fn read(path: &Path) -> Result<Self, String> {
        let mut points: Vec<_> = path.nodes.iter().map(|node| node.point).collect();
        let mut owners: Vec<_> = (0..points.len()).collect();
        let mut controls = Vec::with_capacity(path.segments.len());
        for (segment, item) in path.segments.iter().enumerate() {
            controls.push(item.controls.map(|[a, b]| {
                let index = points.len();
                points.extend([a, b]);
                owners.extend([segment, (segment + 1) % path.nodes.len()]);
                index
            }));
        }
        if points.iter().any(|point| !point.is_finite()) {
            return Err("Path coordinates are out of range".into());
        }
        let mut min = (f64::INFINITY, f64::INFINITY);
        let mut max = (f64::NEG_INFINITY, f64::NEG_INFINITY);
        let mut sum = (0., 0.);
        let mut magnitude: f64 = 0.;
        for point in &points {
            let (x, y) = (f64::from(point.x), f64::from(point.y));
            min = (min.0.min(x), min.1.min(y));
            max = (max.0.max(x), max.1.max(y));
            sum.0 += x;
            sum.1 += y;
            magnitude = magnitude.max(x.abs()).max(y.abs());
        }
        // Reflection and affine-frame storage round back to f32 coordinates.
        let roundoff = 1e-6 + magnitude * f64::from(f32::EPSILON) * 4.;
        Ok(Self {
            centre: (sum.0 / points.len() as f64, sum.1 / points.len() as f64),
            // Allow a sketch to deviate by 10% of its anchor/control bounds'
            // diagonal. Larger mismatches are a different shape, not rounding.
            tolerance: ((max.0 - min.0).hypot(max.1 - min.1) * 0.1).max(roundoff),
            points,
            owners,
            controls,
            roundoff,
        })
    }

    fn pairing(&self, path: &Path, offset: usize) -> Result<Vec<usize>, usize> {
        let count = path.nodes.len();
        let partner = |node| {
            if path.closed {
                (offset + count - node) % count
            } else {
                count - 1 - node
            }
        };
        let mut paired: Vec<_> = (0..count).map(partner).collect();
        paired.resize(self.points.len(), 0);
        let mut incompatible = vec![false; count];
        for (segment, controls) in self.controls.iter().enumerate() {
            // Reversing the path maps this segment's end to the partner's start.
            let other = partner((segment + 1) % count);
            match (*controls, self.controls.get(other).copied().flatten()) {
                (Some(first), Some(second)) => {
                    if let Some([a, b]) = paired.get_mut(first..first + 2) {
                        *a = second + 1;
                        *b = second;
                    }
                }
                (None, None) => {}
                _ => {
                    for node in [segment, (segment + 1) % count, other, (other + 1) % count] {
                        if let Some(incompatible) = incompatible.get_mut(node) {
                            *incompatible = true;
                        }
                    }
                }
            }
        }
        let count = incompatible.into_iter().filter(|bad| *bad).count();
        if count == 0 { Ok(paired) } else { Err(count) }
    }

    fn fit(&self, paired: Vec<usize>, upright: bool) -> Option<Fit> {
        let mut a = 0.;
        let mut b = 0.;
        for (point, other) in self.points.iter().zip(&paired) {
            let (x, y) = self.relative(*point);
            let (u, v) = self.relative(*self.points.get(*other)?);
            a += x * u - y * v;
            b += x * v + y * u;
        }
        // The reflection matrix is [cos(2θ), sin(2θ); sin(2θ), -cos(2θ)].
        // Maximising a*cos(2θ) + b*sin(2θ) gives 2θ = atan2(b, a).
        let length = a.hypot(b);
        let (cosine, sine) = if upright {
            (if a > 0. { 1. } else { -1. }, 0.)
        } else if length > 0. {
            (a / length, b / length)
        } else {
            (-1., 0.)
        };
        let mut fit = Fit {
            paired,
            centre: self.centre,
            cosine,
            sine,
            residual: 0.,
            distances: Vec::with_capacity(self.points.len()),
        };
        for (point, other) in self.points.iter().zip(&fit.paired) {
            let reflected = fit.reflect(*self.points.get(*other)?);
            let distance =
                (f64::from(point.x) - reflected.0).hypot(f64::from(point.y) - reflected.1);
            fit.residual += distance * distance;
            fit.distances.push(distance);
        }
        Some(fit)
    }

    fn relative(&self, point: Point) -> (f64, f64) {
        (
            f64::from(point.x) - self.centre.0,
            f64::from(point.y) - self.centre.1,
        )
    }
}

struct Fit {
    paired: Vec<usize>,
    centre: (f64, f64),
    cosine: f64,
    sine: f64,
    residual: f64,
    distances: Vec<f64>,
}
impl Fit {
    fn reflect(&self, point: Point) -> (f64, f64) {
        self.reflect_xy((f64::from(point.x), f64::from(point.y)))
    }

    fn reflect_xy(&self, (x, y): (f64, f64)) -> (f64, f64) {
        let (x, y) = (x - self.centre.0, y - self.centre.1);
        (
            self.centre.0 + self.cosine * x + self.sine * y,
            self.centre.1 + self.sine * x - self.cosine * y,
        )
    }
}

fn refusal(count: usize) -> String {
    format!("Not symmetric enough: {count} nodes can't be paired across an axis")
}

impl Graphic {
    /// Average a nearly symmetric path across its best reflection axis.
    /// Upright restricts that axis to vertical or horizontal in world space.
    /// Refusals and already symmetric paths leave all stored data unchanged.
    pub fn symmetrise_path(&mut self, upright: bool) -> Result<bool, String> {
        let mut path = Path::read(self).ok_or("Select one continuous path")?;
        let samples = Samples::read(&path)?;
        let mut best: Option<(usize, Fit)> = None;
        let mut incompatible = path.nodes.len();
        for offset in 0..if path.closed { path.nodes.len() } else { 1 } {
            match samples.pairing(&path, offset) {
                Ok(paired) => {
                    let Some(fit) = samples.fit(paired, upright) else {
                        continue;
                    };
                    let mut unpaired = vec![false; path.nodes.len()];
                    for (distance, owner) in fit.distances.iter().zip(&samples.owners) {
                        if *distance > samples.tolerance
                            && let Some(unpaired) = unpaired.get_mut(*owner)
                        {
                            *unpaired = true;
                        }
                    }
                    let count = unpaired.into_iter().filter(|bad| *bad).count();
                    // Minimise refused nodes before comparing total fitting error.
                    if best.as_ref().is_none_or(|(old_count, old)| {
                        (count, fit.residual) < (*old_count, old.residual)
                    }) {
                        best = Some((count, fit));
                    }
                }
                Err(count) => incompatible = incompatible.min(count),
            }
        }
        let (count, fit) = best.ok_or_else(|| {
            format!(
                "{} (straight and curved segments differ)",
                refusal(incompatible)
            )
        })?;
        if count > 0 {
            return Err(refusal(count));
        }
        if fit
            .distances
            .iter()
            .all(|distance| *distance <= samples.roundoff)
        {
            return Ok(false);
        }
        let mut points = samples.points.clone();
        for (index, (point, &other)) in samples.points.iter().zip(&fit.paired).enumerate() {
            if index > other {
                continue;
            }
            let reflected = fit.reflect(*samples.points.get(other).ok_or("Invalid path point")?);
            let average = (
                (f64::from(point.x) + reflected.0) * 0.5,
                (f64::from(point.y) + reflected.1) * 0.5,
            );
            if let Some(point) = points.get_mut(index) {
                *point = Point::new(average.0 as f32, average.1 as f32);
            }
            if index != other {
                let reflected = fit.reflect_xy(average);
                if let Some(point) = points.get_mut(other) {
                    *point = Point::new(reflected.0 as f32, reflected.1 as f32);
                }
            }
        }
        if points == samples.points {
            return Ok(false);
        }
        for (node, point) in path.nodes.iter_mut().zip(&points) {
            node.point = *point;
        }
        for (segment, controls) in path.segments.iter_mut().zip(samples.controls) {
            if let Some(first) = controls
                && let Some([a, b]) = points.get(first..first + 2)
            {
                segment.controls = Some([*a, *b]);
            }
        }
        path.store(self)?;
        Ok(true)
    }
}

#[cfg(test)]
#[path = "symmetry_tests.rs"]
mod tests;
