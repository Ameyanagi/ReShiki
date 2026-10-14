//! Clipping and tessellation for rear-opacity paint in drawing coordinates.
use super::{ink, visibility, with_opacity};
use crate::{
    document::Point,
    graphics::PathCommand,
    polygon::closed_edges,
    scene::{Primitive, depth_field::DepthField},
};

const CAPSULE_HALF_STEPS: usize = 16;
const MAX_VISIBLE_CONTOURS: usize = 8192;
const CURVE_MASK_OVERLAP: f32 = 0.04;
const CUT_BISECTION_STEPS: usize = 24;

/// The side of a segment-parameter threshold retained by clipping.
#[derive(Clone, Copy)]
enum Side {
    /// Keeps t <= threshold (formerly rear == false).
    Before,
    /// Keeps t >= threshold (formerly rear == true).
    After,
}

fn band(part: &Primitive, a: Point, b: Point, low: f32, high: f32) -> Vec<Primitive> {
    let (x, y) = (
        f64::from(b.x) - f64::from(a.x),
        f64::from(b.y) - f64::from(a.y),
    );
    let length = x * x + y * y;
    if length < 1e-12 {
        return vec![part.clone()];
    }
    let below = if low <= 0. {
        vec![part.clone()]
    } else {
        clip_at_parameter(part, a, b, low, Side::After)
    };
    if high >= 1. {
        below
    } else {
        below
            .iter()
            .flat_map(|p| clip_at_parameter(p, a, b, high, Side::Before))
            .collect()
    }
}
pub(super) fn split_intervals(
    part: Primitive,
    a: Point,
    b: Point,
    hidden: &[(f32, f32)],
    alpha: f32,
) -> Vec<Primitive> {
    if hidden.is_empty() || alpha >= 1. {
        return vec![part];
    }
    if matches!(hidden,[(low,high)] if *low<=0.&&*high>=1.) {
        return with_opacity(vec![part], alpha);
    }
    let part = match part {
        Primitive::Line(a, b, w) => Primitive::Polygon(capsule(a, b, w)),
        p => ink::single_paint(p),
    };
    let mut out = with_opacity(vec![part.clone()], alpha);
    let mut cursor = 0.;
    for &(low, high) in hidden {
        if low > cursor {
            out.extend(band(&part, a, b, cursor, low));
        }
        cursor = cursor.max(high);
    }
    if cursor < 1. {
        out.extend(band(&part, a, b, cursor, 1.));
    }
    out
}
/// Polygon clipping in drawing coordinates; f64 evaluations keep equality visible.
pub(super) fn clip_values(points: &[Point], value: impl Fn(Point) -> f64) -> Vec<Point> {
    let mut out = vec![];
    for (a, b) in closed_edges(points) {
        let (va, vb) = (value(a), value(b));
        let (ia, ib) = (va >= 0., vb >= 0.);
        if ia {
            out.push(a);
        }
        if ia != ib && va.is_finite() && vb.is_finite() {
            out.push(a.lerp(b, (va / (va - vb)).clamp(0., 1.) as f32));
        }
    }
    out
}
pub(super) fn mask_clip(points: &[Point], mask: &[Point], outside: bool) -> Vec<Vec<Point>> {
    if mask.len() < 3 {
        return if outside {
            vec![points.to_vec()]
        } else {
            vec![]
        };
    }
    // Keep origin-free arithmetic here; polygon::signed_area subtracts the first vertex.
    let area: f64 = closed_edges(mask)
        .map(|(a, b)| f64::from(a.x) * f64::from(b.y) - f64::from(a.y) * f64::from(b.x))
        .sum();
    if area.abs() < 1e-10 {
        return if outside {
            vec![points.to_vec()]
        } else {
            vec![]
        };
    }
    let sign = area.signum();
    let mut inside = points.to_vec();
    let mut out = vec![];
    for (a, b) in closed_edges(mask) {
        let edge = |p: Point| {
            sign * ((f64::from(b.x) - f64::from(a.x)) * (f64::from(p.y) - f64::from(a.y))
                - (f64::from(b.y) - f64::from(a.y)) * (f64::from(p.x) - f64::from(a.x)))
        };
        if outside {
            let piece = clip_values(&inside, |p| -edge(p));
            if piece.len() >= 3 {
                out.push(piece);
            }
        }
        inside = clip_values(&inside, edge);
        if inside.len() < 3 {
            break;
        }
    }
    if !outside && inside.len() >= 3 {
        out.push(inside);
    }
    out
}
fn contours(part: &Primitive) -> Vec<Vec<Point>> {
    match part {
        Primitive::Polygon(p) => vec![p.clone()],
        Primitive::Path {
            commands,
            filled: true,
            ..
        } => flatten(commands),
        _ => vec![],
    }
}
fn contour_part(part: &Primitive, contours: Vec<Vec<Point>>) -> Vec<Primitive> {
    let mut commands = vec![];
    for path in contours {
        if let Some(first) = path.first().filter(|_| path.len() >= 3) {
            commands.push(PathCommand::Move(*first));
            commands.extend(path.iter().skip(1).copied().map(PathCommand::Line));
            commands.push(PathCommand::Close);
        }
    }
    if commands.is_empty() {
        return vec![];
    }
    let style = match part {
        Primitive::Path { style, .. } => style.clone(),
        _ => crate::graphics::GraphicStyle {
            fill: Some(crate::palette::Color::Ink),
            width_pt: 0.,
            ..Default::default()
        },
    };
    vec![Primitive::Path {
        commands,
        style,
        filled: true,
    }]
}
pub(super) fn split_fill(
    part: Primitive,
    field: DepthField,
    v: &visibility::Visibility,
    ids: &[u64],
    alpha: f32,
) -> Vec<Primitive> {
    let single = ink::single_paint(part.clone());
    let original = contours(&single);
    if original.is_empty() {
        return vec![part];
    }
    let mut visible = original.clone();
    for mask in v.masks(field, ids) {
        visible = visible
            .iter()
            .flat_map(|p| mask_clip(p, &mask, true))
            .collect();
        // A conservative whole-shape fallback avoids insertion-order partial
        // rendering if a highly subdivided custom filled shape exceeds budget.
        if visible.len() > MAX_VISIBLE_CONTOURS {
            return vec![part];
        }
    }
    if visible == original {
        return vec![part];
    }
    let mut out = with_opacity(vec![single.clone()], alpha);
    out.extend(contour_part(&single, visible));
    out
}
pub(super) fn split_curve(
    part: Primitive,
    paths: Vec<Vec<Point>>,
    field: DepthField,
    v: &visibility::Visibility,
    ids: &[u64],
    alpha: f32,
) -> Vec<Primitive> {
    let width = match &part {
        Primitive::Path { style, .. } => style.width(),
        _ => return vec![part],
    };
    let single = ink::single_paint(part);
    let original = contours(&single);
    let mut visible = vec![];
    for path in paths {
        for pair in path.windows(2) {
            let &[a, b] = pair else {
                continue;
            };
            let hidden = v.hidden(a, b, field, ids, 0);
            let mut intervals = vec![];
            let mut low = 0.;
            for (s, e) in hidden {
                if s > low {
                    intervals.push((low, s));
                }
                low = e;
            }
            if low < 1. {
                intervals.push((low, 1.));
            }
            for (s, e) in intervals {
                let mask = Primitive::Polygon(capsule(
                    a.lerp(b, s),
                    a.lerp(b, e),
                    width + CURVE_MASK_OVERLAP,
                ));
                let clipped = band(&mask, a, b, s, e);
                let mask = clipped.first().map(contours).unwrap_or_default();
                for contour in &original {
                    for mask in &mask {
                        visible.extend(mask_clip(contour, mask, false));
                    }
                }
            }
            if visible.len() > MAX_VISIBLE_CONTOURS {
                return vec![single];
            }
        }
    }
    let mut out = with_opacity(vec![single.clone()], alpha);
    out.extend(contour_part(&single, visible));
    out
}

