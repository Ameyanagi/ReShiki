//! Catalytic cycles. A cycle is a circle graphic that holds ordered
//! intermediates, each one molecule or one text, and the steps between them.
//! Its arc arrows, side arrows and step names are worked out from where the
//! parts sit each time it is drawn and are never stored, so moving a part
//! refits every arrow, and reaction detection never sees them.
use crate::{
    arrows::{ArrowStyle, Head, Preset},
    document::{Arrow, Document, Point},
    graphics::{Graphic, GraphicKind},
    style::DEFAULT,
    typography::{TextAlign, TextFormat},
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::f32::consts::{FRAC_PI_2, TAU};

/// Clear space between an arc's end and a part's ink, in points.
pub const GAP_PT: f32 = 6.;
/// The most intermediates, and the most reagents and products of one step.
const MAX_PARTS: usize = 64;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Cycle {
    /// Steps run counterclockwise on the page.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub counterclockwise: bool,
    /// Each intermediate in cycle order: one molecule's atom IDs or one
    /// text's ID.
    pub intermediates: Vec<Vec<u64>>,
    /// Step i runs from intermediate i to the next; one per intermediate.
    pub steps: Vec<Step>,
    /// Clear space between an arc's ends and an intermediate's ink, in points.
    pub gap_pt: f32,
    /// Text in the middle of the cycle.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub label: String,
}
impl Default for Cycle {
    fn default() -> Self {
        Self {
            counterclockwise: false,
            intermediates: vec![],
            steps: vec![],
            gap_pt: GAP_PT,
            label: String::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Step {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Drawn as a pair of half-headed arcs.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub reversible: bool,
    /// No arc; the step keeps its side arrows and name.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
    /// Extra clear space after the step's first intermediate, in points.
    #[serde(skip_serializing_if = "is_zero")]
    pub lead_pt: f32,
    /// Extra clear space before the step's last intermediate, in points.
    #[serde(skip_serializing_if = "is_zero")]
    pub trail_pt: f32,
    /// How far the arc's middle bows out of the circle, in points; below 0
    /// it bows in.
    #[serde(skip_serializing_if = "is_zero")]
    pub bow_pt: f32,
    /// Reagents coming in: one molecule's atom IDs or one text's ID each.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub reagents: Vec<Vec<u64>>,
    /// Products going out, as reagents.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub products: Vec<Vec<u64>>,
}
fn is_zero(v: &f32) -> bool {
    *v == 0.
}

/// What a part does in its cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Intermediate n, counted from 0.
    Intermediate(usize),
    /// A reagent of step n, counted from 0.
    Reagent(usize),
    /// A product of step n, counted from 0.
    Product(usize),
}
impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Intermediate(i) => write!(f, "Intermediate {}", i + 1),
            Self::Reagent(s) => write!(f, "Reagent of step {}", s + 1),
            Self::Product(s) => write!(f, "Product of step {}", s + 1),
        }
    }
}

impl Cycle {
    pub fn validate(&self) -> Result<(), String> {
        let range = |v: f32, lo: f32, hi: f32| v.is_finite() && (lo..=hi).contains(&v);
        if self.steps.len() != self.intermediates.len() {
            return Err("A cycle has one step per intermediate".into());
        }
        if self.intermediates.len() > MAX_PARTS
            || self
                .steps
                .iter()
                .any(|s| s.reagents.len() > MAX_PARTS || s.products.len() > MAX_PARTS)
        {
            return Err("A cycle has at most 64 intermediates and 64 side parts per step".into());
        }
        if !range(self.gap_pt, 0., 72.)
            || self.steps.iter().any(|s| {
                !range(s.lead_pt, -72., 720.)
                    || !range(s.trail_pt, -72., 720.)
                    || !range(s.bow_pt, -720., 720.)
            })
        {
            return Err("Cycle gaps and bows are out of range".into());
        }
        if self.label.chars().count() > 200
            || self.steps.iter().any(|s| s.name.chars().count() > 200)
        {
            return Err("Cycle labels and step names have at most 200 characters".into());
        }
        Ok(())
    }
    /// Every part with its role, intermediates first.
    pub fn parts(&self) -> impl Iterator<Item = (Role, &Vec<u64>)> {
        let intermediates = self
            .intermediates
            .iter()
            .enumerate()
            .map(|(i, ids)| (Role::Intermediate(i), ids));
        let sides = self.steps.iter().enumerate().flat_map(|(s, step)| {
            step.reagents
                .iter()
                .map(move |ids| (Role::Reagent(s), ids))
                .chain(step.products.iter().map(move |ids| (Role::Product(s), ids)))
        });
        intermediates.chain(sides)
    }
    /// Every part ID, for code that renumbers objects.
    pub(crate) fn ids_mut(&mut self) -> impl Iterator<Item = &mut u64> {
        self.parts_mut().flatten()
    }
    fn parts_mut(&mut self) -> impl Iterator<Item = &mut Vec<u64>> {
        self.intermediates.iter_mut().chain(
            self.steps
                .iter_mut()
                .flat_map(|s| s.reagents.iter_mut().chain(s.products.iter_mut())),
        )
    }
    /// The role of the part holding `id`.
    pub fn role(&self, id: u64) -> Option<Role> {
        self.parts()
            .find(|(_, ids)| ids.contains(&id))
            .map(|(role, _)| role)
    }
    /// Drop intermediate `i`. The steps on either side become one, which
    /// keeps the earlier step's settings and both steps' side parts.
    fn remove_intermediate(&mut self, i: usize) {
        self.intermediates.remove(i);
        let gone = self.steps.remove(i);
        if let Some(before) = self
            .steps
            .len()
            .checked_sub(1)
            .map(|last| if i == 0 { last } else { i - 1 })
            && let Some(step) = self.steps.get_mut(before)
        {
            step.reagents.extend(gone.reagents);
            step.products.extend(gone.products);
            if step.name.trim().is_empty() {
                step.name = gone.name;
            }
        }
    }
}

