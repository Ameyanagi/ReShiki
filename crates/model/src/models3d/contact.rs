//! Where overlapping 3D-model balls meet. Each ball shows only the part of its
//! disc whose surface facing the eye lies outside every ball it overlaps; the
//! boundary is the facing arc of the circle where the two spheres meet. Seams
//! are the runs of those circles facing the eye on both balls and inside no
//! other ball.
//!
//! Geometry is worked out in the rotated model (ångström, x right, y up, z
//! toward the viewer) and projected point by point. Under perspective a ball's
//! outline is the small circle where the eye's sight lines graze it, so that
//! circle is its rim and every ball that meets another is drawn from it.
use crate::document::Point;
use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::overlay::FloatOverlay;
use std::collections::HashMap;
use std::f32::consts::{PI, TAU};

type V = [f32; 3];

fn add(a: V, b: V) -> V {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn mul(a: V, s: f32) -> V {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: V, b: V) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V, b: V) -> V {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn len(a: V) -> f32 {
    dot(a, a).sqrt()
}
fn unit(a: V) -> V {
    mul(a, 1. / len(a))
}

/// Samples per full turn of a rim or a meeting circle.
pub(super) const STEPS: f32 = 64.;
/// Fewer chords while turning; the arc ends still meet exactly.
pub(super) const TURNING_STEPS: f32 = 24.;
/// Samples per full turn of a polygon's rim outside the disc: coarse, as the
/// polygon there is cut away.
const PAST_STEPS: f32 = 16.;
/// How far past the rim a polygon reaches there, as a fraction of the
/// radius, so the disc's own polygon never leaves slivers along it.
const PAST_RIM: f32 = 0.25;

/// A ball in the rotated model.
#[derive(Debug, Clone, Copy)]
pub(super) struct Ball {
    pub p: V,
    pub r: f32,
    /// Fading in during the 2D → 3D morph: it cuts only other fading balls.
    pub fading: bool,
}

/// A circle in the model: its centre, two axes in its plane and its radius.
#[derive(Debug, Clone, Copy)]
struct Ring {
    c: V,
    e1: V,
    e2: V,
    r: f32,
}

impl Ring {
    fn at(&self, t: f32) -> V {
        add(
            self.c,
            add(
                mul(self.e1, self.r * t.cos()),
                mul(self.e2, self.r * t.sin()),
            ),
        )
    }
}

/// How a ball faces the eye: a point X on it faces the eye where
/// (X − p) · v ≥ s, and its rim is the circle where that turns. Seen from
/// infinitely far, v is the z axis, s is 0 and the rim is the ball's
/// equator.
#[derive(Debug, Clone, Copy)]
struct Facing {
    v: V,
    s: f32,
    rim: Ring,
}

impl Facing {
    fn new(ball: Ball, eye: Option<f32>) -> Self {
        let (v, s) = match eye {
            Some(camera) => {
                let to = sub([0., 0., camera], ball.p);
                let distance = len(to);
                (mul(to, 1. / distance), ball.r * ball.r / distance)
            }
            None => ([0., 0., 1.], 0.),
        };
        let e1 = unit(cross([0., 1., 0.], v));
        Facing {
            v,
            s,
            rim: Ring {
                c: add(ball.p, mul(v, s)),
                e1,
                e2: cross(v, e1),
                r: (ball.r * ball.r - s * s).max(0.).sqrt(),
            },
        }
    }
    /// The angle of rim point `x`.
    fn angle(&self, x: V) -> f32 {
        let d = sub(x, self.rim.c);
        dot(d, self.rim.e2).atan2(dot(d, self.rim.e1))
    }
}

/// The circle where two spheres meet, as seen from the first, with the axis
/// `u` toward the second sphere and the circle's plane `h` along it.
struct Circle {
    u: V,
    h: f32,
    ring: Ring,
}

enum Meeting {
    /// Apart, or `b` inside `a`: nothing of `a` is covered.
    Clear,
    /// `a` inside `b`.
    Inside,
    Circle(Circle),
}

fn meeting(a: Ball, b: Ball) -> Meeting {
    let ab = sub(b.p, a.p);
    let d = len(ab);
    if d >= a.r + b.r || d + b.r <= a.r {
        return Meeting::Clear;
    }
    if d + a.r <= b.r {
        return Meeting::Inside;
    }
    let u = mul(ab, 1. / d);
    let h = (d * d + a.r * a.r - b.r * b.r) / (2. * d);
    let helper = if u[0].abs() < 0.9 {
        [1., 0., 0.]
    } else {
        [0., 1., 0.]
    };
    let e1 = unit(cross(u, helper));
    Meeting::Circle(Circle {
        u,
        h,
        ring: Ring {
            c: add(a.p, mul(u, h)),
            e1,
            e2: cross(u, e1),
            r: (a.r * a.r - h * h).max(0.).sqrt(),
        },
    })
}

/// The angles on a circle where `p cos t + q sin t ≥ k`: `None` for none,
/// otherwise the centre and half width of the arc (π for all of it).
fn arc_where(p: f32, q: f32, k: f32) -> Option<(f32, f32)> {
    let m = p.hypot(q);
    if m <= f32::EPSILON {
        return (k <= 0.).then_some((0., PI));
    }
    let ratio = k / m;
    if ratio > 1. {
        return None;
    }
    Some((q.atan2(p), ratio.max(-1.).acos()))
}

/// Arcs of a circle taken out as (centre, half width), for each condition
/// `p cos t + q sin t ≥ k` the circle must meet.
#[derive(Default)]
struct Blocked(Vec<(f32, f32)>);

impl Blocked {
    fn need(&mut self, p: f32, q: f32, k: f32) {
        self.0.push(match arc_where(p, q, k) {
            None => (0., PI),
            Some((centre, half)) => (centre + PI, PI - half),
        });
    }
    /// The arcs left, each as its start and end angle with end > start.
    fn free(&self) -> Vec<(f32, f32)> {
        free(&self.0)
    }
}

/// The arcs left of a full turn once every arc in `blocked` (centre, half
/// width) is taken out, each as its start and end angle with end > start.
fn free(blocked: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let mut spans: Vec<(f32, f32)> = blocked
        .iter()
        .filter(|(_, half)| *half > 1e-5)
        .map(|&(centre, half)| {
            let lo = (centre - half).rem_euclid(TAU);
            (lo, lo + 2. * half)
        })
        .collect();
    if spans.iter().any(|(lo, hi)| hi - lo >= TAU) {
        return vec![];
    }
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let Some(&(start, _)) = spans.first() else {
        return vec![(0., TAU)];
    };
    let end = start + TAU;
    // A span running past the end comes round again at the start.
    let wrap = spans.iter().map(|s| s.1).fold(end, f32::max) - end;
    spans.push((start, start + wrap));
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out = vec![];
    let mut reach = start;
    for (lo, hi) in spans {
        if lo > reach {
            out.push((reach, lo));
        }
        reach = reach.max(hi);
    }
    if reach < end {
        out.push((reach, end));
    }
    out
}

/// Points from `t0` to `t1` at most a `steps`-to-the-turn step apart, both
/// ends included.
fn sample(t0: f32, t1: f32, steps: f32) -> impl Iterator<Item = f32> {
    let n = ((t1 - t0).abs() / (TAU / steps)).ceil().max(1.) as usize;
    (0..=n).map(move |k| t0 + (t1 - t0) * k as f32 / n as f32)
}

/// What `b` covers of `a`'s disc.
enum Cover {
    None,
    Hidden,
    /// The disc outside a wholly facing meeting circle: the visible cap
    /// stays inside the circle when the covered cap holds the ball's rim.
    Outside(Ring),
    /// The meeting circle's arc facing the eye, between two angles, then the
    /// rim's angles back round to its start.
    Part(Ring, (f32, f32), Option<(f32, f32)>),
}

fn cover(a: Ball, facing: &Facing, b: Ball) -> Cover {
    let m = match meeting(a, b) {
        Meeting::Clear => return Cover::None,
        Meeting::Inside => return Cover::Hidden,
        Meeting::Circle(m) => m,
    };
    let in_cap = |q: V| dot(sub(q, a.p), m.u) > m.h;
    let ring = m.ring;
    // How far the circle faces the eye is c0 + amp cos(t − phi).
    let c0 = dot(sub(ring.c, a.p), facing.v) - facing.s;
    let (za, zb) = (
        ring.r * dot(ring.e1, facing.v),
        ring.r * dot(ring.e2, facing.v),
    );
    let amp = za.hypot(zb);
    if amp <= c0.abs() {
        if c0 >= 0. {
            // A cap pointing toward the eye covers the projected ring's
            // inside; one pointing away covers its outside, including the
            // rim. The front point alone cannot distinguish them: a small,
            // off-axis cap need not contain it either.
            return if dot(m.u, facing.v) >= 0. {
                Cover::Part(ring, (0., TAU), None)
            } else {
                Cover::Outside(ring)
            };
        }
        return if in_cap(add(a.p, mul(facing.v, a.r))) {
            Cover::Hidden
        } else {
            Cover::None
        };
    }
    let phi = zb.atan2(za);
    let w = (-c0 / amp).clamp(-1., 1.).acos();
    let (t1, t2) = (phi - w, phi + w);
    let (s1, s2) = (facing.angle(ring.at(t1)), facing.angle(ring.at(t2)));
    let mut ds = (s1 - s2).rem_euclid(TAU);
    if !in_cap(facing.rim.at(s2 + ds / 2.)) {
        ds -= TAU;
    }
    Cover::Part(ring, (t1, t2), Some((s2, s2 + ds)))
}

/// What shows of one ball that meets others.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Seen {
    Hidden,
    Shown {
        /// The visible part of the disc: outer contours and holes.
        region: Vec<Vec<Point>>,
        /// The visible runs of the rim.
        rim: Vec<Vec<Point>>,
    },
}

