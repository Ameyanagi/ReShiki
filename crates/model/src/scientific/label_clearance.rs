//! Trim orbital ink around visible atom-label ink without changing the saved
//! orbital frame or painting opaque paper-colored backgrounds.
use super::Part;
use crate::{
    document::Point,
    graphics::{Graphic, PathCommand},
    style::DEFAULT,
};
type Box2 = (Point, Point);

pub(crate) fn readable_orbital_parts(graphic: &Graphic, labels: &[Box2]) -> Vec<Part> {
    if labels.is_empty() {
        return graphic.parts();
    }
    let padding = DEFAULT.world(0.7) + graphic.style.width() * 0.5;
    let boxes: Vec<_> = labels
        .iter()
        .map(|(lo, hi)| (lo.offset(-padding, -padding), hi.offset(padding, padding)))
        .collect();
    graphic
        .parts()
        .into_iter()
        .flat_map(|part| trim(part, &boxes))
        .collect()
}

fn trim(part: Part, boxes: &[Box2]) -> Vec<Part> {
    let paths = flatten(&part.commands);
    let relevant: Vec<_> = boxes
        .iter()
        .copied()
        .filter(|(lo, hi)| {
            paths
                .iter()
                .flatten()
                .any(|p| p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y)
                || paths.iter().any(|points| {
                    points.windows(2).any(|edge| {
                        let [a, b] = edge else {
                            return false;
                        };
                        inside_interval(*a, *b, (*lo, *hi)).is_some()
                    })
                })
                || (part.filled
                    && paths.iter().any(|points| {
                        contains(points, lo.offset((hi.x - lo.x) * 0.5, (hi.y - lo.y) * 0.5))
                    }))
        })
        .collect();
    if relevant.is_empty() {
        return vec![part];
    }
    let mut out = Vec::new();
    if part.filled {
        let mut polygons = paths.clone();
        for bounds in &relevant {
            polygons = polygons
                .into_iter()
                .flat_map(|points| outside_polygon(&points, *bounds))
                .collect();
        }
        let mut commands = Vec::new();
        for points in polygons.into_iter().filter(|points| points.len() >= 3) {
            if let Some(first) = points.first() {
                commands.push(PathCommand::Move(*first));
            }
            commands.extend(points.into_iter().skip(1).map(PathCommand::Line));
            commands.push(PathCommand::Close);
        }
        if !commands.is_empty() {
            out.push(Part {
                commands,
                style: crate::graphics::GraphicStyle {
                    width_pt: 0.,
                    ..part.style.clone()
                },
                filled: true,
            });
        }
    }
    // Clip only the original boundary. Stroking the clipped fill polygons
    // would draw artificial outlines along the label's clearance rectangle.
    let mut commands = Vec::new();
    for points in paths {
        let mut pen = None;
        for edge in points.windows(2) {
            let [a, b] = edge else {
                continue;
            };
            let mut pieces = vec![(*a, *b)];
            for bounds in &relevant {
                pieces = pieces
                    .into_iter()
                    .flat_map(|(a, b)| outside_segment(a, b, *bounds))
                    .collect();
            }
            for (a, b) in pieces {
                if pen != Some(a) {
                    commands.push(PathCommand::Move(a));
                }
                commands.push(PathCommand::Line(b));
                pen = Some(b);
            }
        }
    }
    if !commands.is_empty() {
        out.push(Part {
            commands,
            style: crate::graphics::GraphicStyle {
                fill: None,
                ..part.style
            },
            filled: false,
        });
    }
    out
}

fn contains(points: &[Point], p: Point) -> bool {
    let mut inside = false;
    for (a, b) in points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
    {
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
    }
    inside
}

