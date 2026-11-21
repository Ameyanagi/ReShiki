//! A split run keeps one native source profile and clips its ordered children.
use super::{arrow_style, deform, parameters, step_arrows};
use crate::{
    arrows::Head,
    cycles::{Outline, SplitRun, Step, center_of, grown, shapes::Profile},
    document::{Arrow, Document, Point},
    graphics::Graphic,
    style::DEFAULT,
};
use std::sync::Arc;

#[path = "perimeter_fit_sides.rs"]
mod sides;

#[path = "perimeter_fit_edit.rs"]
mod edit;
pub(in crate::cycles) use edit::{edited, moved, reflow_native, repair};

#[cfg(test)]
#[path = "perimeter_fit_tests.rs"]
mod tests;

pub(in crate::cycles) fn finish_split(
    before: &Document,
    after: &mut Document,
    target: &crate::cycles::Target,
) -> bool {
    valid(after, target.cycle) && sides::repair(before, after, target) && accepted(after, target)
}

pub(in crate::cycles) fn accepted(after: &Document, target: &crate::cycles::Target) -> bool {
    if !valid(after, target.cycle) {
        return false;
    }
    let Some(g) = after.graphics.iter().find(|g| g.id == target.cycle) else {
        return false;
    };
    let Some(cycle) = g.cycle.as_deref() else {
        return false;
    };
    let n = cycle.intermediates.len();
    if n == 0 {
        return false;
    }
    let step = match target.role {
        crate::cycles::Role::Intermediate(i) => (i + n - 1) % n,
        crate::cycles::Role::Reagent(s) | crate::cycles::Role::Product(s) => s,
    };
    if cycle.split_run(step).is_some() && !sides::valid(after, g.id, step) {
        return false;
    }
    let ink = part_ink(after, &target.part);
    if ink.is_empty() {
        return false;
    }
    let overlap = |(a, b): Ink, (c, d): Ink| a.x < d.x && c.x < b.x && a.y < d.y && c.y < b.y;
    if cycle
        .parts()
        .filter(|(_, part)| part.iter().all(|id| !target.part.contains(id)))
        .any(|(_, part)| {
            part_ink(after, part)
                .iter()
                .any(|other| ink.iter().any(|ink| overlap(*ink, *other)))
        })
    {
        return false;
    }
    // A self-crossing source can meet the new structure at another logical
    // station. A single local opening must not leave that extra crossing.
    let drawn = crate::cycles::drawn(after, g);
    let gap = if cycle.split_run(step).is_some()
        && let crate::cycles::Role::Intermediate(slot) = target.role
    {
        let incoming = cycle
            .steps
            .get((slot + n - 1) % n)
            .map_or(0., |s| s.trail_pt);
        let outgoing = cycle.steps.get(slot).map_or(0., |s| s.lead_pt);
        DEFAULT.world((cycle.gap_pt + incoming.min(outgoing).min(0.)).max(0.))
    } else {
        0.
    };
    let arrows = drawn.arrows.iter().any(|arrow| {
        ink.iter()
            .any(|ink| super::names::arrow_hits(arrow, *ink, gap - 0.01))
    });
    let labels = drawn.labels.iter().any(|label| {
        ink.iter()
            .any(|ink| overlap(*ink, crate::cycles::split::label_ink(label)))
    });
    !arrows && !labels
}

type Ink = (Point, Point);
type End = (f64, Option<Ink>);
type Opening = (f64, f64);

/// A passive cache may change decoration, never the stored source or nominal
/// ownership. Keep explicit hydrogens and all authored display settings.
fn nominal_document(doc: &Document) -> Document {
    let mut nominal = doc.clone();
    crate::atom_labels::clear_computed(&mut nominal);
    for atom in &mut nominal.atoms {
        atom.label_h = 0;
    }
    nominal
}

fn extra_ink(doc: &Document, part: &[u64]) -> Vec<Ink> {
    crate::scene::participant_indicator_ink_boxes(doc, part)
        .into_iter()
        .chain(
            crate::molecule_names::include_annotations(doc, part)
                .into_iter()
                .filter(|id| !part.contains(id))
                .filter_map(|id| crate::scene::selection_bounds(doc, &[id])),
        )
        .collect()
}

