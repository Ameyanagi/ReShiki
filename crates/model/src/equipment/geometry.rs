//! Path data and plane geometry for pieces, in f64 millimetres.
use super::format::Xy;

/// An absolute path command; arcs and quadratics become cubics on parsing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Seg {
    Move(Xy),
    Line(Xy),
    Cubic(Xy, Xy, Xy),
    Close,
}

/// A similarity or affine map [a, b, c, d, e, f]: x' = a x + c y + e, y' = b x + d y + f.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affine(pub [f64; 6]);

impl Affine {
    pub const IDENTITY: Self = Self([1., 0., 0., 1., 0., 0.]);
    pub fn translate(t: Xy) -> Self {
        Self([1., 0., 0., 1., t[0], t[1]])
    }
    /// Rotation by `degrees` (clockwise on the page, y down) about `pivot`.
    pub fn rotate(degrees: f64, pivot: Xy) -> Self {
        let (s, c) = degrees.to_radians().sin_cos();
        Self::translate(pivot)
            .then(Self([c, s, -s, c, 0., 0.]))
            .then(Self::translate([-pivot[0], -pivot[1]]))
    }
    pub fn scale(k: f64, pivot: Xy) -> Self {
        Self([k, 0., 0., k, pivot[0] * (1. - k), pivot[1] * (1. - k)])
    }
    /// Scale by `k` along the unit `axis` only, about `pivot`.
    pub fn stretch(k: f64, axis: Xy, pivot: Xy) -> Self {
        let n = axis[0].hypot(axis[1]);
        let (ux, uy) = (axis[0] / n, axis[1] / n);
        let m = [
            1. + (k - 1.) * ux * ux,
            (k - 1.) * ux * uy,
            (k - 1.) * ux * uy,
            1. + (k - 1.) * uy * uy,
        ];
        Self::translate(pivot)
            .then(Self([m[0], m[1], m[2], m[3], 0., 0.]))
            .then(Self::translate([-pivot[0], -pivot[1]]))
    }
    pub fn inverse(&self) -> Option<Self> {
        let [a, b, c, d, e, f] = self.0;
        let det = a * d - b * c;
        if det.abs() < 1e-12 || !det.is_finite() {
            return None;
        }
        let (ia, ib, ic, id) = (d / det, -b / det, -c / det, a / det);
        Some(Self([
            ia,
            ib,
            ic,
            id,
            -(ia * e + ic * f),
            -(ib * e + id * f),
        ]))
    }
    /// `self ∘ other`: apply `other` first.
    pub fn then(self, other: Self) -> Self {
        let [a, b, c, d, e, f] = self.0;
        let [a2, b2, c2, d2, e2, f2] = other.0;
        Self([
            a * a2 + c * b2,
            b * a2 + d * b2,
            a * c2 + c * d2,
            b * c2 + d * d2,
            a * e2 + c * f2 + e,
            b * e2 + d * f2 + f,
        ])
    }
    pub fn apply(&self, p: Xy) -> Xy {
        let [a, b, c, d, e, f] = self.0;
        [a * p[0] + c * p[1] + e, b * p[0] + d * p[1] + f]
    }
    /// A direction, without translation, normalised.
    pub fn apply_dir(&self, v: Xy) -> Xy {
        let [a, b, c, d, ..] = self.0;
        normalize([a * v[0] + c * v[1], b * v[0] + d * v[1]])
    }
    /// Length scale of a similarity.
    pub fn scale_factor(&self) -> f64 {
        let [a, b, c, d, ..] = self.0;
        (a * d - b * c).abs().sqrt()
    }
    pub fn mirrored(&self) -> bool {
        let [a, b, c, d, ..] = self.0;
        a * d - b * c < 0.
    }
    pub fn map(&self, segs: &[Seg]) -> Vec<Seg> {
        segs.iter()
            .map(|s| match *s {
                Seg::Move(p) => Seg::Move(self.apply(p)),
                Seg::Line(p) => Seg::Line(self.apply(p)),
                Seg::Cubic(a, b, p) => Seg::Cubic(self.apply(a), self.apply(b), self.apply(p)),
                Seg::Close => Seg::Close,
            })
            .collect()
    }
}

pub fn add(a: Xy, b: Xy) -> Xy {
    [a[0] + b[0], a[1] + b[1]]
}
pub fn sub(a: Xy, b: Xy) -> Xy {
    [a[0] - b[0], a[1] - b[1]]
}
pub fn mul(a: Xy, k: f64) -> Xy {
    [a[0] * k, a[1] * k]
}
pub fn dot(a: Xy, b: Xy) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
pub fn len(a: Xy) -> f64 {
    a[0].hypot(a[1])
}
pub fn normalize(a: Xy) -> Xy {
    mul(a, 1. / len(a))
}
/// Perpendicular, a quarter turn clockwise on the page.
pub fn perp(a: Xy) -> Xy {
    [-a[1], a[0]]
}