fn inside(field: DepthField, p: Point, keep: Side) -> bool {
    match keep {
        Side::After => field.at(p) >= 0.5,
        Side::Before => field.at(p) <= 0.5,
    }
}
fn cut(a: Point, b: Point, field: DepthField) -> Point {
    // Only linear parameter fields from band() reach this crossing. Keep the
    // 24-step bisection for bit-stable output; exact intersections differ by ulps.
    let mut low = 0.;
    let mut high = 1.;
    let side = field.at(a) > 0.5;
    for _ in 0..CUT_BISECTION_STEPS {
        let t = (low + high) / 2.;
        if (field.at(a.lerp(b, t)) > 0.5) == side {
            low = t;
        } else {
            high = t;
        }
    }
    a.lerp(b, (low + high) / 2.)
}
fn polygon(points: &[Point], field: DepthField, keep: Side) -> Vec<Point> {
    let mut clipped = vec![];
    for (a, b) in closed_edges(points) {
        let ia = inside(field, a, keep);
        let ib = inside(field, b, keep);
        if ia {
            clipped.push(a);
        }
        if ia != ib {
            clipped.push(cut(a, b, field));
        }
    }
    clipped
}
pub(super) fn capsule(a: Point, b: Point, width: f32) -> Vec<Point> {
    let angle = (b.y - a.y).atan2(b.x - a.x);
    let radius = width / 2.;
    [b, a]
        .into_iter()
        .enumerate()
        .flat_map(|(side, p)| {
            (0..=CAPSULE_HALF_STEPS).map(move |i| {
                let theta = angle - std::f32::consts::FRAC_PI_2
                    + side as f32 * std::f32::consts::PI
                    + i as f32 * std::f32::consts::PI / CAPSULE_HALF_STEPS as f32;
                p.offset(radius * theta.cos(), radius * theta.sin())
            })
        })
        .collect()
}
/// Encodes t - threshold + 0.5, so classification compares against 0.5.
/// Keep this legacy encoding and cut()'s 24-step bisection for bit-stable output;
/// an exact segment intersection would change output at the ulp level.
fn parameter_field(a: Point, b: Point, threshold: f32) -> DepthField {
    let (x, y) = (
        f64::from(b.x) - f64::from(a.x),
        f64::from(b.y) - f64::from(a.y),
    );
    let length = x * x + y * y;
    DepthField::Plane {
        origin: a,
        weight: 0.5 - f64::from(threshold),
        dx: x / length,
        dy: y / length,
    }
}
fn clip_at_parameter(
    part: &Primitive,
    a: Point,
    b: Point,
    threshold: f32,
    keep: Side,
) -> Vec<Primitive> {
    clip_field(part, parameter_field(a, b, threshold), keep)
}
fn clip_field(part: &Primitive, field: DepthField, keep: Side) -> Vec<Primitive> {
    match part {
        Primitive::Polygon(p) => {
            let p = polygon(p, field, keep);
            if p.len() >= 3 {
                vec![Primitive::Polygon(p)]
            } else {
                vec![]
            }
        }
        Primitive::Line(a, b, width) => {
            clip_field(&Primitive::Polygon(capsule(*a, *b, *width)), field, keep)
        }
        Primitive::Path {
            commands,
            style,
            filled,
        } => {
            let paths = flatten(commands);
            if *filled {
                let mut clipped = vec![];
                for path in &paths {
                    let p = polygon(path, field, keep);
                    if let Some(first) = p.first().filter(|_| p.len() >= 3) {
                        clipped.push(PathCommand::Move(*first));
                        clipped.extend(p.iter().skip(1).copied().map(PathCommand::Line));
                        clipped.push(PathCommand::Close);
                    }
                }
                let mut parts = vec![];
                if !clipped.is_empty() {
                    let mut fill = style.clone();
                    fill.width_pt = 0.;
                    parts.push(Primitive::Path {
                        commands: clipped,
                        style: fill,
                        filled: true,
                    });
                }
                if style.width_pt > 0. {
                    parts.extend(clip_field(
                        &Primitive::Path {
                            commands: commands.clone(),
                            style: style.clone(),
                            filled: false,
                        },
                        field,
                        keep,
                    ));
                }
                parts
            } else {
                let mut clipped = vec![];
                for path in paths {
                    let mut pen = None;
                    for pair in path.windows(2) {
                        let &[a, b] = pair else { continue };
                        let (ia, ib) = (inside(field, a, keep), inside(field, b, keep));
                        let segment = match (ia, ib) {
                            (true, true) => Some((a, b)),
                            (true, false) => Some((a, cut(a, b, field))),
                            (false, true) => Some((cut(a, b, field), b)),
                            _ => None,
                        };
                        if let Some((a, b)) = segment {
                            if pen != Some(a) {
                                clipped.push(PathCommand::Move(a));
                            }
                            clipped.push(PathCommand::Line(b));
                            pen = Some(b);
                        } else {
                            pen = None;
                        }
                    }
                }
                if clipped.is_empty() {
                    vec![]
                } else {
                    vec![Primitive::Path {
                        commands: clipped,
                        style: style.clone(),
                        filled: false,
                    }]
                }
            }
        }
        _ => vec![part.clone()],
    }
}
const REAR_FLATTEN_TOLERANCE: f32 = 0.02;
const REAR_FLATTEN_DEPTH: u8 = 12;