fn connected_opening(
    profile: &Profile,
    station: f64,
    bracket: Opening,
    boxes: &[Ink],
) -> Option<Opening> {
    let mut intervals: Vec<_> = boxes
        .iter()
        .filter_map(|b| profile.opening(station, bracket, rect(*b)))
        .collect();
    intervals.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.total_cmp(&b.1)));
    let mut connected: Vec<Opening> = vec![];
    for (a, b) in intervals {
        if let Some(last) = connected.last_mut()
            && a <= last.1 + 64. * f64::EPSILON
        {
            last.1 = last.1.max(b);
        } else {
            connected.push((a, b));
        }
    }
    connected
        .into_iter()
        .find(|(a, b)| *a - 64. * f64::EPSILON <= station && station <= *b + 64. * f64::EPSILON)
}

fn part_ink(doc: &Document, part: &[u64]) -> Vec<Ink> {
    crate::scene::selection_bounds(doc, part)
        .into_iter()
        .chain(
            crate::molecule_names::include_annotations(doc, part)
                .into_iter()
                .filter(|id| !part.contains(id))
                .filter_map(|id| crate::scene::selection_bounds(doc, &[id])),
        )
        .collect()
}

#[derive(Clone)]
pub(in crate::cycles) struct Resolved {
    pub arrows: Vec<Arrow>,
    pub first: f64,
    pub span: f64,
}

pub(in crate::cycles) struct Layout<'a> {
    doc: &'a Document,
    graphic: &'a Graphic,
    outline: Outline,
    ends: Vec<End>,
    pub steps: Vec<Option<Resolved>>,
    sources: Vec<Option<Arc<Source>>>,
}

#[derive(Clone)]
struct Source {
    display: Option<Arc<Source>>,
    run: SplitRun,
    profiles: deform::Profiles,
    cuts: Vec<f64>,
    openings: Vec<[Option<Opening>; 3]>,
    ink: Vec<Ink>,
    captions: Vec<Ink>,
    outer: [Option<Ink>; 2],
    outer_crops: [[f64; 3]; 2],
    dir: f64,
    first: f64,
    span: f64,
}

impl Source {
    fn lane(&self, lane: usize) -> Option<&Profile> {
        match lane {
            0 => Some(&self.profiles.center),
            1 => self.profiles.forward.as_ref(),
            2 => self.profiles.reverse.as_ref(),
            _ => None,
        }
    }

    fn painted_lane(&self, step: &Step, lane: usize) -> Option<usize> {
        match (step.reversible, lane) {
            (true, 0) => Some(1),
            (true, 1) => Some(2),
            (false, 0) => Some(usize::from(self.run.source_reversible)),
            _ => None,
        }
    }

    fn bracket(&self, child: usize) -> Option<(f64, f64)> {
        let lo = if child == 0 {
            0.
        } else {
            *self.cuts.get(child - 1)?
        };
        let hi = self.cuts.get(child).copied().unwrap_or(1.);
        (0. <= lo && lo < hi && hi <= 1.).then_some((lo, hi))
    }

    fn cut_bracket(&self, cut: usize) -> Option<(f64, f64)> {
        let lo = if cut == 0 {
            0.
        } else {
            *self.run.cuts.get(cut - 1)?
        };
        let hi = self.run.cuts.get(cut + 1).copied().unwrap_or(1.);
        Some((lo, hi))
    }

    fn transport(&self, next: &Source, u: f64, from: Point) -> Option<Point> {
        let lane = [usize::from(self.run.source_reversible), 2]
            .into_iter()
            .take(if self.run.source_reversible { 2 } else { 1 })
            .filter_map(|lane| Some((lane, self.lane(lane)?.point(u)?)))
            .min_by(|(_, a), (_, b)| a.distance(from).total_cmp(&b.distance(from)))?
            .0;
        let old = self.lane(lane)?;
        let new = next.lane(lane)?;
        let old_at = old.point(u)?;
        let new_at = new.point(u)?;
        let a = old.tangent(u, false)?;
        let b = new.tangent(u, false)?;
        let old_n = Point::new(self.dir as f32 * a.y, -self.dir as f32 * a.x);
        let new_n = Point::new(next.dir as f32 * b.y, -next.dir as f32 * b.x);
        let d = Point::new(from.x - old_at.x, from.y - old_at.y);
        let along = d.x * a.x + d.y * a.y;
        let normal = d.x * old_n.x + d.y * old_n.y;
        let to = new_at.offset(
            along * b.x + normal * new_n.x,
            along * b.y + normal * new_n.y,
        );
        to.is_finite().then_some(to)
    }

