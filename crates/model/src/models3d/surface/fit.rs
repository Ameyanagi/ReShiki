//! Fits orbital contours using P. J. Schneider's digitized-curve algorithm (Graphics Gems, 1990).
//! Fits use drawing units. Smooth joins share a derivative on the cubic unit interval.
//! The topology guard samples each cubic in eight steps and may return the source polygon.

// Knot, span and edge indices stay within the cleaned polyline they were built from.
#![allow(clippy::indexing_slicing)]

use crate::document::Point;
use crate::graphics::PathCommand;
use std::ops::{Add, Mul, Sub};

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
    let k = prepare(p, error, cyclic);
    for tolerance in [error, error * 0.5] {
        let path = fit_path(&k, tolerance, cyclic);
        if !simple || !intersects(&flatten(&path, cyclic), cyclic) {
            return path;
        }
    }
    polygon(p, cyclic)
}

#[derive(Clone, Copy, Default)]
struct Knot {
    p: V,
    incoming: V,
    outgoing: V,
    corner: bool,
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
            let mut incoming = (p[i] - p[left % n]).unit();
            let mut outgoing = (p[right % n] - p[i]).unit();
            if incoming == V::default() {
                incoming = (p[i] - p[j.saturating_sub(1) % n]).unit();
            }
            if outgoing == V::default() {
                outgoing = (p[(j + 1).min(count - 1) % n] - p[i]).unit();
            }
            let corner = before > lo && after <= hi && incoming.dot(outgoing) < cos_limit;
            Knot {
                p: p[i],
                incoming,
                outgoing,
                corner,
            }
        })
        .collect()
}

fn prepare(p: &[V], error: f64, cyclic: bool) -> Vec<Knot> {
    let mut k = knots(p, 3.0 * error, cyclic);
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
    curve: Cubic,
    t: Vec<f64>,
    interpolate: bool,
}

impl Span {
    fn line(&self) -> bool {
        self.last == self.first + 1
    }

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
        Self {
            first,
            last,
            curve,
            interpolate: false,
            t,
        }
    }
}

fn straight(k: &[Knot], tol2: f64) -> bool {
    let a = k[0].p;
    let ab = k[k.len() - 1].p - a;
    let length2 = ab.norm2();
    length2 > 0.0
        && k[1..k.len() - 1].iter().all(|v| {
            let ap = v.p - a;
            let t = (ap.dot(ab) / length2).clamp(0.0, 1.0);
            (ap - ab * t).norm2() <= tol2
        })
}

fn common_handle(prev: &Span, next: &Span, knot: Knot, k: &[Knot]) -> V {
    let incoming = prev.curve.0[3] - prev.curve.0[2];
    let outgoing = next.curve.0[1] - next.curve.0[0];
    let left = (prev.curve.0[3] - prev.curve.0[0]) * (1.0 / 3.0);
    let right = (next.curve.0[3] - next.curve.0[0]) * (1.0 / 3.0);
    let handle = match (prev.line(), next.line()) {
        (true, _) => left,
        (false, true) => right,
        // A long neighbour must not pull a short span past its samples.
        (false, false) => knot.tangent() * incoming.length().min(outgoing.length()),
    };
    if !prev.interpolate && !next.interpolate {
        return handle;
    }
    // An interpolated span emits one cubic per source edge. Its shared
    // endpoint handle must fit that edge, rather than the span's whole chord.
    let original = handle.length();
    let mut length = original;
    if prev.interpolate {
        length = length.min((k[prev.last].p - k[prev.last - 1].p).length() / 3.0);
    }
    if next.interpolate {
        length = length.min((k[next.first + 1].p - k[next.first].p).length() / 3.0);
    }
    if length < original {
        handle * (length / original)
    } else {
        handle
    }
}

// A proposed pair of one-edge children can demand different derivatives at
// one smooth knot. Keep their multi-point parent and interpolate its samples.
fn conflicting_split(spans: &[Span], cuts: &[Option<usize>], i: usize, seam: bool) -> bool {
    let Some(mid) = cuts[i] else { return false };
    let span = &spans[i];
    if span.last == span.first + 2 {
        return true;
    }
    let n = spans.len();
    let prev = (i + n - 1) % n;
    let next = (i + 1) % n;
    let left_line = spans[prev].line() || cuts[prev] == Some(spans[prev].last - 1);
    let right_line = spans[next].line() || cuts[next] == Some(spans[next].first + 1);
    (mid == span.first + 1 && (seam || i > 0) && left_line)
        || (mid + 1 == span.last && (seam || i + 1 < n) && right_line)
}

