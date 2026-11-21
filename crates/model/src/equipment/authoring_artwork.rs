//! A bounded certificate for filled artwork without independent stroke-only traces.
use super::{PathCommand, Point};

const MAX_SEGMENTS: usize = 256;
type P = [i64; 2];
type Hull = [P; 4];

struct Segment {
    hull: Hull,
    contour: usize,
}

/// Every segment is injective along its chord, and segment hulls cannot
/// intersect except at neighboring endpoints. Each resulting Jordan contour
/// borders a nonzero fill on at least one side, including holes. Uncertain
/// or overly complex artwork is refused; sampling cannot certify a cubic.
pub(super) fn certified(commands: &[PathCommand]) -> bool {
    let max = commands
        .iter()
        .flat_map(PathCommand::points)
        .map(|p| f64::from(p.x).abs().max(f64::from(p.y).abs()))
        .fold(0_f64, f64::max);
    if !max.is_finite() || max == 0. {
        return false;
    }
    // f32 coordinates are dyadic rationals. A common exact grid keeps
    // coordinates within 50 bits, differences within 51, and every i128
    // dot product below 104 bits. Refuse excessive precision ranges.
    let scale = 2_f64.powi(50 - max.log2().ceil() as i32);
    let mut segments: Vec<Segment> = Vec::new();
    let mut ranges = Vec::new();
    let mut start = None;
    let mut at = [0; 2];
    let mut first = 0;
    for command in commands {
        let hull = match command {
            PathCommand::Move(point) if start.is_none() => {
                let Some(point) = integer_point(*point, scale) else {
                    return false;
                };
                at = point;
                start = Some(at);
                first = segments.len();
                continue;
            }
            PathCommand::Line(point) if start.is_some() => {
                let Some(point) = integer_point(*point, scale) else {
                    return false;
                };
                [at, at, point, point]
            }
            PathCommand::Cubic(a, b, point) if start.is_some() => {
                let Some(a) = integer_point(*a, scale) else {
                    return false;
                };
                let Some(b) = integer_point(*b, scale) else {
                    return false;
                };
                let Some(point) = integer_point(*point, scale) else {
                    return false;
                };
                [at, a, b, point]
            }
            PathCommand::Close if start.is_some() => {
                let end = start.take().unwrap_or(at);
                [at, at, end, end]
            }
            _ => return false,
        };
        at = hull[3];
        if !hull.iter().all(|p| *p == hull[0]) {
            if segments.len() == MAX_SEGMENTS || !monotone(&hull) {
                return false;
            }
            segments.push(Segment {
                hull,
                contour: ranges.len(),
            });
        }
        if start.is_none() {
            if segments.len() - first < 2 {
                return false;
            }
            ranges.push(first..segments.len());
        }
    }
    if start.is_some() || segments.is_empty() {
        return false;
    }
    for (i, a) in segments.iter().enumerate() {
        for (j, b) in segments.iter().enumerate().skip(i + 1) {
            let shared = if a.contour == b.contour {
                let Some(range) = ranges.get(a.contour) else {
                    return false;
                };
                if j == i + 1 {
                    Some(a.hull[3])
                } else if i == range.start && j + 1 == range.end {
                    Some(a.hull[0])
                } else {
                    None
                }
            } else {
                None
            };
            if !separated(&a.hull, &b.hull, shared) {
                return false;
            }
        }
    }
    true
}

fn integer_point(p: Point, scale: f64) -> Option<P> {
    let (x, y) = (f64::from(p.x) * scale, f64::from(p.y) * scale);
    if !x.is_finite()
        || !y.is_finite()
        || x.abs().max(y.abs()) > (1_u64 << 50) as f64
        || x.fract() != 0.
        || y.fract() != 0.
    {
        return None;
    }
    Some([x as i64, y as i64])
}

fn sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1]]
}
fn dot(a: P, b: P) -> i128 {
    i128::from(a[0]) * i128::from(b[0]) + i128::from(a[1]) * i128::from(b[1])
}

fn monotone(hull: &Hull) -> bool {
    let chord = sub(hull[3], hull[0]);
    if dot(chord, chord) == 0 {
        return false;
    }
    hull.windows(2).all(|pair| {
        let Some([a, b]) = pair.first_chunk::<2>() else {
            return false;
        };
        dot(sub(*b, *a), chord) >= 0
    })
}

fn projected(hull: &Hull, axis: P, origin: P) -> (i128, i128) {
    hull.iter()
        .map(|p| dot(sub(*p, origin), axis))
        .fold((i128::MAX, i128::MIN), |(lo, hi), p| (lo.min(p), hi.max(p)))
}

