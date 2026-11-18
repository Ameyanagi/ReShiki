//! Placement for predicted-shift atom numbers: the nearest box
//! around the atom that touches nothing, with a leader line when it is far
//! away or nearer another atom. Canvas, SVG, PNG and PDF all draw it.
use super::segment_hits_rect;
use crate::document::{Document, Point};
use std::collections::HashMap;

/// What a long number must not touch: label boxes (atom labels, captions and
/// earlier indicators), bonds as `(a, b, order, display)`, visible atom
/// positions and earlier leaders.
pub(super) struct Obstacles<'a> {
    pub boxes: &'a [(Point, Point)],
    pub bonds: &'a [(Point, Point, u8, &'a str)],
    pub atoms: &'a [Point],
    pub leaders: &'a [(Point, Point)],
}

/// A label's centre and its leader, when it has one.
pub(super) type Spot = (Point, Option<(Point, Point)>);

/// One long number to place.
pub(super) struct LongLabel {
    pub id: u64,
    pub anchor: Point,
    /// Text width and height.
    pub size: (f32, f32),
    /// The box the atom itself reserves, which a leader starts outside.
    pub own: Option<(Point, Point)>,
    pub bond_length: f32,
    /// How far from a double, triple or styled bond's centre line its ink can
    /// reach, with a margin.
    pub rails: f32,
    /// The convex hull of the atom's molecule, counterclockwise. Labels stay
    /// outside it, so they never fill the space between the molecule's bonds.
    pub hull: std::rc::Rc<[Point]>,
}

/// The convex hull of `points`, counterclockwise (monotone chain).
pub(super) fn hull(mut points: Vec<Point>) -> Vec<Point> {
    points.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    points.dedup();
    if points.len() < 3 {
        return points;
    }
    let cross =
        |o: Point, a: Point, b: Point| (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x);
    let chain = |points: &mut dyn Iterator<Item = Point>| {
        let mut chain: Vec<Point> = Vec::new();
        for p in points {
            while let [.., o, a] = chain[..]
                && cross(o, a, p) <= 0.
            {
                chain.pop();
            }
            chain.push(p);
        }
        chain.pop();
        chain
    };
    let mut lower = chain(&mut points.iter().copied());
    lower.extend(chain(&mut points.iter().rev().copied()));
    lower
}

/// Whether the box `lo`..`hi` reaches more than `depth` into the convex
/// polygon `hull` (separating axes: the box's and the polygon's edges).
fn inside_hull(lo: Point, hi: Point, hull: &[Point], depth: f32) -> bool {
    if hull.len() < 3 {
        return false;
    }
    let corners = [lo, Point::new(hi.x, lo.y), hi, Point::new(lo.x, hi.y)];
    let edges = hull.iter().zip(hull.iter().cycle().skip(1));
    let axes = edges
        .map(|(a, b)| (b.y - a.y, a.x - b.x))
        .chain([(1., 0.), (0., 1.)]);
    for (x, y) in axes {
        let length = x.hypot(y);
        if length < 1e-6 {
            continue;
        }
        let project = |p: &Point| (p.x * x + p.y * y) / length;
        let (box_lo, box_hi) = corners
            .iter()
            .map(project)
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(l, h), v| {
                (l.min(v), h.max(v))
            });
        let (hull_lo, hull_hi) = hull
            .iter()
            .map(project)
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(l, h), v| {
                (l.min(v), h.max(v))
            });
        if box_hi.min(hull_hi) - box_lo.max(hull_lo) <= depth {
            return false;
        }
    }
    true
}

/// Directions tried, as multiples of 7.5° from the outward direction:
/// outward first, then alternately either side of it.
const STEPS: [i8; 48] = [
    0, 1, -1, 2, -2, 3, -3, 4, -4, 5, -5, 6, -6, 7, -7, 8, -8, 9, -9, 10, -10, 11, -11, 12, -12,
    13, -13, 14, -14, 15, -15, 16, -16, 17, -17, 18, -18, 19, -19, 20, -20, 21, -21, 22, -22, 23,
    -23, 24,
];