    fn new(g: &Graphic, outline: &Outline, bounds: &[Option<Ink>], run: &SplitRun) -> Option<Self> {
        let cycle = g.cycle.as_deref()?;
        let n = cycle.intermediates.len();
        if n == 0 {
            return None;
        }
        let start = run.source_start * outline.length();
        let length = run.source_span * outline.length();
        if length.signum() != if g.cycle_counterclockwise() { -1. } else { 1. } {
            return None;
        }
        let profiles = deform::profiles(
            g.id,
            outline,
            start,
            length,
            f64::from(DEFAULT.world(run.source_bow_pt)),
            run.source_reversible,
            &arrow_style(g),
        )?;
        let mut source = Self {
            display: None,
            run: run.clone(),
            profiles,
            cuts: run.cuts.clone(),
            openings: vec![],
            ink: vec![],
            captions: vec![],
            outer: [*bounds.get(run.first_step)?, *bounds.get(run.end_slot(n))?],
            outer_crops: [[0.; 3], [1.; 3]],
            dir: length.signum(),
            first: start,
            span: length,
        };
        let gap = DEFAULT.world(cycle.gap_pt);
        for (i, &station) in run.cuts.iter().enumerate() {
            let slot = (run.first_step + i + 1) % n;
            let body = (*bounds.get(slot)?)?;
            source.ink.push(body);
            let bracket = source.cut_bracket(i)?;
            let opening = std::array::from_fn(|lane| {
                let (a, b) =
                    source
                        .lane(lane)?
                        .opening(station, bracket, rect(grown(body, gap)))?;
                let error = 64. * f64::EPSILON;
                (a - error <= station && station <= b + error).then_some((a, b))
            });
            source.openings.push(opening);
        }
        Some(source)
    }

    fn projected_cut(&self, cut: usize, point: Point) -> Option<f64> {
        let bracket = self.cut_bracket(cut)?;
        [usize::from(self.run.source_reversible), 2]
            .into_iter()
            .take(if self.run.source_reversible { 2 } else { 1 })
            .filter_map(|lane| self.lane(lane)?.project(point, bracket))
            .min_by(|a, b| {
                a.distance
                    .total_cmp(&b.distance)
                    .then_with(|| a.u.total_cmp(&b.u))
            })
            .map(|p| p.u)
    }

    /// Decoration can widen a local opening on the saved spine. If it would
    /// consume a child, keep its valid nominal arrow instead of changing the
    /// source, moving chemistry, or making a shaft disappear on refresh.
    fn decorated(&self, doc: &Document, g: &Graphic) -> Option<Self> {
        let c = g.cycle.as_deref()?;
        let n = c.intermediates.len();
        let actual = crate::scene::primary_selections_bounds(doc, &c.intermediates);
        let gap = DEFAULT.world(c.gap_pt);
        let mut display = self.clone();
        display.ink.clear();
        display.captions.clear();
        for slot in [self.run.first_step, self.run.end_slot(n)] {
            display
                .captions
                .extend(extra_ink(doc, c.intermediates.get(slot)?));
        }
        display.outer = [
            *actual.get(self.run.first_step)?,
            *actual.get(self.run.end_slot(n))?,
        ];
        for (cut, &u) in self.cuts.iter().enumerate() {
            let slot = (self.run.first_step + cut + 1) % n;
            let body = (*actual.get(slot)?)?;
            let extras = extra_ink(doc, c.intermediates.get(slot)?);
            display.ink.push(body);
            display.captions.extend(extras.iter().copied());
            let boxes: Vec<_> = std::iter::once(body)
                .chain(extras)
                .map(|b| grown(b, gap))
                .collect();
            let bracket = self.cut_bracket(cut)?;
            display.openings[cut] =
                std::array::from_fn(|lane| connected_opening(self.lane(lane)?, u, bracket, &boxes));
        }
        for end in 0..2 {
            let slot = if end == 0 {
                self.run.first_step
            } else {
                self.run.end_slot(n)
            };
            let boxes: Vec<_> = display.outer[end]
                .into_iter()
                .chain(extra_ink(doc, c.intermediates.get(slot)?))
                .map(|b| grown(b, gap))
                .collect();
            let station = end as f64;
            let bracket = if end == 0 {
                self.bracket(0)?
            } else {
                self.bracket(self.cuts.len())?
            };
            for lane in 0..3 {
                if let Some(profile) = self.lane(lane)
                    && let Some((a, b)) = connected_opening(profile, station, bracket, &boxes)
                {
                    display.outer_crops[end][lane] = if end == 0 { b } else { a };
                }
            }
        }
        Some(display)
    }