fn interpolate(k: &[Knot], curve: Cubic, out: &mut Vec<PathCommand>) {
    let [_, mut control, end_control, _] = curve.0;
    for i in 1..k.len() {
        let p = k[i].p;
        let end = if i + 1 == k.len() {
            end_control
        } else {
            let shorter = (p - k[i - 1].p).length().min((k[i + 1].p - p).length());
            p - k[i].tangent() * (shorter / 3.0)
        };
        out.push(PathCommand::Cubic(control.point(), end.point(), p.point()));
        control = p + (p - end);
    }
}

fn fit_chain(k: &[Knot], error: f64, seam: bool, out: &mut Vec<PathCommand>) {
    let last = k.len() - 1;
    let tol2 = error * error;
    if !seam && straight(k, tol2) {
        out.push(PathCommand::Line(k[last].p.point()));
        return;
    }
    // With coincident endpoints and one shared tangent, a single cubic is collinear.
    // Split the zero chord at its largest error before fitting the two arcs.
    let mut spans = if seam {
        let mid = (1..last)
            .max_by(|&a, &b| {
                (k[a].p - k[0].p)
                    .norm2()
                    .total_cmp(&(k[b].p - k[0].p).norm2())
            })
            .unwrap_or(last / 2);
        vec![
            Span::fit(k, 0, mid, seam, tol2),
            Span::fit(k, mid, last, seam, tol2),
        ]
    } else {
        vec![Span::fit(k, 0, last, seam, tol2)]
    };
    // Process one recursion level at a time to share derivatives between siblings.
    for depth in usize::from(seam)..=24 {
        let n = spans.len();
        let mut cuts = Vec::with_capacity(n);
        let mut commands = Vec::with_capacity(n);
        for (i, span) in spans.iter().enumerate() {
            let [a, b, c, d] = span.curve.0;
            let l = (i > 0 || seam)
                .then(|| common_handle(&spans[(i + n - 1) % n], span, k[span.first], k));
            let r = (i + 1 < n || seam)
                .then(|| common_handle(span, &spans[(i + 1) % n], k[span.last], k));
            if span.line() {
                let third = (d - a) * (1.0 / 3.0);
                if l.is_none_or(|h| h == third) && r.is_none_or(|h| h == third) {
                    commands.push(PathCommand::Line(d.point()));
                } else {
                    // A shorter shared derivative still traces this exact line.
                    commands.push(
                        Cubic([
                            a,
                            (a + l.unwrap_or(third)).rounded(),
                            (d - r.unwrap_or(third)).rounded(),
                            d,
                        ])
                        .command(),
                    );
                }
                cuts.push(None);
                continue;
            }
            let curve = Cubic([
                a,
                l.map_or(b, |h| (a + h).rounded()),
                r.map_or(c, |h| (d - h).rounded()),
                d,
            ]);
            let sub = &k[span.first..=span.last];
            if span.interpolate {
                interpolate(sub, curve, &mut commands);
                cuts.push(None);
                continue;
            }
            let (err, split) = worst(sub, &span.t, curve, tol2);
            // A small point error can still hide a loop between samples.
            // Refine suspect control polygons at the same worst-point split.
            if err <= tol2 && curve.valid_controls() {
                commands.push(curve.command());
                cuts.push(None);
            } else if depth == 24 {
                commands.extend(sub[1..].iter().map(|v| PathCommand::Line(v.p.point())));
                cuts.push(None);
            } else {
                cuts.push(Some(span.first + split));
            }
        }
        if cuts.iter().all(Option::is_none) {
            out.extend(commands);
            return;
        }
        let mut refined = Vec::with_capacity(n + cuts.iter().flatten().count());
        let conflicts: Vec<_> = (0..n)
            .map(|i| conflicting_split(&spans, &cuts, i, seam))
            .collect();
        for ((mut span, cut), conflict) in spans.into_iter().zip(cuts).zip(conflicts) {
            if conflict {
                let a = k[span.first].p;
                let d = k[span.last].p;
                let l = (k[span.first + 1].p - a).length() / 3.0;
                let r = (k[span.last - 1].p - d).length() / 3.0;
                span.curve.0[1] = a + (span.curve.0[1] - a).unit() * l;
                span.curve.0[2] = d + (span.curve.0[2] - d).unit() * r;
                span.interpolate = true;
                refined.push(span);
                continue;
            }
            if let Some(mid) = cut {
                refined.push(Span::fit(k, span.first, mid, seam, tol2));
                refined.push(Span::fit(k, mid, span.last, seam, tol2));
            } else {
                refined.push(span);
            }
        }
        spans = refined;
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