/// The cost of a box on text, bonds or other labels, or of a leader through
/// text.
const BLOCKED: f32 = 1000.;

/// The cost of a leader crossing a bond within half a bond length of either
/// end, where the crossing reads as a mark on the bond.
pub(super) const MARK: f32 = 100.;

/// Space kept between a label and other text or leaders.
const TEXT_GAP: f32 = 2.;

impl LongLabel {
    /// The text box at `center`, grown by `pad` on every side.
    pub fn bounds(&self, center: Point, pad: f32) -> (Point, Point) {
        let (w, h) = self.size;
        (
            center.offset(-w / 2. - pad, -h / 2. - pad),
            center.offset(w / 2. + pad, h / 2. + pad),
        )
    }

    /// How far beyond its center a change can move this label.
    pub fn reach(&self) -> f32 {
        let (w, h) = self.size;
        8. * self.bond_length + w + h + 10.
    }

    /// The boxes to try, nearest first and outward first at each distance:
    /// centres along directions from the atom, in steps of half the text
    /// height up to `max_gap`.
    fn candidates(
        &self,
        obstacles: &Obstacles<'_>,
        positions: &HashMap<u64, Point>,
        doc: &Document,
        max_gap: f32,
    ) -> impl Iterator<Item = Point> {
        let neighbors: Vec<_> = doc
            .bonds
            .iter()
            .filter_map(|b| match (b.a == self.id, b.b == self.id) {
                (true, _) => positions.get(&b.b).copied(),
                (_, true) => positions.get(&b.a).copied(),
                _ => None,
            })
            .collect();
        // Away from the drawing around the atom, so labels fan out from the
        // molecule; an atom in the middle of it uses its open bond angle.
        let near: Vec<_> = obstacles
            .atoms
            .iter()
            .filter(|p| p.distance(self.anchor) <= 3. * self.bond_length)
            .collect();
        let count = near.len().max(1) as f32;
        let (dx, dy) = (
            self.anchor.x - near.iter().map(|p| p.x).sum::<f32>() / count,
            self.anchor.y - near.iter().map(|p| p.y).sum::<f32>() / count,
        );
        let outward = if dx.hypot(dy) > 0.25 * self.bond_length {
            dy.atan2(dx)
        } else {
            crate::editing::open_angle(self.anchor, &neighbors)
        };
        let (w, h, anchor) = (self.size.0, self.size.1, self.anchor);
        let step = (h / 2.).max(2.);
        let gaps = (0..)
            .map(move |i| 1.5 + i as f32 * step)
            .take_while(move |gap| *gap <= max_gap);
        gaps.flat_map(move |gap| {
            STEPS.into_iter().map(move |k| {
                let angle = outward + f32::from(k) * std::f32::consts::PI / 24.;
                let (x, y) = (angle.cos(), angle.sin());
                // Along (x, y), the box edge facing the atom sits `gap` away.
                let along = gap + x.abs() * w / 2. + y.abs() * h / 2.;
                anchor.offset(x * along, y * along)
            })
        })
    }

    /// How many free boxes beside the atom, needing no leader, it has among
    /// `obstacles`: the fewer, the earlier it should take one.
    pub fn room(
        &self,
        obstacles: &Obstacles<'_>,
        positions: &HashMap<u64, Point>,
        doc: &Document,
    ) -> usize {
        self.candidates(obstacles, positions, doc, 1.5 * self.bond_length)
            .filter(|center| {
                self.leader(*center, obstacles).is_none()
                    && self.hits(*center, None, obstacles) == 0.
            })
            .count()
    }