impl Graphic {
    /// A cycle's center and radius.
    pub fn cycle_circle(&self) -> (Point, f32) {
        let center = self.origin.offset(
            (self.axis_x.x + self.axis_y.x) * 0.5,
            (self.axis_x.y + self.axis_y.y) * 0.5,
        );
        let radius = (self.axis_x.distance(Point::default())
            + self.axis_y.distance(Point::default()))
            * 0.25;
        (center, radius)
    }
    /// Whether the cycle's steps run counterclockwise on the page; a
    /// mirrored cycle runs the other way round.
    pub fn cycle_counterclockwise(&self) -> bool {
        self.cycle
            .as_ref()
            .is_some_and(|c| c.counterclockwise != (self.frame_determinant() < 0.))
    }
}

/// What a cycle draws, in drawing coordinates.
#[derive(Debug, Default)]
pub struct Drawn {
    pub arrows: Vec<Arrow>,
    /// Step names and the center label: top-left corner, text and format.
    pub labels: Vec<(Point, String, TextFormat)>,
}

/// One step's arc: on the circle about `center` with `radius`, from
/// `first` degrees through a signed `sweep`, clockwise positive.
#[derive(Debug, Clone, Copy)]
struct Span {
    center: Point,
    radius: f32,
    first: f32,
    sweep: f32,
}
impl Span {
    fn angle(self, t: f32) -> f32 {
        (self.first + self.sweep * t).to_radians()
    }
    fn at(self, t: f32, out: f32) -> Point {
        polar(self.center, self.radius + out, self.angle(t))
    }
    /// The way on at `t`, a unit vector.
    fn tangent(self, t: f32) -> Point {
        let a = self.angle(t);
        let s = self.sweep.signum();
        Point::new(-a.sin() * s, a.cos() * s)
    }
    /// Where along the arc a point lies, 0 at its start and 1 at its end.
    fn along(self, p: Point) -> f32 {
        if self.sweep.abs() < 1e-3 {
            return 0.5;
        }
        let angle = (p.y - self.center.y)
            .atan2(p.x - self.center.x)
            .to_degrees();
        let middle = self.first + self.sweep * 0.5;
        let delta = (angle - middle + 180.).rem_euclid(360.) - 180.;
        0.5 + delta / self.sweep
    }
}

fn polar(center: Point, radius: f32, angle: f32) -> Point {
    center.offset(radius * angle.cos(), radius * angle.sin())
}
fn center_of((lo, hi): (Point, Point)) -> Point {
    lo.lerp(hi, 0.5)
}
fn grown((lo, hi): (Point, Point), by: f32) -> (Point, Point) {
    (lo.offset(-by, -by), hi.offset(by, by))
}
fn contains((lo, hi): (Point, Point), p: Point) -> bool {
    p.x > lo.x && p.x < hi.x && p.y > lo.y && p.y < hi.y
}
/// Where the ray from a box's center toward `p` leaves the box.
fn edge((lo, hi): (Point, Point), p: Point) -> Point {
    let c = lo.lerp(hi, 0.5);
    let (dx, dy) = (p.x - c.x, p.y - c.y);
    let (hx, hy) = ((hi.x - lo.x) * 0.5, (hi.y - lo.y) * 0.5);
    let scale = [(dx, hx), (dy, hy)]
        .into_iter()
        .filter(|(d, _)| d.abs() > 1e-6)
        .map(|(d, h)| h / d.abs())
        .fold(f32::INFINITY, f32::min);
    if scale.is_finite() && scale < 1. {
        c.offset(dx * scale, dy * scale)
    } else {
        c
    }
}

/// An arc arrow from `start` to `end` with a signed sweep in degrees.
fn arrow(id: u64, start: Point, end: Point, sweep: f32, style: &ArrowStyle) -> Arrow {
    let mut a = Arrow::new(id, start, end, Preset::Forward, style.clone());
    a.set_arc(sweep);
    a
}

/// The ink box of each part, with the ones that draw nothing left out.
fn boxes(doc: &Document, cycle: &Cycle) -> Vec<Option<(Point, Point)>> {
    let parts: Vec<Vec<u64>> = cycle.parts().map(|(_, ids)| ids.clone()).collect();
    crate::scene::selections_bounds(doc, &parts)
}

