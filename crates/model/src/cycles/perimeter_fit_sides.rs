//! Bounded relocation of inherited side ink after a fitted child is shortened.
use super::super::{
    Frame, Owners, arrow_style, names::arrow_hits, resolve_owned, sides as contacts,
};
use super::Layout;
use crate::{
    cycles::{
        self, Role, Target, center_of, grown,
        shapes::Shaft,
        split::{crossing, label_ink, points},
    },
    document::{Arrow, Document, Point},
    style::DEFAULT,
};
use std::collections::HashSet;

type Ink = (Point, Point);
const MAX_TRIALS: usize = 512;
const MAX_ROUTE_TRIALS: usize = 16_384;

struct Part {
    ids: Vec<u64>,
    incoming: bool,
    body: Ink,
    ink: Ink,
    boxes: Vec<Ink>,
    inside: bool,
    station: f64,
    old_fraction: f64,
}

struct Scene {
    own: Vec<Arrow>,
    foreign: Vec<Arrow>,
    ink: Vec<Ink>,
    gap: f32,
    stroke: f32,
    join: f32,
}

fn overlap(a: Ink, b: Ink) -> bool {
    a.0.x < b.1.x && b.0.x < a.1.x && a.0.y < b.1.y && b.0.y < a.1.y
}

fn translated(ink: Ink, delta: Point) -> Ink {
    (
        ink.0.offset(delta.x, delta.y),
        ink.1.offset(delta.x, delta.y),
    )
}

fn distance(a: Point, b: Point) -> f64 {
    (f64::from(a.x) - f64::from(b.x)).hypot(f64::from(a.y) - f64::from(b.y))
}

fn nearest(shafts: &[Option<Shaft>], point: Point) -> Option<(usize, f64, f64)> {
    shafts
        .iter()
        .enumerate()
        .filter_map(|(lane, shaft)| {
            let shaft = shaft.as_ref()?;
            let p = shaft.project(point)?;
            Some((lane, p.s / shaft.length(), p.distance))
        })
        .min_by(|a, b| a.2.total_cmp(&b.2).then_with(|| a.0.cmp(&b.0)))
}

impl Scene {
    fn new(
        doc: &Document,
        cycle: u64,
        step: usize,
        held: &HashSet<u64>,
        ignore: &HashSet<u64>,
    ) -> Option<Self> {
        let g = doc.graphics.iter().find(|g| g.id == cycle)?;
        let c = g.cycle.as_deref()?;
        let mut own = vec![];
        let mut foreign = doc.arrows.clone();
        let mut ink = vec![];
        for graphic in doc
            .graphics
            .iter()
            .filter(|g| g.cycle.is_some() && g.style.effects.opacity != Some(0.))
        {
            if graphic.id != cycle {
                // Other cycles keep their own renderer, including legacy
                // Circle geometry and malformed-frame compatibility.
                let drawn = cycles::drawn(doc, graphic);
                foreign.extend(drawn.arrows);
                ink.extend(drawn.labels.iter().map(label_ink));
                continue;
            }
            let mut drawn = cycles::Drawn::default();
            let mut owners = Owners::default();
            let outline = cycles::outline(graphic)?;
            resolve_owned(doc, graphic, &outline, &mut drawn, &mut owners, |_, _| {});
            for ((arrow, arc), owner) in drawn.arrows.into_iter().zip(drawn.arcs).zip(owners.arrows)
            {
                if graphic.id == cycle && owner == Some(step) {
                    if arc == Some(step) {
                        own.push(arrow);
                    }
                } else {
                    foreign.push(arrow);
                }
            }
            ink.extend(drawn.labels.iter().map(label_ink));
        }
        let units: Vec<_> = cycles::units(doc, &doc.all_ids())
            .into_iter()
            .filter(|ids| {
                ids.iter()
                    .all(|id| !held.contains(id) && !ignore.contains(id))
            })
            .chain(
                doc.graphics
                    .iter()
                    .filter(|g| g.cycle.is_none() && g.style.effects.opacity != Some(0.))
                    .map(|g| vec![g.id]),
            )
            .collect();
        // Captions are independent painted boxes. A union with their molecule
        // would incorrectly fill the empty corridor between distant captions.
        ink.extend(units.iter().flat_map(|ids| super::part_ink(doc, ids)));
        let gap = DEFAULT.world(c.gap_pt);
        Some(Self {
            own,
            foreign,
            ink,
            gap,
            stroke: DEFAULT.world(g.style.width_pt) * 1.5,
            join: gap.max(doc.drawing_style.bond_length_world * 2.),
        })
    }

