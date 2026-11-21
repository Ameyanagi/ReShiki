//! Tangent-preserving Bow and reversible lanes, built from original
//! perimeter arclength knots. This deliberately is not an exact offset curve.
use crate::{
    arrows::{ArrowStyle, Head, Preset},
    cycles::{Outline, shapes::CubicSection},
    document::{Arrow, ArrowNode, Point},
    graphics::PathNodeType,
    style::DEFAULT,
};

type V = [f64; 2];

#[derive(Clone, Debug)]
struct Path(Vec<CubicSection>);

impl Path {
    fn normals(&self, dir: f64) -> Option<Vec<V>> {
        let mut normals = vec![];
        for i in 0..=self.0.len() {
            let before = i.checked_sub(1).and_then(|i| self.0.get(i));
            let after = self.0.get(i);
            let normal = |t: V| [dir * t[1], -dir * t[0]];
            let incoming = before.and_then(|c| tangent(c.points, false)).map(normal);
            let outgoing = after.and_then(|c| tangent(c.points, true)).map(normal);
            let n = match (incoming, outgoing) {
                (Some(a), Some(b)) if before.is_some_and(|c| c.corner_after) => {
                    let denominator = 1. + dot(a, b);
                    if denominator <= 64. * f64::EPSILON {
                        // The exact miter is numerically unresolved at an
                        // almost folded corner. Keep both native lanes with
                        // a bounded bisector instead of dropping the step.
                        // Choose canonical outgoing, independent of travel,
                        // if even the bisector has rounded to zero.
                        unit(add(a, b)).map_or(if dir > 0. { b } else { a }, |n| mul(n, 4.))
                    } else {
                        mul(add(a, b), 1. / denominator)
                    }
                }
                (_, Some(b)) => b,
                (Some(a), None) => a,
                _ => return None,
            };
            normals.push(n);
        }
        Some(normals)
    }

    fn displaced(&self, displacements: &[V]) -> Option<Self> {
        let mut curves = vec![];
        for (i, source) in self.0.iter().enumerate() {
            let (first, last) = (*displacements.get(i)?, *displacements.get(i + 1)?);
            let mut curve = source.clone();
            curve.points = [
                add(source.points[0], first),
                add(source.points[1], first),
                add(source.points[2], last),
                add(source.points[3], last),
            ];
            if curve.points.iter().flatten().any(|v| !v.is_finite()) {
                return None;
            }
            curves.push(curve);
        }
        Some(Self(curves))
    }

    fn bowed(&self, bow: f64, dir: f64) -> Option<Self> {
        if bow == 0. {
            return Some(self.clone());
        }
        let normals = self.normals(dir)?;
        let mut shifts = vec![[0., 0.]];
        for (i, curve) in self.0.iter().enumerate() {
            let shift = if i + 1 == self.0.len() {
                [0., 0.]
            } else {
                mul(
                    *normals.get(i + 1)?,
                    bow * (std::f64::consts::PI * curve.end_t).sin().powi(2),
                )
            };
            shifts.push(shift);
        }
        self.displaced(&shifts)
    }

    fn offset(&self, amount: f64, dir: f64) -> Option<Self> {
        let shifts: Vec<_> = self
            .normals(dir)?
            .into_iter()
            .map(|n| mul(n, amount))
            .collect();
        self.displaced(&shifts)
    }

    fn reversed(&self) -> Self {
        Self(
            self.0
                .iter()
                .enumerate()
                .rev()
                .map(|(i, curve)| {
                    let previous = i.checked_sub(1).and_then(|j| self.0.get(j));
                    CubicSection {
                        points: [
                            curve.points[3],
                            curve.points[2],
                            curve.points[1],
                            curve.points[0],
                        ],
                        end_t: 1. - previous.map_or(0., |c| c.end_t),
                        corner_after: previous.is_some_and(|c| c.corner_after),
                    }
                })
                .collect(),
        )
    }

    fn half_head(&self, outside: V) -> Option<Head> {
        let t = tangent(self.0.last()?.points, false)?;
        Some(if dot([t[1], -t[0]], outside) >= 0. {
            Head::Left
        } else {
            Head::Right
        })
    }

    fn arrow(&self, id: u64, style: ArrowStyle) -> Option<Arrow> {
        let first = self.0.first()?.points;
        let last = self.0.last()?.points;
        let point = |v: V| {
            let p = Point::new(v[0] as f32, v[1] as f32);
            p.is_finite().then_some(p)
        };
        let mut arrow = Arrow::new(
            id,
            point(first[0])?,
            point(last[3])?,
            Preset::Forward,
            style,
        );
        arrow.cubic = Some([point(first[1])?, point(last[2])?]);
        for (before, after) in self.0.iter().zip(self.0.iter().skip(1)) {
            arrow.nodes.push(ArrowNode {
                point: point(before.points[3])?,
                controls: [point(before.points[2])?, point(after.points[1])?],
                kind: if before.corner_after {
                    PathNodeType::Corner
                } else {
                    PathNodeType::Smooth
                },
            });
        }
        arrow.validate().ok()?;
        Some(arrow)
    }
}

pub(super) fn arrows(
    id: u64,
    outline: &Outline,
    start: f64,
    length: f64,
    bow: f64,
    reversible: bool,
    style: &ArrowStyle,
) -> Option<Vec<Arrow>> {
    if bow == 0. && !reversible {
        return Some(vec![super::path_arrow(id, outline, start, length, style)?]);
    }
    let dir = length.signum();
    let baseline = Path(outline.cubic_sections(start, length)?);
    let bowed = baseline.bowed(bow, dir).unwrap_or_else(|| baseline.clone());
    let render = |path: &Path| {
        if !reversible {
            return Some(vec![path.arrow(id, style.clone())?]);
        }
        let half = f64::from(DEFAULT.world(style.gap_pt.max(2.)) * 0.75);
        let normals = path.normals(dir)?;
        let forward = path.offset(half, dir)?;
        let backward = path.offset(-half, dir)?.reversed();
        let outside = *normals.last()?;
        let inside = mul(*normals.first()?, -1.);
        let forward_style = ArrowStyle {
            head: forward.half_head(outside)?,
            ..style.clone()
        };
        let backward_style = ArrowStyle {
            head: backward.half_head(inside)?,
            ..style.clone()
        };
        Some(vec![
            forward.arrow(id, forward_style)?,
            backward.arrow(id, backward_style)?,
        ])
    };
    // If the requested deformation cannot be represented by native points,
    // retain the undeformed perimeter rather than losing the entire step.
    render(&bowed).or_else(|| render(&baseline))
}

fn add(a: V, b: V) -> V {
    [a[0] + b[0], a[1] + b[1]]
}
fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1]]
}
fn mul(a: V, k: f64) -> V {
    [a[0] * k, a[1] * k]
}
fn dot(a: V, b: V) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
fn unit(v: V) -> Option<V> {
    let length = v[0].hypot(v[1]);
    (length > 0. && length.is_finite()).then(|| mul(v, 1. / length))
}
fn tangent(p: [V; 4], start: bool) -> Option<V> {
    if start {
        unit(sub(p[1], p[0]))
            .or_else(|| unit(sub(p[2], p[0])))
            .or_else(|| unit(sub(p[3], p[0])))
    } else {
        unit(sub(p[3], p[2]))
            .or_else(|| unit(sub(p[3], p[1])))
            .or_else(|| unit(sub(p[3], p[0])))
    }
}

#[cfg(test)]
#[path = "perimeter_deform_tests.rs"]
mod tests;
