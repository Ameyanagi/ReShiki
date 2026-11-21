//! Least-motion phase on a noncircular, world-arclength perimeter.
//!
//! All intervals of one full period participate in the search. A unit-speed
//! midpoint bound and a configuration-chord bound prune intervals globally.
//! The normal stop certifies an RMS displacement objective gap of 0.005 world
//! units, subject to Outline's numerical accuracy. Work is capped: on budget
//! exhaustion the deterministic best sampled phase is returned, without a
//! global-optimality claim. This module does not choose the legacy Circle phase.
//! Equal-cost compared candidates choose the lowest canonical phase; the RMS
//! gap does not certify the ranking of equal global minima left unresolved.
use super::Outline;
use crate::document::Point;
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BinaryHeap},
    f64::consts::TAU,
    rc::Rc,
};

const COVER: usize = 32;
const MAX_EVALUATIONS: usize = 4096;
const MAX_POINT_EVALUATIONS: usize = 32_768;
const RMS_TOLERANCE: f64 = 0.005;

pub(super) fn phase(outline: &Outline, centers: &[Option<Point>], dir: f64) -> Option<f64> {
    let answer = solve(outline, centers, dir)?;
    debug_assert!(answer.evaluations <= MAX_EVALUATIONS);
    debug_assert!(!answer.certified || answer.rms_gap <= RMS_TOLERANCE);
    Some(answer.phase)
}

struct Answer {
    phase: f64,
    certified: bool,
    rms_gap: f64,
    evaluations: usize,
}

struct Sample {
    phase: f64,
    points: Vec<Point>,
    // Mean squared world displacement; its minimizer is also the sum's.
    cost: f64,
}

struct Problem<'a> {
    outline: &'a Outline,
    length: f64,
    targets: Vec<(f64, Point)>,
    samples: BTreeMap<u64, Rc<Sample>>,
    budget: usize,
}