    fn problem(
        &self,
        index: usize,
        routes: &[Arrow],
        ink: &[Vec<Ink>],
        active: &[bool],
    ) -> Option<&'static str> {
        let Some((route, b)) = routes.get(index).zip(ink.get(index)) else {
            return Some("missing side route/ink");
        };
        let touch = if route.appearance().head == crate::arrows::Head::None {
            route.end
        } else {
            route.start
        };
        let path = points(route);
        if b.iter().any(|b| arrow_hits(route, *b, 0.)) {
            return Some("side route enters its own body/caption");
        }
        if self.ink.iter().any(|other| {
            b.iter()
                .any(|b| overlap(grown(*b, self.gap * 0.5), grown(*other, self.gap * 0.5)))
                || arrow_hits(route, *other, self.gap * 0.3)
        }) {
            return Some("side body/route meets fixed ink or a label");
        }
        let joined = self
            .own
            .iter()
            .enumerate()
            .filter_map(|(i, a)| Some((i, Shaft::from_arrow(a)?.project(touch)?.distance)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .filter(|(_, distance)| *distance < 0.01)
            .map(|(i, _)| i);
        if joined.is_none() {
            return Some("side touch misses the painted own lane");
        }
        for (i, arrow) in self.own.iter().enumerate() {
            if b.iter().any(|b| arrow_hits(arrow, *b, self.gap * 0.3)) {
                return Some("own primary arrow enters side body/caption");
            }
            let old = points(arrow);
            if Some(i) == joined {
                if path
                    .windows(2)
                    .filter(|s| s.iter().all(|p| distance(*p, touch) > f64::from(self.join)))
                    .any(|s| crossing(s, &old, self.stroke))
                {
                    return Some("side route recrosses joined lane away from contact");
                }
            } else if crossing(&path, &old, self.stroke) {
                return Some("side route crosses opposite own lane");
            }
        }
        if self.foreign.iter().any(|a| {
            b.iter().any(|b| arrow_hits(a, *b, self.gap * 0.3))
                || crossing(&path, &points(a), self.stroke)
        }) {
            return Some("side body/route meets foreign arrow");
        }
        for (j, other) in ink
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != index && active.get(*j) == Some(&true))
        {
            let Some(other_route) = routes.get(j) else {
                return Some("missing other side route");
            };
            if b.iter().any(|b| {
                other
                    .iter()
                    .any(|other| overlap(grown(*b, self.gap * 0.5), grown(*other, self.gap * 0.5)))
            }) || other
                .iter()
                .any(|other| arrow_hits(route, *other, self.gap * 0.3))
                || b.iter()
                    .any(|b| arrow_hits(other_route, *b, self.gap * 0.3))
                || crossing(&path, &points(other_route), self.stroke)
            {
                return Some("side body/routes meet another active participant");
            }
        }
        None
    }
}

/// Check the exact current fitted child's full side batch without moving ink.
/// This remains valid after transaction/caption normalization or translation;
/// no inference about the pre-insertion source document is involved.
pub(super) fn valid(doc: &Document, cycle: u64, step: usize) -> bool {
    let Some(g) = doc.graphics.iter().find(|g| g.id == cycle) else {
        return false;
    };
    let Some(c) = g.cycle.as_deref() else {
        return false;
    };
    let Some(s) = c.steps.get(step) else {
        return false;
    };
    if c.split_run(step).is_none() || c.validate().is_err() {
        return false;
    }
    if s.reagents.is_empty() && s.products.is_empty() {
        return true;
    }
    let bond = doc.drawing_style.bond_length_world;
    let gap = DEFAULT.world(c.gap_pt);
    if !bond.is_finite() || bond <= 0. || !gap.is_finite() {
        return false;
    }
    let Some(layout) = Layout::graphic(doc, g) else {
        return false;
    };
    let Some(resolved) = layout.steps.get(step).and_then(Option::as_ref) else {
        return false;
    };
    let shafts: Vec<_> = resolved
        .arrows
        .iter()
        .filter_map(Shaft::from_arrow)
        .collect();
    let mut held = HashSet::new();
    let mut batch = vec![];
    let mut ink = vec![];
    for (ids, incoming) in s
        .reagents
        .iter()
        .map(|p| (p, true))
        .chain(s.products.iter().map(|p| (p, false)))
    {
        let linked = crate::molecule_names::include_annotations(doc, ids);
        held.extend(linked.iter().copied());
        // Match rendering's invisible/missing-ink omission without changing
        // the stored role order of the remaining visible participants.
        let Some(body) = crate::scene::selection_bounds(doc, ids) else {
            continue;
        };
        batch.push((incoming, grown(body, gap)));
        ink.push(super::part_ink(doc, ids));
    }
    let routes = contacts::arrows(g.id, &shafts, &batch, bond, &arrow_style(g));
    if routes.len() != batch.len() {
        return false;
    }
    let Some(scene) = Scene::new(doc, cycle, step, &held, &HashSet::new()) else {
        return false;
    };
    let active = vec![true; batch.len()];
    (0..batch.len()).all(|index| scene.problem(index, &routes, &ink, &active).is_none())
}