/// The cycle's arrows and labels as they would be drawn now.
pub fn drawn(doc: &Document, g: &Graphic) -> Drawn {
    let Some(cycle) = &g.cycle else {
        return Drawn::default();
    };
    let mut out = Drawn::default();
    let (center, radius) = g.cycle_circle();
    if !(center.is_finite() && radius.is_finite() && radius > 1e-3) {
        return out;
    }
    let dir = if g.cycle_counterclockwise() { -1. } else { 1. };
    let bond = doc.drawing_style.bond_length_world;
    let gap = DEFAULT.world(cycle.gap_pt);
    let style = ArrowStyle {
        color: g.style.stroke,
        width_pt: g.style.width_pt,
        pattern: g.style.pattern,
        ..ArrowStyle::preset(Preset::Forward)
    };
    let format = TextFormat {
        style: doc.drawing_style.text_style(),
        alignment: TextAlign::Center,
        ..Default::default()
    };
    let n = cycle.intermediates.len();
    let all = boxes(doc, cycle);
    let at = |angle: f32| polar(center, radius, angle);
    if n == 0 {
        // A whole circle, open at the top with its head there.
        let start = -FRAC_PI_2 + dir * 0.02;
        let sweep = dir * (360. - 2. * 0.02_f32.to_degrees());
        out.arrows.push(arrow(
            g.id,
            at(start),
            at(start + sweep.to_radians()),
            sweep,
            &style,
        ));
    }
    let mut boxes: Vec<(Point, Point)> = Vec::with_capacity(n);
    let mut angles = Vec::with_capacity(n);
    for (i, b) in all.iter().take(n).enumerate() {
        let angle = b.map_or(-FRAC_PI_2 + dir * TAU * i as f32 / n as f32, |b| {
            let c = center_of(b);
            (c.y - center.y).atan2(c.x - center.x)
        });
        boxes.push(b.unwrap_or_else(|| (at(angle), at(angle))));
        angles.push(angle);
    }
    // What labels keep clear of: every part's ink.
    let mut taken: Vec<(Point, Point)> = all.iter().flatten().copied().collect();
    let mut sides = all.iter().skip(n);
    let walk = 0.25_f32.to_radians();
    // Half the angle a box spans as seen from the center, about `mid`.
    let seen = |(lo, hi): (Point, Point), mid: f32| -> f32 {
        [lo, Point::new(hi.x, lo.y), hi, Point::new(lo.x, hi.y)]
            .into_iter()
            .map(|p| {
                let a = (p.y - center.y).atan2(p.x - center.x);
                ((a - mid + std::f32::consts::PI).rem_euclid(TAU) - std::f32::consts::PI).abs()
            })
            .fold(0., f32::max)
    };
    let mut names = vec![];
    let ends: Vec<((Point, Point), f32)> = boxes.into_iter().zip(angles).collect();
    for (s, step) in cycle.steps.iter().enumerate().take(n) {
        let (Some(&(first, a0)), Some(&(next, a1))) = (ends.get(s), ends.get((s + 1) % n)) else {
            continue;
        };
        let mut span = (dir * (a1 - a0)).rem_euclid(TAU);
        if span < 1e-4 {
            span = TAU;
        }
        // Leave the first intermediate's ink, then stop short of the next.
        let clear = |b: (Point, Point)| grown(b, gap);
        // An intermediate moved off the circle keeps an opening as wide as
        // it looks from the center, so the arcs still point at it.
        let mut from = 0.;
        while from < span * 0.5 && contains(clear(first), at(a0 + dir * from)) {
            from += walk;
        }
        if from == 0. {
            from = seen(clear(first), a0).min(span * 0.5);
        }
        let mut to = span;
        while to > span * 0.5 && contains(clear(next), at(a0 + dir * to)) {
            to -= walk;
        }
        if to == span {
            to = span - seen(clear(next), a1).min(span * 0.5);
        }
        if to - from > 0.02 {
            // Extra space never runs into the ink, and the arc keeps most of
            // a bond length where the step has room.
            let room = to - from - (0.8 * bond / radius).min(to - from);
            let floor = -cycle.gap_pt;
            let mut lead = DEFAULT.world(step.lead_pt.max(floor)) / radius;
            let mut trail = DEFAULT.world(step.trail_pt.max(floor)) / radius;
            if lead + trail > room && lead + trail > 0. {
                let k = room / (lead + trail);
                lead *= k;
                trail *= k;
            }
            from += lead;
            to -= trail;
        } else {
            from = (from + to) * 0.5;
            to = from;
        }
        // A lone intermediate's arc stays short of a whole circle.
        let most = 358_f32.to_radians();
        if to - from > most {
            let extra = (to - from - most) * 0.5;
            from += extra;
            to -= extra;
        }
        let start = at(a0 + dir * from);
        let end = at(a0 + dir * to);
        let flat = Span {
            center,
            radius,
            first: (a0 + dir * from).to_degrees(),
            sweep: (dir * (to - from)).to_degrees(),
        };
        // A bowed arc runs on a circle of its own through the same ends.
        let bow = DEFAULT
            .world(step.bow_pt)
            .clamp(-0.9 * radius, 0.9 * radius);
        let arc = (bow != 0. && to > from)
            .then(|| {
                let middle = polar(center, radius + bow, a0 + dir * (from + to) * 0.5);
                let sweep = crate::arrows::sweep_through(start, middle, end)?;
                let (c, r, first) = crate::arrows::arc_circle(start, end, sweep)?;
                Some(Span {
                    center: c,
                    radius: r,
                    first,
                    sweep,
                })
            })
            .flatten()
            .unwrap_or(flat);
        // 1 when moving away from the arc's own center leaves the cycle.
        let outward = {
            let p = arc.at(0.5, 0.);
            let dot =
                (p.x - arc.center.x) * (p.x - center.x) + (p.y - arc.center.y) * (p.y - center.y);
            if dot >= 0. { 1. } else { -1. }
        };
        if !step.hidden && to > from {
            if step.reversible {
                // Two half-headed arcs either side of the line, the forward
                // one on the outside.
                let half = DEFAULT.world(style.gap_pt.max(2.)) * 0.75;
                // Half heads face away from the pair.
                let forward = ArrowStyle {
                    head: if (arc.sweep > 0.) == (outward > 0.) {
                        Head::Left
                    } else {
                        Head::Right
                    },
                    ..style.clone()
                };
                out.arrows.push(arrow(
                    g.id,
                    arc.at(0., outward * half),
                    arc.at(1., outward * half),
                    arc.sweep,
                    &forward,
                ));
                out.arrows.push(arrow(
                    g.id,
                    arc.at(1., -outward * half),
                    arc.at(0., -outward * half),
                    -arc.sweep,
                    &forward,
                ));
            } else {
                out.arrows.push(arrow(g.id, start, end, arc.sweep, &style));
            }
        }
        // Side arrows swoop in to the arc, or out of it, meeting it along
        // its way, far enough along from their end for an easy curve.
        let length = arc.radius * arc.sweep.to_radians().abs();
        for incoming in step
            .reagents
            .iter()
            .map(|_| true)
            .chain(step.products.iter().map(|_| false))
        {
            let Some(&Some(b)) = sides.next() else {
                continue;
            };
            let c = center_of(b);
            let ahead = (1.2 * bond).max(1.1 * (c.distance(arc.center) - arc.radius).abs());
            let shift = if length > 0. { ahead / length } else { 0. };
            let t = (arc.along(c) + if incoming { shift } else { -shift }).clamp(0.15, 0.85);
            let side = side_arrow(
                g.id,
                arc.at(t, 0.),
                arc.tangent(t),
                grown(b, gap),
                incoming,
                &style,
            );
            taken.push(side.bounds());
            out.arrows.push(side);
        }
        if !step.name.trim().is_empty() {
            names.push((wrapped(&step.name, &format, bond * 2.5), arc, outward));
        }
    }
    // Names go where they cover the least ink: inside the cycle at the arc's
    // middle when that is free, else outside, else a little along the arc.
    for (text, arc, outward) in names {
        let layout = crate::typography::layout(&text, &format);
        let (w, h) = (layout.width, layout.height);
        let place = |t: f32, out: f32| {
            let angle = arc.angle(t);
            let reach = 0.5 * (w * angle.cos().abs() + h * angle.sin().abs());
            let c = polar(arc.center, arc.radius + out * (gap + reach), angle);
            (c.offset(-w * 0.5, -h * 0.5), c.offset(w * 0.5, h * 0.5))
        };
        // Ink the box covers, and the whole box again if it reaches the arc.
        let covered = |b: (Point, Point), out: f32| -> f32 {
            let inked: f32 = taken
                .iter()
                .map(|t| grown(*t, gap * 0.5))
                .map(|t| {
                    let x = (b.1.x.min(t.1.x) - b.0.x.max(t.0.x)).max(0.);
                    let y = (b.1.y.min(t.1.y) - b.0.y.max(t.0.y)).max(0.);
                    x * y
                })
                .sum();
            let c = arc.center;
            let nearest = Point::new(c.x.clamp(b.0.x, b.1.x), c.y.clamp(b.0.y, b.1.y));
            let farthest = Point::new(
                if c.x - b.0.x > b.1.x - c.x {
                    b.0.x
                } else {
                    b.1.x
                },
                if c.y - b.0.y > b.1.y - c.y {
                    b.0.y
                } else {
                    b.1.y
                },
            );
            let clear = if out < 0. {
                farthest.distance(c) < arc.radius - gap * 0.5
            } else {
                nearest.distance(c) > arc.radius + gap * 0.5
            };
            inked + if clear { 0. } else { w * h }
        };
        let mut best: Option<((Point, Point), f32)> = None;
        'search: for depth in [1., 1.8] {
            for t in [0.5, 0.38, 0.62, 0.26, 0.74] {
                for out in [-outward, outward] {
                    let b = place(t, out * depth);
                    let cost = covered(b, out);
                    if best.is_none_or(|(_, c)| cost < c) {
                        best = Some((b, cost));
                    }
                    if cost <= 0. {
                        break 'search;
                    }
                }
            }
        }
        if let Some((chosen, _)) = best {
            taken.push(chosen);
            out.labels.push((chosen.0, text, format.clone()));
        }
    }
    if !cycle.label.trim().is_empty() {
        let layout = crate::typography::layout(&cycle.label, &format);
        out.labels.push((
            center.offset(-layout.width * 0.5, -layout.height * 0.5),
            cycle.label.clone(),
            format,
        ));
    }
    out
}