/// Open polylines in the drawing.
pub(super) type Runs = Vec<Vec<Point>>;

/// What shows of each ball, `None` where it meets no other, and the visible
/// seams between pairs of balls.
pub(super) struct Contacts {
    pub seen: Vec<Option<Seen>>,
    pub seams: Vec<((usize, usize), Runs)>,
}

/// Each ball's overlapping neighbours, found on a grid of cells as wide as
/// the largest ball.
fn neighbours(balls: &[Ball]) -> Vec<Vec<usize>> {
    let cell = 2. * balls.iter().map(|b| b.r).fold(f32::EPSILON, f32::max);
    let key = |p: V| p.map(|v| (v / cell).floor() as i64);
    let mut grid: HashMap<[i64; 3], Vec<usize>> = HashMap::new();
    for (i, b) in balls.iter().enumerate() {
        grid.entry(key(b.p)).or_default().push(i);
    }
    balls
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let [x, y, z] = key(a.p);
            let mut near = vec![];
            for dx in -1..=1 {
                for dy in -1..=1 {
                    for dz in -1..=1 {
                        for &j in grid.get(&[x + dx, y + dy, z + dz]).into_iter().flatten() {
                            // A fading ball covers only fading balls.
                            if balls.get(j).is_some_and(|b| {
                                j != i && len(sub(b.p, a.p)) < a.r + b.r && (a.fading || !b.fading)
                            }) {
                                near.push(j);
                            }
                        }
                    }
                }
            }
            near.sort_unstable();
            near
        })
        .collect()
}

