//! Noncircle step shafts and side contacts on their resolved native geometry.
use super::{Drawn, Outline, boxes, center_of, contains, grown};
use crate::{
    arrows::{ArrowStyle, Preset},
    document::{Arrow, ArrowNode, Document, Point},
    graphics::{Graphic, PathCommand, PathNodeType},
    style::DEFAULT,
};

#[cfg(test)]
#[path = "perimeter_arc_tests.rs"]
mod tests;

#[path = "perimeter_deform.rs"]
mod deform;
#[path = "perimeter_frames.rs"]
mod frames;
#[path = "perimeter_names.rs"]
mod names;
#[path = "perimeter_sides.rs"]
mod sides;
pub(super) use frames::Frame;

pub(super) struct Step<'a> {
    pub lanes: &'a [Option<super::shapes::Shaft>],
    pub first: f64,
    pub span: f64,
}

/// Nonpersistent hit ownership, including a hidden step's visible decoration.
#[derive(Default)]
pub(super) struct Owners {
    pub arrows: Vec<Option<usize>>,
    pub labels: Vec<usize>,
}

/// Resolve one native step without laying out side arrows or names. The same
/// routine is used for drawing and for the skipped-member ownership bridge.
pub(super) fn step_arrows(
    doc: &Document,
    g: &Graphic,
    perimeter: &Outline,
    step: &super::Step,
    ends: [(f64, Option<(Point, Point)>); 2],
    whole: bool,
) -> Option<(Vec<Arrow>, f64)> {
    let cycle = g.cycle.as_deref()?;
    if ![cycle.gap_pt, step.lead_pt, step.trail_pt, step.bow_pt]
        .into_iter()
        .all(f32::is_finite)
    {
        return None;
    }
    let dir = if g.cycle_counterclockwise() { -1. } else { 1. };
    let length = perimeter.length();
    let [(first, first_ink), (last, last_ink)] = ends;
    let mut span = (dir * (last - first)).rem_euclid(length);
    if span == 0. && whole {
        span = length;
    }
    if span == 0. {
        return None;
    }
    let gap = DEFAULT.world(cycle.gap_pt);
    let clearance = |ink: Option<(Point, Point)>, anchor, way| {
        ink.map_or(0., |b| {
            clear(perimeter, anchor, way, grown(b, gap), span * 0.5)
        })
    };
    let mut from = clearance(first_ink, first, dir);
    let mut to = span - clearance(last_ink, last, -dir);
    let available = to - from;
    if available <= length * 1e-9 {
        return None;
    }
    let room = available - f64::from(0.8 * doc.drawing_style.bond_length_world).min(available);
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
    let (start, signed_length) = (first + dir * from, dir * (to - from));
    let style = arrow_style(g);
    let arrows = if step.bow_pt == 0. && !step.reversible {
        vec![path_arrow(g.id, perimeter, start, signed_length, &style)?]
    } else {
        deform::arrows(
            g.id,
            perimeter,
            start,
            signed_length,
            f64::from(DEFAULT.world(step.bow_pt)),
            step.reversible,
            &style,
        )?
    };
    Some((arrows, dir * span))
}

fn arrow_style(g: &Graphic) -> ArrowStyle {
    ArrowStyle {
        color: g.style.stroke,
        width_pt: g.style.width_pt,
        pattern: g.style.pattern,
        ..ArrowStyle::preset(Preset::Forward)
    }
}

pub(super) fn empty_arrow(g: &Graphic, perimeter: &Outline) -> Option<Arrow> {
    let dir = if g.cycle_counterclockwise() { -1. } else { 1. };
    let (center, radius) = g.cycle_circle();
    let top = perimeter
        .project(center.offset(0., -2. * radius))
        .map_or(0., |p| p.s);
    let length = perimeter.length();
    let gap = length * 0.02 / std::f64::consts::TAU;
    path_arrow(
        g.id,
        perimeter,
        top + dir * gap,
        dir * (length - 2. * gap),
        &arrow_style(g),
    )
}

/// Resolve noncircle shafts, side arrows and names from the same geometry.
/// Each reversible lane has its own explicit arc-to-step association.
pub(super) fn replace(doc: &Document, g: &Graphic, perimeter: &Outline, out: &mut Drawn) {
    resolve(doc, g, perimeter, out, |_, _| {});
}