fn separated(a: &Hull, b: &Hull, shared: Option<P>) -> bool {
    if let Some(at) = shared {
        // Edge normals alone cannot split an ordinary right-angle corner.
        // Bisect its endpoint rays, independently of their lengths.
        for p in a {
            for q in b {
                let u = sub(*p, at);
                let v = sub(*q, at);
                let (un, vn) = (
                    (u[0] as f64).hypot(u[1] as f64),
                    (v[0] as f64).hypot(v[1] as f64),
                );
                if un > 0. && vn > 0.
                    // Only the candidate direction is approximate; its
                    // separating projections are checked exactly below.
                    && along(a, b, [
                        ((v[0] as f64 / vn - u[0] as f64 / un) * (1_u64 << 30) as f64).round() as i64,
                        ((v[1] as f64 / vn - u[1] as f64 / un) * (1_u64 << 30) as f64).round() as i64,
                    ], shared)
                {
                    return true;
                }
            }
        }
    }
    for hull in [a, b] {
        for (i, p) in hull.iter().enumerate() {
            for q in hull.iter().skip(i + 1) {
                let d = sub(*q, *p);
                for axis in [d, [-d[1], d[0]]] {
                    if along(a, b, axis, shared) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn along(a: &Hull, b: &Hull, axis: P, shared: Option<P>) -> bool {
    if axis == [0, 0] {
        return false;
    }
    let origin = shared.unwrap_or(a[0]);
    let (al, ah) = projected(a, axis, origin);
    let (bl, bh) = projected(b, axis, origin);
    if shared.is_some() {
        (ah <= 0 && bl >= 0 && al < 0 && bh > 0) || (bh <= 0 && al >= 0 && bl < 0 && ah > 0)
    } else {
        ah < bl || bh < al
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::equipment::geometry::{Seg, parse_path};

    fn path(data: &str) -> Vec<PathCommand> {
        let p = |p: [f64; 2]| Point::new(p[0] as f32, p[1] as f32);
        parse_path(data)
            .unwrap()
            .into_iter()
            .map(|segment| match segment {
                Seg::Move(a) => PathCommand::Move(p(a)),
                Seg::Line(a) => PathCommand::Line(p(a)),
                Seg::Cubic(a, b, c) => PathCommand::Cubic(p(a), p(b), p(c)),
                Seg::Close => PathCommand::Close,
            })
            .collect()
    }

    #[test]
    fn ordinary_filled_artwork_is_certified_without_sampling() {
        for data in [
            "M0 0H40V30H0Z",
            "M0 0H40V30H0Z M60 0H80V20H60Z",
            "M0 0H40V30H0Z M10 10V20H30V10Z", // a hole
            "M0 0H10V20H30V30H0Z",            // concave artwork
            "M10 0C10 5.522848 5.522848 10 0 10C-5.522848 10 -10 5.522848 -10 0C-10 -5.522848 -5.522848 -10 0 -10C5.522848 -10 10 -5.522848 10 0Z",
            "M0 -2H40C41.10457 -2 42 -1.10457 42 0C42 1.10457 41.10457 2 40 2H0C-1.10457 2 -2 1.10457 -2 0C-2 -1.10457 -1.10457 -2 0 -2Z",
            "M-10 0C-10 -10 10 -10 10 0C10 10 -10 10 -10 0Z",
        ] {
            let commands = path(data);
            assert!(certified(&commands), "{data}");
            let t = |p: Point| Point::new(1000. - 1.7 * p.x, -500. + 0.7 * p.y);
            let transformed: Vec<_> = commands
                .iter()
                .map(|c| match c {
                    PathCommand::Move(p) => PathCommand::Move(t(*p)),
                    PathCommand::Line(p) => PathCommand::Line(t(*p)),
                    PathCommand::Cubic(a, b, c) => PathCommand::Cubic(t(*a), t(*b), t(*c)),
                    PathCommand::Close => PathCommand::Close,
                })
                .collect();
            assert!(certified(&transformed), "transformed {data}");
        }
    }

    #[test]
    fn stroke_only_retraced_and_uncertain_traces_are_refused() {
        for data in [
            "M0 0L40 0",
            "M0 0L40 0Z",
            "M0 0H40V30H0Z M50 0L80 0",
            "M0 0H40V30H0Z L80 0",
            "M0 0H40V30H0Z M0 0V30H40V0Z",
            "M0 0H40V30H0L0 0L-20 0L0 0Z",
            "M0 0H40V30H0Z M60 0H80V20H60Z M60 0V20H80V0Z",
            "M0 0L30 30L0 30L30 0Z",
            "M0 0C40 40 -40 40 0 0Z",
            // A simple curved cap is deliberately outside this certificate:
            // its closing line has no strict extent on one side.
            "M0 0C0 -20 40 -20 40 0Z",
        ] {
            assert!(!certified(&path(data)), "{data}");
        }
        let mut too_many = vec![PathCommand::Move(Point::default())];
        too_many.extend((1..=MAX_SEGMENTS).map(|n| PathCommand::Line(Point::new(n as f32, 1.))));
        too_many.push(PathCommand::Close);
        assert!(!certified(&too_many));
        assert!(!certified(&[
            PathCommand::Move(Point::new(f32::INFINITY, 0.)),
            PathCommand::Close
        ]));
        assert!(integer_point(Point::new(2_f32.powi(-100), 1.), (1_u64 << 50) as f64).is_none());
    }
}
