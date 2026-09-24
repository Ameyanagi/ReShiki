//! Crossing clearance for cubic aromatic circles and partial ring curves.
use super::{Gap, cross, delta, thickness};
use crate::{
    aromatic::Circle,
    document::{Document, Point},
    graphics::{PathCommand, flattened},
    scene::Primitive,
    style::DEFAULT,
};

#[derive(Clone, Copy)]
struct Mask {
    origin: Point,
    axis: Point,
    length: f32,
    half: f32,
}
impl Mask {
    fn local(self, p: Point) -> Point {
        let d = delta(self.origin, p);
        Point::new(
            d.x * self.axis.x + d.y * self.axis.y,
            -d.x * self.axis.y + d.y * self.axis.x,
        )
    }
    fn inside(self, p: Point) -> bool {
        let p = self.local(p);
        p.x >= 0. && p.x <= self.length && p.y.abs() <= self.half
    }
}
fn split([p, a, b, q]: [Point; 4], t: f32) -> ([Point; 4], [Point; 4]) {
    let lerp = |a: Point, b: Point| a.offset((b.x - a.x) * t, (b.y - a.y) * t);
    let (pa, ab, bq) = (lerp(p, a), lerp(a, b), lerp(b, q));
    let (left, right) = (lerp(pa, ab), lerp(ab, bq));
    let center = lerp(left, right);
    ([p, pa, left, center], [center, right, bq, q])
}
fn keep((lo, hi): (f32, f32), out: &mut Vec<(f32, f32)>) {
    if let Some((_, end)) = out.last_mut()
        && *end == lo
    {
        *end = hi;
    } else {
        out.push((lo, hi));
    }
}
fn outside(curve: [Point; 4], mask: Mask, depth: u8, range: (f32, f32), out: &mut Vec<(f32, f32)>) {
    let points = curve.map(|p| mask.local(p));
    // The Bézier curve lies in its control hull. Keep parameter intervals when
    // that hull misses the finite bond mask, preserving smooth vector output.
    if points.iter().all(|p| p.x < 0.)
        || points.iter().all(|p| p.x > mask.length)
        || points.iter().all(|p| p.y < -mask.half)
        || points.iter().all(|p| p.y > mask.half)
    {
        keep(range, out);
    } else if !curve.iter().all(|p| mask.inside(*p)) {
        let (left, right) = split(curve, 0.5);
        if depth >= 18 {
            if left.last().is_some_and(|p| !mask.inside(*p)) {
                keep(range, out);
            }
        } else {
            let (lo, hi) = range;
            let middle = (lo + hi) * 0.5;
            outside(left, mask, depth + 1, (lo, middle), out);
            outside(right, mask, depth + 1, (middle, hi), out);
        }
    }
}
fn cut_curve(commands: &[PathCommand], mask: Mask) -> Vec<PathCommand> {
    let mut out = Vec::new();
    let mut cursor = None;
    let mut first = None;
    let mut pen = None;
    for command in commands {
        let (a, b, end) = match *command {
            PathCommand::Move(p) => {
                cursor = Some(p);
                first = Some(p);
                continue;
            }
            PathCommand::Cubic(a, b, p) => (a, b, p),
            PathCommand::Line(p) => {
                let Some(start) = cursor else { continue };
                (start, p, p)
            }
            PathCommand::Close => {
                let (Some(start), Some(end)) = (cursor, first) else {
                    continue;
                };
                if start.distance(end) < 0.0001 {
                    continue;
                }
                (start, end, end)
            }
        };
        let Some(start) = cursor.replace(end) else {
            continue;
        };
        let mut pieces = Vec::new();
        let curve = [start, a, b, end];
        outside(curve, mask, 0, (0., 1.), &mut pieces);
        for (lo, hi) in pieces {
            let prefix = if hi == 1. { curve } else { split(curve, hi).0 };
            let [from, a, b, to] = if lo == 0. {
                prefix
            } else {
                split(prefix, lo / hi).1
            };
            if pen != Some(from) {
                out.push(PathCommand::Move(from));
            }
            out.push(PathCommand::Cubic(a, b, to));
            pen = Some(to);
        }
    }
    out
}

/// Return the visible curve and any gaps needed in bonds behind it. Ring
/// outlines themselves are excluded; the circle inherits its ring's depth.
pub(crate) fn ring_stroke(
    doc: &Document,
    ring: &Circle,
    mut stroke: Primitive,
) -> (Primitive, Vec<(usize, Gap)>) {
    let Primitive::Path {
        commands,
        style,
        filled: false,
    } = &mut stroke
    else {
        return (stroke, Vec::new());
    };
    let paths = flattened(commands);
    let bounds =
        commands
            .iter()
            .flat_map(PathCommand::points)
            .fold(None::<(Point, Point)>, |bounds, p| {
                Some(match bounds {
                    None => (p, p),
                    Some((lo, hi)) => (
                        Point::new(lo.x.min(p.x), lo.y.min(p.y)),
                        Point::new(hi.x.max(p.x), hi.y.max(p.y)),
                    ),
                })
            });
    let Some((lo, hi)) = bounds else {
        return (stroke, Vec::new());
    };
    let rank = doc
        .bonds
        .iter()
        .enumerate()
        .filter(|(_, b)| ring.contains_bond(b.a, b.b))
        .map(|(i, b)| (b.z_order, i))
        .max()
        .unwrap_or_default();
    let mut gaps = Vec::new();
    for (index, bond) in doc.bonds.iter().enumerate() {
        if ring.contains_bond(bond.a, bond.b) || !doc.bond_visible(bond.a, bond.b) {
            continue;
        }
        let (Some(a), Some(b)) = (doc.atom(bond.a), doc.atom(bond.b)) else {
            continue;
        };
        let (a, b) = (a.position, b.position);
        if a.x.min(b.x) > hi.x || a.x.max(b.x) < lo.x || a.y.min(b.y) > hi.y || a.y.max(b.y) < lo.y
        {
            continue;
        }
        let length = a.distance(b);
        if length < 0.1 {
            continue;
        }
        let ab = delta(a, b);
        let mut hits = Vec::new();
        for edge in paths.iter().flat_map(|path| path.windows(2)) {
            let [c, d] = edge else { continue };
            let cd = delta(*c, *d);
            let determinant = cross(ab, cd);
            if determinant.abs() < 0.00001 {
                continue;
            }
            let ac = delta(a, *c);
            let t = cross(ac, cd) / determinant;
            let u = cross(ac, ab) / determinant;
            if (0.04..0.96).contains(&t) && (0.0..=1.0).contains(&u) {
                let sine = determinant.abs() / (length * c.distance(*d)).max(0.0001);
                hits.push((t, sine));
            }
        }
        if hits.is_empty() {
            continue;
        }
        let axis = Point::new(ab.x / length, ab.y / length);
        if (bond.z_order, index) > rank {
            *commands = cut_curve(
                commands,
                Mask {
                    origin: a,
                    axis,
                    length,
                    half: thickness(bond, &doc.drawing_style) * 0.5 + DEFAULT.world(1.1),
                },
            );
        } else {
            for (t, sine) in hits {
                let half = ((doc.drawing_style.world(style.width_pt) * 0.5 + DEFAULT.world(1.1))
                    / sine.max(0.15))
                .min(length * 0.22);
                gaps.push((
                    index,
                    Gap {
                        origin: a,
                        axis,
                        low: t * length - half,
                        high: t * length + half,
                    },
                ));
            }
        }
    }
    (stroke, gaps)
}