/// Share virtual/painted lanes and the untrimmed directed perimeter span.
pub(super) fn resolve(
    doc: &Document,
    g: &Graphic,
    perimeter: &Outline,
    out: &mut Drawn,
    record: impl FnMut(usize, Step<'_>),
) {
    resolve_owned(doc, g, perimeter, out, &mut Owners::default(), record);
}

pub(super) fn resolve_owned(
    doc: &Document,
    g: &Graphic,
    perimeter: &Outline,
    out: &mut Drawn,
    owners: &mut Owners,
    mut record: impl FnMut(usize, Step<'_>),
) {
    let Some(cycle) = g.cycle.as_deref() else {
        return;
    };
    let style = arrow_style(g);
    owners.arrows.clear();
    owners.labels.clear();
    let dir = if g.cycle_counterclockwise() { -1. } else { 1. };
    let length = perimeter.length();
    let (center, radius) = g.cycle_circle();
    let top = perimeter
        .project(center.offset(0., -2. * radius))
        .map_or(0., |p| p.s);
    let n = cycle.intermediates.len();
    if n == 0 {
        out.arrows = empty_arrow(g, perimeter).into_iter().collect();
        out.arcs = vec![None; out.arrows.len()];
        owners.arrows = vec![None; out.arrows.len()];
        names::place(doc, g, perimeter, vec![], &[], out);
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
    let arrows: Vec<_> = cycle
        .steps
        .iter()
        .enumerate()
        .take(n)
        .map(|(s, step)| {
            if step.hidden
                && step.reagents.is_empty()
                && step.products.is_empty()
                && step.name.trim().is_empty()
            {
                return None;
            }
            let end = |i| Some((*anchors.get(i)?, bounds.get(i).copied().flatten()));
            step_arrows(
                doc,
                g,
                perimeter,
                step,
                [end(s)?, end((s + 1) % n)?],
                n == 1,
            )
            .map(|(arrows, _)| arrows)
        })
        .collect();
    let mut parts = bounds.iter().skip(n);
    let mut names = vec![];
    let mut name_steps = vec![];
    out.arrows.clear();
    out.arcs.clear();
    for (s, (step, arrows)) in cycle.steps.iter().zip(arrows).enumerate() {
        let arrows = arrows.unwrap_or_default();
        let shafts: Vec<_> = arrows
            .iter()
            .map(super::shapes::Shaft::from_arrow)
            .collect();
        let first = anchors.get(s).copied().unwrap_or(0.);
        let mut span = anchors
            .get((s + 1) % n)
            .map_or(0., |last| (dir * (*last - first)).rem_euclid(length));
        if span == 0. && n == 1 {
            span = length;
        }
        record(
            s,
            Step {
                lanes: &shafts,
                first,
                span: dir * span,
            },
        );
        if !step.hidden {
            for arrow in arrows {
                out.arrows.push(arrow);
                out.arcs.push(Some(s));
                owners.arrows.push(Some(s));
            }
        }
        // Hidden steps keep virtual shape-aware targets for their sides.
        // Every part consumes its own ink entry, even when nothing is drawn.
        let mut side_parts = vec![];
        for incoming in step
            .reagents
            .iter()
            .map(|_| true)
            .chain(step.products.iter().map(|_| false))
        {
            let Some(&Some(ink)) = parts.next() else {
                continue;
            };
            side_parts.push((incoming, grown(ink, gap)));
        }
        for arrow in sides::arrows(
            g.id,
            shafts.iter().filter_map(Option::as_ref),
            &side_parts,
            doc.drawing_style.bond_length_world,
            &style,
        ) {
            out.arrows.push(arrow);
            out.arcs.push(None);
            owners.arrows.push(Some(s));
        }
        if !step.name.trim().is_empty()
            && let (Some(&first), Some(&last)) = (anchors.get(s), anchors.get((s + 1) % n))
        {
            let mut span = (dir * (last - first)).rem_euclid(length);
            if span == 0. && n == 1 {
                span = length;
            }
            names.push(names::Name::new(
                step.name.clone(),
                shafts,
                first + dir * span * 0.5,
            ));
            name_steps.push(s);
        }
    }
    owners.labels = names::place(doc, g, perimeter, names, &bounds, out)
        .into_iter()
        .filter_map(|i| name_steps.get(i).copied())
        .collect();
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
