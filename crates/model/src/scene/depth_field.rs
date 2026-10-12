//! Depth interpolation for scene ink and view-occlusion geometry.
//! Positive Z faces the viewer; the existing arithmetic preserves paint output.
use crate::document::{Bond, Document, Point};

/// Exclusive plane-fit residual bound after evaluating depth as f32.
const PLANE_FIT_TOLERANCE: f32 = 0.001;

pub(crate) fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// XYZ depth of ink as a function of drawing XY. The field names `weight`,
/// `wa` and `wb` predate this module (midplane weights); they hold depth.
#[derive(Clone, Copy)]
pub(crate) enum DepthField {
    Constant(f32),
    Bond {
        a: Point,
        b: Point,
        wa: f32,
        wb: f32,
    },
    Plane {
        origin: Point,
        weight: f64,
        dx: f64,
        dy: f64,
    },
}
impl DepthField {
    pub(crate) fn at(self, p: Point) -> f32 {
        match self {
            Self::Constant(w) => w,
            Self::Bond { a, b, wa, wb } => {
                let (x, y) = (
                    f64::from(b.x) - f64::from(a.x),
                    f64::from(b.y) - f64::from(a.y),
                );
                let length = x * x + y * y;
                let t = if length > 1e-12 {
                    ((f64::from(p.x) - f64::from(a.x)) * x + (f64::from(p.y) - f64::from(a.y)) * y)
                        / length
                } else {
                    0.5
                };
                lerp(wa, wb, t.clamp(0., 1.) as f32)
            }
            Self::Plane {
                origin,
                weight,
                dx,
                dy,
            } => {
                (weight
                    + (f64::from(p.x) - f64::from(origin.x)) * dx
                    + (f64::from(p.y) - f64::from(origin.y)) * dy) as f32
            }
        }
    }
    // Keep this separate from crate::crossings::depth::Plane::fit: crossings
    // use f32 throughout; depth uses f64 with weight subtraction still in f32.
    // Crossings start area at 0 with area.max(0.001), versus 1e-6 here, and
    // accept residual 0.01 versus 0.001 here after f32 rounding. These fits
    // must remain separate to preserve output.
    pub(crate) fn plane(vertices: &[(Point, f32)]) -> Option<Self> {
        let &(origin, weight) = vertices.first()?;
        let mut best = None;
        let mut area = 1e-6_f64;
        for (i, (a, wa)) in vertices.iter().enumerate().skip(1) {
            for (b, wb) in vertices.iter().skip(i + 1) {
                let (ax, ay) = (
                    f64::from(a.x) - f64::from(origin.x),
                    f64::from(a.y) - f64::from(origin.y),
                );
                let (bx, by) = (
                    f64::from(b.x) - f64::from(origin.x),
                    f64::from(b.y) - f64::from(origin.y),
                );
                let det = ax * by - ay * bx;
                if det.abs() > area {
                    area = det.abs();
                    let aw = f64::from(*wa - weight);
                    let bw = f64::from(*wb - weight);
                    best = Some(Self::Plane {
                        origin,
                        weight: f64::from(weight),
                        dx: (aw * by - ay * bw) / det,
                        dy: (ax * bw - aw * bx) / det,
                    });
                }
            }
        }
        let field = best?;
        vertices
            .iter()
            .all(|(p, w)| (field.at(*p) - w).abs() < PLANE_FIT_TOLERANCE)
            .then_some(field)
    }
}

pub(crate) fn bond_depth_field(doc: &Document, bond: &Bond) -> DepthField {
    let (Some(a), Some(b)) = (doc.atom(bond.a), doc.atom(bond.b)) else {
        return DepthField::Constant(0.);
    };
    let linear = DepthField::Bond {
        a: a.position,
        b: b.position,
        wa: a.depth,
        wb: b.depth,
    };
    if matches!(bond.order, 2 | 7) && bond.double_position == crate::bonds::DoublePosition::Auto {
        crate::scene::projected_bonds::face_atoms(doc, bond)
            .and_then(|ids| {
                let vertices: Vec<_> = ids.iter().filter_map(|id| doc.atom(*id)).collect();
                let n = vertices.len().max(1) as f32;
                let center = vertices.iter().fold(Point::default(), |p, a| {
                    p.offset(a.position.x / n, a.position.y / n)
                });
                let depth = vertices.iter().map(|a| a.depth / n).sum();
                DepthField::plane(&[
                    (a.position, a.depth),
                    (b.position, b.depth),
                    (center, depth),
                ])
            })
            .unwrap_or(linear)
    } else {
        linear
    }
}