/// A side arrow between a part's ink and the point where it touches the
/// step's arc, heading the arc's way there. A reagent comes in headless; a
/// product goes out with a head.
fn side_arrow(
    id: u64,
    touch: Point,
    way: Point,
    ink: (Point, Point),
    incoming: bool,
    style: &ArrowStyle,
) -> Arrow {
    let style = ArrowStyle {
        head: if incoming { Head::None } else { Head::Full },
        ..style.clone()
    };
    // The curve meets the arc along it: its control there sits on the way.
    let reach = 0.5 * touch.distance(center_of(ink));
    let sign = if incoming { -1. } else { 1. };
    let near = touch.offset(sign * way.x * reach, sign * way.y * reach);
    // A part right beside the arc is met from the touch point instead.
    let end = edge(ink, if contains(ink, near) { touch } else { near });
    let far = end.lerp(near, 0.5);
    let (start, finish, controls) = if incoming {
        (end, touch, [far, near])
    } else {
        (touch, end, [near, far])
    };
    let mut a = Arrow::new(id, start, finish, Preset::Forward, style);
    a.cubic = Some(controls);
    a
}

/// A long step name breaks at the space nearest its middle.
fn wrapped(name: &str, format: &TextFormat, width: f32) -> String {
    let name = name.trim();
    if crate::typography::layout(name, format).width <= width {
        return name.into();
    }
    let middle = name.len() / 2;
    name.match_indices(' ')
        .map(|(i, _)| i)
        .min_by_key(|i| i.abs_diff(middle))
        .map_or_else(
            || name.into(),
            |i| format!("{}\n{}", &name[..i], name[i + 1..].trim_start()),
        )
}