    fn arrows(&self, g: &Graphic, step: &Step, child: usize, gap: f32) -> Option<Vec<Arrow>> {
        let bracket = self.bracket(child)?;
        let mut arrows = vec![];
        for painted_lane in 0..if step.reversible { 2 } else { 1 } {
            let lane = self.painted_lane(step, painted_lane)?;
            let profile = self.lane(lane)?;
            let mut from = if child == 0 {
                self.outer_crops[0][lane]
            } else {
                self.openings.get(child - 1)?.get(lane)?.as_ref()?.1
            };
            let mut to = if child == self.cuts.len() {
                self.outer_crops[1][lane]
            } else {
                self.openings.get(child)?.get(lane)?.as_ref()?.0
            };
            if child > 0 {
                from = profile.shift(from, f64::from(DEFAULT.world(step.lead_pt.max(-gap))))?;
            }
            if child < self.cuts.len() {
                to = profile.shift(to, -f64::from(DEFAULT.world(step.trail_pt.max(-gap))))?;
            }
            // Openings must leave a positive part of this ordered child. Do
            // not silently draw a loop or cross a neighboring cut station.
            if !(bracket.0 <= from && from < to && to <= bracket.1) {
                return None;
            }
            let reverse = painted_lane == 1;
            let mut style = arrow_style(g);
            // Native heads and stroke have area. A centreline ending on the
            // requested opening is insufficient near oblique edges/corners.
            // Refine only this opening, by at most one head envelope; never
            // erase a distant second crossing to make an invalid split fit.
            let radius = f64::from(
                DEFAULT
                    .world(style.head_length_pt)
                    .hypot(DEFAULT.world(style.head_width_pt))
                    + DEFAULT.world(style.width_pt),
            );
            let initial = (from, to);
            let mut candidate = None;
            for trial in 0..=8 {
                if step.reversible {
                    let at = if reverse { from } else { to };
                    let base = self.profiles.center.tangent(at, !reverse)?;
                    let outward = Point::new(self.dir as f32 * base.y, -self.dir as f32 * base.x);
                    let way = profile.tangent(at, !reverse)?;
                    let dot = way.y * outward.x - way.x * outward.y;
                    style.head = if dot >= 0. { Head::Left } else { Head::Right };
                }
                let (a, b) = if reverse { (to, from) } else { (from, to) };
                let arrow = deform::native_arrow(g.id, profile.subpath(a, b)?, style.clone())?;
                let gap_at = |control: f32| DEFAULT.world((gap + control.min(0.)).max(0.));
                let left_ink = if child == 0 {
                    self.outer[0].as_ref()
                } else {
                    self.ink.get(child - 1)
                };
                let right_ink = if child == self.cuts.len() {
                    self.outer[1].as_ref()
                } else {
                    self.ink.get(child)
                };
                let left = left_ink.is_some_and(|ink| {
                    super::names::arrow_hits(&arrow, *ink, gap_at(step.lead_pt) - 0.01)
                });
                let right = right_ink.is_some_and(|ink| {
                    super::names::arrow_hits(&arrow, *ink, gap_at(step.trail_pt) - 0.01)
                });
                if !left && !right {
                    let gap = DEFAULT.world(gap) - 0.01;
                    let other = self.ink.iter().enumerate().any(|(i, ink)| {
                        i != child
                            && Some(i) != child.checked_sub(1)
                            && super::names::arrow_hits(&arrow, *ink, gap)
                    });
                    let outer = self.outer.iter().enumerate().any(|(i, ink)| {
                        !(i == 0 && child == 0 || i == 1 && child == self.cuts.len())
                            && ink.is_some_and(|ink| super::names::arrow_hits(&arrow, ink, gap))
                    });
                    if other
                        || outer
                        || self
                            .captions
                            .iter()
                            .any(|ink| super::names::arrow_hits(&arrow, *ink, gap))
                    {
                        return None;
                    }
                    candidate = Some(arrow);
                    break;
                }
                if trial == 8 {
                    return None;
                }
                let crop = radius * f64::from(trial + 1) / 8.;
                if left {
                    from = profile.shift(initial.0, crop)?;
                }
                if right {
                    to = profile.shift(initial.1, -crop)?;
                }
                if !(bracket.0 <= from && from < to && to <= bracket.1) {
                    return None;
                }
            }
            arrows.push(candidate?);
        }
        Some(arrows)
    }
}

