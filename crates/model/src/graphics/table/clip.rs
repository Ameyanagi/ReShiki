//! Pointer geometry clips ink before applying the editor's pick radius.
//! Curves use the existing bounded flattening; round stroke caps/joins use
//! 32-sided inscribed disks. Dash gaps retain the editor's solid-envelope hit
//! convention, rather than becoming a per-pixel paint certificate.
use super::Frame;
use crate::{
    document::Point,
    graphics::{PathCommand, flattened, segment_distance},
    scientific::Part,
};
use i_overlay::{
    core::{fill_rule::FillRule, overlay_rule::OverlayRule},
    float::overlay::FloatOverlay,
};

/// Pick actual border ink, including the interior of filled affine miters.
pub fn border_hit(graphic: &super::Graphic, point: Point, radius: f32) -> bool {
    graphic.parts().iter().any(|part| {
        let paths = flattened(&part.commands);
        (part.filled && part.style.fill.is_some() && crate::graphics::fills(point, &paths))
            || paths.iter().any(|path| {
                path.windows(2).any(|pair| {
                    matches!(pair, [a, b] if segment_distance(point, *a, *b) < radius + part.style.width() * 0.5)
                })
            })
    })
}

pub fn segment(frame: Frame, from: Point, to: Point) -> Option<(Point, Point)> {
    let a = frame.local(from)?;
    let b = frame.local(to)?;
    let (mut enter, mut leave) = (0_f32, 1_f32);
    for (at, delta) in [(a.x, b.x - a.x), (a.y, b.y - a.y)] {
        if delta == 0. {
            if !(0. ..=1.).contains(&at) {
                return None;
            }
        } else {
            let (a, b) = (-at / delta, (1. - at) / delta);
            enter = enter.max(a.min(b));
            leave = leave.min(a.max(b));
            if enter > leave {
                return None;
            }
        }
    }
    Some((from.lerp(to, enter), from.lerp(to, leave)))
}

fn polygon(frame: Frame, points: &[Point]) -> Vec<Point> {
    let Some(mut points) = points
        .iter()
        .map(|p| frame.local(*p))
        .collect::<Option<Vec<_>>>()
    else {
        return vec![];
    };
    for (axis, edge, sign) in [(0, 0., 1.), (0, 1., -1.), (1, 0., 1.), (1, 1., -1.)] {
        let distance = |p: Point| {
            let value = if axis == 0 { p.x } else { p.y };
            (value - edge) * sign
        };
        let mut clipped = Vec::new();
        for (a, b) in crate::polygon::closed_edges(&points) {
            let (da, db) = (distance(a), distance(b));
            if (da >= 0.) != (db >= 0.) {
                clipped.push(a.lerp(b, da / (da - db)));
            }
            if db >= 0. {
                clipped.push(b);
            }
        }
        points = clipped;
        if points.is_empty() {
            break;
        }
    }
    points.into_iter().map(|p| frame.point(p)).collect()
}

fn crossed(a: Point, b: Point, c: Point, d: Point, radius: f32) -> bool {
    if segment_distance(a, c, d)
        .min(segment_distance(b, c, d))
        .min(segment_distance(c, a, b))
        .min(segment_distance(d, a, b))
        <= radius
    {
        return true;
    }
    let (ux, uy, vx, vy) = (b.x - a.x, b.y - a.y, d.x - c.x, d.y - c.y);
    let det = ux * vy - uy * vx;
    if det.abs() < 1e-8 {
        return false;
    }
    let t = ((c.x - a.x) * vy - (c.y - a.y) * vx) / det;
    let s = ((c.x - a.x) * uy - (c.y - a.y) * ux) / det;
    (0. ..=1.).contains(&t) && (0. ..=1.).contains(&s)
}

pub fn polygon_hit(frame: Frame, points: &[Point], from: Point, to: Point, radius: f32) -> bool {
    let points = polygon(frame, points);
    if points.len() < 3 || crate::polygon::signed_area(&points).abs() < 1e-10 {
        return false;
    }
    let inside = |p: Point| {
        let mut inside = false;
        for (a, b) in crate::polygon::closed_edges(&points) {
            if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
                inside = !inside;
            }
        }
        inside
    };
    inside(from)
        || inside(to)
        || crate::polygon::closed_edges(&points).any(|(a, b)| crossed(from, to, a, b, radius))
}

