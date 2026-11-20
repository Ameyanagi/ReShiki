//! Ordinary noncircle shafts. The perimeter is the sole source of their
//! geometry; circular Bow/reversible and side-path consumers remain separate.
use super::{Cycle, Drawn, Outline, boxes, center_of, contains, grown};
use crate::{
    arrows::{ArrowStyle, Preset},
    document::{Arrow, ArrowNode, Document, Point},
    graphics::{Graphic, PathCommand, PathNodeType},
    style::DEFAULT,
};

#[cfg(test)]
#[path = "perimeter_arc_tests.rs"]
mod tests;

fn ordinary(cycle: &Cycle, step: usize) -> bool {
    cycle
        .steps
        .get(step)
        .is_some_and(|s| !s.reversible && s.bow_pt == 0.)
}

/// Replace only ordinary step shafts, retaining every legacy side arrow,
/// bowed/reversible step and label. Arc-to-step associations remain explicit.
pub(super) fn replace(doc: &Document, g: &Graphic, perimeter: &Outline, out: &mut Drawn) {
    let Some(cycle) = g.cycle.as_deref() else {
        return;
    };
    let style = ArrowStyle {
        color: g.style.stroke,
        width_pt: g.style.width_pt,
        pattern: g.style.pattern,
        ..ArrowStyle::preset(Preset::Forward)
    };
    let dir = if g.cycle_counterclockwise() { -1. } else { 1. };
    let length = perimeter.length();
    let (center, radius) = g.cycle_circle();
    let top = perimeter
        .project(center.offset(0., -2. * radius))
        .map_or(0., |p| p.s);
    let n = cycle.intermediates.len();
    if n == 0 {
        let gap = length * 0.02 / std::f64::consts::TAU;
        out.arrows = path_arrow(
            g.id,
            perimeter,
            top + dir * gap,
            dir * (length - 2. * gap),
            &style,
        )
        .into_iter()
        .collect();
        out.arcs = vec![None; out.arrows.len()];
        return;
    }
    let bounds = boxes(doc, cycle);
    let anchors: Vec<_> = (0..n)
        .map(|i| {
            bounds
                .get(i)
                .copied()
                .flatten()
                .and_then(|b| perimeter.project(center_of(b)))
                .map_or(
                    (top + dir * length * i as f64 / n as f64).rem_euclid(length),
                    |p| p.s,
                )
        })
        .collect();
    let gap = DEFAULT.world(cycle.gap_pt);
    let mut arrows: Vec<_> = cycle
        .steps
        .iter()
        .enumerate()
        .take(n)
        .map(|(s, step)| {
            if step.hidden || !ordinary(cycle, s) {
                return None;
            }
            let (&first, &last) = (anchors.get(s)?, anchors.get((s + 1) % n)?);
            let mut span = (dir * (last - first)).rem_euclid(length);
            if span == 0. && n == 1 {
                span = length;
            }
            if span == 0. {
                return None;
            }
            let clearance = |i, anchor, way| {
                bounds.get(i).copied().flatten().map_or(0., |b| {
                    clear(perimeter, anchor, way, grown(b, gap), span * 0.5)
                })
            };
            let mut from = clearance(s, first, dir);
            let mut to = span - clearance((s + 1) % n, last, -dir);
            let available = to - from;
            if available <= length * 1e-9 {
                return None;
            }
            let room =
                available - f64::from(0.8 * doc.drawing_style.bond_length_world).min(available);
            let floor = -cycle.gap_pt;
            let mut lead = f64::from(DEFAULT.world(step.lead_pt.max(floor)));
            let mut trail = f64::from(DEFAULT.world(step.trail_pt.max(floor)));
            if lead + trail > room && lead + trail > 0. {
                let k = room / (lead + trail);
                lead *= k;
                trail *= k;
            }
            from += lead;
            to -= trail;
            let most = length * 358. / 360.;
            if to - from > most {
                let extra = (to - from - most) * 0.5;
                from += extra;
                to -= extra;
            }
            path_arrow(
                g.id,
                perimeter,
                first + dir * from,
                dir * (to - from),
                &style,
            )
        })
        .collect();
    let mut replaced = vec![false; n];
    let (mut next_arrows, mut next_arcs) = (vec![], vec![]);
    for (arrow, step) in std::mem::take(&mut out.arrows)
        .into_iter()
        .zip(std::mem::take(&mut out.arcs))
    {
        if let Some(s) = step.filter(|s| ordinary(cycle, *s)) {
            if let Some(done) = replaced.get_mut(s) {
                if !*done && let Some(arrow) = arrows.get_mut(s).and_then(Option::take) {
                    next_arrows.push(arrow);
                    next_arcs.push(Some(s));
                }
                *done = true;
            }
        } else {
            next_arrows.push(arrow);
            next_arcs.push(step);
        }
    }
    // A Circle's shorter surrogate may have had no room for a shaft that
    // fits the chosen perimeter. Do not make its old visibility a condition.
    for (s, arrow) in arrows.into_iter().enumerate() {
        if let Some(arrow) = arrow {
            next_arrows.push(arrow);
            next_arcs.push(Some(s));
        }
    }
    out.arrows = next_arrows;
    out.arcs = next_arcs;
}