    /// The first free box from [`Self::candidates`], its leader and its
    /// cost (zero when free). With `beside`, only a free box that needs no
    /// leader, or `None`. Otherwise, when none is free, the box that costs
    /// least.
    pub fn place(
        &self,
        obstacles: &Obstacles<'_>,
        positions: &HashMap<u64, Point>,
        doc: &Document,
        beside: bool,
    ) -> Option<(f32, Spot)> {
        let mut best: Option<(f32, Spot)> = None;
        // Beyond 1.5 bond lengths every box needs a leader.
        let max_gap = if beside { 1.5 } else { 8. } * self.bond_length;
        for center in self.candidates(obstacles, positions, doc, max_gap) {
            let leader = self.leader(center, obstacles);
            if beside && leader.is_some() {
                continue;
            }
            let hits = self.hits(center, leader, obstacles);
            if hits == 0. {
                return Some((0., (center, leader)));
            }
            if best.as_ref().is_none_or(|(least, _)| hits < *least) {
                best = Some((hits, (center, leader)));
            }
        }
        if beside {
            return None;
        }
        best
    }

    /// Whether the box at `center` needs a leader: it is more than 1.5 bond
    /// lengths from this atom, or another atom is as near to it.
    fn needs_leader(&self, center: Point, obstacles: &Obstacles<'_>) -> bool {
        let (lo, hi) = self.bounds(center, 0.);
        let gap = self.anchor.distance(nearest(self.anchor, lo, hi));
        gap > 1.5 * self.bond_length
            || obstacles
                .atoms
                .iter()
                .any(|p| p.distance(self.anchor) > 0.01 && p.distance(nearest(*p, lo, hi)) <= gap)
    }

    /// The leader of the box at `center`, when it needs one: from the edge of
    /// the atom's label (or near the atom) to the box, short of both. `None`
    /// also when the box is too close for a line to show.
    pub fn leader(&self, center: Point, obstacles: &Obstacles<'_>) -> Option<(Point, Point)> {
        if !self.needs_leader(center, obstacles) {
            return None;
        }
        let (lo, hi) = self.bounds(center, 0.);
        let end = nearest(self.anchor, lo, hi);
        let length = end.distance(self.anchor);
        if length < 0.01 {
            return None;
        }
        let (ux, uy) = (
            (end.x - self.anchor.x) / length,
            (end.y - self.anchor.y) / length,
        );
        let start = self
            .own
            .map_or(2., |(lo, hi)| exit(self.anchor, ux, uy, lo, hi))
            + 1.5;
        let stop = length - 1.5;
        // A line shorter than about a third of the text height does not read
        // as a leader.
        (stop - start > self.size.1 / 3.).then(|| {
            (
                self.anchor.offset(ux * start, uy * start),
                self.anchor.offset(ux * stop, uy * stop),
            )
        })
    }

