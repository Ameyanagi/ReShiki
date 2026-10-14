use super::Point;
use super::vector::{V, cross_xy};

#[derive(Clone)]
pub(super) struct Triangle {
    pub(super) vertices: [V; 3],
    pub(super) face: Vec<u64>,
    pub(super) shell: u64,
    pub(super) layer: i16,
}
impl Triangle {
    pub(super) fn depth(&self, p: Point) -> Option<f64> {
        let [a, b, c] = self.vertices;
        let p = [f64::from(p.x), f64::from(p.y), 0.];
        let det = cross_xy(a, b, c);
        if det.abs() < 1e-10 {
            return None;
        }
        let u = cross_xy(a, p, c) / det;
        let v = cross_xy(a, b, p) / det;
        Some(a[2] + u * (b[2] - a[2]) + v * (c[2] - a[2]))
    }
    pub(super) fn interval(
        &self,
        a: Point,
        b: Point,
        depth: Option<(f64, f64)>,
        epsilon: f64,
    ) -> Option<(f32, f32)> {
        let mut range = (0., 1.);
        let det = cross_xy(self.vertices[0], self.vertices[1], self.vertices[2]);
        if det.abs() < 1e-10 {
            return None;
        }
        let sign = det.signum();
        let [x, y, z] = self.vertices;
        for (p, q) in [(x, y), (y, z), (z, x)] {
            clip(
                &mut range,
                sign * cross_xy(p, q, [a.x.into(), a.y.into(), 0.]),
                sign * cross_xy(p, q, [b.x.into(), b.y.into(), 0.]),
            )?;
        }
        if let Some((za, zb)) = depth {
            clip(
                &mut range,
                self.depth(a)? - za - epsilon,
                self.depth(b)? - zb - epsilon,
            )?;
        }
        (range.1 - range.0 > 1e-7).then_some((range.0 as f32, range.1 as f32))
    }
}
/// Intersect an interval with a linear inequality f(t)>=0.
pub(super) fn clip(range: &mut (f64, f64), a: f64, b: f64) -> Option<()> {
    if !a.is_finite() || !b.is_finite() {
        return None;
    }
    let d = b - a;
    if d.abs() < 1e-14 {
        if a < 0. {
            return None;
        }
    } else if d > 0. {
        range.0 = range.0.max(-a / d);
    } else {
        range.1 = range.1.min(-a / d);
    }
    (range.0 <= range.1 && range.1 >= 0. && range.0 <= 1.).then_some(())
}