/// Whether `p` lies on what the cycle draws.
pub fn hit(doc: &Document, g: &Graphic, p: Point, r: f32) -> bool {
    // Side arrows and names stay within a few bond lengths of the circle.
    let (center, radius) = g.cycle_circle();
    let bow = g
        .cycle
        .iter()
        .flat_map(|c| &c.steps)
        .map(|s| s.bow_pt.abs())
        .fold(0., f32::max);
    let reach = 8. * doc.drawing_style.bond_length_world + DEFAULT.world(bow);
    if p.distance(center) > radius + reach {
        return false;
    }
    let drawn = drawn(doc, g);
    drawn.arrows.iter().any(|a| a.hit(p, r))
        || drawn.labels.iter().any(|(at, text, format)| {
            let layout = crate::typography::layout(text, format);
            contains(grown((*at, at.offset(layout.width, layout.height)), r), p)
        })
}

/// The selection's molecules and texts as cycle parts, in drawing order.
/// A molecule's linked name or property caption moves with it and is not a
/// part of its own.
fn units(doc: &Document, ids: &[u64]) -> Vec<Vec<u64>> {
    let linked: HashSet<u64> = doc
        .molecule_names
        .iter()
        .map(|n| n.annotation)
        .chain(doc.property_captions.iter().map(|c| c.annotation))
        .collect();
    let mut units = crate::reactions::molecules(doc, ids);
    units.extend(
        doc.annotations
            .iter()
            .filter(|a| ids.contains(&a.id) && !linked.contains(&a.id))
            .map(|a| vec![a.id]),
    );
    units
}

/// Every object any cycle holds.
pub fn part_ids(doc: &Document) -> HashSet<u64> {
    doc.graphics
        .iter()
        .filter_map(|g| g.cycle.as_deref())
        .flat_map(|c| c.parts().flat_map(|(_, ids)| ids.iter().copied()))
        .collect()
}

/// The cycle that holds `id`, and its role there.
pub fn role_of(doc: &Document, id: u64) -> Option<(u64, Role)> {
    doc.graphics
        .iter()
        .find_map(|g| Some((g.id, g.cycle.as_deref()?.role(id)?)))
}

/// Make a cycle of the selected molecules and texts: they become its
/// intermediates, clockwise from the top, on a circle through them. They
/// slide straight in or out onto it; nothing else changes. The cycle is
/// grouped with them. Returns the cycle's ID.
pub fn make(doc: &mut Document, ids: &[u64]) -> Result<u64, String> {
    let units: Vec<_> = units(doc, ids)
        .into_iter()
        .filter(|u| u.iter().all(|id| role_of(doc, *id).is_none()))
        .collect();
    if units.len() < 2 {
        return Err("Select two or more structures or texts that are not in a cycle".into());
    }
    if units.len() > MAX_PARTS {
        return Err("A cycle has at most 64 intermediates".into());
    }
    let centers: Vec<Point> = crate::scene::selections_bounds(doc, &units)
        .into_iter()
        .map(|b| b.map(center_of))
        .collect::<Option<_>>()
        .ok_or("Every intermediate needs something drawn")?;
    let count = centers.len() as f32;
    let center = centers
        .iter()
        .fold(Point::default(), |a, p| a.offset(p.x / count, p.y / count));
    let bond = doc.drawing_style.bond_length_world;
    let radius = (centers.iter().map(|p| p.distance(center)).sum::<f32>() / count).max(bond);
    // Clockwise, starting from the one nearest the top.
    let angle = |p: &Point| ((p.y - center.y).atan2(p.x - center.x) + FRAC_PI_2).rem_euclid(TAU);
    let mut order: Vec<(f32, &Vec<u64>)> = centers.iter().map(angle).zip(&units).collect();
    order.sort_by(|a, b| a.0.total_cmp(&b.0));
    let top = |a: f32| a.min(TAU - a);
    if let Some(first) = order
        .iter()
        .enumerate()
        .min_by(|a, b| top(a.1.0).total_cmp(&top(b.1.0)))
        .map(|(i, _)| i)
    {
        order.rotate_left(first);
    }
    let intermediates: Vec<Vec<u64>> = order.into_iter().map(|(_, u)| u.clone()).collect();
    for (unit, p) in units.iter().zip(&centers) {
        let d = p.distance(center);
        if d > 1e-3 {
            let k = radius / d - 1.;
            doc.translate(unit, (p.x - center.x) * k, (p.y - center.y) * k);
        }
    }
    let id = doc.next_id();
    let mut g = Graphic::dragged(
        id,
        GraphicKind::Ellipse,
        center.offset(-radius, -radius),
        center.offset(radius, radius),
        crate::graphics::GraphicStyle {
            width_pt: doc.drawing_style.line_width_pt,
            ..Default::default()
        },
        crate::graphics::BracketSides::Both,
        false,
    );
    g.cycle = Some(Box::new(Cycle {
        steps: vec![Step::default(); intermediates.len()],
        intermediates,
        ..Default::default()
    }));
    doc.graphics.push(g);
    let mut members = vec![id];
    members.extend(units.into_iter().flatten());
    doc.group_selection(&members)?;
    Ok(id)
}

