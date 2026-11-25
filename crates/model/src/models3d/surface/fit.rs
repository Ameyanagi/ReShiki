//! Smooth cubic outlines for merged surface regions and runs, after Schneider's
//! digitized-curve fitting (Graphics Gems, 1990).
//! Fits use drawing units. Smooth joins share a derivative on the cubic unit interval.
//! The topology guard samples each cubic in eight steps and may return the source polygon.

// Knot, span and edge indices stay within the cleaned polyline they were built from.
#![allow(clippy::indexing_slicing)]

use crate::document::Point;
use crate::graphics::PathCommand;
use std::ops::{Add, Mul, Sub};

#[cfg(test)]
thread_local! {
    static STROKE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static DIAGNOSTICS: std::cell::Cell<[(usize, usize); 2]> = const { std::cell::Cell::new([(0, 0); 2]) };
}

#[cfg(test)]
pub(super) fn take_diagnostics() -> [(usize, usize); 2] {
    DIAGNOSTICS.take()
}

#[cfg(test)]
fn record_fit(corners: usize, fallback: bool) {
    let mut counts = DIAGNOSTICS.get();
    let (total, fallbacks) = &mut counts[usize::from(STROKE.get())];
    *total += corners;
    *fallbacks += usize::from(fallback);
    DIAGNOSTICS.set(counts);
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct V {
    x: f64,
    y: f64,
}

impl Add for V {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

impl Sub for V {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

impl Mul<f64> for V {
    type Output = Self;
    fn mul(self, rhs: f64) -> Self {
        Self {
            x: self.x * rhs,
            y: self.y * rhs,
        }
    }
}

impl V {
    fn dot(self, rhs: Self) -> f64 {
        self.x * rhs.x + self.y * rhs.y
    }
    fn cross(self, rhs: Self) -> f64 {
        self.x * rhs.y - self.y * rhs.x
    }
    fn norm2(self) -> f64 {
        self.dot(self)
    }
    fn length(self) -> f64 {
        self.norm2().sqrt()
    }
    fn unit(self) -> Self {
        let length = self.length();
        if length == 0.0 {
            self
        } else {
            self * (1.0 / length)
        }
    }
    fn point(self) -> Point {
        let max = f64::from(f32::MAX);
        Point::new(
            self.x.clamp(-max, max) as f32,
            self.y.clamp(-max, max) as f32,
        )
    }
    fn rounded(self) -> Self {
        Self::from(self.point())
    }
}

impl From<Point> for V {
    fn from(p: Point) -> Self {
        Self {
            x: p.x.into(),
            y: p.y.into(),
        }
    }
}

fn clean(points: &[[f32; 2]]) -> Vec<V> {
    let mut result = Vec::with_capacity(points.len());
    for &[x, y] in points {
        if !x.is_finite() || !y.is_finite() {
            return Vec::new();
        }
        let p = V {
            x: x.into(),
            y: y.into(),
        };
        if result.last() != Some(&p) {
            result.push(p);
        }
    }
    result
}

fn strip_seam(p: &mut Vec<V>) {
    if p.len() > 1 && p.first() == p.last() {
        p.pop();
    }
}

/// Fits a closed contour. The closing seam is smooth unless it is a corner.
pub fn closed(points: &[[f32; 2]], error: f32) -> Vec<PathCommand> {
    let mut p = clean(points);
    strip_seam(&mut p);
    guarded(&p, error, true)
}

/// Fits a polyline with its two endpoints fixed.
pub fn open(points: &[[f32; 2]], error: f32) -> Vec<PathCommand> {
    guarded(&clean(points), error, false)
}

/// Fits each nonzero-winding contour with absolute area ≥ `min_area` and
/// width (twice its absolute area divided by its perimeter) ≥ `min_width`.
/// Outer and hole contours retain their respective input orientations.
pub fn region(
    contours: &[Vec<[f32; 2]>],
    error: f32,
    min_area: f32,
    min_width: f32,
) -> Vec<PathCommand> {
    #[cfg(test)]
    STROKE.set(false);
    let mut out = Vec::new();
    for contour in contours {
        let mut p = clean(contour);
        strip_seam(&mut p);
        if p.len() < 3 {
            continue;
        }
        let mut twice_area = 0.0;
        let mut perimeter = 0.0;
        let mut prev = p[p.len() - 1];
        for &v in &p {
            // A local origin avoids cancellation on small, translated polygons.
            twice_area += (prev - p[0]).cross(v - p[0]);
            perimeter += (v - prev).length();
            prev = v;
        }
        let magnitude = twice_area.abs();
        if magnitude >= 2.0 * f64::from(min_area) && magnitude >= f64::from(min_width) * perimeter {
            out.extend(guarded(&p, error, true));
        }
    }
    out
}

/// Fits runs meeting the length threshold, closing those with coincident endpoints.
pub fn runs(runs: &[Vec<[f32; 2]>], error: f32, min_length: f32) -> Vec<PathCommand> {
    #[cfg(test)]
    STROKE.set(true);
    let mut out = Vec::new();
    for run in runs {
        let mut p = clean(run);
        let length: f64 = p.windows(2).map(|w| (w[1] - w[0]).length()).sum();
        if p.is_empty() || length < f64::from(min_length) {
            continue;
        }
        let cyclic = p.len() > 1 && p.first() == p.last();
        if cyclic {
            strip_seam(&mut p);
        }
        out.extend(guarded(&p, error, cyclic));
    }
    out
}

fn polygon(p: &[V], cyclic: bool) -> Vec<PathCommand> {
    let Some(first) = p.first() else {
        return Vec::new();
    };
    let mut out = vec![PathCommand::Move(first.point())];
    out.extend(p[1..].iter().map(|v| PathCommand::Line(v.point())));
    if cyclic {
        out.push(PathCommand::Close);
    }
    out
}

fn guarded(p: &[V], error: f32, cyclic: bool) -> Vec<PathCommand> {
    if p.len() < 3 {
        return polygon(p, cyclic);
    }
    let error = f64::from(error.max(0.0));
    let simple = !intersects(p, cyclic);
    let mut k = prepare(p, error, cyclic);
    #[cfg(test)]
    record_fit(
        k[..p.len()].iter().filter(|knot| knot.corner).count(),
        false,
    );
    let path = fit_path(&k, error, cyclic);
    if !simple || !intersects(&flatten(&path, cyclic), cyclic) {
        return path;
    }
    let retry_error = error * 0.5;
    protect_close_edges(&mut k, retry_error, cyclic);
    let path = fit_path(&k, retry_error, cyclic);
    if !intersects(&flatten(&path, cyclic), cyclic) {
        return path;
    }
    #[cfg(test)]
    record_fit(0, true);
    polygon(p, cyclic)
}

#[derive(Clone, Copy)]
struct Knot {
    p: V,
    incoming: V,
    outgoing: V,
    corner: bool,
    max_handle: f64,
}

impl Knot {
    fn tangent(self) -> V {
        let bisector = (self.incoming + self.outgoing).unit();
        if bisector == V::default() {
            self.outgoing
        } else {
            bisector
        }
    }
}

fn knots(p: &[V], radius: f64, cyclic: bool) -> Vec<Knot> {
    let n = p.len();
    let count = if cyclic { 3 * n } else { n };
    // Three laps give every cyclic vertex a full search window on either side.
    let mut arc = Vec::with_capacity(count);
    arc.push(0.0);
    for i in 1..count {
        arc.push(arc[i - 1] + (p[i % n] - p[(i - 1) % n]).length());
    }
    let cos_limit = 40.0_f64.to_radians().cos();
    let (mut before, mut after) = (0, 0);
    (0..n)
        .map(|i| {
            let j = if cyclic { i + n } else { i };
            let lo = if cyclic { j + 1 - n } else { 0 };
            let hi = if cyclic { j + n - 1 } else { n - 1 };
            // Arc distances increase with j, so neither search ever moves backward.
            before = before.max(lo);
            while before < j && arc[before] <= arc[j] - radius {
                before += 1;
            }
            after = after.max(j + 1);
            while after <= hi && arc[after] < arc[j] + radius {
                after += 1;
            }
            let left = if before > lo {
                before - 1
            } else if cyclic {
                j - 1
            } else {
                lo
            };
            let right = if after <= hi {
                after
            } else if cyclic {
                j + 1
            } else {
                hi
            };
            let enter = (p[i] - p[j.saturating_sub(1) % n]).unit();
            let leave = (p[(j + 1).min(count - 1) % n] - p[i]).unit();
            let mut incoming = (p[i] - p[left % n]).unit();
            let mut outgoing = (p[right % n] - p[i]).unit();
            if incoming == V::default() {
                incoming = enter;
            }
            if outgoing == V::default() {
                outgoing = leave;
            }
            // Both flanks must support the wide turn at this vertex. Two-edge
            // chords cancel alternating noise when one edge spans the window.
            let enter2 = (p[i] - p[j.saturating_sub(2) % n]).unit();
            let leave2 = (p[(j + 2).min(count - 1) % n] - p[i]).unit();
            let turn_cos = incoming.dot(outgoing).max(enter.dot(leave));
            let corner = before > lo
                && after <= hi
                && turn_cos < cos_limit
                && (n < 5 || enter2.dot(leave2) < cos_limit)
                && enter.dot(incoming).min(leave.dot(outgoing)) > turn_cos.max(0.0)
                && enter.cross(leave) * incoming.cross(outgoing) >= 0.0;
            // At a narrow turn the chord window can reach the returning side.
            let tangent = incoming + outgoing;
            if tangent.dot(enter) <= 0.0 || tangent.dot(leave) <= 0.0 {
                incoming = enter;
                outgoing = leave;
            }
            Knot {
                max_handle: f64::INFINITY,
                p: p[i],
                incoming,
                outgoing,
                corner,
            }
        })
        .collect()
}

fn prepare(p: &[V], error: f64, cyclic: bool) -> Vec<Knot> {
    let mut k = knots(p, (3.0 * error).max(1.0), cyclic);
    if cyclic {
        let start = k.iter().position(|v| v.corner).unwrap_or_else(|| {
            let center =
                p.iter().copied().fold(V::default(), |a, b| a + b) * (1.0 / p.len() as f64);
            (0..p.len())
                .max_by(|&a, &b| (p[a] - center).norm2().total_cmp(&(p[b] - center).norm2()))
                .unwrap_or(0)
        });
        k.rotate_left(start);
        k.push(k[0]);
    }
    k
}

fn fit_path(k: &[Knot], error: f64, cyclic: bool) -> Vec<PathCommand> {
    let mut out = vec![PathCommand::Move(k[0].p.point())];
    let smooth_seam = cyclic && !k[0].corner;
    let mut start = 0;
    for i in 1..k.len() {
        if k[i].corner || i + 1 == k.len() {
            fit_chain(&k[start..=i], error, smooth_seam, &mut out);
            start = i;
        }
    }
    if cyclic {
        out.push(PathCommand::Close);
    }
    out
}

#[derive(Clone, Copy)]
struct Cubic([V; 4]);

impl Cubic {
    fn controls_outside_samples(self, k: &[Knot], error: f64) -> bool {
        let [_, b, c, _] = self.0;
        let outside = |coord: fn(V) -> f64| {
            let lo = coord(b).min(coord(c)) - error;
            let hi = coord(b).max(coord(c)) + error;
            k.iter().all(|v| coord(v.p) < lo) || k.iter().all(|v| coord(v.p) > hi)
        };
        k.len() > 2 && (outside(|v| v.x) || outside(|v| v.y))
    }

    fn valid_controls(self) -> bool {
        let [a, b, c, d] = self.0;
        let chord2 = (d - a).norm2();
        if (b - a).norm2() > chord2 || (d - c).norm2() > chord2 {
            return false;
        }
        let first = Edge { a, b };
        let last = Edge { a: c, b: d };
        !(first.xmin() <= last.xmax()
            && last.xmin() <= first.xmax()
            && first.ymin() <= last.ymax()
            && last.ymin() <= first.ymax()
            && first.crosses(last))
    }

    fn eval(self, t: f64) -> V {
        let [a, b, c, d] = self.0;
        let s = 1.0 - t;
        a * (s * s * s) + b * (3.0 * t * s * s) + c * (3.0 * t * t * s) + d * (t * t * t)
    }

    fn project(self, p: V, t: f64) -> f64 {
        let [a, b, c, d] = self.0;
        let s = 1.0 - t;
        let (ab, bc, cd) = (b - a, c - b, d - c);
        let velocity = (ab * (s * s) + bc * (2.0 * s * t) + cd * (t * t)) * 3.0;
        let accel = ((bc - ab) * s + (cd - bc) * t) * 6.0;
        let residual = self.eval(t) - p;
        let curvature = velocity.norm2() + residual.dot(accel);
        if curvature > 0.0 {
            (t - residual.dot(velocity) / curvature).clamp(0.0, 1.0)
        } else {
            t
        }
    }

    fn distance2(self, point: V, mut t: f64, tolerance2: f64) -> f64 {
        let mut distance = (self.eval(t) - point).norm2();
        // A changed handle changes speed. Improve its witness on the emitted curve.
        for _ in 0..4 {
            if distance <= tolerance2 {
                break;
            }
            let next = self.project(point, t);
            let trial = (self.eval(next) - point).norm2();
            if trial >= distance {
                break;
            }
            (t, distance) = (next, trial);
        }
        distance
    }

    fn command(self) -> PathCommand {
        let [_, b, c, d] = self.0;
        PathCommand::Cubic(b.point(), c.point(), d.point())
    }
}

fn chord_parameters(k: &[Knot]) -> Vec<f64> {
    let mut length = 0.0;
    let mut t = vec![0.0; k.len()];
    for (u, w) in t[1..].iter_mut().zip(k.windows(2)) {
        length += (w[1].p - w[0].p).length();
        *u = length;
    }
    for u in &mut t[1..] {
        *u /= length;
    }
    t
}

fn least_squares(k: &[Knot], t: &[f64], left: V, right: V) -> Cubic {
    let a = k[0].p;
    let d = k[k.len() - 1].p;
    let delta = d - a;
    // The two unknowns are the handle lengths along the fixed tangents.
    let (mut xx, mut xy, mut yy, mut xp, mut yp) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for (v, &u) in k.iter().zip(t) {
        let s = 1.0 - u;
        let b1 = 3.0 * u * s * s;
        let b2 = 3.0 * u * u * s;
        let residual = (v.p - a) - delta * (b2 + u * u * u);
        xx += b1 * b1;
        xy -= b1 * b2;
        yy += b2 * b2;
        xp += b1 * left.dot(residual);
        yp -= b2 * right.dot(residual);
    }
    xy *= left.dot(right);
    let det = xx * yy - xy * xy;
    let third = delta.length() / 3.0;
    let (mut l, mut r) = (third, third);
    if det > f64::EPSILON * xx * yy {
        let x = (yy * xp - xy * yp) / det;
        let y = (xx * yp - xy * xp) / det;
        if x > 0.0 && y > 0.0 {
            (l, r) = (x, y);
        }
    }
    Cubic([a, (a + left * l).rounded(), (d - right * r).rounded(), d])
}

// A witness on the rounded cubic certifies an upper bound on point-to-curve distance.
fn worst(k: &[Knot], t: &[f64], c: Cubic, tolerance2: f64) -> (f64, usize) {
    let mut worst = (0.0, k.len() / 2);
    for i in 1..k.len() - 1 {
        let d = c.distance2(k[i].p, t[i], tolerance2);
        if !d.is_finite() {
            return (f64::INFINITY, i);
        }
        if d > worst.0 {
            worst = (d, i);
        }
    }
    worst
}

struct Span {
    first: usize,
    last: usize,
    left: f64,
    right: f64,
    t: Vec<f64>,
}

impl Span {
    fn fit(k: &[Knot], first: usize, last: usize, seam: bool, tol2: f64) -> Self {
        let left = if first == 0 && !seam {
            k[first].outgoing
        } else {
            k[first].tangent()
        };
        let right = if last + 1 == k.len() && !seam {
            k[last].incoming
        } else {
            k[last].tangent()
        };
        let k = &k[first..=last];
        if k.len() == 2 {
            let chord2 = (k[1].p - k[0].p).norm2();
            // Disjoint half-chord disks keep even the rounded control edges apart.
            let handle = |knot: Knot, dir: V| {
                let mut len = (chord2.sqrt() / 3.0).min(knot.max_handle);
                while [-len, len]
                    .iter()
                    .any(|&h| ((knot.p + dir * h).rounded() - knot.p).norm2() >= chord2 * 0.25)
                {
                    len *= 0.5;
                }
                len
            };
            return Self {
                first,
                last,
                left: handle(k[0], left),
                right: handle(k[1], right),
                t: Vec::new(),
            };
        }
        let mut t = chord_parameters(k);
        let mut curve = least_squares(k, &t, left, right);
        let mut err = worst(k, &t, curve, f64::INFINITY).0;
        for _ in 0..4 {
            if err <= tol2 || err > 256.0 * tol2 {
                break;
            }
            let next: Vec<_> = t
                .iter()
                .zip(k)
                .map(|(&u, v)| curve.project(v.p, u))
                .collect();
            if next.windows(2).any(|w| w[0] >= w[1]) {
                break;
            }
            let candidate = least_squares(k, &next, left, right);
            let next_err = worst(k, &next, candidate, f64::INFINITY).0;
            if next_err >= err {
                break;
            }
            (curve, t, err) = (candidate, next, next_err);
        }
        let [a, b, c, d] = curve.0;
        Self {
            first,
            last,
            left: (b - a).length().min(k[0].max_handle),
            right: (d - c).length().min(k[k.len() - 1].max_handle),
            t,
        }
    }
}

fn straight(k: &[Knot], tol2: f64) -> bool {
    let chord = Edge {
        a: k[0].p,
        b: k[k.len() - 1].p,
    };
    chord.a != chord.b
        && k[1..k.len() - 1]
            .iter()
            .all(|v| chord.distance2(v.p) <= tol2)
}

fn fit_chain(k: &[Knot], error: f64, seam: bool, out: &mut Vec<PathCommand>) {
    let last = k.len() - 1;
    let tol2 = error * error;
    if !seam && k[1..last].iter().all(|v| v.max_handle.is_infinite()) && straight(k, tol2) {
        out.push(PathCommand::Line(k[last].p.point()));
        return;
    }
    let mut cuts: Vec<_> = (1..last).filter(|&i| k[i].max_handle.is_finite()).collect();
    if seam && cuts.is_empty() {
        // A closed cubic with one shared tangent is collinear. Start with two arcs.
        let mid = (1..last)
            .max_by(|&a, &b| {
                (k[a].p - k[0].p)
                    .norm2()
                    .total_cmp(&(k[b].p - k[0].p).norm2())
            })
            .unwrap_or(last / 2);
        cuts.push(mid);
    }
    let mut start = 0;
    let mut spans: Vec<_> = cuts
        .into_iter()
        .chain([last])
        .map(|end| {
            let span = Span::fit(k, start, end, seam, tol2);
            start = end;
            span
        })
        .collect();
    // Process one recursion level at a time to share derivatives between siblings.
    let mut depth = 0;
    loop {
        let n = spans.len();
        let mut cuts = Vec::with_capacity(n);
        let mut commands = Vec::with_capacity(n);
        for (i, span) in spans.iter().enumerate() {
            let sub = &k[span.first..=span.last];
            let (a, d) = (sub[0], sub[sub.len() - 1]);
            let left = if i > 0 || seam {
                a.tangent() * span.left.min(spans[(i + n - 1) % n].right)
            } else {
                a.outgoing * span.left
            };
            let right = if i + 1 < n || seam {
                d.tangent() * span.right.min(spans[(i + 1) % n].left)
            } else {
                d.incoming * span.right
            };
            let curve = Cubic([a.p, (a.p + left).rounded(), (d.p - right).rounded(), d.p]);
            let (err, split) = worst(sub, &span.t, curve, tol2);
            // A small point error can still hide a loop between samples.
            if err > tol2 || !curve.valid_controls() {
                cuts.push(Some(span.first + split));
            } else if curve.controls_outside_samples(sub, error) {
                // A long arch can put both controls beyond the drawing bounds.
                cuts.push(Some(span.first + sub.len() / 2));
            } else {
                commands.push(curve.command());
                cuts.push(None);
            }
        }
        if cuts.iter().all(Option::is_none) {
            out.extend(commands);
            return;
        }
        let mut refined = Vec::with_capacity(n + cuts.iter().flatten().count());
        for (span, cut) in spans.into_iter().zip(cuts) {
            if let Some(mid) = cut {
                if depth >= 24 {
                    // Bound recursion on unevenly spaced data while retaining shared tangents.
                    for i in span.first..span.last {
                        refined.push(Span::fit(k, i, i + 1, seam, tol2));
                    }
                } else {
                    refined.push(Span::fit(k, span.first, mid, seam, tol2));
                    refined.push(Span::fit(k, mid, span.last, seam, tol2));
                }
            } else {
                refined.push(span);
            }
        }
        spans = refined;
        depth += 1;
    }
}

fn flatten(path: &[PathCommand], cyclic: bool) -> Vec<V> {
    let mut points = Vec::new();
    for cmd in path {
        match *cmd {
            PathCommand::Move(p) | PathCommand::Line(p) => points.push(V::from(p)),
            PathCommand::Cubic(b, c, d) => {
                let start = points.last().copied().unwrap_or_default();
                let curve = Cubic([start, b.into(), c.into(), d.into()]);
                points.extend((1..=8).map(|i| curve.eval(f64::from(i) / 8.0)));
            }
            PathCommand::Close => {}
        }
    }
    points.dedup();
    if cyclic {
        strip_seam(&mut points);
    }
    points
}

#[derive(Clone, Copy)]
struct Edge {
    a: V,
    b: V,
}

impl Edge {
    fn distance2(self, point: V) -> f64 {
        let ab = self.b - self.a;
        let ap = point - self.a;
        let t = (ab.dot(ap) / ab.norm2()).clamp(0.0, 1.0);
        (ap - ab * t).norm2()
    }
    fn xmin(self) -> f64 {
        self.a.x.min(self.b.x)
    }
    fn xmax(self) -> f64 {
        self.a.x.max(self.b.x)
    }
    fn ymin(self) -> f64 {
        self.a.y.min(self.b.y)
    }
    fn ymax(self) -> f64 {
        self.a.y.max(self.b.y)
    }
    fn crosses(self, other: Self) -> bool {
        let ab = self.b - self.a;
        let cd = other.b - other.a;
        let straddles = |a: f64, b: f64| (a <= 0.0 && b >= 0.0) || (a >= 0.0 && b <= 0.0);
        // The sweep has already established overlap on both axes.
        straddles(ab.cross(other.a - self.a), ab.cross(other.b - self.a))
            && straddles(cd.cross(self.a - other.a), cd.cross(self.b - other.a))
    }
}

// Where two source edges can approach within the retry error on each side,
// interpolate their endpoints and keep their handles inside the gap.
fn protect_close_edges(k: &mut [Knot], error: f64, cyclic: bool) {
    let edges: Vec<_> = k
        .windows(2)
        .map(|w| Edge {
            a: w[0].p,
            b: w[1].p,
        })
        .collect();
    let n = edges.len();
    let mut order: Vec<_> = (0..n).collect();
    order.sort_unstable_by(|&i, &j| edges[i].xmin().total_cmp(&edges[j].xmin()).then(i.cmp(&j)));
    let reach = 2.0 * error;
    for (rank, &i) in order.iter().enumerate() {
        let a = edges[i];
        for &j in &order[rank + 1..] {
            let b = edges[j];
            if b.xmin() > a.xmax() + reach {
                break;
            }
            let gap = i.abs_diff(j);
            if gap == 1
                || (cyclic && gap + 1 == n)
                || b.ymin() > a.ymax() + reach
                || a.ymin() > b.ymax() + reach
            {
                continue;
            }
            let d2 = a
                .distance2(b.a)
                .min(a.distance2(b.b))
                .min(b.distance2(a.a))
                .min(b.distance2(a.b));
            if d2 <= reach * reach {
                let limit = d2.sqrt() / 3.0;
                for index in [i, i + 1, j, j + 1] {
                    k[index].max_handle = k[index].max_handle.min(limit);
                }
            }
        }
    }
    if cyclic {
        let limit = k[0].max_handle.min(k[n].max_handle);
        k[0].max_handle = limit;
        k[n].max_handle = limit;
    }
}

// The x sweep indexes active edges by their lower y. Each subtree caches the
// highest active y so a query visits only intervals that can overlap.
struct ActiveEdges {
    order: Vec<usize>,
    rank: Vec<usize>,
    high: Vec<f64>,
    base: usize,
}

impl ActiveEdges {
    fn new(edges: &[Edge]) -> Self {
        let mut order: Vec<_> = (0..edges.len()).collect();
        order
            .sort_unstable_by(|&i, &j| edges[i].ymin().total_cmp(&edges[j].ymin()).then(i.cmp(&j)));
        let mut rank = vec![0; order.len()];
        for (r, &i) in order.iter().enumerate() {
            rank[i] = r;
        }
        let base = edges.len().next_power_of_two();
        Self {
            order,
            rank,
            high: vec![f64::NEG_INFINITY; 2 * base],
            base,
        }
    }

    fn set(&mut self, edge: usize, high: f64) {
        let mut i = self.base + self.rank[edge];
        self.high[i] = high;
        while i > 1 {
            i /= 2;
            let high = self.high[2 * i].max(self.high[2 * i + 1]);
            if self.high[i] == high {
                break;
            }
            self.high[i] = high;
        }
    }

    fn hits(
        &self,
        node: usize,
        low: usize,
        size: usize,
        end: usize,
        ymin: f64,
        test: &impl Fn(usize) -> bool,
    ) -> bool {
        if low >= end || self.high[node] < ymin {
            return false;
        }
        if size == 1 {
            return test(self.order[low]);
        }
        let half = size / 2;
        self.hits(node * 2, low, half, end, ymin, test)
            || self.hits(node * 2 + 1, low + half, half, end, ymin, test)
    }
}

fn intersects(p: &[V], cyclic: bool) -> bool {
    if p.len() < 3 {
        return false;
    }
    let n = if cyclic { p.len() } else { p.len() - 1 };
    let edges: Vec<_> = (0..n)
        .map(|i| Edge {
            a: p[i],
            b: p[(i + 1) % p.len()],
        })
        .collect();
    for i in usize::from(!cyclic)..n {
        let prev = edges[(i + n - 1) % n];
        let ab = prev.b - prev.a;
        let bc = edges[i].b - edges[i].a;
        if ab.cross(bc) == 0.0 && ab.dot(bc) < 0.0 {
            return true;
        }
    }
    let mut entering: Vec<_> = (0..n).collect();
    let mut leaving = entering.clone();
    entering.sort_unstable_by(|&i, &j| edges[i].xmin().total_cmp(&edges[j].xmin()).then(i.cmp(&j)));
    leaving.sort_unstable_by(|&i, &j| edges[i].xmax().total_cmp(&edges[j].xmax()).then(i.cmp(&j)));
    let mut active = ActiveEdges::new(&edges);
    let mut expired = 0;
    for i in entering {
        let edge = edges[i];
        while expired < n && edges[leaving[expired]].xmax() < edge.xmin() {
            active.set(leaving[expired], f64::NEG_INFINITY);
            expired += 1;
        }
        let end = active
            .order
            .partition_point(|&j| edges[j].ymin() <= edge.ymax());
        if active.hits(1, 0, active.base, end, edge.ymin(), &|j| {
            let gap = i.abs_diff(j);
            gap != 1 && !(cyclic && gap == n - 1) && edge.crosses(edges[j])
        }) {
            return true;
        }
        active.set(i, edge.ymax());
    }
    false
}

#[cfg(test)]
mod tests;