pub fn box_hit(frame: Frame, lo: Point, hi: Point, from: Point, to: Point, radius: f32) -> bool {
    polygon_hit(
        frame,
        &[lo, Point::new(hi.x, lo.y), hi, Point::new(lo.x, hi.y)],
        from,
        to,
        radius,
    )
}

pub fn line_hit(
    frame: Frame,
    a: Point,
    b: Point,
    half: f32,
    from: Point,
    to: Point,
    radius: f32,
) -> bool {
    path_hit(
        frame,
        &[PathCommand::Move(a), PathCommand::Line(b)],
        half,
        false,
        true,
        from,
        to,
        radius,
    )
}

pub fn part_hit(frame: Frame, part: &Part, from: Point, to: Point, radius: f32) -> bool {
    path_hit(
        frame,
        &part.commands,
        part.style.width() * 0.5,
        part.filled && part.style.fill.is_some(),
        part.style.round_caps(),
        from,
        to,
        radius,
    )
}

pub fn arrow_hit(
    frame: Frame,
    part: &crate::arrows::ArrowPath,
    from: Point,
    to: Point,
    radius: f32,
) -> bool {
    path_hit(
        frame,
        &part.commands,
        part.style.width() * 0.5,
        part.filled && part.style.fill.is_some(),
        part.style.round_caps(),
        from,
        to,
        radius,
    )
}

/// Resolve the complete nonzero fill before intersecting the cell. Clipping
/// each contour independently would turn a hole into a filled contour and
/// invent cell-edge ink when the cell lies entirely inside that hole.
fn fill_hit(frame: Frame, paths: &[Vec<Point>], from: Point, to: Point, radius: f32) -> bool {
    let Some(subject): Option<Vec<Vec<[f64; 2]>>> = paths
        .iter()
        .filter(|path| path.len() >= 3)
        .map(|path| {
            path.iter()
                .map(|p| frame.local(*p).map(|p| [f64::from(p.x), f64::from(p.y)]))
                .collect()
        })
        .collect()
    else {
        return false;
    };
    if subject.is_empty() {
        return false;
    }
    let cell = vec![[0., 0.], [1., 0.], [1., 1.], [0., 1.]];
    FloatOverlay::with_subj_and_clip(&subject, &cell)
        .overlay(OverlayRule::Intersect, FillRule::NonZero)
        .into_iter()
        .any(|shape| {
            let contours: Vec<Vec<Point>> = shape
                .into_iter()
                .map(|contour| {
                    contour
                        .into_iter()
                        .map(|p| frame.point(Point::new(p[0] as f32, p[1] as f32)))
                        .collect()
                })
                .collect();
            crate::graphics::fills(from, &contours)
                || crate::graphics::fills(to, &contours)
                || contours.iter().any(|contour| {
                    crate::polygon::closed_edges(contour)
                        .any(|(a, b)| crossed(from, to, a, b, radius))
                })
        })
}

#[allow(clippy::too_many_arguments)]
pub fn path_hit(
    frame: Frame,
    commands: &[PathCommand],
    half: f32,
    filled: bool,
    round: bool,
    from: Point,
    to: Point,
    radius: f32,
) -> bool {
    let paths = flattened(commands);
    if filled && fill_hit(frame, &paths, from, to, radius) {
        return true;
    }
    paths.iter().any(|path| {
        if half <= 0. {
            return false;
        }
        if path.windows(2).any(|pair| {
            let [a, b] = pair else {
                return false;
            };
            let length = a.distance(*b);
            if length < 1e-8 {
                return false;
            }
            let (dx, dy) = ((b.x - a.x) / length * half, (b.y - a.y) / length * half);
            polygon_hit(
                frame,
                &[
                    a.offset(-dy, dx),
                    b.offset(-dy, dx),
                    b.offset(dy, -dx),
                    a.offset(dy, -dx),
                ],
                from,
                to,
                radius,
            )
        }) {
            return true;
        }
        path.iter().enumerate().any(|(i, p)| {
            if !round && (i == 0 || i + 1 == path.len()) {
                return false;
            }
            let circle: Vec<_> = (0..32)
                .map(|i| {
                    let angle = i as f32 * std::f32::consts::TAU / 32.;
                    p.offset(half * angle.cos(), half * angle.sin())
                })
                .collect();
            polygon_hit(frame, &circle, from, to, radius)
        })
    })
}

#[cfg(test)]
mod tests;