/// An empty cycle of `radius` about `center`: a whole circle arrow that
/// structures can be dropped onto. Returns its ID.
pub fn insert(doc: &mut Document, center: Point, radius: f32) -> u64 {
    let id = doc.next_id();
    let mut g = Graphic::dragged(
        id,
        GraphicKind::Ellipse,
        center.offset(-radius, -radius),
        center.offset(radius, radius),
        crate::graphics::GraphicStyle {
            width_pt: doc.drawing_style.line_width_pt,
            ..Default::default()
        },
        crate::graphics::BracketSides::Both,
        false,
    );
    g.cycle = Some(Box::default());
    doc.graphics.push(g);
    id
}

/// Where dropping a moved structure or text would put it in a cycle.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    pub cycle: u64,
    /// The role it would take; an intermediate is inserted at this index.
    pub role: Role,
    pub part: Vec<u64>,
    /// The part's ink box where it is now.
    pub ink: (Point, Point),
}

/// The drop target for `moved`, one molecule or one text that belongs to no
/// cycle, where it now sits: on a circle it becomes an intermediate in angle
/// order; beside the first half of a step's arc a reagent, beside the
/// second half a product.
pub fn drop_target(doc: &Document, moved: &[u64]) -> Option<Target> {
    if !doc.graphics.iter().any(|g| g.cycle.is_some()) {
        return None;
    }
    let [part] = <[Vec<u64>; 1]>::try_from(units(doc, moved)).ok()?;
    // A part that belongs to a cycle, or to a group of its own, keeps it.
    if part
        .iter()
        .any(|id| role_of(doc, *id).is_some() || doc.groups.iter().any(|g| g.members.contains(id)))
    {
        return None;
    }
    let ink = crate::scene::selection_bounds(doc, &part)?;
    let p = center_of(ink);
    let bond = doc.drawing_style.bond_length_world;
    doc.graphics.iter().rev().find_map(|g| {
        let cycle = g.cycle.as_deref()?;
        if moved.contains(&g.id) {
            return None;
        }
        let (center, radius) = g.cycle_circle();
        let d = p.distance(center);
        let ring = 0.6 * bond;
        let dir = if g.cycle_counterclockwise() { -1. } else { 1. };
        let angle = (p.y - center.y).atan2(p.x - center.x);
        let n = cycle.intermediates.len();
        // The step whose stretch of the circle holds the point, and how far
        // along it the point is.
        let centers: Vec<Option<f32>> = boxes(doc, cycle)
            .into_iter()
            .take(n)
            .map(|b| {
                b.map(|b| {
                    let c = center_of(b);
                    (c.y - center.y).atan2(c.x - center.x)
                })
            })
            .collect();
        let within = (0..n).find_map(|s| {
            let a0 = centers.get(s).copied().flatten()?;
            let a1 = centers.get((s + 1) % n).copied().flatten()?;
            let mut span = (dir * (a1 - a0)).rem_euclid(TAU);
            if span < 1e-4 {
                span = TAU;
            }
            let t = (dir * (angle - a0)).rem_euclid(TAU) / span;
            (t <= 1.).then_some((s, t))
        });
        let role = if (d - radius).abs() < ring {
            Role::Intermediate(within.map_or(0, |(s, _)| s + 1))
        } else if d > radius * 0.35 && d < radius + 4. * bond {
            let (s, t) = within?;
            if t < 0.5 {
                Role::Reagent(s)
            } else {
                Role::Product(s)
            }
        } else {
            return None;
        };
        // A full list takes no more.
        let full = match role {
            Role::Intermediate(_) => n,
            Role::Reagent(s) => cycle.steps.get(s).map_or(0, |s| s.reagents.len()),
            Role::Product(s) => cycle.steps.get(s).map_or(0, |s| s.products.len()),
        } >= MAX_PARTS;
        if full {
            return None;
        }
        Some(Target {
            cycle: g.id,
            role,
            part: part.clone(),
            ink,
        })
    })
}

