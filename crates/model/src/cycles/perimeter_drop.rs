//! Noncircle drops and ownership follow native painted lanes, not a mean circle.
use super::{Cycle, Drawn, Outline, Role, center_of, outline, perimeter_arcs, shapes::Shaft};
use crate::{
    document::{Arrow, Document, Point},
    graphics::Graphic,
};

type Ink = (Point, Point);

pub(super) enum Drop {
    Miss,
    /// The foremost painted hit cannot be split without adding a full lap.
    Blocked,
    Target(Role, Vec<Arrow>),
}

struct Geometry<'a> {
    doc: &'a Document,
    graphic: &'a Graphic,
    cycle: &'a Cycle,
    outline: Outline,
    ends: Vec<(f64, Option<Ink>)>,
    dir: f64,
}

struct Route {
    arrows: Vec<Arrow>,
    first: f64,
    span: f64,
}

impl<'a> Geometry<'a> {
    fn new(doc: &'a Document, graphic: &'a Graphic) -> Option<Self> {
        let cycle = graphic.cycle.as_deref()?;
        let n = cycle.intermediates.len();
        let bond = doc.drawing_style.bond_length_world;
        if n > super::MAX_PARTS
            || cycle.steps.len() != n
            || !bond.is_finite()
            || bond <= 0.
            || !cycle.gap_pt.is_finite()
        {
            return None;
        }
        let outline = outline(graphic)?;
        let dir = if graphic.cycle_counterclockwise() {
            -1.
        } else {
            1.
        };
        let (center, radius) = graphic.cycle_circle();
        let top = outline
            .project(center.offset(0., -2. * radius))
            .map_or(0., |p| p.s);
        let ends = crate::scene::selections_bounds(doc, &cycle.intermediates)
            .into_iter()
            .enumerate()
            .map(|(i, ink)| {
                let s = ink.and_then(|b| outline.project(center_of(b))).map_or_else(
                    || {
                        (top + dir * outline.length() * i as f64 / n as f64)
                            .rem_euclid(outline.length())
                    },
                    |p| p.s,
                );
                (s, ink)
            })
            .collect();
        Some(Self {
            doc,
            graphic,
            cycle,
            outline,
            ends,
            dir,
        })
    }

    fn route(&self, setting: usize, first: usize, last: usize) -> Option<Route> {
        let ends = [*self.ends.get(first)?, *self.ends.get(last)?];
        let (arrows, span) = perimeter_arcs::step_arrows(
            self.doc,
            self.graphic,
            &self.outline,
            self.cycle.steps.get(setting)?,
            ends,
            first == last,
        )?;
        Some(Route {
            arrows,
            first: ends[0].0,
            span,
        })
    }

    fn splittable(&self, route: &Route, p: Point) -> bool {
        let Some(projected) = self.outline.project(p) else {
            return false;
        };
        let along = (self.dir * (projected.s - route.first)).rem_euclid(self.outline.length());
        let epsilon = self.outline.length() * 1e-9;
        // Newly constructed children follow their native projected anchors.
        // A far inward Bow can project outside the donor's ordered interval.
        // Do not advertise that insertion: it would silently add a whole lap.
        along > epsilon && along < route.span.abs() - epsilon
    }
}

pub(super) fn target(doc: &Document, graphic: &Graphic, p: Point) -> Drop {
    if !p.is_finite() {
        return Drop::Miss;
    }
    let Some(geometry) = Geometry::new(doc, graphic) else {
        return Drop::Miss;
    };
    let n = geometry.ends.len();
    let bond = doc.drawing_style.bond_length_world;
    if n == 0 {
        return perimeter_arcs::empty_arrow(graphic, &geometry.outline)
            .filter(|arrow| arrow.hit(p, 0.6 * bond))
            .map_or(Drop::Miss, |arrow| {
                Drop::Target(Role::Intermediate(0), vec![arrow])
            });
    }
    let routes: Vec<_> = (0..n)
        .map(|s| {
            geometry
                .cycle
                .steps
                .get(s)
                .filter(|step| !step.hidden)
                .and_then(|_| geometry.route(s, s, (s + 1) % n))
        })
        .collect();
    // Rank actual visible hits before side zones; strict distance ties retain
    // the first step and original lane index, including a reverse-only lane.
    let mut hit = None;
    let mut near = None;
    for (s, route) in routes.iter().enumerate() {
        let Some(route) = route else {
            continue;
        };
        for (lane, arrow) in route.arrows.iter().enumerate() {
            let Some(shaft) = Shaft::from_arrow(arrow) else {
                continue;
            };
            let Some(projected) = shaft.project(p) else {
                continue;
            };
            if arrow.hit(p, 0.6 * bond)
                && hit.is_none_or(|(_, distance)| projected.distance < distance)
            {
                hit = Some((s, projected.distance));
            }
            if near.is_none_or(|(_, _, distance)| projected.distance < distance) {
                let t = projected.s / shaft.length();
                near = Some((s, if lane == 1 { 1. - t } else { t }, projected.distance));
            }
        }
    }
    if let Some((s, _)) = hit {
        let Some(route) = routes.into_iter().nth(s).flatten() else {
            return Drop::Miss;
        };
        return if geometry.splittable(&route, p) {
            Drop::Target(Role::Intermediate(s + 1), route.arrows)
        } else {
            Drop::Blocked
        };
    }
    let Some((s, t, distance)) = near else {
        return Drop::Miss;
    };
    if distance >= f64::from(4. * bond) {
        return Drop::Miss;
    }
    // Match Circle's central 35% exclusion only for new side roles. Scaling
    // about the affine center keeps the core faithful to each actual outline.
    let center = graphic.cycle_circle().0;
    let core = Point::new(
        (f64::from(center.x) + (f64::from(p.x) - f64::from(center.x)) / 0.35) as f32,
        (f64::from(center.y) + (f64::from(p.y) - f64::from(center.y)) / 0.35) as f32,
    );
    if geometry
        .outline
        .locate(core)
        .is_none_or(|(inside, projection)| inside || projection.distance <= 1e-6)
    {
        return Drop::Miss;
    }
    let role = if t < 0.5 {
        Role::Reagent(s)
    } else {
        Role::Product(s)
    };
    routes
        .into_iter()
        .nth(s)
        .flatten()
        .map_or(Drop::Miss, |route| Drop::Target(role, route.arrows))
}