    /// What the box at `center` and its leader run into. Zero is a free
    /// spot. Otherwise the fallback weighs a bond a leader crosses as much as
    /// 2.5 bond lengths of leader; a box on anything, a box that cannot be
    /// attributed and a leader through text or another leader come last,
    /// after a leader crossing a bond within half a bond length of either
    /// end, which reads as a mark on the bond.
    fn hits(
        &self,
        center: Point,
        leader: Option<(Point, Point)>,
        obstacles: &Obstacles<'_>,
    ) -> f32 {
        let (lo, hi) = self.bounds(center, TEXT_GAP);
        let boxes = obstacles
            .boxes
            .iter()
            .filter(|(a, b)| a.x < hi.x && b.x > lo.x && a.y < hi.y && b.y > lo.y)
            .count();
        let bonds = obstacles
            .bonds
            .iter()
            .filter(|&&(a, b, order, display)| {
                // Double, triple and styled bonds draw rails and wedges beside
                // their centre line.
                let pad = if order == 1 && display == "plain" {
                    3.
                } else {
                    self.rails.max(10.)
                };
                let (lo, hi) = self.bounds(center, pad);
                segment_hits_rect(a, b, lo, hi)
            })
            .count();
        let leaders = obstacles
            .leaders
            .iter()
            .filter(|(a, b)| segment_hits_rect(*a, *b, lo, hi))
            .count();
        // A box that needs a leader too short to draw cannot be attributed,
        // and neither can one without a leader behind other text.
        let unattributed = match leader {
            None if self.needs_leader(center, obstacles) => 1,
            None => {
                let (lo, hi) = self.bounds(center, 0.);
                let facing = nearest(self.anchor, lo, hi);
                obstacles
                    .boxes
                    .iter()
                    .filter(|&&(a, b)| {
                        let p = self.anchor;
                        let own = a.x <= p.x && p.x <= b.x && a.y <= p.y && p.y <= b.y;
                        !own && segment_hits_rect(p, facing, a, b)
                    })
                    .count()
            }
            Some(_) => 0,
        };
        let (box_lo, box_hi) = self.bounds(center, 0.);
        let inside = usize::from(inside_hull(box_lo, box_hi, &self.hull, 2.));
        let blocked = boxes + bonds + leaders + unattributed + inside;
        let Some((start, end)) = leader else {
            return BLOCKED * blocked as f32;
        };
        let crossed_text = obstacles
            .boxes
            .iter()
            .filter(|(a, b)| segment_hits_rect(start, end, *a, *b))
            .count()
            + obstacles
                .leaders
                .iter()
                .filter(|(a, b)| segments_cross(start, end, *a, *b))
                .count();
        // Where the leader crosses each bond, as a distance from its nearer end.
        let crossings: Vec<f32> = obstacles
            .bonds
            .iter()
            .filter(|(a, b, ..)| a.distance(self.anchor) > 0.01 && b.distance(self.anchor) > 0.01)
            .filter_map(|(a, b, ..)| crossing(start, end, *a, *b))
            .map(|t| t.min(1. - t) * start.distance(end))
            .collect();
        // A crossing near either end reads as a mark on the bond, not a
        // leader passing over it.
        let marks = crossings
            .iter()
            .filter(|&&d| d < 0.5 * self.bond_length)
            .count();
        if crossed_text + crossings.len() + blocked == 0 {
            return 0.;
        }
        // A leader through text or another leader is as bad as a box on it;
        // a mark on a bond is better than that but worse than any crossing.
        BLOCKED * (blocked + crossed_text) as f32
            + MARK * marks as f32
            + 10. * crossings.len() as f32
            + 4. * start.distance(end) / self.bond_length
    }
}

/// The point of the box `lo`..`hi` nearest to `p`.
fn nearest(p: Point, lo: Point, hi: Point) -> Point {
    Point::new(p.x.clamp(lo.x, hi.x), p.y.clamp(lo.y, hi.y))
}

/// How far from `p`, inside the box `lo`..`hi`, the ray along `(ux, uy)`
/// leaves it; zero when `p` is outside.
fn exit(p: Point, ux: f32, uy: f32, lo: Point, hi: Point) -> f32 {
    if p.x < lo.x || p.x > hi.x || p.y < lo.y || p.y > hi.y {
        return 0.;
    }
    let along = |start: f32, direction: f32, low: f32, high: f32| {
        if direction > 1e-6 {
            (high - start) / direction
        } else if direction < -1e-6 {
            (low - start) / direction
        } else {
            f32::INFINITY
        }
    };
    along(p.x, ux, lo.x, hi.x).min(along(p.y, uy, lo.y, hi.y))
}

/// Whether the segments `a`–`b` and `c`–`d` cross.
fn segments_cross(a: Point, b: Point, c: Point, d: Point) -> bool {
    crossing(a, b, c, d).is_some()
}

/// Where `a`–`b` crosses `c`–`d`, as a fraction of the way from `a` to `b`.
fn crossing(a: Point, b: Point, c: Point, d: Point) -> Option<f32> {
    let side = |p: Point, q: Point, r: Point| (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
    let (d1, d2) = (side(c, d, a), side(c, d, b));
    let (d3, d4) = (side(a, b, c), side(a, b, d));
    (d1 * d2 < 0. && d3 * d4 < 0.).then(|| d1 / (d1 - d2))
}

#[cfg(test)]
mod tests;