impl<'a> Layout<'a> {
    pub(in crate::cycles) fn graphic(doc: &'a Document, g: &'a Graphic) -> Option<Self> {
        let outline = crate::cycles::outline(g)?;
        let cycle = g.cycle.as_deref()?;
        let n = cycle.intermediates.len();
        if n > super::super::MAX_PARTS || cycle.validate().is_err() {
            return None;
        }
        let (center, radius) = g.cycle_circle();
        let top = outline
            .project(center.offset(0., -2. * radius))
            .map_or(0., |p| p.s);
        let dir = if g.cycle_counterclockwise() { -1. } else { 1. };
        let ends: Vec<_> = crate::scene::selections_bounds(doc, &cycle.intermediates)
            .into_iter()
            .enumerate()
            .map(|(i, ink)| {
                let at = ink.and_then(|b| outline.project(center_of(b))).map_or_else(
                    || top + dir * outline.length() * i as f64 / n as f64,
                    |p| p.s,
                );
                (at, ink)
            })
            .collect();
        Some(Self::new(doc, g, &outline, &ends))
    }
    pub(in crate::cycles) fn new(
        doc: &'a Document,
        g: &'a Graphic,
        outline: &Outline,
        ends: &[End],
    ) -> Self {
        let cycle = g.cycle.as_deref();
        let n = cycle.map_or(0, |c| c.intermediates.len());
        if cycle.is_none_or(|c| c.validate().is_err()) || n > super::super::MAX_PARTS {
            return Self {
                doc,
                graphic: g,
                outline: outline.clone(),
                ends: vec![],
                steps: vec![],
                sources: vec![],
            };
        }
        let nominal = cycle
            .filter(|c| !c.split_runs.is_empty())
            .map(|_| nominal_document(doc));
        let bounds = nominal
            .as_ref()
            .zip(cycle)
            .map(|(doc, cycle)| crate::scene::primary_selections_bounds(doc, &cycle.intermediates));
        let sources: Vec<_> = cycle
            .into_iter()
            .flat_map(|c| &c.split_runs)
            .map(|run| {
                let mut source = Source::new(g, outline, bounds.as_deref()?, run)?;
                source.display = source.decorated(doc, g).map(Arc::new);
                Some(Arc::new(source))
            })
            .collect();
        let steps = (0..n)
            .map(|s| {
                let cycle = cycle?;
                let step = cycle.steps.get(s)?;
                if let Some((run, child)) = cycle.split_run(s) {
                    let index = cycle.split_runs.iter().position(|r| std::ptr::eq(r, run))?;
                    let source = sources.get(index)?.as_ref()?;
                    let (lo, hi) = source.bracket(child)?;
                    Some(Resolved {
                        arrows: {
                            let nominal = source.arrows(g, step, child, cycle.gap_pt)?;
                            source
                                .display
                                .as_ref()
                                .and_then(|s| s.arrows(g, step, child, cycle.gap_pt))
                                .unwrap_or(nominal)
                        },
                        first: source.first + source.span * lo,
                        span: source.span * (hi - lo),
                    })
                } else {
                    let end = |i| ends.get(i).copied();
                    let (arrows, span) =
                        step_arrows(doc, g, outline, step, [end(s)?, end((s + 1) % n)?], n == 1)?;
                    Some(Resolved {
                        arrows,
                        first: end(s)?.0,
                        span,
                    })
                }
            })
            .collect();
        Self {
            doc,
            graphic: g,
            outline: outline.clone(),
            ends: ends.to_vec(),
            steps,
            sources,
        }
    }