/// Accept only a complete clear plan; failure leaves `after` untouched.
pub(super) fn repair(before: &Document, after: &mut Document, target: &Target) -> bool {
    let Role::Intermediate(inserted) = target.role else {
        return true;
    };
    let Some(old_g) = before.graphics.iter().find(|g| g.id == target.cycle) else {
        return false;
    };
    let Some(old_c) = old_g.cycle.as_deref() else {
        return false;
    };
    let n = old_c.intermediates.len();
    if n == 0 {
        return true;
    }
    let i = inserted.min(n);
    let donor = (i + n - 1) % n;
    let child = if i == 0 { n } else { i - 1 };
    let Some(old_step) = old_c.steps.get(donor) else {
        return false;
    };
    if old_step.reagents.is_empty() && old_step.products.is_empty() {
        return true;
    }
    let Some(g) = after.graphics.iter().find(|g| g.id == target.cycle) else {
        return false;
    };
    let Some(c) = g.cycle.as_deref() else {
        return false;
    };
    let Some(step) = c.steps.get(child) else {
        return false;
    };
    if step.reagents != old_step.reagents || step.products != old_step.products {
        return false;
    }
    let (Some(old), Some(layout)) = (Layout::graphic(before, old_g), Layout::graphic(after, g))
    else {
        return false;
    };
    let Some(old_arrows) = old
        .steps
        .get(donor)
        .and_then(Option::as_ref)
        .map(|s| &s.arrows)
    else {
        return false;
    };
    let Some(arrows) = layout
        .steps
        .get(child)
        .and_then(Option::as_ref)
        .map(|s| &s.arrows)
    else {
        return false;
    };
    let old_shafts: Vec<_> = old_arrows.iter().map(Shaft::from_arrow).collect();
    let shafts: Vec<_> = arrows.iter().map(Shaft::from_arrow).collect();
    // Frame placement must use an actual painted forward shaft, never its
    // outline-only fallback when the fitted child has no usable shaft.
    if shafts.first().and_then(Option::as_ref).is_none() {
        return false;
    }
    let Some((run, ordinal)) = c.split_run(child) else {
        return false;
    };
    let Some(source) = c
        .split_runs
        .iter()
        .position(|r| std::ptr::eq(r, run))
        .and_then(|r| layout.sources.get(r))
        .and_then(Option::as_ref)
    else {
        return false;
    };
    let Some(bracket) = source.bracket(ordinal) else {
        return false;
    };
    let mut parts = vec![];
    for (ids, incoming) in old_step
        .reagents
        .iter()
        .map(|p| (p, true))
        .chain(old_step.products.iter().map(|p| (p, false)))
    {
        let Some(body) = crate::scene::selection_bounds(after, ids) else {
            continue;
        };
        let from = center_of(body);
        let Some((lane, t, _)) = nearest(&old_shafts, from) else {
            return false;
        };
        let Some(station) = old.station(donor, lane, from) else {
            return false;
        };
        let linked = crate::molecule_names::include_annotations(after, ids);
        let Some(ink) = crate::scene::selection_bounds(after, &linked) else {
            return false;
        };
        let Some(inside) = layout.outline.contains(from) else {
            return false;
        };
        parts.push(Part {
            ids: linked,
            incoming,
            body,
            ink,
            boxes: super::part_ink(after, ids),
            inside,
            station,
            old_fraction: t,
        });
    }
    if parts.is_empty() {
        return true;
    }
    let bond = after.drawing_style.bond_length_world;
    if !bond.is_finite() || bond <= 0. {
        return false;
    }
    let gap = DEFAULT.world(c.gap_pt);
    let held: HashSet<_> = parts.iter().flat_map(|p| p.ids.iter().copied()).collect();
    let Some(scene) = Scene::new(after, target.cycle, child, &held, &HashSet::new()) else {
        return false;
    };
    let draw = |shafts: &[Option<Shaft>], body: &[Ink]| {
        let batch: Vec<_> = parts
            .iter()
            .zip(body)
            .map(|(p, b)| (p.incoming, grown(*b, gap)))
            .collect();
        contacts::arrows(
            g.id,
            shafts.iter().filter_map(Option::as_ref),
            &batch,
            bond,
            &arrow_style(g),
        )
    };
    let mut body: Vec<_> = parts.iter().map(|p| p.body).collect();
    let mut ink: Vec<_> = parts.iter().map(|p| p.boxes.clone()).collect();
    let current = draw(&shafts, &body);
    if current.len() != parts.len() {
        return false;
    }
    let all = vec![true; parts.len()];
    let mut active = all.clone();
    let mut pending = vec![];
    for (index, part) in parts.iter().enumerate() {
        let stranded = part.old_fraction > 0.02
            && part.old_fraction < 0.98
            && (part.station < bracket.0 - 1e-8 || part.station > bracket.1 + 1e-8);
        let clear = scene.problem(index, &current, &ink, &all).is_none();
        // The affected donor is checked as a complete batch. Existing unclear
        // donor ink must also find a clear local plan; other steps stay fixed.
        if stranded || !clear {
            if let Some(enabled) = active.get_mut(index) {
                *enabled = false;
            }
            pending.push(index);
        }
    }
    if pending.is_empty() {
        return true;
    }
    pending.sort_by(|a, b| match (parts.get(*a), parts.get(*b)) {
        (Some(a), Some(b)) => a.station.total_cmp(&b.station),
        _ => a.cmp(b),
    });
    let budget = MAX_TRIALS.min(MAX_ROUTE_TRIALS / parts.len());
    let mut trials = 0;
    let mut candidate = after.clone();
    for index in pending {
        let Some(part) = parts.get(index) else {
            return false;
        };
        let from = center_of(part.body);
        let Some((lane, t, _)) = nearest(&shafts, from) else {
            return false;
        };
        let preferred = if lane == 1 { 1. - t } else { t }.clamp(0.15, 0.85);
        let mut fractions: Vec<_> = std::iter::once(preferred)
            .chain((0..=24).map(|k| 0.15 + 0.7 * k as f64 / 24.))
            .collect();
        fractions.sort_by(|a, b| {
            (a - preferred)
                .abs()
                .total_cmp(&(b - preferred).abs())
                .then_with(|| a.total_cmp(b))
        });
        let w = 2.
            * (from.x - part.ink.0.x)
                .abs()
                .max((part.ink.1.x - from.x).abs());
        let h = 2.
            * (from.y - part.ink.0.y)
                .abs()
                .max((part.ink.1.y - from.y).abs());
        let mut best = None;
        if let Some(flag) = active.get_mut(index) {
            *flag = true;
        }
        'search: for extra in [0., bond * 0.5, bond, bond * 2.] {
            for &fraction in &fractions {
                let Some(frame) = Frame::at(
                    [
                        shafts.first().and_then(Option::as_ref),
                        shafts.get(1).and_then(Option::as_ref),
                    ],
                    &layout.outline,
                    0.,
                    fraction,
                    if g.cycle_counterclockwise() { -1. } else { 1. },
                ) else {
                    continue;
                };
                let Some(to) = cycles::tidy_shape::place(
                    frame,
                    &layout.outline,
                    (w, h),
                    gap + bond + extra,
                    bond,
                    part.inside,
                ) else {
                    continue;
                };
                let cost = distance(from, to);
                if best.as_ref().is_some_and(|(_, _, _, old)| cost >= *old) {
                    continue;
                }
                if trials >= budget {
                    break 'search;
                }
                trials += 1;
                let delta = Point::new(to.x - from.x, to.y - from.y);
                let trial_body = translated(part.body, delta);
                let trial_ink: Vec<_> = part.boxes.iter().map(|b| translated(*b, delta)).collect();
                let Some((b, full)) = body.get_mut(index).zip(ink.get_mut(index)) else {
                    return false;
                };
                *b = trial_body;
                *full = trial_ink.clone();
                let routes = draw(&shafts, &body);
                if routes.len() != parts.len() {
                    continue;
                }
                let clear = parts.iter().enumerate().all(|(j, _)| {
                    if active.get(j) != Some(&true) {
                        return true;
                    }
                    scene.problem(j, &routes, &ink, &active).is_none()
                });
                if clear && cycles::resize::rigid_bonds(&candidate, &part.ids, delta) {
                    best = Some((to, trial_body, trial_ink, cost));
                }
            }
        }
        let Some((to, b, full, _)) = best else {
            return false;
        };
        let delta = Point::new(to.x - from.x, to.y - from.y);
        candidate.translate(&part.ids, delta.x, delta.y);
        if let Some(current) = body.get_mut(index) {
            *current = b;
        }
        if let Some(current) = ink.get_mut(index) {
            *current = full;
        }
    }
    // Names and anchored graphics may react to translations. Rebuild the exact
    // final scene using the same validator as prepared-transaction preflight.
    if !valid(&candidate, target.cycle, child) || candidate.validate().is_err() {
        return false;
    }
    *after = candidate;
    true
}

#[cfg(test)]
#[path = "perimeter_fit_side_tests.rs"]
mod tests;
