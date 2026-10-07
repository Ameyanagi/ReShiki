//! Proper rigid view transforms. Rotating the view never changes physical pins.
use super::{Error, Point3};
use crate::document::Point;

fn add(a: Point3, b: Point3) -> Point3 {
    Point3 {
        x: a.x + b.x,
        y: a.y + b.y,
        z: a.z + b.z,
    }
}
pub(super) fn sub(a: Point3, b: Point3) -> Point3 {
    Point3 {
        x: a.x - b.x,
        y: a.y - b.y,
        z: a.z - b.z,
    }
}
fn scale(a: Point3, s: f64) -> Point3 {
    Point3 {
        x: a.x * s,
        y: a.y * s,
        z: a.z * s,
    }
}
fn dot(a: Point3, b: Point3) -> f64 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

pub(super) fn center(points: &[Point3]) -> Result<Point3, Error> {
    if points.is_empty() || points.iter().any(|p| !super::valid_point(*p)) {
        return Err(Error::Coordinates("Invalid or empty coordinate set"));
    }
    Ok(scale(
        points.iter().copied().fold(Point3::default(), add),
        1. / points.len() as f64,
    ))
}

/// Unit quaternion; construction and composition cannot produce a reflection.
#[derive(Clone, Copy, Debug)]
pub struct Rotation {
    w: f64,
    x: f64,
    y: f64,
    z: f64,
}
impl Default for Rotation {
    fn default() -> Self {
        Self {
            w: 1.,
            x: 0.,
            y: 0.,
            z: 0.,
        }
    }
}
impl Rotation {
    fn normalized(self) -> Self {
        let norm = (self.w * self.w + self.x * self.x + self.y * self.y + self.z * self.z).sqrt();
        if !norm.is_finite() || norm < 1e-15 {
            return Self::default();
        }
        Self {
            w: self.w / norm,
            x: self.x / norm,
            y: self.y / norm,
            z: self.z / norm,
        }
    }
    fn composed(self, b: Self) -> Self {
        Self {
            w: self.w * b.w - self.x * b.x - self.y * b.y - self.z * b.z,
            x: self.w * b.x + self.x * b.w + self.y * b.z - self.z * b.y,
            y: self.w * b.y - self.x * b.z + self.y * b.w + self.z * b.x,
            z: self.w * b.z + self.x * b.y - self.y * b.x + self.z * b.w,
        }
        .normalized()
    }
    pub fn apply(self, p: Point3) -> Point3 {
        let u = Point3 {
            x: self.x,
            y: self.y,
            z: self.z,
        };
        let cross = Point3 {
            x: u.y * p.z - u.z * p.y,
            y: u.z * p.x - u.x * p.z,
            z: u.x * p.y - u.y * p.x,
        };
        add(
            add(
                scale(u, 2. * dot(u, p)),
                scale(p, self.w * self.w - dot(u, u)),
            ),
            scale(cross, 2. * self.w),
        )
    }
    pub fn inverse(self) -> Self {
        Self {
            w: self.w,
            x: -self.x,
            y: -self.y,
            z: -self.z,
        }
    }
    fn axis(degrees: f64, axis: usize) -> Self {
        let (s, c) = (degrees.to_radians() / 2.).sin_cos();
        Self {
            w: c,
            x: if axis == 0 { s } else { 0. },
            y: if axis == 1 { s } else { 0. },
            z: if axis == 2 { s } else { 0. },
        }
    }
}