    pub(in crate::cycles) fn station(&self, s: usize, lane: usize, p: Point) -> Option<f64> {
        let cycle = self.graphic.cycle.as_deref()?;
        let step = cycle.steps.get(s)?;
        if let Some((run, child)) = cycle.split_run(s) {
            let index = cycle.split_runs.iter().position(|r| std::ptr::eq(r, run))?;
            let source = self.sources.get(index)?.as_ref()?;
            let profile = source.lane(source.painted_lane(step, lane)?)?;
            return Some(profile.project(p, source.bracket(child)?)?.u);
        }
        let n = cycle.intermediates.len();
        let first = *self.ends.get(s)?;
        let last = *self.ends.get((s + 1) % n)?;
        let params = parameters(
            self.doc,
            self.graphic,
            &self.outline,
            step,
            [first, last],
            n == 1,
        )?;
        let profiles = deform::profiles(
            self.graphic.id,
            &self.outline,
            params.start,
            params.length,
            f64::from(DEFAULT.world(step.bow_pt)),
            step.reversible,
            &arrow_style(self.graphic),
        )?;
        let profile = match (step.reversible, lane) {
            (false, 0) => &profiles.center,
            (true, 0) => profiles.forward.as_ref()?,
            (true, 1) => profiles.reverse.as_ref()?,
            _ => return None,
        };
        Some(profile.project(p, (0., 1.))?.u)
    }

    pub(in crate::cycles) fn insertion(
        &self,
        s: usize,
        lane: usize,
        p: Point,
    ) -> Option<crate::cycles::split_runs::SplitStation> {
        let cycle = self.graphic.cycle.as_deref()?;
        let (start, span) = if let Some((run, _)) = cycle.split_run(s) {
            (run.source_start, run.source_span)
        } else {
            let n = cycle.intermediates.len();
            let params = parameters(
                self.doc,
                self.graphic,
                &self.outline,
                cycle.steps.get(s)?,
                [*self.ends.get(s)?, *self.ends.get((s + 1) % n)?],
                n == 1,
            )?;
            let start = (params.start / self.outline.length()).rem_euclid(1.);
            (
                if start >= 1. { 0. } else { start },
                params.length / self.outline.length(),
            )
        };
        Some(crate::cycles::split_runs::SplitStation {
            u: self.station(s, lane, p)?,
            start,
            span,
        })
    }

    pub(in crate::cycles) fn cut_intersects(&self, slot: usize) -> Option<bool> {
        let cycle = self.graphic.cycle.as_deref()?;
        for (index, run) in cycle.split_runs.iter().enumerate() {
            let n = cycle.intermediates.len();
            if let Some(cut) = (0..run.cuts.len()).find(|i| (run.first_step + i + 1) % n == slot) {
                let Some(source) = self.sources.get(index).and_then(Option::as_ref) else {
                    return Some(false);
                };
                let complete = (cut..=cut + 1).all(|child| {
                    let slot = (run.first_step + child) % n;
                    cycle
                        .steps
                        .get(slot)
                        .and_then(|step| source.arrows(self.graphic, step, child, cycle.gap_pt))
                        .is_some_and(|arrows| {
                            !arrows.is_empty() && arrows.iter().all(|a| a.validate().is_ok())
                        })
                });
                return Some(complete);
            }
        }
        None
    }
}

fn rect((lo, hi): Ink) -> [f64; 4] {
    [
        f64::from(lo.x),
        f64::from(lo.y),
        f64::from(hi.x),
        f64::from(hi.y),
    ]
}

pub(in crate::cycles) fn valid(doc: &Document, id: u64) -> bool {
    let Some(g) = doc.graphics.iter().find(|g| g.id == id) else {
        return false;
    };
    let Some(cycle) = g.cycle.as_deref() else {
        return false;
    };
    if cycle.split_runs.is_empty() {
        return true;
    }
    let Some(layout) = Layout::graphic(doc, g) else {
        return false;
    };
    cycle.split_runs.iter().all(|run| {
        (0..=run.cuts.len()).all(|child| {
            let slot = (run.first_step + child) % cycle.intermediates.len();
            layout
                .steps
                .get(slot)
                .and_then(Option::as_ref)
                .is_some_and(|step| {
                    !step.arrows.is_empty() && step.arrows.iter().all(|a| a.validate().is_ok())
                })
        })
    })
}
