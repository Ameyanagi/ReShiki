//! Elliptical arcs in an affine frame, shared by the editor and all exporters.
use super::{Graphic, GraphicKind, PathCommand};
use crate::document::Point;
use serde::{Deserialize, Serialize};

// Native files contain ordinary cubic paths as a compatibility fallback. Older
// releases ignore the optional arc metadata and still render/transform/save the
// same drawing. Current releases restore the parametric endpoint controls.
impl Serialize for Graphic {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.kind == GraphicKind::Arc
            && let Some(arc) = self.arc
        {
            let mut wire = self.clone();
            wire.kind = GraphicKind::Path;
            wire.path = arc.commands();
            Graphic::serialize(&wire, serializer)
        } else {
            Graphic::serialize(self, serializer)
        }
    }
}

impl<'de> Deserialize<'de> for Graphic {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut graphic = Graphic::deserialize(deserializer)?;
        if graphic.kind == GraphicKind::Path && graphic.arc.is_some() {
            graphic.kind = GraphicKind::Arc;
            graphic.path.clear();
        }
        Ok(graphic)
    }
}

/// Angles run clockwise in drawing coordinates, with 0° at the right edge.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ArcGeometry {
    pub start_degrees: f32,
    pub sweep_degrees: f32,
}

impl Default for ArcGeometry {
    fn default() -> Self {
        Self {
            start_degrees: 180.,
            sweep_degrees: 180.,
        }
    }
}

impl ArcGeometry {
    pub const PRESETS: [f32; 5] = [90., 120., 180., 270., 360.];

    pub fn validate(self) -> Result<(), String> {
        if !self.start_degrees.is_finite() || !(0.1..=360.).contains(&self.sweep_degrees) {
            return Err("Arc start must be finite and sweep must be 0.1–360°".into());
        }
        Ok(())
    }

    pub fn point(self, angle: f32) -> Point {
        let angle = angle.rem_euclid(360.).to_radians();
        Point::new(0.5 + 0.5 * angle.cos(), 0.5 + 0.5 * angle.sin())
    }

    pub fn commands(self) -> Vec<PathCommand> {
        // Every cubic covers at most a quarter ellipse. Exact endpoints keep
        // the full-circle seam together without closing/filling an open arc.
        let start = self.start_degrees.rem_euclid(360.);
        let count = (self.sweep_degrees / 90.).ceil().clamp(1., 4.) as usize;
        let step = self.sweep_degrees / count as f32;
        let k = 4. / 3. * (step.to_radians() / 4.).tan() * 0.5;
        let mut commands = vec![PathCommand::Move(self.point(start))];
        for i in 0..count {
            let a = start + step * i as f32;
            let b = start + step * (i + 1) as f32;
            let p = self.point(a);
            let q = self.point(b);
            let a = a.to_radians();
            let b = b.to_radians();
            commands.push(PathCommand::Cubic(
                p.offset(-a.sin() * k, a.cos() * k),
                q.offset(b.sin() * k, -b.cos() * k),
                q,
            ));
        }
        commands
    }
}

impl Graphic {
    /// Set the ellipse parameters of a newly dragged graphic.
    pub fn with_arc(mut self, arc: ArcGeometry) -> Self {
        if self.kind == GraphicKind::Arc {
            self.arc = Some(arc);
        }
        self
    }

    /// Upgrade old half-ellipse frames without moving their visible geometry.
    pub fn set_arc(&mut self, arc: ArcGeometry) {
        if self.kind != GraphicKind::Arc {
            return;
        }
        if self.arc.is_none() {
            self.axis_y.x *= 2.;
            self.axis_y.y *= 2.;
            self.depth[2] *= 2.;
        }
        self.arc = Some(arc);
    }

    fn arc_world(&self, p: Point) -> Point {
        let y_scale = if self.arc.is_some() { 1. } else { 2. };
        self.origin.offset(
            self.axis_x.x * p.x + self.axis_y.x * p.y * y_scale,
            self.axis_x.y * p.x + self.axis_y.y * p.y * y_scale,
        )
    }

    /// Arc endpoints stay parametric; other graphics expose their Bézier points.
    pub fn edit_points(&self) -> Vec<Point> {
        if self.kind == GraphicKind::Arc {
            let arc = self.arc.unwrap_or_default();
            let start = arc.start_degrees.rem_euclid(360.);
            [start, start + arc.sweep_degrees]
                .map(|a| self.arc_world(arc.point(a)))
                .to_vec()
        } else {
            self.commands()
                .iter()
                .flat_map(PathCommand::iter_points)
                .collect()
        }
    }

    pub(super) fn edit_arc_endpoint(&mut self, index: usize, p: Point) {
        if index > 1 || !p.x.is_finite() || !p.y.is_finite() {
            return;
        }
        let determinant = self.axis_x.x * self.axis_y.y - self.axis_x.y * self.axis_y.x;
        if determinant.abs() < 0.000_001 {
            return; // An edge-on projected ellipse has no invertible frame.
        }
        let delta = Point::new(p.x - self.origin.x, p.y - self.origin.y);
        let x = (delta.x * self.axis_y.y - delta.y * self.axis_y.x) / determinant;
        let mut y = (self.axis_x.x * delta.y - self.axis_x.y * delta.x) / determinant;
        if self.arc.is_none() {
            y *= 0.5;
        }
        if (x - 0.5).hypot(y - 0.5) < 0.000_001 {
            return;
        }
        let angle = (y - 0.5).atan2(x - 0.5).to_degrees().rem_euclid(360.);
        let mut arc = self.arc.unwrap_or_default();
        arc.start_degrees = arc.start_degrees.rem_euclid(360.);
        let sweep = if index == 0 {
            let end = arc.start_degrees + arc.sweep_degrees;
            arc.start_degrees = angle;
            (end - angle).rem_euclid(360.)
        } else {
            (angle - arc.start_degrees).rem_euclid(360.)
        };
        // Coincident endpoints close the ellipse. In particular this must
        // work from a half arc: drag previews always start from the original
        // document, so they cannot rely on a previous preview's sweep.
        arc.sweep_degrees = if !(0.0001..359.9999).contains(&sweep) {
            360.
        } else {
            sweep.max(0.1)
        };
        self.set_arc(arc);
    }
}