// Deliberate sibling of scientific/label_clearance.rs `flatten` (hypot, 1e-8 cutoff,
// 0.03/depth 14, ignores commands before a Move); do not merge them.
pub(super) fn flatten(commands: &[PathCommand]) -> Vec<Vec<Point>> {
    fn cubic(out: &mut Vec<Point>, p: Point, a: Point, b: Point, z: Point, depth: u8) {
        let distance = |v: Point| {
            let (x, y) = (z.x - p.x, z.y - p.y);
            let length = (x * x + y * y).sqrt();
            if length < 1e-6 {
                p.distance(v)
            } else {
                ((v.x - p.x) * y - (v.y - p.y) * x).abs() / length
            }
        };
        if depth >= REAR_FLATTEN_DEPTH || distance(a).max(distance(b)) <= REAR_FLATTEN_TOLERANCE {
            out.push(z);
            return;
        }
        let (pa, ab, bz) = (p.lerp(a, 0.5), a.lerp(b, 0.5), b.lerp(z, 0.5));
        let (left, right) = (pa.lerp(ab, 0.5), ab.lerp(bz, 0.5));
        let middle = left.lerp(right, 0.5);
        cubic(out, p, pa, left, middle, depth + 1);
        cubic(out, middle, right, bz, z, depth + 1);
    }
    let mut paths = vec![];
    let mut path = vec![];
    for command in commands {
        match *command {
            PathCommand::Move(p) => {
                if !path.is_empty() {
                    paths.push(std::mem::take(&mut path));
                }
                path.push(p);
            }
            PathCommand::Line(p) => path.push(p),
            PathCommand::Cubic(a, b, z) => {
                if let Some(&p) = path.last() {
                    cubic(&mut path, p, a, b, z, 0);
                } else {
                    path.push(z);
                }
            }
            PathCommand::Close => {
                if let Some(&p) = path.first() {
                    path.push(p);
                }
            }
        }
    }
    if !path.is_empty() {
        paths.push(path);
    }
    paths
}