/// What shows of every ball and the seams between them, seen from `eye`
/// (the camera's distance along z, `None` from infinitely far), with
/// `project` taking a rotated point to the drawing.
pub(super) fn contacts(
    balls: &[Ball],
    eye: Option<f32>,
    steps: f32,
    project: impl Fn(V) -> Point + Sync,
) -> Contacts {
    let near = neighbours(balls);
    let facing: Vec<Facing> = balls.iter().map(|&b| Facing::new(b, eye)).collect();
    let mut visible = vec![None; balls.len()];
    let draw = |start: usize, chunk: &mut [Option<Seen>]| {
        // Keep the overlay's working buffers between balls.
        let mut overlay = FloatOverlay::new_empty(Default::default(), Default::default(), 0);
        for (((&a, f), near), out) in balls.iter().zip(&facing).zip(&near).skip(start).zip(chunk) {
            *out =
                (!near.is_empty()).then(|| seen(a, f, near, balls, steps, &project, &mut overlay));
        }
    };
    // Independent overlays write back in ball order, with enough work per
    // worker to pay for starting it. Browser builds stay on one thread.
    let workers = if cfg!(target_arch = "wasm32") {
        1
    } else {
        std::thread::available_parallelism()
            .map_or(1, usize::from)
            .min(near.iter().filter(|list| !list.is_empty()).count() / 8)
    };
    if workers > 1 {
        let size = balls.len().div_ceil(workers);
        std::thread::scope(|scope| {
            for (i, chunk) in visible.chunks_mut(size).enumerate() {
                let draw = &draw;
                scope.spawn(move || draw(i * size, chunk));
            }
        });
    } else {
        draw(0, &mut visible);
    }
    let mut seams = vec![];
    for (i, list) in near.iter().enumerate() {
        for &j in list.iter().filter(|&&j| j > i) {
            // Only balls that cover each other share a seam.
            if !near.get(j).is_some_and(|other| other.contains(&i)) {
                continue;
            }
            let runs = seam(balls, &facing, (i, j), list, steps, &project);
            if !runs.is_empty() {
                seams.push(((i, j), runs));
            }
        }
    }
    Contacts {
        seen: visible,
        seams,
    }
}