/// Give the target's part its role, and group it with the cycle.
pub fn assign(doc: &mut Document, target: &Target) {
    let Some(cycle) = doc
        .graphics
        .iter_mut()
        .find(|g| g.id == target.cycle)
        .and_then(|g| g.cycle.as_deref_mut())
    else {
        return;
    };
    let part = target.part.clone();
    match target.role {
        Role::Intermediate(i) => {
            let i = i.min(cycle.intermediates.len());
            cycle.intermediates.insert(i, part);
            // The step it splits keeps its settings and side parts before
            // the new intermediate; the new step starts plain.
            cycle.steps.insert(i, Step::default());
        }
        Role::Reagent(s) => {
            if let Some(step) = cycle.steps.get_mut(s) {
                step.reagents.push(part);
            }
        }
        Role::Product(s) => {
            if let Some(step) = cycle.steps.get_mut(s) {
                step.products.push(part);
            }
        }
    }
    // Every group holding the cycle, nested ones too, takes the part.
    let mut grouped = false;
    for group in doc
        .groups
        .iter_mut()
        .filter(|g| g.members.contains(&target.cycle))
    {
        grouped = true;
        for id in &target.part {
            if !group.members.contains(id) {
                group.members.push(*id);
            }
        }
    }
    if !grouped {
        let mut members = vec![target.cycle];
        members.extend(&target.part);
        let _ = doc.group_selection(&members);
    }
}

/// Take the parts holding any of `ids` out of their cycles and their groups;
/// they stay in the drawing. Returns how many parts left.
pub fn remove(doc: &mut Document, ids: &[u64]) -> usize {
    let held = part_ids(doc);
    let hit = |part: &Vec<u64>| part.iter().any(|id| ids.contains(id));
    for g in &mut doc.graphics {
        let Some(cycle) = &mut g.cycle else {
            continue;
        };
        let gone: Vec<usize> = cycle
            .intermediates
            .iter()
            .enumerate()
            .filter(|(_, part)| hit(part))
            .map(|(i, _)| i)
            .collect();
        for i in gone.into_iter().rev() {
            cycle.remove_intermediate(i);
        }
        for step in &mut cycle.steps {
            step.reagents.retain(|part| !hit(part));
            step.products.retain(|part| !hit(part));
        }
    }
    // Side parts of a cycle left with no steps go too.
    let freed: HashSet<u64> = held.difference(&part_ids(doc)).copied().collect();
    ungroup_freed(doc, &freed);
    freed.len()
}

/// Parts that lost their role leave the groups that hold a cycle; other
/// groups they belong to stay.
fn ungroup_freed(doc: &mut Document, freed: &HashSet<u64>) {
    if freed.is_empty() {
        return;
    }
    let cycles: HashSet<u64> = doc
        .graphics
        .iter()
        .filter(|g| g.cycle.is_some())
        .map(|g| g.id)
        .collect();
    for group in &mut doc.groups {
        if group.members.iter().any(|id| cycles.contains(id)) {
            group.members.retain(|id| !freed.contains(id));
        }
    }
    doc.prune_groups();
}

/// Run the cycle the other way round: the order reverses and what came in
/// now goes out.
pub fn reverse(cycle: &mut Cycle) {
    // The first intermediate stays first and the rest come in reverse;
    // each step then runs between the same two, the other way.
    cycle.intermediates.reverse();
    if !cycle.intermediates.is_empty() {
        cycle.intermediates.rotate_right(1);
    }
    cycle.steps.reverse();
    for step in &mut cycle.steps {
        std::mem::swap(&mut step.reagents, &mut step.products);
        std::mem::swap(&mut step.lead_pt, &mut step.trail_pt);
    }
    cycle.counterclockwise = !cycle.counterclockwise;
}

/// Change a cycle's radius; its parts slide straight in or out with it.
pub fn set_radius(doc: &mut Document, id: u64, radius: f32) {
    let Some(g) = doc
        .graphics
        .iter()
        .find(|g| g.id == id && g.cycle.is_some())
    else {
        return;
    };
    let (center, old) = g.cycle_circle();
    if !(radius.is_finite() && radius > 0. && old > 0.) {
        return;
    }
    let parts: Vec<Vec<u64>> = g
        .cycle
        .iter()
        .flat_map(|c| c.parts().map(|(_, ids)| ids.clone()))
        .collect();
    let k = radius / old - 1.;
    for (part, b) in parts
        .iter()
        .zip(crate::scene::selections_bounds(doc, &parts))
    {
        if let Some(b) = b {
            let p = center_of(b);
            doc.translate(part, (p.x - center.x) * k, (p.y - center.y) * k);
        }
    }
    if let Some(g) = doc.graphics.iter_mut().find(|g| g.id == id) {
        // A mirrored cycle stays mirrored, so it keeps running its way.
        let mirrored = g.frame_determinant() < 0.;
        g.origin = center.offset(-radius, if mirrored { radius } else { -radius });
        g.axis_x = Point::new(2. * radius, 0.);
        g.axis_y = Point::new(0., if mirrored { -2. } else { 2. } * radius);
    }
}