pub(super) fn holds(doc: &Document, graphic: &Graphic, part: &[u64], point: Point) -> bool {
    let Some(geometry) = Geometry::new(doc, graphic) else {
        return false;
    };
    let reach = f64::from(4. * doc.drawing_style.bond_length_world);
    let Some((inside, projection)) = geometry.outline.locate(point) else {
        return false;
    };
    if inside || projection.distance < reach {
        return true;
    }
    let Some((role, _)) = geometry
        .cycle
        .parts()
        .find(|(_, ids)| ids.iter().any(|id| part.contains(id)))
    else {
        return false;
    };
    let n = geometry.ends.len();
    let near = |setting, first, last| {
        geometry.route(setting, first, last).is_some_and(|route| {
            route
                .arrows
                .iter()
                .filter_map(Shaft::from_arrow)
                .any(|shaft| shaft.project(point).is_some_and(|p| p.distance < reach))
        })
    };
    match role {
        Role::Reagent(s) | Role::Product(s) if s < n => near(s, s, (s + 1) % n),
        Role::Intermediate(i) if i < n => {
            let before = (i + n - 1) % n;
            let next = (i + 1) % n;
            near(before, before, i)
                || near(i, i, next)
                || (n > 1 && {
                    // After insertion the donor's settings precede the member;
                    // reversing moves those settings to its successor instead.
                    [before, i].into_iter().any(|s| {
                        geometry
                            .cycle
                            .steps
                            .get(s)
                            .is_some_and(|step| step.bow_pt != 0. || step.reversible)
                            && near(s, before, next)
                    })
                })
        }
        _ => false,
    }
}

pub(super) fn step_at(doc: &Document, graphic: &Graphic, p: Point) -> Option<usize> {
    let geometry = Geometry::new(doc, graphic)?;
    let mut drawn = Drawn::default();
    let mut owners = perimeter_arcs::Owners::default();
    perimeter_arcs::resolve_owned(
        doc,
        graphic,
        &geometry.outline,
        &mut drawn,
        &mut owners,
        |_, _| {},
    );
    // A hidden step's name or side arrow still belongs to that step. Resolve
    // decoration ownership before considering the nearest ordinary shaft.
    for ((at, text, format), &owner) in drawn.labels.iter().zip(&owners.labels) {
        let size = crate::typography::layout(text, format);
        if super::contains((*at, at.offset(size.width, size.height)), p) {
            return Some(owner);
        }
    }
    if let Some(owner) = drawn
        .arrows
        .iter()
        .zip(&owners.arrows)
        .find_map(|(arrow, owner)| {
            arrow
                .hit(p, 0.6 * doc.drawing_style.bond_length_world)
                .then_some(*owner)
                .flatten()
        })
    {
        return Some(owner);
    }
    let mut best = None;
    for (s, route) in (0..geometry.ends.len()).filter_map(|s| {
        geometry
            .route(s, s, (s + 1) % geometry.ends.len())
            .map(|r| (s, r))
    }) {
        for shaft in route.arrows.iter().filter_map(Shaft::from_arrow) {
            if let Some(projection) = shaft.project(p)
                && best.is_none_or(|(_, distance)| projection.distance < distance)
            {
                best = Some((s, projection.distance));
            }
        }
    }
    best.map(|(s, _)| s)
}

#[cfg(test)]
#[path = "perimeter_drop_tests.rs"]
mod tests;