impl Problem<'_> {
    fn sample(&mut self, phase: f64) -> Option<Rc<Sample>> {
        let phase = phase.rem_euclid(self.length);
        let phase = if phase == 0. || phase >= self.length {
            0.
        } else {
            phase
        };
        if let Some(sample) = self.samples.get(&phase.to_bits()) {
            return Some(Rc::clone(sample));
        }
        if self.samples.len() >= self.budget {
            return None;
        }
        let mut cost = 0.;
        let mut points = Vec::with_capacity(self.targets.len());
        for &(offset, center) in &self.targets {
            let point = self.outline.point(phase + offset)?;
            let (x, y) = delta(point, center);
            cost += x * x + y * y;
            points.push(point);
        }
        let sample = Rc::new(Sample {
            phase,
            points,
            cost: cost / self.targets.len() as f64,
        });
        self.samples.insert(phase.to_bits(), Rc::clone(&sample));
        Some(sample)
    }

    fn consider(&self, sample: &Rc<Sample>, best: &mut Rc<Sample>) {
        // Compare objective differences without subtracting large squared
        // distances. Far targets must not create spurious equal-cost phases.
        let (mut difference, mut magnitude) = (0., 0.);
        for ((&point, &old), &(_, center)) in
            sample.points.iter().zip(&best.points).zip(&self.targets)
        {
            let (dx, dy) = delta(point, old);
            let (px, py) = delta(point, center);
            let (ox, oy) = delta(old, center);
            let (x, y) = (dx * (px + ox), dy * (py + oy));
            difference += x + y;
            magnitude += x.abs() + y.abs();
        }
        let error = 32. * f64::EPSILON * magnitude;
        if difference < -error || difference.abs() <= error && sample.phase < best.phase {
            *best = Rc::clone(sample);
        }
    }

    fn interval(
        &self,
        lo: f64,
        width: f64,
        a: Rc<Sample>,
        b: Rc<Sample>,
        mid: Rc<Sample>,
    ) -> Interval {
        let n = self.targets.len() as f64;
        let h = width * 0.5;
        let mut midpoint = 0.;
        let mut endpoint_error: f64 = 0.;
        let (mut chord_squared, mut chord_dot) = (0., 0.);
        for (((&p, &a), &b), &(_, center)) in mid
            .points
            .iter()
            .zip(&a.points)
            .zip(&b.points)
            .zip(&self.targets)
        {
            let (x, y) = delta(p, center);
            midpoint += (x.hypot(y) - h - point_error(p, center, h, self.length))
                .max(0.)
                .powi(2);
            endpoint_error = endpoint_error
                .max(point_error(a, center, width, self.length))
                .max(point_error(b, center, width, self.length));
            let (dx, dy) = delta(b, a);
            let (ax, ay) = delta(a, center);
            chord_squared += dx * dx + dy * dy;
            chord_dot += ax * dx + ay * dy;
        }
        let fraction = if chord_squared > 0. {
            (-chord_dot / chord_squared).clamp(0., 1.)
        } else {
            0.
        };
        let mut chord_cost = 0.;
        for ((&a, &b), &(_, center)) in a.points.iter().zip(&b.points).zip(&self.targets) {
            let (ax, ay) = delta(a, center);
            let (dx, dy) = delta(b, a);
            chord_cost += (ax + fraction * dx).powi(2) + (ay + fraction * dy).powi(2);
        }
        let chord = (chord_squared / n).sqrt();
        let true_chord = (chord - 2. * endpoint_error).max(0.);
        let curve_error = 0.5 * (width * width - true_chord * true_chord).max(0.).sqrt();
        let chord_lower = ((chord_cost / n).sqrt() - curve_error - endpoint_error)
            .max(0.)
            .powi(2);
        let lower = (midpoint / n).max(chord_lower) * (1. - 64. * f64::EPSILON);
        Interval {
            lo,
            width,
            lower,
            chord_fraction: fraction,
            a,
            b,
            mid,
        }
    }

    fn consider_chord(&mut self, interval: &Interval, best: &mut Rc<Sample>) -> Option<()> {
        // On straight edges the configuration is its chord: this is the exact
        // interval minimizer. On curves it is a useful upper candidate only.
        if self.samples.len() < self.budget && (0. ..1.).contains(&interval.chord_fraction) {
            let sample = self.sample(interval.lo + interval.width * interval.chord_fraction)?;
            self.consider(&sample, best);
        }
        Some(())
    }
}

// Outline emits f32 points and asks kurbo for relative ~1e-10 arclength
// accuracy. Inflate both bounds for output rounding, arclength error and
// distance arithmetic. No fixed world floor penalizes very small outlines.
fn point_error(point: Point, target: Point, width: f64, length: f64) -> f64 {
    let p = f64::from(point.x).abs() + f64::from(point.y).abs();
    let c = f64::from(target.x).abs() + f64::from(target.y).abs();
    2. * f64::from(f32::EPSILON) * (p + width)
        + length * 1e-8
        + 16. * f64::EPSILON * (p + c + width)
}

fn delta(a: Point, b: Point) -> (f64, f64) {
    (
        f64::from(a.x) - f64::from(b.x),
        f64::from(a.y) - f64::from(b.y),
    )
}

