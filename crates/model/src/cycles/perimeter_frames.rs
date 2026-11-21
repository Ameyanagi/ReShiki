//! Shared painted-lane tangent and corner frames for noncircle names and roles.
use super::super::{Outline, shapes::Shaft};
use crate::document::Point;

#[derive(Clone, Copy)]
pub(in crate::cycles) struct Frame {
    pub(in crate::cycles) point: Point,
    pub(in crate::cycles) incoming: Point,
    pub(in crate::cycles) outgoing: Point,
    pub(in crate::cycles) inward: [f32; 2],
}

impl Frame {
    pub(in crate::cycles) fn at(
        [shaft, reverse]: [Option<&Shaft>; 2],
        perimeter: &Outline,
        fallback: f64,
        fraction: f64,
        dir: f32,
    ) -> Option<Self> {
        if let Some(shaft) = shaft {
            let s = fraction * shaft.length();
            let (point, incoming) = shaft.sample(s, true)?;
            let (_, outgoing) = shaft.sample(s, false)?;
            let normal = |t: Point| Point::new(dir * t.y, -dir * t.x);
            let (incoming, outgoing) = (normal(incoming), normal(outgoing));
            let inward = reverse
                .and_then(|s| s.project(point))
                .map_or([0., 0.], |p| {
                    [incoming, outgoing].map(|n| {
                        (-(p.point.x - point.x) * n.x - (p.point.y - point.y) * n.y).max(0.)
                    })
                });
            return Some(Self {
                point,
                incoming,
                outgoing,
                inward,
            });
        }
        let tangent = perimeter.tangents(fallback)?;
        // Outline tangents are already canonical clockwise, including after
        // reflection. A missing route's fallback must not reverse them again.
        let normal = |t: Point| Point::new(t.y, -t.x);
        Some(Self {
            point: perimeter.point(fallback)?,
            incoming: normal(tangent.incoming),
            outgoing: normal(tangent.outgoing),
            inward: [0., 0.],
        })
    }

    pub(in crate::cycles) fn offsets(
        self,
        width: f32,
        height: f32,
        gap: f32,
        inside: bool,
    ) -> Vec<Point> {
        let support = |n: Point| 0.5 * (width * n.x.abs() + height * n.y.abs()) + gap;
        let a = self.incoming;
        let b = self.outgoing;
        let [extra_a, extra_b] = if inside { self.inward } else { [0., 0.] };
        let (ha, hb) = (support(a) + extra_a, support(b) + extra_b);
        let mut out = vec![];
        let determinant = f64::from(a.x) * f64::from(b.y) - f64::from(a.y) * f64::from(b.x);
        if determinant.abs() > 1e-5 {
            // The text box has a different support radius along each side.
            // Solve both clearances; a unit bisector would round the corner.
            let x = (f64::from(ha) * f64::from(b.y) - f64::from(a.y) * f64::from(hb)) / determinant;
            let y = (f64::from(a.x) * f64::from(hb) - f64::from(ha) * f64::from(b.x)) / determinant;
            // Nearly folded corners need nearby one-sided/alternate-position
            // candidates rather than a name sent far outside the drawing.
            if x.hypot(y) <= 4. * f64::from(ha.max(hb)) {
                out.push(Point::new(x as f32, y as f32));
            }
        }
        out.push(Point::new(b.x * hb, b.y * hb));
        if a.x * b.x + a.y * b.y < 1. - 1e-5 {
            out.push(Point::new(a.x * ha, a.y * ha));
        }
        out
    }
}