fn lerp(a: Point, b: Point, t: f32) -> Point {
    a.offset((b.x - a.x) * t, (b.y - a.y) * t)
}
fn inside_interval(a: Point, b: Point, (lo, hi): Box2) -> Option<(f32, f32)> {
    let mut enter: f32 = 0.;
    let mut leave: f32 = 1.;
    for (start, delta, min, max) in [(a.x, b.x - a.x, lo.x, hi.x), (a.y, b.y - a.y, lo.y, hi.y)] {
        if delta.abs() < 1e-8 {
            if start < min || start > max {
                return None;
            }
        } else {
            let t0 = (min - start) / delta;
            let t1 = (max - start) / delta;
            enter = enter.max(t0.min(t1));
            leave = leave.min(t0.max(t1));
        }
        if enter >= leave {
            return None;
        }
    }
    Some((enter, leave))
}
fn outside_segment(a: Point, b: Point, bounds: Box2) -> Vec<(Point, Point)> {
    let Some((enter, leave)) = inside_interval(a, b, bounds) else {
        return vec![(a, b)];
    };
    let mut out = Vec::with_capacity(2);
    if enter > 0. {
        out.push((a, lerp(a, b, enter)));
    }
    if leave < 1. {
        out.push((lerp(a, b, leave), b));
    }
    out
}

fn half_polygon(points: &[Point], axis: usize, bound: f32, below: bool) -> Vec<Point> {
    let coordinate = |p: Point| if axis == 0 { p.x } else { p.y };
    let keep = |p: Point| {
        if below {
            coordinate(p) <= bound
        } else {
            coordinate(p) >= bound
        }
    };
    let mut out = Vec::new();
    for (a, b) in points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
    {
        if keep(*a) {
            out.push(*a);
        }
        if keep(*a) != keep(*b) {
            out.push(lerp(
                *a,
                *b,
                (bound - coordinate(*a)) / (coordinate(*b) - coordinate(*a)),
            ));
        }
    }
    out
}
fn outside_polygon(points: &[Point], (lo, hi): Box2) -> Vec<Vec<Point>> {
    // Disjoint half-plane pieces cover the rectangle's complement. Keep them
    // in one filled path so raster antialiasing cannot leave seams between them.
    let mut remainder = points.to_vec();
    let mut out = Vec::new();
    for (axis, bound, below) in [
        (0, lo.x, true),
        (0, hi.x, false),
        (1, lo.y, true),
        (1, hi.y, false),
    ] {
        let piece = half_polygon(&remainder, axis, bound, below);
        if piece.len() >= 3 {
            out.push(piece);
        }
        remainder = half_polygon(&remainder, axis, bound, !below);
        if remainder.len() < 3 {
            break;
        }
    }
    out
}

fn flatten(commands: &[PathCommand]) -> Vec<Vec<Point>> {
    let mut paths: Vec<Vec<Point>> = Vec::new();
    for command in commands {
        match *command {
            PathCommand::Move(p) => paths.push(vec![p]),
            PathCommand::Line(p) => {
                if let Some(points) = paths.last_mut() {
                    points.push(p);
                }
            }
            PathCommand::Close => {
                if let Some(points) = paths.last_mut()
                    && let Some(first) = points.first().copied()
                {
                    points.push(first);
                }
            }
            PathCommand::Cubic(a, b, end) => {
                if let Some(points) = paths.last_mut()
                    && let Some(start) = points.last().copied()
                {
                    flatten_cubic(points, start, a, b, end, 0);
                }
            }
        }
    }
    paths
}
fn flatten_cubic(out: &mut Vec<Point>, start: Point, a: Point, b: Point, end: Point, depth: u8) {
    let deviation = |p: Point| {
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let length = dx.hypot(dy);
        if length < 1e-8 {
            p.distance(start)
        } else {
            ((p.x - start.x) * dy - (p.y - start.y) * dx).abs() / length
        }
    };
    if depth == 14 || deviation(a).max(deviation(b)) <= 0.03 {
        out.push(end);
        return;
    }
    let a0 = lerp(start, a, 0.5);
    let a1 = lerp(a, b, 0.5);
    let a2 = lerp(b, end, 0.5);
    let b0 = lerp(a0, a1, 0.5);
    let b1 = lerp(a1, a2, 0.5);
    let middle = lerp(b0, b1, 0.5);
    flatten_cubic(out, start, a0, b0, middle, depth + 1);
    flatten_cubic(out, middle, b1, a2, end, depth + 1);
}