/// A closed polygon's signed area, positive counterclockwise with y down.
fn area(points: &[[f64; 2]]) -> f64 {
    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
        .sum::<f64>()
        / 2.
}

fn flat(p: Point) -> [f64; 2] {
    [p.x as f64, p.y as f64]
}

fn point([x, y]: [f64; 2]) -> Point {
    Point::new(x as f32, y as f32)
}

/// One ball's rim and what its neighbours cover of its disc, as drawing
/// polygons, counterclockwise.
struct Disc<'a, P> {
    ball: Ball,
    facing: &'a Facing,
    project: &'a P,
    steps: f32,
}

impl<P: Fn(V) -> Point> Disc<'_, P> {
    /// Rim angle `s` drawn past the rim, radially from the projected rim
    /// centre.
    fn past(&self, s: f32) -> [f64; 2] {
        let centre = (self.project)(self.facing.rim.c);
        let p = (self.project)(self.facing.rim.at(s));
        flat(Point::new(
            centre.x + (p.x - centre.x) * (1. + PAST_RIM),
            centre.y + (p.y - centre.y) * (1. + PAST_RIM),
        ))
    }

    fn counterclockwise(mut polygon: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
        if area(&polygon) < 0. {
            polygon.reverse();
        }
        polygon
    }

    /// The whole disc.
    fn whole(&self) -> Vec<[f64; 2]> {
        Self::counterclockwise(
            sample(0., TAU, self.steps)
                .skip(1)
                .map(|s| flat((self.project)(self.facing.rim.at(s))))
                .collect(),
        )
    }

    /// What `other` covers, reaching past the rim where it meets it. `Err`
    /// when it covers all of the disc.
    fn cover(&self, other: Ball) -> Result<Vec<Vec<[f64; 2]>>, ()> {
        match cover(self.ball, self.facing, other) {
            Cover::None => Ok(vec![]),
            Cover::Hidden => Err(()),
            Cover::Outside(ring) => {
                let outer = Self::counterclockwise(
                    sample(0., TAU, PAST_STEPS).map(|s| self.past(s)).collect(),
                );
                let mut hole = Self::counterclockwise(
                    sample(0., TAU, self.steps)
                        .map(|t| flat((self.project)(ring.at(t))))
                        .collect(),
                );
                // Opposite winding leaves the cap out of this cut. Keeping
                // both contours lets NonZero union it with other neighbours'
                // cuts without filling the cap or cancelling their coverage.
                hole.reverse();
                Ok(vec![outer, hole])
            }
            Cover::Part(ring, (t0, t1), closing) => {
                let mut polygon: Vec<[f64; 2]> = sample(t0, t1, self.steps)
                    .map(|t| flat((self.project)(ring.at(t))))
                    .collect();
                if let Some((s0, s1)) = closing {
                    polygon.extend(sample(s0, s1, PAST_STEPS).map(|s| self.past(s)));
                }
                Ok(vec![Self::counterclockwise(polygon)])
            }
        }
    }
}