/// The drawing as other programs can keep it: each cycle becomes the
/// ordinary arrows and text it draws, grouped with its parts.
pub fn materialized(doc: &Document) -> std::borrow::Cow<'_, Document> {
    if !doc.graphics.iter().any(|g| g.cycle.is_some()) {
        return std::borrow::Cow::Borrowed(doc);
    }
    let mut out = doc.clone();
    let mut next = doc.next_id();
    for g in doc.graphics.iter().filter(|g| g.cycle.is_some()) {
        let drawn = drawn(doc, g);
        let mut ids = vec![];
        for mut arrow in drawn.arrows {
            let style = arrow.appearance();
            if style.head == Head::None && style.tail == Head::None {
                // ChemDraw keeps a headless curve as a plain path.
                for path in arrow.paths() {
                    ids.push(next);
                    out.graphics.push(crate::arrows::shape(
                        next,
                        path.commands,
                        &path.style,
                        path.filled,
                    ));
                    next += 1;
                }
                continue;
            }
            arrow.id = next;
            ids.push(next);
            next += 1;
            out.arrows.push(arrow);
        }
        for (position, text, format) in drawn.labels {
            ids.push(next);
            out.annotations.push(crate::document::Annotation {
                id: next,
                position,
                text,
                format,
            });
            next += 1;
        }
        for group in out
            .groups
            .iter_mut()
            .filter(|group| group.members.contains(&g.id))
        {
            group.members.extend(&ids);
        }
    }
    out.graphics.retain(|g| g.cycle.is_none());
    let gone: HashSet<u64> = doc
        .graphics
        .iter()
        .filter(|g| g.cycle.is_some())
        .map(|g| g.id)
        .collect();
    for group in &mut out.groups {
        group.members.retain(|id| !gone.contains(id));
    }
    out.prune_groups();
    std::borrow::Cow::Owned(out)
}

/// `ids` with every cycle part any of them belongs to, whole.
pub fn whole_parts(doc: &Document, ids: &[u64]) -> Vec<u64> {
    let mut out = ids.to_vec();
    for cycle in doc.graphics.iter().filter_map(|g| g.cycle.as_deref()) {
        for (_, part) in cycle.parts() {
            if part.iter().any(|id| ids.contains(id)) {
                out.extend(part.iter().filter(|id| !ids.contains(id)));
            }
        }
    }
    out
}

/// Follow atoms merged by a join: a part's atom becomes the atom that
/// remains. `mapping` holds every atom before the join.
pub(crate) fn remap_join(doc: &mut Document, mapping: &HashMap<u64, u64>) {
    for g in &mut doc.graphics {
        if let Some(cycle) = &mut g.cycle {
            for part in cycle.parts_mut() {
                for id in part.iter_mut() {
                    if let Some(new) = mapping.get(id) {
                        *id = *new;
                    }
                }
                part.sort_unstable();
                part.dedup();
            }
        }
    }
}

/// Keep cycle parts through an ID change; ones left behind drop out.
pub fn remap(g: &mut Graphic, mapping: &HashMap<u64, u64>) {
    if let Some(cycle) = &mut g.cycle {
        for part in cycle.parts_mut() {
            part.retain_mut(|id| mapping.get(id).map(|new| *id = *new).is_some());
        }
    }
}

/// After an edit, each part holds whole molecules that still exist or one
/// text; parts that went are dropped, and the steps around a dropped
/// intermediate become one.
pub fn reconcile(doc: &mut Document) {
    if !doc.graphics.iter().any(|g| g.cycle.is_some()) {
        return;
    }
    let atoms: HashSet<u64> = doc.atoms.iter().map(|a| a.id).collect();
    let texts: HashSet<u64> = doc.annotations.iter().map(|a| a.id).collect();
    let mut used = HashSet::new();
    let mut fix = |ids: &[u64]| -> Vec<u64> {
        let present: Vec<u64> = ids
            .iter()
            .copied()
            .filter(|id| atoms.contains(id))
            .collect();
        let mut part = if present.is_empty() {
            ids.iter()
                .copied()
                .filter(|id| texts.contains(id))
                .take(1)
                .collect()
        } else {
            let mut whole = crate::reactions::molecules(doc, &present).concat();
            whole.sort_unstable();
            whole
        };
        // A part another part already holds, as when a bond joins them, goes.
        if part.iter().any(|id| used.contains(id)) {
            part.clear();
        }
        used.extend(part.iter().copied());
        part
    };
    let fixed: Vec<Option<Box<Cycle>>> = doc
        .graphics
        .iter()
        .map(|g| {
            let mut cycle = g.cycle.clone()?;
            for part in cycle.parts_mut() {
                *part = fix(part);
            }
            for i in (0..cycle.intermediates.len()).rev() {
                if cycle.intermediates.get(i).is_some_and(Vec::is_empty) {
                    cycle.remove_intermediate(i);
                }
            }
            for step in &mut cycle.steps {
                step.reagents.retain(|p| !p.is_empty());
                step.products.retain(|p| !p.is_empty());
            }
            Some(cycle)
        })
        .collect();
    let held = part_ids(doc);
    for (g, cycle) in doc.graphics.iter_mut().zip(fixed) {
        if cycle.is_some() && g.cycle != cycle {
            g.cycle = cycle;
        }
    }
    // A side part whose step went with its intermediate is free again.
    let freed: HashSet<u64> = held.difference(&part_ids(doc)).copied().collect();
    ungroup_freed(doc, &freed);
}

#[cfg(test)]
mod tests;