/// Clear the ink along distance, never by flattening the shaft. When an
/// intermediate is off the perimeter, its corners still reserve an opening.
fn clear(perimeter: &Outline, anchor: f64, dir: f64, ink: (Point, Point), limit: f64) -> f64 {
    let at = |s| perimeter.point(anchor + dir * s);
    if !at(0.).is_some_and(|p| contains(ink, p)) {
        let (lo, hi) = ink;
        return [lo, Point::new(hi.x, lo.y), hi, Point::new(lo.x, hi.y)]
            .into_iter()
            .filter_map(|p| perimeter.project(p))
            .map(|p| {
                (dir * (p.s - anchor) + perimeter.length() * 0.5).rem_euclid(perimeter.length())
                    - perimeter.length() * 0.5
            })
            .fold(0., f64::max)
            .min(limit);
    }
    let signed = |s| {
        at(s).map_or(f64::INFINITY, |p| {
            [
                f64::from(ink.0.x) - f64::from(p.x),
                f64::from(p.x) - f64::from(ink.1.x),
                f64::from(ink.0.y) - f64::from(p.y),
                f64::from(p.y) - f64::from(ink.1.y),
            ]
            .into_iter()
            .fold(f64::NEG_INFINITY, f64::max)
        })
    };
    let (first, last) = (signed(0.), signed(limit));
    if !first.is_finite() || !last.is_finite() || last <= 0. {
        return limit;
    }
    // ITP keeps bisection's bounded search while taking the near-linear
    // clearance of an edge in a few queries. Each point query can invert a
    // cubic's arc length, so a fixed 32-query search is costly in long cycles.
    let epsilon = perimeter.length() * 1e-8;
    let root = kurbo::common::solve_itp(signed, 0., limit, epsilon, 1, 0.2 / limit, first, last);
    (root + 2. * epsilon).min(limit)
}

/// Preserve each subpath corner as a native arrow node. Straight segments
/// are degree-elevated exactly; existing cubics keep their controls.
fn path_arrow(
    id: u64,
    perimeter: &Outline,
    start: f64,
    length: f64,
    style: &ArrowStyle,
) -> Option<Arrow> {
    if length == 0. {
        return None;
    }
    let path = perimeter.subpath(start, length)?;
    let mut segments = vec![];
    let mut at = None;
    for command in path {
        match command {
            PathCommand::Move(p) => at = Some(p),
            PathCommand::Line(p) => {
                let first = at?;
                segments.push((
                    [first, first.lerp(p, 1. / 3.), p.lerp(first, 1. / 3.), p],
                    true,
                ));
                at = Some(p);
            }
            PathCommand::Cubic(a, b, p) => {
                segments.push(([at?, a, b, p], false));
                at = Some(p);
            }
            PathCommand::Close => return None,
        }
    }
    let ((first, _), (last, _)) = (segments.first()?, segments.last()?);
    let mut arrow = Arrow::new(id, first[0], last[3], Preset::Forward, style.clone());
    arrow.cubic = Some([first[1], last[2]]);
    arrow.nodes = segments
        .iter()
        .zip(segments.iter().skip(1))
        .map(|((before, before_line), (after, after_line))| {
            let vector = |a: Point, b: Point| {
                (
                    f64::from(b.x) - f64::from(a.x),
                    f64::from(b.y) - f64::from(a.y),
                )
            };
            let arriving = vector(before[2], before[3]);
            let leaving = vector(after[0], after[1]);
            let scale = arriving.0.hypot(arriving.1) * leaving.0.hypot(leaving.1);
            let smooth = !(*before_line && *after_line)
                && scale > 0.
                && arriving.0 * leaving.0 + arriving.1 * leaving.1 >= scale * (1. - 1e-5);
            ArrowNode {
                point: before[3],
                controls: [before[2], after[1]],
                kind: if smooth {
                    PathNodeType::Smooth
                } else {
                    PathNodeType::Corner
                },
            }
        })
        .collect();
    Some(arrow)
}