fn seen(
    a: Ball,
    f: &Facing,
    near: &[usize],
    balls: &[Ball],
    steps: f32,
    project: &impl Fn(V) -> Point,
    overlay: &mut FloatOverlay<[f64; 2]>,
) -> Seen {
    let near: Vec<Ball> = near.iter().filter_map(|&j| balls.get(j).copied()).collect();
    let disc = Disc {
        ball: a,
        facing: f,
        project,
        steps,
    };
    let mut cuts = vec![];
    for &b in &near {
        match disc.cover(b) {
            Ok(polygons) => cuts.extend(polygons),
            Err(()) => return Seen::Hidden,
        }
    }
    let whole = disc.whole();
    let region: Vec<Vec<Point>> = if cuts.is_empty() {
        vec![whole.into_iter().map(point).collect()]
    } else {
        overlay.reinit_with_subj_and_clip(&whole, &cuts);
        overlay
            .overlay(OverlayRule::Difference, FillRule::NonZero)
            .into_iter()
            .flatten()
            .filter(|contour| area(contour).abs() > 1e-9)
            .map(|contour| contour.into_iter().map(point).collect())
            .collect()
    };
    if region.is_empty() {
        return Seen::Hidden;
    }
    // The rim shows outside every neighbour b: |rim(s) − b|² ≥ r_b².
    let mut blocked = Blocked::default();
    for b in &near {
        let (rim, w) = (f.rim, sub(f.rim.c, b.p));
        blocked.need(
            2. * rim.r * dot(w, rim.e1),
            2. * rim.r * dot(w, rim.e2),
            b.r * b.r - dot(w, w) - rim.r * rim.r,
        );
    }
    let rim = blocked
        .free()
        .into_iter()
        .map(|(s0, s1)| {
            sample(s0, s1, steps)
                .map(|s| project(f.rim.at(s)))
                .collect()
        })
        .collect();
    Seen::Shown { region, rim }
}

/// The visible runs of the seam between balls `i` and `j`: facing the eye on
/// both and inside none of `i`'s other neighbours.
fn seam(
    balls: &[Ball],
    facing: &[Facing],
    (i, j): (usize, usize),
    near: &[usize],
    steps: f32,
    project: &impl Fn(V) -> Point,
) -> Runs {
    let (Some(&a), Some(&b), Some(fa), Some(fb)) =
        (balls.get(i), balls.get(j), facing.get(i), facing.get(j))
    else {
        return vec![];
    };
    let Meeting::Circle(m) = meeting(a, b) else {
        return vec![];
    };
    let mut blocked = Blocked::default();
    for (ball, f) in [(a, fa), (b, fb)] {
        blocked.need(
            m.ring.r * dot(m.ring.e1, f.v),
            m.ring.r * dot(m.ring.e2, f.v),
            f.s - dot(sub(m.ring.c, ball.p), f.v),
        );
    }
    // Outside every other neighbour: |c − n + ρ(e1 cos t + e2 sin t)|² ≥ r².
    let others = near
        .iter()
        .filter(|&&n| n != j)
        .filter_map(|&n| balls.get(n));
    for &other in others {
        let ring = m.ring;
        let w = sub(ring.c, other.p);
        blocked.need(
            2. * ring.r * dot(w, ring.e1),
            2. * ring.r * dot(w, ring.e2),
            other.r * other.r - dot(w, w) - ring.r * ring.r,
        );
    }
    blocked
        .free()
        .into_iter()
        .filter(|(t0, t1)| t1 - t0 > 1e-3)
        .map(|(t0, t1)| {
            sample(t0, t1, steps)
                .map(|t| project(m.ring.at(t)))
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests;