/// Parse SVG path data (M L H V C S Q T A Z, absolute or relative).
pub fn parse_path(data: &str) -> Result<Vec<Seg>, String> {
    use svgtypes::SimplePathSegment as S;
    let mut segs = Vec::new();
    let mut current = [0., 0.];
    for segment in svgtypes::SimplifyingPathParser::from(data) {
        let segment = segment.map_err(|e| e.to_string())?;
        segs.push(match segment {
            S::MoveTo { x, y } => Seg::Move([x, y]),
            S::LineTo { x, y } => Seg::Line([x, y]),
            S::CurveTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => Seg::Cubic([x1, y1], [x2, y2], [x, y]),
            S::Quadratic { x1, y1, x, y } => {
                let q = [x1, y1];
                let end = [x, y];
                Seg::Cubic(
                    add(current, mul(sub(q, current), 2. / 3.)),
                    add(end, mul(sub(q, end), 2. / 3.)),
                    end,
                )
            }
            S::ClosePath => Seg::Close,
        });
        current = match segs.last() {
            Some(Seg::Move(p) | Seg::Line(p) | Seg::Cubic(_, _, p)) => *p,
            _ => current,
        };
        if matches!(segs.last(), Some(Seg::Close)) {
            current = subpath_start(&segs);
        }
    }
    if segs.is_empty() {
        return Err("it is empty".into());
    }
    Ok(segs)
}

fn subpath_start(segs: &[Seg]) -> Xy {
    segs.iter()
        .rev()
        .find_map(|s| match s {
            Seg::Move(p) => Some(*p),
            _ => None,
        })
        .unwrap_or([0., 0.])
}

/// Flattened subpaths, each with whether it was closed.
pub fn flatten(segs: &[Seg]) -> Vec<(Vec<Xy>, bool)> {
    let mut out: Vec<(Vec<Xy>, bool)> = Vec::new();
    for s in segs {
        match *s {
            Seg::Move(p) => out.push((vec![p], false)),
            Seg::Line(p) => {
                if let Some(last) = out.last_mut() {
                    last.0.push(p);
                }
            }
            Seg::Cubic(a, b, p) => {
                if let Some(last) = out.last_mut() {
                    let p0 = *last.0.last().unwrap_or(&p);
                    let steps = ((len(sub(a, p0)) + len(sub(b, a)) + len(sub(p, b))) / 0.5)
                        .ceil()
                        .clamp(2., 64.) as usize;
                    for i in 1..=steps {
                        let t = i as f64 / steps as f64;
                        let m = 1. - t;
                        last.0.push([
                            m * m * m * p0[0]
                                + 3. * m * m * t * a[0]
                                + 3. * m * t * t * b[0]
                                + t * t * t * p[0],
                            m * m * m * p0[1]
                                + 3. * m * m * t * a[1]
                                + 3. * m * t * t * b[1]
                                + t * t * t * p[1],
                        ]);
                    }
                }
            }
            Seg::Close => {
                if let Some(last) = out.last_mut() {
                    last.1 = true;
                }
            }
        }
    }
    out
}

/// The edges of a polyline, with the closing edge when `closed`.
pub fn edges(points: &[Xy], closed: bool) -> impl Iterator<Item = (Xy, Xy)> + '_ {
    let n = if closed {
        points.len()
    } else {
        points.len().saturating_sub(1)
    };
    points
        .iter()
        .copied()
        .zip(points.iter().copied().cycle().skip(1))
        .take(n)
}

/// Distance from `p` to the outline (every edge, closing edges included for closed subpaths).
pub fn distance_to_outline(p: Xy, outline: &[(Vec<Xy>, bool)]) -> f64 {
    let mut best = f64::INFINITY;
    for (points, closed) in outline {
        for (a, b) in edges(points, *closed) {
            best = best.min(segment_distance(p, a, b));
        }
        if let [only] = points.as_slice() {
            best = best.min(len(sub(p, *only)));
        }
    }
    best
}

fn segment_distance(p: Xy, a: Xy, b: Xy) -> f64 {
    let ab = sub(b, a);
    let l2 = dot(ab, ab);
    let t = if l2 == 0. {
        0.
    } else {
        (dot(sub(p, a), ab) / l2).clamp(0., 1.)
    };
    len(sub(p, add(a, mul(ab, t))))
}

/// Even-odd inside test over all subpaths, each treated as closed.
pub fn inside(p: Xy, outline: &[(Vec<Xy>, bool)]) -> bool {
    let mut odd = false;
    for (points, _) in outline {
        for (a, b) in edges(points, true) {
            if (a[1] > p[1]) != (b[1] > p[1])
                && p[0] < a[0] + (p[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1])
            {
                odd = !odd;
            }
        }
    }
    odd
}

/// Signed area of a closed polygon (positive clockwise on the page, y down).
pub fn area(points: &[Xy]) -> f64 {
    edges(points, true)
        .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
        .sum::<f64>()
        / 2.
}

pub fn polygon(points: &[Xy], closed: bool) -> Vec<Seg> {
    let mut segs: Vec<Seg> = points
        .iter()
        .enumerate()
        .map(|(i, p)| if i == 0 { Seg::Move(*p) } else { Seg::Line(*p) })
        .collect();
    if closed {
        segs.push(Seg::Close);
    }
    segs
}