/// Fixed drawing scale and translation, with a separately editable orientation.
#[derive(Clone, Debug)]
pub struct ViewFrame {
    pub rotation: Rotation,
    pub model_center: Point3,
    pub drawing_center: Point,
    pub depth_center: f64,
    pub units_per_angstrom: f64,
}
impl ViewFrame {
    pub(super) fn fitted(model: &[Point3], drawing: &[Point3], units: f64) -> Result<Self, Error> {
        if model.len() != drawing.len() || !units.is_finite() || units <= 0. {
            return Err(Error::Coordinates("Invalid view correspondence or scale"));
        }
        let model_center = center(model)?;
        let target_center = center(drawing)?;
        let rotation = fit_rotation(model, drawing, model_center, target_center);
        Ok(Self {
            rotation,
            model_center,
            drawing_center: Point::new(
                (target_center.x * units) as f32,
                (-target_center.y * units) as f32,
            ),
            depth_center: target_center.z * units,
            units_per_angstrom: units,
        })
    }
    pub fn project(&self, p: Point3) -> (Point, f32) {
        let p = self.rotation.apply(sub(p, self.model_center));
        (
            Point::new(
                (f64::from(self.drawing_center.x) + p.x * self.units_per_angstrom) as f32,
                (f64::from(self.drawing_center.y) - p.y * self.units_per_angstrom) as f32,
            ),
            (self.depth_center + p.z * self.units_per_angstrom) as f32,
        )
    }
    /// Invert screen XY at the atom's current displayed depth, in drawing units.
    pub fn unproject(&self, p: Point, depth: f32) -> Point3 {
        let p = Point3 {
            x: (f64::from(p.x) - f64::from(self.drawing_center.x)) / self.units_per_angstrom,
            y: -(f64::from(p.y) - f64::from(self.drawing_center.y)) / self.units_per_angstrom,
            z: (f64::from(depth) - self.depth_center) / self.units_per_angstrom,
        };
        add(self.model_center, self.rotation.inverse().apply(p))
    }
    /// Screen-convention X then Y tilt, matching the existing drawing tool.
    pub fn rotate(&mut self, x_degrees: f64, y_degrees: f64) {
        if !x_degrees.is_finite() || !y_degrees.is_finite() {
            return;
        }
        self.rotation = Rotation::axis(y_degrees, 1)
            .composed(Rotation::axis(-x_degrees, 0))
            .composed(self.rotation);
    }
    pub fn roll(&mut self, degrees: f64) {
        if degrees.is_finite() {
            self.rotation = Rotation::axis(-degrees, 2).composed(self.rotation);
        }
    }
    pub(super) fn validate(&self) -> Result<(), Error> {
        if !super::valid_point(self.model_center)
            || !self.units_per_angstrom.is_finite()
            || self.units_per_angstrom <= 0.
            || !self.depth_center.is_finite()
            || !self.drawing_center.x.is_finite()
            || !self.drawing_center.y.is_finite()
        {
            return Err(Error::Coordinates("Invalid projection frame"));
        }
        Ok(())
    }
}

/// Horn's absolute orientation, solved with shifted symmetric power iteration.
/// Trying all four quaternion basis vectors handles degenerate/orthogonal seeds.
fn fit_rotation(source: &[Point3], target: &[Point3], sc: Point3, tc: Point3) -> Rotation {
    let (mut xx, mut xy, mut xz, mut yx, mut yy, mut yz, mut zx, mut zy, mut zz) =
        (0., 0., 0., 0., 0., 0., 0., 0., 0.);
    for (s, t) in source.iter().zip(target) {
        let s = sub(*s, sc);
        let t = sub(*t, tc);
        xx += s.x * t.x;
        xy += s.x * t.y;
        xz += s.x * t.z;
        yx += s.y * t.x;
        yy += s.y * t.y;
        yz += s.y * t.z;
        zx += s.z * t.x;
        zy += s.z * t.y;
        zz += s.z * t.z;
    }
    let (ww, wx, wy, wz, qxx, qxy, qxz, qyy, qyz, qzz) = (
        xx + yy + zz,
        yz - zy,
        zx - xz,
        xy - yx,
        xx - yy - zz,
        xy + yx,
        xz + zx,
        -xx + yy - zz,
        yz + zy,
        -xx - yy + zz,
    );
    let magnitude = ww.abs()
        + wx.abs()
        + wy.abs()
        + wz.abs()
        + qxx.abs()
        + qxy.abs()
        + qxz.abs()
        + qyy.abs()
        + qyz.abs()
        + qzz.abs();
    if magnitude < 1e-12 {
        return Rotation::default();
    }
    let multiply = |q: Rotation, shift: f64| Rotation {
        w: (ww + shift) * q.w + wx * q.x + wy * q.y + wz * q.z,
        x: wx * q.w + (qxx + shift) * q.x + qxy * q.y + qxz * q.z,
        y: wy * q.w + qxy * q.x + (qyy + shift) * q.y + qyz * q.z,
        z: wz * q.w + qxz * q.x + qyz * q.y + (qzz + shift) * q.z,
    };
    let mut best = Rotation::default();
    let mut score = f64::NEG_INFINITY;
    for seed in [
        Rotation::default(),
        Rotation {
            w: 0.,
            x: 1.,
            y: 0.,
            z: 0.,
        },
        Rotation {
            w: 0.,
            x: 0.,
            y: 1.,
            z: 0.,
        },
        Rotation {
            w: 0.,
            x: 0.,
            y: 0.,
            z: 1.,
        },
    ] {
        let mut q = seed;
        for _ in 0..256 {
            q = multiply(q, magnitude).normalized();
        }
        let p = multiply(q, 0.);
        let value = q.w * p.w + q.x * p.x + q.y * p.y + q.z * p.z;
        if value > score {
            score = value;
            best = q;
        }
    }
    best.normalized()
}

#[cfg(test)]
mod tests;