struct Interval {
    lo: f64,
    width: f64,
    lower: f64,
    chord_fraction: f64,
    a: Rc<Sample>,
    b: Rc<Sample>,
    mid: Rc<Sample>,
}
impl PartialEq for Interval {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for Interval {}
impl PartialOrd for Interval {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Interval {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .lower
            .total_cmp(&self.lower)
            .then_with(|| other.lo.total_cmp(&self.lo))
            .then_with(|| other.width.total_cmp(&self.width))
    }
}

fn solve(outline: &Outline, centers: &[Option<Point>], dir: f64) -> Option<Answer> {
    let length = outline.length();
    if centers.is_empty()
        || centers.len() > super::MAX_PARTS
        || !matches!(dir, -1. | 1.)
        || !length.is_finite()
        || length <= 0.
    {
        return None;
    }
    let spacing = length / centers.len() as f64;
    let targets: Vec<_> = centers
        .iter()
        .enumerate()
        .filter_map(|(k, point)| point.map(|point| (dir * k as f64 * spacing, point)))
        .collect();
    if targets.is_empty() || targets.iter().any(|(_, point)| !point.is_finite()) {
        return None;
    }
    // Project's lowest-s tie rule is the phase tie rule only without a slot
    // offset: wrapping a missing slot's offset can reorder equal projections.
    if centers.len() == 1 {
        let &(offset, point) = targets.first()?;
        let projection = outline.project(point)?;
        let sampled = outline.point(projection.s)?;
        // A continuous projection need not minimize distances after f32 point
        // rounding. Bound both the returned and potentially better point's
        // rounding error; large-coordinate outlines require the full search.
        let gap = 2. * point_error(sampled, point, 0., length);
        if gap <= RMS_TOLERANCE {
            return Some(Answer {
                phase: (projection.s - offset).rem_euclid(length),
                certified: true,
                rms_gap: gap,
                evaluations: 0,
            });
        }
    }
    let budget = MAX_EVALUATIONS.min(MAX_POINT_EVALUATIONS / targets.len());
    let mut problem = Problem {
        outline,
        length,
        targets,
        samples: BTreeMap::new(),
        budget,
    };
    let mut best = problem.sample(0.)?;
    let (mut sin, mut cos) = (0., 0.);
    let seeds: Vec<_> = problem
        .targets
        .iter()
        .map(|&(offset, point)| Some((outline.project(point)?.s - offset).rem_euclid(length)))
        .collect::<Option<_>>()?;
    for seed in seeds {
        let angle = seed / length * TAU;
        sin += angle.sin();
        cos += angle.cos();
        let sample = problem.sample(seed)?;
        problem.consider(&sample, &mut best);
    }
    if sin.hypot(cos) > problem.targets.len() as f64 * 1e-12 {
        let sample = problem.sample(sin.atan2(cos) / TAU * length)?;
        problem.consider(&sample, &mut best);
    }
    let mut queue = BinaryHeap::new();
    for k in 0..COVER {
        let lo = length * k as f64 / COVER as f64;
        let width = length / COVER as f64;
        let a = problem.sample(lo)?;
        let b = problem.sample(lo + width)?;
        let mid = problem.sample(lo + width * 0.5)?;
        for sample in [&a, &b, &mid] {
            problem.consider(sample, &mut best);
        }
        let interval = problem.interval(lo, width, a, b, mid);
        problem.consider_chord(&interval, &mut best)?;
        queue.push(interval);
    }
    let (mut certified, mut gap) = (false, f64::INFINITY);
    while let Some(interval) = queue.pop() {
        let upper = best.cost * (1. + 64. * f64::EPSILON);
        gap = (upper.sqrt() - interval.lower.sqrt()).max(0.);
        if gap <= RMS_TOLERANCE {
            certified = true;
            break;
        }
        if problem.samples.len() + 2 > problem.budget {
            break;
        }
        let width = interval.width * 0.5;
        let middle = interval.lo + width;
        if middle == interval.lo || middle == interval.lo + interval.width {
            break;
        }
        let left_mid = problem.sample(interval.lo + width * 0.5)?;
        let right_mid = problem.sample(middle + width * 0.5)?;
        for sample in [&left_mid, &right_mid] {
            problem.consider(sample, &mut best);
        }
        let left = problem.interval(
            interval.lo,
            width,
            interval.a,
            Rc::clone(&interval.mid),
            left_mid,
        );
        let right = problem.interval(middle, width, interval.mid, interval.b, right_mid);
        problem.consider_chord(&left, &mut best)?;
        problem.consider_chord(&right, &mut best)?;
        queue.push(left);
        queue.push(right);
    }
    Some(Answer {
        phase: best.phase,
        certified,
        rms_gap: gap,
        evaluations: problem.samples.len(),
    })
}

#[cfg(test)]
#[path = "tidy_phase_tests.rs"]
mod tests;
