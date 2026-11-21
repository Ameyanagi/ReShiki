//! Circular arc arrows. The sweep is the stored geometry; `cubic` and `nodes`
//! hold its Bézier form so every shaft, head, hit and export path draws it.
use super::Arrow;
use crate::{
    document::{ArrowNode, Point},
    graphics::PathNodeType,
};

/// Smallest and largest sweep an arc arrow keeps, in degrees.
pub const MIN_SWEEP: f32 = 1.;
pub const MAX_SWEEP: f32 = 359.;

fn direction(degrees: f32) -> Point {
    let radians = degrees.to_radians();
    Point::new(radians.cos(), radians.sin())
}

/// The circle of an arc from `start` to `end` with a signed sweep, clockwise
/// positive in drawing coordinates: (center, radius, start angle in degrees).
pub fn circle(start: Point, end: Point, sweep: f32) -> Option<(Point, f32, f32)> {
    let chord = start.distance(end);
    let half = (sweep * 0.5).to_radians().sin();
    if chord < 1e-4 || half.abs() < 1e-6 {
        return None;
    }
    let radius = chord / (2. * half.abs());
    let along = (end.y - start.y).atan2(end.x - start.x).to_degrees();
    let first = along - sweep * 0.5 + if sweep > 0. { -90. } else { 90. };
    let d = direction(first);
    Some((start.offset(-d.x * radius, -d.y * radius), radius, first))
}

/// The signed sweep of the arc from `start` through `via` to `end`, or None
/// when the three points lie on one line.
pub fn sweep_through(start: Point, via: Point, end: Point) -> Option<f32> {
    let (ax, ay) = (f64::from(start.x), f64::from(start.y));
    let (bx, by) = (f64::from(via.x), f64::from(via.y));
    let (cx, cy) = (f64::from(end.x), f64::from(end.y));
    let d = 2. * (ax * (by - cy) + bx * (cy - ay) + cx * (ay - by));
    let chord = (cx - ax).hypot(cy - ay);
    if d.abs() < 1e-6 * chord.max(1.) * chord.max(1.) {
        return None;
    }
    let (a2, b2, c2) = (ax * ax + ay * ay, bx * bx + by * by, cx * cx + cy * cy);
    let ux = (a2 * (by - cy) + b2 * (cy - ay) + c2 * (ay - by)) / d;
    let uy = (a2 * (cx - bx) + b2 * (ax - cx) + c2 * (bx - ax)) / d;
    let angle = |x: f64, y: f64| (y - uy).atan2(x - ux).to_degrees();
    let (s, v, e) = (angle(ax, ay), angle(bx, by), angle(cx, cy));
    let clockwise = (e - s).rem_euclid(360.);
    let sweep = if (v - s).rem_euclid(360.) < clockwise {
        clockwise
    } else {
        clockwise - 360.
    };
    Some(clamp(sweep as f32))
}

/// Keep a sweep inside the drawable range, preserving its direction.
pub fn clamp(sweep: f32) -> f32 {
    sweep.signum() * sweep.abs().clamp(MIN_SWEEP, MAX_SWEEP)
}

impl Arrow {
    /// Whether this arrow is a circular arc.
    pub fn is_arc(&self) -> bool {
        self.arc.is_some()
    }
    /// Make the arrow a circular arc with this signed sweep, keeping its ends.
    /// Elbow arrows keep their corner.
    pub fn set_arc(&mut self, sweep: f32) {
        if self.kind == "bent" || !sweep.is_finite() {
            return;
        }
        self.arc = Some(clamp(sweep));
        self.fit_arc();
    }
    /// Rebuild the Bézier form of an arc from its ends and sweep.
    pub(crate) fn fit_arc(&mut self) {
        let Some(sweep) = self.arc else {
            return;
        };
        let Some((center, radius, first)) = circle(self.start, self.end, sweep) else {
            return;
        };
        let count = (sweep.abs() / 90.).ceil().max(1.) as usize;
        let step = sweep / count as f32;
        let k = 4. / 3. * (step.to_radians() / 4.).tan() * radius;
        let at = |angle: f32| {
            let d = direction(angle);
            center.offset(d.x * radius, d.y * radius)
        };
        let tangent = |angle: f32| {
            let d = direction(angle);
            Point::new(-d.y * k, d.x * k)
        };
        let leaving = |angle: f32| {
            let t = tangent(angle);
            at(angle).offset(t.x, t.y)
        };
        let arriving = |angle: f32| {
            let t = tangent(angle);
            at(angle).offset(-t.x, -t.y)
        };
        let last = first + sweep;
        self.control = None;
        self.cubic = Some([leaving(first), arriving(last)]);
        self.nodes = (1..count)
            .map(|i| {
                let angle = first + step * i as f32;
                ArrowNode {
                    point: at(angle),
                    controls: [arriving(angle), leaving(angle)],
                    kind: PathNodeType::Symmetric,
                }
            })
            .collect();
    }
    /// The arc's center and radius, for exports that write native arcs.
    pub fn arc_circle(&self) -> Option<(Point, f32, f32)> {
        circle(self.start, self.end, self.arc?)
    }
}
