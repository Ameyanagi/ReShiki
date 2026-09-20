//! Drawing depth at crossings; never changes chemical connectivity.
use crate::{
    document::{Document, Point},
    scene::Primitive,
    style::{DEFAULT, DrawingStyle},
};
#[derive(Debug, Clone, Copy)]
pub struct Gap {
    origin: Point,
    axis: Point,
    low: f32,
    high: f32,
}
fn cross(a: Point, b: Point) -> f32 {
    a.x * b.y - a.y * b.x
}
fn delta(a: Point, b: Point) -> Point {
    Point::new(b.x - a.x, b.y - a.y)
}
fn thickness(bond: &crate::document::Bond, style: &DrawingStyle) -> f32 {
    let width =
        if ["bold", "wedge", "hollow_wedge", "hashed", "hash"].contains(&bond.display.as_str()) {
            style.world(style.bold_width_pt)
        } else {
            style.line_width()
        };
    let lines = match bond.order {
        2 | 4 | 7 => 1.,
        3 => 2.,
        6 => 3.,
        _ => 0.,
    };
    width + lines * style.bond_length_world * style.bond_spacing_ratio
}
/// Sorted sweep avoids checking every pair of well-separated molecules.
pub fn gaps(doc: &Document) -> Vec<Vec<Gap>> {
    let mut gaps = vec![vec![]; doc.bonds.len()];
    let mut segments: Vec<_> = doc
        .bonds
        .iter()
        .enumerate()
        .filter_map(|(i, b)| {
            if !doc.bond_visible(b.a, b.b) {
                return None;
            }
            Some((i, doc.atom(b.a)?.position, doc.atom(b.b)?.position))
        })
        .collect();
    segments.sort_by(|(_, a, b), (_, c, d)| a.x.min(b.x).total_cmp(&c.x.min(d.x)));
    let mut active: Vec<(usize, Point, Point)> = vec![];
    let mut budget = 200_000_usize;
    for (i, a, b) in segments {
        active.retain(|(_, c, d)| c.x.max(d.x) >= a.x.min(b.x));
        for (j, c, d) in &active {
            if budget == 0 {
                return gaps;
            }
            budget -= 1;
            if a.y.min(b.y) >= c.y.max(d.y) || a.y.max(b.y) <= c.y.min(d.y) {
                continue;
            }
            let (Some(first), Some(second)) = (doc.bonds.get(i), doc.bonds.get(*j)) else {
                continue;
            };
            if [first.a, first.b]
                .iter()
                .any(|id| *id == second.a || *id == second.b)
            {
                continue;
            }
            let ab = delta(a, b);
            let cd = delta(*c, *d);
            let ac = delta(a, *c);
            let determinant = cross(ab, cd);
            if determinant.abs() < 0.001 {
                continue;
            }
            let t = cross(ac, cd) / determinant;
            let u = cross(ac, ab) / determinant;
            if !(0.04..0.96).contains(&t) || !(0.04..0.96).contains(&u) {
                continue;
            }
            let first_over = (first.z_order, i) > (second.z_order, *j);
            let (under, from, to, parameter, over) = if first_over {
                (*j, *c, *d, u, first)
            } else {
                (i, a, b, t, second)
            };
            let length = from.distance(to);
            let other_length = if first_over {
                a.distance(b)
            } else {
                c.distance(*d)
            };
            let sine = determinant.abs() / (length * other_length).max(0.001);
            let half = ((thickness(over, &doc.drawing_style) * 0.5 + DEFAULT.world(1.1))
                / sine.max(0.15))
            .min(length * 0.22);
            if let Some(list) = gaps.get_mut(under) {
                list.push(Gap {
                    origin: from,
                    axis: Point::new((to.x - from.x) / length, (to.y - from.y) / length),
                    low: parameter * length - half,
                    high: parameter * length + half,
                });
            }
        }
        active.push((i, a, b));
    }
    gaps
}
fn projection(p: Point, g: Gap) -> f32 {
    (p.x - g.origin.x) * g.axis.x + (p.y - g.origin.y) * g.axis.y
}
fn intersection(a: Point, b: Point, pa: f32, pb: f32, bound: f32) -> Point {
    let t = if (pb - pa).abs() > 0.00001 {
        (bound - pa) / (pb - pa)
    } else {
        0.
    };
    Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}
fn half(primitive: &Primitive, g: Gap, bound: f32, below: bool) -> Option<Primitive> {
    let inside = |value: f32| {
        if below {
            value <= bound
        } else {
            value >= bound
        }
    };
    match primitive {
        Primitive::Line(a, b, width) => {
            let pa = projection(*a, g);
            let pb = projection(*b, g);
            match (inside(pa), inside(pb)) {
                (true, true) => Some(primitive.clone()),
                (false, false) => None,
                (true, false) => Some(Primitive::Line(
                    *a,
                    intersection(*a, *b, pa, pb, bound),
                    *width,
                )),
                (false, true) => Some(Primitive::Line(
                    intersection(*a, *b, pa, pb, bound),
                    *b,
                    *width,
                )),
            }
        }
        Primitive::Polygon(points) => {
            let mut clipped = vec![];
            for (a, b) in points
                .iter()
                .zip(points.iter().cycle().skip(1))
                .take(points.len())
            {
                let pa = projection(*a, g);
                let pb = projection(*b, g);
                if inside(pa) {
                    clipped.push(*a);
                }
                if inside(pa) != inside(pb) {
                    clipped.push(intersection(*a, *b, pa, pb, bound));
                }
            }
            (clipped.len() >= 3).then_some(Primitive::Polygon(clipped))
        }
        _ => Some(primitive.clone()),
    }
}
/// Trim real geometry, leaving transparent gaps in vector and raster exports.
pub fn cut(mut primitives: Vec<Primitive>, gaps: &[Gap]) -> Vec<Primitive> {
    for gap in gaps {
        primitives = primitives
            .iter()
            .flat_map(|p| {
                let halves = if matches!(p, Primitive::Line(..) | Primitive::Polygon(_)) {
                    [half(p, *gap, gap.low, true), half(p, *gap, gap.high, false)]
                } else {
                    [Some(p.clone()), None]
                };
                halves.into_iter().flatten()
            })
            .collect();
    }
    primitives
}
