use super::triangle::clip;
use super::vector::{V, cross_xy};
use super::{Field, Point};
use crate::polygon::closed_edges;

pub(super) struct Ink {
    pub(super) owners: Vec<u64>,
    pub(super) layer: i16,
    pub(super) contours: Vec<Vec<Point>>,
    pub(super) field: Field,
}
impl Ink {
    fn winding(&self, p: V) -> i32 {
        let mut winding = 0_i32;
        for contour in &self.contours {
            for (a, b) in closed_edges(contour) {
                if f64::from(a.y) <= p[1]
                    && f64::from(b.y) > p[1]
                    && cross_xy(
                        [a.x.into(), a.y.into(), 0.],
                        [b.x.into(), b.y.into(), 0.],
                        p,
                    ) > 0.
                {
                    winding += 1;
                }
                if f64::from(a.y) > p[1]
                    && f64::from(b.y) <= p[1]
                    && cross_xy(
                        [a.x.into(), a.y.into(), 0.],
                        [b.x.into(), b.y.into(), 0.],
                        p,
                    ) < 0.
                {
                    winding -= 1;
                }
            }
        }
        winding
    }
    pub(super) fn intervals(
        &self,
        a: Point,
        b: Point,
        za: f64,
        zb: f64,
        layer: i16,
        epsilon: f64,
    ) -> Vec<(f32, f32)> {
        if self.layer < layer {
            return vec![];
        }
        let dx = f64::from(b.x) - f64::from(a.x);
        let dy = f64::from(b.y) - f64::from(a.y);
        let mut events = vec![];
        for contour in &self.contours {
            for (p, q) in closed_edges(contour) {
                let ex = f64::from(q.x) - f64::from(p.x);
                let ey = f64::from(q.y) - f64::from(p.y);
                let det = dx * ey - dy * ex;
                let px = f64::from(p.x) - f64::from(a.x);
                let py = f64::from(p.y) - f64::from(a.y);
                let sy = dx * py - dy * px;
                let ty = sy + det;
                // Half-open transverse crossings group vertex/tangent events
                // without repeatedly scanning every contour for each interval.
                if !((sy <= 0. && ty > 0.) || (ty <= 0. && sy > 0.)) {
                    continue;
                }
                let t = (px * ey - py * ex) / det;
                if t > 0. && t < 1. {
                    events.push((t, if det > 0. { -1 } else { 1 }));
                }
            }
        }
        events.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut groups: Vec<(f64, i32)> = vec![];
        for (t, change) in events {
            if let Some(last) = groups.last_mut()
                && (last.0 - t).abs() < 1e-10
            {
                last.1 += change;
            } else {
                groups.push((t, change));
            }
        }
        let first = groups.first().map_or(1., |e| e.0);
        let at = |t: f64| [f64::from(a.x) + t * dx, f64::from(a.y) + t * dy, 0.];
        let mut winding = self.winding(at(first / 2.));
        let mut low = 0.;
        let mut out = vec![];
        groups.push((1., 0));
        for (high, change) in groups {
            if winding != 0 {
                let mut range = (low, high);
                if (self.layer != layer
                    || clip(
                        &mut range,
                        f64::from(self.field.at(a)) - za - epsilon,
                        f64::from(self.field.at(b)) - zb - epsilon,
                    )
                    .is_some())
                    && range.1 - range.0 > 1e-7
                {
                    out.push((range.0 as f32, range.1 as f32));
                }
            }
            winding += change;
            low = high;
        }
        out
    }
}
